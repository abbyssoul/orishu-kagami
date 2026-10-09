//! Explicit captured-component additions with unchanged executable physics.
use super::*;
use orishu_plugin::{authoring_lock::SelectionLock, resolution::Selection};

enum Input {
    Commands(Vec<ExperimentCommand>),
    Template {
        catalog: Arc<kagami_catalog::CatalogSet>,
        spec: Box<kagami_session::InstantiationSpec>,
    },
}

impl ScientificEffects {
    /// Add exact components and their complete lock to a captured experiment.
    /// The supplied guard represents the author's explicit proposal context.
    /// Only attachment commands are accepted; the final lock and new scientific
    /// evidence/history are submitted atomically. Existing providers/kernels and
    /// fields cannot be migrated or reset by this operation. UI/MCP adapters must
    /// resolve choices and explain history regeneration before requesting it.
    pub fn extend_components(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        attachments: Vec<ExperimentCommand>,
        lock: Arc<SelectionLock>,
    ) -> Result<(), EffectError> {
        if attachments.iter().any(|c| !matches!(c,
            ExperimentCommand::AttachComponent { component, .. } if component.contribution().is_some()))
        {
            return Err(EffectError::Unavailable);
        }
        self.compose_scene(document, guard, attachments, lock)
    }

    /// Compose explicit object/component additions and new copied definitions,
    /// preserving every existing provider/kernel and field buffer. The complete
    /// resulting lock is supplied separately; setup, embedded locks, removals and
    /// edits to existing definitions are refused. Callers must obtain consent to
    /// regenerate initial history and retain the exact proposal context.
    pub fn compose_scene(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        commands: Vec<ExperimentCommand>,
        lock: Arc<SelectionLock>,
    ) -> Result<(), EffectError> {
        check_commands(&commands, document.limits())?;
        self.begin_extension(document, guard, Input::Commands(commands), lock)
    }

    /// Materialize one exact, available template from the adopted immutable
    /// catalog snapshot and regenerate history before atomic adoption. A matching
    /// fingerprint and a complete resolved component lock are required. Template
    /// provider choices cannot be overridden by the supplied lock. No file IO or
    /// live catalog link is introduced; UI/MCP must collect explicit consent.
    pub fn instantiate_template(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        spec: kagami_session::InstantiationSpec,
        lock: Arc<SelectionLock>,
    ) -> Result<(), EffectError> {
        let catalog = document.catalog_snapshot().ok_or_else(|| {
            EffectError::Instantiation(Box::new(kagami_session::SessionRejection::NoCatalogLoaded))
        })?;
        self.instantiate_template_from(document, guard, catalog, spec, lock)
    }

    /// Native catalog preparation has explicitly revalidated the immutable
    /// source under selected schemas. No live catalog replacement is necessary.
    pub(crate) fn instantiate_template_from(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        catalog: Arc<kagami_catalog::CatalogSet>,
        spec: kagami_session::InstantiationSpec,
        lock: Arc<SelectionLock>,
    ) -> Result<(), EffectError> {
        scene_inputs::check_instantiation(&spec, document.limits())?;
        if spec.expected_fingerprint.is_none() {
            return Err(EffectError::Unavailable);
        }
        self.begin_extension(
            document,
            guard,
            Input::Template {
                catalog,
                spec: Box::new(spec),
            },
            lock,
        )
    }

