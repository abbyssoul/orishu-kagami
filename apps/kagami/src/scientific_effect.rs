//! One bounded, cancellable authoring effect outside the window update lane.
//! Guests prepare candidates; only Document's guarded command may adopt them.
use crate::{
    document::{AuthoringGuard, Document},
    plugins::{InventoryRevisionGuard, PluginStore, PrepareSelectionOutcome, PreparedSelection},
    scientific::{
        InitializationError, InitializationLimits, InitializationRequest, LocalInitializer,
    },
};
use kagami_document::{
    ExperimentCommand, ExperimentSnapshot, Limits,
    scientific::{ScientificRetention, ScientificSetup},
};
use orishu_plugin::{
    ExecutionContractId, PluginId,
    execution::{AuthoredConfigurationProperty, ConfigurationInput},
    resolution::{ProviderBinding, RequirementKey, ResolutionRequest},
    selected::SelectionLimits,
};
use orishu_runtime::{OperationControl, Sandbox, SandboxLimits};
use orishu_workload::ComponentInstanceId;
use std::{sync::Arc, thread::JoinHandle, time::Duration};

/// Exact startup inventory and process-only overrides. Model construction does
/// not discover a user's files or substitute a later default release.
#[derive(Clone, Debug)]
pub struct ScientificPlugins {
    /// Already-opened secure local inventory authority.
    pub store: Arc<PluginStore>,
    /// Revision used for the window's current vocabulary.
    pub revision: u64,
    /// Session-only logical enablement; never written into a plugin inventory.
    pub overrides: Vec<(PluginId, bool)>,
}

const INPUT_BYTES: usize = 128 * 1024 * 1024;
const SELECTED_BYTES: u64 = 64 * 1024 * 1024;
const SELECTED_METADATA: u64 = 8 * 1024 * 1024;
const OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const SETUP_OUTPUT_BYTES: usize = 128 * 1024 * 1024;

mod component_extension;
mod component_reset;
pub use component_reset::ComponentResetRequest;
mod scene_inputs;

/// Explicit replacement of scientific intent, not a guessed legacy-domain
/// migration. UI/MCP adapters must collect these choices from their caller.
#[derive(Clone, Debug, PartialEq)]
pub struct SetupRequest {
    /// Exact roots and dependency choices; ambiguity is returned, never guessed.
    pub selection: ResolutionRequest,
    /// Configured field/integrator uses, independently compiled kernels.
    pub kernels: Vec<orishu_plugin::selected::SelectedKernel>,
    /// Exactly one request per use, all naming the same explicit domain.
    pub initialization: Vec<InitializationRequest>,
    /// Explicit authored fixed step, not wall time.
    pub time_step: kagami_document::TimeStep,
}

