//! One sandbox engine for local Kagami execution and Orishu workers.
//!
//! Component Model bindings, bounded buffer grants, selected workload admission
//! and atomic fixed-profile execution. Product integration remains in progress.
//! No formation, networking, authoring authority or plugin installation lives here.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod admission;
mod control;
pub use admission::{AdmissionLimits, AdmittedWorkload, admit};
mod forces;
pub use forces::{FieldForces, ForceReducer, ReductionError, ReductionLimits};
mod grants;
pub use control::{Cancellation, Interruption, OperationControl};
mod run;
mod sandbox;
pub use grants::{Buffer, GrantLimits, HostState, InputGrant, OutputGrant, ScientificBufferError};
pub use run::{
    CapturedRunState, CommittedState, FieldSnapshot, FixedRun, RunCheckpoint, RunFailure, RunField,
    RunKernel, RunLimits, RunPhase, RunProgram, RunRejection, RunScope, StateExtent,
};
pub use sandbox::{
    CompiledKernel, ContextRejection, DynamicsOperation, DynamicsResult, FieldOperation,
    FieldResult, KernelRejection, NumericalAdvice, OutputCapacity, OutputExtent, Sandbox,
    SandboxLimits, StepContext,
};

/// Field execution contract generated from the same WIT read by plugin toolchains.
pub mod field {
    #![allow(missing_docs)] // generated API; the WIT is the authoritative documentation
    wasmtime::component::bindgen!({
        path:"../orishu-plugin/wit",world:"field",
        imports:{default:trappable},
        with:{
            "orishu:simulation/buffers.input":crate::InputGrant,
            "orishu:simulation/buffers.output":crate::OutputGrant,
        },
    });
}

/// Integrator contract shares exactly the field contract's imported buffer types.
pub mod dynamics {
    #![allow(missing_docs)]
    wasmtime::component::bindgen!({
        path:"../orishu-plugin/wit",world:"dynamics",
        with: {
            "orishu:simulation/buffers@1.0.0":crate::field::orishu::simulation::buffers,
            "orishu:simulation/types@1.0.0":crate::field::orishu::simulation::types,
        },
    });
}
