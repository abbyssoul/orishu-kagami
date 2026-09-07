use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{manifest, storage::StorageBackend};

/// Stable identifier for a stored simulation result artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResultId(String);

impl std::fmt::Display for ResultId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for ResultId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ResultId(s.to_string()))
    }
}

/// Metadata about a stored simulation result artifact.
/// The metadata is gossip-propagated; the artifact data lives on the owning node(s).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: ResultId,
    /// The `metadata.uid` of the `workload::Manifest` this result was produced from.
    pub workload_id: manifest::ResourceUid,
    /// The name of the `workload::Manifest` this result was produced from.
    pub workload_name: manifest::Name,
    /// The epoch of the `workload` this result was produced from.
    pub workload_epoch: u64,
    /// When the simulation run began.
    pub started_at: DateTime<Utc>,
    /// When the result snapshot was saved (i.e. when the simulation stopped or a checkpoint was taken).
    pub recorded_at: DateTime<Utc>,
    /// `[start, end]` simulation time covered by this artifact.
    pub simulation_time_range: [f64; 2],
    pub total_size_bytes: u64,
    /// `true` if the simulation was stopped gracefully with a consistent snapshot.
    pub graceful: bool,
    pub storage_backend: StorageBackend,
    /// Partition ID → storage reference (node IDs for local/memory; object key for external).
    pub chunks: std::collections::HashMap<String, String>,
    /// Partitions that were not successfully stored (result is incomplete if non-empty).
    pub missing_chunks: Vec<String>,
}

crate::query_filter! {
/// Criteria for filtering result listings and bulk deletions.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub workload_id as "workloadId": Option<String>,
    pub workload_name as "workloadName": Option<String>,
    pub after as "after": Option<DateTime<Utc>>,
    pub before as "before": Option<DateTime<Utc>>,
}
}
