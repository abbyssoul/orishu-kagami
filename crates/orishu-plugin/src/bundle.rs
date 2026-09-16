//! The local `.okplugin` bundle: strict archive framing plus exact release closure.
use crate::{archive, resolution::VerifiedRelease, *};
use std::collections::BTreeMap;

/// Archive budgets in addition to declaration and artifact limits.
#[derive(Clone, Copy, Debug)]
pub struct BundleLimits {
    /// Complete archive bytes.
    pub max_bytes: usize,
    /// Physical entries including the root.
    pub max_entries: usize,
}
impl Default for BundleLimits {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024 * 1024,
            max_entries: 4097,
        }
    }
}
impl BundleLimits {
    fn framing(self, limits: &Limits) -> archive::ArchiveLimits {
        archive::ArchiveLimits {
            max_bytes: self.max_bytes,
            max_entries: self.max_entries.min(limits.max_artifacts.saturating_add(1)),
            root_bytes: limits.max_manifest_bytes,
            blob_bytes: limits.max_artifact_bytes,
            total_blob_bytes: limits.max_declared_bytes,
        }
    }
}
/// Verified declarations and borrowed artifacts; never code execution.
#[derive(Debug)]
pub struct Bundle<'a> {
    release: VerifiedRelease,
    artifacts: BTreeMap<ArtifactDigest, &'a [u8]>,
}
impl<'a> Bundle<'a> {
    /// Verified immutable release identity and declarations.
    pub fn release(&self) -> &VerifiedRelease {
        &self.release
    }
    /// Exact caller-owned artifact bytes, keyed by verified digest.
    pub fn artifacts(&self) -> &BTreeMap<ArtifactDigest, &'a [u8]> {
        &self.artifacts
    }
    /// Consume verification metadata without copying artifact bytes.
    pub fn into_parts(self) -> (VerifiedRelease, BTreeMap<ArtifactDigest, &'a [u8]>) {
        (self.release, self.artifacts)
    }
}
/// Verify strict stored ZIP and the complete declared release closure.
pub fn read<'a>(
    bytes: &'a [u8],
    limits: &Limits,
    policy: BundleLimits,
) -> Result<Bundle<'a>, Error> {
    let archive = archive::read(bytes, archive::Root::Plugin, policy.framing(limits))?;
    let root = release_from_cbor(archive.root, limits)?;
    let artifacts = archive.blobs;
    if artifacts.len() != root.0.spec.artifacts.len()
        || root
            .0
            .spec
            .artifacts
            .iter()
            .any(|a| !artifacts.contains_key(&a.digest))
    {
        return Err(Error::new(
            ErrorCode::Malformed,
            "bundle",
            "bundle differs from declared artifact closure",
        ));
    }
    let release = VerifiedRelease::verify(root, &artifacts, limits)?;
    Ok(Bundle { release, artifacts })
}
/// Pack exactly the declared closure. Extras in the caller's cache are excluded.
pub fn pack(
    root: &Release,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: &Limits,
    policy: BundleLimits,
) -> Result<Vec<u8>, Error> {
    root.validate(limits)?.verify_all(blobs)?;
    let manifest = root.canonical_bytes(limits)?;
    let required = root
        .0
        .spec
        .artifacts
        .iter()
        .map(|a| (a.digest, blobs[&a.digest]))
        .collect();
    archive::pack(
        archive::Root::Plugin,
        &manifest,
        &required,
        policy.framing(limits),
    )
}
