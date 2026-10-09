//! Full numeric-object projection of one committed fixed-profile boundary.
//! This is not a whole-world snapshot, editable document, or resumable stream.
use super::{Batch, BulkError, BulkLimits, Force, ObjectState, SnapshotSource};
use crate::{
    ArtifactDigest, Limits, codec,
    projection::{Project, object},
};
use orishu_resource::ApiVersion;
use orishu_workload::canonical::CanonicalValue as V;
use serde::{Deserialize, Serialize};

/// Complete numeric-object observation, with predecessor-evaluated reduced forces.
pub const OBJECT_OBSERVATION_SCHEMA: &str = "orishu.simulation.object-observation/v1";
const HEADER: usize = 16;
const MAX_METADATA: usize = 2048;
const MAGIC: &[u8; 4] = b"OOF1";

/// Caller-owned limits: aggregate bytes before metadata allocation, packet record
/// counts before their numeric scans. No allocation is driven by a record count.
#[derive(Clone, Copy, Debug)]
pub struct ObjectObservationLimits {
    /// Complete frame, including header, metadata and both scientific packets.
    pub bytes: usize,
    /// Complete objects; the force count cannot exceed this bound either.
    pub objects: usize,
}
impl Default for ObjectObservationLimits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024 * 1024,
            objects: 1_000_000,
        }
    }
}

/// Bounded failures without payload, credentials or arbitrary remote diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ObjectObservationError {
    /// Byte/count/arithmetic/allocation limit exceeded.
    #[error("object observation budget exceeded")]
    Limit,
    /// Unknown version, malformed metadata, truncation or trailing bytes.
    #[error("invalid object observation framing")]
    Framing,
    /// Not a committed source, invalid epoch/time, or not the requested source.
    #[error("object observation source mismatch")]
    Source,
    /// Expected frame digest differs from the complete received bytes.
    #[error("object observation integrity mismatch")]
    Integrity,
    /// Forces must cover exactly all dynamic objects after boundary zero.
    #[error("object observation force coverage mismatch")]
    Coverage,
    /// Malformed or over-budget numeric packet.
    #[error(transparent)]
    Bulk(#[from] BulkError),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    api_version: ApiVersion,
    source: SnapshotSource,
}
impl Project for Metadata {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("source", Some(self.source.project())),
        ])
    }
}
fn metadata_limits() -> Limits {
    Limits {
        max_payload_bytes: MAX_METADATA,
        // The shared canonical reader also checks each text length against the
        // remaining value budget. Leave room for full sha256-prefixed digests.
        max_values: 256,
        max_depth: 4,
        max_text_bytes: 128,
        max_object_fields: 8,
        ..Limits::default()
    }
}
impl Metadata {
    fn encode(&self) -> Result<Vec<u8>, ObjectObservationError> {
        if self.api_version.as_str() != OBJECT_OBSERVATION_SCHEMA {
            return Err(ObjectObservationError::Framing);
        }
        boundary(&self.source)?;
        codec::encode(&self.project(), &metadata_limits(), MAX_METADATA)
            .map_err(|_| ObjectObservationError::Framing)
    }
    fn read(bytes: &[u8]) -> Result<Self, ObjectObservationError> {
        let metadata: Self = codec::structured_from_cbor(bytes, &metadata_limits(), MAX_METADATA)
            .map_err(|_| ObjectObservationError::Framing)?;
        if metadata.encode()? != bytes {
            return Err(ObjectObservationError::Framing);
        }
        Ok(metadata)
    }
}
fn boundary(source: &SnapshotSource) -> Result<u64, ObjectObservationError> {
    match source {
        SnapshotSource::Committed {
            epoch,
            boundary,
            time_seconds,
            ..
        } if *epoch != 0 && time_seconds.get() >= 0.0 => Ok(*boundary),
        _ => Err(ObjectObservationError::Source),
    }
}
fn checked_size(
    metadata: usize,
    objects: usize,
    forces: usize,
    limits: ObjectObservationLimits,
) -> Result<usize, ObjectObservationError> {
    if metadata > MAX_METADATA || u32::try_from(objects).is_err() || u32::try_from(forces).is_err()
    {
        return Err(ObjectObservationError::Limit);
    }
    HEADER
        .checked_add(metadata)
        .and_then(|n| n.checked_add(objects))
        .and_then(|n| n.checked_add(forces))
        .filter(|n| *n <= limits.bytes)
        .ok_or(ObjectObservationError::Limit)
}
fn packets<'a>(
    source: &SnapshotSource,
    objects: &'a [u8],
    forces: Option<&'a [u8]>,
    limits: ObjectObservationLimits,
) -> Result<(Batch<'a, ObjectState>, Option<Batch<'a, Force>>), ObjectObservationError> {
    let step = boundary(source)?;
    if (step == 0) != forces.is_none() {
        return Err(ObjectObservationError::Coverage);
    }
    let bulk = BulkLimits {
        bytes: limits.bytes,
        records: limits.objects,
    };
    let objects = Batch::<ObjectState>::read(objects, bulk)?;
    let forces = forces
        .map(|bytes| Batch::<Force>::read(bytes, bulk))
        .transpose()?;
    if let Some(forces) = &forces {
        // Sorted identities permit a linear merge with constant auxiliary space.
        let mut rows = forces.iter();
        for object in objects.iter().filter(|object| object.dynamic().is_some()) {
            if rows.next().is_none_or(|force| force.id != object.id) {
                return Err(ObjectObservationError::Coverage);
            }
        }
        if rows.next().is_some() {
            return Err(ObjectObservationError::Coverage);
        }
    }
    Ok((objects, forces))
}

