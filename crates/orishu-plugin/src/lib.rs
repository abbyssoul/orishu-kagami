//! The shared, pure X-PLUGIN v1 declaration boundary.
//!
//! Raw declarations are serializable authoring data, not proof of acceptance.
//! [`Release::validate`] checks a root; [`ValidatedRelease::verify_payload`] checks
//! one declared contribution's bytes and scientific schema without executing it.
//! [`resolution`] provides deterministic exact provider selection, and [`bundle`]
//! validates/packs caller-owned stored-ZIP bytes without extraction. [`selected`]
//! compiles and independently verifies exact selected declaration/code closure.
//! Package IO, Wasm inspection and run admission belong to consuming owners. A
//! verified declaration or resolved selection is not runnable.
//!
//! JSON is an authoring representation. Identity uses explicit projections and
//! the existing workload deterministic-CBOR codec, never serde field layout.
//! Public byte readers enforce byte/depth/value budgets before typed decoding.
//! CBOR's temporary tree is bounded by these generic budgets; field-specific
//! collection limits are then checked before constructing typed collections.
//! JSON additionally checks collection limits while reading, before the next
//! rejected element is deserialized. No reader accepts null or duplicate keys.
//!
//! ```
//! use orishu_plugin::{release_from_json, Limits};
//! let limits = Limits::default();
//! let root = release_from_json(br#"{
//!   "apiVersion":"orishu.plugin/v1", "kind":"PluginRelease",
//!   "metadata":{"pluginId":"org.example.empty","versionLabel":"development"},
//!   "spec":{"contributions":[],"artifacts":[]}
//! }"#, &limits)?;
//! assert!(root.release_id(&limits)?.to_string().starts_with("sha256:"));
//! // A valid empty root proves no executable, inventory or runtime capability.
//! # Ok::<(), orishu_plugin::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod archive;
pub mod bundle;
mod codec;
pub mod execution;
mod ids;
mod model;
mod projection;
pub mod resolution;
pub mod selected;
mod validation;
pub mod workload;

pub use codec::{payload_from_cbor, payload_from_json, release_from_cbor, release_from_json};
pub use ids::*;
pub use model::*;
pub use orishu_variables::quantity::Dimension;
pub use orishu_workload::{ArtifactDigest, value::FiniteF64};
pub use validation::{ValidatedRelease, VerifiedPayload};

/// Caller-owned bounds. Values are not part of stored identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Root bytes, before any parsing.
    pub max_manifest_bytes: usize,
    /// One understood payload, before any parsing.
    pub max_payload_bytes: usize,
    /// Total structural values (including keys) in a single document.
    pub max_values: usize,
    /// Structured nesting; caller cannot raise the hard stack-safety ceiling 64.
    pub max_depth: usize,
    /// Maximum string bytes (also applies to authoring expressions).
    pub max_text_bytes: usize,
    /// Contributions in a release.
    pub max_contributions: usize,
    /// Artifact descriptors in a release.
    pub max_artifacts: usize,
    /// Requirements in one declaration.
    pub max_requirements: usize,
    /// Properties/constants or other schema list entries.
    pub max_schema_items: usize,
    /// Structural fields in one object, independent of variable-length lists.
    pub max_object_fields: usize,
    /// Observable slots supplied by a model.
    pub max_channels: usize,
    /// Largest artifact, including opaque unknown payloads.
    pub max_artifact_bytes: u64,
    /// Sum of declared artifact bytes (not archive overhead).
    pub max_declared_bytes: u64,
    /// Shared engine bounds for parsing retained default expressions. Symbol
    /// resolution and evaluation remain in the authoring integration slice.
    pub expressions: orishu_variables::Limits,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: 1024 * 1024,
            max_payload_bytes: 256 * 1024,
            max_values: 65_536,
            max_depth: 32,
            max_text_bytes: 4096,
            max_contributions: 256,
            max_artifacts: 4096,
            max_requirements: 64,
            max_schema_items: 256,
            max_object_fields: 32,
            max_channels: 128,
            max_artifact_bytes: 256 * 1024 * 1024,
            max_declared_bytes: 1024 * 1024 * 1024,
            expressions: orishu_variables::Limits::DEFAULT,
        }
    }
}

/// Stable, machine-readable failure categories for this slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorCode {
    /// Invalid schema, value, duplicate or relationship.
    Malformed,
    /// Caller budget exceeded.
    LimitExceeded,
    /// Bytes differ from a declared digest or size.
    IntegrityMismatch,
    /// Unknown root version, known-point version or execution contract.
    UnsupportedVersion,
    /// A reference does not name a declared artifact or local contribution.
    InvalidSelection,
}

/// A bounded diagnostic, without echoing arbitrary input or credentials.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
#[error("{code:?} at {path}: {message}")]
pub struct Error {
    /// Stable category.
    pub code: ErrorCode,
    /// Bounded field path or subject.
    pub path: String,
    /// Bounded description, not the rejected bytes.
    pub message: String,
}

impl Error {
    pub(crate) fn new(code: ErrorCode, path: &str, message: &str) -> Self {
        fn bounded(value: &str, max: usize) -> String {
            let mut end = value.len().min(max);
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            value[..end].to_owned()
        }
        Self {
            code,
            path: bounded(path, 256),
            message: bounded(message, 512),
        }
    }

    pub(crate) fn malformed(path: &str, message: &str) -> Self {
        Self::new(ErrorCode::Malformed, path, message)
    }
}
