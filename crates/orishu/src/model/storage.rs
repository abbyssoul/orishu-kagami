use serde::{Deserialize, Serialize};

/// Where the artifact data is stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageBackend {
    /// Node-local filesystem.
    Local,
    /// In-process memory (non-persistent).
    Memory,
    /// External object storage (S3-compatible, etc.).
    External,
}
