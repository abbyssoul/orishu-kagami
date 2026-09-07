//! What an adapter submits, and the guards it submits with.
//!
//! An envelope is the *only* thing that changes an experiment. A UI gesture,
//! an MCP tool call, an undo, a redo, and (task K5) a catalog instantiation
//! all become one of these before anything is decided, which is what makes
//! ADR 0006's parity guarantee a property of the authority rather than a
//! promise each adapter has to keep.
//!
//! Three fields carry the guarantees the catalog authority already
//! established, so an adapter learns one pattern for both:
//!
//! - `expected_revision` makes a submission **guarded**;
//! - `command_id` makes it **idempotent**;
//! - `actor` makes it **attributed**.
//!
//! All of them, together with the gesture and the command, are what an
//! identity is compared against when a submission is resent: an envelope
//! identifies a *request*, not just a correlation string.

use std::collections::BTreeMap;

use kagami_catalog::{ContentFingerprint, ParameterName, TemplateIdentity};
use kagami_document::{
    DisplayName, Experiment, ExperimentCommand, ExperimentRevision, GestureId, Transform, Velocity,
};

use crate::identity::{ActorId, CommandId};
use crate::persist::DocumentTarget;

/// What an instantiation asks for: which template, and the placement the
/// template does not own.
///
/// The parameter bindings are authored expressions, so an override may itself
/// read a document variable or a public catalog binding. What the template
/// *does* own — its components and their values — is not repeated here.
#[derive(Clone, Debug, PartialEq)]
pub struct InstantiationSpec {
    /// Which template to materialise.
    pub template: TemplateIdentity,
    /// The content fingerprint the caller believes it is instantiating.
    ///
    /// A mismatch is a refusal, never a silent upgrade to content the caller
    /// never saw: what they picked in a browser may have been edited since.
    pub expected_fingerprint: Option<ContentFingerprint>,
    /// Parameter overrides, as authored expressions.
    pub bindings: BTreeMap<ParameterName, String>,
    /// What to call the new object.
    pub name: DisplayName,
    /// Where to put it.
    pub transform: Transform,
    /// How it starts moving.
    pub velocity: Velocity,
}

impl InstantiationSpec {
    /// Materialise `template` as an object called `name`, at the origin.
    pub fn new(template: TemplateIdentity, name: DisplayName) -> Self {
        Self {
            template,
            expected_fingerprint: None,
            bindings: BTreeMap::new(),
            name,
            transform: Transform::IDENTITY,
            velocity: Velocity::ZERO,
        }
    }

    /// Refuse the instantiation unless the template still hashes to this.
    #[must_use]
    pub fn expecting(mut self, fingerprint: ContentFingerprint) -> Self {
        self.expected_fingerprint = Some(fingerprint);
        self
    }

    /// Override one template parameter with an authored expression.
    #[must_use]
    pub fn binding(mut self, parameter: ParameterName, expression: impl Into<String>) -> Self {
        self.bindings.insert(parameter, expression.into());
        self
    }

    /// Place it.
    #[must_use]
    pub fn at(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Give it an initial velocity.
    #[must_use]
    pub fn moving(mut self, velocity: Velocity) -> Self {
        self.velocity = velocity;
        self
    }
}

/// One thing an adapter can ask the document authority to do.
///
/// Undo and redo are here rather than in an adapter's own stack because only
/// the authority can say what the experiment was and validate that it may be
/// restored. A client keeping its own history would be guessing, and would be
/// wrong the moment a second client shares the session.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionCommand {
    /// Apply a batch of authoring commands as one atomic edit.
    ///
    /// An empty batch is refused rather than accepted as a no-op: it would
    /// advance the revision and add an undo entry for an edit nobody made.
    Edit(Vec<ExperimentCommand>),
    /// Restore the experiment as it stood before the most recent edit.
    Undo,
    /// Reapply the most recently undone edit.
    Redo,
    /// Open an interactive edit — a viewport drag, or an inspector control
    /// being held — so every commit inside it becomes one undo entry.
    ///
    /// The accepted outcome carries the [`GestureId`] to put in the following
    /// submissions' [`ExperimentCommandEnvelope::gesture`].
    BeginInteractiveEdit,
    /// Replace the experiment with a new, empty one.
    ///
    /// An attributed authority operation, not a shell action: what is being
    /// discarded is the document, and only the authority knows whether it had
    /// unsaved changes.
    New {
        /// Whether the caller has decided to discard unsaved changes.
        ///
        /// Where a UI would show a dialog, an MCP caller states the answer
        /// (ADR 0006). The authority never resolves the question silently, in
        /// either direction.
        discard_unsaved: bool,
    },
    /// Replace the experiment with a decoded document.
    ///
    /// The candidate arrives as a value: reading and decoding a file is the
    /// shell's, and a successful read is not an acceptance.
    Open {
        /// The decoded experiment.
        experiment: Box<Experiment>,
        /// Where it was read from, once the read succeeded.
        target: Option<DocumentTarget>,
        /// Whether the caller has decided to discard unsaved changes.
        discard_unsaved: bool,
    },
    /// Materialise a catalog template into the experiment.
    ///
    /// Resolved against one immutable catalog snapshot the authority holds,
    /// then committed as an ordinary document edit. The resulting object is
    /// *self-contained* (ADR 0008): it carries the definitions its expressions
    /// need, copied local, and resolves with no catalog in scope. The template
    /// it came from is recorded as historical evidence and nothing more —
    /// there is no tracking link, no propagation, and no command that
    /// refreshes an object from its template.
    InstantiateObjectTemplate(Box<InstantiationSpec>),
    /// Close the open interactive edit.
    ///
    /// A gesture that commits nothing costs nothing. An adapter that fails to
    /// send this does not wedge the authority: any submission that does not
    /// name the open gesture closes it (see
    /// [`crate::DocumentAuthority::submit`]).
    EndInteractiveEdit,
}

