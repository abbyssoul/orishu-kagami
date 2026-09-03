use crate::{
    client::ClusterAddress,
    model::{manifest::Manifest as DefManifest, node::NodeId, workload::SimulationState},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::workload::Manifest as WorkloadManifest;

/// High-level summary of the cluster, returned by `GET /cluster`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSpec {
    pub membership_locked: bool,
    pub workload: Option<WorkloadManifest>,
}

/// Cluster state version
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub epoch: u64,
    pub counter: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStatus {
    pub version: Version,
    pub node_count: u32,
    pub simulation_status: Option<SimulationState>,
}

pub type Manifest = DefManifest<ClusterSpec, ClusterStatus>;

/// A structured cluster event: node joins/leaves, workload lifecycle transitions,
/// administrative actions. See `AuditEvent` for privileged-operation records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    /// Node ID, workload ID, or other affected resource identifier.
    pub target_id: String,
    pub details: String,
}

crate::query_filter! {

/// Criteria for filtering `GET /cluster/events` results.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Inclusive lower bound.
    pub after as "after": Option<DateTime<Utc>>,
    /// Inclusive upper bound.
    pub before as "before": Option<DateTime<Utc>>,
    /// Restrict to these event type names. Empty means all.
    pub event_types as "type": Vec<String>,
}
}
/// A single log line from a cluster node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub component: String,
    pub node_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => f.write_str("trace"),
            Self::Debug => f.write_str("debug"),
            Self::Info => f.write_str("info"),
            Self::Warn => f.write_str("warn"),
            Self::Error => f.write_str("error"),
        }
    }
}

crate::query_filter! {
/// Criteria for filtering `GET /cluster/logs` results.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Inclusive lower bound.
    pub after  as "after": Option<DateTime<Utc>>,
    /// Inclusive upper bound.
    pub before as "before": Option<DateTime<Utc>>,
    pub level as "level": Option<LogLevel>,
    pub component as "component": Option<String>,
}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadCompatibilityReport {
    /// Number of nodes currently in the online
    /// that match workload requirements
    pub nodes_compatible: u32,
}

/// Locking state intent.
/// When a server receives this intent, it is instructed to transition cluster membership state to 'locked'
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockIntent {
    /// Expression of intent to lock the cluster membership.
    /// Note that this message is reused by GET and POST.
    /// Server only accepts POST with  locked=true, and expects users to use DELETE to unlock.
    pub locked: bool,
}

crate::query_filter! {
/// Filter for members, when listing cluster participants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MembersSelector {
    /// Filter by membership state: `Alive`, `Suspected`, `Dead`, `Removed`.
    pub member_state as "memberState": Option<String>,
    /// Filter by node name.
    pub name as "name": Option<String>,
    /// Filter by capability role: `introducer` or `worker`.
    pub role as "role": Option<String>,
}
}

/// A newly issued join token. The token value is shown exactly once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinToken {
    pub token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// An intent for the recipient of this message to join a given cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinIntent {
    pub addresses: Vec<ClusterAddress>,
    pub token: JoinToken,
}

/// Result of a node voluntarily leaving its cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveResult {
    /// Cluster name the node was previously a member of, if any.
    pub previous_cluster: Option<String>,
    /// Node ID the node held in the previous cluster, if any.
    pub previous_node_id: Option<String>,
}

/// Cluster response when a new node is accepted
/// It provides node with a new NodeId assigned by the cluster,
/// ID of the node that introduced request into the cluster and basic cluster info as [Manifest].
/// Note that Node-ID header in this case is the ID of the node that has joined the cluster, not the introducer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequestAccepted {
    /// ID assigned to the joining node by the cluster.
    pub node_id: NodeId,
    /// ID of the cluster member that admitted the node.
    pub admitted_by: NodeId,
    /// Cluster manifest, providing the joining node with basic cluster information.
    pub cluster: Manifest,
}
