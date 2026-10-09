//! Client-local editing of an explicit complete scientific-setup proposal.
//! No kernel choice or domain change is silently inferred from a legacy file.
use crate::{
    document::{AuthoringGuard, Document},
    plugins::KernelChoice,
    scientific::InitializationRequest,
    scientific_effect::SetupRequest,
};
use orishu_plugin::{
    ExecutionContractId, FiniteF64,
    execution::{ComputePrecision, DOMAIN_SCHEMA, DomainDescriptor, SpatialDiscretization},
    resolution::{ProviderBinding, RequirementKey, ResolutionRequest},
    selected::SelectedKernel,
};
use std::collections::{BTreeMap, BTreeSet};

pub mod dependencies;
mod parameters;
pub use parameters::{ParameterAction, Parameters};

/// Small local form actions, not accepted document commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicsAction {
    /// Toggle an exact displayed kernel; Dynamics is single-choice.
    Kernel(usize),
    /// Edit x/y/z lower bound in metres.
    Lower(usize, String),
    /// Edit x/y/z upper bound in metres.
    Upper(usize, String),
    /// Choose explicit continuous vs Cartesian cell discretization.
    Grid(bool),
    /// Edit a positive Cartesian cell count.
    Cells(usize, String),
    /// Edit a fixed step in seconds.
    Step(String),
    /// Edit an exact selected kernel's declarative configuration input.
    Parameter(ParameterAction),
    /// Read-only resolution/paging or an explicit local provider choice.
    Dependencies(dependencies::Action),
    /// Copy exact captured settings into the local form without running code.
    LoadCaptured,
    /// Explicitly abandon form settings and pins; does not edit the experiment.
    Reset,
    /// Acknowledge replacing domain/physics and resetting initial state.
    Confirm(bool),
    /// Acknowledge resetting only this captured field's initial state.
    ConfirmField(usize, bool),
    /// Apply parameters only to this field, preserving other captures/history.
    ApplyField(usize),
    /// Propose initialization; never adopts any partial candidate.
    Apply,
}

