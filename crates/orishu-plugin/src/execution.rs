//! Portable bulk projections for the initial field-force/integration profile.
//!
//! These are scientific IO schemas, not Rust memory layouts, workload admission,
//! entity allocation or an ECS storage requirement. A packet is meaningful only
//! with its admitted workload/run/boundary and selected contribution descriptors.
//! Role-bound source/response dimensions come from those exact descriptors.
//! Readers borrow bytes, validate the entire packet before exposing records, and
//! allocate nothing. Writers reuse caller-owned storage. See `docs/scientific-bulk-io.md`.

use crate::FiniteF64;
use std::marker::PhantomData;

mod context;
pub use context::*;
mod configuration;
pub use configuration::*;
mod domain;
pub use domain::*;
mod sampling;
pub use sampling::*;
mod workload;
pub use workload::*;
mod scene;
pub use scene::*;

/// Stable identity inside one workload/run's entity set. Constructing this value
/// does not establish that the entity exists or authorize creation/reuse of it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct EntityId(pub u64);

/// Index into the selected field model's canonical coupling-slot descriptor table.
/// The table binds exact component contracts, role properties and dimensions;
/// this compact index alone is not a globally meaningful component identity.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct CouplingSlot(pub u32);

/// Intrinsic kinematics shared by dynamic and static/kinematic entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kinematics {
    /// Cartesian position in the admitted domain frame, metres.
    pub position_metres: [FiniteF64; 3],
    /// Cartesian velocity, metres per second. Without Dynamics it is not integrated.
    pub velocity_metres_per_second: [FiniteF64; 3],
}

/// Resolved Dynamics projection, before packet validation/admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DynamicEntity {
    /// Stable run-local entity identity.
    pub id: EntityId,
    /// Committed kinematics on input; candidate kinematics on output.
    pub kinematics: Kinematics,
    /// Strictly positive inertial mass in kg; not gravitational mass or charge.
    pub inertial_mass_kilograms: FiniteF64,
}

/// Numeric object state retained by the run, including static/kinematic objects
/// with no field coupling. Other authored components remain declared workload
/// data; this is not a replacement for the authoring ECS or component schemas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ObjectState {
    /// Stable run-local object identity.
    pub id: EntityId,
    /// Intrinsic position/velocity, even without Dynamics.
    pub kinematics: Kinematics,
    /// Dynamics component's positive inertial mass, absent for static/kinematic objects.
    pub inertial_mass_kilograms: Option<FiniteF64>,
}
impl ObjectState {
    /// Derived projection; absence of Dynamics never implies integrated motion.
    pub fn dynamic(&self) -> Option<DynamicEntity> {
        self.inertial_mass_kilograms.map(|mass| DynamicEntity {
            id: self.id,
            kinematics: self.kinematics,
            inertial_mass_kilograms: mass,
        })
    }
}

/// One selected field model's resolved scalar role bindings for an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoupledEntity {
    /// Stable run-local entity identity.
    pub id: EntityId,
    /// Selected coupling-component slot. An entity can supply several distinct
    /// slots; the host never combines their strengths implicitly.
    pub slot: CouplingSlot,
    /// Kinematics from the same committed boundary as this field's prior state.
    pub kinematics: Kinematics,
    /// Whether the entity has Dynamics. This is a derived projection, never a
    /// separately persisted authoring motion-authority flag.
    pub has_dynamics: bool,
    /// Source property resolved to SI in the selected coupling contract's dimension.
    pub source_si: Option<FiniteF64>,
    /// Independent response property in its selected contract's SI dimension.
    pub response_si: Option<FiniteF64>,
}
impl CoupledEntity {
    /// This entity requires an explicit force output, even if its response is zero.
    pub const fn responds_dynamically(&self) -> bool {
        self.has_dynamics && self.response_si.is_some()
    }
}

/// Complete Cartesian force contribution for one responding dynamic entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Force {
    /// Stable run-local entity identity.
    pub id: EntityId,
    /// Force in newtons, in the admitted domain frame.
    pub newtons: [FiniteF64; 3],
}

/// Byte and record ceilings supplied by the authority doing admission/execution.
#[derive(Clone, Copy, Debug)]
pub struct BulkLimits {
    /// Maximum complete packet bytes, including framing.
    pub bytes: usize,
    /// Maximum record count, checked before scanning records.
    pub records: usize,
}
impl Default for BulkLimits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024 * 1024,
            records: 1_000_000,
        }
    }
}

