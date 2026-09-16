//! Fixed scientific profile assembly and independent workload-v3 verification.
//! No installed-plugin lookup, IO, Wasm execution or numerical initialization.
use crate::{execution::*, selected::*, *};
use orishu_workload::{self as w, v3};
use std::collections::{BTreeMap, BTreeSet};
pub mod bundle;
mod graph;
mod scene;

/// Exact first-pass graph/profile discriminator. Unknown profiles fail closed.
pub const PROFILE: &str = "orishu.force-then-integrate/v1";
/// Selection/execution descriptor encoding in this profile.
pub const DESCRIPTOR_MEDIA: &str = "application/cbor";
/// Non-code captured input role; schema/count remain in exact InputIdentity uses.
pub const INPUT_ROLE: &str = "scientific-input";

/// Independent policy bounds. Numerical/kernel/store budgets additionally apply.
#[derive(Clone, Copy, Debug)]
pub struct ProfileLimits {
    /// Root graph, descriptor, streaming hash and codec budgets.
    pub workload: w::Limits,
    /// Exact scientific selection and declaration budgets.
    pub selection: SelectionLimits,
    /// Maximum independently modeled fields.
    pub fields: usize,
    /// Complete object count, including static objects.
    pub objects: usize,
    /// Coupling records across all fields.
    pub couplings: usize,
    /// Sum of captured input uses, including shared/repeated uses.
    pub input_bytes: u64,
    /// Maximum bytes of any captured state or input.
    pub state_bytes: u64,
    /// Domain discretization product budget.
    pub domain: DomainLimits,
    /// Initial entity composition and retained authoring provenance bounds.
    pub scene: SceneLimits,
}
impl Default for ProfileLimits {
    fn default() -> Self {
        Self {
            workload: w::Limits::DEFAULT,
            selection: SelectionLimits::default(),
            fields: 64,
            objects: 1_000_000,
            couplings: 4_000_000,
            input_bytes: 512 * 1024 * 1024,
            state_bytes: 128 * 1024 * 1024,
            domain: DomainLimits::default(),
            scene: SceneLimits::default(),
        }
    }
}
/// Typed refusal; errors retain structural/codec detail rather than flattening it.
#[derive(Debug, thiserror::Error)]
pub enum WorkloadError {
    /// Scientific/profile/selection refusal with bounded domain diagnostic.
    #[error(transparent)]
    Scientific(#[from] Error),
    /// Root structural or streamed-byte verification failed.
    #[error(transparent)]
    Closure(#[from] w::ClosureReport),
    /// Root canonical encoding/decoding failed.
    #[error(transparent)]
    Canonical(#[from] w::CanonicalError),
}
fn invalid(path: &str, message: &str) -> Error {
    Error::new(ErrorCode::InvalidSelection, path, message)
}
fn limited(path: &str) -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        path,
        "scientific profile budget exceeded",
    )
}
fn name<T: std::str::FromStr>(s: &str) -> Result<T, Error> {
    s.parse()
        .map_err(|_| invalid("identity", "profile identity is not representable"))
}
fn artifact(
    role: &str,
    digest: ArtifactDigest,
    size_bytes: u64,
    media: &str,
) -> Result<w::ArtifactDescriptor, Error> {
    Ok(w::ArtifactDescriptor {
        role: name(role)?,
        digest,
        size_bytes,
        media_type: name(media)?,
        schema: None,
    })
}
fn descriptor(role: &str, bytes: &[u8]) -> Result<w::ArtifactDescriptor, Error> {
    artifact(
        role,
        ArtifactDigest::sha256_of(bytes),
        bytes.len() as u64,
        DESCRIPTOR_MEDIA,
    )
}
fn blob<'a>(
    digest: ArtifactDigest,
    size: u64,
    blobs: &BTreeMap<ArtifactDigest, &'a [u8]>,
) -> Result<&'a [u8], Error> {
    let bytes = blobs
        .get(&digest)
        .ok_or_else(|| invalid("artifact", "required bytes absent"))?;
    if bytes.len() as u64 != size || !digest.matches(bytes) {
        return Err(Error::new(
            ErrorCode::IntegrityMismatch,
            "artifact",
            "size or digest mismatch",
        ));
    }
    Ok(bytes)
}
struct Source<'a, 'b>(&'a BTreeMap<ArtifactDigest, &'b [u8]>);
impl w::BlobSource for Source<'_, '_> {
    fn read_blob(&self, digest: &ArtifactDigest, verifier: &mut w::BlobVerifier<'_>) -> bool {
        self.0.get(digest).is_some_and(|bytes| {
            verifier.write(bytes);
            true
        })
    }
}

