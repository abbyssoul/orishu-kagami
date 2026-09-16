//! Cold exact instance metadata and a borrowed, kernel-independent validation
//! envelope. These codecs establish shape/identity, not workload admission.
use super::{
    Batch, BulkError, BulkLimits, ComputePrecision, CoupledEntity, DynamicEntity, ObservableBinding,
};
use crate::{
    ArtifactDigest, ContributionRef, Dimension, Error, ExecutionContractId, ExecutionProfile,
    FiniteF64, KnownPoint, Limits, LocalContributionId, ScientificContractRef, StateFormat, codec,
    projection::{Project, object},
};
use orishu_resource::ApiVersion;
use orishu_workload::{ComponentInstanceId, SchemaId, canonical::CanonicalValue as V};
use serde::{Deserialize, Serialize};

/// Portable setup descriptor schema and envelope version.
pub const INSTANCE_SCHEMA: &str = "orishu.simulation.instance/v1";
/// Portable numerical-validation envelope schema; one logical envelope per grant.
pub const VALIDATION_SCHEMA: &str = "orishu.simulation.validation/v1";
/// Hard bound for cold context metadata, before tree construction.
pub const MAX_CONTEXT_BYTES: usize = 65_536;

/// Exact input descriptor, including bytes and schema-defined logical count.
/// A digest without the descriptor is insufficient for binding an input grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputIdentity {
    /// Portable schema, not a filesystem path or mutable registry entry.
    pub schema: SchemaId,
    /// Logical count in that schema.
    pub value_count: u64,
    /// Exact portable byte length.
    pub byte_length: u64,
    /// Hash of those exact bytes, independently verified by admission.
    pub digest: ArtifactDigest,
}
impl InputIdentity {
    /// Describe immutable bytes without interpreting their plugin-owned format.
    pub fn of(schema: SchemaId, value_count: u64, bytes: &[u8]) -> Self {
        Self {
            schema,
            value_count,
            byte_length: bytes.len() as u64,
            digest: ArtifactDigest::sha256_of(bytes),
        }
    }
    /// Compare all descriptor fields before hashing potentially large input bytes.
    pub fn matches(&self, schema: &str, value_count: u64, bytes: &[u8]) -> bool {
        self.schema.as_str() == schema
            && self.value_count == value_count
            && self.byte_length == bytes.len() as u64
            && self.digest.matches(bytes)
    }
}

/// Bounds negotiated for this instance; the sandbox can impose tighter resource
/// policy. These never promise that a numerical computation will succeed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionBounds {
    /// Maximum entity/coupling records in one declared projection.
    pub projection_records: u32,
    /// Maximum portable field state or integrator history bytes.
    pub state_bytes: u64,
    /// Maximum observer-generated points in one request; zero disables sampling.
    pub sample_points: u32,
    /// Maximum requested channels; zero disables sampling.
    pub sample_channels: u32,
}

/// One resolved standard coupling role, not a guess based on a property name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CouplingProperty {
    /// Property in the selected component's scientific contract.
    pub property: LocalContributionId,
    /// SI dimension for the corresponding scalar projection.
    pub dimension: Dimension,
}

/// Canonical slot table entry. Its array index is the bulk `CouplingSlot`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CouplingDescriptor {
    /// Selected field model requirement slot; table sorts strictly by this key.
    pub slot: LocalContributionId,
    /// Exact component vocabulary, independent of the kernel provider.
    pub component: ScientificContractRef,
    /// Source role; absence is not a zero-valued property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CouplingProperty>,
    /// Independent response role; never implicitly inertial mass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<CouplingProperty>,
}

