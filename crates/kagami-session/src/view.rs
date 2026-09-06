//! What an adapter reads.
//!
//! A view is a value, taken at one revision. It carries no mutable access to
//! anything, so a renderer holding one while the user keeps editing is
//! showing a coherent past revision rather than a half-applied present.
//!
//! Deliberately *not* a parallel copy of the experiment. The model already has
//! an immutable, revision-carrying read type
//! ([`kagami_document::ExperimentSnapshot`]); rebuilding its contents here
//! would be a shallow module whose only job is to drift out of sync. What this
//! module adds is the session-level state the model does not have: whether
//! there is anything to undo, whether the document has unsaved changes, and
//! whether an interactive edit is holding.

use std::sync::Arc;

use kagami_document::{CapabilityReport, ExperimentRevision, ExperimentSnapshot, GestureId};

use crate::outcome::EventSeq;
use crate::persist::DocumentTarget;

/// Whether undo and redo are available, and what each would do.
///
/// The labels exist so an affordance can say what it will reverse instead of
/// offering an unlabelled arrow — and so an MCP caller can make an intentional
/// history operation rather than a hopeful one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryStatus {
    /// The name of the edit undo would reverse.
    pub undo: Option<String>,
    /// The name of the edit redo would reapply.
    pub redo: Option<String>,
    /// How many entries undo currently holds.
    pub depth: usize,
    /// The retention bound. Beyond this, the oldest entry is dropped.
    pub capacity: usize,
}

impl HistoryStatus {
    /// `true` when there is an edit to reverse.
    pub fn can_undo(&self) -> bool {
        self.undo.is_some()
    }

    /// `true` when there is a reversed edit to reapply.
    pub fn can_redo(&self) -> bool {
        self.redo.is_some()
    }
}

/// The session as an adapter sees it at one revision.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionView {
    /// The experiment's contents, and the revision they are.
    pub experiment: ExperimentSnapshot,
    /// Which of those contents the installed schemas can govern, and why not
    /// for the rest.
    ///
    /// Diagnostics only: no schema definitions are copied here, because the
    /// registry is not experiment state and two descriptions of what can be
    /// attached could disagree. A caller that needs the declarations reads
    /// [`crate::DocumentAuthority::schemas`].
    ///
    /// Shared rather than cloned: an adapter takes a view every frame, and
    /// most frames the projection has not changed.
    pub capabilities: Arc<CapabilityReport>,
    /// Undo and redo availability.
    pub history: HistoryStatus,
    /// `true` when the experiment has advanced past the revision last
    /// persisted.
    ///
    /// Derived, not a flag an adapter sets: a boolean somebody has to remember
    /// to clear is a boolean that will be wrong after a rejected save.
    pub dirty: bool,
    /// Where the session is saved, once a write has succeeded. `None` for a
    /// document that has never been written.
    pub target: Option<DocumentTarget>,
    /// The interactive edit currently holding, if any.
    pub open_gesture: Option<GestureId>,
    /// The most recent published event. A view that stores this can ask for
    /// everything since, instead of re-reading the whole experiment.
    pub last_event: EventSeq,
}

impl SessionView {
    /// The revision these contents are.
    pub fn revision(&self) -> ExperimentRevision {
        self.experiment.revision()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_history_offers_neither_direction() {
        let status = HistoryStatus::default();
        assert!(!status.can_undo());
        assert!(!status.can_redo());
    }

    #[test]
    fn availability_follows_the_labels() {
        let status = HistoryStatus {
            undo: Some("Add object".to_owned()),
            redo: None,
            depth: 1,
            capacity: 128,
        };
        assert!(status.can_undo());
        assert!(!status.can_redo());
    }
}
