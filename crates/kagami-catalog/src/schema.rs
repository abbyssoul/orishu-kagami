//! The component/property schema registry the catalog validates against.
//!
//! Simulation plugins own these declarations (ADR 0008); the catalog only
//! *consumes* them. The registry is therefore an owned value passed into
//! loading and validation, never a process-global singleton the catalog
//! reaches for — a test can register exactly the two component types it cares
//! about, and two catalogs resolved against different registries cannot
//! interfere.
//!
//! An absent schema is the load-bearing case: a template that names a
//! component type this installation does not have is preserved and reported
//! [`crate::entry::LoadResult::Unavailable`], so installing the plugin later
//! makes it available without the file ever being rewritten.

use std::collections::BTreeMap;

pub use orishu_plugin::execution::PropertyValueRef;
use serde::{Deserialize, Serialize};

use crate::name::{ComponentTypeId, PropertyName};
use crate::quantity::Dimension;

/// Monotonic version of one component schema, recorded in an instantiated
/// object's provenance so a later reader can tell which declaration the
/// authored values were checked against.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SchemaVersion(pub u32);

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "v{}", self.0)
    }
}

/// What kind of authored value a property accepts.
///
/// Only [`PropertyKind::Quantity`] is expression-capable: it is the kind that
/// retains authored expression source, carries a dimension, and is projected
/// into the shared variable environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PropertyKind {
    /// A dimensioned physical value authored as an expression.
    Quantity {
        /// The dimension an authored expression must be declared in.
        dimension: Dimension,
    },
    /// A flag.
    Boolean,
    /// Free-form text.
    Text,
}

impl PropertyKind {
    /// `true` when this kind retains authored expression source and is
    /// projected as a variable binding.
    pub fn is_expression_capable(&self) -> bool {
        matches!(self, PropertyKind::Quantity { .. })
    }

    /// A short name for this kind, for diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            PropertyKind::Quantity { .. } => "quantity",
            PropertyKind::Boolean => "boolean",
            PropertyKind::Text => "text",
        }
    }
}

/// One property a component contributes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertySchema {
    /// The value kind and, for a quantity, its dimension.
    pub kind: PropertyKind,
    /// When `true`, a template that omits this property is not usable.
    pub required: bool,
    /// Complete plugin declaration, including retained defaults and constraints.
    /// Absent only for the legacy hand-registered schema API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plugin: Option<orishu_plugin::PropertyType>,
}

impl PropertySchema {
    /// An optional property of the given kind.
    pub fn optional(kind: PropertyKind) -> Self {
        Self {
            kind,
            required: false,
            plugin: None,
        }
    }

    /// A property a template must author.
    pub fn required(kind: PropertyKind) -> Self {
        Self {
            kind,
            required: true,
            plugin: None,
        }
    }

    /// Project a property from a validated scientific declaration, retaining
    /// constraints and defaults. This is not declaration/provider validation;
    /// callers must establish those before accepting plugin metadata.
    pub fn from_plugin(property: &orishu_plugin::Property) -> Self {
        use orishu_plugin::PropertyType;
        let kind = match &property.schema {
            PropertyType::Quantity { dimension, .. } => PropertyKind::Quantity {
                dimension: *dimension,
            },
            PropertyType::Boolean { .. } => PropertyKind::Boolean,
            PropertyType::Text { .. } => PropertyKind::Text,
        };
        Self {
            kind,
            required: property.required,
            plugin: Some(property.schema.clone()),
        }
    }

    /// Defaults are suggestions to author, not silently inserted during loading
    /// or validation. The source is retained exactly as the plugin declared it.
    pub fn plugin_declaration(&self) -> Option<&orishu_plugin::PropertyType> {
        self.plugin.as_ref()
    }

    /// One constraint check for catalog materialization and document commands.
    /// Workload validation uses the same shared plugin predicate independently.
    pub fn accepts(&self, value: PropertyValueRef<'_>) -> bool {
        let kind_matches = match (&self.kind, value) {
            (
                PropertyKind::Quantity { dimension },
                PropertyValueRef::Quantity {
                    value_si,
                    dimension: actual,
                },
            ) => value_si.is_finite() && *dimension == actual,
            (PropertyKind::Boolean, PropertyValueRef::Boolean(_))
            | (PropertyKind::Text, PropertyValueRef::Text(_)) => true,
            _ => false,
        };
        kind_matches && self.plugin.as_ref().is_none_or(|p| p.accepts(value))
    }
}

/// One component type's declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentSchema {
    /// The plugin-qualified component type this declares.
    pub type_id: ComponentTypeId,
    /// The declaration's version, recorded in instantiation provenance.
    pub version: SchemaVersion,
    /// The properties this component contributes, keyed by name.
    pub properties: BTreeMap<PropertyName, PropertySchema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plugin: Option<Box<orishu_plugin::ComponentSchema>>,
}

impl ComponentSchema {
    /// Start a component schema with no properties.
    pub fn new(type_id: ComponentTypeId, version: SchemaVersion) -> Self {
        Self {
            type_id,
            version,
            properties: BTreeMap::new(),
            plugin: None,
        }
    }