impl SessionCommand {
    /// A short name for this submission, for a log or a queue inspector.
    ///
    /// Not the name of the resulting *edit* — that comes from the commands
    /// themselves, so an undo affordance and a history line agree.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Edit(_) => "edit",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::BeginInteractiveEdit => "begin_interactive_edit",
            Self::EndInteractiveEdit => "end_interactive_edit",
            Self::New { .. } => "new",
            Self::Open { .. } => "open",
            Self::InstantiateObjectTemplate(_) => "instantiate_object_template",
        }
    }

    /// `true` when this submission can change the experiment's contents.
    ///
    /// Opening and closing a gesture cannot: they bracket edits without being
    /// one, which is why they do not advance the revision.
    pub const fn changes_contents(&self) -> bool {
        matches!(
            self,
            Self::Edit(_)
                | Self::Undo
                | Self::Redo
                | Self::New { .. }
                | Self::Open { .. }
                | Self::InstantiateObjectTemplate(_)
        )
    }

    /// `true` when this submission would discard the current document.
    pub const fn replaces_document(&self) -> bool {
        matches!(self, Self::New { .. } | Self::Open { .. })
    }
}

/// A command together with the guards and attribution it was submitted with.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperimentCommandEnvelope {
    /// Identity of this submission, for idempotent replay.
    pub command_id: CommandId,
    /// Who submitted it.
    pub actor: ActorId,
    /// The revision the caller composed the command against.
    ///
    /// `None` opts out of the guard and applies to whatever is current. An
    /// adapter that read a view and is acting on what it saw should set it: a
    /// mismatch is then a refusal rather than an edit applied to an experiment
    /// the caller never saw.
    pub expected_revision: Option<ExperimentRevision>,
    /// The interactive edit this submission joins, if any.
    ///
    /// Naming a gesture that is not open is a refusal, not a silent
    /// downgrade — otherwise a stale identity from a finished drag would keep
    /// coalescing later edits into it.
    pub gesture: Option<GestureId>,
    /// The command itself.
    pub command: SessionCommand,
}

impl ExperimentCommandEnvelope {
    /// An unguarded envelope, outside any gesture.
    pub fn new(command_id: CommandId, actor: ActorId, command: SessionCommand) -> Self {
        Self {
            command_id,
            actor,
            expected_revision: None,
            gesture: None,
            command,
        }
    }

    /// Guard this submission with the revision the caller read.
    #[must_use]
    pub fn guarded_by(mut self, revision: ExperimentRevision) -> Self {
        self.expected_revision = Some(revision);
        self
    }

    /// Join this submission to an open interactive edit.
    #[must_use]
    pub fn within(mut self, gesture: GestureId) -> Self {
        self.gesture = Some(gesture);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(command: SessionCommand) -> ExperimentCommandEnvelope {
        ExperimentCommandEnvelope::new(
            CommandId::new("cmd-1").expect("valid"),
            ActorId::new("ui").expect("valid"),
            command,
        )
    }

    #[test]
    fn an_envelope_is_unguarded_and_ungestured_by_default() {
        let envelope = envelope(SessionCommand::Undo);
        assert_eq!(envelope.expected_revision, None);
        assert_eq!(envelope.gesture, None);
    }

    #[test]
    fn guards_and_gestures_are_added_by_the_builder() {
        let revision = ExperimentRevision::INITIAL.next();
        let gesture = GestureId::new(3);
        let envelope = envelope(SessionCommand::Edit(Vec::new()))
            .guarded_by(revision)
            .within(gesture);
        assert_eq!(envelope.expected_revision, Some(revision));
        assert_eq!(envelope.gesture, Some(gesture));
    }

    #[test]
    fn only_content_changing_commands_say_so() {
        assert!(SessionCommand::Edit(Vec::new()).changes_contents());
        assert!(SessionCommand::Undo.changes_contents());
        assert!(SessionCommand::Redo.changes_contents());
        assert!(!SessionCommand::BeginInteractiveEdit.changes_contents());
        assert!(!SessionCommand::EndInteractiveEdit.changes_contents());
    }
}
