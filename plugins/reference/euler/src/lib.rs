//! Classical kick-then-drift symplectic Euler, independently compiled as a
//! Dynamics Component. One force evaluation at committed kinematics per step.
//! No gravity equation, field lookup, UI, host runtime or relativistic claim.
#![deny(unsafe_code)]
wit_bindgen::generate!({path:"../../../crates/orishu-plugin/wit",world:"dynamics"});

use exports::orishu::simulation::{common, dynamics_kernel};
use orishu::simulation::{
    buffers::{Input, Output},
    types::{Admissibility, KernelError, StepContext},
};
use orishu_plugin::{
    ExecutionContractId, FiniteF64,
    execution::{
        Batch, BulkLimits, BulkRecord, CONFIGURATION_SCHEMA, ConfigurationValue, DynamicEntity,
        EntityId, Force, INSTANCE_SCHEMA, InstanceContext, MAX_CONTEXT_BYTES,
        ResolvedConfiguration, VALIDATION_SCHEMA, ValidationInputs, encode_batch,
    },
};
use std::cell::RefCell;
thread_local! {static CONTEXT:RefCell<Option<InstanceContext>>=const {RefCell::new(None)};}

const CONFIG: &str = CONFIGURATION_SCHEMA;
const HISTORY: &str = "org.orishu.reference.euler.history/v1";
const MAX_ENTITIES: usize = 1_000_000;
struct Euler;

fn bound(session: u64) -> Result<usize, KernelError> {
    let limit = usize::try_from(session).map_err(|_| KernelError::LimitExceeded)?;
    if limit == 0 || limit > MAX_ENTITIES {
        return Err(KernelError::LimitExceeded);
    }
    Ok(limit)
}
fn limits(session: u64) -> Result<BulkLimits, KernelError> {
    Ok(BulkLimits {
        records: bound(session)?,
        bytes: 128 * 1024 * 1024,
    })
}
fn read(input: &Input, schema: &str, max: usize) -> Result<Vec<u8>, KernelError> {
    let d = input.describe();
    if d.schema != schema {
        return Err(KernelError::InvalidInput);
    }
    let length = usize::try_from(d.byte_length).map_err(|_| KernelError::LimitExceeded)?;
    if length > max {
        return Err(KernelError::LimitExceeded);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| KernelError::LimitExceeded)?;
    while bytes.len() < length {
        let n = (length - bytes.len()).min(65536) as u32;
        let chunk = input
            .read_chunk(bytes.len() as u64, n)
            .map_err(|_| KernelError::InvalidInput)?;
        if chunk.len() != n as usize {
            return Err(KernelError::InvalidInput);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn write(output: &Output, schema: &str, bytes: &[u8], count: u64) -> Result<(), KernelError> {
    let d = output.describe();
    if d.schema != schema
        || d.byte_length < bytes.len() as u64
        || d.value_count < count
        || (output.is_exact() && (d.byte_length != bytes.len() as u64 || d.value_count != count))
    {
        return Err(KernelError::InvalidInput);
    }
    let mut frame = [0; 65536];
    for (index, chunk) in bytes.chunks(65536).enumerate() {
        frame[..chunk.len()].copy_from_slice(chunk);
        output
            .write_chunk((index * 65536) as u64, chunk.len() as u32, frame)
            .map_err(|_| KernelError::InvalidInput)?;
    }
    output
        .finish(bytes.len() as u64, count)
        .map_err(|_| KernelError::InvalidInput)
}
fn packet<'a, R: BulkRecord>(
    bytes: &'a [u8],
    input: &Input,
    session: u64,
) -> Result<Batch<'a, R>, KernelError> {
    let batch = Batch::read(bytes, limits(session)?).map_err(|_| KernelError::InvalidInput)?;
    if input.describe().value_count != batch.len() as u64 {
        return Err(KernelError::InvalidInput);
    }
    Ok(batch)
}
// History deliberately contains membership but no numerical samples: this
// one-stage scheme has no prior-force requirement. Magic prevents treating a
// force/state packet as compatible history just because it names the same IDs.
fn history(bytes: &[u8], count: u64, session: u64) -> Result<Batch<'_, EntityId>, KernelError> {
    if bytes.get(..4) != Some(b"OEH1") {
        return Err(KernelError::IncompatibleState);
    }
    let batch =
        Batch::read(&bytes[4..], limits(session)?).map_err(|_| KernelError::IncompatibleState)?;
    if batch.len() as u64 != count {
        return Err(KernelError::IncompatibleState);
    }
    Ok(batch)
}
fn read_history(input: &Input, session: u64) -> Result<Vec<u8>, KernelError> {
    let bytes = read(input, HISTORY, 16 + bound(session)? * 8)?;
    history(&bytes, input.describe().value_count, session)?;
    Ok(bytes)
}
fn write_history(output: &Output, ids: &[EntityId], session: u64) -> Result<(), KernelError> {
    let mut encoded = Vec::new();
    encode_batch(ids, &mut encoded, limits(session)?).map_err(|_| KernelError::InvalidInput)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(encoded.len() + 4)
        .map_err(|_| KernelError::LimitExceeded)?;
    bytes.extend_from_slice(b"OEH1");
    bytes.extend_from_slice(&encoded);
    write(output, HISTORY, &bytes, ids.len() as u64)
}
fn dt(value: f64) -> Result<f64, KernelError> {
    if !value.is_finite() || value <= 0.0 {
        Err(KernelError::InadmissibleTimestep)
    } else {
        Ok(value)
    }
}
fn finite(value: f64) -> Result<FiniteF64, KernelError> {
    FiniteF64::new(value).map_err(|_| KernelError::NumericalFailure)
}

