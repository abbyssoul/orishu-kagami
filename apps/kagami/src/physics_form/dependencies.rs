//! One cold, bounded dependency-discovery lane. Pages and local choices never
//! initialize kernels or mutate a document; Apply revalidates the exact pins.
use super::PhysicsForm;
use crate::{
    document::{AuthoringGuard, Document},
    plugins::{InventoryRevisionGuard, KernelChoice},
    scientific_effect::ScientificPlugins,
};
use orishu_plugin::resolution::{
    CandidatePageResponse, ProviderBinding, RequirementKey, ResolutionOutcome, ResolutionRequest,
    UnavailableReason,
};
use std::{collections::BTreeMap, thread::JoinHandle};
use uuid::Uuid;

/// Shared discovery UI seam for whole-physics and component-only proposals.
/// Implementations own only transient choices; the document owns accepted intent.
pub trait Proposal {
    /// Changes whenever local proposal inputs change.
    fn generation(&self) -> Uuid;
    /// Whether choices are immutable in this proposal.
    fn is_captured(&self) -> bool;
    /// Copy an explicitly displayed candidate into local proposal state.
    fn bind(&mut self, binding: ProviderBinding) -> Result<(), &'static str>;
    /// Discard transient changes without erasing accepted pins.
    fn clear_bindings(&mut self);
    /// Explicitly discard a binding the current resolver report proves unused.
    fn forget_binding(&mut self, requirement: &RequirementKey) -> Result<(), &'static str>;
    /// Build the bounded request for this current document context.
    fn selection_with_scene(
        &self,
        choices: &[KernelChoice],
        revision: u64,
        document: &Document,
    ) -> Result<ResolutionRequest, &'static str>;
    /// Only component proposals can persist a standalone lock through this lane.
    fn permits_lock_adoption(&self) -> bool {
        false
    }
    /// Captured additions are handed to scientific preparation, never directly adopted.
    fn requires_scientific_extension(&self) -> bool {
        false
    }
    /// A captured replacement stages intent only, awaiting full-reset consent.
    fn requires_scientific_reset(&self) -> bool {
        false
    }
    /// Copy verified property fields into a local attachment proposal only.
    fn prepare_properties(
        &mut self,
        _document: &Document,
        _prepared: &crate::plugins::PreparedAuthoringLock,
    ) -> Result<(), &'static str> {
        Err("This proposal does not have component property fields.")
    }
    /// Produce the exact command batch for a revalidated component proposal.
    /// Physics proposals never adopt through this component-only path.
    fn lock_commands(
        &self,
        _document: &Document,
        _prepared: &crate::plugins::PreparedAuthoringLock,
    ) -> Result<Vec<kagami_document::ExperimentCommand>, &'static str> {
        Err("This proposal cannot adopt component choices.")
    }
}
impl Proposal for PhysicsForm {
    fn generation(&self) -> Uuid {
        self.generation()
    }
    fn is_captured(&self) -> bool {
        self.is_captured()
    }
    fn bind(&mut self, binding: ProviderBinding) -> Result<(), &'static str> {
        self.bind(binding)
    }
    fn clear_bindings(&mut self) {
        self.clear_bindings();
    }
    fn forget_binding(&mut self, requirement: &RequirementKey) -> Result<(), &'static str> {
        self.forget_binding(requirement)
    }
    fn selection_with_scene(
        &self,
        choices: &[KernelChoice],
        revision: u64,
        document: &Document,
    ) -> Result<ResolutionRequest, &'static str> {
        self.selection_with_scene(choices, revision, document.snapshot())
    }
}

