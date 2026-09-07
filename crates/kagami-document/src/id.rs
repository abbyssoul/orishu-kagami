//! Stable identities, and the counters that mint them.
//!
//! Two rules make these types load-bearing rather than decorative, and both
//! come from ADR 0019:
//!
//! 1. **A revision is a point in history, not a place to return to.** Undo
//!    restores captured contents *forwards*, producing a revision that never
//!    existed before, so `ExperimentRevision` only ever increases.
//! 2. **Counters are never rewound.** Undoing a creation frees no identifier,
//!    so no later object can inherit a removed predecessor's identity — and
//!    anything keyed by an identity (a selection, a probe attachment, a
//!    recorded series) cannot silently rebind to a different object.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Opaque, stable identity of one object in an experiment.
///
/// Minted only by [`Counters`], and deliberately without a `Default`: there is
/// no such thing as a default object, and a caller that could conjure one
/// would be able to hand a command an identity nothing ever allocated.
/// An adapter turns a raw wire or file value into one through
/// [`crate::ExperimentSnapshot::resolve_object`], which can only succeed for an
/// object that exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(u64);

impl ObjectId {
    /// Rebuild an identity from its counter value.
    ///
    /// Crate-private on purpose: the two callers are the counter that mints
    /// identities and the snapshot lookup that resolves an existing one.
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// The underlying counter value.
    ///
    /// For display and persistence only: an identity is meaningful within one
    /// experiment's history, and arithmetic on it is never meaningful.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "object-{}", self.0)
    }
}

/// A monotonic counter over accepted experiment changes.
///
/// Spelled like [`kagami_catalog::CatalogRevision`] on purpose — an adapter
/// that learns one guard learns both — but deliberately a distinct type:
/// a catalog reload advances the catalog revision and nothing else, and
/// neither counter can be substituted for the other.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ExperimentRevision(u64);

impl ExperimentRevision {
    /// The revision of a newly created experiment.
    pub const INITIAL: Self = Self(0);

    /// The next revision.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The underlying counter.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ExperimentRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "r{}", self.0)
    }
}

/// Monotonic identity allocation for one experiment.
///
/// Carried beside the experiment state rather than inside it, because
/// restoring a checkpoint replaces the state and must *not* replace these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counters {
    object: u64,
    variable: u64,
}

impl Counters {
    /// Allocate the next object identity.
    pub(crate) fn next_object(&mut self) -> ObjectId {
        let id = ObjectId::from_raw(self.object);
        self.object += 1;
        id
    }

    /// Rebuild the counters a persisted document recorded.
    ///
    /// Crate-private: the only caller is [`crate::hydrate`], which is the one
    /// place a document's own history is adopted. Everywhere else, counters
    /// only ever move forward by minting.
    pub(crate) const fn restored(objects: u64, variables: u64) -> Self {
        Self {
            object: objects,
            variable: variables,
        }
    }

    /// Turn a persisted raw value into an object identity, if these counters
    /// say it was ever allocated.
    ///
    /// The parse boundary for a *document's* identities. An identity beyond
    /// the counter was never minted by this experiment's history, so accepting
    /// it would let a file name an object that could later collide with one
    /// the session mints.
    pub(crate) const fn restored_object(&self, raw: u64) -> Option<ObjectId> {
        if raw < self.object {
            Some(ObjectId::from_raw(raw))
        } else {
            None
        }
    }

    /// Turn a persisted raw value into a variable identity, if these counters
    /// say it was ever allocated.
    pub(crate) const fn restored_variable(&self, raw: u64) -> Option<crate::variable::VariableId> {
        if raw < self.variable {
            Some(crate::variable::VariableId::from_raw(raw))
        } else {
            None
        }
    }

    /// Allocate the next variable identity.
    pub(crate) fn next_variable(&mut self) -> crate::variable::VariableId {
        let id = crate::variable::VariableId::from_raw(self.variable);
        self.variable += 1;
        id
    }

    /// How many object identities have been allocated over this experiment's
    /// whole history, including those since removed.
    pub const fn objects_minted(&self) -> u64 {
        self.object
    }

    /// How many variable identities have been allocated over this
    /// experiment's whole history, including those since removed.
    pub const fn variables_minted(&self) -> u64 {
        self.variable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_only_move_forward() {
        let revision = ExperimentRevision::INITIAL;
        assert_eq!(revision.get(), 0);
        assert_eq!(revision.next().get(), 1);
        assert!(revision.next() > revision);
    }

    #[test]
    fn revisions_render_like_a_catalog_revision() {
        assert_eq!(ExperimentRevision::INITIAL.next().to_string(), "r1");
    }

    #[test]
    fn object_identities_are_never_reused() {
        let mut counters = Counters::default();
        let first = counters.next_object();
        let second = counters.next_object();
        assert_ne!(first, second);
        assert_eq!(counters.objects_minted(), 2);

        // Removing an object is not modelled here, but the counter is what
        // guarantees the next identity cannot collide with a removed one.
        let third = counters.next_object();
        assert_ne!(third, first);
        assert_ne!(third, second);
    }

    #[test]
    fn object_identities_display_readably() {
        let mut counters = Counters::default();
        assert_eq!(counters.next_object().to_string(), "object-0");
    }
}
