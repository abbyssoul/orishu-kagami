//! Local authoring initialization using the same sandbox as an Orishu worker.
//!
//! These operations prepare immutable candidate bytes; they do not own or edit
//! a document. The window's scientific effect adapter guards document identity,
//! revision and intent for captured-field reset and complete setup configuration.
//! Other adapters must use the same guarded adoption. Opening,
//! saving and exporting must retain captured bytes, never call this initializer.
use crate::plugins::PreparedSelection;
use orishu_plugin::{execution::*, selected, *};
use orishu_runtime::{
    Buffer, CompiledKernel, ContextRejection, DynamicsOperation, DynamicsResult, FieldOperation,
    FieldResult, Interruption, KernelRejection, OperationControl, OutputCapacity, Sandbox,
};
use orishu_variables::VariablesSystem;
use orishu_workload::ComponentInstanceId;
use std::collections::BTreeMap;

/// Authored choices for one exact selected kernel. Boundary settings belong in
/// its declared configuration. No legacy document-domain conversion is implicit.
#[derive(Clone, Debug, PartialEq)]
pub struct InitializationRequest {
    /// Existing configured use in the selected closure, never a provider name.
    pub instance: ComponentInstanceId,
    /// Explicit shared geometric/discretization input.
    pub domain: DomainDescriptor,
    /// Retained authored expressions/literals; declared defaults apply here only.
    pub configuration: Vec<AuthoredConfigurationProperty>,
    /// Requested numerical precision; unsupported precision is a kernel refusal.
    pub compute_precision: ComputePrecision,
    /// Explicit allowed sampling qualities by declared observable slot.
    pub quality_flags: BTreeMap<LocalContributionId, u32>,
}

/// Host limits, separate from scientific configuration. The sandbox may impose
/// tighter budgets. Output lengths are chosen by kernels, not host layout math.
#[derive(Clone, Copy, Debug)]
pub struct InitializationLimits {
    /// Maximum bytes per captured field/history.
    pub state_bytes: usize,
    /// Maximum logical state values independently of bytes.
    pub state_values: u64,
    /// Maximum initial Dynamic entity records and later coupling projection.
    pub projection_records: u32,
    /// Per-request observer point ceiling; zero disables sampling.
    pub sample_points: u32,
    /// Per-request observer channel ceiling; zero disables sampling.
    pub sample_channels: u32,
    /// Shared domain bounds, checked before any guest compilation.
    pub domain: DomainLimits,
}
impl Default for InitializationLimits {
    fn default() -> Self {
        Self {
            state_bytes: 16 * 1024 * 1024,
            state_values: 1_048_576,
            projection_records: 65_536,
            sample_points: 65_536,
            sample_channels: 64,
            domain: DomainLimits::default(),
        }
    }
}

/// Bounded, structured initialization failure; guest diagnostics and arbitrary
/// native error strings are not copied into document/protocol messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitializationError {
    /// Shared metadata, selection or configuration validation failed.
    Declaration(orishu_plugin::Error),
    /// Request names a missing instance or the wrong execution contract.
    Selection,
    /// Caller-owned preparation budget failed before guest execution.
    LimitExceeded,
    /// A standard projection or context failed runtime preflight.
    Context(ContextRejection),
    /// Selected kernel returned a typed scientific refusal.
    Kernel(KernelRejection),
    /// This operation alone was cancelled or exceeded its wall-time budget.
    Interrupted(Interruption),
    /// Component compilation, sandbox execution or buffer protocol failed.
    Sandbox,
}
impl From<orishu_plugin::Error> for InitializationError {
    fn from(value: orishu_plugin::Error) -> Self {
        Self::Declaration(value)
    }
}
impl std::fmt::Display for InitializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Declaration(error) => error.fmt(f),
            Self::Selection => f.write_str("initialization selection or execution role mismatch"),
            Self::LimitExceeded => f.write_str("initialization preparation budget exceeded"),
            Self::Context(reason) => reason.fmt(f),
            Self::Kernel(reason) => reason.fmt(f),
            Self::Interrupted(reason) => reason.fmt(f),
            Self::Sandbox => f.write_str("initialization sandbox or buffer protocol failure"),
        }
    }
}
impl std::error::Error for InitializationError {}
// Keep the underlying runtime error type outside Kagami's public contract while
// preserving its stable domain reasons. Never expose arbitrary trap text.
macro_rules! runtime {
    ($result:expr) => {
        $result.map_err(|e| {
            if let Some(reason) = e.downcast_ref::<Interruption>() {
                InitializationError::Interrupted(*reason)
            } else if let Some(reason) = e.downcast_ref::<KernelRejection>() {
                InitializationError::Kernel(*reason)
            } else if let Some(reason) = e.downcast_ref::<ContextRejection>() {
                InitializationError::Context(*reason)
            } else {
                InitializationError::Sandbox
            }
        })
    };
}

