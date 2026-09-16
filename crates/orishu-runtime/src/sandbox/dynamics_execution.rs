use super::*;
use common::{accepted, advice, check_step};
use field::orishu::simulation::types::Admissibility;

/// One isolated integrator lifecycle operation. Histories are explicit portable
/// state, never a guest-local session token or an observer's trajectory cache.
pub enum DynamicsOperation<'a> {
    /// Initialize opaque history without assuming its plugin-owned layout.
    InitializeHistoryBounded {
        /// Exact initial Dynamics projection.
        entities: Buffer,
        /// Exact portable schema and independent byte/value ceilings.
        history: OutputCapacity<'a>,
    },
    /// Construct the selected integrator's initial history from initial entities.
    InitializeHistory {
        /// Initial entity projection in the declared dynamics schema.
        entities: Buffer,
        /// Exact bounded history output layout.
        history: OutputExtent<'a>,
    },
    /// Validate history and complete experiment projections before admission.
    Validate {
        /// Captured history, including explicitly declared empty history.
        history: Buffer,
        /// Proposed timestep, profile, entities and other declared projections.
        inputs: Buffer,
    },
    /// Integrate exactly once using the host's deterministic reduced force batch.
    Integrate {
        /// Run supervisor's identified committed boundary and authored fixed dt.
        step: &'a StepContext,
        /// Committed dynamic entities; no candidate from another field stage.
        entities: Buffer,
        /// Complete stable-entity force batch, including explicit zeros.
        forces: Buffer,
        /// All required numerical history for the selected integrator.
        prior_history: Buffer,
        /// Numerical admissibility projections for this operation.
        validation: Buffer,
        /// Exact candidate entity shape.
        next_entities: OutputExtent<'a>,
        /// Exact candidate history shape.
        history: OutputExtent<'a>,
    },
    /// Apply admitted membership changes to history at the new boundary. The run
    /// coordinator, not this operation, controls when newborns join force evaluation.
    TransitionEntities {
        /// Identified boundary transition supplied by the supervisor.
        step: &'a StepContext,
        /// Newborn identities and their authored/resolved initial state.
        births: Buffer,
        /// Explicit retired entity identities.
        deaths: Buffer,
        /// History before the membership transition.
        prior_history: Buffer,
        /// Full declared validation projections including new membership.
        validation: Buffer,
        /// Expected history for the resulting membership.
        history: OutputExtent<'a>,
    },
    /// Export portable history checkpoint state without reconstructing defaults.
    Checkpoint {
        /// Current committed numerical history.
        history: Buffer,
        /// Exact checkpoint output layout.
        output: OutputExtent<'a>,
    },
    /// Restore exact compatible history, validate it and export restored state.
    Restore {
        /// Complete captured history checkpoint.
        checkpoint: Buffer,
        /// Full restart validation inputs.
        validation: Buffer,
        /// Expected restored checkpoint layout.
        output: OutputExtent<'a>,
    },
}

/// Complete integrator output. Scientific validation and atomic field/entity/
/// history commit still belong to the run coordinator.
#[derive(Debug)]
pub enum DynamicsResult {
    /// Explicit initialized, transitioned, checkpointed or restored history.
    History(Buffer),
    /// Accepted numerical validation; advice cannot change the authored timestep.
    Admissible(Option<NumericalAdvice>),
    /// Both integration outputs completed; neither is published independently.
    Integrated {
        /// Candidate dynamic entity state.
        entities: Buffer,
        /// Candidate integrator-private numerical history.
        history: Buffer,
    },
}

