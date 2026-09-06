//! The authoritative experiment, and the immutable views of it.
//!
//! # Why the state is shared, not copied
//!
//! Every accepted edit produces a *new* experiment rather than mutating one,
//! and undo captures whole experiments rather than inverse commands. Both are
//! only affordable because the state is a shared pointer plus a handful of
//! counters: cloning an [`Experiment`] is a few refcount bumps, and a commit
//! that touches only the object map shares the setup with the revision it
//! started from. A checkpoint therefore costs a pointer and a label, which is
//! what makes ADR 0019's "an undo entry is a captured experiment" a cheap
//! decision instead of a memory problem.
//!
//! # Why reads go through a snapshot
//!
//! [`ExperimentSnapshot`] carries the revision it describes, so no reader can
//! observe half of an edit and two views reporting the same revision always
//! describe identical state. A caller that wants to *read* takes a snapshot; a
//! caller that wants to *restore* takes an [`ExperimentCheckpoint`], which is
//! opaque precisely so it cannot become a second way to read.

use std::collections::BTreeMap;
use std::sync::Arc;

use kagami_catalog::ComponentTypeId;

use crate::id::{Counters, ExperimentRevision, ObjectId};
use crate::object::{Object, ObjectComponent};
use crate::setup::Setup;

/// The contents of an experiment at one revision.
///
/// Each collection is separately shared so a commit clones only what it
/// writes. Private to the crate: outside it, contents are reached through
/// [`ExperimentSnapshot`], which names the revision they belong to.
///
/// Deliberately *not* serialisable. This layout exists to make a commit cheap,
/// and a `serde` derive on it would publish that incidental shape as a format
/// with no versioning policy behind it. [`crate::wire`] owns the explicit
/// representation an adapter encodes, and task K4 owns the experiment file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExperimentState {
    pub(crate) revision: ExperimentRevision,
    pub(crate) objects: Arc<BTreeMap<ObjectId, Object>>,
    pub(crate) setup: Arc<Setup>,
}

impl ExperimentState {
    fn empty() -> Self {
        Self {
            revision: ExperimentRevision::INITIAL,
            objects: Arc::new(BTreeMap::new()),
            setup: Arc::new(Setup::default()),
        }
    }
}

/// An experiment: its contents at one revision, plus the identity allocation
/// that spans its whole history.
///
/// The counters live here rather than in the state because restoring a
/// checkpoint replaces the contents and must *not* replace these — that is
/// what stops a re-created object from inheriting a removed one's identity.
#[derive(Clone, Debug, PartialEq)]
pub struct Experiment {
    pub(crate) state: Arc<ExperimentState>,
    pub(crate) counters: Counters,
}

impl Experiment {
    /// A new, empty experiment at [`ExperimentRevision::INITIAL`].
    pub fn new() -> Self {
        Self {
            state: Arc::new(ExperimentState::empty()),
            counters: Counters::default(),
        }
    }

    /// The revision these contents are.
    pub fn revision(&self) -> ExperimentRevision {
        self.state.revision
    }

    /// An immutable view for reading.
    pub fn snapshot(&self) -> ExperimentSnapshot {
        ExperimentSnapshot(Arc::clone(&self.state))
    }

    /// Capture these contents for later restoration.
    ///
    /// A pointer copy. The result is provenance, not a place to return to:
    /// restoring it produces a later revision.
    pub fn checkpoint(&self) -> ExperimentCheckpoint {
        ExperimentCheckpoint(Arc::clone(&self.state))
    }

    /// How many identities of each kind this experiment has ever allocated.
    pub fn counters(&self) -> Counters {
        self.counters
    }
}

impl Default for Experiment {
    fn default() -> Self {
        Self::new()
    }
}

/// An immutable view of an experiment at one revision.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperimentSnapshot(Arc<ExperimentState>);

impl ExperimentSnapshot {
    /// The revision this view describes.
    pub fn revision(&self) -> ExperimentRevision {
        self.0.revision
    }

    /// Every object, in identity order.
    pub fn objects(&self) -> &BTreeMap<ObjectId, Object> {
        &self.0.objects
    }

    /// One object, if it exists.
    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.0.objects.get(&id)
    }

    /// Turn a raw identity — one an MCP client sent, or a persisted document
    /// carried — into an [`ObjectId`] that names an object in this view.
    ///
    /// This is the parse boundary for identities. [`ObjectId`] has no public
    /// constructor precisely so an adapter cannot invent one: an identity in
    /// hand always named a real object at *some* revision. That is not the
    /// same as naming one now, which is why a command that carries a stale
    /// identity is still refused with
    /// [`crate::Rejection::UnknownObject`] — the object may have been removed
    /// between the read and the submission.
    pub fn resolve_object(&self, raw: u64) -> Option<ObjectId> {
        let candidate = ObjectId::from_raw(raw);
        self.0.objects.contains_key(&candidate).then_some(candidate)
    }

    /// How many objects the experiment holds.
    pub fn object_count(&self) -> usize {
        self.0.objects.len()
    }

    /// The numerical setup and plugin composition.
    pub fn setup(&self) -> &Setup {
        &self.0.setup
    }

    /// Every object carrying `component`, with that component's values.
    ///
    /// A filter over all objects rather than an indexed query: the storage
    /// model stays an ordered map until a measured access pattern justifies
    /// otherwise, and this accessor exists so call sites do not have to change
    /// when it does.
    pub fn objects_with<'a>(
        &'a self,
        component: &'a ComponentTypeId,
    ) -> impl Iterator<Item = (ObjectId, &'a Object, &'a ObjectComponent)> {
        self.0
            .objects
            .iter()
            .filter_map(move |(id, object)| Some((*id, object, object.components.get(component)?)))
    }
}

/// Experiment contents captured at one revision, for later restoration.
///
/// Opaque on purpose: it is a thing to hand back to a restore, not a second
/// way to read an experiment. Reading goes through [`ExperimentSnapshot`],
/// which carries the revision that identifies what is being read.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperimentCheckpoint(pub(crate) Arc<ExperimentState>);

impl ExperimentCheckpoint {
    /// The revision these contents were captured at.
    ///
    /// Provenance, not identity: restoring them produces a *new*, later
    /// revision, because a revision names a point in this experiment's
    /// history and history does not run backwards.
    pub fn captured_at(&self) -> ExperimentRevision {
        self.0.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_experiment_is_empty_at_the_initial_revision() {
        let experiment = Experiment::new();
        assert_eq!(experiment.revision(), ExperimentRevision::INITIAL);
        assert_eq!(experiment.snapshot().object_count(), 0);
        assert_eq!(experiment.counters().objects_minted(), 0);
    }

    #[test]
    fn a_checkpoint_reports_where_it_came_from_but_nothing_else() {
        let experiment = Experiment::new();
        let checkpoint = experiment.checkpoint();
        assert_eq!(checkpoint.captured_at(), experiment.revision());
    }

    #[test]
    fn cloning_an_experiment_shares_its_contents() {
        let experiment = Experiment::new();
        let clone = experiment.clone();
        assert!(Arc::ptr_eq(&experiment.state, &clone.state));
        assert_eq!(experiment, clone);
    }

    #[test]
    fn a_snapshot_pins_the_revision_it_was_taken_at() {
        let experiment = Experiment::new();
        let snapshot = experiment.snapshot();
        assert_eq!(snapshot.revision(), ExperimentRevision::INITIAL);
    }
}
