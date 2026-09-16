//! ABI/security fixture, not a scientific gravity model. Build for
//! wasm32-unknown-unknown, then wrap its embedded WIT with wit-component.
wit_bindgen::generate!({path:"../../../orishu-plugin/wit",world:"field"});
use core::sync::atomic::{AtomicBool, Ordering};
use exports::orishu::simulation::{common, field_kernel};
use orishu::simulation::{
    buffers::{Input, Output},
    types::{Admissibility, KernelError, StepContext, TimestepAdvice},
};

static LOADED: AtomicBool = AtomicBool::new(false);

struct Fixture;
fn copy(input: &Input, output: &Output) -> Result<(), KernelError> {
    let descriptor = input.describe();
    let mut offset = 0;
    let mut frame = [0u8; 65536];
    while offset < descriptor.byte_length {
        let n = (descriptor.byte_length - offset).min(1024) as u32;
        let bytes = input
            .read_chunk(offset, n)
            .map_err(|_| KernelError::InvalidInput)?;
        frame[..bytes.len()].copy_from_slice(&bytes);
        output
            .write_chunk(offset, bytes.len() as u32, frame)
            .map_err(|_| KernelError::InvalidInput)?;
        offset += u64::from(n);
    }
    output
        .finish(descriptor.byte_length, descriptor.value_count)
        .map_err(|_| KernelError::InvalidInput)
}
impl common::Guest for Fixture {
    fn setup(_: &Input, config: &Input) -> Result<u64, KernelError> {
        let data = config
            .read_chunk(0, 1)
            .map_err(|_| KernelError::InvalidInput)?;
        Ok(u64::from(data[0]))
    }
    fn load(session: u64, _: &Input) -> Result<(), KernelError> {
        if session == 7 {
            return Err(KernelError::IncompatibleState);
        }
        LOADED.store(true, Ordering::Relaxed);
        Ok(())
    }
    fn validate(session: u64, _: &Input) -> Admissibility {
        if !LOADED.load(Ordering::Relaxed) {
            return Admissibility::Rejected(KernelError::IncompatibleState);
        }
        if session == 8 {
            return Admissibility::Rejected(KernelError::InadmissibleTimestep);
        }
        if session == 14 {
            return Admissibility::Admissible(Some(TimestepAdvice {
                upper_bound_seconds: Some(f64::NAN),
                recommended_seconds: None,
            }));
        }
        if session == 15 {
            return Admissibility::Admissible(Some(TimestepAdvice {
                upper_bound_seconds: Some(0.01),
                recommended_seconds: Some(0.005),
            }));
        }
        Admissibility::Admissible(None)
    }
    fn checkpoint(session: u64, state: &Input, output: &Output) -> Result<(), KernelError> {
        if session == 9 {
            return Err(KernelError::NumericalFailure);
        }
        if !LOADED.load(Ordering::Relaxed) {
            return Err(KernelError::IncompatibleState);
        }
        copy(state, output)
    }
    fn restore(session: u64, _: &Input) -> Result<(), KernelError> {
        if session == 10 {
            return Err(KernelError::IncompatibleState);
        }
        LOADED.store(true, Ordering::Relaxed);
        Ok(())
    }
    fn close(session: u64) -> Result<(), KernelError> {
        if session == 11 {
            return Err(KernelError::NumericalFailure);
        }
        Ok(())
    }
}
impl field_kernel::Guest for Fixture {
    fn initialize(session: u64, domain: &Input, output: &Output) -> Result<(), KernelError> {
        match session {
            1 => loop {
                core::hint::spin_loop()
            }, // instruction fuel must interrupt
            2 => panic!("intentional fixture trap"),
            3 => return Ok(()), // no finish: must never become an accepted buffer
            4 => {
                // Out-of-range host call is rejected before touching a buffer.
                if output.write_chunk(u64::MAX, 1, [7; 65536]).is_ok() {
                    return Err(KernelError::NumericalFailure);
                }
            }
            5 => {
                let desc = output.describe();
                let _ = output.finish(desc.byte_length, desc.value_count);
                return Ok(());
            }
            6 => {
                let _ = output.write_chunk(0, u32::MAX, [0; 65536]);
            }
            _ => (),
        }
        copy(domain, output)
    }
    fn advance(
        session: u64,
        _: StepContext,
        prior: &Input,
        _: &Input,
        field: &Output,
        forces: &Output,
    ) -> Result<(), KernelError> {
        if !LOADED.load(Ordering::Relaxed) {
            return Err(KernelError::IncompatibleState);
        }
        copy(prior, field)?;
        if session == 12 {
            return Ok(());
        } // field finished but forces absent
        forces.finish(0, 0).map_err(|_| KernelError::InvalidInput)
    }
    fn sample(
        session: u64,
        _: &Input,
        snapshot: &Input,
        output: &Output,
    ) -> Result<(), KernelError> {
        if !LOADED.load(Ordering::Relaxed) {
            return Err(KernelError::IncompatibleState);
        }
        LOADED.store(false, Ordering::Relaxed); // must never alter another operation
        copy(snapshot, output)?;
        if session == 13 {
            panic!("sample trapped after writing output");
        }
        Ok(())
    }
}
export!(Fixture);
