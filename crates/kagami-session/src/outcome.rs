//! What the authority decided, and what a view can catch up on.
//!
//! An outcome reports the **authority's decision**, never transport delivery.
//! An adapter that received an [`Acceptance`] knows the experiment moved; one
//! that received a [`SessionRejection`] knows nothing changed at all.

use std::fmt;

use kagami_document::{CapabilitySummary, CommitReport, ExperimentRevision, GestureId, Rejection};
use thiserror::Error;

use crate::identity::{ActorId, CommandId};

/// A monotonic counter over published change events.
///
/// Separate from [`ExperimentRevision`] because not every accepted command
/// advances a revision: opening and closing an interactive edit are
/// observable changes to the session that leave the experiment's contents
/// exactly as they were. A view that caught up by revision alone would miss
/// them and could not tell whether a gesture is holding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSeq(u64);

impl EventSeq {
    /// Before any event has been published.
    pub const INITIAL: Self = Self(0);

    /// The next sequence number.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The underlying counter.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for EventSeq {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "e{}", self.0)
    }
}

/// What one accepted command did.
#[derive(Clone, Debug, PartialEq)]
pub enum ExperimentChange {
    /// A batch was applied as one new revision.
    Edited {
        /// What the batch created, removed, and is called.
        report: CommitReport,
    },
    /// A previously captured experiment was restored.
    ///
    /// The revision moved *forward*: a revision names a point in this
    /// experiment's history, and history does not run backwards.
    Undone {
        /// The name of the edit that was reversed.
        label: String,
    },
    /// A previously undone experiment was restored again.
    Redone {
        /// The name of the edit that was reapplied.
        label: String,
    },
    /// An interactive edit was opened. The contents did not change.
    GestureOpened {
        /// The identity to put in the following submissions.
        gesture: GestureId,
    },
    /// An interactive edit was closed. The contents did not change.
    GestureClosed {
        /// Which gesture ended.
        gesture: GestureId,
        /// `true` when the authority closed it because a submission did not
        /// name it, rather than because the adapter asked. An adapter that
        /// sees this dropped an `EndInteractiveEdit`.
        implicit: bool,
    },
    /// The installed component schemas were replaced, so what the experiment's
    /// existing content *means here* changed.
    ///
    /// The experiment did not change: no revision, no history entry, no dirty
    /// state. This is published anyway because a view that only watched the
    /// revision would keep showing an object as editable after the plugin
    /// governing it was uninstalled.
    CapabilityChanged {
        /// Gaps before the new registry was adopted.
        before: CapabilitySummary,
        /// Gaps after it.
        after: CapabilitySummary,
    },
}

impl ExperimentChange {
    /// A short name for this change.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Edited { .. } => "edited",
            Self::Undone { .. } => "undone",
            Self::Redone { .. } => "redone",
            Self::GestureOpened { .. } => "gesture_opened",
            Self::GestureClosed { .. } => "gesture_closed",
            Self::CapabilityChanged { .. } => "capability_changed",
        }
    }

    /// What a user would call this change.
    pub fn label(&self) -> &str {
        match self {
            Self::Edited { report } => &report.label,
            Self::Undone { label } | Self::Redone { label } => label,
            Self::GestureOpened { .. } => "Begin interactive edit",
            Self::GestureClosed { .. } => "End interactive edit",
            Self::CapabilityChanged { .. } => "Installed physics changed",
        }
    }
}

/// The authority accepted a command.
#[derive(Clone, Debug, PartialEq)]
pub struct Acceptance {
    /// The submission this answers.
    pub command_id: CommandId,
    /// The revision in force afterwards.
    ///
    /// Unchanged from before for a gesture bracket, which changes no contents.
    pub revision: ExperimentRevision,
    /// What happened.
    pub change: ExperimentChange,
    /// The event this appended, for a view catching up.
    pub event: EventSeq,
    /// `true` when this is a recorded outcome replayed for a resubmitted
    /// [`CommandId`] rather than a change applied now.
    ///
    /// The revision and change are the original ones, so a caller that
    /// ignores this field is still correct — it just cannot tell a retry from
    /// a first attempt, and sometimes wants to.
    pub replayed: bool,
}

/// One published change, retained so a view can catch up from where it was.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperimentEvent {
    /// This event's position in the published sequence.
    pub seq: EventSeq,
    /// The revision in force after the change.
    pub revision: ExperimentRevision,
    /// The submission that caused it, when one did.
    ///
    /// `None` for a change the authority made without being asked through an
    /// envelope — adopting a new schema registry is the only one. Modelled as
    /// an absence rather than a synthesised identity so that resubmitting
    /// *any* recorded identity replays a command the caller actually sent.
    pub command_id: Option<CommandId>,
    /// Who submitted it, or who supplied the change.
    pub actor: ActorId,
    /// What happened.
    pub change: ExperimentChange,
}

