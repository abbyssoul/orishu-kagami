use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::node::NodeId;

/// Identity dimension of a blocklist entry. Variants are mutually exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum BlocklistIdentityMatcher {
    /// Block by cluster-assigned node ID (UUID). Transient — only effective while the node holds that ID.
    Id(NodeId),
    /// Block by worker name. Affects all current and future instances sharing this name.
    Name(String),
    /// Block by mTLS certificate fingerprint. Durable — persists across cluster leave/rejoin and name changes.
    CertFingerprint(String),
}

/// Network dimension of a blocklist entry. Variants are mutually exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum BlocklistNetworkMatcher {
    /// Block a specific source IP address.
    Host(String),
    /// Block a CIDR network range (e.g. `"10.0.0.0/8"`).
    Cidr(String),
}

/// Effect of adding a new entry to the blocklist
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlocklistEffect {
    /// No active node matched; the entry prevents future joins.
    Blocked,
    /// The target matched an active node and membership was not locked, so the node was disconnected.
    BlockedAndRemoved,
    /// the target matched an active node but membership is locked; the node remains until it leaves for another reason.
    BlockedDeferred,
}

/// A single entry in the cluster-wide blocklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Unique entry identifier for use with the `rm` command.
    pub entry_id: String,
    /// Identity matcher, if any.
    pub identity: Option<BlocklistIdentityMatcher>,
    /// Network matcher, if any.
    pub network: Option<BlocklistNetworkMatcher>,
    /// System record for when the block was added.
    pub added_at: DateTime<Utc>,
    /// Identity of the operator who added the entry.
    pub added_by: String,
}

crate::query_filter! {
/// Criteria for filtering `GET /cluster/blocklist/` results.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Filter by node ID.
    pub ids as "identity.id": Vec<String>,

    /// Filter by worker name.
    pub names as "identity.name": Vec<String>,

    /// Filter by certificate fingerprint.
    pub certs as "identity.certFingerprint": Vec<String>,

    /// Filter by host IP.
    pub hosts as "network.host": Vec<String>,

    /// Filter by CIDR range.
    pub cidrs as "network.cidr": Vec<String>,

    /// Inclusive lower bound.
    pub after as "after": Option<DateTime<Utc>>,
    /// Inclusive upper bound.
    pub before as "before": Option<DateTime<Utc>>,

    /// Filter by the identity of the operator who added the entry.
    pub added_by as "addedBy": Vec<String>,
}
}

/// Request body for adding a new entry to the blocklist.
/// At least one of `identity` or `network` must be provided.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlocklistAddRequest {
    /// Identity dimension matcher (mutually exclusive variants).
    pub identity: Option<BlocklistIdentityMatcher>,
    /// Network dimension matcher (mutually exclusive variants).
    pub network: Option<BlocklistNetworkMatcher>,
}

/// Reply to addition of a new target to the blocklist
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlocklistAddResult {
    /// The entry that was created.
    pub entry: Entry,
    /// Effect of the operation.
    pub effect: BlocklistEffect,
}
