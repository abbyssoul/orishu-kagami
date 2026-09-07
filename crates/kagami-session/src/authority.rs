//! The one document authority.
//!
//! ADR 0004 gives Kagami a document authority; ADR 0006 requires the UI and
//! MCP to reach it identically; ADR 0019 makes this type it. Every source of
//! authored change submits the same [`ExperimentCommandEnvelope`] here, and
//! nothing else in the product can move an experiment: [`DocumentAuthority`]
//! exposes no mutable access to the model and returns no handle that does.
//!
//! Four properties make that boundary useful rather than ceremonial, and they
//! are deliberately the same four `kagami_catalog::CatalogAuthority`
//! established, so an adapter learns one pattern for both authorities:
//!
//! - **Guarded.** A submission may name the [`ExperimentRevision`] it was
//!   composed against; a mismatch is a refusal, so two adapters editing
//!   concurrently cannot silently clobber each other.
//! - **Idempotent.** A submission carries a [`CommandId`]; resending the same
//!   request replays its recorded outcome instead of applying the change
//!   twice. Reusing that identity for a *different* request is refused, so an
//!   adapter's counter collision cannot hand one actor another's outcome. A
//!   *failed* command is not recorded, so a retry after a transient error
//!   still runs.
//! - **Attributed.** Every accepted command records its actor.
//! - **Observable.** Every accepted command appends one bounded
//!   [`ExperimentEvent`], so a view catches up from where it was rather than
//!   re-reading everything.
//!
//! # Installing a plugin is not an edit
//!
//! What an experiment *contains* and what this installation can *do with it*
//! are separate facts, and only the first is authored. The registry an edit is
//! validated against is therefore replaceable through
//! [`DocumentAuthority::adopt_schemas`], which recomputes the capability
//! projection and publishes an event while leaving the experiment, its
//! revision, its history and its dirty state exactly as they were.
//!
//! # What is not here
//!
//! No run: no play, pause, step, clock, tick pacing, solver, or observation.
//! ADR 0004 puts those behind a separate run authority, and Field CAD's
//! tick-boundary command queue does not transfer because no solver writes this
//! model. No transport, no presentation state, and no catalog writes — the
//! catalog keeps its own authority and its own revision. No filesystem: the
//! authority records *which* revision was written and where
//! ([`mod@crate::persist`]), and task K4 owns the bytes.
//!
//! # One revision per submitted batch
//!
//! An accepted [`SessionCommand::Edit`] always advances the revision, even if
//! the commands happen to write the values that were already there.
//! Recognising a semantically null edit means deep-comparing the candidate
//! against the current experiment on the interactive path, and "one revision
//! per submitted batch" is the simpler contract to reason about. An adapter
//! that knows its edit is a no-op should not submit it.

use std::collections::VecDeque;
use std::sync::Arc;

use kagami_catalog::materialize::{InstantiationRequest, ObjectCandidate, materialize};
use kagami_catalog::{CatalogSet, SchemaRegistry};
use kagami_document::{
    AuthoredValue, CapabilityReport, CommitReport, ComponentProperties, EditHistory, Experiment,
    ExperimentCommand, ExperimentRevision, ExperimentSnapshot, GestureId, Limits, ObjectSpec,
    VariableSpec, resolve_variables, restore, update,
};
use orishu_variables::Namespace;

use crate::command::{ExperimentCommandEnvelope, InstantiationSpec, SessionCommand};
use crate::identity::{ActorId, CommandId};
use crate::outcome::{Acceptance, EventSeq, ExperimentChange, ExperimentEvent, SessionRejection};
use crate::persist::{DocumentTarget, SaveAcknowledgement};
use crate::view::{HistoryStatus, SessionView};

/// How many accepted submissions are remembered for idempotent replay.
pub const MAX_COMMAND_HISTORY: usize = 256;

/// How many authoring commands the replay window retains in total.
///
/// The second axis of the same bound. A remembered submission holds the whole
/// batch it carried, because replaying an identity is only safe after checking
/// that the request is the same one; and a batch may carry
/// [`Limits::max_commands_per_batch`] commands. Counting submissions alone
/// would let an MCP caller pin that product in memory, so eviction runs until
/// *both* bounds hold.
pub const MAX_REPLAY_COMMANDS: usize = 4_096;