/// In-progress text and choices; no revisions until Apply succeeds atomically.
pub struct PhysicsForm {
    pub selected: BTreeSet<usize>,
    pub lower: [String; 3],
    pub upper: [String; 3],
    pub grid: bool,
    pub cells: [String; 3],
    pub step: String,
    pub confirmed: bool,
    /// Bounded client-local explicit overrides, distinct from declared defaults.
    pub parameters: Parameters,
    captured: Option<SetupRequest>,
    captured_guard: Option<AuthoringGuard>,
    field_confirmed: Option<usize>,
    generation: uuid::Uuid,
    bindings: BTreeMap<RequirementKey, orishu_plugin::ContributionRef>,
    // Only a staged captured replacement may substitute proposed component
    // intent. Ordinary physics Apply refuses this form; the reset adapter owns it.
    component_intent: Option<std::sync::Arc<orishu_plugin::authoring_lock::SelectionLock>>,
}
impl Default for PhysicsForm {
    fn default() -> Self {
        Self {
            selected: BTreeSet::new(),
            lower: std::array::from_fn(|_| "-10".into()),
            upper: std::array::from_fn(|_| "10".into()),
            grid: false,
            cells: std::array::from_fn(|_| "16".into()),
            step: "0.01".into(),
            confirmed: false,
            parameters: Parameters::default(),
            captured: None,
            captured_guard: None,
            field_confirmed: None,
            generation: uuid::Uuid::new_v4(),
            bindings: BTreeMap::new(),
            component_intent: None,
        }
    }
}
impl PhysicsForm {
    pub(crate) fn for_component_reset(
        document: &Document,
        lock: std::sync::Arc<orishu_plugin::authoring_lock::SelectionLock>,
    ) -> Result<Self, &'static str> {
        let setup = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or("No captured setup to replace.")?;
        lock.validate(document.limits().dependencies)
            .map_err(|_| "Replacement choices exceed limits.")?;
        let domain = setup.domain();
        let mut form = Self {
            lower: domain.lower_metres.map(|v| number_text(v.get())),
            upper: domain.upper_metres.map(|v| number_text(v.get())),
            step: number_text(setup.time_step().seconds()),
            component_intent: Some(lock),
            ..Default::default()
        };
        if let SpatialDiscretization::CartesianCells { cells } = domain.discretization {
            form.grid = true;
            form.cells = cells.map(|n| n.to_string());
        }
        Ok(form)
    }

    pub(crate) fn generation(&self) -> uuid::Uuid {
        self.generation
    }

    /// Explicit local decisions only. Resolution must revalidate these pins;
    /// neither this form nor a candidate page grants execution permission.
    pub fn bindings(&self) -> impl Iterator<Item = ProviderBinding> + '_ {
        self.bindings
            .iter()
            .map(|(requirement, provider)| ProviderBinding {
                requirement: requirement.clone(),
                provider: provider.clone(),
            })
    }

    pub(crate) fn bind(&mut self, binding: ProviderBinding) -> Result<(), &'static str> {
        if self.is_captured() {
            return Err(
                "Captured dependency pins are immutable here; start a new proposal explicitly.",
            );
        }
        if !self.bindings.contains_key(&binding.requirement)
            && self.bindings.len()
                >= orishu_plugin::resolution::ResolutionLimits::default().max_bindings
        {
            return Err("Provider bindings exceed the form limit.");
        }
        self.bindings.insert(binding.requirement, binding.provider);
        self.confirmed = false;
        self.field_confirmed = None;
        self.generation = uuid::Uuid::new_v4();
        Ok(())
    }

    pub(crate) fn clear_bindings(&mut self) {
        self.bindings.clear();
        self.confirmed = false;
        self.field_confirmed = None;
        self.generation = uuid::Uuid::new_v4();
    }
    pub(crate) fn forget_binding(
        &mut self,
        requirement: &RequirementKey,
    ) -> Result<(), &'static str> {
        if self.is_captured() {
            return Err("Captured provider choices cannot be discarded here.");
        }
        self.bindings
            .remove(requirement)
            .ok_or("No such local binding.")?;
        self.confirmed = false;
        self.field_confirmed = None;
        self.generation = uuid::Uuid::new_v4();
        Ok(())
    }

    /// The same bounded selection used by read-only dependency inspection and
    /// Apply, including exact components from the current document. It does not
    /// require reset consent, validate numeric inputs, or execute guests.
    pub fn selection_with_scene(
        &self,
        choices: &[KernelChoice],
        revision: u64,
        snapshot: &kagami_document::ExperimentSnapshot,
    ) -> Result<ResolutionRequest, &'static str> {
        let mut selection = self.selection(choices, revision)?;
        let mut roots: BTreeSet<_> = selection.roots.into_iter().collect();
        let mut add_root = |pin: &orishu_plugin::ContributionRef| -> Result<(), &'static str> {
            if !roots.contains(pin) {
                if roots.len() == orishu_plugin::selected::SelectionLimits::default().contributions
                {
                    return Err("Scene contribution roots exceed the selection budget.");
                }
                roots.insert(pin.clone());
            }
            Ok(())
        };
        if let Some(lock) = &self.component_intent {
            for pin in &lock.selection().roots {
                add_root(pin)?;
            }
        } else {
            for object in snapshot.objects().values() {
                for kind in object.components.keys() {
                    add_root(kind.contribution().ok_or("Legacy components require explicit migration before configuring plugin physics.")?)?;
                }
            }
        }
        selection.roots = roots.into_iter().collect();
        if let Some(lock) = self
            .component_intent
            .as_ref()
            .or_else(|| snapshot.dependencies())
        {
            let mut bindings: BTreeMap<_, _> = selection
                .bindings
                .into_iter()
                .map(|b| (b.requirement, b.provider))
                .collect();
            for binding in &lock.selection().bindings {
                if bindings
                    .get(&binding.requirement)
                    .is_some_and(|p| p != &binding.provider)
                {
                    return Err(
                        "Physics proposal conflicts with saved component providers; change component intent explicitly first.",
                    );
                }
                if bindings.len()
                    == orishu_plugin::resolution::ResolutionLimits::default().max_bindings
                    && !bindings.contains_key(&binding.requirement)
                {
                    return Err("Combined component/physics bindings exceed the proposal budget.");
                }
                bindings.insert(binding.requirement.clone(), binding.provider.clone());
            }
            selection.bindings = bindings
                .into_iter()
                .map(|(requirement, provider)| ProviderBinding {
                    requirement,
                    provider,
                })
                .collect();
        }
        Ok(selection)
    }

    fn selection(
        &self,
        choices: &[KernelChoice],
        revision: u64,
    ) -> Result<ResolutionRequest, &'static str> {
        if let Some(captured) = &self.captured {
            if captured.selection.expected_inventory_revision != revision {
                return Err("Inventory changed; copy captured settings again.");
            }
            return Ok(captured.selection.clone());
        }
        if self.selected.is_empty() || self.selected.len() > 65 {
            return Err("Select one integrator and up to 64 fields.");
        }
        let mut roots = BTreeSet::new();
        for i in &self.selected {
            roots.insert(
                choices
                    .get(*i)
                    .ok_or("Model choices changed; refresh before retrying.")?
                    .contribution
                    .clone(),
            );
        }
        Ok(ResolutionRequest {
            expected_inventory_revision: revision,
            roots: roots.into_iter().collect(),
            bindings: self.bindings().collect(),
        })
    }
    /// Captured-provider pins and per-use policies are being preserved. Choosing
    /// different models requires an explicit new proposal, not silent rebinding.
    pub fn is_captured(&self) -> bool {
        self.captured.is_some()
    }

    /// Copy a coherent authoring revision and retain its asynchronous edit guard.
    pub fn from_document(
        document: &Document,
        choices: &[KernelChoice],
        revision: u64,
    ) -> Result<Self, &'static str> {
        let guard = document
            .authoring_guard()
            .ok_or("Return to Authoring before copying settings.")?;
        let setup = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or("No captured scientific setup to copy.")?;
        let mut form = Self::from_captured(setup, choices, revision)?;
        form.captured_guard = Some(guard);
        Ok(form)
    }

    /// Whether reset consent was given for precisely this field and these inputs.
    pub fn field_confirmed(&self, index: usize) -> bool {
        self.field_confirmed == Some(index)
    }

    /// Build a field-only proposal from the copied revision. Reject unrelated
    /// pending edits instead of silently dropping them or resetting extra state.
    pub fn field_request(
        &self,
        document: &Document,
        choices: &[KernelChoice],
        revision: u64,
        index: usize,
    ) -> Result<(AuthoringGuard, InitializationRequest), &'static str> {
        if !self.field_confirmed(index) {
            return Err("Confirm resetting this field's initial state first.");
        }
        let guard = self
            .captured_guard
            .filter(|guard| document.accepts_effect(*guard))
            .ok_or("The copied document context changed; copy captured settings again.")?;
        let choice = choices
            .get(index)
            .filter(|c| c.contract == ExecutionContractId::Field)
            .ok_or("Only a captured field supports a field-only parameter edit.")?;
        let setup = document
            .snapshot()
            .setup()
            .scientific()
            .ok_or("No captured scientific setup.")?;
        let request = self.build_request(choices, revision)?;
        let instance = request
            .kernels
            .iter()
            .find(|k| k.contribution == choice.contribution)
            .ok_or("Select a captured field.")?
            .instance_id
            .clone();
        if request.time_step != setup.time_step()
            || request
                .initialization
                .iter()
                .any(|input| &input.domain != setup.domain())
        {
            return Err(
                "Domain/timestep changes require the whole-setup reset; restore those inputs first.",
            );
        }
        if request.initialization.iter().any(|input| {
            input.instance != instance
                && setup
                    .captures()
                    .get(&input.instance)
                    .is_none_or(|c| c.authored != input.configuration)
        }) {
            return Err(
                "Other models have pending parameter edits; restore them before applying only this field.",
            );
        }
        let input = request
            .initialization
            .into_iter()
            .find(|i| i.instance == instance)
            .ok_or("No captured field input.")?;
        Ok((guard, input))
    }

    /// Copy saved settings only. Requires every exact model in this inventory;
    /// never initializes, resolves alternatives, or changes an accepted revision.
    pub fn from_captured(
        setup: &kagami_document::scientific::ScientificSetup,
        choices: &[KernelChoice],
        revision: u64,
    ) -> Result<Self, &'static str> {
        use orishu_plugin::resolution::{ProviderBinding, RequirementKey};
        let descriptor = setup.declarations().descriptor();
        if descriptor.kernel_instances.len() > 65 {
            return Err("Captured setup exceeds the form's kernel limit.");
        }
        let domain = setup.domain();
        let mut form = Self {
            lower: domain.lower_metres.map(|v| number_text(v.get())),
            upper: domain.upper_metres.map(|v| number_text(v.get())),
            step: number_text(setup.time_step().seconds()),
            ..Self::default()
        };
        if let SpatialDiscretization::CartesianCells { cells } = domain.discretization {
            form.grid = true;
            form.cells = cells.map(|v| v.to_string());
        }
        let mut initialization = Vec::new();
        for kernel in &descriptor.kernel_instances {
            let index = choices.iter().position(|c| c.contribution == kernel.contribution && c.contract == kernel.execution_contract)
                .ok_or("A captured model is unavailable in this inventory; no replacement was selected.")?;
            if !form.selected.insert(index) {
                return Err("The form cannot represent repeated uses of the same model.");
            }
            let capture = &setup.captures()[&kernel.instance_id];
            for input in &capture.authored {
                form.parameters.edit(
                    ParameterAction {
                        kernel: index,
                        property: input.id.clone(),
                        input: Some(input.input.clone()),
                    },
                    choices,
                    &form.selected,
                );
                if let Some(error) = form.parameters.error() {
                    return Err(error);
                }
            }
            initialization.push(InitializationRequest {
                instance: kernel.instance_id.clone(),
                domain: domain.clone(),
                configuration: vec![], // Parameters owns the bounded retained overrides.
                compute_precision: capture.context.compute_precision,
                quality_flags: capture
                    .context
                    .observables
                    .iter()
                    .map(|o| (o.slot.clone(), o.quality_flags))
                    .collect(),
            });
        }
        form.captured = Some(SetupRequest {
            selection: ResolutionRequest {
                expected_inventory_revision: revision,
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
            },
            kernels: descriptor.kernel_instances.clone(),
            initialization,
            time_step: setup.time_step(),
        });
        Ok(form)
    }

    /// Bounded local editing. Any changed choice requires fresh reset consent.
    pub fn edit(&mut self, action: PhysicsAction, choices: &[KernelChoice]) {
        let mut changed = true;
        match action {
            PhysicsAction::Kernel(i) if i < choices.len() && self.captured.is_none() => {
                // Changing roots can make prior slot bindings unreachable. Never
                // let them grant eligibility to an unrelated new proposal.
                self.bindings.clear();
                if !self.selected.remove(&i) {
                    if choices[i].contract == ExecutionContractId::Dynamics {
                        self.selected.retain(|n| {
                            choices
                                .get(*n)
                                .is_some_and(|c| c.contract != ExecutionContractId::Dynamics)
                        });
                    }
                    self.selected.insert(i);
                }
            }
            PhysicsAction::Lower(i, v) if i < 3 && v.len() <= 64 => self.lower[i] = v,
            PhysicsAction::Upper(i, v) if i < 3 && v.len() <= 64 => self.upper[i] = v,
            PhysicsAction::Cells(i, v) if i < 3 && v.len() <= 16 => self.cells[i] = v,
            PhysicsAction::Step(v) if v.len() <= 64 => self.step = v,
            PhysicsAction::Grid(value) => self.grid = value,
            PhysicsAction::Parameter(action) => {
                self.parameters.edit(action, choices, &self.selected);
            }
            PhysicsAction::Reset => *self = Self::default(),
            PhysicsAction::Confirm(value) => {
                self.confirmed = value;
                self.field_confirmed = None;
                changed = false;
            }
            PhysicsAction::ConfirmField(index, value) => {
                self.field_confirmed = value.then_some(index);
                self.confirmed = false;
                changed = false;
            }
            _ => changed = false,
        }
        if changed {
            self.confirmed = false;
            self.field_confirmed = None;
            self.generation = uuid::Uuid::new_v4();
        }
    }
    /// Convert explicit UI choices to the shared effect request. Omitted values
    /// retain declared defaults; explicit expressions/literals reach the shared
    /// compiler unchanged. Required values are never invented.
    pub fn request(
        &self,
        choices: &[KernelChoice],
        revision: u64,
    ) -> Result<SetupRequest, &'static str> {
        if self.component_intent.is_some() {
            return Err(
                "Use the explicit component replacement/full-reset consent, not ordinary physics Apply.",
            );
        }
        if !self.confirmed {
            return Err("Confirm replacing the domain/physics and resetting initial states first.");
        }
        self.build_request(choices, revision)
    }

    pub(crate) fn component_reset_request(
        &self,
        choices: &[KernelChoice],
        revision: u64,
    ) -> Result<SetupRequest, &'static str> {
        if self.component_intent.is_none() {
            return Err("Stage component replacement choices before configuring the reset.");
        }
        self.build_request(choices, revision)
    }

    fn build_request(
        &self,
        choices: &[KernelChoice],
        revision: u64,
    ) -> Result<SetupRequest, &'static str> {
        if self
            .lower
            .iter()
            .chain(&self.upper)
            .chain(&self.cells)
            .chain(std::iter::once(&self.step))
            .any(|s| s.len() > 64)
        {
            return Err("Physics form input exceeds its text limit.");
        }
        if self.selected.is_empty() || self.selected.len() > 65 {
            return Err("Select one integrator and up to 64 fields.");
        }
        let number = |text: &str| {
            text.parse::<f64>()
                .ok()
                .and_then(|v| FiniteF64::new(v).ok())
                .ok_or("Domain bounds must be finite numbers in metres.")
        };
        let lower = [
            number(&self.lower[0])?,
            number(&self.lower[1])?,
            number(&self.lower[2])?,
        ];
        let upper = [
            number(&self.upper[0])?,
            number(&self.upper[1])?,
            number(&self.upper[2])?,
        ];
        let discretization = if self.grid {
            let count = |s: &str| {
                s.parse::<u32>()
                    .map_err(|_| "Grid counts must be positive integers.")
            };
            SpatialDiscretization::CartesianCells {
                cells: [
                    count(&self.cells[0])?,
                    count(&self.cells[1])?,
                    count(&self.cells[2])?,
                ],
            }
        } else {
            SpatialDiscretization::Continuous
        };
        let domain = DomainDescriptor {
            api_version: DOMAIN_SCHEMA.parse().expect("static schema"),
            lower_metres: lower,
            upper_metres: upper,
            discretization,
        };
        domain
            .validate(Default::default())
            .map_err(|_| "Domain extents/grid are invalid or exceed limits.")?;
        let time_step = kagami_document::TimeStep::new(
            self.step
                .parse::<f64>()
                .map_err(|_| "Timestep must be a positive number in seconds.")?,
        )
        .map_err(|_| "Timestep must be finite and positive.")?;
        let mut kernels = Vec::new();
        let mut initialization = Vec::new();
        for (n, i) in self.selected.iter().enumerate() {
            let choice = choices
                .get(*i)
                .ok_or("Model choices changed; refresh before retrying.")?;
            let captured_use = self.captured.as_ref().and_then(|s| {
                s.kernels
                    .iter()
                    .find(|k| k.contribution == choice.contribution)
            });
            if self.captured.is_some() && captured_use.is_none() {
                return Err("Captured choices changed; start a new proposal explicitly.");
            }
            let instance: orishu_workload::ComponentInstanceId = captured_use
                .map(|k| k.instance_id.clone())
                .unwrap_or_else(|| {
                    format!("kernel-{n:03}")
                        .parse()
                        .expect("bounded generated instance ID")
                });
            let original = self
                .captured
                .as_ref()
                .and_then(|s| s.initialization.iter().find(|i| i.instance == instance));
            kernels.push(SelectedKernel {
                instance_id: instance.clone(),
                contribution: choice.contribution.clone(),
                execution_contract: choice.contract,
            });
            initialization.push(InitializationRequest {
                instance,
                domain: domain.clone(),
                configuration: self.parameters.inputs(*i, choice)?,
                compute_precision: original
                    .map_or(ComputePrecision::Binary64, |i| i.compute_precision),
                quality_flags: original
                    .map(|i| i.quality_flags.clone())
                    .unwrap_or_else(|| {
                        choice
                            .observable_slots
                            .iter()
                            .map(|s| (s.clone(), 1))
                            .collect()
                    }),
            });
        }
        if kernels
            .iter()
            .filter(|k| k.execution_contract == ExecutionContractId::Dynamics)
            .count()
            != 1
        {
            return Err("Select exactly one dynamics integrator.");
        }
        if self
            .captured
            .as_ref()
            .is_some_and(|s| s.kernels.len() != kernels.len())
        {
            return Err("Captured choices changed; start a new proposal explicitly.");
        }
        kernels.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        let selection = self.selection(choices, revision)?;
        Ok(SetupRequest {
            selection,
            kernels,
            initialization,
            time_step,
        })
    }
}