/// Complete independently checked fixed-profile declarations and captured bytes.
/// This does not prove Wasm ABI, numerical admissibility, execution permission or
/// run identity; the runtime admission adapter checks those before starting a run.
#[derive(Debug)]
pub struct VerifiedWorkload {
    manifest: v3::WorkloadManifest,
    root: w::WorkloadDigest,
    selection: VerifiedSelection,
    execution: ExecutionDefinition,
    contexts: BTreeMap<w::ComponentInstanceId, InstanceContext>,
    scene: Option<SceneDefinition>,
}
impl VerifiedWorkload {
    /// Immutable workload root identity.
    pub fn root(&self) -> w::WorkloadDigest {
        self.root
    }
    /// Exact root graph and byte descriptors.
    pub fn manifest(&self) -> &v3::WorkloadManifest {
        &self.manifest
    }
    /// Exact selected scientific closure and release provenance.
    pub fn selection(&self) -> &VerifiedSelection {
        &self.selection
    }
    /// Captured initial boundary, never initialized by admission.
    pub fn execution(&self) -> &ExecutionDefinition {
        &self.execution
    }
    /// Contexts verified against selected declarations and captured input bytes.
    pub fn contexts(&self) -> &BTreeMap<w::ComponentInstanceId, InstanceContext> {
        &self.contexts
    }
    /// Independently verified initial entity composition and source evidence.
    /// `None` denotes the older numeric-only execution descriptor.
    pub fn scene(&self) -> Option<&SceneDefinition> {
        self.scene.as_ref()
    }
}
/// Pure export result. Only small newly generated descriptor/evidence blobs are
/// owned; code and scientific inputs remain in the caller's content-addressed store.
#[derive(Debug)]
pub struct CompiledWorkload {
    verified: VerifiedWorkload,
    generated: BTreeMap<ArtifactDigest, Vec<u8>>,
    bytes: Vec<u8>,
}
impl CompiledWorkload {
    /// Checked root and scientific definition.
    pub fn verified(&self) -> &VerifiedWorkload {
        &self.verified
    }
    /// Canonical v3 manifest, suitable for hashing/submission.
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Selected evidence/selection/execution blobs generated during export.
    pub fn generated(&self) -> &BTreeMap<ArtifactDigest, Vec<u8>> {
        &self.generated
    }
}

/// Compile a canonical fixed-profile root, then independently verify the same
/// bytes/declarations a worker will receive. Never reevaluate defaults or link code.
pub fn compile(
    metadata: w::WorkloadMeta,
    selection: &CompiledSelection,
    execution: &ExecutionDefinition,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: ProfileLimits,
) -> Result<CompiledWorkload, WorkloadError> {
    selection
        .verified()
        .descriptor()
        .validate(limits.selection)?;
    let mut evidence_bytes = 0u64;
    for bytes in selection.evidence().values() {
        evidence_bytes = evidence_bytes
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= limits.selection.metadata_bytes)
            .ok_or_else(|| limited("evidence"))?;
    }
    let execution_bytes = execution.to_cbor(limits.fields)?;
    let selection_descriptor = descriptor(v3::SELECTION_ROLE, selection.descriptor_bytes())?;
    let execution_descriptor = descriptor(v3::EXECUTION_ROLE, &execution_bytes)?;
    let mut generated = selection.evidence().clone();
    generated.insert(
        selection_descriptor.digest,
        selection.descriptor_bytes().to_vec(),
    );
    generated.insert(execution_descriptor.digest, execution_bytes);
    let mut source = required_source(selection.verified(), execution, blobs, &generated, limits)?;
    let checked = check_execution(selection.verified(), execution, &source, limits)?;
    let compute = graph::build(selection.verified(), execution, &checked.contexts)?;
    let artifacts = additional(selection.verified(), &checked.inputs, &compute)?;
    let manifest = v3::manifest(
        metadata,
        v3::WorkloadSpec {
            compute,
            selection: selection_descriptor,
            execution: execution_descriptor,
            artifacts,
            requirements: w::WorkloadRequirements::default(),
        },
    );
    // Keep only references; generated evidence and caller bytes remain distinct.
    source.extend(generated.iter().map(|(d, b)| (*d, b.as_slice())));
    let verified = verify(manifest, &source, limits)?;
    let bytes = v3::canonical_bytes(verified.manifest(), &limits.workload)?;
    Ok(CompiledWorkload {
        verified,
        generated,
        bytes,
    })
}

