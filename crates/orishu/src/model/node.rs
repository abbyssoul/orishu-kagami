use std::str::FromStr;

use crate::model::{QuerySet, manifest::Manifest as DefManifest, storage::StorageBackend};
use serde::{Deserialize, Serialize};

/// Node manifest kind
pub const NODE_MANIFEST_KIND: &str = "Node";

/// Formation-assigned identity of a cluster node.
///
/// Defined once in [`orishu_identity`] and re-exported here so the client DTOs
/// and the sans-IO membership core share one contract. It is opaque: generated
/// by the admitting formation, unique within that formation only, and never
/// derived from a worker name.
pub use orishu_identity::NodeId;

/// Versioned local membership inspection, not a worker configuration manifest.
/// Remote member facts are this entry worker's potentially stale gossip view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inspection {
    /// Current resource version.
    #[serde(deserialize_with = "inspection_version")]
    pub schema_version: u32,
    /// Formation containing the record at the owner read boundary.
    pub formation_id: orishu_identity::FormationId,
    /// Entry worker that supplied this view, not necessarily the target.
    pub source_node_id: NodeId,
    /// Local-at-request projection; not proof the target is reachable.
    pub view: super::cluster::SummaryView,
    /// Formation-assigned target identity, never its display label.
    pub node_id: NodeId,
    /// Non-unique worker label.
    pub worker_name: orishu_identity::WorkerName,
    /// Public certificate binding held by membership.
    pub cert_fingerprint: orishu_identity::CertFingerprint,
    /// Locally observed liveness, independent of administrative removal.
    pub liveness: MemberState,
    /// Liveness incarnation held by the entry worker.
    pub incarnation: orishu_identity::Incarnation,
    /// Current local record version.
    pub version: orishu_identity::VersionTuple,
    /// Advertised role flags, not proof of current introducer readiness.
    pub accepts: NodeAccepts,
    /// Advertised peer endpoints, not authoritative trust roots.
    pub peer_endpoints: Vec<String>,
    /// Advertised client endpoints; no connection is attempted by inspection.
    pub client_endpoints: Vec<String>,
}

fn inspection_version<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    match u32::deserialize(d)? {
        1 => Ok(1),
        _ => Err(serde::de::Error::custom("unsupported inspection schema")),
    }
}

/// A bounded local read, not a retained snapshot across pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MembershipPage {
    /// Resource schema, currently 1.
    #[serde(deserialize_with = "inspection_version")]
    pub schema_version: u32,
    /// Formation at this page's owner boundary; required on continuation.
    pub formation_id: orishu_identity::FormationId,
    /// Entry worker; paging never silently changes its source.
    pub source_node_id: NodeId,
    /// At most four records, ordered by assigned ID.
    pub members: Vec<Inspection>,
    /// Exclusive last-ID cursor when more records exist locally.
    pub next_after: Option<NodeId>,
}

/// Operational role flags read from the node's startup configuration.
/// Observed by peers via gossip protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeAccepts {
    /// Whether the node accepts direct connections from operator/user clients.
    pub clients: bool,
    /// Whether the node accepts join requests from new peers (acts as introducer/seed).
    pub peers: bool,
    /// Whether the node accepts new workload partitions. When false the node is cordoned.
    pub work: bool,
}

/// Hardware resources advertised by a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCapabilities {
    /// Number of CPU cores this node has access to
    pub cpu_cores: u32,
    /// Amount of memory available to this node
    pub memory_bytes: u64,
    /// e.g. "x86_64", "aarch64"
    pub architecture: String,
    /// Storage backend summary
    pub storage: NodeStorage,
    /// Accelerator descriptions (GPU model, OpenCL version, etc.).
    pub accelerators: Vec<String>,
}

/// Info about Node storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStorage {
    /// Maximum number of peer connections this node will accept. `0` - no limit set.
    pub driver: StorageBackend,
    /// Number of additional node the data partition is replicated to. `1` - no copy.
    pub replicas: Option<u32>,
}

impl Default for NodeStorage {
    /// The default address is the local node's Unix domain socket.
    /// Matches the behavior of running `orishu-ctl` or `orishu-monitor` without `--host`.
    fn default() -> Self {
        Self {
            driver: StorageBackend::Memory,
            replicas: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLimits {
    /// Maximum number of peer connections this node will accept. `0` - no limit set.
    pub peers: Option<u32>,
    /// Maximum number of client connections this node will accept. `0` - no limit set.
    pub clients: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeListen {
    /// Hostname or IP advertised for for peer connections.
    pub peers: Vec<String>,
    /// Hostname or IP advertised for client connections. Empty if the node does not accept clients.
    pub clients: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSpec {
    pub accepts: NodeAccepts,
    pub limits: NodeLimits,
    pub listen: NodeListen,
}

/// Hardware resources advertised by a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStats {
    /// Current number of peer connections
    pub peers: u32,

    /// Current number of active client connections
    pub clients: u32,
}

/// Controls where node manifest data is fetched from during `inspect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InspectSource {
    /// Fetch directly from the target node. Most up-to-date but requires the node to be reachable.
    Direct,
    /// Return from another node's gossip-based view, without contacting the target.
    Indirect,
    /// Try direct first, fall back to gossip-cached if unreachable (default).
    BestEffort,
}

impl std::fmt::Display for InspectSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct => f.write_str("direct"),
            Self::Indirect => f.write_str("indirect"),
            Self::BestEffort => f.write_str("best-effort"),
        }
    }
}

/// Controls how a node is removed from the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoveMode {
    /// Default. The node finishes in-flight work, transfers result data to
    /// replicas, and exits cleanly.
    Graceful,
    /// Drop the node immediately. Peers are instructed to reject any pending
    /// results or state transfers from it, preventing potentially corrupted
    /// data from entering the simulation.
    Force,
}

impl QuerySet for &RemoveMode {
    fn to_query(&self) -> String {
        match self {
            RemoveMode::Graceful => String::new(),
            RemoveMode::Force => String::from_str("force=true").unwrap(),
        }
    }
}

/// Result of a node connectivity and health diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticResult {
    pub target_id: NodeId,
    /// Node that performed the check. Same as target for a direct check.
    pub source_id: Option<NodeId>,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub details: Option<String>,
}

/// Lifecycle membership state of a node as seen by the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemberState {
    Alive,
    Suspected,
    Dead,
    Removed,
}

/// Node state describing current dynamic state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    /// semver version of the running worker process.
    pub version: String,

    /// SHA-256 fingerprint of the node's mTLS certificate.
    pub cert_fingerprint: Vec<u8>,

    /// Runtime-only current stats number (not CRDT-replicated).
    pub connected: ConnectionStats,

    pub capabilities: NodeCapabilities,

    /// Cluster membership status
    /// Note, node is not expected to self report it.
    /// If you get a reply from the node itself, its alive.
    /// If you asked about node another peer, you can get different membership estimate.
    /// Such is nature of a swarm system.
    pub member: MemberState,
}

/// A single cluster member as known to the rest of the cluster.
pub type Manifest = DefManifest<NodeSpec, NodeStatus>;
