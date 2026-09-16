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
                _ => f.write_str("The exact captured plugin selection is unavailable; no substitute was selected."),
            },
            Self::Inventory(e) => e.fmt(f),
            Self::Initialization(e) => e.fmt(f),
            Self::Document => {
                f.write_str("Scientific edit does not match the captured document inputs.")
            }
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
    /// Reinitialize exactly one captured field with its existing domain/config.
    /// Fields never receive entities in init; untouched fields/history keep bytes.
    pub fn reinitialize(
        &mut self,
        document: &Document,
        instance: ComponentInstanceId,
    ) -> Result<(), EffectError> {
        if self.pending.is_some() {
            return Err(EffectError::Busy);
        }
        let guard = document.authoring_guard().ok_or(EffectError::Unavailable)?;
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
        // Reservation before thread creation, selection reads or JIT. At most
        // 128 MiB retained scientific input + 64 MiB selected artifacts (including
        // <=8 MiB metadata) + 16 MiB new state. Guest/JIT memory is separately
        // governed by Sandbox; this is not a claim of an aggregate RSS limit.
        ScientificRetention::new(INPUT_BYTES)
            .include(setup)
            .map_err(|_| EffectError::Limit)?;
        let snapshot = document.snapshot().clone();
        let limits = *document.limits();
        let control =
            OperationControl::new(Duration::from_secs(60)).map_err(|_| EffectError::Executor)?;
        let worker_control = control.clone();
        let task = std::thread::Builder::new()
            .name("kagami-field-init".into())
            .spawn(move || initialize(plugins, snapshot, instance, limits, worker_control))
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
        request.check(&plugins, &limits)?;
        if let Some(setup) = document.snapshot().setup().scientific() {
            ScientificRetention::new(INPUT_BYTES)
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
            Ok(completed) => document.edit_guarded(
                pending.guard,
                vec![ExperimentCommand::AdoptScientificSetup(Arc::new(
                    completed.setup,
                ))],
            ),
            Err(error) => {
                document.notice = Some(error.to_string());
                false
            }
        }
    }
}

impl SetupRequest {
    fn check(&self, plugins: &ScientificPlugins, limits: &Limits) -> Result<(), EffectError> {
        use orishu_plugin::execution::ConfigurationInput;
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
            if input.configuration.len() > orishu_plugin::Limits::default().max_schema_items
                || input.quality_flags.len() > 64
            {
                return Err(EffectError::Limit);
            }
            for p in &input.configuration {
                if match &p.input {
                    ConfigurationInput::Expression { source } => {
                        source.len() > limits.max_expression_bytes
                    }
                    ConfigurationInput::Text { value } => value.len() > limits.max_text_bytes,
                    _ => false,
                } {
                    return Err(EffectError::Limit);
                }
            }
        }
        Ok(())
    }
}

