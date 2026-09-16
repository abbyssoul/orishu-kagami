//! Exact selected scientific closure, independent of installed plugins.
//!
//! Release roots are membership evidence, not dependency edges to every artifact
//! they describe. Only selected payloads, their exact dependencies and their
//! kernels are required. This layer checks declarations/bytes, not Wasm ABI,
//! instance state, workload topology or permission to execute.
use crate::{
    codec,
    projection::{Project, object},
    *,
};
use orishu_resource::ApiVersion;
use orishu_workload::{ComponentInstanceId, canonical::CanonicalValue as V};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Identity-bearing descriptor schema; enclosing workload profile adoption is
/// explicit, never an optional addition to the existing workload-v2 projection.
pub const SELECTION_SCHEMA: &str = "orishu.plugin-selection/v1";
/// Media type of complete canonical release-root evidence (not an archive).
pub const EVIDENCE_MEDIA_TYPE: &str = "application/vnd.orishu.plugin-release+cbor";

mod compile;
pub use compile::{CompiledSelection, compile};
mod context;
pub use context::{ContextInputs, build_context, verify_context};

/// Independent caller-owned closure admission budgets.
#[derive(Clone, Copy, Debug)]
pub struct SelectionLimits {
    /// Maximum descriptor bytes before parsing.
    pub descriptor_bytes: usize,
    /// Maximum contributions (also roots).
    pub contributions: usize,
    /// Maximum exact dependency edges.
    pub bindings: usize,
    /// Maximum configured kernel uses.
    pub kernel_instances: usize,
    /// Maximum distinct releases with selected contributions.
    pub releases: usize,
    /// Maximum unique required blobs, including release evidence.
    pub artifacts: usize,
    /// Maximum total unique required bytes, before hashing executable blobs.
    pub artifact_bytes: u64,
    /// Aggregate root and selected-payload bytes, before parsing each next one.
    pub metadata_bytes: u64,
    /// Per-release and per-payload structural bounds.
    pub declarations: Limits,
}
impl Default for SelectionLimits {
    fn default() -> Self {
        Self {
            descriptor_bytes: 1024 * 1024,
            contributions: 4096,
            bindings: 16_384,
            kernel_instances: 256,
            releases: 256,
            artifacts: 8192,
            artifact_bytes: 1024 * 1024 * 1024,
            metadata_bytes: 32 * 1024 * 1024,
            declarations: Limits::default(),
        }
    }
}
impl SelectionLimits {
    fn codec(&self) -> Limits {
        Limits {
            max_payload_bytes: self.descriptor_bytes,
            max_values: self.descriptor_bytes,
            max_depth: 16,
            max_text_bytes: 256,
            max_contributions: self.contributions,
            max_schema_items: self
                .bindings
                .max(self.contributions)
                .max(self.kernel_instances)
                .max(self.releases),
            ..Limits::default()
        }
    }
}

