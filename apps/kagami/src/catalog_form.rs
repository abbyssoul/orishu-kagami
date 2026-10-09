//! Native read/instantiate adapter over the catalog and document authorities.
//! No catalog writes, competing registry, implicit solver choice or partial edit.
use crate::{
    document::{AuthoringGuard, Document},
    physics_form::dependencies,
    scientific_effect::{EffectError, ScientificEffects, ScientificPlugins},
};
use kagami_catalog::{CatalogAuthority, CatalogSet};
use orishu_plugin::authoring_lock::SelectionLock;
use std::{path::PathBuf, sync::Arc, thread::JoinHandle};
use uuid::Uuid;

mod form;
mod preparation;
pub use form::{Form, Input};

pub(crate) const LIMITS: kagami_catalog::Limits = kagami_catalog::Limits {
    max_files: 32,
    max_file_bytes: 256 * 1024,
    max_documents_per_file: 64,
    max_bindings: 4096,
    ..kagami_catalog::Limits::DEFAULT
};
/// Maximum template rows constructed by one native listing page.
pub const PAGE: usize = 24;

/// Read/instance intents. Only a guarded completed Apply can edit the document.
/// Cancel/Close discard the proposal, not an already requested catalog reload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Close,
    Cancel,
    PickDirectory,
    Load(PathBuf),
    Reload,
    Poll,
    Page { catalog: Uuid, offset: usize },
    Select { catalog: Uuid, index: usize },
    Edit { form: Uuid, input: Input },
    Confirm { proposal: Uuid, confirmed: bool },
    Apply { proposal: Uuid },
    Dependencies(dependencies::Action),
}

