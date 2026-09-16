use super::error;
use crate::field::orishu::simulation::types::{KernelError, TimestepAdvice};

/// Platform step metadata. Scientific buffer schemas still require independent
/// workload admission; a valid context alone does not admit those buffers.
pub use crate::field::orishu::simulation::types::StepContext;

/// Kernel-owned positive finite timestep advice. It never changes the authored dt.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NumericalAdvice {
    /// Conditional maximum timestep, if known.
    pub upper_bound_seconds: Option<f64>,
    /// Conditional recommended timestep, if offered.
    pub recommended_seconds: Option<f64>,
}

/// Bounded, serializable guest rejection. No guest diagnostic strings are lifted.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum KernelRejection {
    /// Input violates the selected contract.
    #[error("kernel rejected invalid input")]
    InvalidInput,
    /// Captured state is incompatible with this kernel/configuration.
    #[error("kernel rejected incompatible state")]
    IncompatibleState,
    /// Authored timestep is not supported for these inputs.
    #[error("kernel rejected timestep")]
    InadmissibleTimestep,
    /// A declared resource bound was exceeded.
    #[error("kernel resource limit exceeded")]
    LimitExceeded,
    /// A requested capability is not implemented.
    #[error("kernel operation unsupported")]
    Unsupported,
    /// Computation could not produce valid numerical output.
    #[error("kernel numerical failure")]
    NumericalFailure,
}
impl From<KernelError> for KernelRejection {
    fn from(value: KernelError) -> Self {
        match value {
            KernelError::InvalidInput => Self::InvalidInput,
            KernelError::IncompatibleState => Self::IncompatibleState,
            KernelError::InadmissibleTimestep => Self::InadmissibleTimestep,
            KernelError::LimitExceeded => Self::LimitExceeded,
            KernelError::Unsupported => Self::Unsupported,
            KernelError::NumericalFailure => Self::NumericalFailure,
        }
    }
}

pub(super) fn accepted<T>(result: Result<T, KernelError>) -> wasmtime::Result<T> {
    result.map_err(|e| KernelRejection::from(e).into())
}
pub(super) fn advice(value: Option<TimestepAdvice>) -> wasmtime::Result<Option<NumericalAdvice>> {
    value
        .map(|value| {
            for number in [value.upper_bound_seconds, value.recommended_seconds]
                .into_iter()
                .flatten()
            {
                if !number.is_finite() || number <= 0.0 {
                    return Err(error(
                        "kernel supplied non-positive or non-finite timestep advice",
                    ));
                }
            }
            if let (Some(upper), Some(recommended)) =
                (value.upper_bound_seconds, value.recommended_seconds)
                && recommended > upper
            {
                return Err(error(
                    "kernel timestep recommendation exceeds its upper bound",
                ));
            }
            Ok(NumericalAdvice {
                upper_bound_seconds: value.upper_bound_seconds,
                recommended_seconds: value.recommended_seconds,
            })
        })
        .transpose()
}

pub(super) fn check_step(step: &StepContext) -> wasmtime::Result<()> {
    if [&step.workload, &step.run, &step.partition, &step.coverage]
        .into_iter()
        .any(|s| s.is_empty() || s.len() > 256)
        || !step.time_seconds.is_finite()
        || step.time_seconds < 0.0
        || !step.dt_seconds.is_finite()
        || step.dt_seconds <= 0.0
        || !(step.time_seconds + step.dt_seconds).is_finite()
        || step.time_seconds + step.dt_seconds <= step.time_seconds
        || step.boundary == u64::MAX
    {
        return Err(error("invalid or excessive step context"));
    }
    Ok(())
}
