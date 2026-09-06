//! What a document does when the installation cannot govern part of it.
//!
//! The case this file exists for is the one persistence makes ordinary:
//! an experiment authored with a plugin installed, opened on a machine
//! without it. The authored content is not corrupt, the document is not
//! unusable, and nothing may quietly reinterpret the part that has no schema.

use std::collections::BTreeMap;

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, PluginId, PropertyKind,
    PropertyName, PropertySchema, SchemaRegistry, SchemaVersion,
};
use kagami_document::capability::{CapabilityGap, CapabilityReport, Participation};
use kagami_document::{
    AuthoredValue, DisplayName, Experiment, ExperimentCommand, Limits, ObjectId, ObjectSpec,
    Transform, Vector3, update,
};

fn mass_component() -> ComponentTypeId {
    ComponentTypeId::new(
        PluginId::new("kagami.mass_sources").expect("valid identifier"),
        ComponentName::new("inertial_mass").expect("valid identifier"),
    )
}

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("valid identifier")
}

fn name(value: &str) -> DisplayName {
    DisplayName::new(value).expect("valid label")
}

fn installed() -> SchemaRegistry {
    SchemaRegistry::new().with(
        ComponentSchema::new(mass_component(), SchemaVersion(1)).with_property(
            property("mass"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::MASS,
            }),
        ),
    )
}

fn mass_properties(expression: &str) -> BTreeMap<PropertyName, AuthoredValue> {
    let mut properties = BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::si(expression));
    properties
}

/// An experiment authored while the mass plugin was installed: one massive
/// object and one bare marker.
fn authored() -> (Experiment, ObjectId, ObjectId) {
    let (experiment, report) = update(
        &Experiment::new(),
        &[
            ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(name("Earth"))
                    .with_component(mass_component(), mass_properties("5.972e24")),
            )),
            ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(name("Marker")))),
        ],
        &installed(),
        &Limits::DEFAULT,
    )
    .expect("accepted while the plugin was installed")
    .adopt();

    let massive = report.created_objects[0];
    let marker = report.created_objects[1];
    (experiment, massive, marker)
}

/// The same experiment, read on a machine where that plugin is not installed.
fn uninstalled() -> SchemaRegistry {
    SchemaRegistry::new()
}

#[test]
fn content_no_installed_plugin_governs_is_preserved_and_reported() {
    let (experiment, massive, _) = authored();
    let snapshot = experiment.snapshot();

    // The values are still there, verbatim.
    let stored = snapshot
        .object(massive)
        .expect("preserved")
        .component(&mass_component())
        .expect("still attached");
    assert_eq!(
        stored.properties[&property("mass")].source(),
        Some("5.972e24")
    );
    assert_eq!(stored.schema_version, SchemaVersion(1));

    // And the reason it cannot be used names what to install, rather than
    // calling the document broken.
    let report = CapabilityReport::of(&snapshot, &uninstalled());
    assert_eq!(
        report.gap(massive, &mass_component()),
        Some(&CapabilityGap::SchemaNotInstalled)
    );
    assert!(
        !report.is_complete(),
        "workload compilation must not run over content nothing governs"
    );
}

#[test]
fn the_rest_of_the_experiment_stays_editable() {
    let (experiment, massive, marker) = authored();
    let bare = uninstalled();

    // Editing an unrelated object, editing the *object* carrying the
    // unavailable component, and adding new content all still work.
    let (experiment, _) = update(
        &experiment,
        &[
            ExperimentCommand::RenameObject {
                object: marker,
                name: name("Origin"),
            },
            ExperimentCommand::SetTransform {
                object: massive,
                transform: Transform::at(Vector3::new(1.0, 0.0, 0.0).expect("finite")),
            },
            ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(name("Moon")))),
        ],
        &bare,
        &Limits::DEFAULT,
    )
    .expect("an absent plugin does not freeze the document")
    .adopt();

    assert_eq!(experiment.snapshot().object_count(), 3);
    // The untouched component was not revalidated, resolved, or rewritten.
    assert_eq!(
        experiment
            .snapshot()
            .object(massive)
            .expect("exists")
            .component(&mass_component())
            .expect("still attached")
            .properties[&property("mass")]
            .source(),
        Some("5.972e24")
    );
}

#[test]
fn authoring_against_a_schema_that_is_not_installed_is_refused() {
    let (experiment, massive, _) = authored();
    let bare = uninstalled();

    // Replacing its values.
    let rejection = update(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: massive,
            component: mass_component(),
            properties: mass_properties("1"),
        }],
        &bare,
        &Limits::DEFAULT,
    )
    .expect_err("no declaration to check the new values against");
    assert_eq!(rejection.code(), "component_type_not_installed");

    // Editing one property of them.
    let rejection = update(
        &experiment,
        &[ExperimentCommand::SetComponentProperty {
            object: massive,
            component: mass_component(),
            property: property("mass"),
            value: AuthoredValue::si("1"),
        }],
        &bare,
        &Limits::DEFAULT,
    )
    .expect_err("no declaration to check the new value against");
    assert_eq!(rejection.code(), "component_type_not_installed");
}