struct Loading {
    work: JoinHandle<Result<CatalogAuthority, String>>,
}
struct Preparing {
    generation: Uuid,
    guard: AuthoringGuard,
    revision: u64,
    work: JoinHandle<Result<preparation::Ready, EffectError>>,
}
pub struct Controller {
    pub open: bool,
    pub notice: String,
    pub form: Form,
    pub dependencies: dependencies::Controller,
    pub offset: usize,
    authority: Option<CatalogAuthority>,
    snapshot: Option<Arc<CatalogSet>>,
    catalog_token: Uuid,
    plugins: Option<ScientificPlugins>,
    loading: Option<Loading>,
    preparing: Option<Preparing>,
    running: Option<Uuid>,
}
impl Controller {
    /// Reuse the installation selected at startup; performs no IO.
    pub fn new(plugins: Option<ScientificPlugins>) -> Self {
        Self {
            dependencies: dependencies::Controller::new(plugins.clone()),
            plugins,
            open: false,
            notice: String::new(),
            form: Default::default(),
            offset: 0,
            authority: None,
            snapshot: None,
            catalog_token: Uuid::new_v4(),
            loading: None,
            preparing: None,
            running: None,
        }
    }
    /// The catalog authority's immutable read projection, not another registry.
    pub fn snapshot(&self) -> Option<&CatalogSet> {
        self.snapshot.as_deref()
    }
    /// Listing incarnation; old row/page indices cannot refer to newer content.
    pub fn catalog_token(&self) -> Uuid {
        self.catalog_token
    }
    /// Actual owned IO/preparation lifetime, including closed/cancelled panels.
    pub fn is_pending(&self) -> bool {
        self.loading.is_some()
            || self.preparing.is_some()
            || self.running.is_some()
            || self.dependencies.is_pending()
    }
    pub(crate) fn refresh_plugins(&mut self, plugins: ScientificPlugins) {
        self.dependencies.refresh_plugins(plugins.clone());
        self.plugins = Some(plugins);
        self.form = Default::default();
        self.notice =
            "Inventory changed. Select the template again and explicitly recheck providers.".into();
    }
    pub(crate) fn act(
        &mut self,
        action: Action,
        document: &mut Document,
        scientific: &mut ScientificEffects,
        revision: Option<u64>,
    ) {
        let current_apply =
            matches!(&action, Action::Apply { proposal } if *proposal == self.form.token());
        let result = self.act_inner(action, document, scientific, revision);
        if let Err(error) = result {
            if current_apply && self.preparing.is_none() && self.running.is_none() {
                self.form.consent = None;
            }
            self.notice = error;
        }
    }
    fn act_inner(
        &mut self,
        action: Action,
        document: &mut Document,
        scientific: &mut ScientificEffects,
        revision: Option<u64>,
    ) -> Result<(), String> {
        match action {
            Action::Open => {
                self.open = true;
                return Ok(());
            }
            Action::Cancel | Action::Close => {
                if self.running.is_some() {
                    scientific.cancel();
                }
                self.form = Default::default();
                if action == Action::Close {
                    self.open = false;
                }
                self.notice =
                    "Template proposal cancelled; pending reads retain capacity until completion."
                        .into();
                return Ok(());
            }
            Action::Poll | Action::PickDirectory => return Ok(()),
            Action::Confirm {
                proposal,
                confirmed,
            } => {
                if proposal != self.form.token()
                    || !self.form.current(document)
                    || !self.form.captured
                {
                    return Err(
                        "History consent is stale; use the current template proposal.".into(),
                    );
                }
                if confirmed && (self.preparing.is_some() || self.running.is_some()) {
                    return Err(
                        "Wait for the previous preparation to finish before confirming again."
                            .into(),
                    );
                }
                self.form.consent = confirmed.then_some(proposal);
                if !confirmed && self.running.is_some() {
                    scientific.cancel();
                }
                return Ok(());
            }
            _ => {}
        }
        if !document.is_authoring() {
            return Err("Return to Authoring to create objects from templates.".into());
        }
        match action {
            Action::Edit { form, input } => {
                self.form.edit(document, form, input)?;
                self.notice =
                    "Instance inputs changed locally; check dependencies before Apply.".into();
            }
            Action::Select { catalog, index } => {
                if self.is_pending() || scientific.is_pending() {
                    return Err("Wait for pending preparation to finish.".into());
                }
                if catalog != self.catalog_token {
                    return Err("Catalog selection is stale; select from the current list.".into());
                }
                self.form = Form::select(
                    document,
                    self.snapshot.clone().ok_or("Load a catalog first.")?,
                    index,
                )?;
                self.notice = "Template selected without edits. Configure instance inputs and check dependencies; Apply independently revalidates availability.".into();
            }
            Action::Page { catalog, offset } => {
                let len = self.snapshot.as_ref().map_or(0, |s| s.entries().len());
                if catalog != self.catalog_token
                    || offset >= len
                    || !(offset == self.offset.saturating_sub(PAGE)
                        || offset == self.offset.saturating_add(PAGE))
                {
                    return Err("Catalog page is stale or unavailable.".into());
                }
                self.offset = offset;
            }
            Action::Load(path) => {
                if self.is_pending() || scientific.is_pending() {
                    return Err("A catalog operation still owns its slot.".into());
                }
                if path.as_os_str().as_encoded_bytes().len() > 4096 {
                    return Err("Catalog directory path exceeds its bound.".into());
                }
                let schemas = document.schemas().clone();
                let work = std::thread::Builder::new()
                    .name("kagami-catalog-load".into())
                    .spawn(move || {
                        if !path.is_dir() {
                            return Err("Select a readable catalog directory.".into());
                        }
                        Ok(CatalogAuthority::open(path, schemas, LIMITS))
                    })
                    .map_err(|_| "Cannot start catalog load.")?;
                self.loading = Some(Loading { work });
                self.form = Default::default();
                self.notice =
                    "Loading catalog through its authority; no experiment changes.".into();
            }
            Action::Reload => {
                if self.is_pending() || scientific.is_pending() {
                    return Err("A catalog operation still owns its slot.".into());
                }
                let mut authority = self
                    .authority
                    .take()
                    .ok_or("Load a catalog directory first.")?;
                let schemas = document.schemas().clone();
                let work = std::thread::Builder::new()
                    .name("kagami-catalog-reload".into())
                    .spawn(move || {
                        use kagami_catalog::authority::{
                            ActorId, CatalogCommand, CatalogCommandEnvelope, CommandId,
                        };
                        let envelope = CatalogCommandEnvelope::new(
                            CommandId::new(Uuid::new_v4().to_string()).expect("UUID"),
                            ActorId::new("kagami-window").expect("constant"),
                            CatalogCommand::ReloadWithSchemas { schemas },
                        )
                        .guarded_by(authority.revision());
                        authority.submit(envelope).map_err(|e| e.to_string())?;
                        Ok(authority)
                    })
                    .map_err(|_| "Cannot start catalog reload.")?;
                self.loading = Some(Loading { work });
                self.form = Default::default();
                self.notice =
                    "Reloading catalog; previously instantiated objects are unchanged.".into();
            }
            Action::Dependencies(action) => {
                if self.preparing.is_some() || self.running.is_some() || scientific.is_pending() {
                    return Err("Finish or cancel the current object preparation first.".into());
                }
                self.dependencies
                    .act(action, document, &mut self.form, &[], revision);
            }
            Action::Apply { proposal } => {
                if self.is_pending() || scientific.is_pending() {
                    return Err("A preparation still owns the background slot.".into());
                }
                if proposal != self.form.token() || !self.form.current(document) {
                    return Err("Template proposal is stale; select it again.".into());
                }
                if self.form.captured && !self.form.confirmed() {
                    return Err(
                        "Confirm history regeneration for this exact template proposal.".into(),
                    );
                }
                let selection = self
                    .dependencies
                    .resolved_selection(document, &self.form, revision)?
                    .clone();
                let lock = Arc::new(
                    SelectionLock::new(selection, document.limits().dependencies)
                        .map_err(|e| e.to_string())?,
                );
                let spec = self.form.request()?;
                kagami_session::instantiation::check_request(&spec, document.limits())
                    .map_err(|e| e.to_string())?;
                let plugins = self
                    .plugins
                    .clone()
                    .ok_or("No plugin inventory is configured.")?;
                let guard = self.form.guard.expect("current");
                let generation = self.form.token();
                let source = self.form.source.clone().expect("selected");
                let experiment = document.experiment().clone();
                let schemas = document.schemas().clone();
                let limits = *document.limits();
                let revision = revision.ok_or("No inventory revision is available.")?;
                let work = std::thread::Builder::new()
                    .name("kagami-template-prepare".into())
                    .spawn(move || {
                        preparation::prepare(
                            plugins, source, experiment, schemas, spec, lock, limits, revision,
                        )
                    })
                    .map_err(|_| "Cannot start template preparation.")?;
                self.preparing = Some(Preparing {
                    generation,
                    guard,
                    revision,
                    work,
                });
                self.notice = "Revalidating the exact template and provider choices; nothing has been accepted.".into();
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn before_scientific(
        &mut self,
        document: &Document,
        scientific: &mut ScientificEffects,
    ) {
        if self.running.is_some_and(|g| {
            g != self.form.token() || !self.form.confirmed() || !self.form.current(document)
        }) {
            scientific.cancel();
        }
    }
    pub(crate) fn after_scientific(
        &mut self,
        document: &Document,
        scientific: &ScientificEffects,
        accepted: bool,
    ) {
        if self.running.is_some() && !scientific.is_pending() {
            self.running = None;
            if accepted {
                self.form = Default::default();
                self.notice = "Template object, copied definitions, choices and history accepted together; fields unchanged.".into();
            } else {
                self.form.consent = None;
                self.notice = document.notice.clone().unwrap_or_else(|| {
                    "Template preparation refused; check and confirm again.".into()
                });
            }
        }
    }
    pub(crate) fn poll(
        &mut self,
        document: &mut Document,
        scientific: &mut ScientificEffects,
        revision: Option<u64>,
    ) {
        if self.loading.as_ref().is_some_and(|p| p.work.is_finished()) {
            let loaded = self
                .loading
                .take()
                .expect("finished")
                .work
                .join()
                .unwrap_or_else(|_| Err("Catalog loader failed.".into()));
            match loaded {
                Ok(authority) => {
                    let snapshot = authority.snapshot();
                    self.notice = format!(
                        "Catalog loaded: {} entries, {} file diagnostics. Availability is revalidated on Apply.",
                        snapshot.entries().len(),
                        snapshot.file_errors().len()
                    );
                    document.adopt_catalog_snapshot(snapshot.clone());
                    self.snapshot = Some(snapshot);
                    self.authority = Some(authority);
                    self.catalog_token = Uuid::new_v4();
                    self.offset = 0;
                }
                Err(error) => self.notice = error,
            }
        }
        self.dependencies.poll(document, &mut self.form, revision);
        if !self
            .preparing
            .as_ref()
            .is_some_and(|p| p.work.is_finished())
        {
            return;
        }
        let p = self.preparing.take().expect("finished");
        let result = p.work.join().unwrap_or(Err(EffectError::Executor));
        if !document.accepts_effect(p.guard)
            || self.form.token() != p.generation
            || revision != Some(p.revision)
            || (self.form.captured && !self.form.confirmed())
        {
            self.form.consent = None;
            self.notice =
                "Stale or cancelled template preparation discarded; no edits accepted.".into();
            return;
        }
        let ready = match result {
            Ok(r) => r,
            Err(e) => {
                self.form.consent = None;
                self.notice = e.to_string();
                return;
            }
        };
        if let Some(commands) = ready.commands {
            if document.edit_guarded_with_schemas(p.guard, commands, ready.schemas) {
                self.form = Default::default();
                self.notice = "Template instantiated as one self-contained undoable edit.".into();
            } else {
                self.notice = document
                    .notice
                    .clone()
                    .unwrap_or_else(|| "Template adoption refused.".into());
            }
        } else {
            match scientific.instantiate_template_from(
                document,
                p.guard,
                ready.catalog,
                ready.spec,
                ready.prepared.lock().clone(),
            ) {
                Ok(()) => {
                    self.running = Some(p.generation);
                    self.notice = "Preparing initial integrator history; fields and existing provider choices are retained.".into();
                }
                Err(e) => {
                    self.form.consent = None;
                    self.notice = e.to_string();
                }
            }
        }
    }
}