/// How many change events are retained for catch-up.
pub const MAX_EVENT_HISTORY: usize = 256;

/// One accepted submission, kept so an identical resubmission can be replayed.
///
/// The whole envelope is retained, not just its identity: see
/// [`SessionRejection::CommandIdentityConflict`].
#[derive(Clone, Debug, PartialEq)]
struct AcceptedRecord {
    envelope: ExperimentCommandEnvelope,
    acceptance: Acceptance,
}

impl AcceptedRecord {
    /// How many authoring commands this record holds against
    /// [`MAX_REPLAY_COMMANDS`].
    ///
    /// A submission that carries no batch still counts as one, so a flood of
    /// gesture brackets is bounded by the same budget.
    fn weight(&self) -> usize {
        match &self.envelope.command {
            SessionCommand::Edit(commands) => commands.len().max(1),
            _ => 1,
        }
    }
}

/// The authoritative owner of the editable experiment.
pub struct DocumentAuthority {
    experiment: Experiment,
    schemas: SchemaRegistry,
    catalog: Option<Arc<CatalogSet>>,
    capabilities: Arc<CapabilityReport>,
    limits: Limits,
    history: EditHistory,
    accepted: VecDeque<AcceptedRecord>,
    retained_commands: usize,
    events: VecDeque<ExperimentEvent>,
    next_event: EventSeq,
    open_gesture: Option<GestureId>,
    next_gesture: u64,
    clean_revision: ExperimentRevision,
    acknowledged_revision: Option<ExperimentRevision>,
    target: Option<DocumentTarget>,
}

impl DocumentAuthority {
    /// A session holding a new, empty experiment.
    ///
    /// `schemas` is the installed simulation-plugin component declarations to
    /// validate every edit against — an owned value the caller supplies, never
    /// a registry this type reaches for.
    pub fn new(schemas: SchemaRegistry, limits: Limits) -> Self {
        let experiment = Experiment::new();
        let clean_revision = experiment.revision();
        Self {
            experiment,
            schemas,
            catalog: None,
            // An empty experiment has nothing for a schema to govern.
            capabilities: Arc::new(CapabilityReport::default()),
            history: EditHistory::new(limits.max_undo_depth),
            limits,
            accepted: VecDeque::new(),
            retained_commands: 0,
            events: VecDeque::new(),
            next_event: EventSeq::INITIAL,
            open_gesture: None,
            next_gesture: 0,
            clean_revision,
            acknowledged_revision: None,
            target: None,
        }
    }

    /// Decide one submission.
    ///
    /// This is the only way an experiment changes.
    ///
    /// # Errors
    ///
    /// Returns a [`SessionRejection`] describing the authority's decision. A
    /// refusal leaves the revision, the experiment, the history and the event
    /// log exactly as they were.
    pub fn submit(
        &mut self,
        envelope: ExperimentCommandEnvelope,
    ) -> Result<Acceptance, SessionRejection> {
        if let Some(recorded) = self.replay(&envelope)? {
            return Ok(recorded);
        }

        if let Some(expected) = envelope.expected_revision
            && expected != self.experiment.revision()
        {
            return Err(SessionRejection::RevisionConflict {
                expected,
                current: self.experiment.revision(),
            });
        }

        // Resolve the gesture *before* anything is applied, so a stale
        // identity is a refusal rather than a silent coalescence into a
        // finished drag's undo entry.
        let gesture = self.resolve_gesture(&envelope)?;

        let change = match &envelope.command {
            SessionCommand::Edit(commands) => self.apply_edit(commands, gesture)?,
            SessionCommand::Undo => self.apply_undo()?,
            SessionCommand::Redo => self.apply_redo()?,
            SessionCommand::BeginInteractiveEdit => self.open_gesture(),
            SessionCommand::EndInteractiveEdit => self.close_gesture()?,
            SessionCommand::InstantiateObjectTemplate(spec) => self.instantiate(spec, gesture)?,
            SessionCommand::New { discard_unsaved } => {
                self.replace(Experiment::new(), None, *discard_unsaved, false)?
            }
            SessionCommand::Open {
                experiment,
                target,
                discard_unsaved,
            } => self.replace(
                (**experiment).clone(),
                target.clone(),
                *discard_unsaved,
                true,
            )?,
        };

        Ok(self.record(envelope, change))
    }