#[test]
fn unavailable_content_can_still_be_removed() {
    let (experiment, massive, _) = authored();
    let bare = uninstalled();

    // Detaching needs no schema to be correct, and an author who cannot
    // interpret content must still be able to get rid of it.
    let (detached, _) = update(
        &experiment,
        &[ExperimentCommand::DetachComponent {
            object: massive,
            component: mass_component(),
        }],
        &bare,
        &Limits::DEFAULT,
    )
    .expect("detaching unavailable content is allowed")
    .adopt();
    assert!(CapabilityReport::of(&detached.snapshot(), &bare).is_complete());
    assert_eq!(
        detached
            .snapshot()
            .object(massive)
            .expect("the object survives its component")
            .participation(&bare),
        Participation::Inert
    );

    // As is removing the object outright.
    let (removed, _) = update(
        &experiment,
        &[ExperimentCommand::RemoveObject(massive)],
        &bare,
        &Limits::DEFAULT,
    )
    .expect("removing unavailable content is allowed")
    .adopt();
    assert!(CapabilityReport::of(&removed.snapshot(), &bare).is_complete());
}

#[test]
fn reinstalling_the_plugin_makes_the_same_content_usable_again() {
    let (experiment, massive, _) = authored();
    let snapshot = experiment.snapshot();

    assert!(!CapabilityReport::of(&snapshot, &uninstalled()).is_complete());
    // Nothing about the experiment changed in between: the *installation* did.
    assert!(CapabilityReport::of(&snapshot, &installed()).is_complete());

    let (edited, _) = update(
        &experiment,
        &[ExperimentCommand::SetComponentProperty {
            object: massive,
            component: mass_component(),
            property: property("mass"),
            value: AuthoredValue::si("5.9722e24"),
        }],
        &installed(),
        &Limits::DEFAULT,
    )
    .expect("the component is governed again")
    .adopt();

    assert_eq!(
        edited
            .snapshot()
            .object(massive)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .properties[&property("mass")]
            .source(),
        Some("5.9722e24")
    );
}

#[test]
fn an_edit_stamps_the_declaration_that_actually_checked_the_value() {
    let (experiment, massive, _) = authored();

    // The plugin was upgraded. The declaration still asks for the same thing,
    // so the component is usable; editing it records that v2 accepted the
    // value, rather than leaving a v1 stamp on a value v1 never saw.
    let upgraded = SchemaRegistry::new().with(
        ComponentSchema::new(mass_component(), SchemaVersion(2)).with_property(
            property("mass"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::MASS,
            }),
        ),
    );
    let (edited, _) = update(
        &experiment,
        &[ExperimentCommand::SetComponentProperty {
            object: massive,
            component: mass_component(),
            property: property("mass"),
            value: AuthoredValue::si("6e24"),
        }],
        &upgraded,
        &Limits::DEFAULT,
    )
    .expect("a version bump alone does not make content unusable")
    .adopt();

    assert_eq!(
        edited
            .snapshot()
            .object(massive)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .schema_version,
        SchemaVersion(2)
    );
}

#[test]
fn a_batch_is_governed_as_a_whole_rather_than_command_by_command() {
    let (experiment, _, marker) = authored();

    // Attaching without the required property and supplying it in a later
    // command is one valid edit: the check runs over the finished candidate.
    let (experiment, _) = update(
        &experiment,
        &[
            ExperimentCommand::AttachComponent {
                object: marker,
                component: mass_component(),
                properties: BTreeMap::new(),
            },
            ExperimentCommand::SetComponentProperty {
                object: marker,
                component: mass_component(),
                property: property("mass"),
                value: AuthoredValue::si("7.35e22"),
            },
        ],
        &installed(),
        &Limits::DEFAULT,
    )
    .expect("the required property is authored before the batch ends")
    .adopt();

    assert_eq!(
        experiment
            .snapshot()
            .object(marker)
            .expect("exists")
            .participation(&installed()),
        Participation::Participating
    );

    // The same batch without the second command is refused as a whole.
    let rejection = update(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: marker,
            component: mass_component(),
            properties: BTreeMap::new(),
        }],
        &installed(),
        &Limits::DEFAULT,
    )
    .expect_err("mass is required");
    assert_eq!(rejection.code(), "required_property_missing");
}