/// Immutable natural state plus every input that gave it meaning. This is a
/// candidate, not proof of full-experiment numerical admissibility or an accepted
/// document revision. Cheap clones share all potentially large byte buffers.
#[derive(Clone, Debug)]
pub struct CapturedInitialization {
    authored: Vec<AuthoredConfigurationProperty>,
    context: InstanceContext,
    context_buffer: Buffer,
    domain: Buffer,
    configuration: Buffer,
    state: Buffer,
    state_identity: InputIdentity,
    history_entities: Option<Buffer>,
}
impl CapturedInitialization {
    /// Transfer this exact initialized candidate to the pure document boundary.
    /// The authority still checks final variables/history and expected revision.
    pub fn document_capture(&self) -> kagami_document::scientific::KernelCapture {
        kagami_document::scientific::KernelCapture {
            context: self.context.clone(),
            authored: self.authored.clone(),
            configuration: self.configuration.bytes.clone(),
            state: self.state.bytes.clone(),
            state_values: self.state.value_count,
            history_entities: self.history_entities.as_ref().map(|v| v.bytes.clone()),
        }
    }
    /// Exact kernel, model/format, provider, input and role bindings.
    pub fn context(&self) -> &InstanceContext {
        &self.context
    }
    /// Canonical portable context bytes.
    pub fn context_buffer(&self) -> &Buffer {
        &self.context_buffer
    }
    /// Exact geometric input; no mutable catalog/install path is retained.
    pub fn domain(&self) -> &Buffer {
        &self.domain
    }
    /// Resolved, dimensioned configuration used for this initialization.
    pub fn configuration(&self) -> &Buffer {
        &self.configuration
    }
    /// Complete kernel-owned state, without unused output capacity.
    pub fn state(&self) -> &Buffer {
        &self.state
    }
    /// Schema/count/size/digest of that immutable state.
    pub fn state_identity(&self) -> &InputIdentity {
        &self.state_identity
    }
    /// Initial membership/kinematics used to initialize numerical history. Fields
    /// always return `None`: object creation order cannot affect natural fields.
    pub fn history_entities(&self) -> Option<&Buffer> {
        self.history_entities.as_ref()
    }
}

/// App-local imperative adapter. The engine owns execution; this adapter derives
/// inputs from shared selected declarations, not from reference-kernel layouts.
pub struct LocalInitializer {
    sandbox: Sandbox,
    limits: InitializationLimits,
}
struct Prepared {
    authored: Vec<AuthoredConfigurationProperty>,
    context: InstanceContext,
    context_buffer: Buffer,
    kernel: CompiledKernel,
    domain: Buffer,
    configuration: Buffer,
}
impl LocalInitializer {
    /// Use an existing bounded worker-compatible engine. No inventory/global
    /// configuration is consulted by initialization after selection is retained.
    pub fn new(sandbox: Sandbox, limits: InitializationLimits) -> Self {
        Self { sandbox, limits }
    }

    /// Initialize one natural field independently of scene entities. The API
    /// deliberately accepts neither coupled entities nor prior field state.
    pub fn field(
        &self,
        selected: &PreparedSelection,
        request: &InitializationRequest,
        variables: &VariablesSystem,
        control: OperationControl,
    ) -> Result<CapturedInitialization, InitializationError> {
        runtime!(control.check())?;
        let p = self.prepare(selected, request, variables, ExecutionContractId::Field)?;
        let schema = state_schema(&p.context);
        let result = runtime!(self.sandbox.invoke_field_bound(
            &p.kernel,
            &p.context,
            p.configuration.clone(),
            FieldOperation::InitializeBounded {
                domain: p.domain.clone(),
                output: self.output(&schema, &p.context),
            },
            control,
        ))?;
        let FieldResult::State(state) = result else {
            return Err(InitializationError::Sandbox);
        };
        Self::capture(p, state, None)
    }