fn configure(
    plugins: ScientificPlugins,
    snapshot: ExperimentSnapshot,
    request: SetupRequest,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    use orishu_plugin::execution::{Batch, BulkLimits, BulkRecord, DynamicEntity};
    let check = || control.check().map_err(|_| EffectError::Cancelled);
    check()?;
    let selected = match plugins
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
        PrepareSelectionOutcome::Ready(p) => *p,
        PrepareSelectionOutcome::Unresolved(outcome) => {
            return Err(EffectError::Resolution(Box::new(outcome)));
        }
    };
    let variables =
        kagami_document::variable_context(&snapshot, &limits).map_err(|_| EffectError::Document)?;
    check()?;
    let initializer = LocalInitializer::new(
        Sandbox::new(SandboxLimits::default()).map_err(|_| EffectError::Executor)?,
        InitializationLimits {
            // Reserve a whole candidate's opaque output capacity before any guest:
            // many small fields remain possible without multiplying 16 MiB by 65.
            state_bytes: (SETUP_OUTPUT_BYTES / request.kernels.len()).min(OUTPUT_BYTES),
            ..Default::default()
        },
    );
    let mut captures = Vec::with_capacity(request.kernels.len());
    for kernel in &request.kernels {
        check()?;
        let input = request
            .initialization
            .iter()
            .find(|i| i.instance == kernel.instance_id)
            .ok_or(EffectError::Selection)?;
        let captured = match kernel.execution_contract {
            ExecutionContractId::Field => {
                initializer.field(&selected, input, &variables, control.clone())
            }
            ExecutionContractId::Dynamics => {
                let bulk = BulkLimits {
                    bytes: OUTPUT_BYTES,
                    records: limits.max_objects.min(65_536),
                };
                let entities = kagami_document::scientific::initial_dynamics(
                    &snapshot,
                    &selected.compiled().verified().declarations(),
                    &kernel.instance_id,
                    &variables,
                    &limits,
                    bulk,
                )
                .map_err(|_| EffectError::Document)?;
                let count = Batch::<DynamicEntity>::read(&entities, bulk)
                    .map_err(|_| EffectError::Document)?
                    .len();
                initializer.history(
                    &selected,
                    input,
                    &variables,
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
    check()?;
    let setup = ScientificSetup::capture(
        selected.compiled().verified(),
        request.initialization[0].domain.clone(),
        request.time_step,
        captures,
        limits.scientific,
    )
    .map_err(|_| EffectError::Document)?;
    let mut retained = ScientificRetention::new(
        INPUT_BYTES + SETUP_OUTPUT_BYTES + OUTPUT_BYTES + SELECTED_METADATA as usize,
    );
    if let Some(old) = snapshot.setup().scientific() {
        retained.include(old).map_err(|_| EffectError::Limit)?;
    }
    retained.include(&setup).map_err(|_| EffectError::Limit)?;
    check()?;
    let availability = plugins
        .store
        .guard_revision(plugins.revision)
        .map_err(EffectError::Inventory)?;
    Ok(Completed {
        setup,
        _availability: availability,
        _selected: selected,
    })
}

fn initialize(
    plugins: ScientificPlugins,
    snapshot: ExperimentSnapshot,
    instance: ComponentInstanceId,
    limits: Limits,
    control: OperationControl,
) -> Result<Completed, EffectError> {
    let check = || control.check().map_err(|_| EffectError::Cancelled);
    check()?;
    let setup = snapshot
        .setup()
        .scientific()
        .ok_or(EffectError::Unavailable)?;
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
    check()?;
    let capture = &setup.captures()[&instance];
    let variables =
        kagami_document::variable_context(&snapshot, &limits).map_err(|_| EffectError::Document)?;
    let request = InitializationRequest {
        instance: instance.clone(),
        domain: setup.domain().clone(),
        configuration: capture.authored.clone(),
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
    let captures = setup
        .captures()
        .iter()
        .map(|(id, capture)| {
            if *id == instance {
                initialized.document_capture()
            } else {
                capture.clone()
            }
        })
        .collect();
    let replacement = ScientificSetup::capture(
        selected.compiled().verified(),
        setup.domain().clone(),
        snapshot.setup().time_step(),
        captures,
        limits.scientific,
    )
    .map_err(|_| EffectError::Document)?;
    let mut retained =
        ScientificRetention::new(INPUT_BYTES + OUTPUT_BYTES + SELECTED_METADATA as usize);
    retained.include(setup).map_err(|_| EffectError::Limit)?;
    retained
        .include(&replacement)
        .map_err(|_| EffectError::Limit)?;
    check()?;
    // No guest can run after this read guard. A concurrent disable/update either
    // wins first and causes stale refusal, or gets Busy until adoption completes.
    let availability = plugins
        .store
        .guard_revision(plugins.revision)
        .map_err(EffectError::Inventory)?;
    Ok(Completed {
        setup: replacement,
        _availability: availability,
        _selected: selected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
