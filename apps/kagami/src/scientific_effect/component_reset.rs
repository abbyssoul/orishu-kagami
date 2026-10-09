//! Explicit whole-state reset accompanying component/provider replacement.
use super::*;
use orishu_plugin::authoring_lock::SelectionLock;

/// Complete proposed replacement, not an automatic migration recipe. Values,
/// provider choices, models, domain and configuration are all explicit inputs.
#[derive(Clone, Debug)]
pub struct ComponentResetRequest {
    /// Ordinary scene edits, including explicit detach/attach/new values. No
    /// implicit copying/conversion, variable edits or embedded setup/lock commands.
    pub edits: Vec<ExperimentCommand>,
    /// Complete resulting component intent, not a patch or a default preference.
    pub dependencies: Arc<SelectionLock>,
    /// Complete requested physics; every field and history is initialized afresh.
    pub setup: SetupRequest,
}

impl ScientificEffects {
    /// Prepare explicit component changes and a full scientific reset atomically.
    /// Callers must obtain consent to losing all previous field/history state.
    /// No old executable is needed or old blob passed to a new kernel. This is
    /// deliberately separate from ordinary Add, which preserves existing fields.
    pub fn reset_components(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        request: ComponentResetRequest,
    ) -> Result<(), EffectError> {
        if self.pending.is_some() {
            return Err(EffectError::Busy);
        }
        if !document.accepts_effect(guard) {
            return Err(EffectError::Document);
        }
        let plugins = self.plugins.clone().ok_or(EffectError::Unavailable)?;
        let limits = *document.limits();
        if request.edits.len().saturating_add(2) > limits.max_commands_per_batch {
            return Err(EffectError::Limit);
        }
        if request
            .edits
            .iter()
            .any(|c| matches!(c, ExperimentCommand::AdoptDependencies(_)))
        {
            return Err(EffectError::Unavailable);
        }
        scene_inputs::check(&request.edits, &limits)?;
        request
            .dependencies
            .validate(limits.dependencies)
            .map_err(|_| EffectError::Limit)?;
        let configuration_bytes = request.setup.check(&plugins, &limits)?;
        let mut retained = ScientificRetention::new(INPUT_BYTES - configuration_bytes);
        let old = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or(EffectError::Unavailable)?;
        retained.include(old).map_err(|_| EffectError::Limit)?;
        if let Some(lock) = document.snapshot().dependencies() {
            retained
                .include_dependencies(lock)
                .map_err(|_| EffectError::Limit)?;
        }
        retained
            .include_dependencies(&request.dependencies)
            .map_err(|_| EffectError::Limit)?;
        let experiment = document.experiment().clone();
        let schemas = document.schemas().clone();
        let control =
            OperationControl::new(Duration::from_secs(60)).map_err(|_| EffectError::Executor)?;
        let worker_control = control.clone();
        let task = std::thread::Builder::new()
            .name("kagami-component-reset".into())
            .spawn(move || {
                reset(
                    plugins,
                    experiment,
                    schemas,
                    request,
                    limits,
                    worker_control,
                )
            })
            .map_err(|_| EffectError::Executor)?;
        self.pending = Some(Pending {
            guard,
            control,
            task: Some(task),
        });
        Ok(())
    }
}

fn reset(
    plugins: ScientificPlugins,
    experiment: kagami_document::Experiment,
    mut schemas: kagami_catalog::SchemaRegistry,
    request: ComponentResetRequest,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    control.check().map_err(|_| EffectError::Cancelled)?;
    let authoring = match plugins
        .store
        .prepare_authoring_lock(
            request.dependencies.clone(),
            plugins.revision,
            &plugins.overrides,
            limits.dependencies,
        )
        .map_err(EffectError::Inventory)?
    {
        crate::plugins::PrepareAuthoringLockOutcome::Ready(p) => p,
        crate::plugins::PrepareAuthoringLockOutcome::Unresolved(outcome) => {
            return Err(EffectError::Resolution(Box::new(outcome)));
        }
    };
    for schema in authoring.schemas().schemas() {
        schemas.insert(schema.clone());
    }
    let selected = prepare_setup_selection(&plugins, &request.setup)?;
    let mut commands = request.edits;
    commands.push(ExperimentCommand::AdoptDependencies(request.dependencies));
    let proposal = kagami_document::update::prepare_scientific_reset(
        &experiment,
        &commands,
        &schemas,
        selected.compiled().verified(),
        &limits,
    )
    .map_err(EffectError::Authoring)?;
    let variables = proposal.variables().map_err(EffectError::Authoring)?;
    let setup = initialize_setup(
        &selected,
        &request.setup,
        &variables,
        |instance, bulk| {
            proposal
                .initial_dynamics(instance, bulk)
                .map_err(EffectError::Authoring)
        },
        limits,
        control.clone(),
    )?;
    let original = experiment.snapshot();
    let mut retained = ScientificRetention::new(
        INPUT_BYTES + SETUP_OUTPUT_BYTES + OUTPUT_BYTES + SELECTED_METADATA as usize,
    );
    retained
        .include(
            original
                .setup()
                .scientific()
                .expect("reset requires capture"),
        )
        .map_err(|_| EffectError::Limit)?;
    if let Some(lock) = original.dependencies() {
        retained
            .include_dependencies(lock)
            .map_err(|_| EffectError::Limit)?;
    }
    for command in &commands {
        if let ExperimentCommand::AdoptDependencies(lock) = command {
            retained
                .include_dependencies(lock)
                .map_err(|_| EffectError::Limit)?;
        }
    }
    retained.include(&setup).map_err(|_| EffectError::Limit)?;
    control.check().map_err(|_| EffectError::Cancelled)?;
    let availability = plugins
        .store
        .guard_revision(plugins.revision)
        .map_err(EffectError::Inventory)?;
    Ok(Completed {
        setup,
        commands,
        schemas: Some(schemas),
        _availability: availability,
        _selected: selected,
    })
}
