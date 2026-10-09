//! Exact read intents for one-shot object and typed field observation adapters.
use super::{
    ApiRoute,
    run::{RunDescriptor, RunIdentity},
};
use orishu_workload::ArtifactDigest;
use serde::{Deserialize, Serialize};

/// Read-only POST, never a scientific command or implicit subscription.
pub const API_ROUTE_RUN_OBJECTS: ApiRoute = ApiRoute::from_static("run/objects");
/// Success body is the shared OOF1 payload, not a CBOR API response envelope.
pub const OBJECT_OBSERVATION_MEDIA_TYPE: &str = "application/vnd.orishu.object-observation.v1";
/// Digest of the entire response payload; canonical sha256 spelling.
pub const OBJECT_OBSERVATION_DIGEST_HEADER: &str = "orishu-observation-digest";
/// Small request ceiling, enforced before deserialization.
pub const MAX_OBJECT_REQUEST_BYTES: usize = 4096;
/// Initial whole-object HTTP delivery policy; larger projections are refused.
pub const MAX_OBJECT_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Read an exact field descriptor, without its opaque state bytes.
pub const API_ROUTE_RUN_FIELD: ApiRoute = ApiRoute::from_static("run/field");
/// Query an exact field snapshot with the shared OSQ1 point packet.
pub const API_ROUTE_RUN_SAMPLES: ApiRoute = ApiRoute::from_static("run/samples");
/// Canonical shared field descriptor success body.
pub const FIELD_OBSERVATION_MEDIA_TYPE: &str = "application/vnd.orishu.field-observation.v1";
/// Four-byte big-endian metadata length, typed CBOR read intent, OSQ1 packet.
pub const FIELD_SAMPLE_REQUEST_MEDIA_TYPE: &str = "application/vnd.orishu.field-sample-request.v1";
/// Shared OSP1 response, paired with the exact request/context retained by caller.
pub const FIELD_SAMPLE_RESPONSE_MEDIA_TYPE: &str =
    "application/vnd.orishu.field-sample-response.v1";
/// Both complete sample packets are independently limited to eight MiB.
pub const MAX_FIELD_SAMPLE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum FieldRequestVersion {
    #[serde(rename = "orishu.field-observation-request/v1")]
    V1,
}
/// Exact run/boundary and configured field instance. A descriptor read does not
/// lease that boundary across later network requests; stale queries are refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldObservationRequest {
    api_version: FieldRequestVersion,
    run: RunIdentity,
    boundary: u64,
    field: orishu_workload::ComponentInstanceId,
}
impl FieldObservationRequest {
    /// Capture explicit observer intent without discovering or updating identity.
    pub fn new(
        run: RunIdentity,
        boundary: u64,
        field: orishu_workload::ComponentInstanceId,
    ) -> Self {
        Self {
            api_version: FieldRequestVersion::V1,
            run,
            boundary,
            field,
        }
    }
    /// Exact execution identity.
    pub fn run(&self) -> &RunIdentity {
        &self.run
    }
    /// Exact committed boundary.
    pub fn boundary(&self) -> u64 {
        self.boundary
    }
    /// Configured field instance, not a field-family or model name.
    pub fn field(&self) -> &orishu_workload::ComponentInstanceId {
        &self.field
    }
    /// Correlate source and field; full state/context are checked against the
    /// retained descriptor and query before any guest invocation.
    pub fn matches(&self, field: &orishu_plugin::execution::FieldObservation) -> bool {
        field.context.instance == self.field
            && ObjectObservationRequest::new(self.run.clone(), self.boundary)
                .matches(&field.snapshot.source)
    }
}
/// Shared remote sampling ceilings, tightened by each selected instance context.
pub fn field_sample_limits(
    context: &orishu_plugin::execution::InstanceContext,
) -> orishu_plugin::execution::SampleLimits {
    let mut limits = orishu_plugin::execution::SampleLimits::default();
    limits.bytes = MAX_FIELD_SAMPLE_BYTES;
    limits.points = limits.points.min(context.bounds.sample_points as usize);
    limits.channels = limits.channels.min(context.bounds.sample_channels as usize);
    limits
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum RequestVersion {
    #[serde(rename = "orishu.object-observation-request/v1")]
    V1,
}
/// Exact execution and committed boundary. No automatic current-run selection,
/// boundary refresh, implicit stepping or retry is permitted by this intent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectObservationRequest {
    api_version: RequestVersion,
    run: RunIdentity,
    boundary: u64,
}
impl ObjectObservationRequest {
    /// Select one exact boundary. Unallocated/absent epochs remain unavailable.
    pub fn new(run: RunIdentity, boundary: u64) -> Self {
        Self {
            api_version: RequestVersion::V1,
            run,
            boundary,
        }
    }
    /// Exact formation, workload root and epoch.
    pub fn run(&self) -> &RunIdentity {
        &self.run
    }
    /// Expected boundary, checked inside the retained executor.
    pub fn boundary(&self) -> u64 {
        self.boundary
    }
    /// Correlate execution/boundary, including the canonical descriptor digest that
    /// binds formation. SI time is reported by the producer, not inferred here.
    pub fn matches(&self, source: &orishu_plugin::execution::SnapshotSource) -> bool {
        matches!(source, orishu_plugin::execution::SnapshotSource::Committed {
            workload, run, epoch, boundary, ..
        } if *workload == self.run.workload_id()
            && *epoch == self.run.workload_epoch().get()
            && *boundary == self.boundary
            && RunDescriptor::new(self.run.clone()).digest().is_ok_and(|digest| digest == *run))
    }
}

