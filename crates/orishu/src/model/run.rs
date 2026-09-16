//! Immutable execution identity, distinct from workload identity and routing.
use orishu_identity::FormationId;
use orishu_workload::{
    ArtifactDigest, WorkloadDigest,
    canonical::{self, CanonicalMap, CanonicalValue as V},
};
use serde::{Deserialize, Serialize};

/// Explicit immutable fact-record schema, not the mutable run-status resource.
pub const RUN_DESCRIPTOR_VERSION: &str = "orishu.run-descriptor/v1";
/// Bound before parsing either supported descriptor encoding.
pub const MAX_RUN_DESCRIPTOR_BYTES: usize = 512;

/// Execution epoch, not a simulation boundary, lease sequence or wall-clock time.
/// The wire can describe any uint64; the current worker allocator starts at one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkloadEpoch(u64);
impl WorkloadEpoch {
    /// Construct an already allocated/received epoch; this does not allocate one.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    /// Exact protocol uint64.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Existing protocol tuple, independent of labels, submitting client and route.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunIdentity {
    formation_id: FormationId,
    workload_id: WorkloadDigest,
    workload_epoch: WorkloadEpoch,
}
impl RunIdentity {
    /// Assemble typed identity; allocation authority remains with the run host.
    pub fn new(
        formation_id: FormationId,
        workload_id: WorkloadDigest,
        workload_epoch: WorkloadEpoch,
    ) -> Self {
        Self {
            formation_id,
            workload_id,
            workload_epoch,
        }
    }
    /// Producing formation, not its display label or a current entry node.
    pub fn formation_id(&self) -> &FormationId {
        &self.formation_id
    }
    /// Exact immutable workload root for the scientific profile.
    pub fn workload_id(&self) -> WorkloadDigest {
        self.workload_id
    }
    /// Distinguishes fresh/reset/resumed executions under this formation.
    pub fn workload_epoch(&self) -> WorkloadEpoch {
        self.workload_epoch
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Version {
    #[serde(rename = "orishu.run-descriptor/v1")]
    V1,
}
/// Immutable content-addressed fact record binding the protocol execution tuple.
/// Generic serde is for bounded host/wire projection; identity uses explicit CBOR.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunDescriptor {
    api_version: Version,
    run: RunIdentity,
}
/// Stable bounded refusal; never echoes arbitrary untrusted keys or input bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunDescriptorError {
    /// Input/structure exceeds the fixed leaf-record budget.
    #[error("run descriptor limit exceeded")]
    Limit,
    /// Encoding, fields or canonical spelling are invalid.
    #[error("malformed run descriptor")]
    Malformed,
    /// Canonical record names an unsupported schema version.
    #[error("unsupported run descriptor version")]
    Version,
}
fn limits() -> orishu_workload::Limits {
    orishu_workload::Limits {
        max_manifest_bytes: MAX_RUN_DESCRIPTOR_BYTES as u64,
        // The shared decoder also compares each text extent with its remaining
        // value budget. Allow a 128-byte formation ID plus visited keys/values;
        // the typed projection below still requires exactly eleven tree nodes.
        max_canonical_values: 160,
        max_nesting_depth: 3,
        ..orishu_workload::Limits::DEFAULT
    }
}
fn codec(error: canonical::CanonicalError) -> RunDescriptorError {
    use canonical::CanonicalError::*;
    match error {
        TooDeep { .. } | TooManyValues { .. } | TooLarge { .. } | EncodedTooLarge { .. } => {
            RunDescriptorError::Limit
        }
        _ => RunDescriptorError::Malformed,
    }
}
fn text(value: &V) -> Result<&str, RunDescriptorError> {
    match value {
        V::Text(value) => Ok(value),
        _ => Err(RunDescriptorError::Malformed),
    }
}
fn map(value: &V, count: usize) -> Result<&CanonicalMap, RunDescriptorError> {
    match value {
        V::Map(map) if map.entries().len() == count => Ok(map),
        _ => Err(RunDescriptorError::Malformed),
    }
}
fn field<'a>(map: &'a CanonicalMap, name: &str) -> Result<&'a V, RunDescriptorError> {
    map.entries()
        .iter()
        .find_map(|(k, v)| matches!(k, V::Text(k) if k == name).then_some(v))
        .ok_or(RunDescriptorError::Malformed)
}
impl RunDescriptor {
    /// Construct the current immutable descriptor for an allocated identity.
    pub fn new(run: RunIdentity) -> Self {
        Self {
            api_version: Version::V1,
            run,
        }
    }
    /// Exact protocol source; no routing hints or credentials are stored.
    pub fn identity(&self) -> &RunIdentity {
        &self.run
    }
    /// Explicit deterministic CBOR; serde field order/attributes cannot change it.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RunDescriptorError> {
        let identity = V::Map(
            CanonicalMap::fields([
                ("formationId", Some(V::text(self.run.formation_id.as_str()))),
                (
                    "workloadId",
                    Some(V::text(self.run.workload_id.to_string())),
                ),
                (
                    "workloadEpoch",
                    Some(V::UInt(self.run.workload_epoch.get())),
                ),
            ])
            .map_err(codec)?,
        );
        let value = V::Map(
            CanonicalMap::fields([
                ("apiVersion", Some(V::text(RUN_DESCRIPTOR_VERSION))),
                ("run", Some(identity)),
            ])
            .map_err(codec)?,
        );
        canonical::encode(&value, &limits()).map_err(codec)
    }
    /// SHA-256 over exact canonical descriptor bytes, not JSON or a mutable label.
    pub fn digest(&self) -> Result<ArtifactDigest, RunDescriptorError> {
        Ok(ArtifactDigest::sha256_of(&self.canonical_bytes()?))
    }
    /// Read exact canonical bytes with bounds before nested allocation. Refuses
    /// duplicates, tags, trailing data, unknown fields and alternate spellings.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, RunDescriptorError> {
        let value = canonical::decode(bytes, &limits()).map_err(codec)?;
        let root = map(&value, 2)?;
        if text(field(root, "apiVersion")?)? != RUN_DESCRIPTOR_VERSION {
            return Err(RunDescriptorError::Version);
        }
        let run = map(field(root, "run")?, 3)?;
        let formation = text(field(run, "formationId")?)?
            .parse()
            .map_err(|_| RunDescriptorError::Malformed)?;
        let workload = text(field(run, "workloadId")?)?
            .parse()
            .map_err(|_| RunDescriptorError::Malformed)?;
        let V::UInt(epoch) = field(run, "workloadEpoch")? else {
            return Err(RunDescriptorError::Malformed);
        };
        let descriptor = Self::new(RunIdentity::new(
            formation,
            workload,
            WorkloadEpoch::new(*epoch),
        ));
        if descriptor.canonical_bytes()? != bytes {
            return Err(RunDescriptorError::Malformed);
        }
        Ok(descriptor)
    }
    /// Bounded human-readable input; JSON formatting is deliberately not identity.
    pub fn from_json(bytes: &[u8]) -> Result<Self, RunDescriptorError> {
        if bytes.len() > MAX_RUN_DESCRIPTOR_BYTES {
            return Err(RunDescriptorError::Limit);
        }
        serde_json::from_slice(bytes).map_err(|_| RunDescriptorError::Malformed)
    }
}
