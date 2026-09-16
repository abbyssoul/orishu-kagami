//! Bounded retained authoring inputs. Defaults remain absent overrides, not
//! silently copied values. Scientific validation belongs to the shared compiler.
use crate::plugins::KernelChoice;
use orishu_plugin::execution::{AuthoredConfigurationProperty, ConfigurationInput};
use orishu_plugin::{LocalContributionId, Property, PropertyType};
use std::collections::{BTreeMap, BTreeSet};

const INPUT_BYTES: usize = 4096;
const TOTAL_BYTES: usize = 1024 * 1024;
const TOTAL_PROPERTIES: usize = 4096;

/// A proposal for a property of the exact model displayed at this startup index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterAction {
    /// Index into the immutable displayed inventory, never a provider lookup.
    pub kernel: usize,
    /// Exact declared property.
    pub property: LocalContributionId,
    /// None uses the declaration's default (or omits an optional value).
    /// False/empty string are explicit values, not absence.
    pub input: Option<ConfigurationInput>,
}

/// A form-owned bounded map. Dropping/deselecting a model never changes authored
/// state; its overrides remain local until explicit successful setup adoption.
#[derive(Default)]
pub struct Parameters {
    values: BTreeMap<usize, BTreeMap<LocalContributionId, ConfigurationInput>>,
    count: usize,
    bytes: usize,
    error: Option<&'static str>,
}
impl Parameters {
    /// Last rejected local input; no arbitrary submitted text is echoed.
    pub fn error(&self) -> Option<&'static str> {
        self.error
    }

    /// Explicit value only; None means the declared default/omission applies.
    pub fn get(
        &self,
        kernel: usize,
        property: &LocalContributionId,
    ) -> Option<&ConfigurationInput> {
        self.values.get(&kernel)?.get(property)
    }

    /// Retain a typed override only for a selected, declared property. Check
    /// byte/count/type bounds before inserting or cloning any supplied content.
    pub fn edit(
        &mut self,
        action: ParameterAction,
        choices: &[KernelChoice],
        selected: &BTreeSet<usize>,
    ) {
        self.error = self.try_edit(action, choices, selected).err();
    }
    fn try_edit(
        &mut self,
        action: ParameterAction,
        choices: &[KernelChoice],
        selected: &BTreeSet<usize>,
    ) -> Result<(), &'static str> {
        if !selected.contains(&action.kernel) {
            return Err("Select this kernel before editing its parameters.");
        }
        let schema = choices
            .get(action.kernel)
            .and_then(|c| c.configuration.iter().find(|p| p.id == action.property))
            .ok_or("This parameter is not declared by the selected kernel.")?;
        if let Some(value) = &action.input {
            check_input(schema, value)?;
        }
        let old = self.get(action.kernel, &action.property);
        let existed = old.is_some();
        if action.input.is_some() && !existed && self.count == TOTAL_PROPERTIES {
            return Err("Physics form exceeds its parameter count budget.");
        }
        let old_bytes = old.map_or(0, input_bytes);
        let new_bytes = action.input.as_ref().map_or(0, input_bytes);
        let total = self.bytes - old_bytes + new_bytes;
        if total > TOTAL_BYTES {
            return Err("Physics form exceeds its parameter text budget.");
        }
        if let Some(value) = action.input {
            self.values
                .entry(action.kernel)
                .or_default()
                .insert(action.property, value);
            if !existed {
                self.count += 1;
            }
        } else if existed {
            let values = self.values.get_mut(&action.kernel).expect("existing input");
            values.remove(&action.property);
            if values.is_empty() {
                self.values.remove(&action.kernel);
            }
            self.count -= 1;
        }
        self.bytes = total;
        Ok(())
    }

    /// Bounded exact inputs in canonical property order. No expression evaluation
    /// or copying of defaults happens here. O(properties * log(overrides)).
    pub fn inputs(
        &self,
        kernel: usize,
        choice: &KernelChoice,
    ) -> Result<Vec<AuthoredConfigurationProperty>, &'static str> {
        if self.error.is_some() {
            return Err("Correct the refused parameter input before applying physics.");
        }
        if choice.configuration.len() > orishu_plugin::Limits::default().max_schema_items {
            return Err("Kernel configuration exceeds the property limit.");
        }
        let mut inputs = Vec::new();
        for schema in &choice.configuration {
            if let Some(input) = self.get(kernel, &schema.id) {
                check_input(schema, input)?;
                inputs.push(AuthoredConfigurationProperty {
                    id: schema.id.clone(),
                    input: input.clone(),
                });
            }
        }
        inputs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(inputs)
    }
}