/// Exact dependency choice. Local dependencies also have explicit bindings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedBinding {
    /// Consuming selected declaration.
    pub consumer: ContributionRef,
    /// Scientific requirement slot on that declaration.
    pub requirement_slot: LocalContributionId,
    /// Selected provider; never resolved by name on a worker.
    pub provider: ContributionRef,
    /// Full semantic identity, checked against both ends of the edge.
    pub exact_contract: ScientificContractRef,
}
/// One configured workload use of an independently compiled kernel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedKernel {
    /// Workload graph instance, unique within the descriptor.
    pub instance_id: ComponentInstanceId,
    /// Selected executable declaration supplying that kernel.
    pub contribution: ContributionRef,
    /// Exactly one host-owned scientific execution contract.
    pub execution_contract: ExecutionContractId,
}
/// Complete canonical root proving selected contribution membership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseEvidence {
    /// Content identity recomputed from the canonical root, not publisher trust.
    pub release: PluginReleaseId,
    /// Exact root bytes only; its artifact list is not a workload dependency list.
    pub artifact: Artifact,
}
/// Raw descriptor, not admission evidence until independently verified.
/// All sets are strictly sorted; binding order is (consumer, requirementSlot),
/// kernel order is instanceId, and evidence order is release identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionDescriptor {
    /// Exactly [`SELECTION_SCHEMA`].
    pub api_version: ApiVersion,
    /// Explicitly used scientific contributions, before dependency expansion.
    pub roots: Vec<ContributionRef>,
    /// Exact reachable transitive set, with no dormant/unrelated contributions.
    pub contributions: Vec<ContributionRef>,
    /// Exactly one entry for every requirement of every selected contribution.
    pub bindings: Vec<SelectedBinding>,
    /// Configured executable uses; enclosing workload verifies their graph/state.
    pub kernel_instances: Vec<SelectedKernel>,
    /// Exactly the releases owning the selected contributions.
    pub release_evidence: Vec<ReleaseEvidence>,
}
fn invalid(path: &str, message: &str) -> Error {
    Error::new(ErrorCode::InvalidSelection, path, message)
}
fn limit(path: &str) -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        path,
        "selected closure budget exceeded",
    )
}
fn sorted<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut previous = None;
    for value in values {
        if previous.as_ref().is_some_and(|p| p >= &value) {
            return false;
        }
        previous = Some(value);
    }
    true
}
impl SelectionDescriptor {
    /// Structural bounds and canonical set order, without fetching any bytes.
    pub fn validate(&self, limits: SelectionLimits) -> Result<(), Error> {
        if self.api_version.as_str() != SELECTION_SCHEMA {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "apiVersion",
                "unsupported selection version",
            ));
        }
        for (count, max, path) in [
            (self.roots.len(), limits.contributions, "roots"),
            (
                self.contributions.len(),
                limits.contributions,
                "contributions",
            ),
            (self.bindings.len(), limits.bindings, "bindings"),
            (
                self.kernel_instances.len(),
                limits.kernel_instances,
                "kernelInstances",
            ),
            (
                self.release_evidence.len(),
                limits.releases,
                "releaseEvidence",
            ),
        ] {
            if count > max {
                return Err(limit(path));
            }
        }
        if !sorted(&self.roots)
            || !sorted(&self.contributions)
            || !sorted(
                self.bindings
                    .iter()
                    .map(|b| (&b.consumer, &b.requirement_slot)),
            )
            || !sorted(self.kernel_instances.iter().map(|k| &k.instance_id))
            || !sorted(self.release_evidence.iter().map(|e| e.release))
        {
            return Err(invalid("selection", "duplicate or noncanonical set order"));
        }
        let has = |c: &ContributionRef| self.contributions.binary_search(c).is_ok();
        if self.roots.iter().any(|c| !has(c))
            || self
                .bindings
                .iter()
                .any(|b| !has(&b.consumer) || !has(&b.provider))
            || self.kernel_instances.iter().any(|k| !has(&k.contribution))
        {
            return Err(invalid(
                "selection",
                "reference outside selected contribution set",
            ));
        }
        let releases: BTreeSet<_> = self.contributions.iter().map(|c| c.release).collect();
        if releases.len() != self.release_evidence.len()
            || self
                .release_evidence
                .iter()
                .any(|e| !releases.contains(&e.release))
        {
            return Err(invalid(
                "releaseEvidence",
                "evidence does not match selected releases",
            ));
        }
        for e in &self.release_evidence {
            if e.artifact.media_type != EVIDENCE_MEDIA_TYPE
                || e.artifact.digest.as_bytes() != e.release.as_bytes()
            {
                return Err(invalid(
                    "releaseEvidence",
                    "root artifact identity or media type mismatch",
                ));
            }
            if e.artifact.size_bytes > limits.declarations.max_manifest_bytes as u64 {
                return Err(limit("releaseEvidence"));
            }
        }
        Ok(())
    }
    /// Canonical bytes after structural validation. Does not prove closure.
    pub fn to_cbor(&self, limits: SelectionLimits) -> Result<Vec<u8>, Error> {
        self.validate(limits)?;
        codec::encode(&self.project(), &limits.codec(), limits.descriptor_bytes)
    }
    /// Bounded canonical reader. Rejects unknown fields/versions, duplicate sets,
    /// noncanonical ordering and references outside the declared contribution set.
    pub fn from_cbor(bytes: &[u8], limits: SelectionLimits) -> Result<Self, Error> {
        let value: Self =
            codec::structured_from_cbor(bytes, &limits.codec(), limits.descriptor_bytes)?;
        if value.to_cbor(limits)? != bytes {
            return Err(invalid("selection", "noncanonical selection descriptor"));
        }
        Ok(value)
    }
}

