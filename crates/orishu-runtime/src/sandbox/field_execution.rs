use super::*;
use common::{accepted, advice, check_step};
use field::orishu::simulation::types::Admissibility;

/// One disposable field operation with host-reserved exact extents or explicit
/// initialization ceilings.
/// Every state input is explicit. In particular, sampling cannot access a live
/// advancing guest, and loading/restoring never invokes natural initialization.
pub enum FieldOperation<'a> {
    /// Produce natural defaults with kernel-chosen size within host ceilings.
    InitializeBounded {
        /// Resolved domain/discretization, with no entity projection.
        domain: Buffer,
        /// Exact schema and independent byte/value ceilings, not a field layout.
        output: OutputCapacity<'a>,
    },
    /// Produce natural defaults without entities or prior state.
    Initialize {
        /// Resolved domain/discretization in its declared portable schema.
        domain: Buffer,
        /// Expected state layout.
        output: OutputExtent<'a>,
    },
    /// Load supplied state and ask the selected kernel about numerical admissibility.
    Validate {
        /// Captured initial or committed state, not regenerated defaults.
        state: Buffer,
        /// Complete declared validation projections, including authored timestep.
        inputs: Buffer,
    },
    /// Load prior state, validate relevant inputs, and compute two isolated outputs.
    Advance {
        /// Identified fixed-step boundary and SI time.
        step: &'a StepContext,
        /// This field's immutable prior state only.
        prior: Buffer,
        /// Coupled entity projection from the same committed boundary.
        entities: Buffer,
        /// Numerical validation inputs for this operation.
        validation: Buffer,
        /// Exact candidate field-state shape.
        field: OutputExtent<'a>,
        /// Exact complete force batch shape, including explicit zeros.
        forces: OutputExtent<'a>,
    },
    /// Query an immutable snapshot using a separate disposable guest instance.
    Sample {
        /// Leased committed field state; never a candidate or mutable guest memory.
        snapshot: Buffer,
        /// Observer-generated bounded points/channels in the admitted query schema.
        query: Buffer,
        /// Exact values/validity/quality output layout.
        output: OutputExtent<'a>,
    },
    /// Load explicit state and export portable checkpoint bytes.
    Checkpoint {
        /// Committed state to checkpoint.
        state: Buffer,
        /// Expected portable checkpoint shape.
        output: OutputExtent<'a>,
    },
    /// Restore a checkpoint without initializing, validate, and re-export its state.
    Restore {
        /// Complete compatible checkpoint bytes.
        checkpoint: Buffer,
        /// Full admission/restart validation inputs.
        validation: Buffer,
        /// Expected restored checkpoint shape.
        output: OutputExtent<'a>,
    },
}

/// Complete operation result, still subject to scientific admission/validation.
/// Neither finishing a buffer nor returning this value commits a run boundary.
#[derive(Debug)]
pub enum FieldResult {
    /// Initialized, checkpointed or restored state (operation determines schema).
    State(Buffer),
    /// Kernel accepted validation. Host checks advice for positive finite values.
    Admissible(Option<NumericalAdvice>),
    /// Both required outputs are complete; neither is published independently.
    Advanced {
        /// Candidate field state.
        field: Buffer,
        /// Candidate force batch.
        forces: Buffer,
    },
    /// Isolated sampling result, not restorable scientific state.
    Samples(Buffer),
}