/// Validated borrowed full object projection. Numeric buffers are never copied
/// or expanded into per-object allocations. Only bounded cold metadata is owned.
/// Successful decoding proves framing/integrity, not that a server committed it;
/// consumers must authenticate the producer and correlate the exact run source.
pub struct ObjectObservation<'a> {
    bytes: &'a [u8],
    id: ArtifactDigest,
    source: SnapshotSource,
    objects: Batch<'a, ObjectState>,
    forces: Option<Batch<'a, Force>>,
}
impl<'a> ObjectObservation<'a> {
    /// Validate size/framing, expected complete-frame digest, canonical metadata,
    /// all numeric records and exact force membership before exposing any values.
    /// O(bytes + objects) work; numeric data are borrowed from `bytes`.
    pub fn read(
        bytes: &'a [u8],
        expected: ArtifactDigest,
        limits: ObjectObservationLimits,
    ) -> Result<Self, ObjectObservationError> {
        if bytes.len() > limits.bytes {
            return Err(ObjectObservationError::Limit);
        }
        if bytes.len() < HEADER || &bytes[..4] != MAGIC {
            return Err(ObjectObservationError::Framing);
        }
        let length = |start| {
            u32::from_le_bytes(bytes[start..start + 4].try_into().expect("checked header")) as usize
        };
        let metadata = length(4);
        let objects = length(8);
        let forces = length(12);
        let end = checked_size(metadata, objects, forces, limits)?;
        if end != bytes.len() {
            return Err(ObjectObservationError::Framing);
        }
        if !expected.matches(bytes) {
            return Err(ObjectObservationError::Integrity);
        }
        let object_start = HEADER + metadata;
        let force_start = object_start + objects;
        let metadata = Metadata::read(&bytes[HEADER..object_start])?;
        let (objects, forces) = packets(
            &metadata.source,
            &bytes[object_start..force_start],
            (forces != 0).then_some(&bytes[force_start..]),
            limits,
        )?;
        Ok(Self {
            bytes,
            id: expected,
            source: metadata.source,
            objects,
            forces,
        })
    }
    /// Content identity of this entire projection, not its workload or run ID.
    pub fn id(&self) -> ArtifactDigest {
        self.id
    }
    /// Exact canonical frame; borrowed from the validated input.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    /// Immutable workload, run descriptor, epoch, boundary and SI simulation time.
    pub fn source(&self) -> &SnapshotSource {
        &self.source
    }
    /// All numeric objects, including static/kinematic objects. Other authored
    /// components remain in the immutable workload, not in this projection.
    pub fn objects(&self) -> &Batch<'a, ObjectState> {
        &self.objects
    }
    /// Complete net forces for dynamic objects, absent only at boundary zero.
    pub fn forces(&self) -> Option<&Batch<'a, Force>> {
        self.forces.as_ref()
    }
    /// Forces at boundary N were evaluated from N-1 kinematics. They are not
    /// recomputed forces at the positions displayed at N. None means uncomputed.
    pub fn force_evaluation_boundary(&self) -> Option<u64> {
        boundary(&self.source)
            .expect("validated source")
            .checked_sub(1)
    }
    /// Correlate all source fields before adopting or composing observations.
    /// Matching only a timestep, filename, or mutable current-run alias is unsafe.
    pub fn check_source(&self, expected: &SnapshotSource) -> Result<(), ObjectObservationError> {
        if &self.source != expected {
            return Err(ObjectObservationError::Source);
        }
        Ok(())
    }
}

/// Encode one complete object projection into reusable caller-owned storage.
/// Rejection leaves output bytes unchanged. Input packets are revalidated before
/// copying; the caller retains its committed lease throughout encoding/delivery.
/// O(bytes + objects) work, bounded cold-metadata allocations, and at most one
/// output reservation. This does not claim allocation-free observation delivery.
pub fn encode_object_observation(
    source: &SnapshotSource,
    objects: &[u8],
    forces: Option<&[u8]>,
    output: &mut Vec<u8>,
    limits: ObjectObservationLimits,
) -> Result<ArtifactDigest, ObjectObservationError> {
    checked_size(0, objects.len(), forces.map_or(0, <[u8]>::len), limits)?;
    packets(source, objects, forces, limits)?;
    let metadata = Metadata {
        api_version: OBJECT_OBSERVATION_SCHEMA.parse().expect("static schema"),
        source: source.clone(),
    }
    .encode()?;
    let size = checked_size(
        metadata.len(),
        objects.len(),
        forces.map_or(0, <[u8]>::len),
        limits,
    )?;
    output
        .try_reserve_exact(size.saturating_sub(output.len()))
        .map_err(|_| ObjectObservationError::Limit)?;
    output.clear();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    output.extend_from_slice(&(objects.len() as u32).to_le_bytes());
    output.extend_from_slice(&(forces.map_or(0, <[u8]>::len) as u32).to_le_bytes());
    output.extend_from_slice(&metadata);
    output.extend_from_slice(objects);
    if let Some(forces) = forces {
        output.extend_from_slice(forces);
    }
    Ok(ArtifactDigest::sha256_of(output))
}