fn input_bytes(input: &ConfigurationInput) -> usize {
    match input {
        ConfigurationInput::Expression { source } => source.len(),
        ConfigurationInput::Text { value } => value.len(),
        ConfigurationInput::Boolean { .. } => 0,
    }
}

fn check_input(schema: &Property, input: &ConfigurationInput) -> Result<(), &'static str> {
    if input_bytes(input) > INPUT_BYTES {
        return Err("Parameter input exceeds 4096 UTF-8 bytes.");
    }
    match (&schema.schema, input) {
        (PropertyType::Quantity { .. }, ConfigurationInput::Expression { .. })
        | (PropertyType::Boolean { .. }, ConfigurationInput::Boolean { .. }) => Ok(()),
        (PropertyType::Text { max_bytes, .. }, ConfigurationInput::Text { value })
            if value.len() <= *max_bytes as usize =>
        {
            Ok(())
        }
        (PropertyType::Text { .. }, ConfigurationInput::Text { .. }) => {
            Err("Text exceeds this parameter's declared byte limit.")
        }
        _ => Err("Input type does not match the declared kernel parameter."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_plugin::{ContributionRef, ExecutionContractId};
    use orishu_variables::{Dimension, VariablesSystem};

    fn choice(configuration: Vec<Property>) -> KernelChoice {
        KernelChoice {
            contribution: ContributionRef {
                release: format!("sha256:{}", "00".repeat(32)).parse().unwrap(),
                extension_point: "orishu.models/v1".parse().unwrap(),
                local_id: "model".parse().unwrap(),
            },
            contract: ExecutionContractId::Dynamics,
            label: "example".into(),
            observable_slots: vec![],
            configuration,
        }
    }
    fn property(id: &str, schema: PropertyType) -> Property {
        Property {
            id: id.parse().unwrap(),
            required: true,
            schema,
        }
    }
    fn action(kernel: usize, property: &str, input: Option<ConfigurationInput>) -> ParameterAction {
        ParameterAction {
            kernel,
            property: property.parse().unwrap(),
            input,
        }
    }
    #[test]
    fn typed_values_distinguish_defaults_false_and_empty_and_resolve_with_units() {
        let choices = vec![choice(vec![
            property(
                "length",
                PropertyType::Quantity {
                    dimension: Dimension::LENGTH,
                    default_expression: Some("1 m".into()),
                    minimum_si: None,
                    maximum_si: None,
                },
            ),
            property(
                "enabled",
                PropertyType::Boolean {
                    default: Some(true),
                },
            ),
            property(
                "label",
                PropertyType::Text {
                    default: Some("default".into()),
                    max_bytes: 8,
                },
            ),
        ])];
        let mut parameters = Parameters::default();
        let selected = BTreeSet::from([0]);
        assert!(parameters.inputs(0, &choices[0]).unwrap().is_empty());
        for (id, input) in [
            (
                "length",
                ConfigurationInput::Expression {
                    source: "2 cm + 3 mm".into(),
                },
            ),
            ("enabled", ConfigurationInput::Boolean { value: false }),
            (
                "label",
                ConfigurationInput::Text {
                    value: String::new(),
                },
            ),
        ] {
            parameters.edit(action(0, id, Some(input)), &choices, &selected);
        }
        let inputs = parameters.inputs(0, &choices[0]).unwrap();
        let resolved = orishu_plugin::execution::resolve_configuration(
            &choices[0].configuration,
            &inputs,
            &VariablesSystem::default(),
            &Default::default(),
        )
        .unwrap();
        use orishu_plugin::execution::ConfigurationValue;
        assert!(matches!(
            resolved.get("enabled"),
            Some(ConfigurationValue::Boolean { value: false })
        ));
        assert!(
            matches!(resolved.get("label"), Some(ConfigurationValue::Text { value }) if value.is_empty())
        );
        assert!(
            matches!(resolved.get("length"), Some(ConfigurationValue::Quantity { value_si, dimension }) if (value_si.get() - 0.023).abs() < 1e-15 && *dimension == Dimension::LENGTH)
        );
        assert!(
            matches!(&inputs[2].input, ConfigurationInput::Expression { source } if source == "2 cm + 3 mm")
        );
        parameters.edit(action(0, "enabled", None), &choices, &selected);
        assert!(parameters.get(0, &"enabled".parse().unwrap()).is_none());
        assert_eq!(parameters.count, 2);
    }

    #[test]
    fn undeclared_wrong_type_unselected_and_oversized_input_preserve_prior_values() {
        let choices = vec![choice(vec![property(
            "label",
            PropertyType::Text {
                default: None,
                max_bytes: 4,
            },
        )])];
        let selected = BTreeSet::from([0]);
        let mut p = Parameters::default();
        let original = ConfigurationInput::Text { value: "ok".into() };
        p.edit(
            action(0, "label", Some(original.clone())),
            &choices,
            &selected,
        );
        for bad in [
            action(
                0,
                "label",
                Some(ConfigurationInput::Text {
                    value: "ééé".into(),
                }),
            ),
            action(
                0,
                "label",
                Some(ConfigurationInput::Text {
                    value: "a".repeat(INPUT_BYTES + 1),
                }),
            ),
            action(
                0,
                "label",
                Some(ConfigurationInput::Boolean { value: true }),
            ),
            action(0, "missing", Some(original.clone())),
            action(1, "label", Some(original.clone())),
        ] {
            p.edit(bad, &choices, &selected);
            assert!(p.error().is_some());
            assert_eq!(p.get(0, &"label".parse().unwrap()), Some(&original));
            assert_eq!(p.bytes, 2);
            assert!(p.inputs(0, &choices[0]).is_err());
        }
        p.edit(action(0, "label", None), &choices, &selected);
        assert!(p.error().is_none());
        assert_eq!((p.count, p.bytes, p.values.len()), (0, 0, 0));
    }

    #[test]
    fn aggregate_input_bounds_are_reclaimed_only_on_accepted_changes() {
        let schema = (0..256)
            .map(|n| {
                property(
                    &format!("p-{n}"),
                    PropertyType::Text {
                        default: None,
                        max_bytes: INPUT_BYTES as u32,
                    },
                )
            })
            .collect();
        let choices = vec![choice(schema); 17];
        let selected = (0..17).collect();
        let mut p = Parameters::default();
        for n in 0..256 {
            p.edit(
                action(
                    0,
                    &format!("p-{n}"),
                    Some(ConfigurationInput::Text {
                        value: "x".repeat(INPUT_BYTES),
                    }),
                ),
                &choices,
                &selected,
            );
        }
        assert_eq!(p.bytes, TOTAL_BYTES);
        p.edit(
            action(
                1,
                "p-0",
                Some(ConfigurationInput::Text { value: "x".into() }),
            ),
            &choices,
            &selected,
        );
        assert!(p.error().is_some());
        assert_eq!(p.count, 256);
        p.edit(action(0, "p-0", None), &choices, &selected);
        p.edit(
            action(
                1,
                "p-0",
                Some(ConfigurationInput::Text { value: "x".into() }),
            ),
            &choices,
            &selected,
        );
        assert!(p.error().is_none());
        assert_eq!(p.bytes, TOTAL_BYTES - INPUT_BYTES + 1);
        let mut p = Parameters::default();
        for k in 0..16 {
            for n in 0..256 {
                p.edit(
                    action(
                        k,
                        &format!("p-{n}"),
                        Some(ConfigurationInput::Text {
                            value: String::new(),
                        }),
                    ),
                    &choices,
                    &selected,
                );
            }
        }
        assert_eq!(p.count, TOTAL_PROPERTIES);
        p.edit(
            action(
                16,
                "p-0",
                Some(ConfigurationInput::Text {
                    value: String::new(),
                }),
            ),
            &choices,
            &selected,
        );
        assert!(p.error().is_some());
        assert_eq!(p.count, TOTAL_PROPERTIES);
        assert_eq!(p.bytes, 0);
    }
}
