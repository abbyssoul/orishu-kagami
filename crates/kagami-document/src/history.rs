//! Undo and redo, as captured experiments rather than inverse commands.
//!
//! # Why an entry is a whole experiment
//!
//! The obvious implementation — store the inverse of each command — does not
//! work here. Undoing a removal has to put the object back *with the identity
//! it had*, and a creation mints a fresh one, so every consumer keyed by
//! identity would rebind to the wrong thing. It would also need a new inverse
//! for every command a plugin's schema makes possible later, and getting one
//! wrong produces an experiment that never existed rather than an error.
//!
//! Capturing is affordable because an experiment is a shared pointer: an entry
//! costs a pointer and a label, and experiments an edit did not change are
//! shared between entries. The cost is proportional to the number of
//! *distinct* experiments reachable through the stack, not to the size of the
//! edits — and the stack is bounded regardless.
//!
//! # What this type does not do
//!
//! It does not hold the current experiment, and it never restores anything
//! itself. [`EditHistory::undo`] hands back captured contents for the caller
//! to validate and adopt as a *new* revision, and takes the current contents
//! so they can be redone. A refusal to adopt therefore costs nothing, because
//! by then the history has already been updated — which is why the caller
//! validates *before* it calls, or restores the history it saved.

use std::collections::VecDeque;

use crate::model::ExperimentCheckpoint;

/// Identifies one interactive edit — a viewport drag, or an inspector control
/// being held — so every commit inside it joins one history entry.
///
/// Minted by whoever owns the gesture's lifetime (the document server), not
/// here: this type only needs to compare two of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GestureId(u64);

impl GestureId {
    /// Build a gesture identity from its counter value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The underlying counter.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Contents to restore, and the name of the edit that restoring reverses.
#[derive(Clone, Debug, PartialEq)]
pub struct Restoration {
    /// The captured contents.
    pub checkpoint: ExperimentCheckpoint,
    /// What the user called the edit being reversed.
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    checkpoint: ExperimentCheckpoint,
    label: String,
    gesture: Option<GestureId>,
}

/// A bounded stack of captured experiments.
#[derive(Clone, Debug, PartialEq)]
pub struct EditHistory {
    undo: VecDeque<Entry>,
    redo: VecDeque<Entry>,
    depth: usize,
}

impl EditHistory {
    /// A history retaining at most `depth` entries on each side.
    ///
    /// A `depth` of zero is clamped to one: a history that cannot hold an
    /// entry is not a configuration anyone wants, and silently accepting it
    /// would make undo appear broken rather than disabled.
    pub fn new(depth: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            depth: depth.max(1),
        }
    }

    /// Record the contents an edit is about to replace.
    ///
    /// `checkpoint` is the experiment *before* the edit and `label` names the
    /// edit, so undoing reports "what you are reversing" rather than "what you
    /// will get".
    ///
    /// When `gesture` matches the entry already on top, nothing is recorded:
    /// the open gesture's first commit already captured where it started, and
    /// the hundred that follow it are the same edit. Recording also discards
    /// the redo stack, because an edit made after an undo creates a branch and
    /// the abandoned one is not reachable.
    pub fn record(
        &mut self,
        checkpoint: ExperimentCheckpoint,
        label: impl Into<String>,
        gesture: Option<GestureId>,
    ) {
        if let (Some(gesture), Some(top)) = (gesture, self.undo.back())
            && top.gesture == Some(gesture)
        {
            return;
        }
        self.redo.clear();
        self.undo.push_back(Entry {
            checkpoint,
            label: label.into(),
            gesture,
        });
        while self.undo.len() > self.depth {
            self.undo.pop_front();
        }
    }

    /// Take the most recent captured contents, banking `current` for redo.
    ///
    /// Returns `None` when there is nothing to undo, having changed nothing.
    pub fn undo(&mut self, current: ExperimentCheckpoint) -> Option<Restoration> {
        let entry = self.undo.pop_back()?;
        self.redo.push_back(Entry {
            checkpoint: current,
            label: entry.label.clone(),
            gesture: entry.gesture,
        });
        while self.redo.len() > self.depth {
            self.redo.pop_front();
        }
        Some(Restoration {
            checkpoint: entry.checkpoint,
            label: entry.label,
        })
    }

