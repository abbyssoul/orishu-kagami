//! Component choices and proposals; captured additions require history consent.
use crate::{
    document::{AuthoringGuard, Document},
    physics_form::dependencies::{self, Proposal},
    plugins::KernelChoice,
};
use orishu_plugin::{
    ContributionRef,
    resolution::{ProviderBinding, RequirementKey, ResolutionLimits, ResolutionRequest},
};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub mod properties;
pub mod reset;

/// Transient form actions; ApplyLock adopts uncaptured intent or hands a consented
/// captured addition to scientific preparation. Only final acceptance edits data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Explicitly copy current exact component roots and retained provider intent.
    Load,
    /// Discard the local proposal; a pending read retains its slot until done.
    Cancel,
    /// Consent belongs to these exact local inputs, not a later proposal.
    ConfirmHistory { proposal: Uuid, confirmed: bool },
    /// Full-reset consent names both component and physics proposal generations.
    ConfirmReset {
        component: Uuid,
        physics: Uuid,
        confirmed: bool,
    },
    /// Accept only the exact consented staged replacement and complete physics.
    ApplyReset { component: Uuid, physics: Uuid },
    /// Select the old component; choosing its replacement remains local.
    BeginReplacement {
        object: kagami_document::ObjectId,
        component: kagami_catalog::ComponentTypeId,
    },
    /// Choose the new exact component for the current replacement proposal.
    ReplaceWith {
        proposal: Uuid,
        component: kagami_catalog::ComponentTypeId,
    },
    /// Change a local attachment property, not the document.
    SetProperty {
        form: Uuid,
        property: kagami_catalog::PropertyName,
        value: properties::Input,
    },
    /// Reuse the shared bounded resolver, pages and guarded adoption lane.
    Dependencies(dependencies::Action),
}