/// Raw serializable instance metadata. Use `to_cbor`/`from_cbor` for validated
/// portable bytes. A worker must additionally verify the selected declaration
/// closure and its agreement with every field here; this is not an admission token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstanceContext {
    /// Exactly `orishu.simulation.instance/v1`.
    pub api_version: ApiVersion,
    /// Configured use in the workload graph, not a process-local guest session.
    pub instance: ComponentInstanceId,
    /// Original independently compiled Component artifact.
    pub kernel: ArtifactDigest,
    /// Exact provider-qualified contribution, never a mutable installed default.
    pub contribution: ContributionRef,
    /// Exact scientific identity of that contribution's model/integrator schema.
    pub scientific: ScientificContractRef,
    /// Exactly one platform scientific execution contract.
    pub execution_contract: ExecutionContractId,
    /// Explicit portable field/history format selected by this contribution.
    pub state_format: StateFormat,
    /// Supported fixed force/integration convention.
    pub profile: ExecutionProfile,
    /// Captured resolved configuration supplied separately to setup.
    pub configuration: InputIdentity,
    /// Captured domain/discretization/boundary configuration in its declared schema.
    pub domain: InputIdentity,
    /// Canonical source/response mapping; empty for Dynamics.
    pub couplings: Vec<CouplingDescriptor>,
    /// Exact observable vocabulary supplied by this model; canonical slot order.
    pub observables: Vec<ObservableBinding>,
    /// Selected computational precision, not inferred from binary64 interchange.
    pub compute_precision: ComputePrecision,
    /// Negotiated instance-local bounds, not global allocation authority.
    pub bounds: ExecutionBounds,
}

fn metadata_limits() -> Limits {
    Limits {
        max_payload_bytes: MAX_CONTEXT_BYTES,
        max_values: 4096,
        max_depth: 12,
        max_schema_items: 64,
        max_text_bytes: 256,
        ..Limits::default()
    }
}
impl InstanceContext {
    /// Structural/role validation. Exact release/declaration correspondence and
    /// state provenance remain mandatory independent workload-admission checks.
    pub fn validate(&self) -> Result<(), Error> {
        let expected = match self.execution_contract {
            ExecutionContractId::Field => KnownPoint::FieldModels,
            ExecutionContractId::Dynamics => KnownPoint::Integrators,
        };
        if self.api_version.as_str() != INSTANCE_SCHEMA
            || self.contribution.extension_point.as_str() != expected.as_str()
            || self.couplings.len() > 64
            || self.observables.len() > 64
            || self.observables.windows(2).any(|p| p[0].slot >= p[1].slot)
            || (self.execution_contract == ExecutionContractId::Dynamics
                && !self.observables.is_empty())
            || (self.execution_contract == ExecutionContractId::Dynamics
                && !self.couplings.is_empty())
            || self.couplings.windows(2).any(|p| p[0].slot >= p[1].slot)
            || self
                .couplings
                .iter()
                .any(|c| c.source.is_none() && c.response.is_none())
            || self.bounds.projection_records == 0
            || (self.bounds.sample_points == 0) != (self.bounds.sample_channels == 0)
            || (self.execution_contract == ExecutionContractId::Dynamics
                && self.bounds.sample_points != 0)
        {
            return Err(Error::malformed("instance", "invalid execution context"));
        }
        for (i, binding) in self.observables.iter().enumerate() {
            binding.channel.validate()?;
            if binding.quality_flags == 0
                || binding.quality_flags & !7 != 0
                || self.observables[..i]
                    .iter()
                    .any(|b| b.channel.contract == binding.channel.contract)
            {
                return Err(Error::malformed(
                    "observables",
                    "invalid or duplicate model observable binding",
                ));
            }
        }
        Ok(())
    }
    /// Deterministic bounded CBOR from an explicit semantic projection. JSON/serde
    /// field order is not a portable encoding and never contributes implicit keys.
    pub fn to_cbor(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        codec::encode(&self.project(), &metadata_limits(), MAX_CONTEXT_BYTES)
    }
    /// Refuse bytes/depth/value excess before typed decoding; then validate all
    /// relationships and require the exact canonical projection (no extra keys).
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self =
            codec::structured_from_cbor(bytes, &metadata_limits(), MAX_CONTEXT_BYTES)?;
        if value.to_cbor()? != bytes {
            return Err(Error::malformed(
                "instance",
                "noncanonical execution context",
            ));
        }
        Ok(value)
    }
}