struct Invocation {
    store: Store<HostState>,
    guest: field::Field,
    session: u64,
}
impl Invocation {
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
    fn begin(&mut self) -> wasmtime::Result<()> {
        self.store.data_mut().begin()?;
        Ok(())
    }
    fn load(&mut self, state: Buffer, restore: bool) -> wasmtime::Result<()> {
        self.begin()?;
        let state = self.input(state)?;
        let common = self.guest.orishu_simulation_common();
        accepted(if restore {
            common.call_restore(&mut self.store, self.session, state)?
        } else {
            common.call_load(&mut self.store, self.session, state)?
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
    ) -> wasmtime::Result<FieldResult> {
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
        Ok(FieldResult::State(self.take(rep)?))
    }
    fn execute(&mut self, operation: FieldOperation<'_>) -> wasmtime::Result<FieldResult> {
        match operation {
            FieldOperation::InitializeBounded { domain, output } => {
                self.begin()?;
                let domain = self.input(domain)?;
                let output = self.store.data_mut().output_bounded(
                    output.schema,
                    output.bytes,
                    output.values,
                )?;
                let rep = output.rep();
                accepted(
                    self.guest
                        .orishu_simulation_field_kernel()
                        .call_initialize(
                            &mut self.store,
                            self.session,
                            domain,
                            Resource::new_borrow(rep),
                        )?,
                )?;
                Ok(FieldResult::State(self.take(rep)?))
            }
            FieldOperation::Initialize { domain, output } => {
                self.begin()?;
                let domain = self.input(domain)?;
                let output = self.output(output)?;
                let rep = output.rep();
                accepted(
                    self.guest
                        .orishu_simulation_field_kernel()
                        .call_initialize(&mut self.store, self.session, domain, output)?,
                )?;
                Ok(FieldResult::State(self.take(rep)?))
            }
            FieldOperation::Validate { state, inputs } => {
                self.load(state, false)?;
                Ok(FieldResult::Admissible(self.validate(inputs)?))
            }
            FieldOperation::Advance {
                step,
                prior,
                entities,
                validation,
                field,
                forces,
            } => {
                self.load(prior.clone(), false)?;
                self.validate(validation)?;
                // The run supervisor supplies the externally identified invocation;
                // the store's separate resource-grant generation is local only.
                self.begin()?;
                let prior = self.input(prior)?;
                let entities = self.input(entities)?;
                let field = self.output(field)?;
                let forces = self.output(forces)?;
                let (field_rep, force_rep) = (field.rep(), forces.rep());
                accepted(self.guest.orishu_simulation_field_kernel().call_advance(
                    &mut self.store,
                    self.session,
                    step,
                    prior,
                    entities,
                    field,
                    forces,
                )?)?;
                Ok(FieldResult::Advanced {
                    field: self.take(field_rep)?,
                    forces: self.take(force_rep)?,
                })
            }
            FieldOperation::Sample {
                snapshot,
                query,
                output,
            } => {
                self.load(snapshot.clone(), false)?;
                self.begin()?;
                let snapshot = self.input(snapshot)?;
                let query = self.input(query)?;
                let output = self.output(output)?;
                let rep = output.rep();
                accepted(self.guest.orishu_simulation_field_kernel().call_sample(
                    &mut self.store,
                    self.session,
                    query,
                    snapshot,
                    output,
                )?)?;
                Ok(FieldResult::Samples(self.take(rep)?))
            }
            FieldOperation::Checkpoint { state, output } => {
                self.load(state.clone(), false)?;
                self.checkpoint(state, output)
            }
            FieldOperation::Restore {
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
    /// Execute one field lifecycle operation in an isolated disposable store.
    /// Fuel and wall time cover instantiate/start, setup, load, validation, the
    /// requested operation and close. Any failure discards all candidates/store.
    /// This bootstrap path allocates per operation; it is not the hot-step run owner.
    pub fn invoke_field(
        &self,
        kernel: &CompiledKernel,
        context: Buffer,
        config: Buffer,
        operation: FieldOperation<'_>,
        control: OperationControl,
    ) -> wasmtime::Result<FieldResult> {
        let control = control.bounded(self.limits.operation_timeout)?;
        control.check()?;
        if kernel.contract != ExecutionContractId::Field {
            return Err(error("field operation requires a field kernel"));
        }
        if !Engine::same(&self.engine, kernel.component.engine()) {
            return Err(error("compiled kernel belongs to another sandbox"));
        }
        if let FieldOperation::Advance { step, .. } = &operation {
            check_step(step)?;
        }
        let mut store = self.store(control.clone())?;
        let guest = field::Field::instantiate(&mut store, &kernel.component, &self.linker()?)?;
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