/// Stable refusal boundary; no guest/native error strings or filesystem paths.
#[derive(Debug)]
pub enum EffectError {
    /// A job still owns capacity, including cancelled native compilation.
    Busy,
    /// No scientific field, authoring mode, or configured inventory is available.
    Unavailable,
    /// Input/candidate retention exceeded the single-job ceiling.
    Limit,
    /// Required exact providers are unavailable; no alternate was selected.
    Selection,
    /// Complete bounded resolver refusal/choices for future UI/MCP adapters.
    Resolution(Box<orishu_plugin::resolution::ResolutionOutcome>),
    /// Inventory IO/revision refused the operation.
    Inventory(crate::plugins::Error),
    /// Scientific initialization refused the operation.
    Initialization(InitializationError),
    /// Final setup or dimension-aware variables failed document validation.
    Document,
    /// Ordinary authoring validation refused the proposed scene edit.
    Authoring(kagami_document::Rejection),
    /// Catalog materialization or preparation refused without accepting edits.
    Instantiation(Box<kagami_session::SessionRejection>),
    /// Off-window executor failed; nothing was adopted.
    Executor,
    /// Explicit cancellation or deadline; previous authored state is unchanged.
    Cancelled,
}
impl std::fmt::Display for EffectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => {
                f.write_str("A scientific edit still owns the background slot; wait for cleanup.")
            }
            Self::Unavailable => {
                f.write_str("Scientific edits require Authoring mode, an available plugin inventory and valid selected physics.")
            }
            Self::Limit => f.write_str("Scientific edit exceeds the pending-effect byte budget."),
            Self::Selection => f.write_str(
                "The exact captured plugin selection is unavailable; no substitute was selected.",
            ),
            Self::Resolution(outcome) => match outcome.as_ref() {
                orishu_plugin::resolution::ResolutionOutcome::StaleRevision { .. } => f.write_str("Plugin inventory changed; refresh the startup inventory before retrying."),
                orishu_plugin::resolution::ResolutionOutcome::Unavailable { issues, .. }
                    if issues.iter().any(|i| i.reason == orishu_plugin::resolution::UnavailableReason::AmbiguousProvider) =>
                    f.write_str("Plugin dependencies require an explicit provider choice. Use Check dependencies in Configure physics, choose a provider, then confirm Apply again."),
                _ => f.write_str("The exact captured plugin selection is unavailable; no substitute was selected."),
            },
            Self::Inventory(e) => e.fmt(f),
            Self::Initialization(e) => e.fmt(f),
            Self::Document => {
                f.write_str("Scientific edit does not match the captured document inputs.")
            }
            Self::Authoring(error) => error.fmt(f),
            Self::Instantiation(error) => error.fmt(f),
            Self::Executor => {
                f.write_str("Scientific edit executor failed; the experiment is unchanged.")
            }
            Self::Cancelled => {
                f.write_str("Scientific edit cancelled; the experiment is unchanged.")
            }
        }
    }
}
impl std::error::Error for EffectError {}

struct Completed {
    setup: ScientificSetup,
    commands: Vec<ExperimentCommand>,
    // Fresh verified capabilities are staged with the commands, never installed
    // before guarded acceptance. Existing field/history edits need no refresh.
    schemas: Option<kagami_catalog::SchemaRegistry>,
    // Held through guarded adoption, not during compilation or guest execution.
    _availability: InventoryRevisionGuard,
    _selected: PreparedSelection,
}
struct Pending {
    guard: AuthoringGuard,
    control: OperationControl,
    task: Option<JoinHandle<Result<Completed, EffectError>>>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.control.cancel();
        // Dropping a live JoinHandle detaches, never blocks the event loop.
        // The thread retains inputs/leases until actual exit, not cancellation.
    }
}

/// A window's single non-queuing initialization lane. Poll never waits for JIT.
#[derive(Default)]
pub struct ScientificEffects {
    plugins: Option<ScientificPlugins>,
    pending: Option<Pending>,
}
impl ScientificEffects {
    /// Explicit availability refresh cancels stale work without releasing its
    /// capacity before the task exits. It never changes captured state.
    pub(crate) fn refresh_plugins(&mut self, plugins: ScientificPlugins) {
        self.cancel();
        self.plugins = Some(plugins);
    }
    /// Bind the inventory already chosen by startup, without filesystem IO.
    pub fn new(plugins: Option<ScientificPlugins>) -> Self {
        Self {
            plugins,
            pending: None,
        }
    }
    /// True until the actual worker exits and its result is adopted/discarded.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Whether this shell can request scientific effects (availability is still
    /// checked against exact pins for each request).
    pub fn is_configured(&self) -> bool {
        self.plugins.is_some()
    }
    /// Cancel without releasing capacity early or changing any authored state.
    pub fn cancel(&self) {
        if let Some(pending) = &self.pending {
            pending.control.cancel();
        }
    }

