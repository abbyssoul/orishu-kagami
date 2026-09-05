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
}

impl PropertySchema {
    /// An optional property of the given kind.
    pub fn optional(kind: PropertyKind) -> Self {
        Self {
            kind,
            required: false,
        }
    }

    /// A property a template must author.
    pub fn required(kind: PropertyKind) -> Self {
        Self {
            kind,
            required: true,
        }
    }
}

/// One component type's declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentSchema {
    /// The plugin-qualified component type this declares.
    pub type_id: ComponentTypeId,
    /// The declaration's version, recorded in instantiation provenance.
    pub version: SchemaVersion,
    /// The properties this component contributes, keyed by name.
    pub properties: BTreeMap<PropertyName, PropertySchema>,
}

impl ComponentSchema {
    /// Start a component schema with no properties.
    pub fn new(type_id: ComponentTypeId, version: SchemaVersion) -> Self {
        Self {
            type_id,
            version,
            properties: BTreeMap::new(),
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
}

/// The component schemas installed in this Kagami installation.
///
/// Ordered by component type so every projection, report, and fingerprint
/// derived from it is deterministic.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