    /// Run a submission's validation without changing anything.
    ///
    /// Advisory only: the authority remains the final decider, and a preflight
    /// that passed can still be refused later if the experiment moves in
    /// between. It exists so an automation client can repair its input before
    /// committing to it, using the *same* rules — a second, looser validation
    /// path would be worse than none.
    ///
    /// The returned report predicts what the edit would create and remove.
    /// Those identities hold only if nothing intervenes, because an identity
    /// is minted when an edit is actually accepted.
    ///
    /// # Errors
    ///
    /// Returns the [`SessionRejection`] the equivalent [`Self::submit`] would.
    pub fn preflight(
        &self,
        envelope: &ExperimentCommandEnvelope,
    ) -> Result<CommitReport, SessionRejection> {
        if let Some(expected) = envelope.expected_revision
            && expected != self.experiment.revision()
        {
            return Err(SessionRejection::RevisionConflict {
                expected,
                current: self.experiment.revision(),
            });
        }
        match &envelope.command {
            SessionCommand::Edit(commands) => {
                if commands.is_empty() {
                    return Err(SessionRejection::EmptyBatch);
                }
                let candidate = update(&self.experiment, commands, &self.schemas, &self.limits)?;
                Ok(candidate.report().clone())
            }
            // Undo, redo, gesture brackets and lifecycle operations have no
            // batch to check; whether they are available is a read, not a
            // validation. A decoded document was already validated on the way
            // out of the codec.
            other => Err(SessionRejection::NotPreflightable { kind: other.kind() }),
        }
    }

    /// The experiment's contents, and the revision they are.
    pub fn snapshot(&self) -> ExperimentSnapshot {
        self.experiment.snapshot()
    }

    /// The experiment itself, for a caller that needs its identity counters.
    ///
    /// Encoding a document needs them, because an identity that was minted
    /// and removed must never be handed out again. A snapshot deliberately
    /// does not carry them: they span the whole history rather than describing
    /// one revision. This hands out a value, so it is still no way to mutate
    /// anything.
    pub fn experiment(&self) -> &Experiment {
        &self.experiment
    }

    /// The revision in force.
    pub fn revision(&self) -> ExperimentRevision {
        self.experiment.revision()
    }

    /// The session as an adapter sees it now.
    pub fn view(&self) -> SessionView {
        SessionView {
            experiment: self.experiment.snapshot(),
            capabilities: Arc::clone(&self.capabilities),
            history: self.history_status(),
            dirty: self.is_dirty(),
            target: self.target.clone(),
            open_gesture: self.open_gesture,
            last_event: self.next_event,
        }
    }

    /// Undo and redo availability, and what each would do.
    pub fn history_status(&self) -> HistoryStatus {
        HistoryStatus {
            undo: self.history.undo_label().map(str::to_owned),
            redo: self.history.redo_label().map(str::to_owned),
            depth: self.history.len(),
            capacity: self.history.depth(),
        }
    }

    /// The catalog snapshot instantiation resolves against.
    ///
    /// `None` until one is adopted: an installation with no catalog can still
    /// author everything by hand.
    pub fn catalog(&self) -> Option<&CatalogSet> {
        self.catalog.as_deref()
    }

    /// Adopt a catalog snapshot for instantiation to resolve against.
    ///
    /// Like [`Self::adopt_schemas`], this is not an edit: reloading a catalog
    /// changes what can be *instantiated next* and nothing about what has
    /// already been materialised. It publishes no event because it alters no
    /// projection over the experiment — an object that came from a template
    /// carries a copy, not a link (ADR 0008), so the catalog moving cannot
    /// change it.
    pub fn adopt_catalog(&mut self, catalog: CatalogSet) {
        self.catalog = Some(Arc::new(catalog));
    }

