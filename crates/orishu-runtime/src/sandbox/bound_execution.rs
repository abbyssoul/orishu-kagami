//! Typed preflight over the low-level lifecycle executor. No plugin-specific
//! format or equation appears here. Selected-closure/run admission is still a
//! separate authority; these checks prevent mismatched invocation inputs.
use super::*;
use orishu_plugin::execution::{
    BulkLimits, BulkRecord, CONFIGURATION_SCHEMA, CoupledEntity, DOMAIN_SCHEMA, DomainDescriptor,
    DomainLimits, DynamicEntity, EntityId, Force, INSTANCE_SCHEMA, InputIdentity, InstanceContext,
    ResolvedConfiguration, SAMPLE_REQUEST_SCHEMA, SAMPLE_RESPONSE_SCHEMA, SampleLimits,
    SampleRequest, SampleResponse, SampleScratch, VALIDATION_SCHEMA, ValidationInputs,
    packet_bytes,
};

/// Stable host rejection before a mismatched scientific invocation can start.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum ContextRejection {
    /// Raw metadata failed its version, bounds or role relationships.
    #[error("invalid scientific instance context")]
    Metadata,
    /// The exact compiled artifact or execution contract differs from the context.
    #[error("scientific context names another kernel")]
    Kernel,
    /// Resolved configuration or domain bytes/count/schema differ from the pin.
    #[error("scientific context input identity mismatch")]
    Input,
    /// Envelope, timestep, profile or entity projection differs from the operation.
    #[error("scientific validation inputs disagree with the invocation")]
    Validation,
    /// Opaque state/history format or size differs from the context.
    #[error("scientific state format or bound mismatch")]
    State,
    /// Standard projection descriptor/packet is malformed or over bound.
    #[error("invalid scientific entity or force projection")]
    Projection,
    /// Query identity, shape, snapshot, or guest sampling result is invalid.
    #[error("invalid scientific sampling request or response")]
    Sampling,
}

struct Bound<'a> {
    context: &'a InstanceContext,
    limits: BulkLimits,
    state_schema: String,
}
impl<'a> Bound<'a> {
    fn new(context: &'a InstanceContext, max_bytes: usize) -> Self {
        Self {
            context,
            limits: BulkLimits {
                bytes: max_bytes,
                records: context.bounds.projection_records as usize,
            },
            state_schema: format!(
                "{}/v{}",
                context.state_format.id, context.state_format.version
            ),
        }
    }
    fn input(&self, identity: &InputIdentity, input: &Buffer) -> wasmtime::Result<()> {
        if input.bytes.len() > self.limits.bytes
            || !identity.matches(&input.schema, input.value_count, &input.bytes)
        {
            return Err(ContextRejection::Input.into());
        }
        Ok(())
    }
    fn state(&self, state: &Buffer) -> wasmtime::Result<()> {
        self.extent(OutputExtent {
            schema: &state.schema,
            bytes: state.bytes.len(),
            values: state.value_count,
        })
    }
    fn extent(&self, state: OutputExtent<'_>) -> wasmtime::Result<()> {
        if state.schema != self.state_schema
            || state.bytes > self.limits.bytes
            || state.bytes as u64 > self.context.bounds.state_bytes
        {
            return Err(ContextRejection::State.into());
        }
        Ok(())
    }
    fn capacity(&self, state: OutputCapacity<'_>) -> wasmtime::Result<()> {
        self.extent(OutputExtent {
            schema: state.schema,
            bytes: state.bytes,
            values: state.values,
        })
    }
    fn projection(&self, entities: &Buffer) -> wasmtime::Result<()> {
        let valid = match self.context.execution_contract {
            ExecutionContractId::Field => entities
                .scientific_batch::<CoupledEntity>(self.limits)
                .map(|_| ()),
            ExecutionContractId::Dynamics => entities
                .scientific_batch::<DynamicEntity>(self.limits)
                .map(|_| ()),
        };
        valid.map_err(|_| ContextRejection::Projection.into())
    }
    fn projection_extent<R: BulkRecord>(
        &self,
        extent: OutputExtent<'_>,
        count: usize,
    ) -> wasmtime::Result<()> {
        let bytes =
            packet_bytes::<R>(count, self.limits).map_err(|_| ContextRejection::Projection)?;
        if extent.schema != R::SCHEMA || extent.values != count as u64 || extent.bytes != bytes {
            return Err(ContextRejection::Projection.into());
        }
        Ok(())
    }
    fn validation(
        &self,
        input: &Buffer,
        step: Option<&StepContext>,
        entities: Option<&Buffer>,
    ) -> wasmtime::Result<()> {
        if input.schema != VALIDATION_SCHEMA || input.value_count != 1 {
            return Err(ContextRejection::Validation.into());
        }
        let input = ValidationInputs::read(&input.bytes, self.context, self.limits)
            .map_err(|_| ContextRejection::Validation)?;
        DomainDescriptor::from_cbor(input.domain, DomainLimits { cells: u64::MAX })
            .map_err(|_| ContextRejection::Input)?;
        if let Some(step) = step {
            common::check_step(step)?;
            if input.timestep_seconds.get() != step.dt_seconds {
                return Err(ContextRejection::Validation.into());
            }
        }
        if let Some(entities) = entities {
            self.projection(entities)?;
            if input.entities != &*entities.bytes {
                return Err(ContextRejection::Validation.into());
            }
        }
        Ok(())
    }
}