/// Bounded diagnostic; never copies hostile payload bytes or IDs into an error.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum BulkError {
    /// Packet size/count or caller allocation budget was exceeded.
    #[error("scientific packet limit exceeded")]
    LimitExceeded,
    /// Unknown kind/version, nonzero reserved bits, truncation or trailing bytes.
    #[error("invalid scientific packet framing")]
    Framing,
    /// Record keys are not strictly increasing; duplicate entity/slot keys are not merged.
    #[error("scientific packet entity order or uniqueness violation")]
    EntityOrder,
    /// Non-finite or noncanonical numeric value, invalid mass, flags or role values.
    #[error("invalid scientific packet record")]
    InvalidRecord,
}

mod sealed {
    pub trait Sealed {}
}
/// Closed set of versioned bulk record schemas. Implementations are platform-owned;
/// plugin-private fields/history remain opaque and do not implement this trait.
pub trait BulkRecord: sealed::Sealed + Copy {
    /// Stable descriptor schema label, independently checked by the supervisor.
    const SCHEMA: &'static str;
    /// Wire tag in the version-1 packet header.
    const KIND: u16;
    /// Exact encoded record size; never `size_of::<Self>()`.
    const BYTES: usize;
    /// Identity used for canonical ordering and completeness checks.
    fn entity_id(&self) -> EntityId;
    #[doc(hidden)]
    fn ordering_key(&self) -> (EntityId, u32) {
        (self.entity_id(), 0)
    }
    #[doc(hidden)]
    fn consistent_entity(&self, _previous: &Self) -> bool {
        true
    }
    #[doc(hidden)]
    fn validate(&self) -> Result<(), BulkError>;
    #[doc(hidden)]
    fn decode(bytes: &[u8]) -> Result<Self, BulkError>;
    #[doc(hidden)]
    fn encode(&self, output: &mut Vec<u8>);
}

/// Length of the fixed version-1 packet header.
pub const HEADER_BYTES: usize = 12;
const MAGIC: &[u8; 4] = b"OSB1";

/// Completely validated borrowed packet. Validation is O(records), allocation-free;
/// subsequent iteration is O(records), decoding values without per-record allocation.
#[derive(Clone, Copy, Debug)]
pub struct Batch<'a, R: BulkRecord> {
    bytes: &'a [u8],
    records: usize,
    marker: PhantomData<R>,
}
impl<'a, R: BulkRecord> Batch<'a, R> {
    /// Refuse byte/count excess and framing before record scanning; then validate
    /// every record and strict identity ordering before exposing any record.
    pub fn read(bytes: &'a [u8], limits: BulkLimits) -> Result<Self, BulkError> {
        if bytes.len() > limits.bytes {
            return Err(BulkError::LimitExceeded);
        }
        if bytes.len() < HEADER_BYTES
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes([bytes[4], bytes[5]]) != R::KIND
            || bytes[6..8] != [0, 0]
        {
            return Err(BulkError::Framing);
        }
        let records =
            u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| BulkError::Framing)?) as usize;
        let length = packet_bytes::<R>(records, limits)?;
        if bytes.len() != length {
            return Err(BulkError::Framing);
        }
        let mut previous = None;
        for chunk in bytes[HEADER_BYTES..].chunks_exact(R::BYTES) {
            let record = R::decode(chunk)?;
            record.validate()?;
            ordered(&mut previous, record)?;
        }
        Ok(Self {
            bytes,
            records,
            marker: PhantomData,
        })
    }
    /// Number of validated records.
    pub const fn len(&self) -> usize {
        self.records
    }
    /// Whether this is an explicitly encoded empty batch.
    pub const fn is_empty(&self) -> bool {
        self.records == 0
    }
    /// Original canonical packet; no re-encoding or allocation.
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    /// Decode one validated record in constant time, without walking preceding records.
    pub fn get(&self, index: usize) -> Option<R> {
        if index >= self.records {
            return None;
        }
        let start = HEADER_BYTES + index * R::BYTES;
        Some(
            R::decode(&self.bytes[start..start + R::BYTES])
                .expect("validated immutable scientific record"),
        )
    }
    /// Decode validated immutable bytes, without allocations or hidden sorting.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = R> + '_ {
        self.bytes[HEADER_BYTES..]
            .chunks_exact(R::BYTES)
            .map(|bytes| R::decode(bytes).expect("validated immutable scientific record"))
    }
}

/// Determine the exact caller-owned output size with checked arithmetic.
pub fn packet_bytes<R: BulkRecord>(records: usize, limits: BulkLimits) -> Result<usize, BulkError> {
    if records > limits.records || u32::try_from(records).is_err() {
        return Err(BulkError::LimitExceeded);
    }
    records
        .checked_mul(R::BYTES)
        .and_then(|n| n.checked_add(HEADER_BYTES))
        .filter(|n| *n <= limits.bytes)
        .ok_or(BulkError::LimitExceeded)
}