/// Independent fixed-profile admission, operating only on root and exact blobs.
/// Unknown profiles, changes to the fixed graph, extra closure members and absent
/// selected/captured state fail closed. Actual guest admission is a separate step.
pub fn verify(
    manifest: v3::WorkloadManifest,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: ProfileLimits,
) -> Result<VerifiedWorkload, WorkloadError> {
    if manifest.spec.compute.workload_graph_profile.as_str() != PROFILE
        || manifest.spec.compute.step_plan.profile.as_str() != PROFILE
    {
        return Err(Error::new(
            ErrorCode::UnsupportedVersion,
            "profile",
            "unsupported scientific graph profile",
        )
        .into());
    }
    for (a, max) in [
        (&manifest.spec.selection, limits.selection.descriptor_bytes),
        (&manifest.spec.execution, MAX_EXECUTION_BYTES),
    ] {
        if a.size_bytes > max as u64 {
            return Err(limited("descriptor").into());
        }
        if a.media_type.as_str() != DESCRIPTOR_MEDIA || a.schema.is_some() {
            return Err(invalid(
                "descriptor",
                "unexpected descriptor media or schema annotation",
            )
            .into());
        }
    }
    let closure = v3::validate_closure(&manifest, &Source(blobs), &limits.workload)?;
    let selection = crate::selected::verify(
        SelectionDescriptor::from_cbor(
            blob(
                manifest.spec.selection.digest,
                manifest.spec.selection.size_bytes,
                blobs,
            )?,
            limits.selection,
        )?,
        blobs,
        limits.selection,
    )?;
    let execution = ExecutionDefinition::from_cbor(
        blob(
            manifest.spec.execution.digest,
            manifest.spec.execution.size_bytes,
            blobs,
        )?,
        limits.fields,
    )?;
    let checked = check_execution(&selection, &execution, blobs, limits)?;
    let expected = graph::build(&selection, &execution, &checked.contexts)?;
    if expected != manifest.spec.compute {
        return Err(invalid(
            "compute",
            "graph differs from fixed scientific execution profile",
        )
        .into());
    }
    if additional(&selection, &checked.inputs, &expected)? != manifest.spec.artifacts {
        return Err(invalid(
            "artifacts",
            "root does not contain exactly selected scientific closure",
        )
        .into());
    }
    Ok(VerifiedWorkload {
        root: closure.root(),
        manifest,
        selection,
        execution,
        contexts: checked.contexts,
        scene: checked.scene,
    })
}

