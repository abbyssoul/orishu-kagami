use crate::{Buffer, GrantLimits, HostState, OperationControl, dynamics, field};
use orishu_plugin::{ArtifactDigest, ExecutionContractId};
use std::time::Duration;
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, Resource},
};

mod bound_execution;
mod common;
pub use bound_execution::ContextRejection;
pub use common::{KernelRejection, NumericalAdvice, StepContext};
mod dynamics_execution;
mod field_execution;
pub use dynamics_execution::{DynamicsOperation, DynamicsResult};
pub use field_execution::{FieldOperation, FieldResult};

/// Explicit initial host policy; not workload identity or a numerical recommendation.
#[derive(Clone, Copy, Debug)]
pub struct SandboxLimits {
    /// Maximum original component bytes before validation/JIT compilation.
    pub component_bytes: usize,
    /// Maximum one guest linear memory; this profile permits one memory per store.
    pub memory_bytes: usize,
    /// Maximum elements per guest table (at most eight tables per store).
    pub table_elements: usize,
    /// Native stack ceiling for guest execution.
    pub stack_bytes: usize,
    /// Instruction fuel for each complete bounded host operation including setup.
    pub fuel: u64,
    /// Maximum wall time of an instantiated guest operation, including setup.
    pub operation_timeout: Duration,
    /// Invocation-scoped host input/output and transfer budgets.
    pub grants: GrantLimits,
}
impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            component_bytes: 32 * 1024 * 1024,
            memory_bytes: 256 * 1024 * 1024,
            table_elements: 4096,
            stack_bytes: 512 * 1024,
            fuel: 10_000_000,
            operation_timeout: Duration::from_secs(5),
            grants: GrantLimits::default(),
        }
    }
}

/// Verified executable technology/contract, not proof of numerical admissibility.
/// Compiled code is a disposable local cache, never a workload artifact.
pub struct CompiledKernel {
    component: Component,
    artifact: ArtifactDigest,
    contract: ExecutionContractId,
}

/// Exact output shape reserved before a kernel runs; not an allocation requested
/// by the guest or proof that the resulting bytes satisfy the scientific schema.
#[derive(Clone, Copy, Debug)]
pub struct OutputExtent<'a> {
    /// Portable output format identity.
    pub schema: &'a str,
    /// Exact byte extent for this invocation.
    pub bytes: usize,
    /// Exact schema-defined logical value count.
    pub values: u64,
}
/// Host-owned ceilings for an opaque candidate whose size is kernel-defined.
/// These are capacity limits, not layout assumptions or requests to fill padding.
#[derive(Clone, Copy, Debug)]
pub struct OutputCapacity<'a> {
    /// Exact portable format identity, unchanged by the guest.
    pub schema: &'a str,
    /// Maximum bytes; the full ceiling is charged to the grant budget.
    pub bytes: usize,
    /// Maximum schema-defined logical values; actual count comes from finish.
    pub values: u64,
}
impl CompiledKernel {
    /// Content identity of the original portable component bytes.
    pub fn artifact(&self) -> ArtifactDigest {
        self.artifact
    }
    /// Exactly one platform-owned scientific execution contract.
    pub fn contract(&self) -> ExecutionContractId {
        self.contract
    }
}