/// Validate and encode into reusable storage. Rejection leaves existing output
/// bytes unchanged. Capacity is reserved once, before publishing any new bytes.
pub fn encode_batch<R: BulkRecord>(
    records: &[R],
    output: &mut Vec<u8>,
    limits: BulkLimits,
) -> Result<(), BulkError> {
    let size = packet_bytes::<R>(records.len(), limits)?;
    let mut previous = None;
    for record in records {
        record.validate()?;
        ordered(&mut previous, *record)?;
    }
    output
        .try_reserve_exact(size.saturating_sub(output.len()))
        .map_err(|_| BulkError::LimitExceeded)?;
    output.clear();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&R::KIND.to_le_bytes());
    output.extend_from_slice(&[0, 0]);
    output.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for record in records {
        record.encode(output);
    }
    Ok(())
}

fn ordered<R: BulkRecord>(previous: &mut Option<R>, record: R) -> Result<(), BulkError> {
    if previous.is_some_and(|prior| prior.ordering_key() >= record.ordering_key()) {
        return Err(BulkError::EntityOrder);
    }
    if previous.is_some_and(|prior| {
        prior.entity_id() == record.entity_id() && !record.consistent_entity(&prior)
    }) {
        return Err(BulkError::InvalidRecord);
    }
    *previous = Some(record);
    Ok(())
}
fn id(bytes: &[u8]) -> Result<EntityId, BulkError> {
    Ok(EntityId(u64::from_le_bytes(
        bytes
            .get(..8)
            .ok_or(BulkError::Framing)?
            .try_into()
            .map_err(|_| BulkError::Framing)?,
    )))
}
fn scalar(bytes: &[u8]) -> Result<FiniteF64, BulkError> {
    let bits = u64::from_le_bytes(bytes.try_into().map_err(|_| BulkError::Framing)?);
    // Negative zero has a second byte representation; writers always use +0.
    if bits == 1 << 63 {
        return Err(BulkError::InvalidRecord);
    }
    FiniteF64::new(f64::from_bits(bits)).map_err(|_| BulkError::InvalidRecord)
}
fn vector(bytes: &[u8]) -> Result<[FiniteF64; 3], BulkError> {
    if bytes.len() != 24 {
        return Err(BulkError::Framing);
    }
    Ok([
        scalar(&bytes[..8])?,
        scalar(&bytes[8..16])?,
        scalar(&bytes[16..])?,
    ])
}
fn kinematics(bytes: &[u8]) -> Result<Kinematics, BulkError> {
    if bytes.len() != 48 {
        return Err(BulkError::Framing);
    }
    Ok(Kinematics {
        position_metres: vector(&bytes[..24])?,
        velocity_metres_per_second: vector(&bytes[24..])?,
    })
}
fn put_scalar(output: &mut Vec<u8>, value: FiniteF64) {
    output.extend_from_slice(&value.get().to_le_bytes());
}
fn put_kinematics(output: &mut Vec<u8>, value: Kinematics) {
    for v in value
        .position_metres
        .into_iter()
        .chain(value.velocity_metres_per_second)
    {
        put_scalar(output, v);
    }
}

impl sealed::Sealed for ObjectState {}
impl BulkRecord for ObjectState {
    const SCHEMA: &'static str = "orishu.simulation.objects/v1";
    const KIND: u16 = 5;
    const BYTES: usize = 72;
    fn entity_id(&self) -> EntityId {
        self.id
    }
    fn validate(&self) -> Result<(), BulkError> {
        if self
            .inertial_mass_kilograms
            .is_some_and(|mass| mass.get() <= 0.0)
        {
            return Err(BulkError::InvalidRecord);
        }
        Ok(())
    }
    fn decode(bytes: &[u8]) -> Result<Self, BulkError> {
        if bytes.len() != Self::BYTES || bytes[56] > 1 || bytes[57..64] != [0; 7] {
            return Err(BulkError::InvalidRecord);
        }
        let mass = scalar(&bytes[64..72])?;
        if bytes[56] == 0 && mass != FiniteF64::ZERO {
            return Err(BulkError::InvalidRecord);
        }
        let value = Self {
            id: id(bytes)?,
            kinematics: kinematics(&bytes[8..56])?,
            inertial_mass_kilograms: (bytes[56] == 1).then_some(mass),
        };
        value.validate()?;
        Ok(value)
    }
    fn encode(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.id.0.to_le_bytes());
        put_kinematics(output, self.kinematics);
        output.push(u8::from(self.inertial_mass_kilograms.is_some()));
        output.extend_from_slice(&[0; 7]);
        put_scalar(
            output,
            self.inertial_mass_kilograms.unwrap_or(FiniteF64::ZERO),
        );
    }
}

