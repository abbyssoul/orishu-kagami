use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::manifest;

/// Stable, cluster unique identity of a checkpoint in the simulation datastream.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CheckpointId(pub String);

impl CheckpointId {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for CheckpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for CheckpointId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(CheckpointId(s.to_string()))
    }
}

/// Metadata for a stored checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: CheckpointId,
    pub workload_name: manifest::Name,
    pub workload_id: manifest::ResourceUid,
    pub workload_epoch: u64,
    pub recorded_at: DateTime<Utc>,
    pub simulation_time: f64,
    pub graceful: bool,
    /// True if all chunks are present and the checkpoint can be used for resume.
    pub resumable: bool,
}

crate::query_filter! {
/// Criteria for filtering checkpoint list results.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub workload_id as "workloadId": Option<String>,
    pub workload_name as "workloadName": Option<String>,
    pub after as "after": Option<DateTime<Utc>>,
    pub before as "before": Option<DateTime<Utc>>,
    pub resumable as "resumable": Option<bool>,
}
}