impl common::Guest for Euler {
    fn setup(context: &Input, config: &Input) -> Result<u64, KernelError> {
        let metadata =
            InstanceContext::from_cbor(&read(context, INSTANCE_SCHEMA, MAX_CONTEXT_BYTES)?)
                .map_err(|_| KernelError::InvalidInput)?;
        if context.describe().value_count != 1
            || metadata.compute_precision != orishu_plugin::execution::ComputePrecision::Binary64
            || metadata.execution_contract != ExecutionContractId::Dynamics
            || metadata.state_format.id.as_str() != "org.orishu.reference.euler.history"
            || metadata.state_format.version.get() != 1
        {
            return Err(KernelError::Unsupported);
        }
        let count = config.describe().value_count;
        let config = read(
            config,
            CONFIG,
            orishu_plugin::Limits::default().max_payload_bytes,
        )?;
        if !metadata.configuration.matches(CONFIG, count, &config) || count != 1 {
            return Err(KernelError::InvalidInput);
        }
        let config = ResolvedConfiguration::from_cbor(&config, &orishu_plugin::Limits::default())
            .map_err(|_| KernelError::InvalidInput)?;
        if config.properties.len() != 1 {
            return Err(KernelError::InvalidInput);
        }
        let Some(ConfigurationValue::Quantity {
            value_si,
            dimension,
        }) = config.get("capacity")
        else {
            return Err(KernelError::InvalidInput);
        };
        let value = value_si.get();
        if !dimension.is_dimensionless()
            || value.fract() != 0.0
            || value < 1.0
            || value > MAX_ENTITIES as f64
        {
            return Err(KernelError::InvalidInput);
        }
        let max = value as u64;
        bound(max)?;
        if max > u64::from(metadata.bounds.projection_records)
            || 16 + 8 * max > metadata.bounds.state_bytes
        {
            return Err(KernelError::LimitExceeded);
        }
        CONTEXT.with_borrow_mut(|ctx| {
            if ctx.is_some() {
                return Err(KernelError::InvalidInput);
            }
            *ctx = Some(metadata);
            Ok(())
        })?;
        Ok(max)
    }
    fn load(session: u64, state: &Input) -> Result<(), KernelError> {
        read_history(state, session)?;
        Ok(())
    }
    fn validate(session: u64, inputs: &Input) -> Admissibility {
        let check = || -> Result<(), KernelError> {
            let bytes = read(inputs, VALIDATION_SCHEMA, limits(session)?.bytes)?;
            if inputs.describe().value_count != 1 {
                return Err(KernelError::InvalidInput);
            }
            CONTEXT.with_borrow(|ctx| {
                let ctx = ctx.as_ref().ok_or(KernelError::InvalidInput)?;
                ValidationInputs::read(&bytes, ctx, limits(session)?)
                    .map_err(|_| KernelError::InvalidInput)?;
                Ok(())
            })?;
            Ok(())
        };
        match check() {
            Ok(()) => Admissibility::Admissible(None),
            Err(error) => Admissibility::Rejected(error),
        }
    }
    fn checkpoint(session: u64, state: &Input, output: &Output) -> Result<(), KernelError> {
        let bytes = read_history(state, session)?;
        write(output, HISTORY, &bytes, state.describe().value_count)
    }
    fn restore(session: u64, state: &Input) -> Result<(), KernelError> {
        Self::load(session, state)
    }
    fn close(session: u64) -> Result<(), KernelError> {
        bound(session)?;
        CONTEXT.with_borrow_mut(|ctx| *ctx = None);
        Ok(())
    }
}
impl dynamics_kernel::Guest for Euler {
    fn initialize_history(
        session: u64,
        entities: &Input,
        output: &Output,
    ) -> Result<(), KernelError> {
        let bytes = read(
            entities,
            DynamicEntity::SCHEMA,
            12 + bound(session)? * DynamicEntity::BYTES,
        )?;
        let entities = packet::<DynamicEntity>(&bytes, entities, session)?;
        let ids: Vec<_> = entities.iter().map(|e| e.id).collect();
        write_history(output, &ids, session)
    }
    fn transition_entities(
        session: u64,
        _context: StepContext,
        births: &Input,
        deaths: &Input,
        prior: &Input,
        output: &Output,
    ) -> Result<(), KernelError> {
        let b = read(
            births,
            DynamicEntity::SCHEMA,
            12 + bound(session)? * DynamicEntity::BYTES,
        )?;
        let births = packet::<DynamicEntity>(&b, births, session)?;
        let d = read(deaths, EntityId::SCHEMA, 12 + bound(session)? * 8)?;
        let deaths = packet::<EntityId>(&d, deaths, session)?;
        let h = read_history(prior, session)?;
        let prior = history(&h, prior.describe().value_count, session)?;
        let count = prior
            .len()
            .checked_add(births.len())
            .and_then(|n| n.checked_sub(deaths.len()))
            .filter(|n| *n <= bound(session).unwrap_or(0))
            .ok_or(KernelError::LimitExceeded)?;
        // Reject overlap without a per-entity linear search.
        let mut dying = deaths.iter().peekable();
        for born in births.iter() {
            while dying.peek().is_some_and(|id| *id < born.id) {
                dying.next();
            }
            if dying.peek() == Some(&born.id) {
                return Err(KernelError::InvalidInput);
            }
        }
        let mut dying = deaths.iter().peekable();
        let mut born = births.iter().map(|e| e.id).peekable();
        // Establish exact membership before allocating the result; invalid death
        // claims must not make an undersized reservation grow during the merge.
        for old in prior.iter() {
            while born.peek().is_some_and(|id| *id < old) {
                born.next();
            }
            if born.peek() == Some(&old) {
                return Err(KernelError::InvalidInput);
            }
            if dying.peek().is_some_and(|id| *id < old) {
                return Err(KernelError::InvalidInput);
            }
            if dying.peek() == Some(&old) {
                dying.next();
            }
        }
        if dying.next().is_some() {
            return Err(KernelError::InvalidInput);
        }
        let mut dying = deaths.iter().peekable();
        let mut born = births.iter().map(|e| e.id).peekable();
        let mut next = Vec::new();
        next.try_reserve_exact(count)
            .map_err(|_| KernelError::LimitExceeded)?;
        for old in prior.iter() {
            if dying.peek().is_some_and(|id| *id < old) {
                return Err(KernelError::InvalidInput);
            }
            if dying.peek() == Some(&old) {
                dying.next();
                continue;
            }
            while born.peek().is_some_and(|id| *id < old) {
                next.push(born.next().ok_or(KernelError::InvalidInput)?);
            }
            if born.peek() == Some(&old) {
                return Err(KernelError::InvalidInput);
            }
            next.push(old);
        }
        if dying.next().is_some() {
            return Err(KernelError::InvalidInput);
        }
        next.extend(born);
        if next.len() != count {
            return Err(KernelError::InvalidInput);
        }
        write_history(output, &next, session)
    }
    fn integrate(
        session: u64,
        context: StepContext,
        entities: &Input,
        forces: &Input,
        prior: &Input,
        next: &Output,
        history_output: &Output,
    ) -> Result<(), KernelError> {
        let dt = dt(context.dt_seconds)?;
        let e = read(
            entities,
            DynamicEntity::SCHEMA,
            12 + bound(session)? * DynamicEntity::BYTES,
        )?;
        let entities = packet::<DynamicEntity>(&e, entities, session)?;
        let f = read(forces, Force::SCHEMA, 12 + bound(session)? * Force::BYTES)?;
        let forces = packet::<Force>(&f, forces, session)?;
        let h = read_history(prior, session)?;
        let old = history(&h, prior.describe().value_count, session)?;
        if entities.len() != forces.len() || entities.len() != old.len() {
            return Err(KernelError::IncompatibleState);
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(entities.len())
            .map_err(|_| KernelError::LimitExceeded)?;
        for ((mut entity, force), id) in entities.iter().zip(forces.iter()).zip(old.iter()) {
            if entity.id != force.id || entity.id != id {
                return Err(KernelError::IncompatibleState);
            }
            let mass = entity.inertial_mass_kilograms.get();
            for axis in 0..3 {
                let velocity = finite(
                    entity.kinematics.velocity_metres_per_second[axis].get()
                        + force.newtons[axis].get() / mass * dt,
                )?;
                let position =
                    finite(entity.kinematics.position_metres[axis].get() + velocity.get() * dt)?;
                entity.kinematics.velocity_metres_per_second[axis] = velocity;
                entity.kinematics.position_metres[axis] = position;
            }
            result.push(entity);
        }
        let mut encoded = Vec::new();
        encode_batch(&result, &mut encoded, limits(session)?)
            .map_err(|_| KernelError::InvalidInput)?;
        write(next, DynamicEntity::SCHEMA, &encoded, result.len() as u64)?;
        write(history_output, HISTORY, &h, old.len() as u64)
    }
}
// Only generated canonical-ABI trampolines require raw memory/export machinery.
// The numerical and lifecycle implementation above remains safe Rust.
#[allow(unsafe_code)]
mod abi_exports {
    use super::Euler;
    super::export!(Euler with_types_in super);
}
