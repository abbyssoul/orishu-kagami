//! Golden fixtures for the document's adapter boundary, and the bounds a
//! message cannot get past.
//!
//! Two properties are asserted here, and they are why the boundary exists at
//! all. The *shape* is pinned by a fixture, so a field rename is a deliberate
//! act rather than a surprise for whatever is decoding on the other side. And
//! *decoding is not a second ingress*: everything that reaches a command goes
//! through the same constructor a Rust caller must use, so no message can
//! assert a value the model would refuse.
//!
//! Set `KAGAMI_WIRE_BLESS=1` to rewrite the fixtures after an intentional
//! change to the representation.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, PluginId, PropertyKind,
    PropertyName, PropertySchema, SchemaRegistry, SchemaVersion,
};
use kagami_document::wire::{WireObjectSpec, WireValue};
use kagami_document::{
    AuthoredValue, DisplayName, Experiment, ExperimentCommand, Limits, ObjectShape, ObjectSpec,
    Transform, Vector3, WireCommand, WireSnapshot, update,
};
use serde::{Serialize, de::DeserializeOwned};

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

fn schemas() -> SchemaRegistry {
    SchemaRegistry::new().with(
        ComponentSchema::new(mass_component(), SchemaVersion(1)).with_property(
            property("mass"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::MASS,
            }),
        ),
    )
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"))
}

/// Assert that `value` serialises to the fixture and the fixture deserialises
/// back to `value`.
fn assert_fixture<T>(name: &str, value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("serialization must succeed")
    );
    let path = fixture_path(name);

    if std::env::var("KAGAMI_WIRE_BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().expect("fixtures directory"))
            .expect("fixture directory must be creatable");
        std::fs::write(&path, &rendered).expect("fixture must be writable");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}. Re-run with KAGAMI_WIRE_BLESS=1 to create it.",
            path.display()
        )
    });
    assert_eq!(
        rendered, expected,
        "the wire shape of `{name}` changed; if that is intended, re-bless the fixture and say \
         so wherever the representation is documented"
    );

    let parsed: T = serde_json::from_str(&expected).expect("fixture must deserialize");
    assert_eq!(&parsed, value, "`{name}` must round-trip");
}

/// One object carrying a mass, and one bare marker.
fn experiment() -> Experiment {
    let mut properties = BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::si("5.972e24"));
    let (experiment, _) = update(
        &Experiment::new(),
        &[
            ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(name("Earth"))
                    .with_component(mass_component(), properties)
                    .with_shape(ObjectShape::sphere(6.371e6).expect("valid radius")),
            )),
            ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(name("Marker")))),
        ],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();
    experiment
}

#[test]
fn a_creation_command_pins_its_shape() {
    let mut properties = BTreeMap::new();
    properties.insert(
        property("mass"),
        WireValue::Quantity {
            expression: "1.989e30 / 2".to_owned(),
            unit: None,
        },
    );
    assert_fixture(
        "create_object_command",
        &WireCommand::CreateObject {
            spec: Box::new(WireObjectSpec {
                name: name("Half a sun"),
                transform: Transform::at(Vector3::new(1.0, 2.0, 3.0).expect("finite")),
                velocity: Default::default(),
                shape: Some(ObjectShape::sphere(6.9634e8).expect("valid radius")),
                components: vec![kagami_document::wire::WireComponentValues {
                    component: mass_component(),
                    properties,
                }],
            }),
        },
    );
}

#[test]
fn a_property_edit_pins_its_shape() {
    assert_fixture(
        "set_component_property_command",
        &WireCommand::SetComponentProperty {
            object: 0,
            component: mass_component(),
            property: property("mass"),
            value: WireValue::Quantity {
                expression: "2.7".to_owned(),
                unit: Some("g".to_owned()),
            },
        },
    );
}

#[test]
fn a_read_projection_pins_its_shape() {
    assert_fixture("snapshot", &WireSnapshot::of(&experiment().snapshot()));
}