/// Verified selected declarations and required byte descriptors. This witness
/// proves exact closure only, never executable ABI, state validity or admission.
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedSelection {
    descriptor: SelectionDescriptor,
    payloads: BTreeMap<ContributionRef, Payload>,
    artifacts: BTreeMap<ArtifactDigest, Artifact>,
    releases: BTreeMap<PluginReleaseId, ReleaseMetadata>,
    evidence: BTreeMap<PluginReleaseId, Release>,
    metadata_bytes: u64,
}
impl VerifiedSelection {
    /// Declaration-only witness for restoring authored state offline. This does
    /// not grant executable admission and cannot be converted back without bytes.
    pub fn declarations(&self) -> VerifiedDeclarations {
        VerifiedDeclarations {
            selection: self.clone(),
        }
    }
    /// Exact release provenance recovered from independently hashed root evidence.
    pub fn releases(&self) -> &BTreeMap<PluginReleaseId, ReleaseMetadata> {
        &self.releases
    }
    /// Exact independently checked descriptor.
    pub fn descriptor(&self) -> &SelectionDescriptor {
        &self.descriptor
    }
    /// Only selected understood scientific declarations, not dormant payloads.
    pub fn payloads(&self) -> &BTreeMap<ContributionRef, Payload> {
        &self.payloads
    }
    /// Unique required bytes: release roots, selected payloads and selected code.
    /// Does not include this descriptor itself or workload state/input resources.
    pub fn artifacts(&self) -> &BTreeMap<ArtifactDigest, Artifact> {
        &self.artifacts
    }
}

/// Exact selected vocabulary and release membership, without executable bytes.
/// An experiment may reopen while a plugin is uninstalled. Workload compilation
/// and runtime admission still require [`verify`] and a [`VerifiedSelection`].
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedDeclarations {
    selection: VerifiedSelection,
}
impl VerifiedDeclarations {
    /// Canonical release/payload bytes charged by independent verification.
    /// A logical retention weight, not a measurement of Rust heap overhead.
    pub const fn metadata_bytes(&self) -> u64 {
        self.selection.metadata_bytes
    }
    /// Verified selection intent, not evidence that its code is available.
    pub fn descriptor(&self) -> &SelectionDescriptor {
        self.selection.descriptor()
    }
    /// Exact selected scientific declarations.
    pub fn payloads(&self) -> &BTreeMap<ContributionRef, Payload> {
        self.selection.payloads()
    }
    /// Verify context metadata without implying that code is installed/admitted.
    pub fn verify_context(
        &self,
        context: &crate::execution::InstanceContext,
        limits: &Limits,
    ) -> Result<(), Error> {
        verify_context(&self.selection, context, limits)
    }
    /// Canonical release evidence and selected payloads only. No executable or
    /// unrelated artifact is copied into the returned self-contained metadata.
    pub fn blobs(&self, limits: &Limits) -> Result<BTreeMap<ArtifactDigest, Vec<u8>>, Error> {
        let mut blobs = BTreeMap::new();
        for root in self.selection.evidence.values() {
            let bytes = root.canonical_bytes(limits)?;
            blobs.insert(ArtifactDigest::sha256_of(&bytes), bytes);
        }
        for payload in self.payloads().values() {
            let bytes = payload.canonical_bytes(limits)?;
            blobs.insert(ArtifactDigest::sha256_of(&bytes), bytes);
        }
        Ok(blobs)
    }
}

