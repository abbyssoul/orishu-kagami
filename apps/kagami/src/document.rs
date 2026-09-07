//! The app's side of the document boundary.
//!
//! `apps/kagami` is the imperative shell. It owns pointer events, dialogs and
//! pixels; it owns no rule about what an experiment is. This type is the seam:
//! every experiment change becomes a [`kagami_session::ExperimentCommandEnvelope`] submitted
//! to the authority, and what comes back is the only thing the UI renders.
//!
//! Two consequences are worth stating, because they are what make the boundary
//! real rather than decorative:
//!
//! - **The UI never mutates the model.** There is no path from a widget to an
//!   `Experiment`. If the authority refuses an edit, the view keeps showing the
//!   last accepted revision and the refusal is reported — an app that painted
//!   the edit optimistically would be presenting something no revision
//!   contains.
//! - **The UI is not privileged.** These envelopes are the same ones an MCP
//!   client sends (ADR 0006), carrying a command identity and an actor. Undo
//!   is a submitted command, not a local stack, so both callers share one
//!   history.

use std::path::PathBuf;

use kagami_catalog::{CatalogSet, SchemaRegistry};
use kagami_document::{
    CapabilityReport, Experiment, ExperimentCommand, ExperimentSnapshot, Limits,
};
use kagami_session::{
    ActorId, CommandId, DocumentAuthority, DocumentMetadata, DocumentTarget, ExperimentDocument,
    RealFileStore, SessionCommand, SessionView,
};

/// Who the window is, in every envelope it sends.
const ACTOR: &str = "ui";

/// What this build stamps on a document it writes.
const GENERATOR: &str = concat!("kagami ", env!("CARGO_PKG_VERSION"));

/// The authority, plus the little the shell needs to talk to it.
pub struct Document {
    authority: DocumentAuthority,
    /// The projection the window is currently drawing.
    ///
    /// Refreshed after every accepted submission and never otherwise, so the
    /// whole frame is drawn from one revision — a view that re-read the
    /// authority per widget could show half of an edit.
    view: SessionView,
    actor: ActorId,
    /// Correlation counter. Every submission gets its own identity so a
    /// resend can be recognised as one.
    next_command: u64,
    /// When the document was first saved, preserved across re-saves.
    created: Option<String>,
    /// The most recent refusal or IO failure, for the status line. Cleared by
    /// the next accepted submission, so it reads as feedback about what just
    /// happened rather than a log.
    pub notice: Option<String>,
}

impl Document {
    /// An empty experiment, validated against `schemas`.
    pub fn new(schemas: SchemaRegistry, limits: Limits) -> Self {
        let authority = DocumentAuthority::new(schemas, limits);
        Self {
            view: authority.view(),
            authority,
            actor: ActorId::new(ACTOR).expect("a constant actor name is valid"),
            next_command: 0,
            created: None,
            notice: None,
        }
    }

    /// Adopt a catalog snapshot for instantiation to resolve against.
    pub fn adopt_catalog(&mut self, catalog: CatalogSet) {
        self.authority.adopt_catalog(catalog);
    }

    /// Adopt a new snapshot of the installed component schemas.
    ///
    /// Availability feedback changes; the experiment does not. Refreshing the
    /// view is what makes the inspector show the new diagnostics without
    /// anything having been edited.
    pub fn adopt_schemas(&mut self, schemas: SchemaRegistry) {
        self.authority.adopt_schemas(schemas, self.actor.clone());
        self.view = self.authority.view();
    }

    /// What the UI renders.
    pub fn view(&self) -> &SessionView {
        &self.view
    }

    /// The experiment's contents at the revision being drawn.
    pub fn snapshot(&self) -> &ExperimentSnapshot {
        &self.view.experiment
    }

    /// What can be attached, for the inspector to offer.
    pub fn schemas(&self) -> &SchemaRegistry {
        self.authority.schemas()
    }

    /// Which components the installed schemas cannot govern.
    pub fn capabilities(&self) -> &CapabilityReport {
        &self.view.capabilities
    }

    /// Whether there is a catalog to instantiate from.
    pub fn has_catalog(&self) -> bool {
        self.authority.catalog().is_some()
    }

    /// Submit one authoring batch.
    ///
    /// Returns whether it was accepted. A refusal is recorded as a notice and
    /// changes nothing, which is the behaviour the whole boundary exists for.
    pub fn edit(&mut self, commands: Vec<ExperimentCommand>) -> bool {
        self.submit(SessionCommand::Edit(commands))
    }