/// Why a command was refused.
///
/// A refusal changes nothing: the revision, the experiment, the history, and
/// the event log are exactly as they were, and the refused [`CommandId`] is
/// not recorded, so a retry after a transient transport error still runs.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SessionRejection {
    /// The experiment moved on since the caller read it.
    #[error("experiment is at {current}, not the expected {expected}")]
    RevisionConflict {
        /// The revision the caller composed against.
        expected: ExperimentRevision,
        /// The revision actually in force.
        current: ExperimentRevision,
    },
    /// A different request reused the identity of one that already succeeded.
    ///
    /// Replaying is only safe for the *same* request: an identity that named
    /// one accepted submission cannot be made to answer for another, or an
    /// adapter reusing a counter would receive an outcome describing a change
    /// it never asked for — possibly another actor's. Nothing about the
    /// recorded outcome is reported here, for the same reason.
    ///
    /// Retention is bounded, so an identity old enough to have been forgotten
    /// is a new request rather than a conflict.
    #[error("command identity `{command_id}` already named a different accepted submission")]
    CommandIdentityConflict {
        /// The identity that was reused.
        command_id: CommandId,
    },
    /// The submission named an interactive edit that is not open.
    ///
    /// Refused rather than treated as ungestured, because a stale identity
    /// from a finished drag would otherwise keep coalescing later edits into
    /// that drag's undo entry.
    #[error("interactive edit {gesture:?} is not open")]
    GestureNotOpen {
        /// The identity that was named.
        gesture: GestureId,
    },
    /// `EndInteractiveEdit` arrived with no interactive edit open.
    #[error("no interactive edit is open")]
    NoGestureOpen,
    /// An edit carried no commands.
    #[error("an edit must carry at least one command")]
    EmptyBatch,
    /// A preflight was asked about a command that has no batch to check.
    ///
    /// Whether undo is available, or a gesture may be opened, is a *read* —
    /// see [`crate::DocumentAuthority::history_status`]. Reporting that as a
    /// validation pass or failure would invite a caller to treat a preflight
    /// as a dry run of the whole submission, which it is not.
    #[error("`{kind}` has nothing to validate; read the session view instead")]
    NotPreflightable {
        /// The submission kind that was offered.
        kind: &'static str,
    },
    /// There is no edit to reverse.
    #[error("there is nothing to undo")]
    NothingToUndo,
    /// There is no reversed edit to reapply.
    #[error("there is nothing to redo")]
    NothingToRedo,
    /// The document core refused the edit.
    ///
    /// Boxed to keep this type small: it is the `Err` half of every
    /// submission, so its size is paid on the success path too.
    #[error(transparent)]
    Document(Box<Rejection>),
}

impl SessionRejection {
    /// A stable identifier for this reason.
    ///
    /// For a document refusal this is the underlying
    /// [`Rejection::code`], so an adapter branches on one flat vocabulary
    /// rather than unwrapping a layer first.
    pub fn code(&self) -> &'static str {
        match self {
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::CommandIdentityConflict { .. } => "command_identity_conflict",
            Self::GestureNotOpen { .. } => "gesture_not_open",
            Self::NoGestureOpen => "no_gesture_open",
            Self::EmptyBatch => "empty_batch",
            Self::NotPreflightable { .. } => "not_preflightable",
            Self::NothingToUndo => "nothing_to_undo",
            Self::NothingToRedo => "nothing_to_redo",
            Self::Document(rejection) => rejection.code(),
        }
    }
}

impl From<Rejection> for SessionRejection {
    fn from(value: Rejection) -> Self {
        Self::Document(Box::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_sequence_only_moves_forward() {
        let seq = EventSeq::INITIAL;
        assert_eq!(seq.get(), 0);
        assert!(seq.next() > seq);
        assert_eq!(seq.next().to_string(), "e1");
    }

    #[test]
    fn a_document_refusal_reports_the_underlying_code() {
        let rejection = SessionRejection::from(Rejection::BatchTooLarge { found: 3, limit: 2 });
        // Flat vocabulary: an adapter branches on the document's own code
        // without unwrapping a session layer first.
        assert_eq!(rejection.code(), "batch_too_large");
    }

    #[test]
    fn a_rejection_stays_small_enough_to_return_by_value() {
        // The `Err` half of every submission, so its size is paid on the
        // success path too. Adding a wide payload is fine; adding it unboxed
        // is not.
        assert!(
            size_of::<SessionRejection>() <= 64,
            "SessionRejection grew to {} bytes; box the new payload",
            size_of::<SessionRejection>()
        );
    }

    #[test]
    fn a_change_names_itself_for_a_log_and_for_a_user() {
        let change = ExperimentChange::Undone {
            label: "Move object".to_owned(),
        };
        assert_eq!(change.kind(), "undone");
        assert_eq!(change.label(), "Move object");
    }
}