/// No implicit tracking of a changing document. Reload is explicit after edits.
pub struct ComponentForm {
    generation: Uuid,
    guard: Option<AuthoringGuard>,
    roots: Vec<ContributionRef>,
    base: BTreeMap<RequirementKey, ContributionRef>,
    bindings: BTreeMap<RequirementKey, ContributionRef>,
    attachment: Option<(kagami_document::ObjectId, kagami_catalog::ComponentTypeId)>,
    properties: Option<properties::Properties>,
    replacement: Option<(kagami_document::ObjectId, kagami_catalog::ComponentTypeId)>,
    preserved: Option<std::sync::Arc<orishu_plugin::authoring_lock::SelectionLock>>,
    captured_addition: bool,
    captured_reset: bool,
    confirmed_history: Option<Uuid>,
    frozen: BTreeMap<RequirementKey, ContributionRef>,
}
impl Default for ComponentForm {
    fn default() -> Self {
        Self {
            generation: Uuid::new_v4(),
            guard: None,
            roots: vec![],
            base: BTreeMap::new(),
            bindings: BTreeMap::new(),
            attachment: None,
            properties: None,
            replacement: None,
            preserved: None,
            captured_addition: false,
            captured_reset: false,
            confirmed_history: None,
            frozen: BTreeMap::new(),
        }
    }
}
impl ComponentForm {
    /// Copy accepted intent without execution or adoption. Captured choices stage
    /// a complete scientific reset; uncaptured choices can be accepted directly.
    pub fn from_document(document: &Document) -> Result<Self, &'static str> {
        Self::prepare(document, None, None)
    }

    /// Stage an exact component and its complete dependency graph. The object
    /// is not modified until the checked graph and declared defaults are adopted.
    pub fn for_attachment(
        document: &Document,
        object: kagami_document::ObjectId,
        component: kagami_catalog::ComponentTypeId,
    ) -> Result<Self, &'static str> {
        let target = document
            .snapshot()
            .object(object)
            .ok_or("No such object.")?;
        if target.components.contains_key(&component) {
            return Err("The object already carries this component.");
        }
        if target.components.len() >= document.limits().max_components_per_object {
            return Err("The object's component budget is exhausted.");
        }
        component
            .contribution()
            .ok_or("Dependency attachment requires an exact component pin.")?;
        Self::prepare(document, Some((object, component)), None)
    }

    /// Begin explicit replacement without selecting a new component, copying
    /// old values or changing document state. Captured physics requires migration.
    pub fn begin_replacement(
        document: &Document,
        object: kagami_document::ObjectId,
        component: kagami_catalog::ComponentTypeId,
    ) -> Result<Self, &'static str> {
        let guard = document
            .authoring_guard()
            .ok_or("Replacement requires Authoring mode.")?;
        if !document
            .snapshot()
            .object(object)
            .is_some_and(|o| o.components.contains_key(&component))
        {
            return Err("The component to replace is no longer attached.");
        }
        Ok(Self {
            guard: Some(guard),
            replacement: Some((object, component)),
            captured_reset: document.snapshot().setup().scientific().is_some(),
            ..Default::default()
        })
    }

    /// Choose new defaults/values explicitly. No automatic data or scientific
    /// migration is inferred from similar property names or contract names.
    pub fn replace_with(
        &self,
        document: &Document,
        proposal: Uuid,
        component: kagami_catalog::ComponentTypeId,
    ) -> Result<Self, &'static str> {
        if !self.current(document) || proposal != self.generation {
            return Err("Replacement proposal is stale; select it again.");
        }
        let (object, previous) = self
            .replacement
            .as_ref()
            .ok_or("No replacement is being chosen.")?;
        let target = document
            .snapshot()
            .object(*object)
            .ok_or("No such object.")?;
        if !target.components.contains_key(previous) {
            return Err("The component to replace is no longer attached.");
        }
        if target.components.contains_key(&component) {
            return Err("Choose a different component not already attached to this object.");
        }
        component
            .contribution()
            .ok_or("Replacement requires an exact new component pin.")?;
        Self::prepare(
            document,
            Some((*object, component)),
            Some((*object, previous.clone())),
        )
    }

    /// Old component whose replacement is being chosen/prepared, never removal.
    pub fn replacement(
        &self,
    ) -> Option<&(kagami_document::ObjectId, kagami_catalog::ComponentTypeId)> {
        self.replacement.as_ref()
    }
    /// Token for replacement-choice buttons, invalidated by proposal changes.
    pub fn proposal_id(&self) -> Uuid {
        self.generation
    }

    /// Whether acceptance requires the coordinated scientific-extension lane.
    pub fn captured_addition(&self) -> bool {
        self.captured_addition
    }
    /// Replacement must be staged for explicit complete scientific reset.
    pub fn captured_reset(&self) -> bool {
        self.captured_reset
    }
    /// Consent is invalidated by every change to inputs or provider choices.
    pub fn history_confirmed(&self) -> bool {
        self.confirmed_history == Some(self.generation)
    }
    /// Grant/revoke consent only for the current exact authoring proposal.
    pub fn confirm_history(
        &mut self,
        document: &Document,
        proposal: Uuid,
        confirmed: bool,
    ) -> Result<(), &'static str> {
        if !self.current(document) || proposal != self.generation || !self.captured_addition {
            return Err("History consent is stale; use the current captured-addition proposal.");
        }
        self.confirmed_history = confirmed.then_some(proposal);
        Ok(())
    }
    /// A refused/cancelled effect needs fresh consent before another attempt.
    pub(crate) fn clear_history_consent(&mut self) {
        self.confirmed_history = None;
    }

    fn prepare(
        document: &Document,
        attachment: Option<(kagami_document::ObjectId, kagami_catalog::ComponentTypeId)>,
        replacement: Option<(kagami_document::ObjectId, kagami_catalog::ComponentTypeId)>,
    ) -> Result<Self, &'static str> {
        let captured_addition = attachment.is_some()
            && replacement.is_none()
            && document.snapshot().setup().scientific().is_some();
        let captured_reset =
            !captured_addition && document.snapshot().setup().scientific().is_some();
        let guard = document
            .authoring_guard()
            .ok_or("Component choices require Authoring mode.")?;
        let limits = ResolutionLimits::default();
        let mut roots = BTreeSet::new();
        for (id, object) in document.snapshot().objects() {
            for component in object.components.keys() {
                if replacement
                    .as_ref()
                    .is_some_and(|(target, old)| id == target && component == old)
                {
                    continue;
                }
                let Some(pin) = component.contribution() else {
                    if captured_reset {
                        return Err(
                            "Other legacy components require explicit migration before configuring replacement physics.",
                        );
                    }
                    continue;
                };
                if roots.len() == limits.max_roots && !roots.contains(pin) {
                    return Err("Component roots exceed the discovery budget.");
                }
                roots.insert(pin.clone());
            }
        }
        // Keep surviving roots and any already-selected member promoted to the
        // new root. Only intent unreachable after explicit replacement is pruned.
        let mut preserved = document
            .snapshot()
            .dependencies()
            .map(|lock| {
                if replacement.is_none() {
                    return Ok(lock.clone());
                }
                let mut keep = roots.clone();
                if let Some((_, component)) = &attachment
                    && let Some(root) = component.contribution()
                    && lock.selection().contributions.binary_search(root).is_ok()
                {
                    keep.insert(root.clone());
                }
                lock.for_roots(
                    &keep.into_iter().collect::<Vec<_>>(),
                    document.limits().dependencies,
                )
                .map(std::sync::Arc::new)
                .map_err(|_| "Cannot preserve the surviving component graph within its bounds.")
            })
            .transpose()?;
        if let Some((_, component)) = &attachment {
            let root = component.contribution().expect("attachment checked exact");
            if roots.len() == limits.max_roots && !roots.contains(root) {
                return Err("Component roots exceed the discovery budget.");
            }
            roots.insert(root.clone());
        }
        if roots.is_empty() {
            return Err("Add an exact plugin component before configuring its provider choices.");
        }
        let mut frozen = BTreeMap::new();
        if captured_addition || captured_reset {
            use orishu_plugin::authoring_lock::SelectionLock;
            let descriptor = document
                .snapshot()
                .setup()
                .scientific()
                .expect("captured checked")
                .declarations()
                .descriptor();
            let captured = SelectionLock::new(
                orishu_plugin::resolution::Selection {
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
                document.limits().dependencies,
            )
            .map_err(|_| "Captured choices exceed the component proposal budget.")?;
            if captured_addition {
                frozen.extend(
                    captured
                        .selection()
                        .bindings
                        .iter()
                        .map(|b| (b.requirement.clone(), b.provider.clone())),
                );
            }
            let selected_roots: Vec<_> = roots
                .iter()
                .filter(|r| captured.selection().contributions.binary_search(r).is_ok())
                .cloned()
                .collect();
            let retained = captured
                .for_roots(&selected_roots, document.limits().dependencies)
                .map_err(|_| "Cannot preserve captured component choices.")?;
            preserved = Some(std::sync::Arc::new(match preserved {
                Some(existing) => existing
                    .merge(&retained, document.limits().dependencies)
                    .map_err(|_| "Saved and captured component choices disagree.")?,
                None => retained,
            }));
        }
        let mut base = BTreeMap::new();
        if let Some(lock) = &preserved {
            if lock.selection().bindings.len() > limits.max_bindings {
                return Err("Stored provider choices exceed the discovery budget.");
            }
            base.extend(
                lock.selection()
                    .bindings
                    .iter()
                    .map(|b| (b.requirement.clone(), b.provider.clone())),
            );
        }
        let properties = attachment
            .as_ref()
            .and_then(|(_, component)| document.schemas().get(component))
            .map(|schema| properties::Properties::new(schema, document.limits()))
            .transpose()?;
        Ok(Self {
            generation: Uuid::new_v4(),
            guard: Some(guard),
            roots: roots.into_iter().collect(),
            bindings: base.clone(),
            base,
            attachment,
            properties,
            replacement,
            preserved,
            captured_addition,
            captured_reset,
            confirmed_history: None,
            frozen,
        })
    }
    /// Pending attachment identity for the inspector, not accepted scene state.
    pub fn attachment(
        &self,
    ) -> Option<&(kagami_document::ObjectId, kagami_catalog::ComponentTypeId)> {
        self.attachment.as_ref()
    }
    /// Whether this form still refers to the current authoring incarnation.
    pub fn current(&self, document: &Document) -> bool {
        self.guard.is_some_and(|g| document.accepts_effect(g))
    }
    /// Local fields from an exact declaration, never live capability adoption.
    pub fn properties(&self) -> Option<&properties::Properties> {
        self.properties.as_ref()
    }
    /// Change a bounded local value. Stale form/document identities, unknown
    /// properties, wrong kinds and source/count budgets refuse without mutation.
    /// Successful edits invalidate pending resolver/adoption work; numerical
    /// validity remains the document authority's decision.
    pub fn set_property(
        &mut self,
        document: &Document,
        form: Uuid,
        property: kagami_catalog::PropertyName,
        value: properties::Input,
    ) -> Result<(), &'static str> {
        if !self.current(document) {
            return Err("Component proposal is stale; prepare it again.");
        }
        let fields = self
            .properties
            .as_mut()
            .filter(|p| p.id() == form)
            .ok_or("Property form is stale; load the current fields.")?;
        fields.set(property, value, document.limits())?;
        self.generation = Uuid::new_v4();
        Ok(())
    }
    /// Exact local proposal choices, never accepted intent until Apply succeeds.
    pub fn bindings(&self) -> impl Iterator<Item = ProviderBinding> + '_ {
        self.bindings
            .iter()
            .map(|(requirement, provider)| ProviderBinding {
                requirement: requirement.clone(),
                provider: provider.clone(),
            })
    }
}
impl Proposal for ComponentForm {
    fn generation(&self) -> Uuid {
        self.generation
    }
    fn is_captured(&self) -> bool {
        false
    }
    fn requires_scientific_extension(&self) -> bool {
        self.captured_addition
    }
    fn requires_scientific_reset(&self) -> bool {
        self.captured_reset
    }
    fn permits_lock_adoption(&self) -> bool {
        self.replacement.is_none() || self.attachment.is_some()
    }
    fn prepare_properties(
        &mut self,
        document: &Document,
        prepared: &crate::plugins::PreparedAuthoringLock,
    ) -> Result<(), &'static str> {
        if !self.current(document) {
            return Err("Component proposal is stale.");
        }
        let (_, component) = self
            .attachment
            .as_ref()
            .ok_or("No attachment is proposed.")?;
        let schema = prepared
            .schemas()
            .get(component)
            .ok_or("Selected component schema is unavailable.")?;
        if let Some(fields) = &self.properties {
            if fields.schema() != schema {
                return Err("Component declaration changed; prepare the attachment again.");
            }
            return Ok(());
        }
        self.properties = Some(properties::Properties::new(schema, document.limits())?);
        self.generation = Uuid::new_v4();
        Ok(())
    }
    fn lock_commands(
        &self,
        document: &Document,
        prepared: &crate::plugins::PreparedAuthoringLock,
    ) -> Result<Vec<kagami_document::ExperimentCommand>, &'static str> {
        use kagami_document::ExperimentCommand as Edit;
        if !self.current(document) {
            return Err("Component proposal is stale; prepare it again.");
        }
        if !self.permits_lock_adoption() {
            return Err("Choose the replacement component first.");
        }
        if self.captured_addition && !self.history_confirmed() {
            return Err("Confirm regeneration of integrator history before adding this component.");
        }
        let mut commands = Vec::new();
        if let Some((object, component)) = &self.attachment {
            if let Some(existing) = &self.preserved
                && !self.captured_reset
            {
                let merged = existing.merge(prepared.lock(), document.limits().dependencies)
                    .map_err(|_| "Attachment conflicts with accepted component providers; migrate them explicitly first.")?;
                if &merged != prepared.lock().as_ref() {
                    return Err("Attachment selection omits accepted component intent.");
                }
            }
            let schema = prepared
                .schemas()
                .get(component)
                .ok_or("Selected component schema is unavailable.")?;
            let properties = if let Some(fields) = &self.properties {
                if fields.schema() != schema {
                    return Err("Component declaration changed; prepare the attachment again.");
                }
                fields.values().clone()
            } else {
                properties::Properties::new(schema, document.limits())?
                    .values()
                    .clone()
            };
            if let Some((target, previous)) = &self.replacement {
                commands.push(Edit::DetachComponent {
                    object: *target,
                    component: previous.clone(),
                });
            }
            commands.push(Edit::AttachComponent {
                object: *object,
                component: component.clone(),
                properties,
            });
        }
        let lock = match document.snapshot().dependencies() {
            Some(existing) if existing == prepared.lock() => existing.clone(),
            _ => prepared.lock().clone(),
        };
        commands.push(Edit::AdoptDependencies(lock));
        Ok(commands)
    }
    fn bind(&mut self, binding: ProviderBinding) -> Result<(), &'static str> {
        if self
            .frozen
            .get(&binding.requirement)
            .is_some_and(|p| p != &binding.provider)
        {
            return Err("This choice governs captured physics; changing it requires migration.");
        }
        if self.attachment.is_some()
            && !self.captured_reset
            && self
                .base
                .get(&binding.requirement)
                .is_some_and(|p| p != &binding.provider)
        {
            return Err(
                "Attachment cannot change an accepted provider; edit provider choices explicitly first.",
            );
        }
        if !self.bindings.contains_key(&binding.requirement)
            && self.bindings.len() == ResolutionLimits::default().max_bindings
        {
            return Err("Provider choices exceed the form budget.");
        }
        self.bindings.insert(binding.requirement, binding.provider);
        self.generation = Uuid::new_v4();
        Ok(())
    }
    fn clear_bindings(&mut self) {
        self.bindings = self.base.clone();
        self.generation = Uuid::new_v4();
    }
    fn forget_binding(&mut self, requirement: &RequirementKey) -> Result<(), &'static str> {
        if self.attachment.is_some() && !self.captured_reset && self.base.contains_key(requirement)
        {
            return Err("Attachment cannot discard an accepted binding.");
        }
        self.bindings
            .remove(requirement)
            .ok_or("No such local binding.")?;
        self.generation = Uuid::new_v4();
        Ok(())
    }
    fn selection_with_scene(
        &self,
        _choices: &[KernelChoice],
        revision: u64,
        document: &Document,
    ) -> Result<ResolutionRequest, &'static str> {
        if !self.current(document) {
            return Err("Component proposal is stale; load current component choices explicitly.");
        }
        if self.replacement.is_some() && self.attachment.is_none() {
            return Err("Choose the replacement component before checking dependencies.");
        }
        Ok(ResolutionRequest {
            expected_inventory_revision: revision,
            roots: self.roots.clone(),
            bindings: self.bindings().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_document_does_not_offer_a_lock_that_would_constrain_future_roots() {
        let document = Document::new(Default::default(), Default::default());
        assert!(ComponentForm::from_document(&document).is_err());
        assert!(!ComponentForm::default().current(&document));
        assert!(!document.is_dirty());
    }

    #[test]
    fn replacement_preparation_works_at_capacity_without_consuming_a_new_slot() {
        use kagami_catalog::{ComponentSchema, ComponentTypeId, SchemaRegistry, SchemaVersion};
        use kagami_document::{DisplayName, ExperimentCommand, Limits, ObjectSpec};
        let pin = |id: &str| {
            ComponentTypeId::exact(ContributionRef {
                release: format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
                extension_point: "orishu.model.components/v1".parse().unwrap(),
                local_id: id.parse().unwrap(),
            })
            .unwrap()
        };
        let previous = pin("old");
        let next = pin("new");
        let schemas = SchemaRegistry::new()
            .with(ComponentSchema::new(previous.clone(), SchemaVersion(1)))
            .with(ComponentSchema::new(next.clone(), SchemaVersion(1)));
        let mut document = Document::new(
            schemas,
            Limits {
                max_components_per_object: 1,
                ..Default::default()
            },
        );
        assert!(document.edit(vec![ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("full object").unwrap())
                    .with_component(previous.clone(), Default::default())
            ))]));
        let object = *document.snapshot().objects().keys().next().unwrap();
        assert!(ComponentForm::for_attachment(&document, object, next.clone()).is_err());
        let choice = ComponentForm::begin_replacement(&document, object, previous.clone()).unwrap();
        assert!(!choice.permits_lock_adoption());
        assert!(
            choice
                .replace_with(&document, choice.proposal_id(), previous)
                .is_err()
        );
        let prepared = choice
            .replace_with(&document, choice.proposal_id(), next.clone())
            .unwrap();
        assert_eq!(prepared.attachment(), Some(&(object, next)));
        assert_eq!(
            document.snapshot().object(object).unwrap().components.len(),
            1
        );
    }
}
