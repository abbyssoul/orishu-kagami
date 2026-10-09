//! Exact local template/instance intent; selection never edits an experiment.
use super::*;
use orishu_plugin::{
    ContributionRef,
    resolution::{ProviderBinding, RequirementKey, ResolutionLimits, ResolutionRequest, Selection},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
/// Bounded instance text. Coordinates are explicit SI scalars; parameters retain
/// expression source. `None` restores a parameter's authored template default.
pub enum Input {
    Name(String),
    Position {
        axis: usize,
        metres: String,
    },
    Velocity {
        axis: usize,
        metres_per_second: String,
    },
    Parameter {
        name: kagami_catalog::ParameterName,
        source: Option<String>,
    },
}

/// A selected immutable template and transient values/choices, never an edit.
pub struct Form {
    input_token: Uuid,
    pub(crate) generation: Uuid,
    pub(crate) guard: Option<AuthoringGuard>,
    pub(crate) source: Option<Arc<CatalogSet>>,
    pub(crate) spec: Option<kagami_session::InstantiationSpec>,
    pub name: String,
    pub position: [String; 3],
    pub velocity: [String; 3],
    roots: Vec<ContributionRef>,
    base: BTreeMap<RequirementKey, ContributionRef>,
    choices: BTreeMap<RequirementKey, ContributionRef>,
    frozen: BTreeSet<ContributionRef>,
    locked_bindings: BTreeMap<RequirementKey, ContributionRef>,
    pub captured: bool,
    pub(crate) consent: Option<Uuid>,
}
impl Default for Form {
    fn default() -> Self {
        Self {
            input_token: Uuid::new_v4(),
            generation: Uuid::new_v4(),
            guard: None,
            source: None,
            spec: None,
            name: String::new(),
            position: std::array::from_fn(|_| "0".into()),
            velocity: std::array::from_fn(|_| "0".into()),
            roots: vec![],
            base: BTreeMap::new(),
            choices: BTreeMap::new(),
            frozen: BTreeSet::new(),
            locked_bindings: BTreeMap::new(),
            captured: false,
            consent: None,
        }
    }
}
impl Form {
    /// Changes with every input/provider edit; binds consent and resolver reports.
    pub fn token(&self) -> Uuid {
        self.generation
    }
    /// Stable while editing this instance; distinct from consent/report generation.
    pub fn input_token(&self) -> Uuid {
        self.input_token
    }
    /// Content the author selected; revalidation cannot silently upgrade it.
    pub fn fingerprint(&self) -> Option<kagami_catalog::ContentFingerprint> {
        self.spec.as_ref()?.expected_fingerprint
    }
    /// Whether the exact original document incarnation/revision/mode still holds.
    pub fn current(&self, document: &Document) -> bool {
        self.guard.is_some_and(|g| document.accepts_effect(g))
    }
    /// Structurally valid source, which may still require availability resolution.
    pub fn template(&self) -> Option<&kagami_catalog::Template> {
        self.source
            .as_ref()?
            .get(&self.spec.as_ref()?.template)?
            .result
            .template()
    }
    /// Explicit expressions only; absent parameters use the source's defaults.
    pub fn overrides(&self) -> Option<&BTreeMap<kagami_catalog::ParameterName, String>> {
        Some(&self.spec.as_ref()?.bindings)
    }
    /// Consent for these exact current inputs, not another proposal.
    pub fn confirmed(&self) -> bool {
        self.consent == Some(self.generation)
    }
    /// Effective bounded choices for presentation/resolution; never adoption.
    pub fn bindings(&self) -> impl Iterator<Item = ProviderBinding> + '_ {
        let mut bindings = self.base.clone();
        bindings.extend(self.choices.clone());
        bindings
            .into_iter()
            .map(|(requirement, provider)| ProviderBinding {
                requirement,
                provider,
            })
    }
    pub(crate) fn select(
        document: &Document,
        source: Arc<CatalogSet>,
        index: usize,
    ) -> Result<Self, String> {
        let guard = document
            .authoring_guard()
            .ok_or("Template creation requires Authoring mode.")?;
        let entry = source
            .entries()
            .get(index)
            .ok_or("Template row is no longer available.")?;
        let template = entry
            .result
            .template()
            .ok_or("Repair the invalid template file before selecting it.")?;
        let spec = kagami_session::InstantiationSpec::new(
            template.identity.clone(),
            kagami_document::DisplayName::new(template.identity.template.as_str())
                .map_err(|e| e.to_string())?,
        )
        .expecting(
            entry
                .fingerprint
                .ok_or("Template has no content fingerprint.")?,
        );
        let mut roots = BTreeSet::new();
        for component in document
            .snapshot()
            .objects()
            .values()
            .flat_map(|o| o.components.keys())
            .chain(template.spec.components.iter().map(|c| &c.type_id))
        {
            if let Some(pin) = component.contribution() {
                if !roots.contains(pin) && roots.len() >= ResolutionLimits::default().max_roots {
                    return Err("Component roots exceed the discovery budget.".into());
                }
                roots.insert(pin.clone());
            } else if document.snapshot().setup().scientific().is_some() {
                return Err("Captured creation requires exact component pins; migrate legacy template/component IDs explicitly.".into());
            }
        }
        let limits = document.limits().dependencies;
        let mut retained = document.snapshot().dependencies().cloned();
        if let Some(incoming) = &template.dependencies {
            retained = Some(match retained {
                Some(old) => Arc::new(old.merge(incoming, limits).map_err(|e| e.to_string())?),
                None => incoming.clone(),
            });
        }
        let captured = document.snapshot().setup().scientific();
        let mut frozen = BTreeSet::new();
        let mut locked_bindings = BTreeMap::new();
        if let Some(setup) = captured {
            let d = setup.declarations().descriptor();
            let complete = SelectionLock::new(
                Selection {
                    roots: d.roots.clone(),
                    contributions: d.contributions.clone(),
                    bindings: d
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
                limits,
            )
            .map_err(|e| e.to_string())?;
            if let Some(required) = &retained {
                // A template cannot rebind any captured consumer, including
                // kernel/family declarations not yet reachable from its roots.
                complete
                    .merge(required, limits)
                    .map_err(|e| e.to_string())?;
            }
            frozen.extend(complete.selection().contributions.iter().cloned());
            locked_bindings.extend(
                complete
                    .selection()
                    .bindings
                    .iter()
                    .map(|b| (b.requirement.clone(), b.provider.clone())),
            );
            let used: Vec<_> = roots
                .iter()
                .filter(|r| complete.selection().contributions.binary_search(r).is_ok())
                .cloned()
                .collect();
            let component_part = complete
                .for_roots(&used, limits)
                .map_err(|e| e.to_string())?;
            retained = Some(Arc::new(match retained {
                Some(old) => old
                    .merge(&component_part, limits)
                    .map_err(|e| e.to_string())?,
                None => component_part,
            }));
        }
        let mut form = Self {
            guard: Some(guard),
            name: spec.name.as_str().into(),
            captured: captured.is_some(),
            roots: roots.into_iter().collect(),
            frozen,
            locked_bindings,
            ..Default::default()
        };
        if let Some(lock) = retained {
            lock.validate(limits).map_err(|e| e.to_string())?;
            form.base = lock
                .selection()
                .bindings
                .iter()
                .map(|b| (b.requirement.clone(), b.provider.clone()))
                .collect();
            form.frozen
                .extend(lock.selection().contributions.iter().cloned());
            form.locked_bindings.extend(form.base.clone());
        }
        form.source = Some(source);
        form.spec = Some(spec);
        Ok(form)
    }
    pub(crate) fn edit(
        &mut self,
        document: &Document,
        token: Uuid,
        input: Input,
    ) -> Result<(), String> {
        if token != self.input_token || !self.current(document) {
            return Err("Template proposal is stale; select it again.".into());
        }
        match input {
            Input::Name(value) => {
                if value.len() > 256 {
                    return Err("Object name exceeds the input budget.".into());
                }
                self.name = value;
            }
            Input::Position { axis, metres }
            | Input::Velocity {
                axis,
                metres_per_second: metres,
            } if axis >= 3 || metres.len() > 64 => {
                return Err("Invalid axis or oversized SI coordinate.".into());
            }
            Input::Position { axis, metres } => self.position[axis] = metres,
            Input::Velocity {
                axis,
                metres_per_second,
            } => self.velocity[axis] = metres_per_second,
            Input::Parameter { name, source } => {
                if !self
                    .template()
                    .is_some_and(|t| t.spec.parameters.contains_key(&name))
                {
                    return Err("Parameter is not declared by the selected template.".into());
                }
                if source.as_ref().is_some_and(|s| {
                    s.len()
                        > document
                            .limits()
                            .max_expression_bytes
                            .min(super::LIMITS.max_expression_bytes)
                }) {
                    return Err("Parameter source exceeds the expression budget.".into());
                }
                let bindings = &mut self.spec.as_mut().expect("selected").bindings;
                match source {
                    Some(source) => {
                        bindings.insert(name, source);
                    }
                    None => {
                        bindings.remove(&name);
                    }
                }
            }
        }
        self.changed();
        Ok(())
    }
    fn changed(&mut self) {
        self.generation = Uuid::new_v4();
        self.consent = None;
    }
    pub(crate) fn request(&self) -> Result<kagami_session::InstantiationSpec, String> {
        let mut spec = self.spec.clone().ok_or("Select a template first.")?;
        spec.name = kagami_document::DisplayName::new(&self.name).map_err(|e| e.to_string())?;
        let vector = |values: &[String; 3]| -> Result<kagami_document::Vector3, String> {
            let mut xyz = [0.0; 3];
            for (n, source) in xyz.iter_mut().zip(values) {
                *n = source.parse::<f64>().map_err(
                    |_| "Enter finite numeric SI coordinates (metres or metres/second).",
                )?;
            }
            kagami_document::Vector3::new(xyz[0], xyz[1], xyz[2]).map_err(|e| e.to_string())
        };
        spec.transform = kagami_document::Transform::at(vector(&self.position)?);
        spec.velocity = kagami_document::Velocity {
            linear: vector(&self.velocity)?,
            angular: kagami_document::Vector3::ZERO,
        };
        Ok(spec)
    }
}
impl dependencies::Proposal for Form {
    fn generation(&self) -> Uuid {
        self.generation
    }
    fn is_captured(&self) -> bool {
        false
    }
    fn bind(&mut self, binding: ProviderBinding) -> Result<(), &'static str> {
        if self.frozen.contains(&binding.requirement.consumer)
            && self.locked_bindings.get(&binding.requirement) != Some(&binding.provider)
        {
            return Err(
                "Template or experiment choices are fixed; creation cannot replace an existing provider.",
            );
        }
        if self.choices.len() >= ResolutionLimits::default().max_bindings
            && !self.choices.contains_key(&binding.requirement)
        {
            return Err("Provider choices exceed the proposal budget.");
        }
        self.choices.insert(binding.requirement, binding.provider);
        self.changed();
        Ok(())
    }
    fn clear_bindings(&mut self) {
        self.choices.clear();
        self.changed();
    }
    fn forget_binding(&mut self, requirement: &RequirementKey) -> Result<(), &'static str> {
        if self.base.contains_key(requirement) {
            return Err("Accepted template/document bindings cannot be discarded.");
        }
        if self.choices.remove(requirement).is_none() {
            return Err("No such local binding.");
        }
        self.changed();
        Ok(())
    }
    fn selection_with_scene(
        &self,
        _: &[crate::plugins::KernelChoice],
        revision: u64,
        document: &Document,
    ) -> Result<ResolutionRequest, &'static str> {
        if !self.current(document) {
            return Err("Template proposal is stale; select it again.");
        }
        Ok(ResolutionRequest {
            expected_inventory_revision: revision,
            roots: self.roots.clone(),
            bindings: self.bindings().collect(),
        })
    }
}
