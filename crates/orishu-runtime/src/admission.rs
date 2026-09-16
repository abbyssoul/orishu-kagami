//! The shared local/worker admission adapter. Installation is not consulted.
use crate::*;
use orishu_plugin::{
    ArtifactDigest,
    execution::*,
    workload::{self, ProfileLimits, VerifiedWorkload},
};
use orishu_workload::v3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Host-owned admission policy; guest/store and run-owner limits still apply.
#[derive(Clone, Copy, Debug)]
pub struct AdmissionLimits {
    /// Root/selected/scientific input admission budgets.
    pub profile: ProfileLimits,
    /// Atomic owner and detached observer budgets.
    pub run: RunLimits,
    /// Maximum distinct independently compiled kernels in this admission.
    pub kernels: usize,
    /// Aggregate distinct executable bytes before starting any compilation.
    pub executable_bytes: u64,
}
impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            profile: ProfileLimits::default(),
            run: RunLimits::default(),
            kernels: 65,
            executable_bytes: 128 * 1024 * 1024,
        }
    }
}
/// Complete admission result. Boundary zero has passed actual guest validation;
/// no natural initialization has run while accepting the workload.
pub struct AdmittedWorkload {
    /// Exact immutable definition and provenance independently checked by admission.
    pub workload: VerifiedWorkload,
    /// Single-partition scientific owner, ready for explicit fixed-step advances.
    pub run: FixedRun,
}

/// Admit canonical workload-v3 bytes and the complete selected closure into the
/// same runtime used locally or by a worker. The embedding authority supplies a
/// fenced run scope and current deny set; this call does not allocate cluster IDs,
/// fetch missing bytes, install plugins, initialize state or choose physics.
///
/// Kernel JIT byte/count admission is bounded here; interruptible JIT compilation
/// and whole-process resident-memory accounting remain separate host hardening.
pub fn admit(
    sandbox: Arc<Sandbox>,
    manifest_bytes: &[u8],
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    scope: RunScope,
    denied: &BTreeSet<ArtifactDigest>,
    limits: AdmissionLimits,
    control: OperationControl,
) -> wasmtime::Result<AdmittedWorkload> {
    control.check()?;
    let manifest = v3::from_canonical_bytes(manifest_bytes, &limits.profile.workload)?;
    // Denial is independent of cache residency and applies to all required bytes.
    if manifest
        .spec
        .compute
        .components
        .iter()
        .map(|c| &c.artifact)
        .chain([&manifest.spec.selection, &manifest.spec.execution])
        .chain(&manifest.spec.artifacts)
        .any(|a| denied.contains(&a.digest))
        || denied.contains(&ArtifactDigest::sha256_of(manifest_bytes))
    {
        return Err(wasmtime::Error::msg("required workload resource is denied"));
    }
    if !manifest.spec.requirements.hardware.is_empty()
        || !manifest.spec.requirements.execution_profile.is_empty()
    {
        return Err(wasmtime::Error::msg(
            "unsupported additional execution requirements",
        ));
    }
    let workload = workload::verify(manifest, blobs, limits.profile)?;
    if scope.workload != workload.root() {
        return Err(wasmtime::Error::msg("run scope workload mismatch"));
    }
    let mut selected = BTreeMap::new();
    for c in workload.contexts().values() {
        selected.insert(c.kernel, c.execution_contract);
    }
    if selected.len() > limits.kernels {
        return Err(wasmtime::Error::msg("kernel count budget exceeded"));
    }
    let mut bytes = 0u64;
    for digest in selected.keys() {
        bytes = bytes
            .checked_add(workload.selection().artifacts()[digest].size_bytes)
            .filter(|n| *n <= limits.executable_bytes)
            .ok_or_else(|| wasmtime::Error::msg("aggregate executable byte budget exceeded"))?;
    }
    let mut compiled = BTreeMap::new();
    for (digest, contract) in selected {
        control.check()?;
        compiled.insert(
            digest,
            Arc::new(sandbox.compile(blobs[&digest], digest, contract)?),
        );
        control.check()?;
    }
    let e = workload.execution();
    let kernel = |k: &CapturedKernel| -> wasmtime::Result<RunKernel> {
        let c = &workload.contexts()[&k.instance];
        Ok(RunKernel {
            kernel: compiled[&c.kernel].clone(),
            context: c.clone(),
            configuration: buffer(&c.configuration, blobs)?,
            domain: buffer(&c.domain, blobs)?,
            state_extent: StateExtent {
                schema: k.state.schema.clone(),
                bytes: usize::try_from(k.state.byte_length)?,
                values: k.state.value_count,
            },
        })
    };
    let fields = e
        .fields
        .iter()
        .map(|f| {
            Ok(RunField {
                binding: kernel(&f.kernel)?,
                coupled: buffer(&f.coupled, blobs)?,
            })
        })
        .collect::<wasmtime::Result<Vec<_>>>()?;
    let program = RunProgram {
        fields,
        dynamics: kernel(&e.dynamics)?,
        timestep_seconds: e.timestep_seconds,
    };
    let captured = CapturedRunState {
        objects: buffer(&e.objects, blobs)?,
        fields: e
            .fields
            .iter()
            .map(|f| buffer(&f.kernel.state, blobs))
            .collect::<wasmtime::Result<Vec<_>>>()?,
        history: buffer(&e.dynamics.state, blobs)?,
    };
    let run = FixedRun::from_captured(sandbox, scope, program, captured, limits.run, control)?;
    Ok(AdmittedWorkload { workload, run })
}
fn buffer(
    input: &InputIdentity,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
) -> wasmtime::Result<Buffer> {
    let bytes = blobs
        .get(&input.digest)
        .ok_or_else(|| wasmtime::Error::msg("captured input absent"))?;
    if !input.matches(input.schema.as_str(), input.value_count, bytes) {
        return Err(wasmtime::Error::msg("captured input integrity mismatch"));
    }
    Ok(Buffer {
        schema: input.schema.to_string(),
        value_count: input.value_count,
        bytes: Arc::from(*bytes),
    })
}
