//! Gossip deltas and the bounded retirement queue that disseminates them.
//!
//! A delta is one replicated entity at one version. Deltas reach peers by
//! being piggybacked onto whatever message is going out anyway — a probe, a
//! probe reply, or later a workload message — which is what keeps
//! dissemination `O(1)` messages per node per period regardless of formation
//! size.

use std::collections::BTreeMap;

use orishu_identity::{MembershipTombstone, NodeId, VersionTuple};
use serde::{Deserialize, Serialize};

use crate::{
    limits::Limits,
    model::{BlocklistEntry, BlocklistKey, Member},
};

/// Bytes of a delta the membership core does not own.
///
/// Kept opaque on purpose. Decoding a `WorkloadUpdate` or a checkpoint record
/// here would put its schema, its bounds, and its bugs inside membership's
/// blast radius; carrying the bytes across costs nothing and leaves ownership
/// where it belongs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaquePayload(pub Vec<u8>);

/// A gossip delta for a subsystem other than membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignDelta {
    /// The wire `deltaType`, for example `WorkloadUpdate`.
    pub delta_type: String,
    /// The wire `key`, identifying the entity within its own subsystem.
    pub key: String,
    /// The wire `version`, so the owning subsystem can order it.
    pub version: VersionTuple,
    /// The wire `data`, undecoded.
    pub payload: OpaquePayload,
}

/// What one delta carries.
///
/// The three membership-owned variants serialize with exactly the
/// `deltaType` discriminators the peer protocol names. `Foreign` is a
/// core-internal tag: the decoder maps any *other* `deltaType` onto it,
/// preserving the original string in [`ForeignDelta::delta_type`], and
/// restores that string when encoding. Membership therefore never has to know
/// the set of delta types another subsystem might add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", tag = "deltaType", content = "data")]
pub enum DeltaBody {
    /// A member record.
    MembershipUpdate(Member),
    /// A membership tombstone.
    TombstoneUpdate(MembershipTombstone),
    /// A blocklist entry.
    BlocklistUpdate(BlocklistEntry),
    /// Something membership does not own.
    Foreign(ForeignDelta),
}

impl DeltaBody {
    /// The version this delta claims.
    #[must_use]
    pub fn version(&self) -> &VersionTuple {
        match self {
            Self::MembershipUpdate(member) => &member.version,
            Self::TombstoneUpdate(tombstone) => &tombstone.version,
            Self::BlocklistUpdate(entry) => &entry.version,
            Self::Foreign(delta) => &delta.version,
        }
    }

    /// The queue key this delta occupies, or `None` for foreign deltas, which
    /// membership relays but never queues.
    #[must_use]
    pub fn key(&self) -> Option<GossipKey> {
        match self {
            Self::MembershipUpdate(member) => Some(GossipKey::Member(member.id.clone())),
            Self::TombstoneUpdate(tombstone) => {
                Some(GossipKey::Tombstone(tombstone.node_id.clone()))
            }
            Self::BlocklistUpdate(entry) => Some(GossipKey::Blocklist(entry.key.clone())),
            Self::Foreign(_) => None,
        }
    }

    /// A short identifier for diagnostics.
    #[must_use]
    pub fn entity(&self) -> String {
        match self {
            Self::MembershipUpdate(member) => format!("member:{}", member.id),
            Self::TombstoneUpdate(tombstone) => format!("tombstone:{}", tombstone.node_id),
            Self::BlocklistUpdate(entry) => format!("blocklist:{}", entry.key),
            Self::Foreign(delta) => format!("{}:{}", delta.delta_type, delta.key),
        }
    }
}

/// One gossip delta as it appears on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GossipDelta {
    /// Times this delta has been piggybacked. Starts at `0`.
    pub hops: u32,
    /// The delta itself.
    #[serde(flatten)]
    pub body: DeltaBody,
}

/// The entity a queued delta is about.
///
/// Keying by entity rather than by delta means a newer version of the same
/// entity replaces the older one in the queue instead of both being sent,
/// which is the difference between a queue that converges and one that grows.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GossipKey {
    /// A member record.
    Member(NodeId),
    /// A membership tombstone.
    Tombstone(NodeId),
    /// A blocklist entry.
    Blocklist(BlocklistKey),
}

/// A bounded queue of deltas awaiting dissemination.
///
/// Selection order is `(hops, version descending, key)`, which sends the
/// least-travelled news first — the protocol's stated priority — and breaks
/// every tie deterministically so two nodes with identical queues piggyback
/// identical deltas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GossipQueue {
    entries: BTreeMap<GossipKey, QueuedDelta>,
}