    /// The installed component schemas an edit is validated against.
    ///
    /// This is the capability-discovery surface: a caller reads the component
    /// types, their properties, kinds, dimensions and required flags, and can
    /// then construct a valid command instead of guessing. Exposed as the
    /// registry itself rather than a derived description, so there is one
    /// answer to "what can be attached" rather than two that can disagree.
    pub fn schemas(&self) -> &SchemaRegistry {
        &self.schemas
    }

    /// Which of the experiment's components the installed schemas can govern.
    ///
    /// A projection, recomputed when the contents or the registry change, and
    /// never part of the experiment: the same authored document has different
    /// diagnostics on a machine with different plugins installed.
    pub fn capabilities(&self) -> &CapabilityReport {
        &self.capabilities
    }

    /// Adopt a new snapshot of the installed component schemas.
    ///
    /// The one operation that changes what an experiment's existing content
    /// *means here*. Installing or removing a simulation plugin is not an
    /// edit: it mutates no experiment, advances no revision, adds no history
    /// entry, and cannot make a clean document dirty. What it does change is
    /// the capability projection, and that is worth one sequenced event —
    /// a view watching only the revision would otherwise keep offering edits
    /// the authority will now refuse.
    ///
    /// `schemas` is an owned snapshot supplied by the plugin-inventory adapter
    /// (X-PLUGIN's to build); this type never reaches for a registry. `actor`
    /// attributes the change the same way a submission does, so an event log
    /// reads uniformly.
    ///
    /// Returns the sequence number of the published event.
    pub fn adopt_schemas(&mut self, schemas: SchemaRegistry, actor: ActorId) -> EventSeq {
        let before = self.capabilities.summary();
        self.schemas = schemas;
        self.rebuild_capabilities();
        let after = self.capabilities.summary();
        self.push_event(
            None,
            actor,
            ExperimentChange::CapabilityChanged { before, after },
        )
    }

    /// The bounds every submission is checked against.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// `true` when the experiment has advanced past the revision last
    /// persisted.
    pub fn is_dirty(&self) -> bool {
        self.experiment.revision() != self.clean_revision
    }

    /// The revision the experiment was last persisted at.
    pub fn clean_revision(&self) -> ExperimentRevision {
        self.clean_revision
    }

    /// Where this session is saved, once a write has succeeded.
    pub fn target(&self) -> Option<&DocumentTarget> {
        self.target.as_ref()
    }

    /// Acknowledge that `revision` was successfully written to `target`.
    ///
    /// Called by the document format after a completed write (task K4), which
    /// is what makes dirty state *derived* rather than a flag an adapter
    /// remembers to clear.
    ///
    /// Both arguments are load-bearing. A save is asynchronous, so the
    /// experiment may already have moved past the revision that was captured;
    /// the session is marked clean only when the acknowledged revision is the
    /// one in force, and a completion for an older snapshot leaves a newer one
    /// dirty. The target is adopted with the acknowledgement rather than when
    /// the write is *started*, so a Save As that fails — and therefore never
    /// acknowledges — leaves both the previous target and the dirty state
    /// exactly as they were.
    ///
    /// Publishes no event: persistence changes no contents, and what is
    /// observable about it is already in [`Self::view`].
    pub fn acknowledge_save(
        &mut self,
        revision: ExperimentRevision,
        target: DocumentTarget,
    ) -> SaveAcknowledgement {
        // Two writes can complete out of order. The older one still wrote real
        // bytes, but it describes a document the newer one has superseded, so
        // it must not retarget the session or move the clean marker back.
        if let Some(acknowledged) = self.acknowledged_revision
            && revision < acknowledged
        {
            return SaveAcknowledgement::Stale { acknowledged };
        }

        self.acknowledged_revision = Some(revision);
        self.target = Some(target);
        let current = self.experiment.revision();
        if revision == current {
            self.clean_revision = revision;
            SaveAcknowledgement::Clean
        } else {
            SaveAcknowledgement::Superseded { current }
        }
    }

