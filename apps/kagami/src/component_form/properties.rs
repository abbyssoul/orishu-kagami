//! Bounded, local attachment values. Only the document evaluates expressions.
use kagami_catalog::{ComponentSchema, PropertyKind, PropertyName};
use kagami_document::{AuthoredValue, ComponentProperties, Limits};
use uuid::Uuid;

const MAX_FIELDS: usize = 256;
const MAX_BYTES: usize = 1024 * 1024;

/// A UI value, preserving the distinction between absent and empty/false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A retained expression interpreted in the declaration's SI dimension.
    Quantity(String),
    /// Literal text, including an explicitly empty value.
    Text(String),
    /// An explicit flag, distinct from absence.
    Boolean(bool),
    /// Remove the proposed value; requiredness is checked on acceptance.
    Unset,
}

/// Read-only declaration plus local values, never installed capabilities.
pub struct Properties {
    id: Uuid,
    schema: ComponentSchema,
    values: ComponentProperties,
    bytes: usize,
}
impl Properties {
    pub(super) fn new(schema: &ComponentSchema, limits: &Limits) -> Result<Self, &'static str> {
        if schema.properties.len() > MAX_FIELDS {
            return Err("Component property fields exceed the form budget.");
        }
        // Bound retained defaults before cloning their sources or the schema.
        let mut count = 0usize;
        let mut bytes = 0usize;
        for property in schema.properties.values() {
            use orishu_plugin::PropertyType;
            let size = match property.plugin_declaration() {
                Some(PropertyType::Quantity {
                    default_expression: Some(value),
                    ..
                }) => {
                    if value.len() > limits.max_expression_bytes {
                        return Err("Default expression exceeds the form budget.");
                    }
                    Some(value.len())
                }
                Some(PropertyType::Text {
                    default: Some(value),
                    ..
                }) => {
                    if value.len() > limits.max_text_bytes {
                        return Err("Default text exceeds the form budget.");
                    }
                    Some(value.len())
                }
                Some(PropertyType::Boolean {
                    default: Some(_), ..
                }) => Some(0),
                _ => None,
            };
            if let Some(size) = size {
                count += 1;
                bytes = bytes
                    .checked_add(size)
                    .filter(|n| *n <= MAX_BYTES)
                    .ok_or("Property sources exceed the form budget.")?;
            }
        }
        if count > limits.max_properties_per_component {
            return Err("Default properties exceed the component budget.");
        }
        Ok(Self {
            id: Uuid::new_v4(),
            schema: schema.clone(),
            values: crate::plugins::component_defaults(schema),
            bytes,
        })
    }
    /// Stable until a different property form is loaded; not a document identity.
    pub fn id(&self) -> Uuid {
        self.id
    }
    /// The exact component declaration used to present these fields.
    pub fn schema(&self) -> &ComponentSchema {
        &self.schema
    }
    /// Retained local sources, flags and text; not evaluated experiment values.
    pub fn values(&self) -> &ComponentProperties {
        &self.values
    }

    pub(super) fn set(
        &mut self,
        name: PropertyName,
        input: Input,
        limits: &Limits,
    ) -> Result<(), &'static str> {
        let property = self
            .schema
            .properties
            .get(&name)
            .ok_or("Unknown component property.")?;
        let value = match (&property.kind, input) {
            (_, Input::Unset) => None,
            (PropertyKind::Quantity { .. }, Input::Quantity(source))
                if source.len() <= limits.max_expression_bytes =>
            {
                Some(AuthoredValue::si(source))
            }
            (PropertyKind::Text, Input::Text(source)) if source.len() <= limits.max_text_bytes => {
                Some(AuthoredValue::Text(source))
            }
            (PropertyKind::Boolean, Input::Boolean(value)) => Some(AuthoredValue::Boolean(value)),
            _ => return Err("Property kind or source length is invalid."),
        };
        let existing = self.values.get(&name);
        if existing.is_none()
            && value.is_some()
            && self.values.len() >= limits.max_properties_per_component
        {
            return Err("Property count exceeds the component budget.");
        }
        let bytes = self
            .bytes
            .saturating_sub(size(existing))
            .checked_add(size(value.as_ref()))
            .filter(|n| *n <= MAX_BYTES)
            .ok_or("Property sources exceed the form budget.")?;
        if let Some(value) = value {
            self.values.insert(name, value);
        } else {
            self.values.remove(&name);
        }
        self.bytes = bytes;
        Ok(())
    }
}
fn size(value: Option<&AuthoredValue>) -> usize {
    match value {
        Some(AuthoredValue::Quantity { expression, .. }) => expression.len(),
        Some(AuthoredValue::Text(text)) => text.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kagami_catalog::{ComponentTypeId, PropertySchema, SchemaVersion};
    fn schema() -> ComponentSchema {
        let pin = ComponentTypeId::exact(orishu_plugin::ContributionRef {
            release: format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
            extension_point: "orishu.model.components/v1".parse().unwrap(),
            local_id: "values".parse().unwrap(),
        })
        .unwrap();
        ComponentSchema::new(pin, SchemaVersion(1))
            .with_property(
                "text".try_into().unwrap(),
                PropertySchema::optional(PropertyKind::Text),
            )
            .with_property(
                "other".try_into().unwrap(),
                PropertySchema::optional(PropertyKind::Text),
            )
            .with_property(
                "flag".try_into().unwrap(),
                PropertySchema::required(PropertyKind::Boolean),
            )
    }

    #[test]
    fn optional_absence_empty_text_and_false_are_distinct_and_refusals_are_atomic() {
        let limits = Limits {
            max_properties_per_component: 1,
            ..Default::default()
        };
        let mut fields = Properties::new(&schema(), &limits).unwrap();
        let flag = "flag".try_into().unwrap();
        fields.set(flag, Input::Boolean(false), &limits).unwrap();
        let before = fields.values().clone();
        assert!(
            fields
                .set(
                    "text".try_into().unwrap(),
                    Input::Text(String::new()),
                    &limits
                )
                .is_err()
        );
        assert!(
            fields
                .set(
                    "flag".try_into().unwrap(),
                    Input::Quantity("1".into()),
                    &limits
                )
                .is_err()
        );
        assert!(
            fields
                .set("unknown".try_into().unwrap(), Input::Unset, &limits)
                .is_err()
        );
        assert_eq!(fields.values(), &before);
        fields
            .set("flag".try_into().unwrap(), Input::Unset, &limits)
            .unwrap();
        fields
            .set(
                "text".try_into().unwrap(),
                Input::Text(String::new()),
                &limits,
            )
            .unwrap();
        assert_eq!(
            fields.values()[&"text".try_into().unwrap()],
            AuthoredValue::Text(String::new())
        );
        assert!(
            fields
                .set(
                    "text".try_into().unwrap(),
                    Input::Text("x".repeat(limits.max_text_bytes + 1)),
                    &limits
                )
                .is_err()
        );
        assert_eq!(
            fields.values()[&"text".try_into().unwrap()],
            AuthoredValue::Text(String::new())
        );
    }

    #[test]
    fn aggregate_sources_stay_bounded_even_with_permissive_caller_limits() {
        let limits = Limits {
            max_text_bytes: MAX_BYTES,
            ..Default::default()
        };
        let mut fields = Properties::new(&schema(), &limits).unwrap();
        fields
            .set(
                "text".try_into().unwrap(),
                Input::Text("x".repeat(MAX_BYTES)),
                &limits,
            )
            .unwrap();
        assert!(
            fields
                .set(
                    "other".try_into().unwrap(),
                    Input::Text("y".into()),
                    &limits
                )
                .is_err()
        );
        assert!(!fields.values().contains_key(&"other".try_into().unwrap()));
        fields
            .set("text".try_into().unwrap(), Input::Unset, &limits)
            .unwrap();
        fields
            .set(
                "other".try_into().unwrap(),
                Input::Text("y".into()),
                &limits,
            )
            .unwrap();
        assert_eq!(fields.bytes, 1);
    }

    #[test]
    fn defaults_and_field_counts_are_checked_before_form_construction() {
        let limits = Limits::default();
        let declared = PropertySchema::from_plugin(&orishu_plugin::Property {
            id: "text".parse().unwrap(),
            required: true,
            schema: orishu_plugin::PropertyType::Text {
                max_bytes: 10000,
                default: Some("x".repeat(limits.max_text_bytes + 1)),
            },
        });
        assert!(
            Properties::new(
                &schema().with_property("text".try_into().unwrap(), declared),
                &limits
            )
            .is_err()
        );
        let mut many = schema();
        for i in 0..MAX_FIELDS {
            many.properties.insert(
                format!("p{i}").try_into().unwrap(),
                PropertySchema::optional(PropertyKind::Boolean),
            );
        }
        assert!(Properties::new(&many, &limits).is_err());
    }
}