/// A delta waiting in the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedDelta {
    hops: u32,
    body: DeltaBody,
}

impl GossipQueue {
    /// Number of deltas queued.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Deltas currently queued, in key order, for inspection and tests.
    pub fn iter(&self) -> impl Iterator<Item = (&GossipKey, u32, &DeltaBody)> {
        self.entries
            .iter()
            .map(|(key, queued)| (key, queued.hops, &queued.body))
    }

    /// Queues `body` for dissemination, restarting its hop count.
    ///
    /// A newer version of an entity supersedes the queued one; an older one is
    /// ignored, so re-queuing on every merge is safe. Foreign deltas are never
    /// queued and are silently ignored here — they reach their owner through
    /// [`crate::Transition::foreign_gossip`], not through membership's queue.
    ///
    /// When the queue is full, the delta closest to retirement (highest hop
    /// count) is evicted to make room, because it is the one whose remaining
    /// dissemination value is lowest.
    pub fn enqueue(&mut self, body: DeltaBody, limits: &Limits) {
        let Some(key) = body.key() else {
            return;
        };

        if let Some(existing) = self.entries.get(&key)
            && existing.body.version() >= body.version()
            && existing.body == body
        {
            // Exact replay of something already queued: restarting its hops
            // would let a duplicate keep the queue slot alive forever.
            return;
        }
        if let Some(existing) = self.entries.get(&key)
            && existing.body.version() > body.version()
        {
            return;
        }

        if !self.entries.contains_key(&key) && self.entries.len() >= limits.max_gossip_queue() {
            self.evict_one();
        }

        self.entries.insert(key, QueuedDelta { hops: 0, body });
    }

    /// Removes the delta with the highest hop count, breaking ties by key so
    /// eviction is deterministic.
    fn evict_one(&mut self) {
        let victim = self
            .entries
            .iter()
            .max_by(|(left_key, left), (right_key, right)| {
                left.hops
                    .cmp(&right.hops)
                    .then_with(|| left_key.cmp(right_key))
            })
            .map(|(key, _)| key.clone());
        if let Some(key) = victim {
            self.entries.remove(&key);
        }
    }