    /// Every retained event published after `seq`, oldest first.
    ///
    /// A view stores [`SessionView::last_event`] and asks for everything
    /// since. When the view has fallen further behind than
    /// [`MAX_EVENT_HISTORY`], [`Self::can_catch_up_from`] says so and the
    /// caller re-reads the snapshot instead of silently missing changes.
    pub fn events_since(&self, seq: EventSeq) -> impl Iterator<Item = &ExperimentEvent> {
        self.events.iter().filter(move |event| event.seq > seq)
    }

    /// `true` when [`Self::events_since`] can report every change made after
    /// `seq` — that is, when nothing has been dropped from the retained
    /// window.
    pub fn can_catch_up_from(&self, seq: EventSeq) -> bool {
        match self.events.front() {
            // Nothing published yet: a caller at the initial sequence is by
            // definition current.
            None => seq <= self.next_event,
            Some(oldest) => seq.next() >= oldest.seq,
        }
    }

    /// Forget every undo and redo entry.
    ///
    /// Session setup — the empty starting experiment, later a loaded file —
    /// authors through the same command path as any edit, which is what keeps
    /// validation and attribution uniform, and then calls this. The opening
    /// undo of a session emptying the workspace is not a feature.
    ///
    /// It changes no contents and advances no revision, so it is not a
    /// submission and publishes no event.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Answer a resubmission from the recorded window, if this identity is in
    /// it.
    ///
    /// Replay is bound to the whole request, not just its identity. A caller
    /// that reuses an identity for a different actor, guard, gesture or batch
    /// is not retrying: answering it with the first request's outcome would
    /// report a change it never asked for, and could hand one actor another
    /// actor's result. That is refused, and the refusal says nothing about
    /// what was recorded.
    fn replay(
        &self,
        envelope: &ExperimentCommandEnvelope,
    ) -> Result<Option<Acceptance>, SessionRejection> {
        let Some(record) = self
            .accepted
            .iter()
            .find(|record| record.envelope.command_id == envelope.command_id)
        else {
            return Ok(None);
        };
        if &record.envelope != envelope {
            return Err(SessionRejection::CommandIdentityConflict {
                command_id: envelope.command_id.clone(),
            });
        }
        let mut replayed = record.acceptance.clone();
        replayed.replayed = true;
        Ok(Some(replayed))
    }

    /// Decide which gesture, if any, this submission belongs to, closing an
    /// open one that the submission does not name.
    fn resolve_gesture(
        &mut self,
        envelope: &ExperimentCommandEnvelope,
    ) -> Result<Option<GestureId>, SessionRejection> {
        match (envelope.gesture, self.open_gesture) {
            (Some(named), Some(open)) if named == open => Ok(Some(open)),
            // A named gesture that is not the open one — or any named gesture
            // with none open — is stale. Refusing keeps a finished drag's
            // entry from absorbing unrelated later edits.
            (Some(named), _) => Err(SessionRejection::GestureNotOpen { gesture: named }),
            // An unnamed submission ends whatever was holding. This is what
            // stops an adapter that dropped its `EndInteractiveEdit` from
            // coalescing the rest of the session into one undo entry.
            (None, Some(open))
                if !matches!(envelope.command, SessionCommand::EndInteractiveEdit) =>
            {
                self.publish_implicit_gesture_close(envelope, open);
                Ok(None)
            }
            (None, _) => Ok(None),
        }
    }

    fn publish_implicit_gesture_close(
        &mut self,
        envelope: &ExperimentCommandEnvelope,
        gesture: GestureId,
    ) {
        self.open_gesture = None;
        let change = ExperimentChange::GestureClosed {
            gesture,
            implicit: true,
        };
        // Attributed to the submission that caused it, but not recorded under
        // its command identity: that identity still names the command the
        // caller sent, and replaying it must replay *that*, not this.
        self.push_event(
            Some(envelope.command_id.clone()),
            envelope.actor.clone(),
            change,
        );
    }

