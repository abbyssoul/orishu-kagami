//! Immutable authored scientific setup and captured opaque kernel state.
//!
//! Construction checks exact declarations and byte identities, never executes a
//! kernel. The document transition also checks configuration expressions and
//! captured history against the final candidate. Numerical validation remains a
//! runtime obligation; an accepted edit is not permission to run a workload.
use crate::{TimeStep, model::ExperimentState};
use orishu_plugin::{
    execution::*,
    selected::{SelectionDescriptor, SelectionLimits, VerifiedDeclarations, VerifiedSelection},
    *,
};
use orishu_variables::VariablesSystem;
use orishu_workload::ComponentInstanceId;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
mod dynamics;
pub use dynamics::initial_dynamics;

/// Owned per-setup limits plus the session's independent aggregate retention
/// ceiling. Constructors/transitions check the former; the authority checks the
/// latter across all state and receipts it will retain after acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScientificLimits {
    /// Maximum configured uses, including the integrator.
    pub kernels: usize,
    /// Maximum one opaque state/history or history-source packet.
    pub blob_bytes: usize,
    /// Aggregate captured bytes, counting repeated references conservatively.
    pub total_bytes: usize,
    /// Unique retained scientific buffers plus canonical metadata weight across
    /// one authority's current state, undo/redo and command replay receipts.
    /// Separate from per-candidate limits and from shell pending-effect memory.
    pub retained_bytes: usize,
}
impl Default for ScientificLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
impl ScientificLimits {
    /// Default interactive document policy; not a numerical layout specification.
    pub const DEFAULT: Self = Self {
        kernels: 65,
        blob_bytes: 128 * 1024 * 1024,
        total_bytes: 512 * 1024 * 1024,
        retained_bytes: 512 * 1024 * 1024,
    };
}

/// A capture supplied by an initialization effect, not yet accepted document data.
#[derive(Clone, Debug, PartialEq)]
pub struct KernelCapture {
    /// Exact selected scientific context.
    pub context: InstanceContext,
    /// Authored expressions/literals; defaults belong to the exact pinned schema.
    pub authored: Vec<AuthoredConfigurationProperty>,
    /// Canonical resolved configuration bytes used by the initialization.
    pub configuration: Arc<[u8]>,
    /// Complete portable kernel-owned state/history.
    pub state: Arc<[u8]>,
    /// Logical state value count supplied by the kernel.
    pub state_values: u64,
    /// Exact initial Dynamics packet for history. Must be absent for fields.
    pub history_entities: Option<Arc<[u8]>>,
}

/// Stable, bounded reason an initialization result cannot become authored state.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScientificError {
    /// Current state/history and a new receipt cannot fit the authority budget.
    #[error("scientific retention budget exceeded; release history or reduce captured state")]
    Retention,
    /// Structural bounds were exceeded.
    #[error("scientific setup exceeds its document budget")]
    Limit,
    /// Exact declarations or identities disagree.
    #[error("scientific capture does not match its selected context or inputs")]
    Mismatch,
    /// Shared declaration/codec rejected metadata.
    #[error(transparent)]
    Declaration(Box<orishu_plugin::Error>),
    /// Final authored parameters differ from the captured inputs.
    #[error("changed scientific parameters require atomic reinitialization")]
    Reinitialize,
    /// Initial objects no longer match numerical-history initialization.
    #[error("changed dynamic objects require updated initial integrator history")]
    History,
}
impl From<orishu_plugin::Error> for ScientificError {
    fn from(value: orishu_plugin::Error) -> Self {
        Self::Declaration(Box::new(value))
    }
}
impl ScientificError {
    /// Stable machine-readable rejection category.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Retention => "scientific_retention_limit",
            Self::Limit => "scientific_setup_limit",
            Self::Mismatch => "scientific_capture_mismatch",
            Self::Declaration(_) => "scientific_declaration_invalid",
            Self::Reinitialize => "scientific_reinitialization_required",
            Self::History => "scientific_history_changed",
        }
    }
}