    /// Takes up to `count` deltas to piggyback, incrementing their hop counts
    /// and retiring any that reach `max_hops`.
    ///
    /// Retirement is what bounds the queue's lifetime: without it a delta
    /// would be re-sent forever, and dissemination cost would grow with the
    /// formation's history rather than with its news.
    pub fn take(&mut self, count: usize, max_hops: u32) -> Vec<GossipDelta> {
        if count == 0 || self.entries.is_empty() {
            return Vec::new();
        }

        let mut candidates: Vec<(&GossipKey, &QueuedDelta)> = self.entries.iter().collect();
        candidates.sort_by(|(left_key, left), (right_key, right)| {
            left.hops
                .cmp(&right.hops)
                .then_with(|| right.body.version().cmp(left.body.version()))
                .then_with(|| left_key.cmp(right_key))
        });

        let chosen: Vec<GossipKey> = candidates
            .into_iter()
            .take(count)
            .map(|(key, _)| key.clone())
            .collect();

        let mut taken = Vec::with_capacity(chosen.len());
        let mut retired = Vec::new();
        for key in chosen {
            let Some(queued) = self.entries.get_mut(&key) else {
                continue;
            };
            queued.hops = queued.hops.saturating_add(1);
            taken.push(GossipDelta {
                hops: queued.hops,
                body: queued.body.clone(),
            });
            if queued.hops >= max_hops {
                retired.push(key);
            }
        }
        for key in retired {
            self.entries.remove(&key);
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        limits::LimitsSpec,
        model::{BlocklistAction, Liveness},
        testing,
    };

    fn queue_with(count: usize) -> (GossipQueue, Limits) {
        let limits = Limits::default();
        let mut queue = GossipQueue::default();
        for index in 0..count {
            queue.enqueue(
                DeltaBody::MembershipUpdate(testing::member(&format!("node-{index:04}"), 1)),
                &limits,
            );
        }
        (queue, limits)
    }

    #[test]
    fn a_newer_version_supersedes_a_queued_one() {
        let (mut queue, limits) = queue_with(0);
        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member("node-a", 1)),
            &limits,
        );
        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member("node-a", 5)),
            &limits,
        );
        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue.iter().next().unwrap().2.version().counter,
            5,
            "the newer version must win"
        );
    }

    #[test]
    fn an_older_version_does_not_displace_a_queued_one() {
        let (mut queue, limits) = queue_with(0);
        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member("node-a", 5)),
            &limits,
        );
        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member("node-a", 2)),
            &limits,
        );
        assert_eq!(queue.iter().next().unwrap().2.version().counter, 5);
    }

    #[test]
    fn exact_replay_does_not_reset_hops() {
        let (mut queue, limits) = queue_with(0);
        let member = testing::member("node-a", 1);
        queue.enqueue(DeltaBody::MembershipUpdate(member.clone()), &limits);
        assert_eq!(queue.take(1, 10).len(), 1);
        assert_eq!(queue.iter().next().unwrap().1, 1, "hop count advanced");

        queue.enqueue(DeltaBody::MembershipUpdate(member), &limits);
        assert_eq!(
            queue.iter().next().unwrap().1,
            1,
            "an identical replay must not restart dissemination"
        );
    }

    #[test]
    fn a_changed_payload_at_the_same_version_does_restart_hops() {
        // The queue is a dissemination buffer, not the conflict detector; the
        // merge path decides whether the change is legitimate.
        let (mut queue, limits) = queue_with(0);
        let mut member = testing::member("node-a", 1);
        queue.enqueue(DeltaBody::MembershipUpdate(member.clone()), &limits);
        queue.take(1, 10);

        member.liveness = Liveness::Suspected;
        queue.enqueue(DeltaBody::MembershipUpdate(member), &limits);
        assert_eq!(queue.iter().next().unwrap().1, 0);
    }

    #[test]
    fn foreign_deltas_are_never_queued() {
        let (mut queue, limits) = queue_with(0);
        queue.enqueue(DeltaBody::Foreign(testing::foreign_delta()), &limits);
        assert!(queue.is_empty());
    }

    #[test]
    fn take_prefers_least_travelled_then_newest() {
        let limits = Limits::default();
        let mut queue = GossipQueue::default();
        for (id, counter) in [("node-a", 1u64), ("node-b", 3), ("node-c", 2)] {
            queue.enqueue(
                DeltaBody::MembershipUpdate(testing::member(id, counter)),
                &limits,
            );
        }

        // Nothing has travelled yet, so the newest version leads.
        assert_eq!(queue.take(1, 100)[0].body.entity(), "member:node-b");

        // node-b has now been sent once, so both untravelled deltas precede
        // it however new it is.
        let order: Vec<String> = queue
            .take(3, 100)
            .iter()
            .map(|delta| delta.body.entity())
            .collect();
        assert_eq!(
            order,
            vec!["member:node-c", "member:node-a", "member:node-b"]
        );
    }

    #[test]
    fn deltas_retire_at_the_hop_limit() {
        let (mut queue, _) = queue_with(1);
        assert_eq!(queue.take(1, 2)[0].hops, 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.take(1, 2)[0].hops, 2);
        assert!(queue.is_empty(), "delta retires once hops reach the limit");
    }

    #[test]
    fn a_full_queue_evicts_the_most_travelled_delta() {
        let limits = Limits::try_from(LimitsSpec {
            max_gossip_queue: 3,
            max_gossip_per_message: 3,
            ..LimitsSpec::default()
        })
        .unwrap();
        let mut queue = GossipQueue::default();
        for index in 0..3 {
            queue.enqueue(
                DeltaBody::MembershipUpdate(testing::member(&format!("node-{index}"), 1)),
                &limits,
            );
        }
        // Travel node-0 so it becomes the eviction victim.
        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member("node-0", 2)),
            &limits,
        );
        queue.take(1, 100);

        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member("node-9", 1)),
            &limits,
        );
        assert_eq!(queue.len(), 3);
        let keys: Vec<String> = queue.iter().map(|(_, _, body)| body.entity()).collect();
        assert!(!keys.contains(&"member:node-0".to_owned()));
        assert!(keys.contains(&"member:node-9".to_owned()));
    }

    #[test]
    fn selection_is_identical_for_identical_queues() {
        let (mut left, _) = queue_with(20);
        let (mut right, _) = queue_with(20);
        assert_eq!(left.take(7, 100), right.take(7, 100));
    }

    #[test]
    fn blocklist_entries_key_on_their_target() {
        let (mut queue, limits) = queue_with(0);
        let mut entry = testing::blocklist_entry("banned");
        queue.enqueue(DeltaBody::BlocklistUpdate(entry.clone()), &limits);
        entry.action = BlocklistAction::Allow;
        entry.version.counter += 1;
        queue.enqueue(DeltaBody::BlocklistUpdate(entry), &limits);
        assert_eq!(queue.len(), 1);
        let (_, _, body) = queue.iter().next().unwrap();
        match body {
            DeltaBody::BlocklistUpdate(entry) => {
                assert_eq!(entry.action, BlocklistAction::Allow);
            }
            other => panic!("unexpected body: {other:?}"),
        }
    }
}