    fn apply_edit(
        &mut self,
        commands: &[ExperimentCommand],
        gesture: Option<GestureId>,
    ) -> Result<ExperimentChange, SessionRejection> {
        if commands.is_empty() {
            return Err(SessionRejection::EmptyBatch);
        }
        // Build and validate before touching anything: the candidate is what
        // may fail, and it must fail before the history has been written.
        let candidate = update(&self.experiment, commands, &self.schemas, &self.limits)?;
        let label = ExperimentCommand::batch_label(commands);
        self.history
            .record(self.experiment.checkpoint(), label, gesture);
        let (experiment, report) = candidate.adopt();
        self.experiment = experiment;
        // An accepted edit cannot introduce a capability gap — the document
        // refuses a batch that touches content the installed schemas do not
        // govern — so the projection only ever loses entries here.
        self.prune_capabilities();
        Ok(ExperimentChange::Edited { report })
    }

    fn apply_undo(&mut self) -> Result<ExperimentChange, SessionRejection> {
        // Operate on a copy of the history so a refused restore costs nothing:
        // `EditHistory::undo` moves an entry across as it hands it back, and a
        // candidate that was representable when captured may not be now.
        let mut history = self.history.clone();
        let restoration = history
            .undo(self.experiment.checkpoint())
            .ok_or(SessionRejection::NothingToUndo)?;
        let candidate = restore(
            &self.experiment,
            &restoration.checkpoint,
            &self.schemas,
            &self.limits,
        )?;
        self.history = history;
        let (experiment, _) = candidate.adopt();
        self.experiment = experiment;
        // A restore replaces the contents wholesale, so unlike an edit it can
        // bring back content whose plugin has since been uninstalled.
        self.rebuild_capabilities();
        Ok(ExperimentChange::Undone {
            label: restoration.label,
        })
    }

    fn apply_redo(&mut self) -> Result<ExperimentChange, SessionRejection> {
        let mut history = self.history.clone();
        let restoration = history
            .redo(self.experiment.checkpoint())
            .ok_or(SessionRejection::NothingToRedo)?;
        let candidate = restore(
            &self.experiment,
            &restoration.checkpoint,
            &self.schemas,
            &self.limits,
        )?;
        self.history = history;
        let (experiment, _) = candidate.adopt();
        self.experiment = experiment;
        self.rebuild_capabilities();
        Ok(ExperimentChange::Redone {
            label: restoration.label,
        })
    }

    /// Materialise a catalog template into the experiment.
    ///
    /// Resolution happens *here*, at the session boundary, and never inside
    /// the pure transition: giving the sans-IO model a catalog authority as a
    /// hidden input is precisely what ADR 0019 keeps it free of. What reaches
    /// the model is an ordinary command batch over one immutable snapshot's
    /// answer, so an instantiation is validated exactly as a hand-authored
    /// object would be.
    fn instantiate(
        &mut self,
        spec: &InstantiationSpec,
        gesture: Option<GestureId>,
    ) -> Result<ExperimentChange, SessionRejection> {
        let catalog = self
            .catalog
            .clone()
            .ok_or(SessionRejection::NoCatalogLoaded)?;

        // The object's own scope has to be known before it exists, because the
        // copied definitions are rewritten into it. The next identity the
        // counters will mint is the one this batch is about to use.
        let scope = Namespace::new(format!(
            "objects.object_{}",
            self.experiment.counters().objects_minted()
        ));

        // Instantiation *materialises*: a document variable an override reads
        // is captured as the literal it resolves to now, not as a live
        // reference (ADR 0018).
        let document_values = resolve_variables(&self.experiment.snapshot(), &self.limits)?;

        let request = InstantiationRequest {
            identity: spec.template.clone(),
            expected_fingerprint: spec.expected_fingerprint,
            bindings: spec.bindings.clone(),
            object_scope: scope.clone(),
            document_values,
        };
        let candidate = materialize(&catalog, &self.schemas, &request)
            .map_err(|source| SessionRejection::Instantiation(Box::new(source)))?;

        let commands = instantiation_commands(spec, &scope, &candidate)?;
        self.apply_edit(&commands, gesture)
    }

