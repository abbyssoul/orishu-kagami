//! Direct point-source Newtonian gravity. The opaque portable field state owns
//! source positions/masses; sampling never consults live entities or another guest.
#![deny(unsafe_code)]
mod channels;
wit_bindgen::generate!({path:"../../../crates/orishu-plugin/wit",world:"field"});
use exports::orishu::simulation::{common, field_kernel};
use orishu::simulation::{
    buffers::{Input, Output},
    types::{Admissibility, KernelError, StepContext},
};
use orishu_plugin::execution::{
    ComputePrecision, SAMPLE_REQUEST_SCHEMA, SAMPLE_RESPONSE_SCHEMA, SampleInvalidity,
    SampleLimits, SampleOutput, SampleRequest, SampleScratch,
};
use orishu_plugin::{
    ExecutionContractId, FiniteF64,
    execution::{
        Batch, BulkLimits, BulkRecord, CONFIGURATION_SCHEMA, ConfigurationValue, CoupledEntity,
        DOMAIN_SCHEMA, DomainDescriptor, DomainLimits, Force, INSTANCE_SCHEMA, InstanceContext,
        MAX_CONTEXT_BYTES, MAX_DOMAIN_BYTES, ResolvedConfiguration, SpatialDiscretization,
        VALIDATION_SCHEMA, ValidationInputs, encode_batch,
    },
};
use std::cell::{Cell, RefCell};

