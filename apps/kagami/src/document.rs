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
//!
//! # Why the workspace mode is here
//!
//! ADR 0022's mode gate is a question about this seam: may an authoring
//! command pass at all? Holding the [`Workspace`] here rather than beside the
//! other presentation state makes the answer unbypassable — every command
//! reaches the authority through [`Document::submit`], and that is where the
//! gate is. A gate checked in `update` would be one an added call site could
//! forget.
//!
//! The authority itself stays innocent of it, which ADR 0022 requires: it may
//! own a successor draft while another client observes a run, and client mode
//! is never one of its reasons for refusing a valid command.

use std::path::PathBuf;

use kagami_catalog::{CatalogSet, SchemaRegistry};
use kagami_document::{
    CapabilityReport, Experiment, ExperimentCommand, ExperimentSnapshot, Limits,
};
use kagami_session::{
    ActorId, AuthoringView, AuthoringViewState, CameraMotion, CameraPose, CommandId,
    DocumentAuthority, DocumentMetadata, DocumentTarget, ExperimentDocument, LeaveConsequence,
    Projection, RealFileStore, RunAttachment, SceneScale, SessionCommand, SessionView,
    StoredDefaultView, ViewChange, ViewError, ViewRevision, Workspace, WorkspaceMode,
    WorkspaceRejection,
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
    /// How the experiment is being looked at while authoring, and whether that
    /// has been saved.
    ///
    /// Beside the authority rather than inside it: ADR 0022 makes the default
    /// view client-owned and invisible to workload compilation, so the
    /// authority must not learn about it. It is here rather than in
    /// [`crate::model::Model`] because saving has to capture it, and because
    /// one file-modified indication is derived from both halves.
    authoring_view: AuthoringViewState,
    /// Whether authoring is routed at all (ADR 0022).
    ///
    /// Consulted by [`Self::submit`] before an envelope exists, so a command
    /// arriving in the wrong mode never reaches the authority — and cannot
    /// change the mode as a side effect of being refused.
    workspace: Workspace,
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
            authoring_view: AuthoringViewState::new(),
            // ADR 0022: a session always begins in Authoring, and no saved
            // file can say otherwise.
            workspace: Workspace::authoring(),
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

    /// Which workspace mode this window is in.
    pub fn mode(&self) -> &WorkspaceMode {
        self.workspace.mode()
    }

    /// `true` when authoring is available.
    pub fn is_authoring(&self) -> bool {
        self.workspace.is_authoring()
    }

    /// Attach to `attachment` and enter Observation/replay.
    ///
    /// # Errors
    ///
    /// Returns the workspace's refusal when a run is already attached.
    pub fn observe(&mut self, attachment: RunAttachment) -> Result<(), WorkspaceRejection> {
        let outcome = self.workspace.observe(attachment);
        if outcome.is_ok() {
            self.notice = None;
        }
        outcome
    }

    /// Return to Authoring, reporting what leaving the run costs.
    ///
    /// Nothing about the observed run is adopted: ADR 0022 keeps adopting a
    /// computed state a separate validated workflow, so this returns to the
    /// authored scene exactly as it was — at exactly the revision it was.
    ///
    /// # Errors
    ///
    /// Returns the workspace's refusal when there is no run to leave.
    pub fn edit_initial_conditions(&mut self) -> Result<LeaveConsequence, WorkspaceRejection> {
        let outcome = self.workspace.edit_initial_conditions();
        if outcome.is_ok() {
            self.notice = None;
        }
        outcome
    }

    /// How the experiment is being looked at while authoring.
    pub fn authoring_view(&self) -> AuthoringView {
        self.authoring_view.view()
    }

    /// The authoring view's revision, separate from the experiment's.
    pub fn view_revision(&self) -> ViewRevision {
        self.authoring_view.revision()
    }

    /// Choose the authoring projection. Returns whether anything changed.
    ///
    /// Not a submission. It advances the *view* revision and marks the file
    /// dirty, and it touches neither the experiment revision nor undo — which
    /// is the entire point of ADR 0022's split.
    pub fn set_projection(&mut self, projection: Projection) -> bool {
        self.authoring_view.set_projection(projection)
    }

    /// Choose the authoring scene scale. Returns whether anything changed.
    ///
    /// Presentation, exactly like the projection: it decides what the camera
    /// can reach, and nothing about what the experiment *is*. No authored
    /// position, extent or constant is expressed in these units.
    /// Returns whether the scale was **accepted**, which a choice already in
    /// force also is. Deliberately not "did anything change": a caller needs to
    /// know whether to keep an entry field open for correction, and a no-op is
    /// nothing to correct.
    pub fn set_scale(&mut self, scale: SceneScale) -> bool {
        self.reporting(|view| view.set_scale(scale))
    }

    /// Move the authoring camera. Returns whether anything changed.
    ///
    /// A gesture clamped at a limit is not reported: that is what a limit feels
    /// like, and a notice per frame of a held drag would be noise.
    pub fn move_camera(&mut self, motion: CameraMotion) -> bool {
        match self.authoring_view.move_camera(motion) {
            Ok(changed) => changed,
            Err(error) => {
                self.notice = Some(error.to_string());
                false
            }
        }
    }

    /// Place the authoring camera. Returns whether it was accepted.
    pub fn set_camera(&mut self, camera: CameraPose) -> bool {
        self.reporting(|view| view.set_camera(camera))
    }

    /// Apply one explicit view change, reporting whatever the user needs told.
    ///
    /// Returns whether the change was **accepted**, which is not the same
    /// question as whether it produced a notice — and conflating the two was a
    /// bug: an accepted scale change that adjusted the camera set a notice, and
    /// a caller reading "notice present" as "refused" then kept an entry field
    /// open over a change that had worked.
    ///
    /// Two things get reported, and both matter:
    ///
    /// - A **refusal**. A view that cannot go where it was asked is worth
    ///   saying once and not worth failing over: the window keeps the view it
    ///   had, the same way a refused edit leaves the experiment alone.
    /// - An **adjustment**. Choosing nanometre scale takes the default camera
    ///   from eighteen metres to two micrometres. Applying that silently
    ///   leaves someone looking at something they did not ask for with no clue
    ///   why, so the amount it moved is said out loud.
    fn reporting(
        &mut self,
        change: impl FnOnce(&mut AuthoringViewState) -> Result<ViewChange, ViewError>,
    ) -> bool {
        match change(&mut self.authoring_view) {
            Ok(outcome) => {
                self.notice = outcome.adjustment.message();
                true
            }
            Err(error) => {
                self.notice = Some(error.to_string());
                false
            }
        }
    }

    /// Submit one authoring batch.
    ///
    /// Returns whether it was accepted. A refusal is recorded as a notice and
    /// changes nothing, which is the behaviour the whole boundary exists for.
    pub fn edit(&mut self, commands: Vec<ExperimentCommand>) -> bool {
        self.submit(SessionCommand::Edit(commands))
    }

    /// Submit one session command.
    ///
    /// The mode gate runs first, before an identity is minted or an envelope
    /// built. This is the only route to the authority, so it is also the only
    /// place the gate has to be.
    pub fn submit(&mut self, command: SessionCommand) -> bool {
        if let Err(rejection) = self.workspace.admit(&command) {
            self.notice = Some(rejection.to_string());
            return false;
        }

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

    /// Whether a replacement may discard what is unsaved *here*.
    ///
    /// The authority answers this question for the experiment, and refuses
    /// without `discard_unsaved`. It cannot answer it for the authoring view,
    /// because ADR 0022 makes the view client-owned and the authority is not
    /// allowed to learn about it. So the shell asks the same question about its
    /// own half, with the same rule: nobody's unsaved work is discarded without
    /// an explicit decision (ADR 0006).
    ///
    /// Checked before the authority is consulted, so a refusal costs nothing
    /// and leaves both halves exactly as they were.
    fn may_discard_view(&mut self, discard_unsaved: bool) -> bool {
        if self.authoring_view.is_dirty() && !discard_unsaved {
            self.notice = Some(
                "The saved view has unsaved changes; save it or choose to discard them.".to_owned(),
            );
            return false;
        }
        true
    }

    /// Start a new, empty experiment.
    ///
    /// `discard_unsaved` is the answer to the question a dialog would ask. The
    /// authority refuses without it rather than deciding for anyone (ADR 0006).
    pub fn new_experiment(&mut self, discard_unsaved: bool) -> bool {
        if !self.may_discard_view(discard_unsaved) {
            return false;
        }
        let accepted = self.submit(SessionCommand::New { discard_unsaved });
        if accepted {
            self.created = None;
            // A new experiment opens at the default camera, clean. Carrying
            // the previous document's view over would make an untitled
            // experiment born modified.
            self.authoring_view.adopt(AuthoringView::default());
        }
        accepted
    }

    /// Read `path` and adopt it as the current experiment.
    ///
    /// The mode is checked before the filesystem is touched. [`Self::submit`]
    /// would refuse this anyway while a run is being watched, but reading and
    /// decoding a document only to discard it is work nobody asked for — and
    /// the refusal is more honest about *why* if it arrives before the IO
    /// rather than after a load failure that was never the real problem.
    pub fn open(&mut self, path: PathBuf, discard_unsaved: bool) -> bool {
        if let Err(rejection) = self.workspace.admit_kind("open") {
            self.notice = Some(rejection.to_string());
            return false;
        }
        if !self.may_discard_view(discard_unsaved) {
            return false;
        }

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

        // Interpret the view section before the experiment consumes the
        // document, and keep the two outcomes apart: a view this build cannot
        // read is a warning, never a reason the experiment fails to open
        // (ADR 0022).
        let mut warnings = Vec::new();
        let view = match loaded
            .document
            .default_view
            .as_ref()
            .map(StoredDefaultView::decode)
        {
            // No saved view. Defined absence, not a missing value: open at the
            // default camera.
            None => AuthoringView::default(),
            Some(Ok(view)) => view,
            Some(Err(error)) => {
                warnings.push(format!("{error}. Saving will replace it."));
                AuthoringView::default()
            }
        };

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
            self.authoring_view.adopt(view);
            // Reading anything but the document itself means an earlier save
            // was interrupted. The user is the one who can decide what to do
            // about that, so it is said out loud rather than passed silently.
            if recovered.is_recovery() {
                warnings.push(format!("Recovered from {recovered}."));
            }
            if !warnings.is_empty() {
                self.notice = Some(warnings.join(" "));
            }
        }
        accepted
    }

    /// Write the current experiment to `path`, or to its existing target.
    ///
    /// Both revisions written are captured *before* the write and named in
    /// their acknowledgements afterwards, so a completion that lands after
    /// another edit — of the experiment or of the camera — cannot mark the
    /// newer state clean. Saving records the view in force and never changes
    /// it (ADR 0022).
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
        let captured_view = self.authoring_view.revision();
        let metadata = DocumentMetadata {
            generator: GENERATOR.to_owned(),
            created: self.created.clone().unwrap_or_else(|| now.clone()),
            saved: now,
            saved_revision: captured.get(),
        };
        let document = ExperimentDocument::of(
            &self.experiment_for_save(),
            &self.authority.snapshot(),
            &self.authoring_view.view(),
            metadata.clone(),
        );

        if let Err(error) = kagami_session::save(&RealFileStore, &target, &document) {
            self.notice = Some(error.to_string());
            return false;
        }

        self.created = Some(metadata.created);
        // Each half acknowledges only itself; the combined indication is the
        // OR in `is_dirty`. That is what makes "clean only if both still
        // match" true without either half having to know about the other.
        let experiment_saved = self.authority.acknowledge_save(captured, target).is_clean();
        let view_saved = self
            .authoring_view
            .acknowledge_save(captured_view)
            .is_clean();
        self.view = self.authority.view();
        self.notice = match (experiment_saved, view_saved) {
            (true, true) => None,
            (false, _) => {
                Some("Saved, but the experiment has changed since — still unsaved.".to_owned())
            }
            (true, false) => {
                Some("Saved, but the view has changed since — still unsaved.".to_owned())
            }
        };
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

    /// Whether there are unsaved changes, of either kind.
    ///
    /// One indication derived from both halves (ADR 0022): unsaved experiment
    /// intent *or* an unsaved authoring view. The user is told the file has
    /// changed, not which section changed — the title bar is not the place to
    /// explain the file format.
    pub fn is_dirty(&self) -> bool {
        self.authority.is_dirty() || self.authoring_view.is_dirty()
    }

    /// Whether the experiment itself has unsaved changes.
    ///
    /// Not what the user is shown or asked — [`Self::is_dirty`] is, and it
    /// covers both halves. This exposes the *separation* ADR 0022 requires, so
    /// that "the camera moved but the science did not" is a checkable property
    /// rather than a claim. Anything deciding whether work is about to be lost
    /// wants [`Self::is_dirty`]: a view someone framed and has not saved is
    /// also work.
    pub fn experiment_is_dirty(&self) -> bool {
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