    /// Prepare object edits and replacement integrator history as one atomic
    /// authoring operation. Fields/configuration are retained, not initialized.
    /// No identities or draft edits are accepted until the guarded completion.
    pub fn edit_scene(
        &mut self,
        document: &Document,
        commands: Vec<ExperimentCommand>,
    ) -> Result<(), EffectError> {
        if self.pending.is_some() {
            return Err(EffectError::Busy);
        }
        let guard = document.authoring_guard().ok_or(EffectError::Unavailable)?;
        let plugins = self.plugins.clone().ok_or(EffectError::Unavailable)?;
        let limits = *document.limits();
        if commands.is_empty() || commands.len() >= limits.max_commands_per_batch {
            return Err(EffectError::Limit);
        }
        scene_inputs::check(&commands, &limits)?;
        let setup = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or(EffectError::Unavailable)?;
        let mut retention = ScientificRetention::new(INPUT_BYTES);
        retention.include(setup).map_err(|_| EffectError::Limit)?;
        if let Some(lock) = document.snapshot().dependencies() {
            retention
                .include_dependencies(lock)
                .map_err(|_| EffectError::Limit)?;
        }
        for command in &commands {
            if let ExperimentCommand::AdoptDependencies(lock) = command {
                retention
                    .include_dependencies(lock)
                    .map_err(|_| EffectError::Limit)?;
            }
        }
        let experiment = document.experiment().clone();
        let schemas = document.schemas().clone();
        let control =
            OperationControl::new(Duration::from_secs(60)).map_err(|_| EffectError::Executor)?;
        let worker_control = control.clone();
        let task = std::thread::Builder::new()
            .name("kagami-history-edit".into())
            .spawn(move || {
                edit_scene(
                    plugins,
                    experiment,
                    schemas,
                    commands,
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
    /// Reinitialize exactly one captured field with its existing domain/config.
    /// Fields never receive entities in init; untouched fields/history keep bytes.
    pub fn reinitialize(
        &mut self,
        document: &Document,
        instance: ComponentInstanceId,
    ) -> Result<(), EffectError> {
        let guard = document.authoring_guard().ok_or(EffectError::Unavailable)?;
        self.change_field(document, guard, instance, None)
    }

    /// Replace one field's authored parameters and natural initial state. The
    /// caller must name the document context from which the edit was prepared.
    /// Other field captures and Dynamics history retain their original buffers.
    pub fn configure_field(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        instance: ComponentInstanceId,
        configuration: Vec<AuthoredConfigurationProperty>,
    ) -> Result<(), EffectError> {
        self.change_field(document, guard, instance, Some(configuration))
    }

    fn change_field(
        &mut self,
        document: &Document,
        guard: AuthoringGuard,
        instance: ComponentInstanceId,
        configuration: Option<Vec<AuthoredConfigurationProperty>>,
    ) -> Result<(), EffectError> {
        if self.pending.is_some() {
            return Err(EffectError::Busy);
        }
        if !document.accepts_effect(guard) {
            return Err(EffectError::Document);
        }
        let plugins = self.plugins.clone().ok_or(EffectError::Unavailable)?;
        let setup = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or(EffectError::Unavailable)?;
        if !setup
            .captures()
            .get(&instance)
            .is_some_and(|c| c.context.execution_contract == ExecutionContractId::Field)
        {
            return Err(EffectError::Unavailable);
        }
        let configuration_bytes = configuration
            .as_ref()
            .map(|input| check_configuration(input, document.limits()))
            .transpose()?
            .unwrap_or(0);
        // Reservation before thread creation, selection reads or JIT. New
        // authored inputs share the retained-input budget with old captures.
        // At most
        // 128 MiB retained scientific input + 64 MiB selected artifacts (including
        // <=8 MiB metadata) + 16 MiB new state. Guest/JIT memory is separately
        // governed by Sandbox; this is not a claim of an aggregate RSS limit.
        ScientificRetention::new(
            INPUT_BYTES
                .checked_sub(configuration_bytes)
                .ok_or(EffectError::Limit)?,
        )
        .include(setup)
        .map_err(|_| EffectError::Limit)?;
        let snapshot = document.snapshot().clone();
        let limits = *document.limits();
        let control =
            OperationControl::new(Duration::from_secs(60)).map_err(|_| EffectError::Executor)?;
        let worker_control = control.clone();
        let task = std::thread::Builder::new()
            .name("kagami-field-init".into())
            .spawn(move || {
                initialize(
                    plugins,
                    snapshot,
                    instance,
                    configuration,
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

    /// Construct a whole scientific setup from explicit caller choices. Each
    /// field initializes independently; Dynamics history uses current objects.
    /// Failure/cancellation of any invocation leaves the entire document intact.
    pub fn configure(
        &mut self,
        document: &Document,
        request: SetupRequest,
    ) -> Result<(), EffectError> {
        if self.pending.is_some() {
            return Err(EffectError::Busy);
        }
        let guard = document.authoring_guard().ok_or(EffectError::Unavailable)?;
        let plugins = self.plugins.clone().ok_or(EffectError::Unavailable)?;
        let limits = *document.limits();
        let configuration_bytes = request.check(&plugins, &limits)?;
        if let Some(setup) = document.snapshot().setup().scientific() {
            ScientificRetention::new(INPUT_BYTES - configuration_bytes)
                .include(setup)
                .map_err(|_| EffectError::Limit)?;
        }
        let snapshot = document.snapshot().clone();
        let control =
            OperationControl::new(Duration::from_secs(60)).map_err(|_| EffectError::Executor)?;
        let worker_control = control.clone();
        let task = std::thread::Builder::new()
            .name("kagami-setup-init".into())
            .spawn(move || configure(plugins, snapshot, request, limits, worker_control))
            .map_err(|_| EffectError::Executor)?;
        self.pending = Some(Pending {
            guard,
            control,
            task: Some(task),
        });
        Ok(())
    }
    /// Nonblocking completion/cancellation handling. Returns true only when an
    /// ordinary guarded document command accepted the complete candidate.
    pub fn poll(&mut self, document: &mut Document) -> bool {
        let Some(pending) = &self.pending else {
            return false;
        };
        if !document.accepts_effect(pending.guard) {
            pending.control.cancel();
        }
        if !pending.task.as_ref().is_some_and(JoinHandle::is_finished) {
            return false;
        }
        let mut pending = self.pending.take().expect("pending checked");
        let result = pending
            .task
            .take()
            .expect("pending task")
            .join()
            .unwrap_or(Err(EffectError::Executor));
        if pending.control.check().is_err() {
            document.notice = Some(if document.accepts_effect(pending.guard) {
                EffectError::Cancelled.to_string()
            } else {
                "Scientific edit discarded: the document or authoring context changed. Retry explicitly.".into()
            });
            return false;
        }
        match result {
            Ok(mut completed) => {
                completed
                    .commands
                    .push(ExperimentCommand::AdoptScientificSetup(Arc::new(
                        completed.setup,
                    )));
                if let Some(schemas) = completed.schemas {
                    document.edit_guarded_with_schemas(pending.guard, completed.commands, schemas)
                } else {
                    document.edit_guarded(pending.guard, completed.commands)
                }
            }
            Err(error) => {
                document.notice = Some(error.to_string());
                false
            }
        }
    }
}

impl SetupRequest {
    fn check(&self, plugins: &ScientificPlugins, limits: &Limits) -> Result<usize, EffectError> {
        if self.selection.expected_inventory_revision != plugins.revision {
            return Err(EffectError::Selection);
        }
        let selection_limits = SelectionLimits::default();
        if self.selection.roots.len() > selection_limits.contributions
            || self.selection.bindings.len() > selection_limits.bindings
        {
            return Err(EffectError::Limit);
        }
        let count = self.kernels.len();
        if count == 0 || count > limits.scientific.kernels || self.initialization.len() != count {
            return Err(EffectError::Limit);
        }
        if self
            .kernels
            .iter()
            .filter(|k| k.execution_contract == ExecutionContractId::Dynamics)
            .count()
            != 1
        {
            return Err(EffectError::Selection);
        }
        let mut ids = std::collections::BTreeSet::new();
        for input in &self.initialization {
            if !ids.insert(&input.instance)
                || !self.kernels.iter().any(|k| k.instance_id == input.instance)
                || input.domain != self.initialization[0].domain
            {
                return Err(EffectError::Selection);
            }
            input
                .domain
                .to_cbor(Default::default())
                .map_err(|_| EffectError::Document)?;
            if input.quality_flags.len() > 64 {
                return Err(EffectError::Limit);
            }
        }
        check_configurations(
            self.initialization
                .iter()
                .map(|i| i.configuration.as_slice()),
            limits,
            INPUT_BYTES,
        )
    }
}

fn check_configurations<'a>(
    configurations: impl Iterator<Item = &'a [AuthoredConfigurationProperty]>,
    limits: &Limits,
    budget: usize,
) -> Result<usize, EffectError> {
    let mut bytes = 0usize;
    for configuration in configurations {
        bytes = bytes
            .checked_add(check_configuration(configuration, limits)?)
            .filter(|n| *n <= budget)
            .ok_or(EffectError::Limit)?;
    }
    Ok(bytes)
}

fn check_configuration(
    configuration: &[AuthoredConfigurationProperty],
    limits: &Limits,
) -> Result<usize, EffectError> {
    if configuration.len() > orishu_plugin::Limits::default().max_schema_items {
        return Err(EffectError::Limit);
    }
    let mut bytes = std::mem::size_of_val(configuration);
    for property in configuration {
        let (length, bound) = match &property.input {
            ConfigurationInput::Expression { source } => {
                (source.len(), limits.max_expression_bytes)
            }
            ConfigurationInput::Text { value } => (value.len(), limits.max_text_bytes),
            ConfigurationInput::Boolean { .. } => (0, 0),
        };
        if length > bound {
            return Err(EffectError::Limit);
        }
        bytes = bytes
            .checked_add(property.id.as_str().len())
            .and_then(|n| n.checked_add(length))
            .filter(|n| *n <= INPUT_BYTES)
            .ok_or(EffectError::Limit)?;
    }
    Ok(bytes)
}

fn prepare_setup_selection(
    plugins: &ScientificPlugins,
    request: &SetupRequest,
) -> Result<PreparedSelection, EffectError> {
    match plugins
        .store
        .prepare_selection(
            &request.selection,
            &plugins.overrides,
            &request.kernels,
            SelectionLimits {
                artifact_bytes: SELECTED_BYTES,
                metadata_bytes: SELECTED_METADATA,
                ..Default::default()
            },
        )
        .map_err(EffectError::Inventory)?
    {
        PrepareSelectionOutcome::Ready(p) => Ok(*p),
        PrepareSelectionOutcome::Unresolved(outcome) => {
            Err(EffectError::Resolution(Box::new(outcome)))
        }
    }
}

fn configure(
    plugins: ScientificPlugins,
    snapshot: ExperimentSnapshot,
    request: SetupRequest,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    control.check().map_err(|_| EffectError::Cancelled)?;
    let selected = prepare_setup_selection(&plugins, &request)?;
    let variables =
        kagami_document::variable_context(&snapshot, &limits).map_err(|_| EffectError::Document)?;
    let setup = initialize_setup(
        &selected,
        &request,
        &variables,
        |instance, bulk| {
            kagami_document::scientific::initial_dynamics(
                &snapshot,
                &selected.compiled().verified().declarations(),
                instance,
                &variables,
                &limits,
                bulk,
            )
            .map_err(|_| EffectError::Document)
        },
        limits,
        control.clone(),
    )?;
    let mut retained = ScientificRetention::new(
        INPUT_BYTES + SETUP_OUTPUT_BYTES + OUTPUT_BYTES + SELECTED_METADATA as usize,
    );
    if let Some(old) = snapshot.setup().scientific() {
        retained.include(old).map_err(|_| EffectError::Limit)?;
    }
    retained.include(&setup).map_err(|_| EffectError::Limit)?;
    control.check().map_err(|_| EffectError::Cancelled)?;
    let availability = plugins
        .store
        .guard_revision(plugins.revision)
        .map_err(EffectError::Inventory)?;
    Ok(Completed {
        setup,
        commands: vec![],
        schemas: None,
        _availability: availability,
        _selected: selected,
    })
}

/// Shared full-reset initialization: fields receive no entities, and history
/// receives only freshly projected objects. No old opaque capture is an input.
fn initialize_setup(
    selected: &PreparedSelection,
    request: &SetupRequest,
    variables: &orishu_variables::VariablesSystem,
    initial_dynamics: impl Fn(
        &ComponentInstanceId,
        orishu_plugin::execution::BulkLimits,
    ) -> Result<Vec<u8>, EffectError>,
    limits: Limits,
    control: OperationControl,
) -> Result<ScientificSetup, EffectError> {
    use orishu_plugin::execution::{Batch, BulkLimits, BulkRecord, DynamicEntity};
    control.check().map_err(|_| EffectError::Cancelled)?;
    let initializer = LocalInitializer::new(
        Sandbox::new(SandboxLimits::default()).map_err(|_| EffectError::Executor)?,
        InitializationLimits {
            state_bytes: (SETUP_OUTPUT_BYTES / request.kernels.len()).min(OUTPUT_BYTES),
            ..Default::default()
        },
    );
    let mut captures = Vec::with_capacity(request.kernels.len());
    for kernel in &request.kernels {
        control.check().map_err(|_| EffectError::Cancelled)?;
        let input = request
            .initialization
            .iter()
            .find(|i| i.instance == kernel.instance_id)
            .ok_or(EffectError::Selection)?;
        let captured = match kernel.execution_contract {
            ExecutionContractId::Field => {
                initializer.field(selected, input, variables, control.clone())
            }
            ExecutionContractId::Dynamics => {
                let bulk = BulkLimits {
                    bytes: OUTPUT_BYTES,
                    records: limits.max_objects.min(65_536),
                };
                let entities = initial_dynamics(&kernel.instance_id, bulk)?;
                let count = Batch::<DynamicEntity>::read(&entities, bulk)
                    .map_err(|_| EffectError::Document)?
                    .len();
                initializer.history(
                    selected,
                    input,
                    variables,
                    orishu_runtime::Buffer {
                        schema: DynamicEntity::SCHEMA.into(),
                        value_count: count as u64,
                        bytes: entities.into(),
                    },
                    control.clone(),
                )
            }
        }
        .map_err(EffectError::Initialization)?;
        captures.push(captured.document_capture());
    }
    control.check().map_err(|_| EffectError::Cancelled)?;
    ScientificSetup::capture(
        selected.compiled().verified(),
        request.initialization[0].domain.clone(),
        request.time_step,
        captures,
        limits.scientific,
    )
    .map_err(|_| EffectError::Document)
}

fn initialize(
    plugins: ScientificPlugins,
    snapshot: ExperimentSnapshot,
    instance: ComponentInstanceId,
    configuration: Option<Vec<AuthoredConfigurationProperty>>,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    let check = || control.check().map_err(|_| EffectError::Cancelled);
    check()?;
    let setup = snapshot
        .setup()
        .scientific()
        .ok_or(EffectError::Unavailable)?;
    let selected = prepare_captured(&plugins, setup)?;
    check()?;
    let capture = &setup.captures()[&instance];
    let variables =
        kagami_document::variable_context(&snapshot, &limits).map_err(|_| EffectError::Document)?;
    let request = InitializationRequest {
        instance: instance.clone(),
        domain: setup.domain().clone(),
        configuration: configuration.unwrap_or_else(|| capture.authored.clone()),
        compute_precision: capture.context.compute_precision,
        quality_flags: capture
            .context
            .observables
            .iter()
            .map(|o| (o.slot.clone(), o.quality_flags))
            .collect(),
    };
    let initializer = LocalInitializer::new(
        Sandbox::new(SandboxLimits::default()).map_err(|_| EffectError::Executor)?,
        InitializationLimits {
            state_bytes: OUTPUT_BYTES,
            ..Default::default()
        },
    );
    let initialized = initializer
        .field(&selected, &request, &variables, control.clone())
        .map_err(EffectError::Initialization)?;
    check()?;
    complete_capture(
        plugins,
        setup,
        selected,
        initialized.document_capture(),
        vec![],
        limits,
        control,
    )
}

fn prepare_captured(
    plugins: &ScientificPlugins,
    setup: &ScientificSetup,
) -> Result<PreparedSelection, EffectError> {
    let descriptor = setup.declarations().descriptor();
    let request = ResolutionRequest {
        expected_inventory_revision: plugins.revision,
        roots: descriptor.roots.clone(),
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
        PrepareSelectionOutcome::Ready(selected) => *selected,
        PrepareSelectionOutcome::Unresolved(outcome) => {
            return Err(EffectError::Resolution(Box::new(outcome)));
        }
    };
    if selected.compiled().verified().descriptor() != descriptor {
        return Err(EffectError::Selection);
    }
    Ok(selected)
}

fn complete_capture(
    plugins: ScientificPlugins,
    setup: &ScientificSetup,
    selected: PreparedSelection,
    initialized: kagami_document::scientific::KernelCapture,
    commands: Vec<ExperimentCommand>,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    // History capture retains both the new initial-entity packet and guest
    // output; each was admitted under OUTPUT_BYTES before invocation.
    let history_input_bytes = usize::from(initialized.history_entities.is_some()) * OUTPUT_BYTES;
    let captures = setup
        .captures()
        .iter()
        .map(|(id, capture)| {
            if *id == initialized.context.instance {
                initialized.clone()
            } else {
                capture.clone()
            }
        })
        .collect();
    let replacement = ScientificSetup::capture(
        selected.compiled().verified(),
        setup.domain().clone(),
        setup.time_step(),
        captures,
        limits.scientific,
    )
    .map_err(|_| EffectError::Document)?;
    let mut retained = ScientificRetention::new(
        INPUT_BYTES + OUTPUT_BYTES + history_input_bytes + SELECTED_METADATA as usize,
    );
    retained.include(setup).map_err(|_| EffectError::Limit)?;
    retained
        .include(&replacement)
        .map_err(|_| EffectError::Limit)?;
    control.check().map_err(|_| EffectError::Cancelled)?;
    // No guest can run after this read guard. A concurrent disable/update either
    // wins first and causes stale refusal, or gets Busy until adoption completes.
    let availability = plugins
        .store
        .guard_revision(plugins.revision)
        .map_err(EffectError::Inventory)?;
    Ok(Completed {
        setup: replacement,
        commands,
        schemas: None,
        _availability: availability,
        _selected: selected,
    })
}

fn edit_scene(
    plugins: ScientificPlugins,
    experiment: kagami_document::Experiment,
    schemas: kagami_catalog::SchemaRegistry,
    commands: Vec<ExperimentCommand>,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    control.check().map_err(|_| EffectError::Cancelled)?;
    let proposal =
        kagami_document::update::prepare_history_edit(&experiment, &commands, &schemas, &limits)
            .map_err(EffectError::Authoring)?;
    let setup = proposal.setup();
    let selected = prepare_captured(&plugins, setup)?;
    let history = initialize_history(&proposal, &selected, limits, control.clone())?;
    complete_capture(plugins, setup, selected, history, commands, limits, control)
}

fn initialize_history(
    proposal: &kagami_document::update::HistoryEdit,
    selected: &PreparedSelection,
    limits: Limits,
    control: OperationControl,
) -> Result<kagami_document::scientific::KernelCapture, EffectError> {
    use orishu_plugin::execution::{Batch, BulkLimits, BulkRecord, DynamicEntity};
    let setup = proposal.setup();
    let capture = setup
        .captures()
        .values()
        .find(|c| c.context.execution_contract == ExecutionContractId::Dynamics)
        .ok_or(EffectError::Selection)?;
    let bulk = BulkLimits {
        bytes: OUTPUT_BYTES,
        records: limits.max_objects.min(65_536),
    };
    let entities = proposal
        .initial_dynamics(&capture.context.instance, bulk)
        .map_err(EffectError::Authoring)?;
    let count = Batch::<DynamicEntity>::read(&entities, bulk)
        .map_err(|_| EffectError::Document)?
        .len();
    let input = InitializationRequest {
        instance: capture.context.instance.clone(),
        domain: setup.domain().clone(),
        configuration: capture.authored.clone(),
        compute_precision: capture.context.compute_precision,
        quality_flags: Default::default(),
    };
    let initializer = LocalInitializer::new(
        Sandbox::new(SandboxLimits::default()).map_err(|_| EffectError::Executor)?,
        InitializationLimits {
            state_bytes: OUTPUT_BYTES,
            ..Default::default()
        },
    );
    let history = initializer
        .history(
            selected,
            &input,
            &proposal.variables().map_err(EffectError::Authoring)?,
            orishu_runtime::Buffer {
                schema: DynamicEntity::SCHEMA.into(),
                value_count: count as u64,
                bytes: entities.into(),
            },
            control.clone(),
        )
        .map_err(EffectError::Initialization)?;
    Ok(history.document_capture())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_parameters_reserve_owned_metadata_and_refuse_oversized_inputs() {
        let limits = Limits::default();
        let property = AuthoredConfigurationProperty {
            id: "radius".parse().unwrap(),
            input: ConfigurationInput::Expression {
                source: "4 mm".into(),
            },
        };
        let expected =
            std::mem::size_of::<AuthoredConfigurationProperty>() + "radius".len() + "4 mm".len();
        assert_eq!(
            check_configuration(std::slice::from_ref(&property), &limits).unwrap(),
            expected
        );
        let inputs = [std::slice::from_ref(&property); 2];
        assert_eq!(
            check_configurations(inputs.into_iter(), &limits, expected * 2).unwrap(),
            expected * 2
        );
        assert!(matches!(
            check_configurations(inputs.into_iter(), &limits, expected * 2 - 1),
            Err(EffectError::Limit)
        ));
        let mut excess = property.clone();
        excess.input = ConfigurationInput::Text {
            value: "x".repeat(limits.max_text_bytes + 1),
        };
        assert!(matches!(
            check_configuration(&[excess], &limits),
            Err(EffectError::Limit)
        ));
        assert!(matches!(
            check_configuration(
                &vec![property; orishu_plugin::Limits::default().max_schema_items + 1],
                &limits
            ),
            Err(EffectError::Limit)
        ));
    }

    #[test]
    fn cancellation_keeps_the_slot_until_the_actual_thread_exits() {
        let mut document = Document::new(Default::default(), Default::default());
        let revision = document.view().revision();
        let (release, receive) = std::sync::mpsc::sync_channel(1);
        let task = std::thread::spawn(move || {
            receive.recv().unwrap();
            Err(EffectError::Executor)
        });
        let mut effects = ScientificEffects {
            plugins: None,
            pending: Some(Pending {
                guard: document.authoring_guard().unwrap(),
                control: OperationControl::new(Duration::from_secs(60)).unwrap(),
                task: Some(task),
            }),
        };
        effects.cancel();
        assert!(!effects.poll(&mut document));
        assert!(effects.is_pending());
        assert!(matches!(
            effects.reinitialize(&document, "field".parse().unwrap()),
            Err(EffectError::Busy)
        ));
        release.send(()).unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while effects.is_pending() {
            effects.poll(&mut document);
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        assert_eq!(document.view().revision(), revision);
        assert!(document.notice.as_ref().unwrap().contains("cancelled"));
    }
}