/// An explicit user action against the displayed immutable report/page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Resolve the current selected models and scene component roots, without JIT.
    Check,
    /// Revalidate the displayed resolved graph and save component-only intent.
    /// Never accepted for a whole-physics proposal.
    ApplyLock { report: Uuid },
    /// Read selected component property fields without adopting capabilities.
    LoadProperties { report: Uuid },
    /// Remove only an unused local binding identified by the current report.
    ForgetUnused {
        report: Uuid,
        requirement: RequirementKey,
    },
    /// Read a bounded candidate page for this exact diagnostic.
    Page {
        report: Uuid,
        issue: usize,
        offset: usize,
    },
    /// Copy one displayed exact candidate into local proposal bindings.
    Choose {
        report: Uuid,
        issue: usize,
        candidate: usize,
    },
    /// Explicitly browse enabled compatible installed providers, including non-defaults.
    Browse {
        report: Uuid,
        requirement: RequirementKey,
    },
    /// Move within the explicit installed-provider page, not the automatic list.
    BrowsePage { report: Uuid, offset: usize },
    /// Bind one provider on the current explicit installed-provider page.
    ChooseInstalled { report: Uuid, candidate: usize },
    /// Explicitly discard local bindings, not captured or accepted scientific pins.
    Clear,
    /// Nonblocking completion notification.
    Poll,
}

/// Client-local report; no installation, execution or persisted identity authority.
pub struct Report {
    token: Uuid,
    guard: AuthoringGuard,
    generation: Uuid,
    request: ResolutionRequest,
    outcome: ResolutionOutcome,
    offsets: BTreeMap<usize, usize>,
    providers: BTreeMap<orishu_plugin::PluginReleaseId, orishu_plugin::PluginId>,
    alternatives: Option<InstalledPage>,
}
/// A bounded explicit-choice page. Presence never changes automatic eligibility.
pub struct InstalledPage {
    /// External scientific dependency being explicitly bound.
    pub requirement: RequirementKey,
    /// Current page only, in deterministic exact-reference order.
    pub candidates: Vec<orishu_plugin::ContributionRef>,
    /// First candidate index represented by this page.
    pub offset: usize,
    /// Complete eligible installed-provider count, not page length.
    pub total: usize,
}
impl Report {
    /// Optional explicit installed-provider page, separate from automatic choices.
    pub fn alternatives(&self) -> Option<&InstalledPage> {
        self.alternatives.as_ref()
    }
    /// Distinguishes pages even when their candidate indices coincide.
    pub fn token(&self) -> Uuid {
        self.token
    }
    /// Complete bounded resolver outcome, including explicit truncation flags.
    pub fn outcome(&self) -> &ResolutionOutcome {
        &self.outcome
    }
    /// First candidate index represented by a diagnostic's current page.
    pub fn offset(&self, issue: usize) -> usize {
        self.offsets.get(&issue).copied().unwrap_or(0)
    }
    /// Human-readable logical owner from the same inventory revision; the exact
    /// release/contribution reference remains the actual selection identity.
    pub fn provider_plugin(&self, provider: &orishu_plugin::ContributionRef) -> Option<&str> {
        self.providers.get(&provider.release).map(|id| id.as_str())
    }
    fn current(&self, document: &Document, form: &impl Proposal, revision: Option<u64>) -> bool {
        document.accepts_effect(self.guard)
            && form.generation() == self.generation
            && revision == Some(self.request.expected_inventory_revision)
    }
}
enum Query {
    Resolve,
    Lock {
        selection: orishu_plugin::resolution::Selection,
        preview: bool,
    },
    Installed {
        token: Uuid,
        requirement: RequirementKey,
        offset: usize,
    },
    Page {
        token: Uuid,
        issue: usize,
        offset: usize,
        requirement: orishu_plugin::resolution::RequirementKey,
    },
}
enum Answer {
    Resolved(ResolutionOutcome),
    Lock {
        outcome: crate::plugins::PrepareAuthoringLockOutcome,
        preview: bool,
    },
    Installed {
        token: Uuid,
        requirement: RequirementKey,
        offset: usize,
        page: CandidatePageResponse,
    },
    Page {
        token: Uuid,
        issue: usize,
        offset: usize,
        page: CandidatePageResponse,
    },
}
struct Completed {
    answer: Answer,
    providers: BTreeMap<orishu_plugin::PluginReleaseId, orishu_plugin::PluginId>,
    // Prevent inventory mutation between final verification and UI adoption.
    _revision: InventoryRevisionGuard,
}
struct Pending {
    guard: AuthoringGuard,
    generation: Uuid,
    request: ResolutionRequest,
    work: JoinHandle<Result<Completed, String>>,
}