// Rust's Display can expand tiny/large finite floats beyond the input's 64-byte
// ceiling. Both shortest formats round-trip; exponent notation stays bounded.
fn number_text(value: f64) -> String {
    let ordinary = value.to_string();
    if ordinary.len() <= 64 {
        ordinary
    } else {
        format!("{value:e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_plugin::ContributionRef;

    #[test]
    fn captured_finite_numbers_fit_the_form_and_round_trip_exactly() {
        for value in [
            0.0,
            -0.0,
            0.001,
            f64::MAX,
            -f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            1e-100,
            1e100,
        ] {
            let text = number_text(value);
            assert!(text.len() <= 64);
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
    }

    fn choices() -> Vec<KernelChoice> {
        (0..13)
            .map(|n| KernelChoice {
                contribution: ContributionRef {
                    release: format!("sha256:{}", "00".repeat(32)).parse().unwrap(),
                    extension_point: "orishu.models/v1".parse().unwrap(),
                    local_id: format!("model-{n}").parse().unwrap(),
                },
                contract: if n < 2 {
                    ExecutionContractId::Dynamics
                } else {
                    ExecutionContractId::Field
                },
                label: format!("model-{n}"),
                observable_slots: vec![],
                configuration: vec![],
            })
            .collect()
    }

    #[test]
    fn explicit_choices_have_canonical_ids_and_one_integrator() {
        let choices = choices();
        let mut form = PhysicsForm::default();
        for n in 0..choices.len() {
            form.edit(PhysicsAction::Kernel(n), &choices);
        }
        assert!(!form.selected.contains(&0));
        form.edit(PhysicsAction::Confirm(true), &choices);
        let request = form.request(&choices, 7).unwrap();
        assert_eq!(request.selection.expected_inventory_revision, 7);
        assert_eq!(request.kernels.len(), 12);
        assert!(
            request
                .kernels
                .windows(2)
                .all(|p| p[0].instance_id < p[1].instance_id)
        );
        assert!(request.selection.roots.windows(2).all(|p| p[0] < p[1]));
        assert!(request.selection.bindings.is_empty());
        form.edit(PhysicsAction::Kernel(1), &choices);
        assert!(!form.confirmed);
        form.edit(PhysicsAction::Confirm(true), &choices);
        assert!(form.request(&choices, 7).is_err());
    }

    #[test]
    fn rejects_invalid_or_unbounded_domain_input_without_changing_intent() {
        let choices = choices();
        let mut form = PhysicsForm::default();
        form.edit(PhysicsAction::Kernel(0), &choices);
        form.edit(PhysicsAction::Confirm(true), &choices);
        let original = form.request(&choices, 1).unwrap();
        form.edit(PhysicsAction::Lower(3, "4".into()), &choices);
        form.edit(PhysicsAction::Step("1".repeat(65)), &choices);
        assert_eq!(form.request(&choices, 1).unwrap(), original);
        for bad in ["NaN", "inf", "0", "-1"] {
            form.edit(PhysicsAction::Step(bad.into()), &choices);
            assert!(!form.confirmed);
            form.edit(PhysicsAction::Confirm(true), &choices);
            assert!(form.request(&choices, 1).is_err());
        }
        form.edit(PhysicsAction::Step("0.01".into()), &choices);
        form.edit(PhysicsAction::Grid(true), &choices);
        for bad in ["0", "4294967295", "-1", "1.5"] {
            form.edit(PhysicsAction::Cells(0, bad.into()), &choices);
            form.edit(PhysicsAction::Confirm(true), &choices);
            assert!(form.request(&choices, 1).is_err());
        }
        form.cells[0] = "1".repeat(65);
        assert!(form.request(&choices, 1).is_err());
    }
}
