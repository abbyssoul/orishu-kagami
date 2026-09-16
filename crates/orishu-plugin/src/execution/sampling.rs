//! Exact-channel sampling over immutable snapshots. Numeric buffers are flat;
//! point geometry and field-private state interpretation do not live here.
use super::{InputIdentity, InstanceContext};
use crate::{
    ArtifactDigest, Declaration, Error, ExecutionContractId, FiniteF64, Limits,
    LocalContributionId, ObservableSchema, Payload, ScientificContractRef, Shape, codec,
    projection::{Project, object},
};
use orishu_resource::ApiVersion;
use orishu_workload::{ComponentInstanceId, WorkloadDigest, canonical::CanonicalValue as V};
use serde::{Deserialize, Serialize};

/// Point request packet schema; logical count is the number of points.
pub const SAMPLE_REQUEST_SCHEMA: &str = "orishu.simulation.sample-request/v1";
/// Flat response schema, paired with its exact request; logical count is points.
pub const SAMPLE_RESPONSE_SCHEMA: &str = "orishu.simulation.sample-response/v1";
/// Cold metadata inside the point request packet.
pub const SAMPLE_METADATA_SCHEMA: &str = "orishu.simulation.sample-metadata/v1";
/// Hard bound on cold metadata before decoding its tree.
pub const MAX_SAMPLE_METADATA_BYTES: usize = 65_536;

/// Kernel-declared compute precision, independent of binary64 interchange values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComputePrecision {
    /// Binary32 scientific arithmetic.
    Binary32,
    /// Binary64 scientific arithmetic.
    Binary64,
}
impl Project for ComputePrecision {
    fn project(&self) -> V {
        V::text(match self {
            Self::Binary32 => "binary32",
            Self::Binary64 => "binary64",
        })
    }
}

/// Exact typed channel vocabulary; dimensions and coordinate conventions are not
/// inferred from a name or from vector length. Admission verifies its provider.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SampleChannel {
    /// Scientific content identity, independent of the computational provider.
    pub contract: ScientificContractRef,
    /// Exact vocabulary whose semantic digest must equal `contract`.
    pub schema: ObservableSchema,
}
impl Eq for SampleChannel {}
impl SampleChannel {
    /// Validate schema constraints and exact scientific identity.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema.requirements.len() > 64
            || self.schema.axes.len() > 2
            || [
                &self.schema.meaning,
                &self.schema.frame,
                &self.schema.conventions,
            ]
            .into_iter()
            .chain(&self.schema.axes)
            .any(|s| s.len() > 4096)
        {
            return Err(Error::malformed(
                "channel",
                "observable metadata exceeds bounds",
            ));
        }
        let payload = Payload::Observables(Declaration {
            scientific: self.schema.clone(),
            presentation: None,
        });
        if payload.contract_ref(&metadata_limits())? != self.contract {
            return Err(Error::malformed(
                "channel",
                "observable scientific identity mismatch",
            ));
        }
        Ok(())
    }
    /// Number of row-major numeric components after validation.
    pub fn components(&self) -> usize {
        match self.schema.shape {
            Shape::Scalar => 1,
            Shape::Vector { length } => length as usize,
            Shape::Matrix { rows, columns } => usize::from(rows) * usize::from(columns),
        }
    }
}
impl Project for SampleChannel {
    fn project(&self) -> V {
        object(vec![
            ("contract", Some(self.contract.project())),
            ("schema", Some(self.schema.project())),
        ])
    }
}

/// One selected model's supplied observable requirement slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservableBinding {
    /// Kernel's declared requirement slot; context bindings sort strictly by slot.
    pub slot: LocalContributionId,
    /// Exact supplied vocabulary, not a channel name match.
    pub channel: SampleChannel,
    /// Supported quality flags: direct=1, interpolation=2, reconstruction=4.
    /// Any nonempty subset/combination may be reported per valid cell.
    pub quality_flags: u32,
}
impl Project for ObservableBinding {
    fn project(&self) -> V {
        object(vec![
            ("slot", Some(self.slot.project())),
            ("channel", Some(self.channel.project())),
            ("qualityFlags", Some(self.quality_flags.project())),
        ])
    }
}

