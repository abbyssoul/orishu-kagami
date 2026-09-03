use crate::model::node::NodeId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Record of a node that was removed from the cluster.
/// Exposed via /cluster/tombstones.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TombstoneRecord {
    /// The cluster-assigned ID the node held when it was removed.
    pub node_id: NodeId,
    /// Last known node name.
    pub name: String,
    /// SHA-256 fingerprint of the node's mTLS certificate.
    pub cert_fingerprint: Vec<u8>,
    /// When the node was removed.
    pub removed_at: DateTime<Utc>,
    /// Operator identity or system actor that triggered the removal.
    pub removed_by: String,
    /// How the node was removed.
    pub removal_mode: RemovalMode,
    /// Optional human-readable reason for removal.
    pub reason: Option<String>,
}

/// How a node was removed from the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemovalMode {
    /// Graceful removal (operator-initiated).
    Graceful,
    /// Immediate removal (operator-initiated force).
    Force,
    /// Automated removal after the node was confirmed dead.
    Dead,
    /// Automated removal after the node matched a blocklist entry.
    Blocklist,
}

impl crate::model::ToQueryParam for RemovalMode {
    fn to_query_param(&self) -> Option<String> {
        Some(match self {
            RemovalMode::Graceful => "graceful".to_string(),
            RemovalMode::Force => "force".to_string(),
            RemovalMode::Dead => "dead".to_string(),
            RemovalMode::Blocklist => "blocklist".to_string(),
        })
    }
}

impl crate::model::ToQueryParam for Vec<RemovalMode> {
    fn to_query_param(&self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(
                self.iter()
                    .map(|v| match v {
                        RemovalMode::Graceful => "graceful",
                        RemovalMode::Force => "force",
                        RemovalMode::Dead => "dead",
                        RemovalMode::Blocklist => "blocklist",
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            )
        }
    }
}

crate::query_filter! {
    /// Filtering options for listing tombstones.
    #[derive(Debug, Clone, Default)]
    pub struct TombstoneFilter {
        /// Filter by name (exact match).
        pub name as "name": Option<String>,
        /// Filter by removal mode.
        pub removal_mode as "removalMode": Vec<RemovalMode>,
        /// Filter by operator identity.
        pub removed_by as "removedBy": Vec<String>,
        /// Inclusive lower bound.
        pub after as "after": Option<DateTime<Utc>>,
        /// Exclusive upper bound.
        pub before as "before": Option<DateTime<Utc>>,
        /// Pagination cursor.
        pub cursor as "cursor": Option<String>,
        /// Pagination limit.
        pub limit as "limit": Option<u32>,
    }
}
