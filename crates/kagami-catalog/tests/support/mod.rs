//! The component schemas the shipped example catalogs are written against.
//!
//! Simulation plugins will eventually contribute these. Until the plugin
//! subsystem exists, declaring them here keeps the catalog's dependency on a
//! schema registry an *input* rather than an assumption: the same catalog
//! files resolve differently against a different registry, which is exactly
//! the property the availability tests exercise.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, PluginId, PropertyKind,
    PropertyName, PropertySchema, SchemaRegistry, SchemaVersion, TemplateIdentity,
};

/// The directory holding the catalogs shipped with this repository.
pub fn example_catalog_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../etc/catalogs")
}

/// Build a plugin-qualified component type from its parts.
pub fn component(plugin: &str, name: &str) -> ComponentTypeId {
    ComponentTypeId::new(
        PluginId::new(plugin).expect("a valid plugin identifier"),
        ComponentName::new(name).expect("a valid component name"),
    )
}

/// Build a template identity from its parts.
pub fn identity(catalog: &str, template: &str) -> TemplateIdentity {
    TemplateIdentity::new(
        catalog.try_into().expect("a valid catalog name"),
        template.try_into().expect("a valid template name"),
    )
}

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("a valid property name")
}

fn quantity(dimension: Dimension) -> PropertyKind {
    PropertyKind::Quantity { dimension }
}

/// Only the mass components: enough for `planets`, not for `particles`.
pub fn mass_only_registry() -> SchemaRegistry {
    SchemaRegistry::new()
        .with(
            ComponentSchema::new(
                component("kagami.mass_sources", "inertial_mass"),
                SchemaVersion(1),
            )
            .with_property(
                property("mass"),
                PropertySchema::required(quantity(Dimension::MASS)),
            ),
        )
        .with(
            ComponentSchema::new(
                component("kagami.mass_sources", "gravitational_mass"),
                SchemaVersion(1),
            )
            .with_property(
                property("follows_inertial"),
                PropertySchema::optional(PropertyKind::Boolean),
            ),
        )
        .with(
            ComponentSchema::new(component("kagami.geometry", "sphere"), SchemaVersion(2))
                .with_property(
                    property("radius"),
                    PropertySchema::required(quantity(Dimension::LENGTH)),
                ),
        )
}

/// Every component type the shipped catalogs use.
pub fn full_registry() -> SchemaRegistry {
    mass_only_registry()
        .with(
            ComponentSchema::new(
                component("kagami.electromagnetic_sources", "charge_source"),
                SchemaVersion(1),
            )
            .with_property(
                property("charge"),
                PropertySchema::required(quantity(Dimension::CHARGE)),
            ),
        )
        .with(
            ComponentSchema::new(component("kagami.geometry", "point"), SchemaVersion(2))
                .with_property(
                    property("exclusion_radius"),
                    PropertySchema::required(quantity(Dimension::LENGTH)),
                ),
        )
}
