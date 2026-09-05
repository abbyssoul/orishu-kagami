//! Membership tombstones: formation-scoped barriers against readmission.

use serde::{Deserialize, Serialize};

use crate::{CertFingerprint, NodeId, VersionTuple, WorkerName};

/// How a node was removed from a formation.
///
/// Removal is an *operator or policy* action and is deliberately distinct from
/// SWIM liveness. A node the failure detector believes is `Dead` is still a
/// member; a removed node is not, and cannot become one again until the
/// tombstone is explicitly cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemovalMode {
    /// Graceful removal (operator-initiated). The node finishes in-flight work
    /// and transfers state before exiting.
    Graceful,
    /// Immediate removal (operator-initiated force). Peers reject pending
    /// results and state transfers from the removed node.
    Force,
    /// Automated removal after the node was confirmed dead.
    ///
    /// Reaching this state requires an explicit removal decision. The failure
    /// detector alone marks a member `Dead`; it never writes a tombstone,
    /// because doing so would make an unreachable-but-healthy node
    /// permanently unwelcome on the strength of a timeout.
    Dead,
    /// Automated removal after the node matched a blocklist entry.
    Blocklist,
}

impl std::fmt::Display for RemovalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Graceful => "graceful",
            Self::Force => "force",
            Self::Dead => "dead",
            Self::Blocklist => "blocklist",
        })
    }
}

/// Record that a node was removed from a formation, and must not be readmitted.
///
/// The tombstone fences three separate things, all of which a hostile or merely
/// stale peer might otherwise use to resurrect the node: the assigned
/// [`NodeId`], the certificate fingerprint that would let it reconnect as a
/// known peer, and — for diagnostics only — the label it was last seen under.
///
/// A tombstone is not a liveness state. No `Alive` announcement, however high
/// its incarnation, clears one; only an explicit operator action does. A
/// voluntary self-`Leave` does *not* create one, because leaving is a node's
/// own decision to stop participating, not the formation's decision to bar it.
///
/// # Time
///
/// There is no timestamp field. Ordering between competing tombstones uses
/// [`VersionTuple`], which is causal and requires no clock, so this type stays
/// usable inside a sans-IO core. The wall-clock `removedAt` an operator sees is
/// a client-facing projection recorded by whichever adapter owns the audit
/// trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipTombstone {
    /// The formation-assigned ID the node held when it was removed.
    pub node_id: NodeId,
    /// Label the node was last known by. Diagnostic only — it is not unique
    /// and must never be used on its own to fence a join.
    pub name: WorkerName,
    /// Certificate fingerprint pinned at admission, fenced against reconnect.
    pub cert_fingerprint: CertFingerprint,
    /// How the node was removed.
    pub removal_mode: RemovalMode,
    /// Version of this tombstone, for deterministic merge.
    pub version: VersionTuple,
    /// Whether an operator has explicitly lifted the barrier.
    ///
    /// Clearing sets this flag and bumps [`MembershipTombstone::version`]
    /// rather than deleting the record, because a deletion is not itself a
    /// versioned fact: a peer that had not yet heard about the clear would
    /// re-gossip the tombstone and re-fence the node forever. A cleared
    /// tombstone still fences nothing while remaining an auditable record that
    /// the removal happened.
    #[serde(default)]
    pub cleared: bool,
    /// Optional operator-supplied reason. Bounded by the consuming core's
    /// limits before it is adopted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombstone_round_trips_as_json() {
        let tombstone = MembershipTombstone {
            node_id: NodeId::new("node-abc").unwrap(),
            name: WorkerName::new("worker-alpha").unwrap(),
            cert_fingerprint: CertFingerprint::from_bytes([7u8; 32]),
            removal_mode: RemovalMode::Force,
            version: VersionTuple::initial(1, NodeId::new("node-seed").unwrap()),
            cleared: false,
            reason: Some("decommissioned".to_owned()),
        };

        let json = serde_json::to_value(&tombstone).unwrap();
        assert_eq!(json["nodeId"], "node-abc");
        assert_eq!(json["removalMode"], "force");
        assert_eq!(json["version"]["actor"], "node-seed");
        assert_eq!(
            serde_json::from_value::<MembershipTombstone>(json).unwrap(),
            tombstone
        );
    }

    #[test]
    fn reason_is_omitted_when_absent() {
        let tombstone = MembershipTombstone {
            node_id: NodeId::new("node-abc").unwrap(),
            name: WorkerName::new("worker-alpha").unwrap(),
            cert_fingerprint: CertFingerprint::from_bytes([7u8; 32]),
            removal_mode: RemovalMode::Graceful,
            version: VersionTuple::initial(1, NodeId::new("node-seed").unwrap()),
            cleared: false,
            reason: None,
        };
        let json = serde_json::to_value(&tombstone).unwrap();
        assert!(json.get("reason").is_none());
    }
}