struct Invocation {
    store: Store<HostState>,
    guest: dynamics::Dynamics,
    session: u64,
}
impl Invocation {
    fn begin(&mut self) -> wasmtime::Result<()> {
        self.store.data_mut().begin()?;
        Ok(())
    }
    fn input(&mut self, buffer: Buffer) -> wasmtime::Result<Resource<crate::InputGrant>> {
        let handle = self.store.data_mut().input(buffer)?;
        Ok(Resource::new_borrow(handle.rep()))
    }
    fn output(
        &mut self,
        extent: OutputExtent<'_>,
    ) -> wasmtime::Result<Resource<crate::OutputGrant>> {
        let handle = self
            .store
            .data_mut()
            .output(extent.schema, extent.bytes, extent.values)?;
        Ok(Resource::new_borrow(handle.rep()))
    }
    fn take(&mut self, rep: u32) -> wasmtime::Result<Buffer> {
        self.store.data_mut().take_output(Resource::new_borrow(rep))
    }
    fn load(&mut self, history: Buffer, restore: bool) -> wasmtime::Result<()> {
        self.begin()?;
        let history = self.input(history)?;
        let common = self.guest.orishu_simulation_common();
        accepted(if restore {
            common.call_restore(&mut self.store, self.session, history)?
        } else {
            common.call_load(&mut self.store, self.session, history)?
        })
    }
    fn validate(&mut self, inputs: Buffer) -> wasmtime::Result<Option<NumericalAdvice>> {
        self.begin()?;
        let inputs = self.input(inputs)?;
        match self.guest.orishu_simulation_common().call_validate(
            &mut self.store,
            self.session,
            inputs,
        )? {
            Admissibility::Admissible(value) => advice(value),
            Admissibility::Rejected(reason) => Err(KernelRejection::from(reason).into()),
        }
    }
    fn checkpoint(
        &mut self,
        state: Buffer,
        output: OutputExtent<'_>,
    ) -> wasmtime::Result<DynamicsResult> {
        self.begin()?;
        let state = self.input(state)?;
        let output = self.output(output)?;
        let rep = output.rep();
        accepted(self.guest.orishu_simulation_common().call_checkpoint(
            &mut self.store,
            self.session,
            state,
            output,
        )?)?;
        Ok(DynamicsResult::History(self.take(rep)?))
    }
    fn execute(&mut self, operation: DynamicsOperation<'_>) -> wasmtime::Result<DynamicsResult> {
        match operation {
            DynamicsOperation::InitializeHistoryBounded { entities, history } => {
                self.begin()?;
                let entities = self.input(entities)?;
                let history = self.store.data_mut().output_bounded(
                    history.schema,
                    history.bytes,
                    history.values,
                )?;
                let rep = history.rep();
                accepted(
                    self.guest
                        .orishu_simulation_dynamics_kernel()
                        .call_initialize_history(
                            &mut self.store,
                            self.session,
                            entities,
                            Resource::new_borrow(rep),
                        )?,
                )?;
                Ok(DynamicsResult::History(self.take(rep)?))
            }
            DynamicsOperation::InitializeHistory { entities, history } => {
                self.begin()?;
                let entities = self.input(entities)?;
                let history = self.output(history)?;
                let rep = history.rep();
                accepted(
                    self.guest
                        .orishu_simulation_dynamics_kernel()
                        .call_initialize_history(
                            &mut self.store,
                            self.session,
                            entities,
                            history,
                        )?,
                )?;
                Ok(DynamicsResult::History(self.take(rep)?))
            }
            DynamicsOperation::Validate { history, inputs } => {
                self.load(history, false)?;
                Ok(DynamicsResult::Admissible(self.validate(inputs)?))
            }
            DynamicsOperation::Integrate {
                step,
                entities,
                forces,
                prior_history,
                validation,
                next_entities,
                history,
            } => {
                self.load(prior_history.clone(), false)?;
                self.validate(validation)?;
                self.begin()?;
                let entities = self.input(entities)?;
                let forces = self.input(forces)?;
                let prior_history = self.input(prior_history)?;
                let next_entities = self.output(next_entities)?;
                let history = self.output(history)?;
                let (entity_rep, history_rep) = (next_entities.rep(), history.rep());
                accepted(
                    self.guest
                        .orishu_simulation_dynamics_kernel()
                        .call_integrate(
                            &mut self.store,
                            self.session,
                            step,
                            entities,
                            forces,
                            prior_history,
                            next_entities,
                            history,
                        )?,
                )?;
                Ok(DynamicsResult::Integrated {
                    entities: self.take(entity_rep)?,
                    history: self.take(history_rep)?,
                })
            }
            DynamicsOperation::TransitionEntities {
                step,
                births,
                deaths,
                prior_history,
                validation,
                history,
            } => {
                self.load(prior_history.clone(), false)?;
                self.validate(validation)?;
                self.begin()?;
                let births = self.input(births)?;
                let deaths = self.input(deaths)?;
                let prior_history = self.input(prior_history)?;
                let history = self.output(history)?;
                let rep = history.rep();
                accepted(
                    self.guest
                        .orishu_simulation_dynamics_kernel()
                        .call_transition_entities(
                            &mut self.store,
                            self.session,
                            step,
                            births,
                            deaths,
                            prior_history,
                            history,
                        )?,
                )?;
                Ok(DynamicsResult::History(self.take(rep)?))
            }
            DynamicsOperation::Checkpoint { history, output } => {
                self.load(history.clone(), false)?;
                self.checkpoint(history, output)
            }
            DynamicsOperation::Restore {
                checkpoint,
                validation,
                output,
            } => {
                self.load(checkpoint.clone(), true)?;
                self.validate(validation)?;
                self.checkpoint(checkpoint, output)
            }
        }
    }
}

impl Sandbox {
    /// Invoke the independently compiled Dynamics world in a disposable metered
    /// guest. The engine/profile/policies are shared with fields; a Dynamics kernel
    /// receives no field handles or extra evaluation hooks. No returned candidate
    /// commits state. This bootstrap path is not yet a reusable hot-step executor.
    pub fn invoke_dynamics(
        &self,
        kernel: &CompiledKernel,
        context: Buffer,
        config: Buffer,
        operation: DynamicsOperation<'_>,
        control: OperationControl,
    ) -> wasmtime::Result<DynamicsResult> {
        let control = control.bounded(self.limits.operation_timeout)?;
        control.check()?;
        if kernel.contract != ExecutionContractId::Dynamics {
            return Err(error("dynamics operation requires a dynamics kernel"));
        }
        if !Engine::same(&self.engine, kernel.component.engine()) {
            return Err(error("compiled kernel belongs to another sandbox"));
        }
        if let DynamicsOperation::Integrate { step, .. }
        | DynamicsOperation::TransitionEntities { step, .. } = &operation
        {
            check_step(step)?;
        }
        let mut store = self.store(control.clone())?;
        let guest =
            dynamics::Dynamics::instantiate(&mut store, &kernel.component, &self.linker()?)?;
        store.data_mut().begin()?;
        let ctx = store.data_mut().input(context)?;
        let cfg = store.data_mut().input(config)?;
        let session = accepted(guest.orishu_simulation_common().call_setup(
            &mut store,
            Resource::new_borrow(ctx.rep()),
            Resource::new_borrow(cfg.rep()),
        )?)?;
        let mut invocation = Invocation {
            store,
            guest,
            session,
        };
        let result = invocation.execute(operation)?;
        invocation.store.data_mut().revoke()?;
        accepted(
            invocation
                .guest
                .orishu_simulation_common()
                .call_close(&mut invocation.store, session)?,
        )?;
        control.check()?;
        Ok(result)
    }
}
