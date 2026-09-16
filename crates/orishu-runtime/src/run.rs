//! Single-partition fixed-profile state owner. Workload closure admission remains
//! outside this module; once constructed, only a whole validated step replaces its
//! committed state. There is no host-owned integration or field equation here.
use crate::{
    Buffer, CompiledKernel, DynamicsOperation, DynamicsResult, FieldForces, FieldOperation,
    FieldResult, ForceReducer, OperationControl, OutputExtent, ReductionLimits, Sandbox,
    StepContext,
};
use orishu_plugin::{ArtifactDigest, ExecutionContractId, FiniteF64, execution::*};
use orishu_workload::{ComponentInstanceId, WorkloadDigest};
use std::sync::Arc;
mod observation;
pub use observation::FieldSnapshot;
use observation::SnapshotBudget;

/// Execution source supplied by the embedding admission authority. The immutable
/// run descriptor must bind the protocol's formation/workload/epoch identity;
/// this content reference is not an independently allocated cluster run ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunScope {
    /// Independently admitted immutable workload root.
    pub workload: WorkloadDigest,
    /// Exact immutable run-descriptor artifact.
    pub run: ArtifactDigest,
    /// Execution epoch, not an attempt counter.
    pub epoch: u64,
}
/// Exact caller-declared portable output extent, never derived from Rust layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateExtent {
    /// Versioned portable schema.
    pub schema: orishu_workload::SchemaId,
    /// Reserved output bytes.
    pub bytes: usize,
    /// Schema-defined logical value count.
    pub values: u64,
}
impl StateExtent {
    fn borrowed(&self) -> OutputExtent<'_> {
        OutputExtent {
            schema: self.schema.as_str(),
            bytes: self.bytes,
            values: self.values,
        }
    }
    fn matches(&self, buffer: &Buffer) -> bool {
        self.schema.as_str() == buffer.schema
            && self.bytes == buffer.bytes.len()
            && self.values == buffer.value_count
    }
}
/// Exact selected kernel inputs. A verified Component does not itself prove the
/// contribution closure; the embedding workload admission must establish that.
pub struct RunKernel {
    /// Independently compiled exact Component.
    pub kernel: Arc<CompiledKernel>,
    /// Selected scientific/provider context.
    pub context: InstanceContext,
    /// Captured dimensioned configuration, not expressions/default lookup.
    pub configuration: Buffer,
    /// Captured explicit geometry/discretization.
    pub domain: Buffer,
    /// Declared output/checkpoint extent in the first fixed-storage profile.
    pub state_extent: StateExtent,
}
/// One field's initial resolved coupling projection. During a run, the owner
/// refreshes kinematics from its object state; role strengths stay captured.
pub struct RunField {
    /// Exact selected field kernel and inputs.
    pub binding: RunKernel,
    /// Canonical coupled-entity packet, consistent with the captured objects.
    pub coupled: Buffer,
}
/// Raw fixed-profile execution inputs, to be produced by independent workload
/// admission. Does not carry catalogs, plugin installation paths or a resolver.
pub struct RunProgram {
    /// Complete field set, in ascending instance-ID order.
    pub fields: Vec<RunField>,
    /// One selected integrator, including an explicitly empty dynamic set.
    pub dynamics: RunKernel,
    /// Authored fixed SI timestep; advice never changes it.
    pub timestep_seconds: FiniteF64,
}
/// Scientific input state captured during authoring, not regenerated at start.
pub struct CapturedRunState {
    /// Complete canonical object packet, including static uncoupled objects.
    pub objects: Buffer,
    /// Complete field set in program order, in each declared portable format.
    pub fields: Vec<Buffer>,
    /// Explicit selected integrator history for the initial dynamic membership.
    pub history: Buffer,
}
/// Owner-local aggregate limits; sandbox/grant limits additionally apply.
#[derive(Clone, Copy, Debug)]
pub struct RunLimits {
    /// Complete selected field count.
    pub fields: usize,
    /// Total object count, including static entities.
    pub objects: usize,
    /// Aggregate coupling records across fields.
    pub couplings: usize,
    /// Captured/candidate scientific bytes plus projected packets/force buffers.
    pub scientific_bytes: usize,
    /// Aggregate cold setup/configuration/domain bytes retained by this owner.
    pub input_bytes: usize,
    /// Maximum simultaneous observer snapshot leases (one sampling call per lease).
    pub snapshots: usize,
    /// Maximum sum of field-state bytes retained by observer leases.
    pub snapshot_bytes: usize,
}
impl Default for RunLimits {
    fn default() -> Self {
        Self {
            fields: 64,
            objects: 1_000_000,
            couplings: 4_000_000,
            scientific_bytes: 512 * 1024 * 1024,
            input_bytes: 64 * 1024 * 1024,
            snapshots: 8,
            snapshot_bytes: 128 * 1024 * 1024,
        }
    }
}
/// Stage attribution for an attempted boundary. Guest details remain typed in
/// the error chain; this context never copies guest-controlled diagnostic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunPhase {
    /// Captured input validation, with no natural initialization.
    Admission,
    /// Field state and force computation.
    Field,
    /// Complete force coverage/reduction.
    Reduction,
    /// One selected integrator invocation.
    Dynamics,
    /// All candidate field/history/entity checks before publication.
    Validation,
    /// Embedding authority refused complete-candidate publication.
    Publication,
    /// Complete portable checkpoint production.
    Checkpoint,
    /// Portable restore, never natural initialization.
    Restore,
}
/// Stable owner rejection; errors cannot publish partial scientific state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunRejection {
    /// Count/byte/product/allocation budget exceeded.
    #[error("run resource limit exceeded")]
    Limit,
    /// Missing/duplicate/misordered roles, fields or state parts.
    #[error("run definition or state coverage mismatch")]
    Definition,
    /// Wrong entity identity, mass, coupling or static kinematics.
    #[error("run entity projection mismatch")]
    Objects,
    /// Stop is terminal for this owner; construct an explicitly resumed owner.
    #[error("run is stopped")]
    Stopped,
    /// Fixed time or monotone boundary/invocation cannot advance representably.
    #[error("run time or counter exhausted")]
    Time,
    /// Checkpoint belongs to another run or exact scientific definition.
    #[error("checkpoint run or definition mismatch")]
    Checkpoint,
}
/// Host-owned failure attribution, accessible in the returned error chain.
#[derive(Clone, Debug, thiserror::Error)]
#[error("run {run} epoch {epoch} boundary {boundary}, {phase:?}, instance {instance:?}")]
pub struct RunFailure {
    /// Immutable workload root associated with the attempted operation.
    pub workload: WorkloadDigest,
    /// Immutable run descriptor reference.
    pub run: ArtifactDigest,
    /// Attempted execution epoch.
    pub epoch: u64,
    /// Last committed boundary; no new boundary was published.
    pub boundary: u64,
    /// Failed host/kernel stage.
    pub phase: RunPhase,
    /// Selected instance where applicable.
    pub instance: Option<ComponentInstanceId>,
}

