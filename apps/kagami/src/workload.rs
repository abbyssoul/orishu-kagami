//! Captured-document workload compilation through the shared runtime contract.
//! Delivery/UI integration and the emitter profile remain separate open work.
use crate::plugins::PreparedSelection;
use kagami_document::{
    ExperimentSnapshot,
    projection::{self, ExecutionProjection, ProjectionError},
};
use orishu_plugin::{
    ArtifactDigest,
    workload::{self, CompiledWorkload, ProfileLimits, WorkloadError},
};
use std::{collections::BTreeMap, sync::Arc};

/// Independent failures at the authoring/selected-workload seam.
#[derive(Debug)]
pub enum CompileError {
    /// A prepared inventory selection may not change a document's exact pins.
    SelectionMismatch,
    /// Unsupported intent or invalid numerical projection.
    Projection(ProjectionError),
    /// Shared compiler's independently verified closure refusal.
    Workload(WorkloadError),
}
impl From<ProjectionError> for CompileError {
    fn from(value: ProjectionError) -> Self {
        Self::Projection(value)
    }
}
impl From<WorkloadError> for CompileError {
    fn from(value: WorkloadError) -> Self {
        Self::Workload(value)
    }
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelectionMismatch => {
                f.write_str("prepared plugin selection differs from the captured document")
            }
            Self::Projection(e) => e.fmt(f),
            Self::Workload(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SelectionMismatch => None,
            Self::Projection(e) => Some(e),
            Self::Workload(e) => Some(e),
        }
    }
}

/// Self-contained captured scene, numerical inputs and selected executable closure.
/// Source expressions and location-free template evidence are serialized in the
/// shared scene artifact. No inventory is needed to execute these bytes.
#[derive(Debug)]
pub struct CapturedWorkload {
    projection: ExecutionProjection,
    compiled: CompiledWorkload,
    blobs: BTreeMap<ArtifactDigest, Arc<[u8]>>,
}
impl CapturedWorkload {
    /// Source revision and scientific packet projection.
    pub fn projection(&self) -> &ExecutionProjection {
        &self.projection
    }
    /// Exact canonical root and independently verified scientific closure.
    pub fn compiled(&self) -> &CompiledWorkload {
        &self.compiled
    }
    /// Only required digest-addressed artifacts, including exact selected code.
    pub fn blobs(&self) -> &BTreeMap<ArtifactDigest, Arc<[u8]>> {
        &self.blobs
    }
}

/// Compile one captured revision with an already frozen installed selection.
/// Does not initialize, consult defaults, execute guests, mutate the document or
/// authorize submission. A future asynchronous exporter must guard document and
/// inventory revisions before publishing/adopting its result.
pub fn compile_captured(
    snapshot: &ExperimentSnapshot,
    prepared: &PreparedSelection,
    metadata: orishu_workload::WorkloadMeta,
    limits: ProfileLimits,
    authoring: &kagami_document::Limits,
) -> Result<CapturedWorkload, CompileError> {
    let setup = snapshot
        .setup()
        .scientific()
        .ok_or(ProjectionError::LegacySetup)?;
    if setup.declarations().descriptor() != prepared.compiled().verified().descriptor() {
        return Err(CompileError::SelectionMismatch);
    }
    let projection = projection::project(snapshot, limits, authoring)?;
    let borrowed = prepared
        .blobs()
        .iter()
        .chain(projection.blobs())
        .map(|(d, b)| (*d, b.as_ref()))
        .collect();
    let compiled = workload::compile(
        metadata,
        prepared.compiled(),
        projection.definition(),
        &borrowed,
        limits,
    )?;
    let manifest = compiled.verified().manifest();
    let mut blobs = BTreeMap::new();
    for artifact in manifest
        .spec
        .compute
        .components
        .iter()
        .map(|c| &c.artifact)
        .chain([&manifest.spec.selection, &manifest.spec.execution])
        .chain(&manifest.spec.artifacts)
    {
        blobs.entry(artifact.digest).or_insert_with(|| {
            if let Some(bytes) = compiled.generated().get(&artifact.digest) {
                Arc::from(bytes.as_slice())
            } else {
                projection
                    .blobs()
                    .get(&artifact.digest)
                    .or_else(|| prepared.blobs().get(&artifact.digest))
                    .expect("independently verified complete closure")
                    .clone()
            }
        });
    }
    Ok(CapturedWorkload {
        projection,
        compiled,
        blobs,
    })
}