impl sealed::Sealed for DynamicEntity {}
impl BulkRecord for DynamicEntity {
    const SCHEMA: &'static str = "orishu.dynamic-entities/v1";
    const KIND: u16 = 1;
    const BYTES: usize = 64;
    fn entity_id(&self) -> EntityId {
        self.id
    }
    fn validate(&self) -> Result<(), BulkError> {
        if !self.inertial_mass_kilograms.is_positive() {
            Err(BulkError::InvalidRecord)
        } else {
            Ok(())
        }
    }
    fn decode(bytes: &[u8]) -> Result<Self, BulkError> {
        if bytes.len() != Self::BYTES {
            return Err(BulkError::Framing);
        }
        Ok(Self {
            id: id(bytes)?,
            kinematics: kinematics(&bytes[8..56])?,
            inertial_mass_kilograms: scalar(&bytes[56..64])?,
        })
    }
    fn encode(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.id.0.to_le_bytes());
        put_kinematics(output, self.kinematics);
        put_scalar(output, self.inertial_mass_kilograms);
    }
}
impl sealed::Sealed for CoupledEntity {}
impl BulkRecord for CoupledEntity {
    const SCHEMA: &'static str = "orishu.coupled-entities/v1";
    const KIND: u16 = 2;
    const BYTES: usize = 80;
    fn entity_id(&self) -> EntityId {
        self.id
    }
    fn ordering_key(&self) -> (EntityId, u32) {
        (self.id, self.slot.0)
    }
    fn consistent_entity(&self, previous: &Self) -> bool {
        self.kinematics == previous.kinematics && self.has_dynamics == previous.has_dynamics
    }
    fn validate(&self) -> Result<(), BulkError> {
        if self.source_si.is_none() && self.response_si.is_none() {
            Err(BulkError::InvalidRecord)
        } else {
            Ok(())
        }
    }
    fn decode(bytes: &[u8]) -> Result<Self, BulkError> {
        if bytes.len() != Self::BYTES {
            return Err(BulkError::Framing);
        }
        let flags = bytes[56];
        if flags & !7 != 0 || bytes[57..60] != [0; 3] {
            return Err(BulkError::InvalidRecord);
        }
        let source = scalar(&bytes[64..72])?;
        let response = scalar(&bytes[72..80])?;
        if (flags & 2 == 0 && source != FiniteF64::ZERO)
            || (flags & 4 == 0 && response != FiniteF64::ZERO)
        {
            return Err(BulkError::InvalidRecord);
        }
        Ok(Self {
            id: id(bytes)?,
            slot: CouplingSlot(u32::from_le_bytes(
                bytes[60..64].try_into().map_err(|_| BulkError::Framing)?,
            )),
            kinematics: kinematics(&bytes[8..56])?,
            has_dynamics: flags & 1 != 0,
            source_si: (flags & 2 != 0).then_some(source),
            response_si: (flags & 4 != 0).then_some(response),
        })
    }
    fn encode(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.id.0.to_le_bytes());
        put_kinematics(output, self.kinematics);
        let flags = u8::from(self.has_dynamics)
            | (u8::from(self.source_si.is_some()) << 1)
            | (u8::from(self.response_si.is_some()) << 2);
        output.extend_from_slice(&[flags, 0, 0, 0]);
        output.extend_from_slice(&self.slot.0.to_le_bytes());
        put_scalar(output, self.source_si.unwrap_or(FiniteF64::ZERO));
        put_scalar(output, self.response_si.unwrap_or(FiniteF64::ZERO));
    }
}
impl sealed::Sealed for Force {}
impl BulkRecord for Force {
    const SCHEMA: &'static str = "orishu.forces/v1";
    const KIND: u16 = 3;
    const BYTES: usize = 32;
    fn entity_id(&self) -> EntityId {
        self.id
    }
    fn validate(&self) -> Result<(), BulkError> {
        Ok(())
    }
    fn decode(bytes: &[u8]) -> Result<Self, BulkError> {
        if bytes.len() != Self::BYTES {
            return Err(BulkError::Framing);
        }
        Ok(Self {
            id: id(bytes)?,
            newtons: vector(&bytes[8..32])?,
        })
    }
    fn encode(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.id.0.to_le_bytes());
        for value in self.newtons {
            put_scalar(output, value);
        }
    }
}

impl sealed::Sealed for EntityId {}
impl BulkRecord for EntityId {
    const SCHEMA: &'static str = "orishu.entity-ids/v1";
    const KIND: u16 = 4;
    const BYTES: usize = 8;
    fn entity_id(&self) -> EntityId {
        *self
    }
    fn validate(&self) -> Result<(), BulkError> {
        Ok(())
    }
    fn decode(bytes: &[u8]) -> Result<Self, BulkError> {
        if bytes.len() != Self::BYTES {
            return Err(BulkError::Framing);
        }
        id(bytes)
    }
    fn encode(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.0.to_le_bytes());
    }
}
