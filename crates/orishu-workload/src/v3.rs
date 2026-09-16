//! Composed scientific workload format. V2 remains a separate immutable format;
//! no v2 bytes or identities are rewritten by this addition.
//!
//! The component graph stays in the root. Captured scientific setup and exact
//! plugin selection are digest-addressed inputs, interpreted only by a supported
//! execution profile. In particular this format does not impose v2's uniform
//! spatial step or host-level integration selector on analytic/plugin models.
use crate::{
    ArtifactDescriptor, BlobSource, CanonicalError, ClosureReport, ComputeSpec, Limits,
    VerifiedClosure, WorkloadDigest, WorkloadMeta, WorkloadRequirements, canonical,
    manifest::{ApiVersion, DenyUnknown, NoStatus, Resource},
};
use serde::{Deserialize, Serialize};

/// New format discriminator; never a rename of the existing v2 root.
pub const API_VERSION: &str = "orishu.dev/v3";
/// Exact selection descriptor input role.
pub const SELECTION_ROLE: &str = "plugin-selection";
/// Captured scientific execution definition input role.
pub const EXECUTION_ROLE: &str = "scientific-execution";

/// The composed workload's root specification. Scientific profile-specific state
/// remains data in the closure, not a second mutable run/document authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadSpec {
    /// Independently compiled kernels, ownership and deterministic invocation plan.
    pub compute: ComputeSpec,
    /// Exact selected contribution/provider/release evidence descriptor.
    pub selection: ArtifactDescriptor,
    /// Captured objects, field/history state, domains, configuration and timestep.
    /// This descriptor names the definition; its referenced blobs also appear in
    /// `artifacts` so ordinary byte-delivery needs no plugin-specific parser.
    pub execution: ArtifactDescriptor,
    /// Every remaining required blob: selected evidence/payloads and scientific
    /// inputs/state. Component code is already declared by `compute.components`.
    /// Profile admission rejects missing or unrelated entries, not just corruption.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,
    /// Explicit hardware/numerical requirements; never worker-local substitutions.
    #[serde(default)]
    pub requirements: WorkloadRequirements,
}
/// Identity-bearing resource, with no status or cluster-assigned identity slot.
pub type WorkloadManifest = Resource<WorkloadMeta, WorkloadSpec, NoStatus, DenyUnknown>;

/// Construct with the v3 discriminator. Raw values still require validation.
pub fn manifest(metadata: WorkloadMeta, spec: WorkloadSpec) -> WorkloadManifest {
    WorkloadManifest::new(
        ApiVersion::from_static(API_VERSION),
        crate::manifest::kind(),
        metadata,
        spec,
    )
}
/// Exact canonical bytes. Unsupported profiles may be represented, but only an
/// embedding admission authority recognizing that profile may execute them.
pub fn canonical_bytes(
    manifest: &WorkloadManifest,
    limits: &Limits,
) -> Result<Vec<u8>, CanonicalError> {
    canonical::v3::canonical_bytes(manifest, limits)
}
/// Bounded canonical reader, rejecting v2 and any unsupported root discriminator.
/// Generic graph/schema reading is not scientific execution-profile admission.
pub fn from_canonical_bytes(
    bytes: &[u8],
    limits: &Limits,
) -> Result<WorkloadManifest, CanonicalError> {
    canonical::v3::from_canonical_bytes(bytes, limits)
}
/// Root identity commits to every descriptor and the full scientific closure.
pub fn workload_digest(
    manifest: &WorkloadManifest,
    limits: &Limits,
) -> Result<WorkloadDigest, CanonicalError> {
    Ok(WorkloadDigest::of_canonical_bytes(&canonical_bytes(
        manifest, limits,
    )?))
}
/// Structural graph/metadata/descriptor validation followed by streaming byte
/// verification. No source is consulted before all structural checks pass.
/// This witness is not scientific profile, selected-plugin or Wasm admission.
pub fn validate_closure(
    manifest: &WorkloadManifest,
    blobs: &impl BlobSource,
    limits: &Limits,
) -> Result<VerifiedClosure, ClosureReport> {
    crate::closure::validate_v3_closure(manifest, blobs, limits)
}