impl Sandbox {
    fn bound_context(
        &self,
        kernel: &CompiledKernel,
        context: &InstanceContext,
        config: &Buffer,
    ) -> wasmtime::Result<Buffer> {
        let bytes = context.to_cbor().map_err(|_| ContextRejection::Metadata)?;
        if context.kernel != kernel.artifact || context.execution_contract != kernel.contract {
            return Err(ContextRejection::Kernel.into());
        }
        Bound::new(context, self.limits.grants.bytes).input(&context.configuration, config)?;
        if context.configuration.schema.as_str() != CONFIGURATION_SCHEMA
            || context.configuration.value_count != 1
            || context.domain.schema.as_str() != DOMAIN_SCHEMA
            || context.domain.value_count != 1
        {
            return Err(ContextRejection::Input.into());
        }
        ResolvedConfiguration::from_cbor(&config.bytes, &orishu_plugin::Limits::default())
            .map_err(|_| ContextRejection::Input)?;
        Ok(Buffer {
            schema: INSTANCE_SCHEMA.into(),
            value_count: 1,
            bytes: bytes.into(),
        })
    }
    /// Execute with exact standard instance/validation binding. This is stronger
    /// than `invoke_field`'s ABI-only harness but is not independent selected-release
    /// closure admission, snapshot provenance or an atomic run commit.
    pub fn invoke_field_bound(
        &self,
        kernel: &CompiledKernel,
        context: &InstanceContext,
        config: Buffer,
        operation: FieldOperation<'_>,
        control: OperationControl,
    ) -> wasmtime::Result<FieldResult> {
        let control = control.bounded(self.limits.operation_timeout)?;
        control.check()?;
        let setup = self.bound_context(kernel, context, &config)?;
        let bound = Bound::new(context, self.limits.grants.bytes);
        let sample_policy = SampleLimits::default();
        let sample_limits = SampleLimits {
            bytes: self.limits.grants.bytes.min(sample_policy.bytes),
            points: (context.bounds.sample_points as usize).min(sample_policy.points),
            channels: (context.bounds.sample_channels as usize).min(sample_policy.channels),
            ..sample_policy
        };
        // Keep the immutable query alive through execution; the response must be
        // checked against this exact request, not only its allocation descriptor.
        let sample_query = match &operation {
            FieldOperation::Sample { query, .. } => Some(query.clone()),
            _ => None,
        };
        let mut sample_scratch = SampleScratch::default();
        let sample_request = sample_query
            .as_ref()
            .map(|query| {
                SampleRequest::read(&query.bytes, &mut sample_scratch, sample_limits)
                    .map_err(|_| ContextRejection::Sampling)
            })
            .transpose()?;
        match &operation {
            FieldOperation::InitializeBounded { domain, output } => {
                bound.input(&context.domain, domain)?;
                DomainDescriptor::from_cbor(&domain.bytes, DomainLimits { cells: u64::MAX })
                    .map_err(|_| ContextRejection::Input)?;
                bound.capacity(*output)?;
            }
            FieldOperation::Initialize { domain, output } => {
                bound.input(&context.domain, domain)?;
                DomainDescriptor::from_cbor(&domain.bytes, DomainLimits { cells: u64::MAX })
                    .map_err(|_| ContextRejection::Input)?;
                bound.extent(*output)?;
            }
            FieldOperation::Validate { state, inputs } => {
                bound.state(state)?;
                bound.validation(inputs, None, None)?;
            }
            FieldOperation::Advance {
                step,
                prior,
                entities,
                validation,
                field,
                forces,
            } => {
                bound.state(prior)?;
                bound.extent(*field)?;
                bound.validation(validation, Some(step), Some(entities))?;
                let coupled = entities.scientific_batch::<CoupledEntity>(bound.limits)?;
                let mut previous = None;
                let mut count = 0;
                for e in coupled.iter().filter(CoupledEntity::responds_dynamically) {
                    if previous != Some(e.id) {
                        count += 1;
                        previous = Some(e.id);
                    }
                }
                bound.projection_extent::<Force>(*forces, count)?;
            }
            FieldOperation::Sample {
                snapshot,
                query,
                output,
            } => {
                bound.state(snapshot)?;
                let request = sample_request.as_ref().ok_or(ContextRejection::Sampling)?;
                request
                    .metadata()
                    .check_context(context)
                    .map_err(|_| ContextRejection::Sampling)?;
                bound.input(&request.metadata().snapshot.state, snapshot)?;
                let layout = request
                    .layout(sample_limits)
                    .map_err(|_| ContextRejection::Sampling)?;
                if query.schema != SAMPLE_REQUEST_SCHEMA
                    || query.value_count != request.len() as u64
                    || output.schema != SAMPLE_RESPONSE_SCHEMA
                    || output.values != query.value_count
                    || output.bytes != layout.bytes
                {
                    return Err(ContextRejection::Sampling.into());
                }
            }
            FieldOperation::Checkpoint { state, output } => {
                bound.state(state)?;
                bound.extent(*output)?;
            }
            FieldOperation::Restore {
                checkpoint,
                validation,
                output,
            } => {
                bound.state(checkpoint)?;
                bound.extent(*output)?;
                bound.validation(validation, None, None)?;
            }
        }
        control.check()?;
        let result = self.invoke_field(kernel, setup, config, operation, control.clone())?;
        if let (Some(request), FieldResult::Samples(samples)) = (&sample_request, &result) {
            SampleResponse::read(&samples.bytes, request, context, sample_limits)
                .map_err(|_| ContextRejection::Sampling)?;
        }
        control.check()?;
        Ok(result)
    }
    /// Dynamics counterpart with exact setup, state-format and validation binding.
    /// Integrator formula and history interpretation remain entirely inside the
    /// selected guest; immutable manifest and whole-run admission remain required.
    pub fn invoke_dynamics_bound(
        &self,
        kernel: &CompiledKernel,
        context: &InstanceContext,
        config: Buffer,
        operation: DynamicsOperation<'_>,
        control: OperationControl,
    ) -> wasmtime::Result<DynamicsResult> {
        let control = control.bounded(self.limits.operation_timeout)?;
        control.check()?;
        let setup = self.bound_context(kernel, context, &config)?;
        let bound = Bound::new(context, self.limits.grants.bytes);
        match &operation {
            DynamicsOperation::InitializeHistoryBounded { entities, history } => {
                bound.projection(entities)?;
                bound.capacity(*history)?;
            }
            DynamicsOperation::InitializeHistory { entities, history } => {
                bound.projection(entities)?;
                bound.extent(*history)?;
            }
            DynamicsOperation::Validate { history, inputs } => {
                bound.state(history)?;
                bound.validation(inputs, None, None)?;
            }
            DynamicsOperation::Integrate {
                step,
                entities,
                forces,
                prior_history,
                validation,
                history,
                next_entities,
            } => {
                bound.state(prior_history)?;
                bound.extent(*history)?;
                forces
                    .scientific_batch::<Force>(bound.limits)
                    .map_err(|_| ContextRejection::Projection)?;
                bound.validation(validation, Some(step), Some(entities))?;
                bound.projection_extent::<DynamicEntity>(
                    *next_entities,
                    entities.value_count as usize,
                )?;
            }
            DynamicsOperation::TransitionEntities {
                step,
                births,
                deaths,
                prior_history,
                validation,
                history,
            } => {
                bound.state(prior_history)?;
                bound.extent(*history)?;
                bound.projection(births)?;
                deaths
                    .scientific_batch::<EntityId>(bound.limits)
                    .map_err(|_| ContextRejection::Projection)?;
                bound.validation(validation, Some(step), None)?;
            }
            DynamicsOperation::Checkpoint { history, output } => {
                bound.state(history)?;
                bound.extent(*output)?;
            }
            DynamicsOperation::Restore {
                checkpoint,
                validation,
                output,
            } => {
                bound.state(checkpoint)?;
                bound.extent(*output)?;
                bound.validation(validation, None, None)?;
            }
        }
        control.check()?;
        self.invoke_dynamics(kernel, setup, config, operation, control)
    }
}