/// One bounded handoff, not document acceptance or authorization to skip the
/// scientific lane's independent revalidation. Consumed in the same update fold.
pub(crate) struct ComponentIntent {
    pub reset: bool,
    pub guard: AuthoringGuard,
    pub generation: Uuid,
    pub revision: u64,
    pub attachments: Vec<kagami_document::ExperimentCommand>,
    pub lock: std::sync::Arc<orishu_plugin::authoring_lock::SelectionLock>,
}

/// One bounded discovery/adoption lane, separate from scientific initialization.
/// Pending reads retain their slot until actual exit, including after edits or
/// refresh invalidate them. No polling retry or filesystem watcher is hidden here.
pub struct Controller {
    plugins: Option<ScientificPlugins>,
    pending: Option<Pending>,
    report: Option<Report>,
    scientific_intent: Option<ComponentIntent>,
    /// Status independent of scientific-command acceptance.
    pub notice: String,
}
impl Controller {
    /// Borrow the exact displayed complete graph for a separate guarded adapter.
    pub(crate) fn resolved_selection(
        &self,
        document: &Document,
        form: &impl Proposal,
        revision: Option<u64>,
    ) -> Result<&orishu_plugin::resolution::Selection, &'static str> {
        let report = self
            .report
            .as_ref()
            .filter(|r| r.current(document, form, revision))
            .ok_or("Check the current dependency proposal before applying.")?;
        match &report.outcome {
            ResolutionOutcome::Resolved { selection, .. } => Ok(selection),
            _ => Err("Resolve all dependencies before applying."),
        }
    }
    /// Freeze the exact displayed resolved proposal for a separately guarded
    /// reset. No re-resolution/default choice is implicit in confirmation.
    pub(crate) fn resolved_request(
        &self,
        document: &Document,
        form: &impl Proposal,
        revision: Option<u64>,
    ) -> Result<ResolutionRequest, &'static str> {
        let report = self
            .report
            .as_ref()
            .filter(|r| r.current(document, form, revision))
            .ok_or("Check the current physics dependencies before confirming reset.")?;
        let ResolutionOutcome::Resolved { selection, .. } = &report.outcome else {
            return Err("Resolve all physics dependencies before confirming reset.");
        };
        Ok(ResolutionRequest {
            expected_inventory_revision: report.request.expected_inventory_revision,
            roots: selection.roots.clone(),
            bindings: selection.bindings.clone(),
        })
    }
    /// Reuse startup's inventory authority and process-only preferences.
    pub fn new(plugins: Option<ScientificPlugins>) -> Self {
        Self {
            plugins,
            pending: None,
            report: None,
            scientific_intent: None,
            notice: String::new(),
        }
    }
    /// Actual off-thread reader lifetime, not whether the panel is visible.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Last current, complete bounded response; an unavailable result is not success.
    pub fn report(&self) -> Option<&Report> {
        self.report.as_ref()
    }
    pub(crate) fn take_scientific_intent(&mut self) -> Option<ComponentIntent> {
        self.scientific_intent.take()
    }
    pub(crate) fn refresh_plugins(&mut self, plugins: ScientificPlugins) {
        self.plugins = Some(plugins);
        self.report = None;
        self.scientific_intent = None;
        self.notice = "Inventory changed; check dependencies again explicitly.".into();
    }
    /// Dispatch explicit local discovery/choice. Invalid or stale UI indices cannot
    /// fabricate a binding; every chosen provider must appear in the current page.
    pub fn act(
        &mut self,
        action: Action,
        document: &Document,
        form: &mut impl Proposal,
        choices: &[KernelChoice],
        revision: Option<u64>,
    ) {
        if action == Action::Poll {
            return;
        }
        if self.is_pending() {
            self.notice = "A dependency read still owns the background slot.".into();
            return;
        }
        let Some(guard) = document.authoring_guard() else {
            self.notice = "Dependency choices require Authoring mode.".into();
            return;
        };
        if action == Action::Clear {
            if !form.is_captured() {
                form.clear_bindings();
                self.report = None;
                self.notice = "Local provider choices cleared; accepted pins are unchanged.".into();
            }
            return;
        }
        let result = (|| {
            let plugins = self
                .plugins
                .clone()
                .ok_or("No plugin inventory is configured.")?;
            let (request, query) = match action {
                Action::ForgetUnused {
                    report: token,
                    ref requirement,
                } => {
                    let report = self
                        .report
                        .as_ref()
                        .filter(|r| r.token == token && r.current(document, form, revision))
                        .ok_or("Dependency report is stale; check again.")?;
                    let unused = matches!(&report.outcome, ResolutionOutcome::Unavailable {issues, ..} if issues.iter().any(|i| i.reason == UnavailableReason::UnusedBinding && i.requirement.as_ref() == Some(requirement)));
                    if !unused {
                        return Err("Only a reported unused binding can be discarded here.");
                    }
                    form.forget_binding(requirement)?;
                    self.report = None;
                    self.notice = "Unused binding removed from the local proposal only; recheck before Apply.".into();
                    return Ok(());
                }
                Action::Check => (
                    form.selection_with_scene(
                        choices,
                        revision.ok_or("No inventory revision.")?,
                        document,
                    )?,
                    Query::Resolve,
                ),
                Action::ApplyLock { report: token } | Action::LoadProperties { report: token } => {
                    if !form.permits_lock_adoption() {
                        return Err("This proposal cannot save standalone component choices.");
                    }
                    let report = self
                        .report
                        .as_ref()
                        .filter(|r| r.token == token && r.current(document, form, revision))
                        .ok_or("Resolved choices are stale; check again.")?;
                    let ResolutionOutcome::Resolved { selection, .. } = &report.outcome else {
                        return Err("Resolve every dependency before saving choices.");
                    };
                    // Also check the proposal's document guard, not just the
                    // report's guard, before scheduling a persistence effect.
                    form.selection_with_scene(
                        choices,
                        report.request.expected_inventory_revision,
                        document,
                    )?;
                    (
                        report.request.clone(),
                        Query::Lock {
                            selection: selection.clone(),
                            preview: matches!(action, Action::LoadProperties { .. }),
                        },
                    )
                }
                Action::Browse {
                    report: token,
                    ref requirement,
                } => {
                    let report = self
                        .report
                        .as_ref()
                        .filter(|r| r.token == token && r.current(document, form, revision))
                        .ok_or("Dependency choices are stale; check again.")?;
                    let known = match &report.outcome {
                        ResolutionOutcome::Resolved { selection, .. } => selection
                            .bindings
                            .iter()
                            .any(|b| &b.requirement == requirement),
                        ResolutionOutcome::Unavailable { issues, .. } => issues
                            .iter()
                            .any(|i| i.requirement.as_ref() == Some(requirement)),
                        _ => false,
                    };
                    if !known {
                        return Err("Requirement is not in this report.");
                    }
                    (
                        report.request.clone(),
                        Query::Installed {
                            token,
                            requirement: requirement.clone(),
                            offset: 0,
                        },
                    )
                }
                Action::BrowsePage {
                    report: token,
                    offset,
                }
                | Action::ChooseInstalled {
                    report: token,
                    candidate: offset,
                } => {
                    let report = self
                        .report
                        .as_ref()
                        .filter(|r| r.token == token && r.current(document, form, revision))
                        .ok_or("Dependency choices are stale; check again.")?;
                    let page = report
                        .alternatives
                        .as_ref()
                        .ok_or("No installed-provider page is open.")?;
                    if matches!(action, Action::ChooseInstalled { .. }) {
                        let provider = page
                            .candidates
                            .get(offset)
                            .ok_or("Candidate is not on this page.")?
                            .clone();
                        form.bind(ProviderBinding {
                            requirement: page.requirement.clone(),
                            provider,
                        })?;
                        self.report = None;
                        self.notice = "Exact installed provider copied to the proposal; defaults and accepted pins are unchanged. Recheck before Apply.".into();
                        return Ok(());
                    }
                    let previous = page.offset.saturating_sub(
                        orishu_plugin::resolution::ResolutionLimits::default().max_candidates,
                    );
                    let next = page.offset + page.candidates.len();
                    if !((page.offset > 0 && offset == previous)
                        || (next < page.total && offset == next))
                    {
                        return Err("Installed-provider page offset is not available.");
                    }
                    (
                        report.request.clone(),
                        Query::Installed {
                            token,
                            requirement: page.requirement.clone(),
                            offset,
                        },
                    )
                }
                Action::Page {
                    report: token,
                    issue,
                    offset,
                }
                | Action::Choose {
                    report: token,
                    issue,
                    candidate: offset,
                } => {
                    let report = self
                        .report
                        .as_ref()
                        .filter(|r| r.token == token && r.current(document, form, revision))
                        .ok_or("Dependency choices are stale; check again.")?;
                    let ResolutionOutcome::Unavailable { issues, .. } = &report.outcome else {
                        return Err("This report has no unresolved dependency choices.");
                    };
                    let diagnostic = issues
                        .get(issue)
                        .filter(|i| i.reason == UnavailableReason::AmbiguousProvider)
                        .ok_or("This diagnostic has no ambiguous provider choice.")?;
                    let requirement = diagnostic
                        .requirement
                        .clone()
                        .ok_or("Missing requirement identity.")?;
                    if matches!(action, Action::Choose { .. }) {
                        let provider = diagnostic
                            .candidates
                            .get(offset)
                            .ok_or("Candidate is not on this page.")?
                            .clone();
                        form.bind(ProviderBinding {
                            requirement,
                            provider,
                        })?;
                        self.report = None;
                        self.notice = "Exact provider copied to the proposal. Check dependencies again, then confirm Apply; no state has changed.".into();
                        return Ok(());
                    }
                    // Only adjacent advertised pages are valid UI intents.
                    let current = report.offset(issue);
                    let previous = current.saturating_sub(
                        orishu_plugin::resolution::ResolutionLimits::default().max_candidates,
                    );
                    let next = current + diagnostic.candidates.len();
                    if !((current > 0 && offset == previous)
                        || (next < diagnostic.candidate_count && offset == next))
                    {
                        return Err("Candidate page offset is not available in this report.");
                    }
                    (
                        report.request.clone(),
                        Query::Page {
                            token,
                            issue,
                            offset,
                            requirement,
                        },
                    )
                }
                Action::Clear | Action::Poll => return Ok(()),
            };
            if matches!(query, Query::Resolve) {
                self.report = None;
            }
            let input = request.clone();
            let lock_limits = document.limits().dependencies;
            let work = std::thread::Builder::new()
                .name("kagami-dependencies".into())
                .spawn(move || {
                    let answer = match query {
                        Query::Lock { selection, preview } => {
                            let lock = std::sync::Arc::new(
                                orishu_plugin::authoring_lock::SelectionLock::new(
                                    selection,
                                    lock_limits,
                                )
                                .map_err(|e| e.to_string())?,
                            );
                            Answer::Lock {
                                outcome: plugins
                                    .store
                                    .prepare_authoring_lock(
                                        lock,
                                        input.expected_inventory_revision,
                                        &plugins.overrides,
                                        lock_limits,
                                    )
                                    .map_err(|e| e.to_string())?,
                                preview,
                            }
                        }
                        Query::Installed {
                            token,
                            requirement,
                            offset,
                        } => {
                            let page = plugins
                                .store
                                .explicit_provider_page(
                                    &input,
                                    &plugins.overrides,
                                    &requirement,
                                    offset,
                                )
                                .map_err(|e| e.to_string())?;
                            Answer::Installed {
                                token,
                                requirement,
                                offset,
                                page,
                            }
                        }
                        Query::Resolve => Answer::Resolved(
                            plugins
                                .store
                                .resolve(&input, &plugins.overrides)
                                .map_err(|e| e.to_string())?,
                        ),
                        Query::Page {
                            token,
                            issue,
                            offset,
                            requirement,
                        } => Answer::Page {
                            token,
                            issue,
                            offset,
                            page: plugins
                                .store
                                .candidate_page(&input, &plugins.overrides, &requirement, offset)
                                .map_err(|e| e.to_string())?,
                        },
                    };
                    let guard = plugins
                        .store
                        .guard_revision(input.expected_inventory_revision)
                        .map_err(|e| e.to_string())?;
                    Ok(Completed {
                        answer,
                        providers: plugins
                            .store
                            .list()
                            .map_err(|e| e.to_string())?
                            .releases
                            .into_iter()
                            .map(|r| (r.release, r.plugin_id))
                            .collect(),
                        _revision: guard,
                    })
                })
                .map_err(|_| "Cannot start dependency discovery.")?;
            self.pending = Some(Pending {
                guard,
                generation: form.generation(),
                request,
                work,
            });
            self.notice = "Checking exact dependencies; no kernels are executing.".into();
            Ok(())
        })();
        if let Err(error) = result {
            self.notice = error.into();
        }
    }

    /// Adopt only a response for the unchanged document/form/inventory context.
    /// A stale or failed read releases its slot but never retries implicitly.
    pub fn poll(
        &mut self,
        document: &mut Document,
        form: &mut impl Proposal,
        revision: Option<u64>,
    ) {
        if self
            .report
            .as_ref()
            .is_some_and(|r| !r.current(document, form, revision))
        {
            self.report = None;
            self.notice = "Document or proposal changed; check dependencies again.".into();
        }
        if !self.pending.as_ref().is_some_and(|p| p.work.is_finished()) {
            return;
        }
        let pending = self.pending.take().expect("finished");
        let result = pending
            .work
            .join()
            .unwrap_or_else(|_| Err("Dependency worker failed.".into()));
        if !document.accepts_effect(pending.guard)
            || form.generation() != pending.generation
            || revision != Some(pending.request.expected_inventory_revision)
        {
            self.notice = "Stale dependency response discarded; check again explicitly.".into();
            self.report = None;
            return;
        }
        let done = match result {
            Ok(done) => done,
            Err(error) => {
                self.report = None;
                self.notice = error;
                return;
            }
        };
        let answer = match done.answer {
            Answer::Lock {
                outcome: crate::plugins::PrepareAuthoringLockOutcome::Ready(prepared),
                preview,
            } => {
                if !form.permits_lock_adoption() {
                    self.notice =
                        "Component choice adoption is not allowed for this proposal.".into();
                    return;
                }
                self.report = None;
                if preview {
                    self.notice = match form.prepare_properties(document, &prepared) {
                        Ok(()) => "Property fields loaded into the local proposal only. Enter values, then check dependencies again before saving.".into(),
                        Err(error) => error.into(),
                    };
                    return;
                }
                let mut commands = match form.lock_commands(document, &prepared) {
                    Ok(commands) => commands,
                    Err(error) => {
                        self.notice = error.into();
                        return;
                    }
                };
                if form.requires_scientific_extension() || form.requires_scientific_reset() {
                    let Some(kagami_document::ExperimentCommand::AdoptDependencies(lock)) =
                        commands.pop()
                    else {
                        self.notice =
                            "Captured addition omitted its complete component lock.".into();
                        return;
                    };
                    self.scientific_intent = Some(ComponentIntent {
                        reset: form.requires_scientific_reset(),
                        guard: pending.guard,
                        generation: pending.generation,
                        revision: pending.request.expected_inventory_revision,
                        attachments: commands,
                        lock,
                    });
                    self.notice = if form.requires_scientific_reset() {
                        "Replacement choices staged only. Choose complete physics, check its dependencies and confirm a full reset.".into()
                    } else {
                        "Choices checked; preparing the component and new integrator history. Fields are retained.".into()
                    };
                    return;
                }
                let mut schemas = document.schemas().clone();
                for schema in prepared.schemas().schemas() {
                    schemas.insert(schema.clone());
                }
                let accepted = document.edit_guarded_with_schemas(pending.guard, commands, schemas);
                self.notice = if accepted {
                    "Component provider choices saved as one undoable revision; no scientific state was initialized.".into()
                } else {
                    document
                        .notice
                        .clone()
                        .unwrap_or_else(|| "Component choice adoption was refused.".into())
                };
                return;
            }
            Answer::Lock {
                outcome: crate::plugins::PrepareAuthoringLockOutcome::Unresolved(outcome),
                ..
            } => Answer::Resolved(outcome),
            answer => answer,
        };
        match answer {
            Answer::Lock { .. } => unreachable!("handled above"),
            Answer::Resolved(outcome) => {
                self.notice = match &outcome {
                    ResolutionOutcome::Resolved { .. } => "Dependencies resolve. Apply will revalidate these choices before adoption.",
                    _ => "Dependencies need attention. Choose an exact provider or repair the unavailable contribution, then retry explicitly.",
                }.into();
                self.report = Some(Report {
                    token: Uuid::new_v4(),
                    guard: pending.guard,
                    generation: pending.generation,
                    request: pending.request,
                    outcome,
                    offsets: BTreeMap::new(),
                    providers: done.providers,
                    alternatives: None,
                });
            }
            Answer::Installed {
                token,
                requirement,
                offset,
                page,
            } => {
                let Some(report) = self.report.as_mut().filter(|r| r.token == token) else {
                    return;
                };
                match page {
                    CandidatePageResponse::Page {
                        candidates, total, ..
                    } => {
                        report.alternatives = Some(InstalledPage {
                            requirement,
                            candidates,
                            offset,
                            total,
                        });
                        report.token = Uuid::new_v4();
                        self.notice = "Compatible enabled installed providers, including non-default releases. Browsing changes no selection or defaults.".into();
                    }
                    CandidatePageResponse::StaleRevision { .. } => {
                        self.report = None;
                        self.notice = "Inventory changed; refresh and check again.".into();
                    }
                }
            }
            Answer::Page {
                token,
                issue,
                offset,
                page,
            } => {
                let Some(report) = self.report.as_mut().filter(|r| r.token == token) else {
                    return;
                };
                match page {
                    CandidatePageResponse::Page {
                        candidates, total, ..
                    } => {
                        if let ResolutionOutcome::Unavailable { issues, .. } = &mut report.outcome {
                            let Some(diagnostic) = issues.get_mut(issue) else {
                                return;
                            };
                            diagnostic.candidates = candidates;
                            diagnostic.candidate_count = total;
                            report.offsets.insert(issue, offset);
                            // Delayed clicks from the previous page cannot select a different pin.
                            report.token = Uuid::new_v4();
                            self.notice = "Candidate page updated; no provider selected.".into();
                        }
                    }
                    CandidatePageResponse::StaleRevision { .. } => {
                        self.report = None;
                        self.notice = "Inventory changed; refresh and check again.".into();
                    }
                }
            }
        }
    }
}