/// Small serializable read projection. Bytes are separate immutable blobs, never
/// giant JSON arrays. This projection cannot itself construct accepted setup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KernelDescription {
    /// Exact context, including domain/configuration identities.
    pub context: InstanceContext,
    /// Retained authored configuration.
    pub authored: Vec<AuthoredConfigurationProperty>,
    /// Portable captured state identity.
    pub state: InputIdentity,
    /// History-source projection identity, absent for fields.
    pub history_entities: Option<InputIdentity>,
}
/// Plugin-based setup description, distinct from the legacy global boundary grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScientificDescription {
    /// Explicit discriminator for read projections; not the experiment file version.
    pub api_version: String,
    /// Authored fixed timestep.
    pub time_step: TimeStep,
    /// Geometric region/discretization, without a global physical boundary policy.
    pub domain: DomainDescriptor,
    /// Exact provider bindings and configured uses, never installation defaults.
    pub selection: SelectionDescriptor,
    /// Captured configured kernels, in instance-ID order.
    pub kernels: Vec<KernelDescription>,
}

/// Validated immutable captured setup. Clones share large buffers; undo restores
/// bytes, not an initialization recipe. Selection/declarations cannot be changed
/// independently of captured inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct ScientificSetup {
    time_step: TimeStep,
    domain: DomainDescriptor,
    selection: Arc<SelectionDescriptor>,
    payloads: Arc<BTreeMap<ContributionRef, Payload>>,
    declarations: Arc<VerifiedDeclarations>,
    captures: Arc<BTreeMap<ComponentInstanceId, KernelCapture>>,
    descriptions: Arc<Vec<KernelDescription>>,
    captured_bytes: usize,
    retained_metadata_bytes: usize,
}
impl ScientificSetup {
    /// Assemble one atomic setup from verified selection and initialization
    /// results. No entity can enter field initialization through this interface.
    /// Current document expressions/history are checked again on adoption.
    pub fn capture(
        selected: &VerifiedSelection,
        domain: DomainDescriptor,
        time_step: TimeStep,
        captures: Vec<KernelCapture>,
        limits: ScientificLimits,
    ) -> Result<Self, ScientificError> {
        Self::restore(
            &selected.declarations(),
            domain,
            time_step,
            captures,
            limits,
        )
    }
    /// Restore exact captured state using verified retained declaration evidence.
    /// Executable availability is deliberately not required to reopen a draft;
    /// export and execution must independently verify the complete code closure.
    pub fn restore(
        selected: &VerifiedDeclarations,
        domain: DomainDescriptor,
        time_step: TimeStep,
        captures: Vec<KernelCapture>,
        limits: ScientificLimits,
    ) -> Result<Self, ScientificError> {
        selected.descriptor().validate(SelectionLimits::default())?;
        if captures.len() > limits.kernels {
            return Err(ScientificError::Limit);
        }
        if captures.len() != selected.descriptor().kernel_instances.len() {
            return Err(ScientificError::Mismatch);
        }
        let domain_bytes = domain.to_cbor(DomainLimits::default())?;
        let domain_id = InputIdentity::of(
            DOMAIN_SCHEMA.parse().expect("static schema"),
            1,
            &domain_bytes,
        );
        let mut total = domain_bytes.len();
        if total > limits.total_bytes {
            return Err(ScientificError::Limit);
        }
        let mut values = BTreeMap::new();
        let mut descriptions = BTreeMap::new();
        let mut dynamics = 0;
        let mut families = std::collections::BTreeSet::new();
        let mut metadata_bytes = 0usize;
        for payload in selected.payloads().values() {
            metadata_bytes = metadata_bytes
                .checked_add(
                    payload
                        .canonical_bytes(&orishu_plugin::Limits::default())?
                        .len(),
                )
                .filter(|n| *n <= 32 * 1024 * 1024)
                .ok_or(ScientificError::Limit)?;
        }
        // The setup currently retains a separate payload/selection projection
        // beside its declaration witness, and a context in each read description.
        // Charge their canonical weights once per shared capture-map allocation.
        let mut retained_metadata_bytes = usize::try_from(selected.metadata_bytes())
            .ok()
            .and_then(|n| n.checked_add(metadata_bytes))
            .and_then(|n| n.checked_add(domain_bytes.len()))
            .ok_or(ScientificError::Limit)?;
        retained_metadata_bytes = retained_metadata_bytes
            .checked_add(
                selected
                    .descriptor()
                    .to_cbor(SelectionLimits::default())?
                    .len()
                    .saturating_mul(2),
            )
            .ok_or(ScientificError::Limit)?;
        for capture in captures {
            let c = &capture.context;
            retained_metadata_bytes = retained_metadata_bytes
                .checked_add(c.to_cbor()?.len().saturating_mul(2))
                .ok_or(ScientificError::Limit)?;
            // Charge before hashing/scanning caller-owned opaque bytes.
            for bytes in [&capture.configuration, &capture.state]
                .into_iter()
                .chain(capture.history_entities.iter())
            {
                if bytes.len() > limits.blob_bytes {
                    return Err(ScientificError::Limit);
                }
                total = total
                    .checked_add(bytes.len())
                    .filter(|n| *n <= limits.total_bytes)
                    .ok_or(ScientificError::Limit)?;
            }
            selected.verify_context(c, &orishu_plugin::Limits::default())?;
            if values.contains_key(&c.instance)
                || c.domain != domain_id
                || !c
                    .configuration
                    .matches(CONFIGURATION_SCHEMA, 1, &capture.configuration)
                || capture.state.len() as u64 > c.bounds.state_bytes
            {
                return Err(ScientificError::Mismatch);
            }
            let payload = &selected.payloads()[&c.contribution];
            let properties = match payload {
                Payload::FieldModels(model) => {
                    if capture.history_entities.is_some() {
                        return Err(ScientificError::Mismatch);
                    }
                    let family = selected
                        .descriptor()
                        .bindings
                        .iter()
                        .find(|b| {
                            b.consumer == c.contribution
                                && b.requirement_slot == model.scientific.field
                        })
                        .expect("verified model family");
                    if !families.insert(family.exact_contract.clone()) {
                        return Err(ScientificError::Mismatch);
                    }
                    &model.scientific.configuration
                }
                Payload::Integrators(model) => {
                    dynamics += 1;
                    if dynamics > 1 || capture.history_entities.is_none() {
                        return Err(ScientificError::Mismatch);
                    }
                    &model.scientific.configuration
                }
                _ => return Err(ScientificError::Mismatch),
            };
            ResolvedConfiguration::from_cbor(
                &capture.configuration,
                &orishu_plugin::Limits::default(),
            )?
            .validate_against(properties, &orishu_plugin::Limits::default())?;
            // Validate authored structure/source bounds now; value agreement is
            // checked using the document's actual variables at adoption.
            if capture.authored.len() > properties.len() {
                return Err(ScientificError::Limit);
            }
            for input in &capture.authored {
                let bytes = match &input.input {
                    ConfigurationInput::Expression { source } => source.len(),
                    ConfigurationInput::Text { value } => value.len(),
                    ConfigurationInput::Boolean { .. } => 0,
                };
                if bytes > orishu_plugin::Limits::default().max_text_bytes {
                    return Err(ScientificError::Limit);
                }
                retained_metadata_bytes = retained_metadata_bytes
                    .checked_add((bytes + input.id.as_str().len() + 64).saturating_mul(2))
                    .ok_or(ScientificError::Limit)?;
            }
            let history_entities = capture
                .history_entities
                .as_ref()
                .map(|bytes| {
                    let packet = Batch::<DynamicEntity>::read(
                        bytes,
                        BulkLimits {
                            bytes: limits.blob_bytes,
                            records: c.bounds.projection_records as usize,
                        },
                    )
                    .map_err(|_| ScientificError::History)?;
                    Ok::<_, ScientificError>(InputIdentity::of(
                        DynamicEntity::SCHEMA.parse().expect("static schema"),
                        packet.len() as u64,
                        bytes,
                    ))
                })
                .transpose()?;
            descriptions.insert(
                c.instance.clone(),
                KernelDescription {
                    context: c.clone(),
                    authored: capture.authored.clone(),
                    state: InputIdentity::of(
                        format!("{}/v{}", c.state_format.id, c.state_format.version)
                            .parse()
                            .map_err(|_| ScientificError::Mismatch)?,
                        capture.state_values,
                        &capture.state,
                    ),
                    history_entities,
                },
            );
            values.insert(c.instance.clone(), capture);
        }
        Ok(Self {
            time_step,
            domain,
            selection: Arc::new(selected.descriptor().clone()),
            payloads: Arc::new(selected.payloads().clone()),
            declarations: Arc::new(selected.clone()),
            captures: Arc::new(values),
            descriptions: Arc::new(descriptions.into_values().collect()),
            captured_bytes: total,
            retained_metadata_bytes,
        })
    }
    /// Current fixed authored step.
    pub const fn time_step(&self) -> TimeStep {
        self.time_step
    }
    pub(crate) fn set_time_step(&mut self, value: TimeStep) {
        self.time_step = value;
    }
    /// Immutable captured inputs, keyed by configured use.
    pub fn captures(&self) -> &BTreeMap<ComponentInstanceId, KernelCapture> {
        &self.captures
    }
    /// Exact retained vocabulary/membership evidence for self-contained saving.
    /// This witness proves no executable availability or runtime admission.
    pub fn declarations(&self) -> &VerifiedDeclarations {
        &self.declarations
    }
    /// Explicit captured geometric/discretization input, without cloning captures.
    pub fn domain(&self) -> &DomainDescriptor {
        &self.domain
    }
    /// Read projection with digests/descriptors, never inline opaque bytes.
    pub fn describe(&self) -> ScientificDescription {
        ScientificDescription {
            api_version: "kagami.scientific-setup/v1".into(),
            time_step: self.time_step,
            domain: self.domain.clone(),
            selection: (*self.selection).clone(),
            kernels: (*self.descriptions).clone(),
        }
    }
    /// Check a new owner's per-setup policy without trusting the policy used by
    /// the original constructor. Aggregate history/receipt admission is separate.
    pub fn check_limits(&self, limits: ScientificLimits) -> Result<(), ScientificError> {
        if self.captures.len() > limits.kernels
            || self.captured_bytes > limits.total_bytes
            || self.captures.values().any(|c| {
                [&c.configuration, &c.state]
                    .into_iter()
                    .chain(c.history_entities.iter())
                    .any(|b| b.len() > limits.blob_bytes)
            })
        {
            return Err(ScientificError::Limit);
        }
        Ok(())
    }
    pub(crate) fn validate_candidate(
        &self,
        state: &ExperimentState,
        variables: &VariablesSystem,
        limits: &crate::Limits,
    ) -> Result<(), ScientificError> {
        for capture in self.captures.values() {
            let (properties, dynamics) = match &self.payloads[&capture.context.contribution] {
                Payload::FieldModels(m) => (&m.scientific.configuration, None),
                Payload::Integrators(m) => {
                    (&m.scientific.configuration, Some(&m.scientific.dynamics))
                }
                _ => unreachable!("checked executable capture"),
            };
            let source_bound = |source: &str| -> Result<(), ScientificError> {
                if source.len() > limits.max_expression_bytes {
                    return Err(ScientificError::Limit);
                }
                // The shared resolver below supplies syntax diagnostics. This
                // pass additionally enforces the document's reference policy.
                if let Ok(expression) = orishu_variables::CompiledExpression::parse_bounded(
                    source,
                    &orishu_plugin::Limits::default().expressions,
                ) && expression.variables().len() > limits.max_expression_references
                {
                    return Err(ScientificError::Limit);
                }
                Ok(())
            };
            for input in &capture.authored {
                match &input.input {
                    ConfigurationInput::Expression { source } => source_bound(source)?,
                    ConfigurationInput::Text { value } if value.len() > limits.max_text_bytes => {
                        return Err(ScientificError::Limit);
                    }
                    _ => {}
                }
            }
            for property in properties {
                if capture.authored.iter().any(|v| v.id == property.id) {
                    continue;
                }
                match &property.schema {
                    PropertyType::Quantity {
                        default_expression: Some(source),
                        ..
                    } => source_bound(source)?,
                    PropertyType::Text {
                        default: Some(value),
                        ..
                    } if value.len() > limits.max_text_bytes => return Err(ScientificError::Limit),
                    _ => {}
                }
            }
            let configuration = resolve_configuration(
                properties,
                &capture.authored,
                variables,
                &orishu_plugin::Limits::default(),
            )?
            .to_cbor(&orishu_plugin::Limits::default())?;
            if configuration.as_slice() != &*capture.configuration {
                return Err(ScientificError::Reinitialize);
            }
            if dynamics.is_some() {
                self.check_history(capture, state, variables, limits)?;
            }
        }
        Ok(())
    }
    fn check_history(
        &self,
        capture: &KernelCapture,
        state: &ExperimentState,
        variables: &VariablesSystem,
        limits: &crate::Limits,
    ) -> Result<(), ScientificError> {
        let projection = dynamics::DynamicsProjection::new(
            state,
            &self.declarations,
            &capture.context.contribution,
        )?;
        let bytes = capture
            .history_entities
            .as_ref()
            .expect("history source checked");
        let packet = Batch::<DynamicEntity>::read(
            bytes,
            BulkLimits {
                bytes: bytes.len(),
                records: capture.context.bounds.projection_records as usize,
            },
        )
        .map_err(|_| ScientificError::History)?;
        let mut index = 0;
        projection.visit(variables, limits, |expected| {
            let actual = packet.get(index).ok_or(ScientificError::History)?;
            if actual != expected {
                return Err(ScientificError::History);
            }
            index += 1;
            Ok(())
        })?;
        if index != packet.len() {
            return Err(ScientificError::History);
        }
        Ok(())
    }
}