    /// Replace the whole document, atomically.
    ///
    /// The three things that make this a lifecycle operation rather than an
    /// edit: the revision moves *forward* onto the running session's next one,
    /// so a file cannot rewind what a view has already seen; the history is
    /// cleared, because the opening undo of a session must not empty the
    /// workspace someone just opened; and the result is clean, because what is
    /// here is exactly what is on disk.
    fn replace(
        &mut self,
        experiment: Experiment,
        target: Option<DocumentTarget>,
        discard_unsaved: bool,
        opened: bool,
    ) -> Result<ExperimentChange, SessionRejection> {
        if self.is_dirty() && !discard_unsaved {
            return Err(SessionRejection::UnsavedChanges);
        }

        self.experiment = experiment.adopted_after(self.experiment.revision());
        self.history.clear();
        self.open_gesture = None;
        self.clean_revision = self.experiment.revision();
        self.acknowledged_revision = None;
        self.target = target;
        self.rebuild_capabilities();
        Ok(ExperimentChange::Replaced { opened })
    }

    fn open_gesture(&mut self) -> ExperimentChange {
        // `resolve_gesture` has already closed any previous one, because a
        // `BeginInteractiveEdit` never names a gesture.
        let gesture = GestureId::new(self.next_gesture);
        self.next_gesture += 1;
        self.open_gesture = Some(gesture);
        ExperimentChange::GestureOpened { gesture }
    }

    fn close_gesture(&mut self) -> Result<ExperimentChange, SessionRejection> {
        let gesture = self
            .open_gesture
            .take()
            .ok_or(SessionRejection::NoGestureOpen)?;
        Ok(ExperimentChange::GestureClosed {
            gesture,
            implicit: false,
        })
    }

    fn record(
        &mut self,
        envelope: ExperimentCommandEnvelope,
        change: ExperimentChange,
    ) -> Acceptance {
        let seq = self.push_event(
            Some(envelope.command_id.clone()),
            envelope.actor.clone(),
            change.clone(),
        );
        let acceptance = Acceptance {
            command_id: envelope.command_id.clone(),
            revision: self.experiment.revision(),
            change,
            event: seq,
            replayed: false,
        };
        let record = AcceptedRecord {
            envelope,
            acceptance: acceptance.clone(),
        };
        self.retained_commands += record.weight();
        self.accepted.push_back(record);
        while self.accepted.len() > MAX_COMMAND_HISTORY
            || self.retained_commands > MAX_REPLAY_COMMANDS
        {
            let Some(evicted) = self.accepted.pop_front() else {
                break;
            };
            self.retained_commands -= evicted.weight();
        }
        acceptance
    }

    fn push_event(
        &mut self,
        command_id: Option<CommandId>,
        actor: ActorId,
        change: ExperimentChange,
    ) -> EventSeq {
        self.next_event = self.next_event.next();
        self.events.push_back(ExperimentEvent {
            seq: self.next_event,
            revision: self.experiment.revision(),
            command_id,
            actor,
            change,
        });
        while self.events.len() > MAX_EVENT_HISTORY {
            self.events.pop_front();
        }
        self.next_event
    }

    /// Rebuild the capability projection from scratch.
    ///
    /// Needed whenever the contents were *replaced* rather than edited — a
    /// restore — or the registry changed. An ordinary edit uses
    /// [`CapabilityReport::retain_present`] instead; see [`Self::adopt_schemas`].
    fn rebuild_capabilities(&mut self) {
        self.capabilities = Arc::new(CapabilityReport::of(
            &self.experiment.snapshot(),
            &self.schemas,
        ));
    }

    /// Drop diagnostics for content an accepted edit removed.
    fn prune_capabilities(&mut self) {
        if self.capabilities.is_complete() {
            return;
        }
        let mut capabilities = (*self.capabilities).clone();
        capabilities.retain_present(&self.experiment.snapshot());
        self.capabilities = Arc::new(capabilities);
    }
}

