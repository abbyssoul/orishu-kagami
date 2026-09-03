use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type of a recorded administrative audit event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditEventType {
    NodeJoined,
    NodeRemoved,
    NodeDead,
    NodeLeft,
    MembershipLocked,
    MembershipUnlocked,
    BlocklistEntryAdded,
    BlocklistEntryRemoved,
    WorkloadLoaded,
    WorkloadStarted,
    WorkloadStopped,
    TokenRotated,
    NodeTombstoneCleared,
    ResultPurged,
    CheckpointPurged,
}

/// Outcome of the operation that triggered the audit event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditOutcome {
    Success,
    Failure,
}

/// A structured record of a significant administrative action.
/// Stored as an append-only log on each node and best-effort gossip-propagated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: AuditEventType,
    /// Identity of the operator or system component that triggered the event.
    pub actor: String,
    /// Node ID, blocklist entry, or workload ID affected by the event.
    pub target_id: String,
    pub outcome: AuditOutcome,
    /// Event-type-specific metadata.
    pub details: std::collections::HashMap<String, String>,
}

crate::query_filter! {
    /// Criteria for filtering `GET /cluster/audit-log` results.
    #[derive(Debug, Clone, Default)]
    pub struct AuditFilter {
        /// Inclusive lower bound.
        pub after as "after": Option<DateTime<Utc>>,
        /// Inclusive upper bound.
        pub before as "before": Option<DateTime<Utc>>,
        /// Restrict to these event type names. Empty means all.
        pub event_types as "type": Vec<String>,
    }
}