/// Ephemeral authority retention accounting. Allocation identity is used only
/// inside this process-local calculation, never as scientific or persisted ID.
/// Kept strong references prevent address reuse while the tally is alive.
/// This counts exact shared byte buffers and canonical metadata weights, not
/// allocator overhead, whole object graphs or the process's total RSS.
#[derive(Debug)]
pub struct ScientificRetention {
    limit: usize,
    bytes: usize,
    refused: bool,
    buffers: BTreeMap<usize, Arc<[u8]>>,
    captures: BTreeMap<usize, Arc<BTreeMap<ComponentInstanceId, KernelCapture>>>,
}
impl ScientificRetention {
    /// Start an empty bounded tally. Zero permits no scientific retained data.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            bytes: 0,
            refused: false,
            buffers: BTreeMap::new(),
            captures: BTreeMap::new(),
        }
    }
    /// Charge one setup, sharing both metadata and byte-buffer allocations with
    /// earlier inclusions. On refusal discard the tally; it is not transactional.
    pub fn include(&mut self, setup: &ScientificSetup) -> Result<(), ScientificError> {
        if self.refused {
            return Err(ScientificError::Retention);
        }
        let result = self.include_inner(setup);
        self.refused = result.is_err();
        result
    }
    fn include_inner(&mut self, setup: &ScientificSetup) -> Result<(), ScientificError> {
        let key = Arc::as_ptr(&setup.captures) as usize;
        if self.captures.contains_key(&key) {
            return Ok(());
        }
        self.charge(setup.retained_metadata_bytes)?;
        self.captures.insert(key, setup.captures.clone());
        for capture in setup.captures.values() {
            for bytes in [&capture.configuration, &capture.state]
                .into_iter()
                .chain(capture.history_entities.iter())
            {
                let key = Arc::as_ptr(bytes) as *const u8 as usize;
                if !self.buffers.contains_key(&key) {
                    self.charge(bytes.len())?;
                    self.buffers.insert(key, bytes.clone());
                }
            }
        }
        Ok(())
    }
    /// Current logical retained-byte weight, valid after successful inclusions.
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    fn charge(&mut self, bytes: usize) -> Result<(), ScientificError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= self.limit)
            .ok_or(ScientificError::Retention)?;
        Ok(())
    }
}
