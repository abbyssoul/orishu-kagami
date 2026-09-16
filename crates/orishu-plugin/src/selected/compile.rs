//! Pure selected-closure compiler. No inventory/byte-store mutation or IO.
use super::*;
use crate::resolution::{Selection, VerifiedRelease};

/// Compiler result: small owned evidence plus references describing exactly which
/// caller-held payload/code blobs to export. No code/input blobs are copied here.
#[derive(Debug)]
pub struct CompiledSelection {
    verified: VerifiedSelection,
    descriptor_bytes: Vec<u8>,
    evidence: BTreeMap<ArtifactDigest, Vec<u8>>,
}
impl CompiledSelection {
    /// Independently verified selected declarations and unique required artifacts.
    pub fn verified(&self) -> &VerifiedSelection {
        &self.verified
    }
    /// Canonical descriptor to bind by digest in the new workload profile.
    pub fn descriptor_bytes(&self) -> &[u8] {
        &self.descriptor_bytes
    }
    /// Newly generated canonical root evidence. Other required bytes remain in
    /// the caller's blob store. Roots do not cause unselected blobs to be copied.
    pub fn evidence(&self) -> &BTreeMap<ArtifactDigest, Vec<u8>> {
        &self.evidence
    }
}

/// Compile exact authoring choices into selected closure evidence, then verify it
/// through the same independent function a worker uses. Raw `Selection` is never
/// trusted. The graph compiler supplies explicit instance IDs/roles; this function
/// neither invents executable uses nor selects substitutes for absent providers.
///
/// Release handles supply canonical root evidence only. The selected verifier
/// rechecks selected payload bytes/requirements even though these handles were
/// verified earlier. Source cache extras are ignored and not copied to the result.
pub fn compile(
    selection: &Selection,
    instances: &[SelectedKernel],
    releases: &[&VerifiedRelease],
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: SelectionLimits,
) -> Result<CompiledSelection, Error> {
    if selection.roots.len() > limits.contributions
        || selection.contributions.len() > limits.contributions
        || selection.bindings.len() > limits.bindings
        || instances.len() > limits.kernel_instances
        || releases.len() > limits.releases
    {
        return Err(limit("selection"));
    }
    let mut available = BTreeMap::new();
    for release in releases {
        if available.insert(release.id(), *release).is_some() {
            return Err(invalid("releaseEvidence", "duplicate supplied release"));
        }
    }
    let required: BTreeSet<_> = selection.contributions.iter().map(|c| c.release).collect();
    let mut evidence = BTreeMap::new();
    let mut release_evidence = Vec::new();
    let mut metadata = 0u64;
    for id in required {
        let root = available
            .get(&id)
            .ok_or_else(|| invalid("releaseEvidence", "selected release absent"))?
            .root();
        let bytes = root.canonical_bytes(&limits.declarations)?;
        metadata = metadata
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= limits.metadata_bytes)
            .ok_or_else(|| limit("metadata"))?;
        let artifact = Artifact {
            digest: ArtifactDigest::sha256_of(&bytes),
            size_bytes: bytes.len() as u64,
            media_type: EVIDENCE_MEDIA_TYPE.into(),
        };
        evidence.insert(artifact.digest, bytes);
        release_evidence.push(ReleaseEvidence {
            release: id,
            artifact,
        });
    }
    let mut bindings = Vec::with_capacity(selection.bindings.len());
    for binding in &selection.bindings {
        let consumer = &binding.requirement.consumer;
        let payload = available
            .get(&consumer.release)
            .and_then(|r| r.payloads().get(&consumer.local_id))
            .and_then(VerifiedPayload::payload)
            .ok_or_else(|| invalid("bindings", "consumer declaration unavailable"))?;
        let requirement = payload
            .requirements()
            .iter()
            .find(|r| r.slot == binding.requirement.slot)
            .ok_or_else(|| invalid("bindings", "consumer requirement absent"))?;
        bindings.push(SelectedBinding {
            consumer: consumer.clone(),
            requirement_slot: requirement.slot.clone(),
            provider: binding.provider.clone(),
            exact_contract: requirement.contract.clone(),
        });
    }
    let descriptor = SelectionDescriptor {
        api_version: ApiVersion::new(SELECTION_SCHEMA).expect("static schema"),
        roots: selection.roots.clone(),
        contributions: selection.contributions.clone(),
        bindings,
        kernel_instances: instances.to_vec(),
        release_evidence,
    };
    // Bound an index of selected descriptors, not all blobs in a potentially huge
    // caller cache. Root evidence remains separate from cache-controlled bytes.
    let mut selected_blobs = BTreeMap::new();
    for (digest, bytes) in &evidence {
        selected_blobs.insert(*digest, bytes.as_slice());
    }
    for c in &selection.contributions {
        let release = available
            .get(&c.release)
            .ok_or_else(|| invalid("contribution", "release absent"))?;
        let payload = release
            .payloads()
            .get(&c.local_id)
            .ok_or_else(|| invalid("contribution", "selected member absent"))?;
        for digest in
            std::iter::once(payload.digest()).chain(payload.payload().and_then(Payload::kernel))
        {
            let bytes = blobs
                .get(&digest)
                .ok_or_else(|| invalid("artifact", "selected bytes absent"))?;
            if let Some(previous) = selected_blobs.insert(digest, bytes)
                && previous != *bytes
            {
                return Err(invalid("artifact", "conflicting root and payload bytes"));
            }
        }
    }
    let descriptor_bytes = descriptor.to_cbor(limits)?;
    let verified = verify(descriptor, &selected_blobs, limits)?;
    Ok(CompiledSelection {
        verified,
        descriptor_bytes,
        evidence,
    })
}