impl Project for InputIdentity {
    fn project(&self) -> V {
        object(vec![
            ("schema", Some(V::text(self.schema.as_str()))),
            ("valueCount", Some(self.value_count.project())),
            ("byteLength", Some(self.byte_length.project())),
            ("digest", Some(self.digest.project())),
        ])
    }
}
impl Project for ExecutionBounds {
    fn project(&self) -> V {
        object(vec![
            ("projectionRecords", Some(self.projection_records.project())),
            ("stateBytes", Some(self.state_bytes.project())),
            ("samplePoints", Some(self.sample_points.project())),
            ("sampleChannels", Some(self.sample_channels.project())),
        ])
    }
}
impl Project for CouplingProperty {
    fn project(&self) -> V {
        object(vec![
            ("property", Some(self.property.project())),
            ("dimension", Some(self.dimension.project())),
        ])
    }
}
impl Project for CouplingDescriptor {
    fn project(&self) -> V {
        object(vec![
            ("slot", Some(self.slot.project())),
            ("component", Some(self.component.project())),
            ("source", self.source.as_ref().map(Project::project)),
            ("response", self.response.as_ref().map(Project::project)),
        ])
    }
}
impl Project for InstanceContext {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("instance", Some(V::text(self.instance.as_str()))),
            ("kernel", Some(self.kernel.project())),
            ("contribution", Some(self.contribution.project())),
            ("scientific", Some(self.scientific.project())),
            ("executionContract", Some(self.execution_contract.project())),
            ("stateFormat", Some(self.state_format.project())),
            ("profile", Some(self.profile.project())),
            ("configuration", Some(self.configuration.project())),
            ("domain", Some(self.domain.project())),
            ("couplings", Some(self.couplings.project())),
            ("observables", Some(self.observables.project())),
            ("computePrecision", Some(self.compute_precision.project())),
            ("bounds", Some(self.bounds.project())),
        ])
    }
}

