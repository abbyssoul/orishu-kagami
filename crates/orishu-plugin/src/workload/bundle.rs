//! Portable workload stored-ZIP v1: `workload.cbor` and the exact flat blob closure.
//! Distribution only; archive ordering/metadata never define workload identity.
//! Verification does not compile or execute guests or authorize a worker run.
use super::*;
use crate::archive::{self, ArchiveLimits, Root};

/// Complete independently verified scientific closure, borrowing archive bytes.
#[derive(Debug)]
pub struct Bundle<'a> {
    verified: VerifiedWorkload,
    root: &'a [u8],
    blobs: BTreeMap<ArtifactDigest, &'a [u8]>,
}
impl<'a> Bundle<'a> {
    /// Verified root, composition and declared scientific inputs.
    pub fn verified(&self) -> &VerifiedWorkload {
        &self.verified
    }
    /// Canonical root bytes, suitable for runtime admission or thin submission.
    pub fn manifest_bytes(&self) -> &'a [u8] {
        self.root
    }
    /// Exactly required bytes; no package installation or external fetch is needed.
    pub fn blobs(&self) -> &BTreeMap<ArtifactDigest, &'a [u8]> {
        &self.blobs
    }
}
fn framing(mut archive: ArchiveLimits, limits: ProfileLimits) -> ArchiveLimits {
    archive.max_entries = archive
        .max_entries
        .min(limits.workload.max_artifacts.saturating_add(1));
    archive.root_bytes = archive
        .root_bytes
        .min(usize::try_from(limits.workload.max_manifest_bytes).unwrap_or(usize::MAX));
    archive.blob_bytes = archive.blob_bytes.min(limits.workload.max_artifact_bytes);
    archive.total_blob_bytes = archive
        .total_blob_bytes
        .min(limits.workload.max_aggregate_declared_bytes);
    archive
}
fn descriptors(manifest: &v3::WorkloadManifest) -> impl Iterator<Item = &w::ArtifactDescriptor> {
    manifest
        .spec
        .compute
        .components
        .iter()
        .map(|c| &c.artifact)
        .chain([&manifest.spec.selection, &manifest.spec.execution])
        .chain(&manifest.spec.artifacts)
}
/// Read a complete v3 scientific workload with bounded framing, exact closure and
/// independent profile verification. Extra physical blobs fail, even if hashed
/// correctly; release evidence is not an edge to unused plugin artifacts.
pub fn read(
    bytes: &[u8],
    limits: ProfileLimits,
    policy: ArchiveLimits,
) -> Result<Bundle<'_>, WorkloadError> {
    let archive = archive::read(bytes, Root::Workload, framing(policy, limits))?;
    let manifest = v3::from_canonical_bytes(archive.root, &limits.workload)?;
    let wanted: BTreeSet<_> = descriptors(&manifest).map(|a| a.digest).collect();
    if wanted.len() != archive.blobs.len() || archive.blobs.keys().any(|d| !wanted.contains(d)) {
        return Err(invalid(
            "bundle",
            "physical bundle differs from required workload closure",
        )
        .into());
    }
    let verified = super::verify(manifest, &archive.blobs, limits)?;
    Ok(Bundle {
        verified,
        root: archive.root,
        blobs: archive.blobs,
    })
}
/// Pack a canonical v3 root and only its required closure from a caller's store.
/// Scientific validation precedes packing; cache extras are never copied. The
/// resulting bytes are deterministic, but only the root/individual blob digests
/// are scientific identities. No initialization or native/Wasm loading occurs.
pub fn pack(
    manifest_bytes: &[u8],
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: ProfileLimits,
    policy: ArchiveLimits,
) -> Result<Vec<u8>, WorkloadError> {
    let manifest = v3::from_canonical_bytes(manifest_bytes, &limits.workload)?;
    let verified = super::verify(manifest, blobs, limits)?;
    let required = descriptors(verified.manifest())
        .map(|a| (a.digest, blobs[&a.digest]))
        .collect();
    Ok(archive::pack(
        Root::Workload,
        manifest_bytes,
        &required,
        framing(policy, limits),
    )?)
}