const CONFIG: &str = CONFIGURATION_SCHEMA;
const DOMAIN: &str = DOMAIN_SCHEMA;
const STATE: &str = "org.orishu.reference.newtonian.state/v1";
const QUERY: &str = SAMPLE_REQUEST_SCHEMA;
const SAMPLES: &str = SAMPLE_RESPONSE_SCHEMA;
const MAX_ENTITIES: usize = 4096;
const MAX_POINTS: usize = 4096;
const HEADER: usize = 80;
#[derive(Clone, Copy)]
struct Config {
    capacity: usize,
    g: f64,
    exclusion: f64,
}
thread_local! { static ACTIVE:Cell<Option<Config>>=const { Cell::new(None) }; }
thread_local! { static LOADED_DOMAIN:Cell<Option<([f64;3],[f64;3])>>=const { Cell::new(None) }; }
thread_local! { static CONTEXT:RefCell<Option<InstanceContext>>=const {RefCell::new(None)}; }
struct Newtonian;
fn config(session: u64) -> Result<Config, KernelError> {
    if session != 1 {
        return Err(KernelError::InvalidInput);
    }
    ACTIVE.get().ok_or(KernelError::InvalidInput)
}
fn limits(cfg: Config) -> BulkLimits {
    BulkLimits {
        bytes: 128 * 1024 * 1024,
        records: cfg.capacity,
    }
}
fn number(bytes: &[u8]) -> Result<f64, KernelError> {
    let bits = u64::from_le_bytes(bytes.try_into().map_err(|_| KernelError::InvalidInput)?);
    let value = f64::from_bits(bits);
    if !value.is_finite() || bits == 1 << 63 {
        return Err(KernelError::InvalidInput);
    }
    Ok(value)
}
fn checked(value: f64) -> Result<f64, KernelError> {
    if value.is_finite() {
        Ok(value + 0.0)
    } else {
        Err(KernelError::NumericalFailure)
    }
}
fn vector(bytes: &[u8]) -> Result<[f64; 3], KernelError> {
    if bytes.len() != 24 {
        return Err(KernelError::InvalidInput);
    }
    Ok([
        number(&bytes[..8])?,
        number(&bytes[8..16])?,
        number(&bytes[16..])?,
    ])
}
fn read(input: &Input, schema: &str, max: usize) -> Result<Vec<u8>, KernelError> {
    let d = input.describe();
    if d.schema != schema {
        return Err(KernelError::InvalidInput);
    }
    let len = usize::try_from(d.byte_length).map_err(|_| KernelError::LimitExceeded)?;
    if len > max {
        return Err(KernelError::LimitExceeded);
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|_| KernelError::LimitExceeded)?;
    while out.len() < len {
        let n = (len - out.len()).min(65536) as u32;
        let chunk = input
            .read_chunk(out.len() as u64, n)
            .map_err(|_| KernelError::InvalidInput)?;
        if chunk.len() != n as usize {
            return Err(KernelError::InvalidInput);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}
fn write(output: &Output, schema: &str, bytes: &[u8], values: u64) -> Result<(), KernelError> {
    let d = output.describe();
    if d.schema != schema
        || d.byte_length < bytes.len() as u64
        || d.value_count < values
        || (output.is_exact() && (d.byte_length != bytes.len() as u64 || d.value_count != values))
    {
        return Err(KernelError::InvalidInput);
    }
    let mut frame = [0; 65536];
    for (i, chunk) in bytes.chunks(65536).enumerate() {
        frame[..chunk.len()].copy_from_slice(chunk);
        output
            .write_chunk((i * 65536) as u64, chunk.len() as u32, frame)
            .map_err(|_| KernelError::InvalidInput)?;
    }
    output
        .finish(bytes.len() as u64, values)
        .map_err(|_| KernelError::InvalidInput)
}
fn dt(value: f64) -> Result<(), KernelError> {
    if !value.is_finite() || value <= 0.0 {
        Err(KernelError::InadmissibleTimestep)
    } else {
        Ok(())
    }
}

fn separation(position: [f64; 3], source: [f64; 3]) -> Result<([f64; 3], f64), u32> {
    let d = [
        position[0] - source[0],
        position[1] - source[1],
        position[2] - source[2],
    ];
    if d.iter().any(|x| !x.is_finite()) {
        return Err(3);
    }
    let scale = d.iter().fold(0.0_f64, |a, x| a.max(x.abs()));
    if scale == 0.0 {
        return Err(2);
    }
    let distance =
        scale * ((d[0] / scale).powi(2) + (d[1] / scale).powi(2) + (d[2] / scale).powi(2)).sqrt();
    if !distance.is_finite() {
        return Err(3);
    }
    Ok((d, distance))
}
fn domain(bytes: &[u8]) -> Result<([f64; 3], [f64; 3]), KernelError> {
    if bytes.len() != 48 {
        return Err(KernelError::InvalidInput);
    }
    let low = vector(&bytes[..24])?;
    let high = vector(&bytes[24..])?;
    if (0..3).any(|i| low[i] >= high[i]) {
        return Err(KernelError::InvalidInput);
    }
    Ok((low, high))
}
fn within(position: [f64; 3], low: [f64; 3], high: [f64; 3]) -> bool {
    (0..3).all(|i| position[i] >= low[i] && position[i] <= high[i])
}
fn domain_descriptor(low: [f64; 3], high: [f64; 3]) -> Result<DomainDescriptor, KernelError> {
    let coordinates = |values: [f64; 3]| -> Result<[FiniteF64; 3], KernelError> {
        let mut out = [FiniteF64::ZERO; 3];
        for (to, from) in out.iter_mut().zip(values) {
            *to = FiniteF64::new(from).map_err(|_| KernelError::InvalidInput)?;
        }
        Ok(out)
    };
    Ok(DomainDescriptor {
        api_version: DOMAIN.parse().map_err(|_| KernelError::InvalidInput)?,
        lower_metres: coordinates(low)?,
        upper_metres: coordinates(high)?,
        discretization: SpatialDiscretization::Continuous,
    })
}
fn coupled(bytes: &[u8], count: u64, cfg: Config) -> Result<Batch<'_, CoupledEntity>, KernelError> {
    let entities =
        Batch::<CoupledEntity>::read(bytes, limits(cfg)).map_err(|_| KernelError::InvalidInput)?;
    if entities.len() as u64 != count {
        return Err(KernelError::InvalidInput);
    }
    for e in entities.iter() {
        // This model declares one mass coupling slot. Other models may declare more.
        if e.slot.0 != 0 {
            return Err(KernelError::Unsupported);
        }
        if [e.source_si, e.response_si]
            .into_iter()
            .flatten()
            .any(|m| m.get() < 0.0)
        {
            return Err(KernelError::InvalidInput);
        }
    }
    Ok(entities)
}
struct Field<'a> {
    bytes: &'a [u8],
    count: usize,
    low: [f64; 3],
    high: [f64; 3],
}
impl<'a> Field<'a> {
    fn parse(bytes: &'a [u8], cfg: Config) -> Result<Self, KernelError> {
        if bytes.len() != HEADER + 40 * cfg.capacity || bytes.get(..4) != Some(b"OGF1") {
            return Err(KernelError::IncompatibleState);
        }
        if u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| KernelError::IncompatibleState)?,
        ) as usize
            != cfg.capacity
            || number(&bytes[8..16])? != cfg.g
            || number(&bytes[16..24])? != cfg.exclusion
        {
            return Err(KernelError::IncompatibleState);
        }
        let (low, high) = domain(&bytes[24..72])?;
        let count = u32::from_le_bytes(
            bytes[72..76]
                .try_into()
                .map_err(|_| KernelError::IncompatibleState)?,
        ) as usize;
        if count > cfg.capacity
            || bytes[76..80] != [0; 4]
            || bytes[HEADER + 40 * count..].iter().any(|b| *b != 0)
        {
            return Err(KernelError::IncompatibleState);
        }
        let mut previous = None;
        for entry in bytes[HEADER..HEADER + 40 * count].chunks_exact(40) {
            let id = u64::from_le_bytes(
                entry[..8]
                    .try_into()
                    .map_err(|_| KernelError::IncompatibleState)?,
            );
            if previous.is_some_and(|old| old >= id) || number(&entry[32..40])? < 0.0 {
                return Err(KernelError::IncompatibleState);
            }
            vector(&entry[8..32])?;
            previous = Some(id);
        }
        Ok(Self {
            bytes,
            count,
            low,
            high,
        })
    }
    fn sources(&self) -> impl Iterator<Item = (u64, [f64; 3], f64)> + '_ {
        self.bytes[HEADER..HEADER + 40 * self.count]
            .chunks_exact(40)
            .map(|e| {
                (
                    u64::from_le_bytes(e[..8].try_into().expect("validated source")),
                    vector(&e[8..32]).expect("validated position"),
                    number(&e[32..40]).expect("validated mass"),
                )
            })
    }
}
fn read_state(input: &Input, cfg: Config) -> Result<Vec<u8>, KernelError> {
    if input.describe().value_count != 1 {
        return Err(KernelError::IncompatibleState);
    }
    let bytes = read(input, STATE, HEADER + 40 * cfg.capacity)?;
    let field = Field::parse(&bytes, cfg)?;
    // The field's opaque state repeats its domain. Bind it to the captured setup
    // domain on every load, restore and sample, not just at natural initialization.
    let domain = domain_descriptor(field.low, field.high)?
        .to_cbor(DomainLimits::default())
        .map_err(|_| KernelError::IncompatibleState)?;
    CONTEXT.with_borrow(|ctx| {
        let ctx = ctx.as_ref().ok_or(KernelError::InvalidInput)?;
        if !ctx.domain.matches(DOMAIN, 1, &domain) {
            return Err(KernelError::IncompatibleState);
        }
        Ok(())
    })?;
    Ok(bytes)
}

