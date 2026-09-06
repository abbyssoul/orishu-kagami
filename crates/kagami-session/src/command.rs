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

use kagami_document::{ExperimentCommand, ExperimentRevision, GestureId};

use crate::identity::{ActorId, CommandId};

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
        }
    }

    /// `true` when this submission can change the experiment's contents.
    ///
    /// Opening and closing a gesture cannot: they bracket edits without being
    /// one, which is why they do not advance the revision.
    pub const fn changes_contents(&self) -> bool {
        matches!(self, Self::Edit(_) | Self::Undo | Self::Redo)
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