#[test]
fn every_command_survives_a_round_trip_through_the_representation() {
    let experiment = experiment();
    let snapshot = experiment.snapshot();
    let earth = snapshot.resolve_object(0).expect("the first object");

    let mut properties = BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::si("1"));
    let gram = *kagami_catalog::quantity::lookup("g").expect("known unit");

    let commands = vec![
        ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Moon")).with_component(mass_component(), properties.clone()),
        )),
        ExperimentCommand::RemoveObject(earth),
        ExperimentCommand::RenameObject {
            object: earth,
            name: name("Terra"),
        },
        ExperimentCommand::SetTransform {
            object: earth,
            transform: Transform::at(Vector3::new(1.0, -2.0, 3.5).expect("finite")),
        },
        ExperimentCommand::SetVelocity {
            object: earth,
            velocity: Default::default(),
        },
        ExperimentCommand::SetShape {
            object: earth,
            shape: Some(ObjectShape::boxed(Vector3::new(1.0, 2.0, 0.0).expect("finite")).unwrap()),
        },
        ExperimentCommand::SetShape {
            object: earth,
            shape: None,
        },
        ExperimentCommand::AttachComponent {
            object: earth,
            component: mass_component(),
            properties,
        },
        ExperimentCommand::DetachComponent {
            object: earth,
            component: mass_component(),
        },
        ExperimentCommand::SetComponentProperty {
            object: earth,
            component: mass_component(),
            property: property("mass"),
            value: AuthoredValue::in_unit("2.7", gram),
        },
        ExperimentCommand::SetDomain(Default::default()),
        ExperimentCommand::SetTimeStep(kagami_document::TimeStep::new(1e-9).expect("positive")),
        ExperimentCommand::SetPluginEnabled {
            plugin: PluginId::new("kagami.mass_sources").expect("valid identifier"),
            enabled: true,
        },
    ];

    for command in commands {
        let encoded = WireCommand::of(&command);
        let json = serde_json::to_string(&encoded).expect("encodes");
        let decoded: WireCommand = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, encoded, "the JSON form must round-trip");
        assert_eq!(
            decoded.into_command(&snapshot).expect("resolves"),
            command,
            "the command must survive the representation unchanged"
        );
    }
}

#[test]
fn an_identity_that_names_no_object_is_refused_rather_than_invented() {
    let experiment = experiment();
    let snapshot = experiment.snapshot();

    // There is no object 99, so no `ObjectId` can be made for it. This is the
    // whole reason decoding takes a snapshot.
    let error = WireCommand::RemoveObject { object: 99 }
        .into_command(&snapshot)
        .expect_err("no such object");
    assert_eq!(error.code(), "unknown_object");
}

#[test]
fn a_message_cannot_widen_a_bound_the_model_enforces() {
    // Each of these is well-formed JSON in the right shape, and each names a
    // value a constructor refuses. None may decode.
    let refused = [
        // An empty display name.
        r#"{"command":"renameObject","object":0,"name":""}"#,
        // A sphere with no extent.
        r#"{"command":"setShape","object":0,"shape":{"shape":"sphere","radius":-1}}"#,
        // A non-finite coordinate.
        r#"{"command":"setTransform","object":0,
             "transform":{"translation":[1e400,0,0],"rotation":[0,0,0,1]}}"#,
        // A time step that no solver could advance by.
        r#"{"command":"setTimeStep","time_step":0}"#,
        // An inverted domain.
        r#"{"command":"setDomain","domain":{"lower":[1,1,1],"upper":[0,0,0],
             "cells":[4,4,4],"boundary":["periodic","periodic","periodic"]}}"#,
        // A component type whose plugin identifier could never be a variable
        // segment.
        r#"{"command":"detachComponent","object":0,
             "component":{"plugin":"kagami.mass-sources","name":"inertial_mass"}}"#,
    ];
    for message in refused {
        assert!(
            serde_json::from_str::<WireCommand>(message).is_err(),
            "decoding must refuse: {message}"
        );
    }
}

#[test]
fn a_unit_the_product_does_not_know_is_refused_when_the_command_is_built() {
    let experiment = experiment();
    let snapshot = experiment.snapshot();

    // The symbol is a perfectly good string, so it deserialises; what it
    // cannot do is become an authored quantity.
    let message = r#"{"command":"setComponentProperty","object":0,
        "component":{"plugin":"kagami.mass_sources","name":"inertial_mass"},
        "property":"mass","value":{"kind":"quantity","expression":"1","unit":"furlong"}}"#;
    let decoded: WireCommand = serde_json::from_str(message).expect("well-formed message");
    let error = decoded
        .into_command(&snapshot)
        .expect_err("furlongs are not in the unit table");
    assert_eq!(error.code(), "unknown_unit");
}

#[test]
fn a_decoded_command_is_still_only_a_proposal() {
    // Decoding resolves an identity against a read; it decides nothing. The
    // object may be gone by the time the command is applied, and the model
    // refuses it then — which is why decoding is not a second validation path.
    let experiment = experiment();
    let snapshot = experiment.snapshot();
    let earth = snapshot.resolve_object(0).expect("the first object");

    let decoded = WireCommand::RenameObject {
        object: earth.get(),
        name: name("Terra"),
    }
    .into_command(&snapshot)
    .expect("resolves against the read");

    let (experiment, _) = update(
        &experiment,
        &[ExperimentCommand::RemoveObject(earth)],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();

    let rejection = update(&experiment, &[decoded], &schemas(), &Limits::DEFAULT)
        .expect_err("the object was removed in between");
    assert_eq!(rejection.code(), "unknown_object");
}