/// One immutable whole scientific boundary. It is only constructed by this
/// owner; callers cannot insert candidates into the committed view.
#[derive(Clone, Debug)]
pub struct CommittedState {
    boundary: u64,
    time: FiniteF64,
    objects: Buffer,
    fields: Vec<Buffer>,
    field_identities: Vec<InputIdentity>,
    history: Buffer,
    forces: Option<Buffer>,
}
impl CommittedState {
    /// Fixed committed boundary index.
    pub fn boundary(&self) -> u64 {
        self.boundary
    }
    /// SI simulation time, not wall time or playback time.
    pub fn time_seconds(&self) -> FiniteF64 {
        self.time
    }
    /// Whole object state; no Dynamics means its kinematics are unchanged.
    pub fn objects(&self) -> &Buffer {
        &self.objects
    }
    /// Field states in the admitted program's canonical order.
    pub fn fields(&self) -> &[Buffer] {
        &self.fields
    }
    /// Complete scientific integrator history, unrelated to display trails.
    pub fn history(&self) -> &Buffer {
        &self.history
    }
    /// Reduced forces that produced this boundary, evaluated at its predecessor.
    /// Initial captured state has no computed force batch, rather than invented zeros.
    pub fn forces(&self) -> Option<&Buffer> {
        self.forces.as_ref()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Definition {
    contexts: Vec<InstanceContext>,
    couplings: Vec<InputIdentity>,
    extents: Vec<StateExtent>,
    dt: FiniteF64,
}
/// Complete in-memory portable checkpoint result. Storage framing/provenance
/// admission is a separate adapter; this is not a newly invented file format.
pub struct RunCheckpoint {
    scope: RunScope,
    definition: Arc<Definition>,
    state: CommittedState,
    next_invocation: u64,
}
impl RunCheckpoint {
    /// Exact run/epoch source.
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    /// Read-only complete portable state, never a subset advertised as complete.
    pub fn state(&self) -> &CommittedState {
        &self.state
    }
}

/// Single-partition scientific state owner using the real shared sandbox. Calls
/// are synchronous; the application owns scheduling and cancellation. Rejected
/// attempts preserve all committed data and simulation time; `stop` is terminal.
///
/// This first owner uses disposable guest stores and immutable buffer candidates.
/// Cold decoding/store reuse and bounded observer leases remain integration work;
/// do not use this path to claim allocation-free stepping or cluster admission.
pub struct FixedRun {
    sandbox: Arc<Sandbox>,
    scope: RunScope,
    program: RunProgram,
    definition: Arc<Definition>,
    limits: RunLimits,
    committed: CommittedState,
    next_invocation: u64,
    stopped: bool,
    reducer: ForceReducer,
    selected: Vec<ComponentInstanceId>,
    snapshots: Arc<SnapshotBudget>,
}
impl FixedRun {
    /// Validate every captured part and numerical input before exposing boundary
    /// zero. This never calls field initialization or history initialization.
    pub fn from_captured(
        sandbox: Arc<Sandbox>,
        scope: RunScope,
        program: RunProgram,
        captured: CapturedRunState,
        limits: RunLimits,
        control: OperationControl,
    ) -> wasmtime::Result<Self> {
        let definition = Arc::new(check_program(&program, &captured, limits, true)?);
        let field_identities = field_identities(&captured.fields)?;
        let selected = program
            .fields
            .iter()
            .map(|f| f.binding.context.instance.clone())
            .collect();
        let run = Self {
            sandbox,
            scope,
            program,
            definition,
            limits,
            committed: CommittedState {
                boundary: 0,
                time: FiniteF64::ZERO,
                objects: captured.objects,
                fields: captured.fields,
                field_identities,
                history: captured.history,
                forces: None,
            },
            next_invocation: 1,
            stopped: false,
            reducer: ForceReducer::default(),
            selected,
            snapshots: Arc::new(SnapshotBudget::new(limits.snapshots, limits.snapshot_bytes)),
        };
        run.validate_state(&run.committed, RunPhase::Admission, &control)?;
        control.check()?;
        Ok(run)
    }
    /// The last whole committed boundary. No candidate is returned by `advance`.
    pub fn state(&self) -> &CommittedState {
        &self.committed
    }
    /// Exact source identity supplied by independent admission.
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    /// Acquire one committed field without retaining a borrow of the run. The
    /// isolated sampling call can then run on another thread while this owner
    /// advances. Exhausted observer budgets reject acquisition, never commit.
    pub fn acquire_field(&self, instance: &ComponentInstanceId) -> wasmtime::Result<FieldSnapshot> {
        let index = self
            .selected
            .binary_search(instance)
            .map_err(|_| RunRejection::Definition)?;
        FieldSnapshot::new(
            self.sandbox.clone(),
            &self.program.fields[index].binding,
            self.committed.fields[index].clone(),
            self.committed.field_identities[index].clone(),
            SnapshotSource::Committed {
                workload: self.scope.workload,
                run: self.scope.run,
                epoch: self.scope.epoch,
                boundary: self.committed.boundary,
                time_seconds: self.committed.time,
            },
            self.snapshots.clone(),
        )
    }
    /// Stop future advances, retaining the last committed state for checkpointing.
    pub fn stop(&mut self) {
        self.stopped = true;
    }
    /// Whether this owner has been explicitly stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
    fn failure(&self, phase: RunPhase, instance: Option<&ComponentInstanceId>) -> RunFailure {
        RunFailure {
            workload: self.scope.workload,
            run: self.scope.run,
            epoch: self.scope.epoch,
            boundary: self.committed.boundary,
            phase,
            instance: instance.cloned(),
        }
    }
    fn step_context(&self, invocation: u64) -> wasmtime::Result<StepContext> {
        let dt = self.program.timestep_seconds.get();
        let next = self.committed.time.get() + dt;
        if self.committed.boundary == u64::MAX
            || !next.is_finite()
            || next <= self.committed.time.get()
        {
            return Err(RunRejection::Time.into());
        }
        Ok(StepContext {
            workload: self.scope.workload.to_string(),
            run: self.scope.run.to_string(),
            epoch: self.scope.epoch,
            boundary: self.committed.boundary,
            invocation,
            partition: "whole".into(),
            coverage: "whole".into(),
            time_seconds: self.committed.time.get(),
            dt_seconds: dt,
        })
    }
    /// Compute all fields from one boundary, reduce by stable instance order,
    /// integrate once and validate all candidates before atomically replacing it.
    /// An attempt may consume its invocation ID on failure, never simulation time.
    pub fn advance(&mut self, control: OperationControl) -> wasmtime::Result<&CommittedState> {
        self.advance_with_commit(control, |_, _| Ok(()))
    }
    /// Compute and validate a whole step, then ask the embedding publication
    /// authority to accept it. The gate runs exactly once after final validation
    /// and cancellation checks. An error preserves the prior committed state.
    ///
    /// This is a trusted host authority hook, never an observer or guest callback.
    /// The supplied state is still a candidate until the gate accepts it. After
    /// acceptance no fallible check or cancellation may undo that decision; the
    /// private committed state is replaced immediately. A distributed/worker gate
    /// must resolve uncertain publication before allowing further steps.
    pub fn advance_with_commit(
        &mut self,
        control: OperationControl,
        commit: impl FnOnce(&RunScope, &CommittedState) -> wasmtime::Result<()>,
    ) -> wasmtime::Result<&CommittedState> {
        if self.stopped {
            return Err(RunRejection::Stopped.into());
        }
        control.check()?;
        let invocation = self.next_invocation;
        self.next_invocation = invocation.checked_add(1).ok_or(RunRejection::Time)?;
        let step = self.step_context(invocation)?;
        let result = self.candidate(&step, &control);
        let next = result?;
        control.check()?;
        commit(&self.scope, &next)
            .map_err(|error| error.context(self.failure(RunPhase::Publication, None)))?;
        // No observer, callback, guest operation or fallible check after acceptance.
        self.committed = next;
        Ok(&self.committed)
    }
    fn candidate(
        &mut self,
        step: &StepContext,
        control: &OperationControl,
    ) -> wasmtime::Result<CommittedState> {
        let objects = self
            .committed
            .objects
            .scientific_batch::<ObjectState>(bulk(self.limits))?;
        let dynamics = dynamic_projection(&objects, self.limits)?;
        let mut coupled = Vec::new();
        let mut forces = Vec::new();
        let mut fields = Vec::new();
        coupled
            .try_reserve_exact(self.program.fields.len())
            .map_err(|_| RunRejection::Limit)?;
        forces
            .try_reserve_exact(self.program.fields.len())
            .map_err(|_| RunRejection::Limit)?;
        fields
            .try_reserve_exact(self.program.fields.len())
            .map_err(|_| RunRejection::Limit)?;
        for (index, field) in self.program.fields.iter().enumerate() {
            control.check()?;
            let projection = coupling_projection(field, &objects, self.limits)?;
            let count = response_count(&projection, self.limits)?;
            let validation = validation(
                &field.binding,
                &projection,
                self.program.timestep_seconds,
                self.limits,
            )?;
            let result = self
                .sandbox
                .invoke_field_bound(
                    &field.binding.kernel,
                    &field.binding.context,
                    field.binding.configuration.clone(),
                    FieldOperation::Advance {
                        step,
                        prior: self.committed.fields[index].clone(),
                        entities: projection.clone(),
                        validation,
                        field: field.binding.state_extent.borrowed(),
                        forces: packet_extent::<Force>(count, self.limits)?,
                    },
                    control.clone(),
                )
                .map_err(|e| {
                    e.context(self.failure(RunPhase::Field, Some(&field.binding.context.instance)))
                })?;
            let FieldResult::Advanced {
                field,
                forces: force,
            } = result
            else {
                return Err(RunRejection::Definition.into());
            };
            coupled.push(projection);
            forces.push(force);
            fields.push(field);
        }
        let dynamic_batch = dynamics.scientific_batch::<DynamicEntity>(bulk(self.limits))?;
        let views: Vec<_> = self
            .program
            .fields
            .iter()
            .enumerate()
            .map(|(i, field)| {
                Ok(FieldForces {
                    instance: &field.binding.context.instance,
                    coupling_slots: field.binding.context.couplings.len() as u32,
                    coupled: coupled[i]
                        .scientific_batch::<CoupledEntity>(bulk_couplings(self.limits))?,
                    forces: forces[i].scientific_batch::<Force>(bulk(self.limits))?,
                })
            })
            .collect::<wasmtime::Result<_>>()?;
        let attribution = self.failure(RunPhase::Reduction, None);
        let reduced = self
            .reducer
            .reduce(
                &dynamic_batch,
                &self.selected,
                &views,
                ReductionLimits {
                    fields: self.limits.fields,
                    entities: self.limits.objects,
                    field_records: self
                        .limits
                        .couplings
                        .checked_mul(2)
                        .ok_or(RunRejection::Limit)?,
                },
            )
            .map_err(|e| wasmtime::Error::new(e).context(attribution))?;
        let net = packet(reduced, self.limits)?;
        let integrator = &self.program.dynamics;
        let inputs = validation(
            integrator,
            &dynamics,
            self.program.timestep_seconds,
            self.limits,
        )?;
        let result = self
            .sandbox
            .invoke_dynamics_bound(
                &integrator.kernel,
                &integrator.context,
                integrator.configuration.clone(),
                DynamicsOperation::Integrate {
                    step,
                    entities: dynamics.clone(),
                    forces: net.clone(),
                    prior_history: self.committed.history.clone(),
                    validation: inputs,
                    next_entities: packet_extent::<DynamicEntity>(
                        dynamic_batch.len(),
                        self.limits,
                    )?,
                    history: integrator.state_extent.borrowed(),
                },
                control.clone(),
            )
            .map_err(|e| {
                e.context(self.failure(RunPhase::Dynamics, Some(&integrator.context.instance)))
            })?;
        let DynamicsResult::Integrated { entities, history } = result else {
            return Err(RunRejection::Definition.into());
        };
        let next_objects = merge_dynamics(&objects, &entities, self.limits).map_err(|e| {
            e.context(self.failure(RunPhase::Validation, Some(&integrator.context.instance)))
        })?;
        let identities = field_identities(&fields)?;
        let next = CommittedState {
            boundary: step.boundary + 1,
            time: FiniteF64::new(step.time_seconds + step.dt_seconds)?,
            objects: next_objects,
            fields,
            field_identities: identities,
            history,
            forces: Some(net),
        };
        self.validate_state(&next, RunPhase::Validation, control)?;
        Ok(next)
    }
    fn validate_state(
        &self,
        state: &CommittedState,
        phase: RunPhase,
        control: &OperationControl,
    ) -> wasmtime::Result<()> {
        let objects = state
            .objects
            .scientific_batch::<ObjectState>(bulk(self.limits))?;
        for (index, field) in self.program.fields.iter().enumerate() {
            control.check()?;
            if !field.binding.state_extent.matches(&state.fields[index]) {
                return Err(RunRejection::Definition.into());
            }
            let coupled = coupling_projection(field, &objects, self.limits)?;
            let inputs = validation(
                &field.binding,
                &coupled,
                self.program.timestep_seconds,
                self.limits,
            )?;
            self.sandbox
                .invoke_field_bound(
                    &field.binding.kernel,
                    &field.binding.context,
                    field.binding.configuration.clone(),
                    FieldOperation::Validate {
                        state: state.fields[index].clone(),
                        inputs,
                    },
                    control.clone(),
                )
                .map_err(|e| {
                    e.context(self.failure(phase, Some(&field.binding.context.instance)))
                })?;
        }
        let integrator = &self.program.dynamics;
        if !integrator.state_extent.matches(&state.history) {
            return Err(RunRejection::Definition.into());
        }
        let dynamics = dynamic_projection(&objects, self.limits)?;
        self.sandbox
            .invoke_dynamics_bound(
                &integrator.kernel,
                &integrator.context,
                integrator.configuration.clone(),
                DynamicsOperation::Validate {
                    history: state.history.clone(),
                    inputs: validation(
                        integrator,
                        &dynamics,
                        self.program.timestep_seconds,
                        self.limits,
                    )?,
                },
                control.clone(),
            )
            .map_err(|e| e.context(self.failure(phase, Some(&integrator.context.instance))))?;
        Ok(())
    }
    /// Produce every portable part from one committed state, or no checkpoint.
    /// Holding the result cannot mutate the owner; observer/storage quotas are the
    /// embedding adapter's responsibility until the lease/pool integration lands.
    pub fn checkpoint(&self, control: OperationControl) -> wasmtime::Result<RunCheckpoint> {
        let mut state = self.committed.clone();
        for (i, field) in self.program.fields.iter().enumerate() {
            let output = self
                .sandbox
                .invoke_field_bound(
                    &field.binding.kernel,
                    &field.binding.context,
                    field.binding.configuration.clone(),
                    FieldOperation::Checkpoint {
                        state: state.fields[i].clone(),
                        output: field.binding.state_extent.borrowed(),
                    },
                    control.clone(),
                )
                .map_err(|e| {
                    e.context(
                        self.failure(RunPhase::Checkpoint, Some(&field.binding.context.instance)),
                    )
                })?;
            let FieldResult::State(buffer) = output else {
                return Err(RunRejection::Definition.into());
            };
            state.fields[i] = buffer;
        }
        let integrator = &self.program.dynamics;
        let output = self
            .sandbox
            .invoke_dynamics_bound(
                &integrator.kernel,
                &integrator.context,
                integrator.configuration.clone(),
                DynamicsOperation::Checkpoint {
                    history: state.history.clone(),
                    output: integrator.state_extent.borrowed(),
                },
                control.clone(),
            )
            .map_err(|e| {
                e.context(self.failure(RunPhase::Checkpoint, Some(&integrator.context.instance)))
            })?;
        let DynamicsResult::History(history) = output else {
            return Err(RunRejection::Definition.into());
        };
        state.history = history;
        state.field_identities = field_identities(&state.fields)?;
        self.validate_state(&state, RunPhase::Checkpoint, &control)?;
        control.check()?;
        Ok(RunCheckpoint {
            scope: self.scope.clone(),
            definition: self.definition.clone(),
            state,
            next_invocation: self.next_invocation,
        })
    }
    /// Resume complete portable state into a fresh engine without initialization.
    /// Exact program and run identity must match; changing a model is authoring,
    /// not silently relabeling history. Durable checkpoint parsing is separate.
    pub fn restore(
        sandbox: Arc<Sandbox>,
        scope: RunScope,
        program: RunProgram,
        checkpoint: &RunCheckpoint,
        limits: RunLimits,
        control: OperationControl,
    ) -> wasmtime::Result<Self> {
        let captured = CapturedRunState {
            objects: checkpoint.state.objects.clone(),
            fields: checkpoint.state.fields.clone(),
            history: checkpoint.state.history.clone(),
        };
        let definition = check_program(&program, &captured, limits, false)?;
        if checkpoint.scope != scope || checkpoint.definition.as_ref() != &definition {
            return Err(RunRejection::Checkpoint.into());
        }
        let selected = program
            .fields
            .iter()
            .map(|f| f.binding.context.instance.clone())
            .collect();
        let mut run = Self {
            sandbox,
            scope,
            program,
            definition: checkpoint.definition.clone(),
            limits,
            committed: checkpoint.state.clone(),
            next_invocation: checkpoint.next_invocation,
            stopped: false,
            reducer: ForceReducer::default(),
            selected,
            snapshots: Arc::new(SnapshotBudget::new(limits.snapshots, limits.snapshot_bytes)),
        };
        let objects = run
            .committed
            .objects
            .scientific_batch::<ObjectState>(bulk(limits))?;
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(run.program.fields.len())
            .map_err(|_| RunRejection::Limit)?;
        for (i, field) in run.program.fields.iter().enumerate() {
            let coupled = coupling_projection(field, &objects, limits)?;
            let output = run
                .sandbox
                .invoke_field_bound(
                    &field.binding.kernel,
                    &field.binding.context,
                    field.binding.configuration.clone(),
                    FieldOperation::Restore {
                        checkpoint: run.committed.fields[i].clone(),
                        validation: validation(
                            &field.binding,
                            &coupled,
                            run.program.timestep_seconds,
                            limits,
                        )?,
                        output: field.binding.state_extent.borrowed(),
                    },
                    control.clone(),
                )
                .map_err(|e| {
                    e.context(run.failure(RunPhase::Restore, Some(&field.binding.context.instance)))
                })?;
            let FieldResult::State(buffer) = output else {
                return Err(RunRejection::Definition.into());
            };
            fields.push(buffer);
        }
        let integrator = &run.program.dynamics;
        let dynamics = dynamic_projection(&objects, limits)?;
        let output = run
            .sandbox
            .invoke_dynamics_bound(
                &integrator.kernel,
                &integrator.context,
                integrator.configuration.clone(),
                DynamicsOperation::Restore {
                    checkpoint: run.committed.history.clone(),
                    validation: validation(
                        integrator,
                        &dynamics,
                        run.program.timestep_seconds,
                        limits,
                    )?,
                    output: integrator.state_extent.borrowed(),
                },
                control.clone(),
            )
            .map_err(|e| {
                e.context(run.failure(RunPhase::Restore, Some(&integrator.context.instance)))
            })?;
        let DynamicsResult::History(history) = output else {
            return Err(RunRejection::Definition.into());
        };
        run.committed.fields = fields;
        run.committed.field_identities = field_identities(&run.committed.fields)?;
        run.committed.history = history;
        run.validate_state(&run.committed, RunPhase::Restore, &control)?;
        control.check()?;
        Ok(run)
    }
}

fn bulk(limits: RunLimits) -> BulkLimits {
    BulkLimits {
        bytes: limits.scientific_bytes,
        records: limits.objects,
    }
}
fn field_identities(fields: &[Buffer]) -> wasmtime::Result<Vec<InputIdentity>> {
    let mut identities = Vec::new();
    identities
        .try_reserve_exact(fields.len())
        .map_err(|_| RunRejection::Limit)?;
    for field in fields {
        identities.push(InputIdentity::of(
            field.schema.parse()?,
            field.value_count,
            &field.bytes,
        ));
    }
    Ok(identities)
}
fn bulk_couplings(limits: RunLimits) -> BulkLimits {
    BulkLimits {
        bytes: limits.scientific_bytes,
        records: limits.couplings,
    }
}
fn packet_extent<R: BulkRecord>(
    count: usize,
    limits: RunLimits,
) -> wasmtime::Result<OutputExtent<'static>> {
    Ok(OutputExtent {
        schema: R::SCHEMA,
        bytes: packet_bytes::<R>(count, bulk(limits))?,
        values: count as u64,
    })
}
fn packet<R: BulkRecord>(records: &[R], limits: RunLimits) -> wasmtime::Result<Buffer> {
    let mut bytes = vec![];
    let bounds = if R::KIND == CoupledEntity::KIND {
        bulk_couplings(limits)
    } else {
        bulk(limits)
    };
    encode_batch(records, &mut bytes, bounds)?;
    Ok(Buffer {
        schema: R::SCHEMA.into(),
        value_count: records.len() as u64,
        bytes: bytes.into(),
    })
}
fn dynamic_projection(
    objects: &Batch<'_, ObjectState>,
    limits: RunLimits,
) -> wasmtime::Result<Buffer> {
    let mut records = Vec::new();
    records
        .try_reserve_exact(objects.len())
        .map_err(|_| RunRejection::Limit)?;
    records.extend(objects.iter().filter_map(|o| o.dynamic()));
    packet(&records, limits)
}
fn find(objects: &Batch<'_, ObjectState>, id: EntityId) -> Option<ObjectState> {
    let (mut lo, mut hi) = (0, objects.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let value = objects.get(mid)?;
        match value.id.cmp(&id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Some(value),
        }
    }
    None
}
fn coupling_projection(
    field: &RunField,
    objects: &Batch<'_, ObjectState>,
    limits: RunLimits,
) -> wasmtime::Result<Buffer> {
    let batch = field
        .coupled
        .scientific_batch::<CoupledEntity>(bulk_couplings(limits))?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(batch.len())
        .map_err(|_| RunRejection::Limit)?;
    for mut coupled in batch.iter() {
        let object = find(objects, coupled.id).ok_or(RunRejection::Objects)?;
        let slot = field
            .binding
            .context
            .couplings
            .get(coupled.slot.0 as usize)
            .ok_or(RunRejection::Objects)?;
        if coupled.has_dynamics != object.inertial_mass_kilograms.is_some()
            || (coupled.source_si.is_some() && slot.source.is_none())
            || (coupled.response_si.is_some() && slot.response.is_none())
        {
            return Err(RunRejection::Objects.into());
        }
        coupled.kinematics = object.kinematics;
        records.push(coupled);
    }
    packet(&records, limits)
}
fn response_count(buffer: &Buffer, limits: RunLimits) -> wasmtime::Result<usize> {
    let mut last = None;
    let mut count = 0;
    for c in buffer
        .scientific_batch::<CoupledEntity>(bulk_couplings(limits))?
        .iter()
        .filter(CoupledEntity::responds_dynamically)
    {
        if last != Some(c.id) {
            last = Some(c.id);
            count += 1;
        }
    }
    Ok(count)
}
fn validation(
    kernel: &RunKernel,
    entities: &Buffer,
    dt: FiniteF64,
    limits: RunLimits,
) -> wasmtime::Result<Buffer> {
    let context = kernel.context.to_cbor()?;
    let mut bytes = vec![];
    ValidationInputs {
        timestep_seconds: dt,
        context: &context,
        domain: &kernel.domain.bytes,
        configuration: &kernel.configuration.bytes,
        entities: &entities.bytes,
    }
    .encode(
        &mut bytes,
        &kernel.context,
        if kernel.context.execution_contract == ExecutionContractId::Field {
            bulk_couplings(limits)
        } else {
            bulk(limits)
        },
    )?;
    Ok(Buffer {
        schema: VALIDATION_SCHEMA.into(),
        value_count: 1,
        bytes: bytes.into(),
    })
}
fn merge_dynamics(
    objects: &Batch<'_, ObjectState>,
    next: &Buffer,
    limits: RunLimits,
) -> wasmtime::Result<Buffer> {
    let dynamics = next.scientific_batch::<DynamicEntity>(bulk(limits))?;
    let mut dynamic = dynamics.iter();
    let mut records = Vec::new();
    records
        .try_reserve_exact(objects.len())
        .map_err(|_| RunRejection::Limit)?;
    for mut object in objects.iter() {
        if let Some(prior) = object.dynamic() {
            let next = dynamic.next().ok_or(RunRejection::Objects)?;
            if prior.id != next.id || prior.inertial_mass_kilograms != next.inertial_mass_kilograms
            {
                return Err(RunRejection::Objects.into());
            }
            object.kinematics = next.kinematics;
        }
        records.push(object);
    }
    if dynamic.next().is_some() {
        return Err(RunRejection::Objects.into());
    }
    packet(&records, limits)
}
fn add(total: &mut usize, bytes: usize, limit: usize) -> wasmtime::Result<()> {
    *total = total
        .checked_add(bytes)
        .filter(|n| *n <= limit)
        .ok_or(RunRejection::Limit)?;
    Ok(())
}
fn check_program(
    program: &RunProgram,
    captured: &CapturedRunState,
    limits: RunLimits,
    check_initial_kinematics: bool,
) -> wasmtime::Result<Definition> {
    if program.fields.len() > limits.fields {
        return Err(RunRejection::Limit.into());
    }
    if program.fields.len() != captured.fields.len()
        || !program.timestep_seconds.is_positive()
        || !program
            .fields
            .windows(2)
            .all(|f| f[0].binding.context.instance < f[1].binding.context.instance)
    {
        return Err(RunRejection::Definition.into());
    }
    let objects = captured
        .objects
        .scientific_batch::<ObjectState>(bulk(limits))?;
    let mut inputs = 0;
    let mut scientific = 0;
    let mut couplings = 0;
    let mut contexts = vec![];
    let mut extents = vec![];
    let mut pins = vec![];
    // Bound retained input/candidate scientific packets before guest execution.
    // Temporary typed projection vectors, validation copies, Wasm/JIT and external
    // checkpoint holders still need the aggregate arena/store budget; these packet
    // counts must not be advertised as a complete resident-memory ceiling.
    add(
        &mut scientific,
        captured
            .objects
            .bytes
            .len()
            .checked_mul(3)
            .ok_or(RunRejection::Limit)?,
        limits.scientific_bytes,
    )?;
    let dynamics_count = objects
        .iter()
        .filter(|o| o.inertial_mass_kilograms.is_some())
        .count();
    add(
        &mut scientific,
        packet_bytes::<DynamicEntity>(dynamics_count, bulk(limits))?
            .checked_mul(3)
            .ok_or(RunRejection::Limit)?,
        limits.scientific_bytes,
    )?;
    add(
        &mut scientific,
        packet_bytes::<Force>(dynamics_count, bulk(limits))?
            .checked_mul(2)
            .ok_or(RunRejection::Limit)?,
        limits.scientific_bytes,
    )?;
    for (i, binding) in program
        .fields
        .iter()
        .map(|f| &f.binding)
        .chain([&program.dynamics])
        .enumerate()
    {
        let is_field = i < program.fields.len();
        let expected = if is_field {
            ExecutionContractId::Field
        } else {
            ExecutionContractId::Dynamics
        };
        let state = if is_field {
            &captured.fields[i]
        } else {
            &captured.history
        };
        // Reject oversized cold inputs before hashing their contents.
        add(
            &mut inputs,
            binding.configuration.bytes.len(),
            limits.input_bytes,
        )?;
        add(&mut inputs, binding.domain.bytes.len(), limits.input_bytes)?;
        if binding.context.execution_contract != expected
            || binding.kernel.contract() != expected
            || binding.kernel.artifact() != binding.context.kernel
            || !binding.context.configuration.matches(
                &binding.configuration.schema,
                binding.configuration.value_count,
                &binding.configuration.bytes,
            )
            || !binding.context.domain.matches(
                &binding.domain.schema,
                binding.domain.value_count,
                &binding.domain.bytes,
            )
            || !binding.state_extent.matches(state)
            || binding.state_extent.bytes as u64 > binding.context.bounds.state_bytes
            || binding.state_extent.schema.as_str()
                != format!(
                    "{}/v{}",
                    binding.context.state_format.id, binding.context.state_format.version
                )
            || (is_field && binding.context.instance == program.dynamics.context.instance)
        {
            return Err(RunRejection::Definition.into());
        }
        let context = binding.context.to_cbor()?;
        add(&mut inputs, context.len(), limits.input_bytes)?;
        add(
            &mut scientific,
            binding
                .state_extent
                .bytes
                .checked_mul(3)
                .ok_or(RunRejection::Limit)?,
            limits.scientific_bytes,
        )?;
        if is_field {
            let field = &program.fields[i];
            let batch = field
                .coupled
                .scientific_batch::<CoupledEntity>(bulk_couplings(limits))?;
            add(&mut couplings, batch.len(), limits.couplings)?;
            if batch.len() > binding.context.bounds.projection_records as usize {
                return Err(RunRejection::Limit.into());
            }
            for c in batch.iter() {
                let object = find(&objects, c.id).ok_or(RunRejection::Objects)?;
                if (check_initial_kinematics && c.kinematics != object.kinematics)
                    || c.has_dynamics != object.inertial_mass_kilograms.is_some()
                {
                    return Err(RunRejection::Objects.into());
                }
                let slot = binding
                    .context
                    .couplings
                    .get(c.slot.0 as usize)
                    .ok_or(RunRejection::Objects)?;
                if (c.source_si.is_some() && slot.source.is_none())
                    || (c.response_si.is_some() && slot.response.is_none())
                {
                    return Err(RunRejection::Objects.into());
                }
            }
            add(
                &mut scientific,
                field
                    .coupled
                    .bytes
                    .len()
                    .checked_mul(3)
                    .ok_or(RunRejection::Limit)?,
                limits.scientific_bytes,
            )?;
            add(
                &mut scientific,
                packet_bytes::<Force>(response_count(&field.coupled, limits)?, bulk(limits))?,
                limits.scientific_bytes,
            )?;
            pins.push(InputIdentity::of(
                field.coupled.schema.parse()?,
                field.coupled.value_count,
                &field.coupled.bytes,
            ));
        } else if dynamics_count > binding.context.bounds.projection_records as usize {
            return Err(RunRejection::Limit.into());
        }
        contexts.push(binding.context.clone());
        extents.push(binding.state_extent.clone());
    }
    Ok(Definition {
        contexts,
        couplings: pins,
        extents,
        dt: program.timestep_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merge_rejects_missing_extra_static_or_mass_rewriting_integrator_output() {
        let limits = RunLimits {
            couplings: 0,
            ..RunLimits::default()
        };
        let objects = [
            ObjectState {
                id: EntityId(1),
                kinematics: Kinematics {
                    position_metres: [FiniteF64::ZERO; 3],
                    velocity_metres_per_second: [FiniteF64::ZERO; 3],
                },
                inertial_mass_kilograms: None,
            },
            ObjectState {
                id: EntityId(2),
                kinematics: Kinematics {
                    position_metres: [FiniteF64::ZERO; 3],
                    velocity_metres_per_second: [FiniteF64::ZERO; 3],
                },
                inertial_mass_kilograms: Some(FiniteF64::new(4.0).unwrap()),
            },
        ];
        let original = packet(&objects, limits).unwrap();
        let batch = original
            .scientific_batch::<ObjectState>(bulk(limits))
            .unwrap();
        let expected = dynamic_projection(&batch, limits).unwrap();
        assert_eq!(
            merge_dynamics(&batch, &expected, limits).unwrap().bytes,
            original.bytes
        );
        let dynamic = objects[1].dynamic().unwrap();
        for values in [
            vec![],
            vec![DynamicEntity {
                id: EntityId(1),
                ..dynamic
            }],
            vec![
                DynamicEntity {
                    id: EntityId(1),
                    ..dynamic
                },
                dynamic,
            ],
            vec![DynamicEntity {
                inertial_mass_kilograms: FiniteF64::new(5.0).unwrap(),
                ..dynamic
            }],
        ] {
            let corrupt = packet(&values, limits).unwrap();
            assert!(merge_dynamics(&batch, &corrupt, limits).is_err());
        }
        assert_eq!(batch.get(0).unwrap(), objects[0]);
    }
}
