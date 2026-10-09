//! Cold descriptor needed to query an exact opaque committed field remotely.
use super::{InstanceContext, SampleChannel, SampleMetadata, SampleSnapshot, SnapshotSource};
use crate::{
    ArtifactDigest, Error, ExecutionContractId, Limits, codec,
    projection::{Project, object},
};
use orishu_resource::ApiVersion;
use orishu_workload::canonical::CanonicalValue as V;
use serde::{Deserialize, Serialize};

/// Versioned descriptor, not scientific state or a lease token.
pub const FIELD_OBSERVATION_SCHEMA: &str = "orishu.simulation.field-observation/v1";
/// Includes one bounded context plus its exact snapshot reference.
pub const MAX_FIELD_OBSERVATION_BYTES: usize = 128 * 1024;

/// Exact committed source and selected model context. Field-private bytes never
/// cross this interface. This value neither pins server state nor proves admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldObservation {
    /// Exactly `orishu.simulation.field-observation/v1`.
    pub api_version: ApiVersion,
    /// Source and exact opaque field-state identity.
    pub snapshot: SampleSnapshot,
    /// Selected provider, configuration/domain identities and typed observables.
    pub context: InstanceContext,
}
impl FieldObservation {
    /// Describe a retained committed field, validating the cold envelope.
    pub fn new(snapshot: SampleSnapshot, context: InstanceContext) -> Result<Self, Error> {
        let value = Self {
            api_version: FIELD_OBSERVATION_SCHEMA.parse().expect("static version"),
            snapshot,
            context,
        };
        value.validate()?;
        Ok(value)
    }
    /// Structural checks only; runtime admission owns state-format semantics.
    pub fn validate(&self) -> Result<(), Error> {
        if self.api_version.as_str() != FIELD_OBSERVATION_SCHEMA
            || self.context.execution_contract != ExecutionContractId::Field
            || self.snapshot.state.byte_length > self.context.bounds.state_bytes
            || !matches!(self.snapshot.source, SnapshotSource::Committed {
                epoch, time_seconds, ..
            } if epoch != 0 && time_seconds.get() >= 0.0)
        {
            return Err(Error::malformed("field-observation", "invalid descriptor"));
        }
        // Preserve the independent context byte/structure ceiling.
        self.context.to_cbor()?;
        Ok(())
    }
    /// Bounded canonical representation, including exact finite SI time.
    pub fn to_cbor(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        codec::encode(&self.project(), &limits(), MAX_FIELD_OBSERVATION_BYTES)
    }
    /// Preflight all bytes/nesting/counts before typed allocation; require canonical
    /// encoding and the complete exact schema, not a permissive CBOR envelope.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self =
            codec::structured_from_cbor(bytes, &limits(), MAX_FIELD_OBSERVATION_BYTES)?;
        if value.to_cbor()? != bytes {
            return Err(Error::malformed(
                "field-observation",
                "noncanonical descriptor",
            ));
        }
        Ok(value)
    }
    /// Form observer-scoped query metadata; point geometry stays with the caller.
    pub fn request(
        &self,
        request_id: u64,
        channels: Vec<SampleChannel>,
    ) -> Result<SampleMetadata, Error> {
        self.validate()?;
        let metadata = SampleMetadata {
            api_version: super::SAMPLE_METADATA_SCHEMA
                .parse()
                .expect("static version"),
            request_id,
            snapshot: self.snapshot.clone(),
            field: self.context.instance.clone(),
            context: ArtifactDigest::sha256_of(&self.context.to_cbor()?),
            channels,
        };
        metadata.validate()?;
        Ok(metadata)
    }
}
fn limits() -> Limits {
    Limits {
        max_payload_bytes: MAX_FIELD_OBSERVATION_BYTES,
        max_values: 8192,
        max_depth: 16,
        max_schema_items: 64,
        max_text_bytes: 256,
        ..Limits::default()
    }
}
impl Project for FieldObservation {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("snapshot", Some(self.snapshot.project())),
            ("context", Some(self.context.project())),
        ])
    }
}