    /// Submit one session command.
    pub fn submit(&mut self, command: SessionCommand) -> bool {
        let envelope = kagami_session::ExperimentCommandEnvelope::new(
            self.mint(),
            self.actor.clone(),
            command,
        );
        match self.authority.submit(envelope) {
            Ok(_) => {
                self.view = self.authority.view();
                self.notice = None;
                true
            }
            Err(rejection) => {
                // The refusal is reported and *nothing* is refreshed: the
                // window keeps drawing the last accepted revision, because
                // that is the only one that exists.
                self.notice = Some(rejection.to_string());
                false
            }
        }
    }

    /// Start a new, empty experiment.
    ///
    /// `discard_unsaved` is the answer to the question a dialog would ask. The
    /// authority refuses without it rather than deciding for anyone (ADR 0006).
    pub fn new_experiment(&mut self, discard_unsaved: bool) -> bool {
        let accepted = self.submit(SessionCommand::New { discard_unsaved });
        if accepted {
            self.created = None;
        }
        accepted
    }

    /// Read `path` and adopt it as the current experiment.
    pub fn open(&mut self, path: PathBuf, discard_unsaved: bool) -> bool {
        let Some(target) = self.target_for(path) else {
            return false;
        };
        let loaded = match kagami_session::load(&RealFileStore, &target) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.notice = Some(error.to_string());
                return false;
            }
        };
        let recovered = loaded.from;
        let created = loaded.document.metadata.created.clone();
        let experiment = match loaded
            .document
            .into_experiment(self.authority.schemas(), self.authority.limits())
        {
            Ok(experiment) => experiment,
            Err(error) => {
                self.notice = Some(error.to_string());
                return false;
            }
        };

        let accepted = self.submit(SessionCommand::Open {
            experiment: Box::new(experiment),
            target: Some(target),
            discard_unsaved,
        });
        if accepted {
            self.created = Some(created);
            // Reading anything but the document itself means an earlier save
            // was interrupted. The user is the one who can decide what to do
            // about that, so it is said out loud rather than passed silently.
            if recovered.is_recovery() {
                self.notice = Some(format!("Recovered from {recovered}."));
            }
        }
        accepted
    }

    /// Write the current experiment to `path`, or to its existing target.
    ///
    /// The revision written is captured *before* the write and named in the
    /// acknowledgement afterwards, so a completion that lands after another
    /// edit cannot mark the newer revision clean.
    pub fn save(&mut self, path: Option<PathBuf>, now: String) -> bool {
        let target = match path {
            Some(path) => self.target_for(path),
            None => self.authority.target().cloned(),
        };
        let Some(target) = target else {
            self.notice = Some("This experiment has no file yet; use Save As.".to_owned());
            return false;
        };

        let captured = self.authority.revision();
        let metadata = DocumentMetadata {
            generator: GENERATOR.to_owned(),
            created: self.created.clone().unwrap_or_else(|| now.clone()),
            saved: now,
            saved_revision: captured.get(),
        };
        let document = ExperimentDocument::of(
            &self.experiment_for_save(),
            &self.authority.snapshot(),
            metadata.clone(),
        );

        if let Err(error) = kagami_session::save(&RealFileStore, &target, &document) {
            self.notice = Some(error.to_string());
            return false;
        }

        self.created = Some(metadata.created);
        let acknowledgement = self.authority.acknowledge_save(captured, target);
        self.view = self.authority.view();
        if !acknowledgement.is_clean() {
            self.notice =
                Some("Saved, but the experiment has changed since — still unsaved.".to_owned());
        } else {
            self.notice = None;
        }
        true
    }

    /// The experiment value the codec needs, for its counters.
    fn experiment_for_save(&self) -> Experiment {
        self.authority.experiment().clone()
    }

    /// Where the document is, if it has been saved.
    pub fn target(&self) -> Option<&DocumentTarget> {
        self.authority.target()
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.authority.is_dirty()
    }

    fn target_for(&mut self, path: PathBuf) -> Option<DocumentTarget> {
        match DocumentTarget::new(path) {
            Ok(target) => Some(target),
            Err(error) => {
                self.notice = Some(error.to_string());
                None
            }
        }
    }

    fn mint(&mut self) -> CommandId {
        let id = CommandId::new(format!("{ACTOR}-{}", self.next_command))
            .expect("a counter-derived identity is valid");
        self.next_command += 1;
        id
    }
}