/// Borrowed complete numerical-admission inputs. The initialized/loaded opaque
/// state is already supplied by common.load; no defaults are generated here.
/// Domain/configuration schema and count come from the exact setup context.
#[derive(Clone, Copy, Debug)]
pub struct ValidationInputs<'a> {
    /// Authored positive finite SI timestep; no host-selected stability formula.
    pub timestep_seconds: FiniteF64,
    /// Canonical context supplied to setup, including profile and role bindings.
    pub context: &'a [u8],
    /// Exact captured resolved domain/discretization input bytes.
    pub domain: &'a [u8],
    /// Exact captured configuration bytes, also supplied to setup.
    pub configuration: &'a [u8],
    /// Standard coupled-entity (field) or dynamic-entity (Dynamics) packet.
    pub entities: &'a [u8],
}
/// Fixed validation framing length, preceding four length-delimited byte sections.
pub const VALIDATION_HEADER_BYTES: usize = 28;
impl<'a> ValidationInputs<'a> {
    /// Checked extent before reserving caller-owned storage. A valid envelope is
    /// additionally read/checked against an exact `InstanceContext` before use.
    pub fn encoded_bytes(&self, limits: BulkLimits) -> Result<usize, BulkError> {
        if !self.timestep_seconds.is_positive() {
            return Err(BulkError::InvalidRecord);
        }
        [self.context, self.domain, self.configuration, self.entities]
            .into_iter()
            .try_fold(VALIDATION_HEADER_BYTES, |total, part| {
                if u32::try_from(part.len()).is_err() {
                    return Err(BulkError::LimitExceeded);
                }
                total
                    .checked_add(part.len())
                    .filter(|n| *n <= limits.bytes)
                    .ok_or(BulkError::LimitExceeded)
            })
    }
    /// Encode using reusable caller storage. Validation/rejection leaves its prior
    /// bytes intact; the exact context supplied separately is checked before writes.
    pub fn encode(
        &self,
        output: &mut Vec<u8>,
        context: &InstanceContext,
        limits: BulkLimits,
    ) -> Result<(), BulkError> {
        let len = self.encoded_bytes(limits)?;
        self.check(context, limits)?;
        output
            .try_reserve(len.saturating_sub(output.len()))
            .map_err(|_| BulkError::LimitExceeded)?;
        output.clear();
        output.extend_from_slice(b"OSV1");
        output.extend_from_slice(&self.timestep_seconds.get().to_le_bytes());
        for bytes in [self.context, self.domain, self.configuration, self.entities] {
            output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        }
        for bytes in [self.context, self.domain, self.configuration, self.entities] {
            output.extend_from_slice(bytes);
        }
        Ok(())
    }
    /// Check framing and aggregate bounds before slicing/scanning, then validate
    /// context equality, exact domain/configuration hashes and the role packet.
    /// The numeric bulk data stays borrowed; cold context decoding is bounded.
    pub fn read(
        bytes: &'a [u8],
        context: &InstanceContext,
        limits: BulkLimits,
    ) -> Result<Self, BulkError> {
        if bytes.len() > limits.bytes {
            return Err(BulkError::LimitExceeded);
        }
        if bytes.len() < VALIDATION_HEADER_BYTES || &bytes[..4] != b"OSV1" {
            return Err(BulkError::Framing);
        }
        let bits = u64::from_le_bytes(bytes[4..12].try_into().map_err(|_| BulkError::Framing)?);
        let timestep_seconds =
            FiniteF64::new(f64::from_bits(bits)).map_err(|_| BulkError::InvalidRecord)?;
        if !timestep_seconds.is_positive() {
            return Err(BulkError::InvalidRecord);
        }
        let mut cursor = VALIDATION_HEADER_BYTES;
        let mut parts = [&[][..]; 4];
        for (i, part) in parts.iter_mut().enumerate() {
            let start = 12 + 4 * i;
            let n = u32::from_le_bytes(
                bytes[start..start + 4]
                    .try_into()
                    .map_err(|_| BulkError::Framing)?,
            ) as usize;
            let end = cursor.checked_add(n).ok_or(BulkError::LimitExceeded)?;
            *part = bytes.get(cursor..end).ok_or(BulkError::Framing)?;
            cursor = end;
        }
        if cursor != bytes.len() {
            return Err(BulkError::Framing);
        }
        let input = Self {
            timestep_seconds,
            context: parts[0],
            domain: parts[1],
            configuration: parts[2],
            entities: parts[3],
        };
        input.check(context, limits)?;
        Ok(input)
    }
    fn check(&self, context: &InstanceContext, limits: BulkLimits) -> Result<(), BulkError> {
        if self.context.len() > MAX_CONTEXT_BYTES {
            return Err(BulkError::LimitExceeded);
        }
        if InstanceContext::from_cbor(self.context).map_err(|_| BulkError::InvalidRecord)?
            != *context
            || !context.domain.matches(
                context.domain.schema.as_str(),
                context.domain.value_count,
                self.domain,
            )
            || !context.configuration.matches(
                context.configuration.schema.as_str(),
                context.configuration.value_count,
                self.configuration,
            )
        {
            return Err(BulkError::InvalidRecord);
        }
        let limits = BulkLimits {
            records: limits
                .records
                .min(context.bounds.projection_records as usize),
            ..limits
        };
        match context.execution_contract {
            ExecutionContractId::Dynamics => {
                Batch::<DynamicEntity>::read(self.entities, limits)?;
            }
            ExecutionContractId::Field => {
                let entities = Batch::<CoupledEntity>::read(self.entities, limits)?;
                for e in entities.iter() {
                    let slot = context
                        .couplings
                        .get(e.slot.0 as usize)
                        .ok_or(BulkError::InvalidRecord)?;
                    if (e.source_si.is_some() && slot.source.is_none())
                        || (e.response_si.is_some() && slot.response.is_none())
                    {
                        return Err(BulkError::InvalidRecord);
                    }
                }
            }
        }
        Ok(())
    }
}