    fn begin_extension(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        input: Input,
        lock: Arc<SelectionLock>,
    ) -> Result<(), EffectError> {
        if self.pending.is_some() {
            return Err(EffectError::Busy);
        }
        if !document.accepts_effect(guard) {
            return Err(EffectError::Document);
        }
        let plugins = self.plugins.clone().ok_or(EffectError::Unavailable)?;
        let limits = *document.limits();
        lock.validate(limits.dependencies)
            .map_err(|_| EffectError::Limit)?;
        let setup = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or(EffectError::Unavailable)?;
        let mut retained = ScientificRetention::new(INPUT_BYTES);
        retained.include(setup).map_err(|_| EffectError::Limit)?;
        retained
            .include_dependencies(&lock)
            .map_err(|_| EffectError::Limit)?;
        if let Some(previous) = document.snapshot().dependencies() {
            retained
                .include_dependencies(previous)
                .map_err(|_| EffectError::Limit)?;
            if previous
                .merge(&lock, limits.dependencies)
                .map_err(|_| EffectError::Selection)?
                != *lock
            {
                return Err(EffectError::Selection);
            }
        }
        let experiment = document.experiment().clone();
        let schemas = document.schemas().clone();
        let control =
            OperationControl::new(Duration::from_secs(60)).map_err(|_| EffectError::Executor)?;
        let worker_control = control.clone();
        let task = std::thread::Builder::new()
            .name("kagami-component-extension".into())
            .spawn(move || {
                extend(
                    plugins,
                    experiment,
                    schemas,
                    input,
                    lock,
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

fn check_commands(commands: &[ExperimentCommand], limits: &Limits) -> Result<(), EffectError> {
    if commands.is_empty() || commands.len().saturating_add(2) > limits.max_commands_per_batch {
        return Err(EffectError::Limit);
    }
    for command in commands {
        match command {
            ExperimentCommand::AttachComponent { component, .. }
                if component.contribution().is_some() => {}
            ExperimentCommand::CreateObject(spec)
                if spec.components.keys().all(|c| c.contribution().is_some()) => {}
            ExperimentCommand::DefineVariable(_) => {}
            _ => return Err(EffectError::Unavailable),
        }
    }
    scene_inputs::check_addition(commands, limits)
}

fn extend(
    plugins: ScientificPlugins,
    experiment: kagami_document::Experiment,
    mut schemas: kagami_catalog::SchemaRegistry,
    input: Input,
    lock: Arc<SelectionLock>,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    control.check().map_err(|_| EffectError::Cancelled)?;
    let authoring = match plugins
        .store
        .prepare_authoring_lock(
            lock.clone(),
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
    let mut commands = match input {
        Input::Commands(commands) => commands,
        Input::Template { catalog, spec } => {
            let mut commands = kagami_session::instantiation::prepare(
                &experiment,
                &catalog,
                &schemas,
                &spec,
                &limits,
            )
            .map_err(|e| EffectError::Instantiation(Box::new(e)))?;
            if let Some(ExperimentCommand::AdoptDependencies(required)) = commands.last() {
                let merged = required
                    .merge(&lock, limits.dependencies)
                    .map_err(kagami_document::dependencies::DependencyError::from)
                    .map_err(kagami_document::Rejection::from)
                    .map_err(EffectError::Authoring)?;
                if merged != *lock {
                    return Err(EffectError::Authoring(
                        kagami_document::dependencies::DependencyError::Roots.into(),
                    ));
                }
                commands.pop();
            }
            check_commands(&commands, &limits)?;
            commands
        }
    };
    let old = experiment.snapshot();
    let setup = old.setup().scientific().ok_or(EffectError::Unavailable)?;
    let descriptor = setup.declarations().descriptor();
    let previous = SelectionLock::new(
        Selection {
            roots: descriptor.roots.clone(),
            contributions: descriptor.contributions.clone(),
            bindings: descriptor
                .bindings
                .iter()
                .map(|b| ProviderBinding {
                    requirement: RequirementKey {
                        consumer: b.consumer.clone(),
                        slot: b.requirement_slot.clone(),
                    },
                    provider: b.provider.clone(),
                })
                .collect(),
        },
        limits.dependencies,
    )
    .map_err(|_| EffectError::Limit)?;
    let combined = previous
        .merge(&lock, limits.dependencies)
        .map_err(|_| EffectError::Selection)?;
    let request = ResolutionRequest {
        expected_inventory_revision: plugins.revision,
        roots: combined.selection().roots.clone(),
        bindings: combined.selection().bindings.clone(),
    };
    let selected = match plugins
        .store
        .prepare_selection(
            &request,
            &plugins.overrides,
            &descriptor.kernel_instances,
            SelectionLimits {
                artifact_bytes: SELECTED_BYTES,
                metadata_bytes: SELECTED_METADATA,
                ..Default::default()
            },
        )
        .map_err(EffectError::Inventory)?
    {
        PrepareSelectionOutcome::Ready(p) => *p,
        PrepareSelectionOutcome::Unresolved(outcome) => {
            return Err(EffectError::Resolution(Box::new(outcome)));
        }
    };
    commands.push(ExperimentCommand::AdoptDependencies(lock));
    let proposal = kagami_document::update::prepare_extended_history_edit(
        &experiment,
        &commands,
        &schemas,
        selected.compiled().verified(),
        &limits,
    )
    .map_err(EffectError::Authoring)?;
    control.check().map_err(|_| EffectError::Cancelled)?;
    let setup = proposal.setup();
    let history = initialize_history(&proposal, &selected, limits, control.clone())?;
    let mut completed =
        complete_capture(plugins, setup, selected, history, commands, limits, control)?;
    // Count the original declaration evidence too, not just the rebased proposal.
    let mut retained =
        ScientificRetention::new(INPUT_BYTES + OUTPUT_BYTES * 2 + SELECTED_METADATA as usize);
    retained
        .include(old.setup().scientific().expect("captured checked"))
        .map_err(|_| EffectError::Limit)?;
    retained.include(setup).map_err(|_| EffectError::Limit)?;
    retained
        .include(&completed.setup)
        .map_err(|_| EffectError::Limit)?;
    completed.schemas = Some(schemas);
    Ok(completed)
}
