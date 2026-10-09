//! Bound retained scene proposals before cloning, threads, inventory IO or JIT.
//! This is input admission, not semantic validation or a process-wide RSS claim.
use super::EffectError;
use kagami_document::{AuthoredValue, ComponentProperties, ExperimentCommand, Limits};

const ENTRIES: usize = 4096;
const TEXT_BYTES: usize = 1024 * 1024;

pub(super) fn check_instantiation(
    spec: &kagami_session::InstantiationSpec,
    limits: &Limits,
) -> Result<(), EffectError> {
    kagami_session::instantiation::check_request(spec, limits)
        .map_err(|e| EffectError::Instantiation(Box::new(e)))?;
    let mut budget = Budget {
        entries: 0,
        text: 0,
        limits,
    };
    for (name, source) in &spec.bindings {
        budget.entry()?;
        budget.text_len(name.as_str().len(), limits.max_expression_bytes)?;
        budget.text_len(source.len(), limits.max_expression_bytes)?;
    }
    Ok(())
}

pub(super) fn check(commands: &[ExperimentCommand], limits: &Limits) -> Result<(), EffectError> {
    check_inner(commands, limits, false)
}

/// Addition preparation can introduce copied definitions, never change/remove
/// existing variables. Final coherence is still the shared document core's job.
pub(super) fn check_addition(
    commands: &[ExperimentCommand],
    limits: &Limits,
) -> Result<(), EffectError> {
    check_inner(commands, limits, true)
}

fn check_inner(
    commands: &[ExperimentCommand],
    limits: &Limits,
    definitions: bool,
) -> Result<(), EffectError> {
    let mut budget = Budget {
        entries: commands.len(),
        text: 0,
        limits,
    };
    if budget.entries > ENTRIES {
        return Err(EffectError::Limit);
    }
    for command in commands {
        match command {
            ExperimentCommand::DefineVariable(spec) if definitions => {
                budget.entry()?;
                budget.text_len(spec.namespace.as_str().len(), limits.max_expression_bytes)?;
                budget.text_len(spec.name.as_str().len(), limits.max_expression_bytes)?;
                budget.text_len(spec.expression.len(), limits.max_expression_bytes)?;
                if let Some(description) = &spec.description {
                    budget.text_len(description.len(), limits.max_description_bytes)?;
                }
            }
            ExperimentCommand::CreateObject(spec) => {
                if let Some(provenance) = &spec.provenance {
                    budget.text_len(provenance.api_version.len(), 128)?;
                    budget.text_len(
                        provenance.source.file.as_os_str().as_encoded_bytes().len(),
                        4096,
                    )?;
                }
                if spec.components.len() > limits.max_components_per_object {
                    return Err(EffectError::Limit);
                }
                for properties in spec.components.values() {
                    budget.properties(properties)?;
                }
            }
            ExperimentCommand::AttachComponent { properties, .. } => {
                budget.properties(properties)?
            }
            ExperimentCommand::SetComponentProperty { value, .. } => budget.value(value)?,
            ExperimentCommand::AdoptDependencies(lock) => {
                lock.validate(limits.dependencies)
                    .map_err(|_| EffectError::Limit)?;
            }
            ExperimentCommand::RemoveObject(_)
            | ExperimentCommand::RenameObject { .. }
            | ExperimentCommand::SetTransform { .. }
            | ExperimentCommand::SetVelocity { .. }
            | ExperimentCommand::SetShape { .. }
            | ExperimentCommand::DetachComponent { .. } => {}
            // This lane changes objects, not scientific settings or the variable
            // graph. Those need an explicit affected-capture regeneration plan.
            _ => return Err(EffectError::Unavailable),
        }
    }
    Ok(())
}