// Result is acceleration, potential and row-major d(acceleration_i)/d(position_j).
// Codes: 1 outside domain, 2 singular/excluded point, 3 numerical overflow.
fn evaluate(
    field: &Field<'_>,
    cfg: Config,
    position: [f64; 3],
    exclude: Option<u64>,
    sample_channels: bool,
) -> Result<([f64; 13], [u32; 3]), u32> {
    if !within(position, field.low, field.high) {
        return Err(1);
    }
    let mut result = [0.0; 13];
    let mut validity = [0; 3];
    for (id, source, mass) in field.sources() {
        if exclude == Some(id) || mass == 0.0 {
            continue;
        }
        let (d, distance) = separation(position, source)?;
        if distance < cfg.exclusion {
            return Err(2);
        }
        let inverse = 1.0 / distance;
        let mu = cfg.g * mass;
        let a = mu * inverse * inverse;
        let unit = [d[0] * inverse, d[1] * inverse, d[2] * inverse];
        if !mu.is_finite() || !inverse.is_finite() {
            return Err(3);
        }
        if validity[0] == 0 {
            for axis in 0..3 {
                result[axis] -= a * unit[axis];
            }
            if result[..3].iter().any(|v| !v.is_finite()) {
                validity[0] = 3;
                result[..3].fill(0.0);
            }
        }
        if sample_channels {
            if validity[1] == 0 {
                result[3] -= mu * inverse;
                if !result[3].is_finite() {
                    validity[1] = 3;
                    result[3] = 0.0;
                }
            }
            if validity[2] == 0 {
                let j = a * inverse;
                for i in 0..3 {
                    for k in 0..3 {
                        result[4 + i * 3 + k] -= j * (f64::from(i == k) - 3.0 * unit[i] * unit[k]);
                    }
                }
                if result[4..].iter().any(|v| !v.is_finite()) {
                    validity[2] = 3;
                    result[4..].fill(0.0);
                }
            }
        }
    }
    Ok((result, validity))
}
impl common::Guest for Newtonian {
    fn setup(context: &Input, input: &Input) -> Result<u64, KernelError> {
        if ACTIVE.get().is_some() {
            return Err(KernelError::InvalidInput);
        }
        let metadata =
            InstanceContext::from_cbor(&read(context, INSTANCE_SCHEMA, MAX_CONTEXT_BYTES)?)
                .map_err(|_| KernelError::InvalidInput)?;
        if context.describe().value_count != 1
            || metadata.compute_precision != ComputePrecision::Binary64
            || metadata.observables != channels::bindings()
            || metadata.execution_contract != ExecutionContractId::Field
            || metadata.state_format.id.as_str() != "org.orishu.reference.newtonian.state"
            || metadata.state_format.version.get() != 1
            || metadata.couplings.len() != 1
            || metadata.couplings.iter().any(|slot| {
                [&slot.source, &slot.response]
                    .into_iter()
                    .flatten()
                    .any(|role| role.dimension != orishu_plugin::Dimension::MASS)
            })
        {
            return Err(KernelError::Unsupported);
        }
        let bytes = read(
            input,
            CONFIG,
            orishu_plugin::Limits::default().max_payload_bytes,
        )?;
        if input.describe().value_count != 1 || !metadata.configuration.matches(CONFIG, 1, &bytes) {
            return Err(KernelError::InvalidInput);
        }
        let values = ResolvedConfiguration::from_cbor(&bytes, &orishu_plugin::Limits::default())
            .map_err(|_| KernelError::InvalidInput)?;
        if values.properties.len() != 4 {
            return Err(KernelError::InvalidInput);
        }
        let quantity = |id: &str, expected: orishu_plugin::Dimension| -> Result<f64, KernelError> {
            match values.get(id) {
                Some(ConfigurationValue::Quantity {
                    value_si,
                    dimension,
                }) if *dimension == expected => Ok(value_si.get()),
                _ => Err(KernelError::InvalidInput),
            }
        };
        let capacity = quantity("capacity", orishu_plugin::Dimension::DIMENSIONLESS)?;
        if capacity < 1.0 || capacity > MAX_ENTITIES as f64 || capacity.fract() != 0.0 {
            return Err(KernelError::InvalidInput);
        }
        if !matches!(values.get("boundary"),Some(ConfigurationValue::Text {value}) if value=="isolated")
        {
            return Err(KernelError::Unsupported);
        }
        let capacity = capacity as usize;
        let cfg = Config {
            capacity,
            g: quantity(
                "gravitational-constant",
                orishu_plugin::Dimension::new([3, -1, -2, 0, 0, 0, 0]),
            )?,
            exclusion: quantity("exclusion-radius", orishu_plugin::Dimension::LENGTH)?,
        };
        if capacity == 0 || capacity > MAX_ENTITIES || cfg.g <= 0.0 || cfg.exclusion < 0.0 {
            return Err(KernelError::InvalidInput);
        }
        if capacity > metadata.bounds.projection_records as usize
            || (HEADER + 40 * capacity) as u64 > metadata.bounds.state_bytes
        {
            return Err(KernelError::LimitExceeded);
        }
        CONTEXT.with_borrow_mut(|ctx| *ctx = Some(metadata));
        ACTIVE.set(Some(cfg));
        Ok(1)
    }
    fn load(session: u64, state: &Input) -> Result<(), KernelError> {
        let cfg = config(session)?;
        let bytes = read_state(state, cfg)?;
        let field = Field::parse(&bytes, cfg)?;
        LOADED_DOMAIN.set(Some((field.low, field.high)));
        Ok(())
    }
    fn validate(session: u64, input: &Input) -> Admissibility {
        let check = || -> Result<(), KernelError> {
            let cfg = config(session)?;
            let bytes = read(input, VALIDATION_SCHEMA, limits(cfg).bytes)?;
            if input.describe().value_count != 1 {
                return Err(KernelError::InvalidInput);
            }
            let offset = CONTEXT.with_borrow(|ctx| {
                let ctx = ctx.as_ref().ok_or(KernelError::InvalidInput)?;
                let inputs = ValidationInputs::read(&bytes, ctx, limits(cfg))
                    .map_err(|_| KernelError::InvalidInput)?;
                // The projection is the final section. No copy of entity rows.
                Ok::<_, KernelError>(bytes.len() - inputs.entities.len())
            })?;
            let batch = Batch::<CoupledEntity>::read(&bytes[offset..], limits(cfg))
                .map_err(|_| KernelError::InvalidInput)?;
            let entities = coupled(batch.bytes(), batch.len() as u64, cfg)?;
            let (low, high) = LOADED_DOMAIN.get().ok_or(KernelError::IncompatibleState)?;
            for target in entities.iter().filter(|e| {
                e.responds_dynamically() && e.response_si.is_some_and(|m| m.get() > 0.0)
            }) {
                let position = target.kinematics.position_metres.map(|v| v.get());
                if !within(position, low, high) {
                    return Err(KernelError::InvalidInput);
                }
                for source in entities
                    .iter()
                    .filter(|e| e.id != target.id && e.source_si.is_some_and(|m| m.get() > 0.0))
                {
                    let (_, distance) =
                        separation(position, source.kinematics.position_metres.map(|v| v.get()))
                            .map_err(|_| KernelError::NumericalFailure)?;
                    if distance < cfg.exclusion {
                        return Err(KernelError::NumericalFailure);
                    }
                }
            }
            Ok(())
        };
        match check() {
            Ok(()) => Admissibility::Admissible(None),
            Err(error) => Admissibility::Rejected(error),
        }
    }
    fn checkpoint(session: u64, state: &Input, output: &Output) -> Result<(), KernelError> {
        let bytes = read_state(state, config(session)?)?;
        write(output, STATE, &bytes, 1)
    }
    fn restore(session: u64, state: &Input) -> Result<(), KernelError> {
        Self::load(session, state)
    }
    fn close(session: u64) -> Result<(), KernelError> {
        config(session)?;
        ACTIVE.set(None);
        LOADED_DOMAIN.set(None);
        CONTEXT.with_borrow_mut(|ctx| *ctx = None);
        Ok(())
    }
}
impl field_kernel::Guest for Newtonian {
    fn initialize(session: u64, input: &Input, output: &Output) -> Result<(), KernelError> {
        let cfg = config(session)?;
        let bytes = read(input, DOMAIN, MAX_DOMAIN_BYTES)?;
        if input.describe().value_count != 1 {
            return Err(KernelError::InvalidInput);
        }
        CONTEXT.with_borrow(|ctx| {
            let ctx = ctx.as_ref().ok_or(KernelError::InvalidInput)?;
            if !ctx.domain.matches(DOMAIN, 1, &bytes) {
                return Err(KernelError::InvalidInput);
            }
            Ok(())
        })?;
        let domain = DomainDescriptor::from_cbor(&bytes, DomainLimits::default())
            .map_err(|_| KernelError::InvalidInput)?;
        if domain.discretization != SpatialDiscretization::Continuous {
            return Err(KernelError::Unsupported);
        }
        let mut state = vec![0; HEADER + 40 * cfg.capacity];
        state[..4].copy_from_slice(b"OGF1");
        state[4..8].copy_from_slice(&(cfg.capacity as u32).to_le_bytes());
        state[8..16].copy_from_slice(&cfg.g.to_le_bytes());
        state[16..24].copy_from_slice(&cfg.exclusion.to_le_bytes());
        for (i, value) in domain
            .lower_metres
            .iter()
            .chain(&domain.upper_metres)
            .enumerate()
        {
            state[24 + 8 * i..32 + 8 * i].copy_from_slice(&value.get().to_le_bytes());
        }
        write(output, STATE, &state, 1)
    }
    fn advance(
        session: u64,
        context: StepContext,
        prior: &Input,
        input: &Input,
        output: &Output,
        force_output: &Output,
    ) -> Result<(), KernelError> {
        let cfg = config(session)?;
        dt(context.dt_seconds)?;
        let mut state = read_state(prior, cfg)?;
        let bytes = read(input, CoupledEntity::SCHEMA, 12 + 80 * cfg.capacity)?;
        let entities = coupled(&bytes, input.describe().value_count, cfg)?;
        state[HEADER..].fill(0);
        let mut count = 0;
        for entity in entities.iter() {
            if let Some(mass) = entity.source_si {
                let start = HEADER + 40 * count;
                state[start..start + 8].copy_from_slice(&entity.id.0.to_le_bytes());
                for axis in 0..3 {
                    state[start + 8 + 8 * axis..start + 16 + 8 * axis].copy_from_slice(
                        &entity.kinematics.position_metres[axis].get().to_le_bytes(),
                    );
                }
                state[start + 32..start + 40].copy_from_slice(&mass.get().to_le_bytes());
                count += 1;
            }
        }
        state[72..76].copy_from_slice(&(count as u32).to_le_bytes());
        let field = Field::parse(&state, cfg)?;
        let mut forces = Vec::new();
        forces
            .try_reserve_exact(entities.len())
            .map_err(|_| KernelError::LimitExceeded)?;
        for entity in entities.iter().filter(CoupledEntity::responds_dynamically) {
            let response = entity.response_si.ok_or(KernelError::InvalidInput)?.get();
            let (sample, validity) = if response == 0.0 {
                ([0.0; 13], [0; 3])
            } else {
                evaluate(
                    &field,
                    cfg,
                    entity.kinematics.position_metres.map(|v| v.get()),
                    Some(entity.id.0),
                    false,
                )
                .map_err(|_| KernelError::NumericalFailure)?
            };
            if validity[0] != 0 {
                return Err(KernelError::NumericalFailure);
            }
            let mut newtons = [FiniteF64::ZERO; 3];
            for axis in 0..3 {
                newtons[axis] = FiniteF64::new(checked(sample[axis] * response)?)
                    .map_err(|_| KernelError::NumericalFailure)?;
            }
            forces.push(Force {
                id: entity.id,
                newtons,
            });
        }
        let mut encoded = Vec::new();
        encode_batch(&forces, &mut encoded, limits(cfg)).map_err(|_| KernelError::InvalidInput)?;
        write(output, STATE, &state, 1)?;
        write(force_output, Force::SCHEMA, &encoded, forces.len() as u64)
    }
    fn sample(
        session: u64,
        query: &Input,
        snapshot: &Input,
        output: &Output,
    ) -> Result<(), KernelError> {
        let cfg = config(session)?;
        let bytes = read_state(snapshot, cfg)?;
        let field = Field::parse(&bytes, cfg)?;
        let query_bytes = read(
            query,
            QUERY,
            12 + orishu_plugin::execution::MAX_SAMPLE_METADATA_BYTES + 32 * MAX_POINTS,
        )?;
        CONTEXT.with_borrow(|ctx| {
            let ctx = ctx.as_ref().ok_or(KernelError::InvalidInput)?;
            let limits = SampleLimits {
                points: MAX_POINTS.min(ctx.bounds.sample_points as usize),
                channels: ctx.bounds.sample_channels as usize,
                ..SampleLimits::default()
            };
            let request = SampleRequest::read(&query_bytes, &mut SampleScratch::default(), limits)
                .map_err(|_| KernelError::InvalidInput)?;
            if query.describe().value_count != request.len() as u64
                || !request.metadata().snapshot.state.matches(STATE, 1, &bytes)
            {
                return Err(KernelError::InvalidInput);
            }
            let selected: Vec<_> = request
                .metadata()
                .channels
                .iter()
                .map(|channel| {
                    ctx.observables
                        .iter()
                        .find(|b| b.channel.contract == channel.contract)
                        .and_then(|b| match b.slot.as_str() {
                            "acceleration" => Some((0, 0, 3)),
                            "potential" => Some((1, 3, 1)),
                            "jacobian" => Some((2, 4, 9)),
                            _ => None,
                        })
                })
                .collect();
            let mut result = Vec::new();
            let mut writer = SampleOutput::new(&mut result, &request, ctx, limits)
                .map_err(|_| KernelError::InvalidInput)?;
            for (i, point) in request.points().enumerate() {
                let (values, validity) = if selected.iter().any(Option::is_some) {
                    evaluate(
                        &field,
                        cfg,
                        point.position_metres.map(|v| v.get()),
                        None,
                        true,
                    )
                    .unwrap_or_else(|reason| ([0.0; 13], [reason; 3]))
                } else {
                    ([0.0; 13], [0; 3])
                };
                let mut finite = [FiniteF64::ZERO; 13];
                for (to, from) in finite.iter_mut().zip(values) {
                    *to = FiniteF64::new(from).map_err(|_| KernelError::NumericalFailure)?;
                }
                for (c, selection) in selected.iter().enumerate() {
                    let result = match selection {
                        None => writer.invalid(i, c, SampleInvalidity::ChannelUnavailable),
                        Some((v, start, len)) => match validity[*v] {
                            0 => writer.valid(i, c, &finite[*start..start + len], 1),
                            1 => writer.invalid(i, c, SampleInvalidity::OutsideDomain),
                            2 => writer.invalid(i, c, SampleInvalidity::Singular),
                            _ => writer.invalid(i, c, SampleInvalidity::Undefined),
                        },
                    };
                    result.map_err(|_| KernelError::InvalidInput)?;
                }
            }
            let result = writer.finish().map_err(|_| KernelError::InvalidInput)?;
            write(output, SAMPLES, result, request.len() as u64)
        })
    }
}
#[allow(unsafe_code)]
mod abi_exports {
    use super::Newtonian;
    super::export!(Newtonian with_types_in super);
}
