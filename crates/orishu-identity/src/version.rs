//! Formation-scoped version ordering.

use serde::{Deserialize, Serialize};

use crate::NodeId;

/// Version of one replicated entity, as carried by the peer protocol's
/// `VersionTuple`.
///
/// # Ordering
///
/// The derived ordering is lexicographic over `(epoch, counter, actor)`, and it
/// is *total*: any two versions compare, and only byte-identical tuples compare
/// equal. Including the actor breaks ties between two members that
/// independently reached the same `(epoch, counter)`, which is what makes the
/// merge in `orishu-membership` deterministic rather than
/// delivery-order-dependent.
///
/// Equality of versions is deliberately not equality of payloads. Merge treats
/// an equal version carrying an identical payload as an idempotent replay, and
/// an equal version carrying a *different* payload as a structured conflict —
/// never as a silent tie broken by whichever copy arrived last.
///
/// Versions are scoped to one formation. A version from another formation is
/// meaningless here and must be rejected by the formation guard before it ever
/// reaches a comparison.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionTuple {
    /// Formation epoch. Bumped by transitions that invalidate earlier
    /// counters, so a later epoch always wins regardless of counter.
    pub epoch: u64,
    /// Monotonic counter within an epoch.
    pub counter: u64,
    /// Member that produced this version. Breaks `(epoch, counter)` ties.
    pub actor: NodeId,
}

impl VersionTuple {
    /// First version an actor can publish in `epoch`.
    #[must_use]
    pub fn initial(epoch: u64, actor: NodeId) -> Self {
        Self {
            epoch,
            counter: 0,
            actor,
        }
    }

    /// Next version `actor` should publish after observing `self`.
    ///
    /// Returns `None` when the counter would overflow, so a caller must decide
    /// what to do rather than silently wrapping into the past. Wrapping is
    /// never correct: it would let a stale update outrank a current one.
    #[must_use]
    pub fn checked_successor(&self, actor: NodeId) -> Option<Self> {
        Some(Self {
            epoch: self.epoch,
            counter: self.counter.checked_add(1)?,
            actor,
        })
    }

    /// First version of the next epoch, used when a transition invalidates
    /// every counter in the current one.
    #[must_use]
    pub fn checked_next_epoch(&self, actor: NodeId) -> Option<Self> {
        Some(Self {
            epoch: self.epoch.checked_add(1)?,
            counter: 0,
            actor,
        })
    }
}

/// SWIM incarnation number of one member.
///
/// Only the member itself increments its incarnation, and only to refute a
/// suspicion. A higher incarnation therefore means "the subject has spoken more
/// recently than whatever you believe", which is what lets `Alive(n)` override
/// `Suspect(m)` for `n > m`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Incarnation(pub u64);

impl Incarnation {
    /// The incarnation a member starts with when it is first admitted.
    pub const INITIAL: Self = Self(0);

    /// The next incarnation, or `None` when the counter is exhausted.
    ///
    /// Exhaustion must not wrap: an incarnation of `0` after `u64::MAX` would
    /// make the node unable to ever refute suspicion again, and would let a
    /// replayed old `Suspect` outrank a fresh `Alive`.
    #[must_use]
    pub fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

impl std::fmt::Display for Incarnation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str) -> NodeId {
        NodeId::new(name).unwrap()
    }

    fn version(epoch: u64, counter: u64, name: &str) -> VersionTuple {
        VersionTuple {
            epoch,
            counter,
            actor: actor(name),
        }
    }

    #[test]
    fn epoch_dominates_counter() {
        assert!(version(2, 0, "a") > version(1, u64::MAX, "a"));
    }

    #[test]
    fn actor_breaks_ties_totally() {
        assert!(version(1, 1, "node-a") < version(1, 1, "node-b"));
        assert_eq!(version(1, 1, "node-a"), version(1, 1, "node-a"));
    }

    #[test]
    fn successor_is_strictly_greater() {
        let base = version(1, 7, "node-a");
        let next = base.checked_successor(actor("node-a")).unwrap();
        assert!(next > base);
        assert_eq!(next.counter, 8);
    }

    #[test]
    fn successor_refuses_to_wrap() {
        assert_eq!(
            version(1, u64::MAX, "a").checked_successor(actor("a")),
            None
        );
        assert_eq!(
            version(u64::MAX, 0, "a").checked_next_epoch(actor("a")),
            None
        );
    }

    #[test]
    fn incarnation_refuses_to_wrap() {
        assert_eq!(Incarnation(u64::MAX).checked_next(), None);
        assert_eq!(Incarnation::INITIAL.checked_next(), Some(Incarnation(1)));
    }
}