struct Budget<'a> {
    entries: usize,
    text: usize,
    limits: &'a Limits,
}
impl Budget<'_> {
    fn properties(&mut self, properties: &ComponentProperties) -> Result<(), EffectError> {
        if properties.len() > self.limits.max_properties_per_component {
            return Err(EffectError::Limit);
        }
        self.entry()?;
        for value in properties.values() {
            self.value(value)?;
        }
        Ok(())
    }
    fn entry(&mut self) -> Result<(), EffectError> {
        self.entries = self
            .entries
            .checked_add(1)
            .filter(|n| *n <= ENTRIES)
            .ok_or(EffectError::Limit)?;
        Ok(())
    }
    fn value(&mut self, value: &AuthoredValue) -> Result<(), EffectError> {
        self.entry()?;
        let (length, bound) = match value {
            AuthoredValue::Quantity { expression, .. } => {
                (expression.len(), self.limits.max_expression_bytes)
            }
            AuthoredValue::Text(text) => (text.len(), self.limits.max_text_bytes),
            AuthoredValue::Boolean(_) => (0, 0),
        };
        self.text_len(length, bound)
    }
    fn text_len(&mut self, length: usize, bound: usize) -> Result<(), EffectError> {
        if length > bound {
            return Err(EffectError::Limit);
        }
        self.text = self
            .text
            .checked_add(length)
            .filter(|n| *n <= TEXT_BYTES)
            .ok_or(EffectError::Limit)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kagami_document::{DisplayName, ObjectSpec};

    #[test]
    fn copied_definitions_are_addition_only_and_obey_owned_and_aggregate_limits() {
        let limits = Limits::default();
        let definition = ExperimentCommand::DefineVariable(Box::new(
            kagami_document::VariableSpec::new("mass".try_into().unwrap(), "1 kg"),
        ));
        check_addition(std::slice::from_ref(&definition), &limits).unwrap();
        assert!(matches!(
            check(std::slice::from_ref(&definition), &limits),
            Err(EffectError::Unavailable)
        ));
        let strict = Limits {
            max_expression_bytes: 3,
            ..limits
        };
        assert!(matches!(
            check_addition(&[definition], &strict),
            Err(EffectError::Limit)
        ));
        let definition =
            ExperimentCommand::DefineVariable(Box::new(kagami_document::VariableSpec::new(
                "mass".try_into().unwrap(),
                "1".repeat(limits.max_expression_bytes),
            )));
        assert!(matches!(
            check_addition(
                &vec![definition; TEXT_BYTES / limits.max_expression_bytes + 1],
                &limits
            ),
            Err(EffectError::Limit)
        ));
        let mut spec = kagami_session::InstantiationSpec::new(
            kagami_catalog::TemplateIdentity::new(
                "test".try_into().unwrap(),
                "body".try_into().unwrap(),
            ),
            DisplayName::new("test").unwrap(),
        );
        for i in 0..=TEXT_BYTES / limits.max_expression_bytes {
            spec.bindings.insert(
                format!("parameter_{i}").try_into().unwrap(),
                "1".repeat(limits.max_expression_bytes),
            );
        }
        assert!(matches!(
            check_instantiation(&spec, &limits),
            Err(EffectError::Limit)
        ));
    }

    #[test]
    fn dependency_proposals_are_bounded_before_worker_retention() {
        let lock = std::sync::Arc::new(
            orishu_plugin::authoring_lock::SelectionLock::new(
                orishu_plugin::resolution::Selection {
                    roots: vec![],
                    contributions: vec![],
                    bindings: vec![],
                },
                Default::default(),
            )
            .unwrap(),
        );
        let mut limits = Limits::default();
        let commands = [ExperimentCommand::AdoptDependencies(lock)];
        check(&commands, &limits).unwrap();
        limits.dependencies.bytes = 1;
        assert!(matches!(check(&commands, &limits), Err(EffectError::Limit)));
    }

    #[test]
    fn refuses_large_text_nested_collections_and_non_object_intent_before_preparation() {
        let limits = Limits::default();
        let mut budget = Budget {
            entries: 0,
            text: 0,
            limits: &limits,
        };
        assert!(matches!(
            budget.value(&AuthoredValue::si(
                "0".repeat(limits.max_expression_bytes + 1)
            )),
            Err(EffectError::Limit)
        ));
        let text = AuthoredValue::Text("a".repeat(limits.max_text_bytes));
        for _ in 0..TEXT_BYTES / limits.max_text_bytes {
            budget.value(&text).unwrap();
        }
        assert!(matches!(budget.value(&text), Err(EffectError::Limit)));
        let properties = (0..=limits.max_properties_per_component)
            .map(|i| {
                (
                    kagami_catalog::PropertyName::new(format!("p{i}")).unwrap(),
                    AuthoredValue::Boolean(false),
                )
            })
            .collect();
        assert!(matches!(
            budget.properties(&properties),
            Err(EffectError::Limit)
        ));
        let command = ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
            DisplayName::new("object").unwrap(),
        )));
        assert!(matches!(
            check(&vec![command; ENTRIES + 1], &limits),
            Err(EffectError::Limit)
        ));
        assert!(matches!(
            check(
                &[ExperimentCommand::SetTimeStep(
                    kagami_document::TimeStep::default()
                )],
                &limits
            ),
            Err(EffectError::Unavailable)
        ));
    }
}