/// Identity domain of an immutable snapshot. These values do not themselves
/// prove that an authority committed or leased the named state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SnapshotSource {
    /// Exact canonical authored-revision artifact, before a run exists.
    Authored {
        /// Digest of the captured revision, not a mutable filename.
        revision: ArtifactDigest,
    },
    /// One fixed scientific boundary of an identified run.
    Committed {
        /// Immutable workload root digest.
        workload: WorkloadDigest,
        /// Content identity of the immutable run descriptor (not its current state).
        run: ArtifactDigest,
        /// Execution/recovery epoch.
        epoch: u64,
        /// Collectively committed fixed boundary.
        boundary: u64,
        /// SI simulation time, never wall-clock time.
        time_seconds: FiniteF64,
    },
}
impl Project for SnapshotSource {
    fn project(&self) -> V {
        match self {
            Self::Authored { revision } => object(vec![
                ("kind", Some(V::text("authored"))),
                ("revision", Some(revision.project())),
            ]),
            Self::Committed {
                workload,
                run,
                epoch,
                boundary,
                time_seconds,
            } => object(vec![
                ("kind", Some(V::text("committed"))),
                ("workload", Some(V::text(workload.to_string()))),
                ("run", Some(run.project())),
                ("epoch", Some(epoch.project())),
                ("boundary", Some(boundary.project())),
                ("timeSeconds", Some(time_seconds.project())),
            ]),
        }
    }
}
/// Complete immutable field snapshot reference within its source boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SampleSnapshot {
    /// Authored or committed origin.
    pub source: SnapshotSource,
    /// Exact state schema/count/bytes/digest, not sampled-value identity.
    pub state: InputIdentity,
}
impl Project for SampleSnapshot {
    fn project(&self) -> V {
        object(vec![
            ("source", Some(self.source.project())),
            ("state", Some(self.state.project())),
        ])
    }
}
/// Cold query identity; the flat packet supplies points in observer-chosen order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SampleMetadata {
    /// Exactly `orishu.simulation.sample-metadata/v1`.
    pub api_version: ApiVersion,
    /// Observer-scoped request identity, unchanged by batching/transport.
    pub request_id: u64,
    /// Exact leased immutable snapshot.
    pub snapshot: SampleSnapshot,
    /// Field instance, not the plugin installation or field-family name.
    pub field: ComponentInstanceId,
    /// Digest of exact canonical InstanceContext, including kernel/configuration.
    pub context: ArtifactDigest,
    /// Requested channel order. Unsupported exact contracts stay unavailable.
    pub channels: Vec<SampleChannel>,
}
fn metadata_limits() -> Limits {
    Limits {
        max_payload_bytes: MAX_SAMPLE_METADATA_BYTES,
        max_values: 8192,
        max_depth: 16,
        max_text_bytes: 4096,
        max_schema_items: 128,
        max_channels: 128,
        ..Limits::default()
    }
}
impl SampleMetadata {
    /// Bounded schema/identity validation; no state access or registry lookup.
    pub fn validate(&self) -> Result<(), Error> {
        if self.api_version.as_str() != SAMPLE_METADATA_SCHEMA
            || self.channels.is_empty()
            || self.channels.len() > 128
        {
            return Err(Error::malformed(
                "sampling",
                "invalid sample metadata version or channel count",
            ));
        }
        if let SnapshotSource::Committed { time_seconds, .. } = self.snapshot.source
            && time_seconds.get() < 0.0
        {
            return Err(Error::malformed("snapshot", "negative simulation time"));
        }
        for (i, channel) in self.channels.iter().enumerate() {
            channel.validate()?;
            if self.channels[..i]
                .iter()
                .any(|c| c.contract == channel.contract)
            {
                return Err(Error::malformed("channels", "duplicate requested channel"));
            }
        }
        Ok(())
    }
    /// Canonical bounded metadata; no point array is materialized in this tree.
    pub fn to_cbor(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        codec::encode(
            &self.project(),
            &metadata_limits(),
            MAX_SAMPLE_METADATA_BYTES,
        )
    }
    /// Decode metadata within fixed byte/depth/value bounds and require canonical bytes.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, Error> {
        let v: Self =
            codec::structured_from_cbor(bytes, &metadata_limits(), MAX_SAMPLE_METADATA_BYTES)?;
        if v.to_cbor()? != bytes {
            return Err(Error::malformed("sampling", "noncanonical sample metadata"));
        }
        Ok(v)
    }
    /// Bind to the exact selected field context, never substitute by channel name.
    pub fn check_context(&self, context: &InstanceContext) -> Result<(), SampleError> {
        if context.execution_contract != ExecutionContractId::Field
            || self.field != context.instance
            || self.channels.len() > context.bounds.sample_channels as usize
            || !self
                .context
                .matches(&context.to_cbor().map_err(|_| SampleError::Context)?)
        {
            return Err(SampleError::Context);
        }
        Ok(())
    }
}
impl Project for SampleMetadata {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("requestId", Some(self.request_id.project())),
            ("snapshot", Some(self.snapshot.project())),
            ("field", Some(V::text(self.field.as_str()))),
            ("context", Some(self.context.project())),
            ("channels", Some(self.channels.project())),
        ])
    }
}