    /// Declare one property, replacing any earlier declaration of that name.
    #[must_use]
    pub fn with_property(mut self, name: PropertyName, schema: PropertySchema) -> Self {
        self.properties.insert(name, schema);
        self
    }

    /// Every property this component requires a template to author.
    pub fn required_properties(&self) -> impl Iterator<Item = &PropertyName> {
        self.properties
            .iter()
            .filter(|(_, schema)| schema.required)
            .map(|(name, _)| name)
    }

    /// Project one verified component payload under its exact provider pin.
    /// This checks integrity, not installation enablement or transitive
    /// availability: the inventory resolver must authorize use separately.
    /// Other contribution kinds (including opaque future slots) are refused.
    /// A registry remains caller-supplied authoring data, not an admission token;
    /// export/runtime admission must independently verify the exact release.
    pub fn from_plugin(
        release: &orishu_plugin::resolution::VerifiedRelease,
        local: &orishu_plugin::LocalContributionId,
    ) -> Result<Self, crate::name::NameError> {
        let Some(orishu_plugin::Payload::Components(declaration)) =
            release.payloads().get(local).and_then(|p| p.payload())
        else {
            return Err(crate::name::NameError::NotComponentContribution);
        };
        let reference = release
            .contribution_ref(local)
            .ok_or(crate::name::NameError::NotComponentContribution)?;
        let type_id = ComponentTypeId::exact(reference)?;
        let properties = declaration
            .scientific
            .properties
            .iter()
            .map(|p| Ok((property_name(&p.id)?, PropertySchema::from_plugin(p))))
            .collect::<Result<_, crate::name::NameError>>()?;
        Ok(Self {
            type_id,
            version: SchemaVersion(declaration.scientific.version.get()),
            properties,
            plugin: Some(Box::new(declaration.scientific.clone())),
        })
    }

    /// Original scientific role, bindings, requirements and property IDs.
    /// Expression aliases never replace the exact names used by workload export.
    pub fn plugin_declaration(&self) -> Option<&orishu_plugin::ComponentSchema> {
        self.plugin.as_deref()
    }
}

/// Injective expression spelling for a plugin's local property identifier.
/// Plugin identifiers exclude underscores; their hyphens map to underscores.
pub fn property_name(
    id: &orishu_plugin::LocalContributionId,
) -> Result<PropertyName, crate::name::NameError> {
    PropertyName::new(id.as_str().replace('-', "_"))
}

/// The component schemas installed in this Kagami installation.
///
/// Ordered by component type so every projection, report, and fingerprint
/// derived from it is deterministic.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaRegistry {
    components: BTreeMap<ComponentTypeId, ComponentSchema>,
}

impl SchemaRegistry {
    /// An installation with no simulation plugins: every template that names
    /// a component is unavailable, and none is invalid for that reason.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) one component schema.
    pub fn insert(&mut self, schema: ComponentSchema) {
        self.components.insert(schema.type_id.clone(), schema);
    }

    /// Builder form of [`Self::insert`].
    #[must_use]
    pub fn with(mut self, schema: ComponentSchema) -> Self {
        self.insert(schema);
        self
    }

    /// The schema for `type_id`, or `None` when the contributing plugin is
    /// not installed.
    pub fn get(&self, type_id: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.components.get(type_id)
    }

    /// Every registered schema, in component-type order.
    pub fn schemas(&self) -> impl Iterator<Item = &ComponentSchema> {
        self.components.values()
    }

    /// How many component types are installed.
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// `true` when no simulation plugin has contributed a component type.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::{ComponentName, PluginId};

    fn type_id(plugin: &str, name: &str) -> ComponentTypeId {
        ComponentTypeId::new(
            PluginId::new(plugin).unwrap(),
            ComponentName::new(name).unwrap(),
        )
    }

    fn property(name: &str) -> PropertyName {
        PropertyName::new(name).unwrap()
    }

    #[test]
    fn empty_registry_resolves_nothing() {
        let registry = SchemaRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.get(&type_id("kagami.mass", "inertial")), None);
    }

    #[test]
    fn registered_schema_is_resolvable_by_component_type() {
        let schema = ComponentSchema::new(type_id("kagami.mass", "inertial"), SchemaVersion(1))
            .with_property(
                property("mass"),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            );
        let registry = SchemaRegistry::new().with(schema.clone());
        assert_eq!(
            registry.get(&type_id("kagami.mass", "inertial")),
            Some(&schema)
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn required_properties_lists_only_the_required_ones() {
        let schema = ComponentSchema::new(type_id("kagami.mass", "inertial"), SchemaVersion(1))
            .with_property(
                property("mass"),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            )
            .with_property(
                property("label"),
                PropertySchema::optional(PropertyKind::Text),
            );
        assert_eq!(
            schema.required_properties().collect::<Vec<_>>(),
            vec![&property("mass")]
        );
    }

    #[test]
    fn only_quantities_are_expression_capable() {
        assert!(
            PropertyKind::Quantity {
                dimension: Dimension::MASS
            }
            .is_expression_capable()
        );
        assert!(!PropertyKind::Boolean.is_expression_capable());
        assert!(!PropertyKind::Text.is_expression_capable());
    }
}