struct Inputs<'a, 'b> {
    blobs: &'a BTreeMap<ArtifactDigest, &'b [u8]>,
    limits: ProfileLimits,
    total: u64,
    descriptors: BTreeMap<ArtifactDigest, w::ArtifactDescriptor>,
}
impl<'b> Inputs<'_, 'b> {
    fn read(&mut self, input: &InputIdentity, max: u64) -> Result<&'b [u8], Error> {
        if input.byte_length > max.min(self.limits.state_bytes) {
            return Err(limited("input"));
        }
        self.total = self
            .total
            .checked_add(input.byte_length)
            .filter(|n| *n <= self.limits.input_bytes)
            .ok_or_else(|| limited("inputs"))?;
        let a = artifact(
            INPUT_ROLE,
            input.digest,
            input.byte_length,
            "application/octet-stream",
        )?;
        if self
            .descriptors
            .insert(input.digest, a.clone())
            .is_some_and(|old| old != a)
        {
            return Err(invalid("input", "conflicting captured input lengths"));
        }
        blob(input.digest, input.byte_length, self.blobs)
    }
}
struct CheckedExecution {
    contexts: BTreeMap<w::ComponentInstanceId, InstanceContext>,
    inputs: BTreeMap<ArtifactDigest, w::ArtifactDescriptor>,
    scene: Option<SceneDefinition>,
}
fn packet<'a, R: BulkRecord>(
    input: &InputIdentity,
    bytes: &'a [u8],
    limit: usize,
) -> Result<Batch<'a, R>, Error> {
    if input.schema.as_str() != R::SCHEMA {
        return Err(invalid("packet", "captured packet schema mismatch"));
    }
    let batch = Batch::<R>::read(
        bytes,
        BulkLimits {
            bytes: bytes.len(),
            records: limit,
        },
    )
    .map_err(|_| invalid("packet", "invalid or over-budget captured packet"))?;
    if batch.iter().len() as u64 != input.value_count {
        return Err(invalid("packet", "captured packet count mismatch"));
    }
    Ok(batch)
}
fn check_execution(
    selected: &VerifiedSelection,
    e: &ExecutionDefinition,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: ProfileLimits,
) -> Result<CheckedExecution, Error> {
    e.to_cbor(limits.fields)?;
    if selected.descriptor().kernel_instances.len() != e.fields.len() + 1 {
        return Err(invalid(
            "instances",
            "selected and captured executable coverage differs",
        ));
    }
    let mut input = Inputs {
        blobs,
        limits,
        total: 0,
        descriptors: BTreeMap::new(),
    };
    let scene = e
        .scene
        .as_ref()
        .map(|s| {
            SceneDefinition::from_cbor(input.read(s, limits.scene.bytes as u64)?, limits.scene)
        })
        .transpose()?;
    // Executable uses, actual scene components and their transitive scientific
    // requirements are the only roots permitted by this profile.
    let mut dependencies: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for binding in &selected.descriptor().bindings {
        dependencies
            .entry(&binding.consumer)
            .or_default()
            .push(&binding.provider);
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<_> = selected
        .descriptor()
        .kernel_instances
        .iter()
        .map(|k| &k.contribution)
        .collect();
    if let Some(scene) = &scene {
        pending.extend(
            scene
                .objects
                .iter()
                .flat_map(|o| &o.components)
                .map(|c| &c.contribution),
        );
    }
    while let Some(c) = pending.pop() {
        if reached.insert(c) {
            pending.extend(dependencies.get(c).into_iter().flatten().copied());
        }
    }
    if reached.len() != selected.descriptor().contributions.len()
        || reached
            .iter()
            .any(|c| !selected.payloads().contains_key(*c))
    {
        return Err(invalid(
            "selection",
            "selected vocabulary is unused by this execution profile",
        ));
    }
    let objects: BTreeMap<_, _> = packet::<ObjectState>(
        &e.objects,
        input.read(&e.objects, limits.state_bytes)?,
        limits.objects,
    )?
    .iter()
    .map(|o| (o.id, o))
    .collect();
    let mut contexts = BTreeMap::new();
    let mut geometry = None;
    let mut families = BTreeSet::new();
    for (k, role) in e
        .fields
        .iter()
        .map(|f| (&f.kernel, ExecutionContractId::Field))
        .chain([(&e.dynamics, ExecutionContractId::Dynamics)])
    {
        let c = InstanceContext::from_cbor(input.read(&k.context, MAX_CONTEXT_BYTES as u64)?)?;
        if c.instance != k.instance || c.execution_contract != role {
            return Err(invalid(
                "instance",
                "captured context role/identity mismatch",
            ));
        }
        verify_context(selected, &c, &limits.selection.declarations)?;
        let p = &selected.payloads()[&c.contribution];
        let properties = match p {
            Payload::FieldModels(v) => {
                let family = selected
                    .descriptor()
                    .bindings
                    .iter()
                    .find(|b| {
                        b.consumer == c.contribution && b.requirement_slot == v.scientific.field
                    })
                    .expect("verified field requirement");
                if !families.insert(family.exact_contract.clone()) {
                    return Err(invalid("field", "multiple models govern one field family"));
                }
                &v.scientific.configuration
            }
            Payload::Integrators(v) => {
                let provider = selected
                    .descriptor()
                    .bindings
                    .iter()
                    .find(|b| {
                        b.consumer == c.contribution && b.requirement_slot == v.scientific.dynamics
                    })
                    .expect("verified dynamics requirement");
                let Payload::Components(component) = &selected.payloads()[&provider.provider]
                else {
                    unreachable!("verified dynamics component")
                };
                let mass = component
                    .scientific
                    .bindings
                    .inertial_mass
                    .as_ref()
                    .expect("verified dynamics mass binding");
                let bounds = quantity_bounds(&component.scientific, mass);
                for value in objects.values().filter_map(|o| o.inertial_mass_kilograms) {
                    quantity(bounds, value)?;
                }
                &v.scientific.configuration
            }
            _ => unreachable!("verified executable context"),
        };
        if c.configuration.schema.as_str() != CONFIGURATION_SCHEMA
            || c.configuration.value_count != 1
            || c.domain.schema.as_str() != DOMAIN_SCHEMA
            || c.domain.value_count != 1
        {
            return Err(invalid(
                "configuration",
                "unsupported configuration/domain descriptor",
            ));
        }
        ResolvedConfiguration::from_cbor(
            input.read(
                &c.configuration,
                limits.selection.declarations.max_payload_bytes as u64,
            )?,
            &limits.selection.declarations,
        )?
        .validate_against(properties, &limits.selection.declarations)?;
        let domain = DomainDescriptor::from_cbor(
            input.read(&c.domain, MAX_DOMAIN_BYTES as u64)?,
            limits.domain,
        )?;
        let bounds = (domain.lower_metres, domain.upper_metres);
        if geometry
            .replace(bounds)
            .is_some_and(|previous| previous != bounds)
        {
            return Err(invalid("domain", "kernels disagree on experiment geometry"));
        }
        if k.state.schema.as_str() != format!("{}/v{}", c.state_format.id, c.state_format.version) {
            return Err(invalid(
                "state",
                "captured portable format differs from selected kernel",
            ));
        }
        input.read(&k.state, c.bounds.state_bytes)?;
        if role == ExecutionContractId::Dynamics
            && objects
                .values()
                .filter(|o| o.inertial_mass_kilograms.is_some())
                .count()
                > c.bounds.projection_records as usize
        {
            return Err(limited("dynamics"));
        }
        contexts.insert(k.instance.clone(), c);
    }
    let mut records = 0usize;
    for f in &e.fields {
        let context = &contexts[&f.kernel.instance];
        // Resolve declaration/property lookups once per slot, never per entity.
        let bounds: Vec<_> = context
            .couplings
            .iter()
            .map(|slot| {
                let provider = selected
                    .descriptor()
                    .bindings
                    .iter()
                    .find(|b| b.consumer == context.contribution && b.requirement_slot == slot.slot)
                    .expect("verified coupling requirement");
                let Payload::Components(component) = &selected.payloads()[&provider.provider]
                else {
                    unreachable!("verified coupling component")
                };
                (
                    slot.source
                        .as_ref()
                        .map(|p| quantity_bounds(&component.scientific, &p.property)),
                    slot.response
                        .as_ref()
                        .map(|p| quantity_bounds(&component.scientific, &p.property)),
                )
            })
            .collect();
        let remaining = limits
            .couplings
            .checked_sub(records)
            .ok_or_else(|| limited("couplings"))?;
        let batch = packet::<CoupledEntity>(
            &f.coupled,
            input.read(&f.coupled, limits.state_bytes)?,
            remaining.min(context.bounds.projection_records as usize),
        )?;
        records += batch.iter().len();
        for value in batch.iter() {
            let object = objects
                .get(&value.id)
                .ok_or_else(|| invalid("coupled", "coupled entity is absent"))?;
            let slot = context
                .couplings
                .get(value.slot.0 as usize)
                .ok_or_else(|| invalid("coupled", "coupling slot absent"))?;
            if value.kinematics != object.kinematics
                || value.has_dynamics != object.inertial_mass_kilograms.is_some()
                || value.source_si.is_some() != slot.source.is_some()
                || value.response_si.is_some() != slot.response.is_some()
                || (slot
                    .source
                    .as_ref()
                    .zip(slot.response.as_ref())
                    .is_some_and(|(a, b)| a.property == b.property)
                    && value.source_si != value.response_si)
            {
                return Err(invalid(
                    "coupled",
                    "coupled projection differs from captured objects or property roles",
                ));
            }
            let (source, response) = bounds[value.slot.0 as usize];
            for (bounds, value) in [source.zip(value.source_si), response.zip(value.response_si)]
                .into_iter()
                .flatten()
            {
                quantity(bounds, value)?;
            }
        }
    }
    if let Some(scene) = &scene {
        scene::verify(scene, selected, e, &objects, &contexts, blobs, limits)?;
    }
    Ok(CheckedExecution {
        contexts,
        inputs: input.descriptors,
        scene,
    })
}

fn quantity_bounds(
    component: &ComponentSchema,
    property: &LocalContributionId,
) -> (Option<FiniteF64>, Option<FiniteF64>) {
    let p = component
        .properties
        .iter()
        .find(|p| &p.id == property)
        .expect("verified scalar role binding");
    let PropertyType::Quantity {
        minimum_si,
        maximum_si,
        ..
    } = p.schema
    else {
        unreachable!("verified scalar quantity")
    };
    (minimum_si, maximum_si)
}
fn quantity(
    (minimum_si, maximum_si): (Option<FiniteF64>, Option<FiniteF64>),
    value: FiniteF64,
) -> Result<(), Error> {
    if minimum_si.is_some_and(|min| value.get() < min.get())
        || maximum_si.is_some_and(|max| value.get() > max.get())
    {
        return Err(invalid(
            "component",
            "projected role quantity violates selected property constraints",
        ));
    }
    Ok(())
}

fn additional(
    selected: &VerifiedSelection,
    inputs: &BTreeMap<ArtifactDigest, w::ArtifactDescriptor>,
    compute: &w::ComputeSpec,
) -> Result<Vec<w::ArtifactDescriptor>, Error> {
    let code: BTreeSet<_> = compute
        .components
        .iter()
        .map(|c| c.artifact.digest)
        .collect();
    let mut required = BTreeMap::new();
    for a in selected
        .artifacts()
        .values()
        .filter(|a| !code.contains(&a.digest))
    {
        required.insert(
            a.digest,
            artifact("scientific-evidence", a.digest, a.size_bytes, &a.media_type)?,
        );
    }
    for a in inputs.values() {
        if code.contains(&a.digest) {
            return Err(invalid("input", "input aliases executable bytes"));
        }
        if required
            .insert(a.digest, a.clone())
            .is_some_and(|old| old != *a)
        {
            return Err(invalid("artifact", "conflicting evidence/input descriptor"));
        }
    }
    Ok(required.into_values().collect())
}
// Build only a bounded selected-source index; never copy/index the whole cache.
fn required_source<'a>(
    selected: &VerifiedSelection,
    e: &ExecutionDefinition,
    blobs: &BTreeMap<ArtifactDigest, &'a [u8]>,
    generated: &'a BTreeMap<ArtifactDigest, Vec<u8>>,
    limits: ProfileLimits,
) -> Result<BTreeMap<ArtifactDigest, &'a [u8]>, Error> {
    let mut result: BTreeMap<_, _> = generated.iter().map(|(d, b)| (*d, b.as_slice())).collect();
    let mut add = |digest| -> Result<(), Error> {
        if let std::collections::btree_map::Entry::Vacant(entry) = result.entry(digest) {
            entry.insert(
                *blobs
                    .get(&digest)
                    .ok_or_else(|| invalid("artifact", "source bytes absent"))?,
            );
        }
        Ok(())
    };
    for digest in selected.artifacts().keys() {
        add(*digest)?;
    }
    add(e.objects.digest)?;
    if let Some(scene) = &e.scene {
        add(scene.digest)?;
    }
    for f in &e.fields {
        add(f.coupled.digest)?;
    }
    for k in e.fields.iter().map(|f| &f.kernel).chain([&e.dynamics]) {
        add(k.context.digest)?;
        add(k.state.digest)?;
        if k.context.byte_length > MAX_CONTEXT_BYTES as u64 {
            return Err(limited("context"));
        }
        let c = InstanceContext::from_cbor(blob(k.context.digest, k.context.byte_length, blobs)?)?;
        add(c.configuration.digest)?;
        add(c.domain.digest)?;
    }
    if result.len() > limits.workload.max_artifacts {
        return Err(limited("artifacts"));
    }
    Ok(result)
}