/// A finite observer-generated point; IDs may be arbitrary but must be unique.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplePoint {
    /// Request-local stable correspondence.
    pub id: u64,
    /// World Cartesian metres, x/y/z.
    pub position_metres: [FiniteF64; 3],
}
/// Explicit query/response budgets, checked before count products or allocations.
#[derive(Clone, Copy, Debug)]
pub struct SampleLimits {
    /// Maximum one packet, including metadata/framing.
    pub bytes: usize,
    /// Point ceiling.
    pub points: usize,
    /// Requested channels.
    pub channels: usize,
    /// Sum of point × component counts across channels.
    pub values: usize,
}
impl Default for SampleLimits {
    fn default() -> Self {
        Self {
            bytes: 8 * 1024 * 1024,
            points: 4096,
            channels: 16,
            values: 1_048_576,
        }
    }
}
/// Bounded transport-neutral sampling rejection; invalid cells are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum SampleError {
    /// Bytes/counts/work or allocation budget exhausted.
    #[error("sample budget exceeded")]
    Limit,
    /// Malformed/corrupt version, framing or identity.
    #[error("invalid sampling packet")]
    Framing,
    /// Duplicate IDs, non-finite coordinates or invalid channel declaration.
    #[error("invalid sampling request")]
    Request,
    /// Field/snapshot/configuration/kernel context mismatch.
    #[error("sampling context mismatch")]
    Context,
    /// Missing, duplicate, non-finite or invalid-quality output cell.
    #[error("invalid or incomplete sampling output")]
    Output,
}
/// Reused point-ID scratch, O(points), never an allocation per point or cell.
#[derive(Default)]
pub struct SampleScratch {
    ids: Vec<u64>,
}
fn u32_at(bytes: &[u8], start: usize) -> Result<u32, SampleError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(start..start + 4)
            .ok_or(SampleError::Framing)?
            .try_into()
            .map_err(|_| SampleError::Framing)?,
    ))
}
fn point(row: &[u8]) -> Result<SamplePoint, SampleError> {
    if row.len() != 32 {
        return Err(SampleError::Framing);
    }
    let id = u64::from_le_bytes(row[..8].try_into().map_err(|_| SampleError::Framing)?);
    let mut position_metres = [FiniteF64::ZERO; 3];
    for (i, v) in position_metres.iter_mut().enumerate() {
        let bits = u64::from_le_bytes(
            row[8 + 8 * i..16 + 8 * i]
                .try_into()
                .map_err(|_| SampleError::Framing)?,
        );
        if bits == 1 << 63 {
            return Err(SampleError::Request);
        }
        *v = FiniteF64::new(f64::from_bits(bits)).map_err(|_| SampleError::Request)?;
    }
    Ok(SamplePoint {
        id,
        position_metres,
    })
}
impl SampleScratch {
    fn unique(&mut self, ids: impl Iterator<Item = u64>, count: usize) -> Result<(), SampleError> {
        self.ids
            .try_reserve(count.saturating_sub(self.ids.len()))
            .map_err(|_| SampleError::Limit)?;
        self.ids.clear();
        self.ids.extend(ids);
        self.ids.sort_unstable();
        if self.ids.windows(2).any(|p| p[0] == p[1]) {
            return Err(SampleError::Request);
        }
        Ok(())
    }
}
/// Fully validated borrowed points plus bounded owned cold query metadata.
pub struct SampleRequest<'a> {
    bytes: &'a [u8],
    metadata: SampleMetadata,
    offset: usize,
    count: usize,
}
impl<'a> SampleRequest<'a> {
    /// Validate complete framing/metadata/point correspondence with reusable scratch.
    pub fn read(
        bytes: &'a [u8],
        scratch: &mut SampleScratch,
        limits: SampleLimits,
    ) -> Result<Self, SampleError> {
        if bytes.len() > limits.bytes {
            return Err(SampleError::Limit);
        }
        if bytes.get(..4) != Some(b"OSQ1") {
            return Err(SampleError::Framing);
        }
        let n = u32_at(bytes, 4)? as usize;
        let count = u32_at(bytes, 8)? as usize;
        if n > MAX_SAMPLE_METADATA_BYTES || count > limits.points {
            return Err(SampleError::Limit);
        }
        let offset = 12usize.checked_add(n).ok_or(SampleError::Limit)?;
        let end = count
            .checked_mul(32)
            .and_then(|v| v.checked_add(offset))
            .ok_or(SampleError::Limit)?;
        if end != bytes.len() {
            return Err(SampleError::Framing);
        }
        let metadata =
            SampleMetadata::from_cbor(&bytes[12..offset]).map_err(|_| SampleError::Request)?;
        response_layout(&metadata, count, limits)?;
        for row in bytes[offset..].chunks_exact(32) {
            point(row)?;
        }
        scratch.unique(
            bytes[offset..]
                .chunks_exact(32)
                .map(|r| u64::from_le_bytes(r[..8].try_into().expect("validated point"))),
            count,
        )?;
        Ok(Self {
            bytes,
            metadata,
            offset,
            count,
        })
    }
    /// Exact cold request identity and ordered channel descriptors.
    pub fn metadata(&self) -> &SampleMetadata {
        &self.metadata
    }
    /// Number of points, including explicitly empty batches.
    pub fn len(&self) -> usize {
        self.count
    }
    /// An explicit zero-point query (still with identified channels).
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Original identified packet bytes.
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
    /// Check exact instance metadata and its point budget. Snapshot provenance
    /// and access to the actual identified state remain the caller's authority.
    pub fn check_context(&self, context: &InstanceContext) -> Result<(), SampleError> {
        self.metadata.check_context(context)?;
        if self.count > context.bounds.sample_points as usize {
            return Err(SampleError::Limit);
        }
        Ok(())
    }
    /// Decode points without per-point allocation, preserving request order.
    pub fn points(&self) -> impl ExactSizeIterator<Item = SamplePoint> + '_ {
        self.bytes[self.offset..]
            .chunks_exact(32)
            .map(|r| point(r).expect("validated immutable point"))
    }
    /// Exact flat response allocation and per-channel ranges before execution.
    pub fn layout(&self, limits: SampleLimits) -> Result<SampleLayout, SampleError> {
        response_layout(&self.metadata, self.count, limits)
    }
}
/// Encode a request using reusable storage; rejection leaves prior bytes intact.
pub fn encode_sample_request(
    metadata: &SampleMetadata,
    points: &[SamplePoint],
    output: &mut Vec<u8>,
    scratch: &mut SampleScratch,
    limits: SampleLimits,
) -> Result<(), SampleError> {
    if points.len() > limits.points || u32::try_from(points.len()).is_err() {
        return Err(SampleError::Limit);
    }
    response_layout(metadata, points.len(), limits)?;
    let meta = metadata.to_cbor().map_err(|_| SampleError::Request)?;
    let len = points
        .len()
        .checked_mul(32)
        .and_then(|n| n.checked_add(12 + meta.len()))
        .filter(|n| *n <= limits.bytes)
        .ok_or(SampleError::Limit)?;
    scratch.unique(points.iter().map(|p| p.id), points.len())?;
    output
        .try_reserve(len.saturating_sub(output.len()))
        .map_err(|_| SampleError::Limit)?;
    output.clear();
    output.extend_from_slice(b"OSQ1");
    output.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    output.extend_from_slice(&(points.len() as u32).to_le_bytes());
    output.extend_from_slice(&meta);
    for p in points {
        output.extend_from_slice(&p.id.to_le_bytes());
        for v in p.position_metres {
            output.extend_from_slice(&v.get().to_le_bytes());
        }
    }
    Ok(())
}
/// Disjoint flat channel ranges, derived from checked request shapes/counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleChannelLayout {
    /// Byte offset of point validity u32s.
    pub validity_offset: usize,
    /// Byte offset of per-point quality masks.
    pub quality_offset: usize,
    /// Byte offset of row-major finite binary64 values.
    pub values_offset: usize,
    /// Values in each point's row.
    pub components: usize,
    /// Byte stride between point rows.
    pub point_stride: usize,
    /// Number of point rows.
    pub points: usize,
}
/// Complete response extent and O(channels) cold layout, not per-cell objects.
#[derive(Clone, Debug)]
pub struct SampleLayout {
    /// Exact whole response bytes.
    pub bytes: usize,
    /// Channels in the request's order.
    pub channels: Vec<SampleChannelLayout>,
}
fn response_layout(
    metadata: &SampleMetadata,
    count: usize,
    limits: SampleLimits,
) -> Result<SampleLayout, SampleError> {
    if count > limits.points
        || metadata.channels.is_empty()
        || metadata.channels.len() > limits.channels.min(128)
    {
        return Err(SampleError::Limit);
    }
    let mut bytes = 40usize;
    let mut values = 0usize;
    // Validate all count arithmetic before allocating even the channel descriptors.
    for c in &metadata.channels {
        let components = c.components();
        if !(1..=256).contains(&components) {
            return Err(SampleError::Request);
        }
        let n = count.checked_mul(components).ok_or(SampleError::Limit)?;
        values = values
            .checked_add(n)
            .filter(|v| *v <= limits.values)
            .ok_or(SampleError::Limit)?;
        bytes = n
            .checked_mul(8)
            .and_then(|v| count.checked_mul(8).and_then(|flags| v.checked_add(flags)))
            .and_then(|v| bytes.checked_add(v))
            .filter(|v| *v <= limits.bytes)
            .ok_or(SampleError::Limit)?;
    }
    let mut channels = Vec::new();
    channels
        .try_reserve(metadata.channels.len())
        .map_err(|_| SampleError::Limit)?;
    let mut offset = 40;
    for c in &metadata.channels {
        let components = c.components();
        let values_offset = offset + 8 * count;
        channels.push(SampleChannelLayout {
            validity_offset: offset,
            quality_offset: offset + 4 * count,
            values_offset,
            components,
            point_stride: 8 * components,
            points: count,
        });
        offset = values_offset + 8 * components * count;
    }
    Ok(SampleLayout { bytes, channels })
}

