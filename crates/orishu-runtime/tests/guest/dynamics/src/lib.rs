//! History/membership ABI fixture, NOT a physical integrator. Entities are sorted
//! nonzero byte IDs; history is sorted (ID, counter) byte pairs. Counters advance
//! per integration to make lost/implicitly reinitialized history observable.
wit_bindgen::generate!({path:"../../../../orishu-plugin/wit",world:"dynamics"});
use core::sync::atomic::{AtomicBool, Ordering};
use exports::orishu::simulation::{common, dynamics_kernel};
use orishu::simulation::{
    buffers::{Input, Output},
    types::{Admissibility, KernelError, StepContext, TimestepAdvice},
};

static LOADED: AtomicBool = AtomicBool::new(false);
struct Fixture;
fn read(input: &Input) -> Result<Vec<u8>, KernelError> {
    let d = input.describe();
    if d.byte_length > 1024 {
        return Err(KernelError::LimitExceeded);
    }
    input
        .read_chunk(0, d.byte_length as u32)
        .map_err(|_| KernelError::InvalidInput)
}
fn ids(input: &Input) -> Result<Vec<u8>, KernelError> {
    let d = input.describe();
    let bytes = read(input)?;
    if d.value_count != bytes.len() as u64
        || bytes.first() == Some(&0)
        || !bytes.windows(2).all(|p| p[0] < p[1])
    {
        return Err(KernelError::InvalidInput);
    }
    Ok(bytes)
}
fn history(input: &Input) -> Result<Vec<[u8; 2]>, KernelError> {
    let d = input.describe();
    let bytes = read(input)?;
    if d.schema != "fixture.history/v1"
        || bytes.len() % 2 != 0
        || d.value_count != bytes.len() as u64 / 2
    {
        return Err(KernelError::IncompatibleState);
    }
    let pairs: Vec<[u8; 2]> = bytes.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
    if pairs.first().is_some_and(|p| p[0] == 0) || !pairs.windows(2).all(|p| p[0][0] < p[1][0]) {
        return Err(KernelError::IncompatibleState);
    }
    Ok(pairs)
}
fn write(output: &Output, bytes: &[u8], values: u64) -> Result<(), KernelError> {
    if bytes.len() > 1024 {
        return Err(KernelError::LimitExceeded);
    }
    if !bytes.is_empty() {
        let mut frame = [0; 65536];
        frame[..bytes.len()].copy_from_slice(bytes);
        output
            .write_chunk(0, bytes.len() as u32, frame)
            .map_err(|_| KernelError::InvalidInput)?;
    }
    output
        .finish(bytes.len() as u64, values)
        .map_err(|_| KernelError::InvalidInput)
}
fn write_history(output: &Output, pairs: &[[u8; 2]]) -> Result<(), KernelError> {
    let bytes: Vec<u8> = pairs.iter().flatten().copied().collect();
    write(output, &bytes, pairs.len() as u64)
}
fn loaded() -> Result<(), KernelError> {
    if LOADED.load(Ordering::Relaxed) {
        Ok(())
    } else {
        Err(KernelError::IncompatibleState)
    }
}
impl common::Guest for Fixture {
    fn setup(_: &Input, config: &Input) -> Result<u64, KernelError> {
        let bytes = read(config)?;
        if bytes.len() != 1 {
            return Err(KernelError::InvalidInput);
        }
        Ok(u64::from(bytes[0]))
    }
    fn load(session: u64, state: &Input) -> Result<(), KernelError> {
        if session == 2 {
            return Err(KernelError::IncompatibleState);
        }
        history(state)?;
        LOADED.store(true, Ordering::Relaxed);
        Ok(())
    }
    fn validate(session: u64, _: &Input) -> Admissibility {
        if loaded().is_err() {
            return Admissibility::Rejected(KernelError::IncompatibleState);
        }
        if session == 3 {
            return Admissibility::Rejected(KernelError::InadmissibleTimestep);
        }
        if session == 8 {
            return Admissibility::Admissible(Some(TimestepAdvice {
                upper_bound_seconds: Some(-1.0),
                recommended_seconds: None,
            }));
        }
        Admissibility::Admissible(None)
    }
    fn checkpoint(_: u64, state: &Input, output: &Output) -> Result<(), KernelError> {
        loaded()?;
        write_history(output, &history(state)?)
    }
    fn restore(session: u64, state: &Input) -> Result<(), KernelError> {
        Self::load(session, state)
    }
    fn close(session: u64) -> Result<(), KernelError> {
        if session == 6 {
            return Err(KernelError::NumericalFailure);
        }
        Ok(())
    }
}
impl dynamics_kernel::Guest for Fixture {
    fn initialize_history(
        session: u64,
        entities: &Input,
        output: &Output,
    ) -> Result<(), KernelError> {
        if session == 1 {
            loop {
                core::hint::spin_loop();
            }
        }
        let pairs: Vec<_> = ids(entities)?.into_iter().map(|id| [id, 0]).collect();
        write_history(output, &pairs)
    }
    fn transition_entities(
        session: u64,
        _: StepContext,
        births: &Input,
        deaths: &Input,
        prior: &Input,
        output: &Output,
    ) -> Result<(), KernelError> {
        loaded()?;
        if session == 5 {
            return Err(KernelError::NumericalFailure);
        }
        let mut pairs = history(prior)?;
        let deaths = ids(deaths)?;
        let births = ids(births)?;
        // A single transition cannot reuse a just-retired identity.
        if births.iter().any(|id| deaths.contains(id)) {
            return Err(KernelError::InvalidInput);
        }
        for id in deaths {
            let at = pairs
                .binary_search_by_key(&id, |p| p[0])
                .map_err(|_| KernelError::InvalidInput)?;
            pairs.remove(at);
        }
        for id in births {
            let at = pairs
                .binary_search_by_key(&id, |p| p[0])
                .err()
                .ok_or(KernelError::InvalidInput)?;
            pairs.insert(at, [id, 0]);
        }
        write_history(output, &pairs)
    }
    fn integrate(
        session: u64,
        _: StepContext,
        entities: &Input,
        forces: &Input,
        prior: &Input,
        next: &Output,
        output: &Output,
    ) -> Result<(), KernelError> {
        loaded()?;
        if session == 7 {
            panic!("intentional integration trap");
        }
        let entity_ids = ids(entities)?;
        if !read(forces)?.is_empty() {
            return Err(KernelError::InvalidInput);
        }
        let mut pairs = history(prior)?;
        if !pairs.iter().map(|p| p[0]).eq(entity_ids.iter().copied()) {
            return Err(KernelError::IncompatibleState);
        }
        for pair in &mut pairs {
            pair[1] = pair[1]
                .checked_add(1)
                .ok_or(KernelError::NumericalFailure)?;
        }
        write(next, &entity_ids, entity_ids.len() as u64)?;
        if session == 4 {
            return Ok(());
        } // completed entities, missing history
        write_history(output, &pairs)
    }
}
export!(Fixture);