    /// Take the most recently undone contents, banking `current` for undo.
    ///
    /// Returns `None` when there is nothing to redo, having changed nothing.
    pub fn redo(&mut self, current: ExperimentCheckpoint) -> Option<Restoration> {
        let entry = self.redo.pop_back()?;
        self.undo.push_back(Entry {
            checkpoint: current,
            label: entry.label.clone(),
            gesture: entry.gesture,
        });
        while self.undo.len() > self.depth {
            self.undo.pop_front();
        }
        Some(Restoration {
            checkpoint: entry.checkpoint,
            label: entry.label,
        })
    }

    /// `true` when there is an edit to reverse.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// `true` when there is a reversed edit to reapply.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The name of the edit undo would reverse.
    ///
    /// So an affordance can say what it will do instead of offering an
    /// unlabelled arrow.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|entry| entry.label.as_str())
    }

    /// The name of the edit redo would reapply.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.back().map(|entry| entry.label.as_str())
    }

    /// How many entries undo currently holds.
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// `true` when there is nothing to undo.
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// The retention bound.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Forget everything.
    ///
    /// Session setup — the empty starting experiment, later a loaded file —
    /// authors through the same command path as any edit, which is what keeps
    /// validation uniform, and then calls this. The opening undo of a session
    /// emptying the workspace is not a feature.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::new(crate::Limits::DEFAULT.max_undo_depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Experiment;

    fn checkpoint() -> ExperimentCheckpoint {
        Experiment::new().checkpoint()
    }

    #[test]
    fn a_fresh_history_offers_nothing() {
        let history = EditHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_label(), None);
        assert_eq!(history.redo_label(), None);
        assert!(history.is_empty());
    }

    #[test]
    fn an_entry_is_labelled_with_the_edit_it_reverses() {
        let mut history = EditHistory::default();
        history.record(checkpoint(), "Add object", None);
        assert_eq!(history.undo_label(), Some("Add object"));
        let restored = history.undo(checkpoint()).expect("one entry");
        assert_eq!(restored.label, "Add object");
        assert_eq!(history.redo_label(), Some("Add object"));
    }

    #[test]
    fn every_commit_inside_one_gesture_is_one_entry() {
        let mut history = EditHistory::default();
        let gesture = GestureId::new(7);
        for _ in 0..100 {
            history.record(checkpoint(), "Move object", Some(gesture));
        }
        assert_eq!(history.len(), 1);

        // A different gesture is a different edit.
        history.record(checkpoint(), "Move object", Some(GestureId::new(8)));
        assert_eq!(history.len(), 2);

        // And an unbracketed edit never coalesces with anything.
        history.record(checkpoint(), "Rename object", None);
        history.record(checkpoint(), "Rename object", None);
        assert_eq!(history.len(), 4);
    }

    #[test]
    fn recording_after_an_undo_discards_the_abandoned_branch() {
        let mut history = EditHistory::default();
        history.record(checkpoint(), "Add object", None);
        history.undo(checkpoint()).expect("one entry");
        assert!(history.can_redo());

        history.record(checkpoint(), "Remove object", None);
        assert!(!history.can_redo());
    }

    #[test]
    fn undo_and_redo_walk_back_and_forth() {
        let mut history = EditHistory::default();
        history.record(checkpoint(), "first", None);
        history.record(checkpoint(), "second", None);

        assert_eq!(history.undo(checkpoint()).expect("entry").label, "second");
        assert_eq!(history.undo(checkpoint()).expect("entry").label, "first");
        assert!(!history.can_undo());
        assert!(history.undo(checkpoint()).is_none());

        assert_eq!(history.redo(checkpoint()).expect("entry").label, "first");
        assert_eq!(history.redo(checkpoint()).expect("entry").label, "second");
        assert!(!history.can_redo());
        assert!(history.redo(checkpoint()).is_none());
    }

    #[test]
    fn the_stack_is_bounded_and_drops_the_oldest_entry() {
        let mut history = EditHistory::new(3);
        for index in 0..10 {
            history.record(checkpoint(), format!("edit {index}"), None);
        }
        assert_eq!(history.len(), 3);
        assert_eq!(history.undo_label(), Some("edit 9"));
    }

    #[test]
    fn a_zero_depth_history_still_holds_one_entry() {
        let mut history = EditHistory::new(0);
        assert_eq!(history.depth(), 1);
        history.record(checkpoint(), "only", None);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn clearing_forgets_both_directions() {
        let mut history = EditHistory::default();
        history.record(checkpoint(), "first", None);
        history.undo(checkpoint()).expect("entry");
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