/// Turn a materialised candidate into the document commands that commit it.
///
/// Two kinds of command, in one batch: the copied definitions become variable
/// definitions in the object's own scope, and the components become one
/// object. Going through the ordinary command path is what makes an
/// instantiated object validated, undoable and revisioned exactly like a
/// hand-authored one — and what stops this becoming a second way to put
/// objects in an experiment.
fn instantiation_commands(
    spec: &InstantiationSpec,
    scope: &Namespace,
    candidate: &ObjectCandidate,
) -> Result<Vec<ExperimentCommand>, SessionRejection> {
    let mut commands = Vec::with_capacity(candidate.definitions.len() + 1);
    for (name, definition) in &candidate.definitions {
        commands.push(ExperimentCommand::DefineVariable(Box::new(
            VariableSpec::new(name.clone(), definition.source.clone()).in_namespace(scope.clone()),
        )));
    }

    let mut object = ObjectSpec::new(spec.name.clone())
        .with_transform(spec.transform)
        .with_velocity(spec.velocity)
        .from_template(candidate.provenance.clone());
    for component in &candidate.components {
        let mut properties = ComponentProperties::new();
        for (property, value) in &component.properties {
            properties.insert(property.clone(), authored_value(value));
        }
        object = object.with_component(component.type_id.clone(), properties);
    }
    commands.push(ExperimentCommand::CreateObject(Box::new(object)));
    Ok(commands)
}

/// A materialised value as the authoring command that reproduces it.
///
/// The catalog already resolved these, but the document re-derives every
/// magnitude from the source it is given — that invariant is the reason a
/// value cannot enter an experiment except through validation, and an
/// instantiation is not an exception to it.
fn authored_value(value: &kagami_catalog::ObjectPropertyValue) -> AuthoredValue {
    match value {
        kagami_catalog::ObjectPropertyValue::Quantity { source, .. } => {
            AuthoredValue::si(source.clone())
        }
        kagami_catalog::ObjectPropertyValue::Boolean(value) => AuthoredValue::Boolean(*value),
        kagami_catalog::ObjectPropertyValue::Text(value) => AuthoredValue::Text(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use kagami_catalog::SchemaRegistry;
    use kagami_document::{DisplayName, ObjectSpec};

    use super::*;

    fn create(label: &str) -> ExperimentCommand {
        ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
            DisplayName::new(label).expect("valid label"),
        )))
    }

    fn submit(authority: &mut DocumentAuthority, id: usize, commands: Vec<ExperimentCommand>) {
        authority
            .submit(ExperimentCommandEnvelope::new(
                CommandId::new(format!("cmd-{id}")).expect("valid"),
                ActorId::new("agent").expect("valid"),
                SessionCommand::Edit(commands),
            ))
            .expect("accepted");
    }

    #[test]
    fn the_replay_window_is_bounded_by_submissions() {
        let mut authority = DocumentAuthority::new(SchemaRegistry::new(), Limits::DEFAULT);
        for index in 0..(MAX_COMMAND_HISTORY + 16) {
            submit(&mut authority, index, vec![create("o")]);
        }
        assert_eq!(authority.accepted.len(), MAX_COMMAND_HISTORY);
    }

    #[test]
    fn the_replay_window_is_also_bounded_by_the_commands_it_retains() {
        // Comparing a resubmission against the request that earned the cached
        // outcome means keeping the batch, and a batch may be large. Counting
        // submissions alone would let a caller pin
        // MAX_COMMAND_HISTORY * max_commands_per_batch commands in memory.
        let mut authority = DocumentAuthority::new(SchemaRegistry::new(), Limits::DEFAULT);
        let batch: Vec<_> = (0..64).map(|_| create("o")).collect();
        for index in 0..MAX_COMMAND_HISTORY {
            submit(&mut authority, index, batch.clone());
        }

        assert!(authority.accepted.len() < MAX_COMMAND_HISTORY);
        assert!(authority.retained_commands <= MAX_REPLAY_COMMANDS);
        assert_eq!(
            authority.retained_commands,
            authority
                .accepted
                .iter()
                .map(AcceptedRecord::weight)
                .sum::<usize>(),
            "the running total must agree with what is retained"
        );
    }
}