/// Shared metered Component Model host. No WASI or native guest capability is linked.
pub struct Sandbox {
    engine: Engine,
    limits: SandboxLimits,
    _ticker: crate::control::EpochTicker,
}
fn error(message: &str) -> wasmtime::Error {
    wasmtime::Error::msg(message.to_owned())
}
impl Sandbox {
    /// Construct the engine with explicit deterministic arithmetic and resource
    /// policy. This does not initialize any plugin or scientific state.
    pub fn new(limits: SandboxLimits) -> wasmtime::Result<Self> {
        if limits.component_bytes == 0
            || limits.memory_bytes == 0
            || limits.stack_bytes == 0
            || limits.fuel == 0
            || limits.operation_timeout.is_zero()
        {
            return Err(error("zero sandbox budget"));
        }
        let mut config = Config::new();
        config
            .wasm_component_model(true)
            .wasm_component_model_fixed_length_lists(true)
            .consume_fuel(true)
            .epoch_interruption(true)
            .max_wasm_stack(limits.stack_bytes)
            .cranelift_nan_canonicalization(true)
            .wasm_relaxed_simd(false);
        let engine = Engine::new(&config)?;
        let ticker = crate::control::EpochTicker::new(engine.clone())?;
        Ok(Self {
            engine,
            limits,
            _ticker: ticker,
        })
    }
    fn linker(&self) -> wasmtime::Result<Linker<HostState>> {
        let mut linker = Linker::new(&self.engine);
        field::Field::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |state| {
            state
        })?;
        Ok(linker)
    }
    /// Validate exact bytes, reject unauthorized top-level imports/exports and
    /// type-check the required world before instantiation (no guest start runs).
    pub fn compile(
        &self,
        bytes: &[u8],
        expected: ArtifactDigest,
        contract: ExecutionContractId,
    ) -> wasmtime::Result<CompiledKernel> {
        if bytes.len() > self.limits.component_bytes {
            return Err(error("component byte budget exceeded"));
        }
        if !expected.matches(bytes) {
            return Err(error("component artifact digest mismatch"));
        }
        // Reject text/core modules; portable submitted artifacts are components.
        if bytes.get(..8) != Some(b"\0asm\x0d\0\x01\0") {
            return Err(error("expected a binary WebAssembly Component"));
        }
        let component = Component::new(&self.engine, bytes)?;
        let ty = component.component_type();
        for (name, _) in ty.imports(&self.engine) {
            if ![
                "orishu:simulation/buffers@1.0.0",
                "orishu:simulation/types@1.0.0",
            ]
            .contains(&name)
            {
                return Err(error("unauthorized component import"));
            }
        }
        let scientific = match contract {
            ExecutionContractId::Field => "orishu:simulation/field-kernel@1.0.0",
            ExecutionContractId::Dynamics => "orishu:simulation/dynamics-kernel@1.0.0",
        };
        for (name, _) in ty.exports(&self.engine) {
            if ![
                scientific,
                "orishu:simulation/common@1.0.0",
                "orishu:simulation/types@1.0.0",
            ]
            .contains(&name)
            {
                return Err(error("unexpected or second scientific execution export"));
            }
        }
        let pre = self.linker()?.instantiate_pre(&component)?;
        match contract {
            ExecutionContractId::Field => {
                field::FieldPre::new(pre)?;
            }
            ExecutionContractId::Dynamics => {
                dynamics::DynamicsPre::new(pre)?;
            }
        }
        Ok(CompiledKernel {
            component,
            artifact: expected,
            contract,
        })
    }
    fn store(&self, control: OperationControl) -> wasmtime::Result<Store<HostState>> {
        control.check()?;
        let mut store = Store::new(
            &self.engine,
            HostState::new(
                self.limits.memory_bytes,
                self.limits.table_elements,
                self.limits.grants,
            ),
        );
        store.limiter(|state| &mut state.memory);
        store.set_fuel(self.limits.fuel)?;
        store.data_mut().set_control(control.clone());
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(move |_| {
            control.check()?;
            Ok(wasmtime::UpdateDeadline::Continue(1))
        });
        Ok(store)
    }
    /// Invoke natural field initialization in an isolated component instance.
    /// There is deliberately no entity argument. The caller must validate and
    /// capture the result through its document authority; this operation does not
    /// admit a workload, regenerate saved state, or change an existing run.
    pub fn initialize_field(
        &self,
        kernel: &CompiledKernel,
        context: Buffer,
        config: Buffer,
        domain: Buffer,
        extent: OutputExtent<'_>,
    ) -> wasmtime::Result<Buffer> {
        self.initialize_field_with_control(
            kernel,
            context,
            config,
            domain,
            extent,
            OperationControl::new(self.limits.operation_timeout)?,
        )
    }
    /// Initialization with caller cancellation and a deadline clamped to host policy.
    /// All guest execution, including module starts and setup, is supervised.
    pub fn initialize_field_with_control(
        &self,
        kernel: &CompiledKernel,
        context: Buffer,
        config: Buffer,
        domain: Buffer,
        extent: OutputExtent<'_>,
        control: OperationControl,
    ) -> wasmtime::Result<Buffer> {
        match self.invoke_field(
            kernel,
            context,
            config,
            FieldOperation::Initialize {
                domain,
                output: extent,
            },
            control,
        )? {
            FieldResult::State(state) => Ok(state),
            _ => Err(error("internal initialization outcome mismatch")),
        }
    }
}