/// Fully validated immutable response bytes. Kept separate from editable state,
/// checkpoints, stream cursors and command receipts. Reading a borrowed view
/// revalidates the bounded payload; callers should retain a view while consuming it.
pub struct ObservedObjects {
    bytes: Vec<u8>,
    digest: ArtifactDigest,
}
impl ObservedObjects {
    /// Validate the complete shared format and exact requested source before
    /// making the response available. Does not authenticate its producer.
    pub fn validate(
        bytes: Vec<u8>,
        digest: ArtifactDigest,
        request: &ObjectObservationRequest,
    ) -> Result<Self, orishu_plugin::execution::ObjectObservationError> {
        let view =
            orishu_plugin::execution::ObjectObservation::read(&bytes, digest, object_limits())?;
        if !request.matches(view.source()) {
            return Err(orishu_plugin::execution::ObjectObservationError::Source);
        }
        Ok(Self { bytes, digest })
    }
    /// Complete immutable portable frame.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Content identity, not run identity or resume cursor.
    pub fn digest(&self) -> ArtifactDigest {
        self.digest
    }
    /// Borrow a validated scientific view. This initial adapter repeats bounded
    /// validation rather than caching self-referential packet views.
    pub fn view(&self) -> orishu_plugin::execution::ObjectObservation<'_> {
        orishu_plugin::execution::ObjectObservation::read(&self.bytes, self.digest, object_limits())
            .expect("private immutable validated observation")
    }
}
/// Initial HTTP policy; local runtime callers may choose different byte ceilings.
pub fn object_limits() -> orishu_plugin::execution::ObjectObservationLimits {
    orishu_plugin::execution::ObjectObservationLimits {
        bytes: MAX_OBJECT_RESPONSE_BYTES,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_intent_roundtrips_and_refuses_unknown_versions_fields_and_duplicates() {
        let request = ObjectObservationRequest::new(
            RunIdentity::new(
                "formation-a".parse().unwrap(),
                ArtifactDigest::sha256_of(b"workload")
                    .to_string()
                    .parse()
                    .unwrap(),
                super::super::run::WorkloadEpoch::new(1),
            ),
            4,
        );
        let json = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<ObjectObservationRequest>(&json).unwrap(),
            request
        );
        let mut cbor = Vec::new();
        ciborium::into_writer(&request, &mut cbor).unwrap();
        assert_eq!(
            ciborium::from_reader::<ObjectObservationRequest, _>(&cbor[..]).unwrap(),
            request
        );
        let text = String::from_utf8(json).unwrap();
        for bad in [
            text.replace("request/v1", "request/v2"),
            text.replace("\"boundary\":4", "\"boundary\":4,\"boundary\":5"),
            text.replace("\"boundary\":4", "\"boundary\":4,\"current\":true"),
            text.replace("\"boundary\":4", "\"boundary\":-1"),
        ] {
            assert!(serde_json::from_str::<ObjectObservationRequest>(&bad).is_err());
        }
        let field =
            FieldObservationRequest::new(request.run().clone(), 4, "field".parse().unwrap());
        let json = serde_json::to_string(&field).unwrap();
        let mut cbor = vec![];
        ciborium::into_writer(&field, &mut cbor).unwrap();
        assert_eq!(
            ciborium::from_reader::<FieldObservationRequest, _>(&cbor[..]).unwrap(),
            field
        );
        assert_eq!(
            serde_json::from_str::<FieldObservationRequest>(&json).unwrap(),
            field
        );
        for bad in [
            json.replace("request/v1", "request/v2"),
            json.replace("\"boundary\":4", "\"boundary\":4,\"boundary\":5"),
            json.replace("\"boundary\":4", "\"boundary\":4,\"current\":true"),
            json.replace("\"field\":\"field\"", "\"field\":\"\""),
        ] {
            assert!(serde_json::from_str::<FieldObservationRequest>(&bad).is_err());
        }
    }
}