/// Per-cell absence/invalidity, independent of quality and never an encoded zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum SampleInvalidity {
    /// Point outside the model domain.
    OutsideDomain = 1,
    /// No finite/defined value under this model at this point.
    Undefined = 2,
    /// A declared singular point/exclusion region.
    Singular = 3,
    /// Exact requested vocabulary is not supplied by this model.
    ChannelUnavailable = 4,
}
fn invalidity(code: u32) -> Result<SampleInvalidity, SampleError> {
    match code {
        1 => Ok(SampleInvalidity::OutsideDomain),
        2 => Ok(SampleInvalidity::Undefined),
        3 => Ok(SampleInvalidity::Singular),
        4 => Ok(SampleInvalidity::ChannelUnavailable),
        _ => Err(SampleError::Output),
    }
}
fn allowed(request: &SampleRequest<'_>, context: &InstanceContext, channel: usize) -> Option<u32> {
    context
        .observables
        .iter()
        .find(|b| b.channel.contract == request.metadata.channels[channel].contract)
        .map(|b| b.quality_flags)
}
/// Borrowed valid numeric row. Iteration decodes binary64 without allocation.
pub struct SampleValues<'a>(&'a [u8]);
impl SampleValues<'_> {
    /// Finite SI components, row-major for matrices.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = FiniteF64> + '_ {
        self.0.chunks_exact(8).map(|v| {
            FiniteF64::new(f64::from_le_bytes(v.try_into().expect("validated value")))
                .expect("validated finite value")
        })
    }
}
/// Logical cell view. Invalid cells expose no numeric bytes or quality value.
pub enum SampleCell<'a> {
    /// Finite complete row with independently checked quality flags.
    Valid {
        /// Borrowed finite SI row.
        values: SampleValues<'a>,
        /// Direct/interpolated/reconstructed bit flags.
        quality_flags: u32,
    },
    /// Explicit invalidity, no numeric values.
    Invalid(SampleInvalidity),
}
/// Complete response paired with the exact request and selected context. Retained
/// recordings must keep that request/context, not only these response bytes.
pub struct SampleResponse<'a> {
    bytes: &'a [u8],
    layout: SampleLayout,
}
impl<'a> SampleResponse<'a> {
    /// Validate the exact query digest, precision, complete cell coverage, finite
    /// values and per-model quality support. Invalid numeric slots are not read.
    pub fn read(
        bytes: &'a [u8],
        request: &SampleRequest<'_>,
        context: &InstanceContext,
        limits: SampleLimits,
    ) -> Result<Self, SampleError> {
        request.check_context(context)?;
        let layout = request.layout(limits)?;
        if bytes.len() != layout.bytes
            || bytes.get(..4) != Some(b"OSP1")
            || bytes.get(4..36) != Some(ArtifactDigest::sha256_of(request.bytes).as_bytes())
            || u32_at(bytes, 36)?
                != match context.compute_precision {
                    ComputePrecision::Binary32 => 32,
                    ComputePrecision::Binary64 => 64,
                }
        {
            return Err(SampleError::Context);
        }
        for (channel, c) in layout.channels.iter().enumerate() {
            let quality = allowed(request, context, channel);
            for p in 0..c.points {
                let status = u32_at(bytes, c.validity_offset + 4 * p)?;
                let flags = u32_at(bytes, c.quality_offset + 4 * p)?;
                if status == 0 {
                    if quality.is_none_or(|allowed| flags == 0 || flags & !allowed != 0) {
                        return Err(SampleError::Output);
                    }
                    let start = c.values_offset + p * c.point_stride;
                    for v in bytes[start..start + c.point_stride].chunks_exact(8) {
                        let bits =
                            u64::from_le_bytes(v.try_into().map_err(|_| SampleError::Output)?);
                        if bits == 1 << 63 || !f64::from_bits(bits).is_finite() {
                            return Err(SampleError::Output);
                        }
                    }
                } else {
                    let reason = invalidity(status)?;
                    if flags != 0
                        || (quality.is_none() && reason != SampleInvalidity::ChannelUnavailable)
                        || (quality.is_some() && reason == SampleInvalidity::ChannelUnavailable)
                    {
                        return Err(SampleError::Output);
                    }
                }
            }
        }
        Ok(Self { bytes, layout })
    }
    /// Flat checked descriptor ranges in request-channel order.
    pub fn layout(&self) -> &SampleLayout {
        &self.layout
    }
    /// Logical cell in request point/channel order. No per-cell allocation.
    pub fn cell(&self, point: usize, channel: usize) -> Option<SampleCell<'_>> {
        let c = self.layout.channels.get(channel)?;
        if point >= c.points {
            return None;
        }
        let status = u32_at(self.bytes, c.validity_offset + 4 * point).expect("validated status");
        if status != 0 {
            return Some(SampleCell::Invalid(
                invalidity(status).expect("validated invalidity"),
            ));
        }
        let start = c.values_offset + point * c.point_stride;
        Some(SampleCell::Valid {
            values: SampleValues(&self.bytes[start..start + c.point_stride]),
            quality_flags: u32_at(self.bytes, c.quality_offset + 4 * point)
                .expect("validated quality"),
        })
    }
}
/// Reusable flat candidate writer. Every point/channel must be written exactly
/// once; an unfinished or invalid cell prevents `finish`. Inputs remain borrowed.
pub struct SampleOutput<'a, 'q> {
    output: &'a mut Vec<u8>,
    request: &'q SampleRequest<'q>,
    context: &'q InstanceContext,
    layout: SampleLayout,
    limits: SampleLimits,
    poisoned: bool,
}
impl<'a, 'q> SampleOutput<'a, 'q> {
    /// Reserve once, then initialize explicit incomplete markers; not valid zeros.
    pub fn new(
        output: &'a mut Vec<u8>,
        request: &'q SampleRequest<'q>,
        context: &'q InstanceContext,
        limits: SampleLimits,
    ) -> Result<Self, SampleError> {
        request.check_context(context)?;
        let layout = request.layout(limits)?;
        output
            .try_reserve(layout.bytes.saturating_sub(output.len()))
            .map_err(|_| SampleError::Limit)?;
        output.clear();
        output.resize(layout.bytes, 0);
        output[..4].copy_from_slice(b"OSP1");
        output[4..36].copy_from_slice(ArtifactDigest::sha256_of(request.bytes).as_bytes());
        output[36..40].copy_from_slice(
            &match context.compute_precision {
                ComputePrecision::Binary32 => 32u32,
                ComputePrecision::Binary64 => 64,
            }
            .to_le_bytes(),
        );
        for c in &layout.channels {
            output[c.validity_offset..c.quality_offset].fill(0xff);
        }
        Ok(Self {
            output,
            request,
            context,
            layout,
            limits,
            poisoned: false,
        })
    }
    fn location(
        &mut self,
        point: usize,
        channel: usize,
    ) -> Result<SampleChannelLayout, SampleError> {
        let c = self
            .layout
            .channels
            .get(channel)
            .copied()
            .filter(|c| point < c.points);
        if self.poisoned
            || c.is_none_or(|c| u32_at(self.output, c.validity_offset + point * 4) != Ok(u32::MAX))
        {
            self.poisoned = true;
            return Err(SampleError::Output);
        }
        Ok(c.expect("checked descriptor"))
    }
    /// Write one finite row with allowed independent quality flags.
    pub fn valid(
        &mut self,
        point: usize,
        channel: usize,
        values: &[FiniteF64],
        quality_flags: u32,
    ) -> Result<(), SampleError> {
        let c = self.location(point, channel)?;
        if values.len() != c.components
            || allowed(self.request, self.context, channel)
                .is_none_or(|a| quality_flags == 0 || quality_flags & !a != 0)
        {
            self.poisoned = true;
            return Err(SampleError::Output);
        }
        let start = c.values_offset + point * c.point_stride;
        for (i, v) in values.iter().enumerate() {
            self.output[start + 8 * i..start + 8 * i + 8].copy_from_slice(&v.get().to_le_bytes());
        }
        self.output[c.validity_offset + point * 4..c.validity_offset + point * 4 + 4]
            .copy_from_slice(&0u32.to_le_bytes());
        self.output[c.quality_offset + point * 4..c.quality_offset + point * 4 + 4]
            .copy_from_slice(&quality_flags.to_le_bytes());
        Ok(())
    }
    /// Write invalidity without exposing/fabricating numeric values.
    pub fn invalid(
        &mut self,
        point: usize,
        channel: usize,
        reason: SampleInvalidity,
    ) -> Result<(), SampleError> {
        let c = self.location(point, channel)?;
        let available = allowed(self.request, self.context, channel).is_some();
        if available == (reason == SampleInvalidity::ChannelUnavailable) {
            self.poisoned = true;
            return Err(SampleError::Output);
        }
        self.output[c.validity_offset + point * 4..c.validity_offset + point * 4 + 4]
            .copy_from_slice(&(reason as u32).to_le_bytes());
        Ok(())
    }
    /// Return a candidate only after complete response validation. An earlier
    /// ignored write error poisons completion, even if later writes fill all cells.
    pub fn finish(self) -> Result<&'a [u8], SampleError> {
        if self.poisoned {
            return Err(SampleError::Output);
        }
        SampleResponse::read(self.output, self.request, self.context, self.limits)?;
        Ok(self.output)
    }
}