/// Verify selected declarations and release evidence without requiring kernel
/// bytes. The result deliberately cannot satisfy execution/compilation APIs.
/// All graph, provider, schema and declared artifact budgets still apply.
pub fn verify_declarations(
    descriptor: SelectionDescriptor,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: SelectionLimits,
) -> Result<VerifiedDeclarations, Error> {
    verify_inner(descriptor, blobs, limits, false)
        .map(|selection| VerifiedDeclarations { selection })
}
struct Budget {
    artifacts: BTreeMap<ArtifactDigest, Artifact>,
    bytes: u64,
    metadata: u64,
    limits: SelectionLimits,
}
impl Budget {
    fn add(&mut self, artifact: &Artifact) -> Result<(), Error> {
        if let Some(old) = self.artifacts.get(&artifact.digest) {
            if old != artifact {
                return Err(invalid(
                    "artifact",
                    "conflicting descriptors for identical bytes",
                ));
            }
            return Ok(());
        }
        if self.artifacts.len() >= self.limits.artifacts
            || artifact.size_bytes > self.limits.declarations.max_artifact_bytes
        {
            return Err(limit("artifacts"));
        }
        self.bytes = self
            .bytes
            .checked_add(artifact.size_bytes)
            .filter(|n| *n <= self.limits.artifact_bytes)
            .ok_or_else(|| limit("artifacts"))?;
        self.artifacts.insert(artifact.digest, artifact.clone());
        Ok(())
    }
    fn metadata(&mut self, bytes: u64) -> Result<(), Error> {
        self.metadata = self
            .metadata
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.metadata_bytes)
            .ok_or_else(|| limit("metadata"))?;
        Ok(())
    }
}
fn bytes<'a>(a: &Artifact, blobs: &BTreeMap<ArtifactDigest, &'a [u8]>) -> Result<&'a [u8], Error> {
    let bytes = blobs
        .get(&a.digest)
        .ok_or_else(|| invalid("artifact", "required bytes absent"))?;
    if bytes.len() as u64 != a.size_bytes || !a.digest.matches(bytes) {
        return Err(Error::new(
            ErrorCode::IntegrityMismatch,
            "artifact",
            "size or digest mismatch",
        ));
    }
    Ok(bytes)
}

/// Independently verify selected closure from caller-owned digest-addressed bytes.
/// No inventory, resolver, IO or executable engine is consulted. Unknown selected
/// points fail closed; unknown unselected payload bytes need not even be present.
/// Extras in the supplied cache are ignored, never activated or required.
/// Graph walks are iterative O(V + E). Declaration lookup additionally scans
/// bounded per-release lists; schema validation and channel compatibility are
/// bounded by the caller's per-declaration limits. No search for providers occurs.
pub fn verify(
    descriptor: SelectionDescriptor,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: SelectionLimits,
) -> Result<VerifiedSelection, Error> {
    verify_inner(descriptor, blobs, limits, true)
}