    /// Initialize selected integrator history from the exact initial Dynamics
    /// projection. This is numerical history, not optional observer trails.
    pub fn history(
        &self,
        selected: &PreparedSelection,
        request: &InitializationRequest,
        variables: &VariablesSystem,
        entities: Buffer,
        control: OperationControl,
    ) -> Result<CapturedInitialization, InitializationError> {
        runtime!(control.check())?;
        entities
            .scientific_batch::<DynamicEntity>(BulkLimits {
                bytes: self.limits.state_bytes,
                records: self.limits.projection_records as usize,
            })
            .map_err(|_| InitializationError::Context(ContextRejection::Projection))?;
        let p = self.prepare(selected, request, variables, ExecutionContractId::Dynamics)?;
        let schema = state_schema(&p.context);
        let result = runtime!(self.sandbox.invoke_dynamics_bound(
            &p.kernel,
            &p.context,
            p.configuration.clone(),
            DynamicsOperation::InitializeHistoryBounded {
                entities: entities.clone(),
                history: self.output(&schema, &p.context),
            },
            control,
        ))?;
        let DynamicsResult::History(state) = result else {
            return Err(InitializationError::Sandbox);
        };
        Self::capture(p, state, Some(entities))
    }

    fn output<'a>(&self, schema: &'a str, context: &InstanceContext) -> OutputCapacity<'a> {
        OutputCapacity {
            schema,
            bytes: context.bounds.state_bytes as usize,
            values: self.limits.state_values,
        }
    }
    fn prepare(
        &self,
        selected: &PreparedSelection,
        request: &InitializationRequest,
        variables: &VariablesSystem,
        role: ExecutionContractId,
    ) -> Result<Prepared, InitializationError> {
        if request.quality_flags.len() > 64 {
            return Err(InitializationError::LimitExceeded);
        }
        let selection = selected.compiled().verified();
        let use_ = selection
            .descriptor()
            .kernel_instances
            .iter()
            .find(|k| k.instance_id == request.instance && k.execution_contract == role)
            .ok_or(InitializationError::Selection)?;
        let payload = &selection.payloads()[&use_.contribution];
        let (properties, max_state) = match payload {
            Payload::FieldModels(m) => (&m.scientific.configuration, m.scientific.max_state_bytes),
            Payload::Integrators(m) => (&m.scientific.configuration, u64::MAX),
            _ => return Err(InitializationError::Selection),
        };
        let domain = buffer(DOMAIN_SCHEMA, request.domain.to_cbor(self.limits.domain)?);
        let resolved = resolve_configuration(
            properties,
            &request.configuration,
            variables,
            &Limits::default(),
        )?;
        let configuration = buffer(CONFIGURATION_SCHEMA, resolved.to_cbor(&Limits::default())?);
        let context = selected::build_context(
            selection,
            &request.instance,
            selected::ContextInputs {
                configuration: identity(&configuration)?,
                domain: identity(&domain)?,
                compute_precision: request.compute_precision,
                quality_flags: request.quality_flags.clone(),
                bounds: ExecutionBounds {
                    state_bytes: (self.limits.state_bytes as u64).min(max_state),
                    projection_records: self.limits.projection_records,
                    sample_points: if role == ExecutionContractId::Field {
                        self.limits.sample_points
                    } else {
                        0
                    },
                    sample_channels: if role == ExecutionContractId::Field {
                        self.limits.sample_channels
                    } else {
                        0
                    },
                },
            },
            &Limits::default(),
        )?;
        let context_buffer = buffer(INSTANCE_SCHEMA, context.to_cbor()?);
        let code = selected
            .blobs()
            .get(&context.kernel)
            .ok_or(InitializationError::Selection)?;
        let kernel = runtime!(self.sandbox.compile(code, context.kernel, role))?;
        Ok(Prepared {
            authored: request.configuration.clone(),
            context,
            context_buffer,
            kernel,
            domain,
            configuration,
        })
    }
    fn capture(
        p: Prepared,
        state: Buffer,
        history_entities: Option<Buffer>,
    ) -> Result<CapturedInitialization, InitializationError> {
        let state_identity = identity(&state)?;
        Ok(CapturedInitialization {
            authored: p.authored,
            context: p.context,
            context_buffer: p.context_buffer,
            domain: p.domain,
            configuration: p.configuration,
            state,
            state_identity,
            history_entities,
        })
    }
}
fn state_schema(context: &InstanceContext) -> String {
    format!(
        "{}/v{}",
        context.state_format.id, context.state_format.version
    )
}
fn buffer(schema: &str, bytes: Vec<u8>) -> Buffer {
    Buffer {
        schema: schema.into(),
        value_count: 1,
        bytes: bytes.into(),
    }
}
fn identity(buffer: &Buffer) -> Result<InputIdentity, InitializationError> {
    let schema = buffer
        .schema
        .parse()
        .map_err(|_| InitializationError::Selection)?;
    Ok(InputIdentity::of(schema, buffer.value_count, &buffer.bytes))
}