fn verify_inner(
    descriptor: SelectionDescriptor,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: SelectionLimits,
    require_code: bool,
) -> Result<VerifiedSelection, Error> {
    descriptor.validate(limits)?;
    // Also bound raw in-memory descriptors before allocating graph indexes.
    descriptor.to_cbor(limits)?;
    let mut budget = Budget {
        artifacts: BTreeMap::new(),
        bytes: 0,
        metadata: 0,
        limits,
    };
    let mut roots = BTreeMap::new();
    for e in &descriptor.release_evidence {
        budget.add(&e.artifact)?;
        budget.metadata(e.artifact.size_bytes)?;
        let root = release_from_cbor(bytes(&e.artifact, blobs)?, &limits.declarations)?;
        if root.release_id(&limits.declarations)? != e.release {
            return Err(invalid("releaseEvidence", "release identity mismatch"));
        }
        roots.insert(e.release, root);
    }
    let validated: BTreeMap<_, _> = roots
        .iter()
        .map(|(id, root)| root.validate(&limits.declarations).map(|r| (*id, r)))
        .collect::<Result<_, _>>()?;
    let mut payloads = BTreeMap::new();
    let mut contracts = BTreeMap::new();
    let mut kernels = BTreeMap::new();
    for c in &descriptor.contributions {
        let root = &roots[&c.release];
        let declaration = root
            .0
            .spec
            .contributions
            .iter()
            .find(|d| d.local_id == c.local_id && d.extension_point == c.extension_point)
            .ok_or_else(|| {
                invalid(
                    "contribution",
                    "selected member absent from release evidence",
                )
            })?;
        if KnownPoint::from_id(&c.extension_point).is_none() {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "contribution",
                "selected extension point unsupported",
            ));
        }
        let artifact = root
            .0
            .spec
            .artifacts
            .iter()
            .find(|a| a.digest == declaration.payload)
            .expect("validated release has payload descriptor");
        if artifact.size_bytes > limits.declarations.max_payload_bytes as u64 {
            return Err(limit("payload"));
        }
        budget.add(artifact)?;
        budget.metadata(artifact.size_bytes)?;
        let payload = validated[&c.release]
            .verify_payload(&c.local_id, bytes(artifact, blobs)?)?
            .payload()
            .expect("known point produces a payload")
            .clone();
        if let Some(digest) = payload.kernel() {
            let contract =
                kernel_contract(&payload).expect("only executable payloads have kernels");
            if kernels
                .insert(digest, contract)
                .is_some_and(|old| old != contract)
            {
                return Err(invalid(
                    "kernel",
                    "one artifact declares two execution contracts",
                ));
            }
            budget.add(
                root.0
                    .spec
                    .artifacts
                    .iter()
                    .find(|a| a.digest == digest)
                    .expect("verified payload has kernel descriptor"),
            )?;
        }
        contracts.insert(c.clone(), payload.contract_ref(&limits.declarations)?);
        payloads.insert(c.clone(), payload);
    }
    let bindings: BTreeMap<_, _> = descriptor
        .bindings
        .iter()
        .map(|b| ((&b.consumer, &b.requirement_slot), b))
        .collect();
    let mut required_edges = 0usize;
    for (c, p) in &payloads {
        required_edges = required_edges
            .checked_add(p.requirements().len())
            .filter(|n| *n <= limits.bindings)
            .ok_or_else(|| limit("bindings"))?;
        let declaration = roots[&c.release]
            .0
            .spec
            .contributions
            .iter()
            .find(|d| d.local_id == c.local_id)
            .expect("checked member");
        for requirement in p.requirements() {
            let b = bindings
                .get(&(c, &requirement.slot))
                .ok_or_else(|| invalid("bindings", "required exact binding absent"))?;
            if b.exact_contract != requirement.contract
                || contracts[&b.provider] != b.exact_contract
            {
                return Err(invalid(
                    "bindings",
                    "scientific identity differs at dependency edge",
                ));
            }
            if let Some(Requirement::Local {
                local_contribution, ..
            }) = declaration
                .requirements
                .iter()
                .find(|r| r.slot() == &requirement.slot)
                && (b.provider.release != c.release || &b.provider.local_id != local_contribution)
            {
                return Err(invalid(
                    "bindings",
                    "local dependency rebound to another provider",
                ));
            }
        }
    }
    if required_edges != bindings.len() {
        return Err(invalid("bindings", "unexpected dependency binding"));
    }
    check_graph(&descriptor)?;
    check_roles(&payloads, &bindings, &contracts)?;
    for k in &descriptor.kernel_instances {
        if kernel_contract(&payloads[&k.contribution]) != Some(k.execution_contract) {
            return Err(invalid(
                "kernelInstances",
                "instance does not match executable declaration",
            ));
        }
    }
    let used: BTreeSet<_> = descriptor
        .kernel_instances
        .iter()
        .map(|k| &k.contribution)
        .collect();
    if payloads
        .iter()
        .any(|(c, p)| p.kernel().is_some() && !used.contains(c))
    {
        return Err(invalid(
            "kernelInstances",
            "selected executable has no configured use",
        ));
    }
    // Full selected byte verification happens only after the required aggregate
    // size/count is known. Never traverse root artifacts as workload dependencies.
    for artifact in budget.artifacts.values() {
        if require_code || !kernels.contains_key(&artifact.digest) {
            bytes(artifact, blobs)?;
        }
    }
    Ok(VerifiedSelection {
        descriptor,
        payloads,
        artifacts: budget.artifacts,
        releases: roots
            .iter()
            .map(|(id, root)| (*id, root.0.metadata.clone()))
            .collect(),
        evidence: roots,
        metadata_bytes: budget.metadata,
    })
}
fn kernel_contract(p: &Payload) -> Option<ExecutionContractId> {
    match p {
        Payload::FieldModels(v) => Some(v.scientific.execution_contract),
        Payload::Integrators(v) => Some(v.scientific.execution_contract),
        _ => None,
    }
}
fn check_graph(d: &SelectionDescriptor) -> Result<(), Error> {
    let index: BTreeMap<_, _> = d
        .contributions
        .iter()
        .enumerate()
        .map(|(i, c)| (c, i))
        .collect();
    let mut edges = vec![Vec::new(); index.len()];
    let mut incoming = vec![0usize; index.len()];
    for b in &d.bindings {
        let to = index[&b.provider];
        edges[index[&b.consumer]].push(to);
        incoming[to] += 1;
    }
    let mut seen = vec![false; index.len()];
    let mut pending = Vec::new();
    for c in &d.roots {
        let i = index[c];
        seen[i] = true;
        pending.push(i);
    }
    while let Some(i) = pending.pop() {
        for &j in &edges[i] {
            if !seen[j] {
                seen[j] = true;
                pending.push(j);
            }
        }
    }
    if seen.iter().any(|v| !v) {
        return Err(invalid("contributions", "unreachable extra contribution"));
    }
    pending.extend(
        incoming
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (*n == 0).then_some(i)),
    );
    let mut visited = 0;
    while let Some(i) = pending.pop() {
        visited += 1;
        for &j in &edges[i] {
            incoming[j] -= 1;
            if incoming[j] == 0 {
                pending.push(j);
            }
        }
    }
    if visited != index.len() {
        return Err(invalid("bindings", "cyclic scientific dependencies"));
    }
    Ok(())
}
type Bindings<'a> = BTreeMap<(&'a ContributionRef, &'a LocalContributionId), &'a SelectedBinding>;
fn check_roles(
    payloads: &BTreeMap<ContributionRef, Payload>,
    bindings: &Bindings<'_>,
    contracts: &BTreeMap<ContributionRef, ScientificContractRef>,
) -> Result<(), Error> {
    for (c, p) in payloads {
        let provider = |slot: &LocalContributionId| &bindings[&(c, slot)].provider;
        let observable = |slot: &LocalContributionId| {
            matches!(payloads[provider(slot)], Payload::Observables(_))
        };
        let valid = match p {
            Payload::Fields(v) => v.scientific.required_observables.iter().all(observable),
            Payload::FieldModels(v) => {
                let s = &v.scientific;
                let field = provider(&s.field);
                let Payload::Fields(f) = &payloads[field] else {
                    return Err(invalid(
                        "field",
                        "field model requires a field-family provider",
                    ));
                };
                s.couplings.iter().all(|slot| {
                    matches!(&payloads[provider(slot)],
                    Payload::Components(v) if v.scientific.role == ComponentRole::FieldCoupling)
                }) && s.observables.iter().all(observable)
                    && f.scientific.required_observables.iter().all(|slot| {
                        let required = &contracts[&bindings[&(field, slot)].provider];
                        s.observables
                            .iter()
                            .any(|slot| &contracts[provider(slot)] == required)
                    })
            }
            Payload::Integrators(v) => matches!(&payloads[provider(&v.scientific.dynamics)],
                Payload::Components(v) if v.scientific.role == ComponentRole::Dynamics),
            _ => true,
        };
        if !valid {
            return Err(invalid(
                "requirements",
                "provider role or required field channels incompatible",
            ));
        }
    }
    Ok(())
}

impl Project for SelectedBinding {
    fn project(&self) -> V {
        object(vec![
            ("consumer", Some(self.consumer.project())),
            ("requirementSlot", Some(self.requirement_slot.project())),
            ("provider", Some(self.provider.project())),
            ("exactContract", Some(self.exact_contract.project())),
        ])
    }
}
impl Project for SelectedKernel {
    fn project(&self) -> V {
        object(vec![
            ("instanceId", Some(V::text(self.instance_id.as_str()))),
            ("contribution", Some(self.contribution.project())),
            ("executionContract", Some(self.execution_contract.project())),
        ])
    }
}
impl Project for ReleaseEvidence {
    fn project(&self) -> V {
        object(vec![
            ("release", Some(self.release.project())),
            ("artifact", Some(self.artifact.project())),
        ])
    }
}
impl Project for SelectionDescriptor {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("roots", Some(self.roots.project())),
            ("contributions", Some(self.contributions.project())),
            ("bindings", Some(self.bindings.project())),
            ("kernelInstances", Some(self.kernel_instances.project())),
            ("releaseEvidence", Some(self.release_evidence.project())),
        ])
    }
}
