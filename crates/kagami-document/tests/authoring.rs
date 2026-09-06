//! Authoring behaviour, exercised through the public API only.
//!
//! These are the properties ADR 0019 turns on — atomicity, derived magnitudes,
//! never-reused identities, forward-restoring undo — asserted the way a
//! caller sees them, so a refactor that preserves the contract cannot break
//! them and one that breaks the contract cannot pass.

use std::collections::BTreeMap;

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, PluginId, PropertyKind,
    PropertyName, PropertySchema, SchemaRegistry, SchemaVersion,
};
use kagami_document::capability::Participation;
use kagami_document::{
    AuthoredValue, DisplayName, EditHistory, Experiment, ExperimentCommand, ExperimentRevision,
    GestureId, Limits, ObjectShape, ObjectSpec, Rejection, Transform, Vector3, restore, update,
};

fn plugin() -> PluginId {
    PluginId::new("kagami.mass_sources").expect("valid identifier")
}

fn mass_component() -> ComponentTypeId {
    ComponentTypeId::new(
        plugin(),
        ComponentName::new("inertial_mass").expect("valid identifier"),
    )
}

fn charge_component() -> ComponentTypeId {
    ComponentTypeId::new(
        PluginId::new("kagami.em_sources").expect("valid identifier"),
        ComponentName::new("point_charge").expect("valid identifier"),
    )
}

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("valid identifier")
}

fn name(value: &str) -> DisplayName {
    DisplayName::new(value).expect("valid label")
}

/// An installation with a mass plugin and an electromagnetism plugin, so a
/// single object can carry both without either knowing about the other.
fn schemas() -> SchemaRegistry {
    SchemaRegistry::new()
        .with(
            ComponentSchema::new(mass_component(), SchemaVersion(1)).with_property(
                property("mass"),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            ),
        )
        .with(
            ComponentSchema::new(charge_component(), SchemaVersion(3))
                .with_property(
                    property("charge"),
                    PropertySchema::required(PropertyKind::Quantity {
                        dimension: Dimension::CHARGE,
                    }),
                )
                .with_property(
                    property("label"),
                    PropertySchema::optional(PropertyKind::Text),
                ),
        )
}

fn mass_properties(expression: &str) -> BTreeMap<PropertyName, AuthoredValue> {
    let mut properties = BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::si(expression));
    properties
}

fn create(label: &str) -> ExperimentCommand {
    ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(name(label))))
}

/// Apply a batch and adopt it, panicking on rejection.
fn commit(
    experiment: &Experiment,
    commands: &[ExperimentCommand],
) -> (Experiment, kagami_document::CommitReport) {
    update(experiment, commands, &schemas(), &Limits::DEFAULT)
        .expect("batch should be accepted")
        .adopt()
}

#[test]
fn an_object_is_created_bare_and_composed_afterwards() {
    let experiment = Experiment::new();
    let (experiment, report) = commit(&experiment, &[create("Earth")]);
    let id = report.first_created().expect("one object created");

    // Created with no physics at all. That is a valid scene object, not an
    // incomplete one.
    assert_eq!(
        experiment
            .snapshot()
            .object(id)
            .expect("exists")
            .participation(&schemas()),
        Participation::Inert
    );

    let (experiment, _) = commit(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: id,
            component: mass_component(),
            properties: mass_properties("5.972e24"),
        }],
    );
    let (experiment, _) = commit(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: id,
            component: charge_component(),
            properties: {
                let mut properties = BTreeMap::new();
                properties.insert(property("charge"), AuthoredValue::si("0"));
                properties
            },
        }],
    );

    // One object carrying components from two unrelated plugins.
    let snapshot = experiment.snapshot();
    let object = snapshot.object(id).expect("exists");
    assert_eq!(object.components.len(), 2);
    assert_eq!(
        object
            .component(&mass_component())
            .expect("attached")
            .schema_version,
        SchemaVersion(1)
    );

    // Detaching leaves the object valid rather than deleting it.
    let (experiment, _) = commit(
        &experiment,
        &[ExperimentCommand::DetachComponent {
            object: id,
            component: charge_component(),
        }],
    );
    assert_eq!(
        experiment
            .snapshot()
            .object(id)
            .expect("still there")
            .components
            .len(),
        1
    );
}

#[test]
fn each_accepted_batch_advances_the_revision_by_exactly_one() {
    let experiment = Experiment::new();
    assert_eq!(experiment.revision(), ExperimentRevision::INITIAL);

    let (experiment, report) = commit(&experiment, &[create("a"), create("b"), create("c")]);
    assert_eq!(experiment.revision(), ExperimentRevision::INITIAL.next());
    assert_eq!(report.revision, experiment.revision());
    assert_eq!(report.created_objects.len(), 3);
    assert_eq!(report.label, "Add object and 2 more");
}

#[test]
fn a_batch_that_fails_on_its_last_command_changes_nothing() {
    let (experiment, report) = commit(&Experiment::new(), &[create("Earth"), create("Moon")]);
    let id = report.created_objects[0];
    let moon = report.created_objects[1];

    // Remove the Moon, so its identity is one that *was* real and no longer is
    // — the stale-identity case a client hits when it submits against a view
    // it read a moment ago.
    let (experiment, _) = commit(&experiment, &[ExperimentCommand::RemoveObject(moon)]);
    let before = experiment.clone();

    let rejection = update(
        &experiment,
        &[
            ExperimentCommand::RenameObject {
                object: id,
                name: name("Terra"),
            },
            ExperimentCommand::SetShape {
                object: id,
                shape: Some(ObjectShape::sphere(6.371e6).expect("valid radius")),
            },
            // The offending command: nothing before it may survive.
            ExperimentCommand::RemoveObject(moon),
        ],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("the last command names a removed object");

    assert_eq!(rejection.code(), "unknown_object");
    assert_eq!(experiment, before);
    assert_eq!(
        experiment
            .snapshot()
            .object(id)
            .expect("exists")
            .name
            .as_str(),
        "Earth"
    );
    assert_eq!(
        experiment.snapshot().object(id).expect("exists").shape,
        None
    );
}

#[test]
fn a_component_from_an_uninstalled_plugin_cannot_be_attached() {
    let experiment = Experiment::new();
    let (experiment, report) = commit(&experiment, &[create("Earth")]);
    let id = report.first_created().expect("one object");

    let unknown = ComponentTypeId::new(
        PluginId::new("third.party").expect("valid identifier"),
        ComponentName::new("exotic").expect("valid identifier"),
    );
    let rejection = update(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: id,
            component: unknown.clone(),
            properties: BTreeMap::new(),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("no plugin contributes that component");

    // A fact about this installation, and it names what to install.
    assert_eq!(
        rejection,
        Rejection::ComponentTypeNotInstalled { component: unknown }
    );
}

#[test]
fn a_component_missing_a_required_property_is_refused_by_path() {
    let experiment = Experiment::new();
    let (experiment, report) = commit(&experiment, &[create("Earth")]);
    let id = report.first_created().expect("one object");

    let rejection = update(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: id,
            component: mass_component(),
            properties: BTreeMap::new(),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("mass is required");

    assert_eq!(rejection.code(), "required_property_missing");
    assert!(
        rejection.to_string().contains("inertial_mass.mass"),
        "the rejection must name the property: {rejection}"
    );
}

#[test]
fn a_property_the_schema_does_not_declare_is_refused() {
    let experiment = Experiment::new();
    let (experiment, report) = commit(&experiment, &[create("Earth")]);
    let id = report.first_created().expect("one object");

    let mut properties = mass_properties("1");
    properties.insert(property("colour"), AuthoredValue::Text("blue".to_owned()));

    let rejection = update(
        &experiment,
        &[ExperimentCommand::AttachComponent {
            object: id,
            component: mass_component(),
            properties,
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("the schema declares no `colour`");

    assert_eq!(rejection.code(), "property_not_declared");
}

#[test]
fn an_authored_quantity_retains_its_source_and_reports_a_derived_magnitude() {
    let experiment = Experiment::new();
    let (experiment, report) = commit(
        &experiment,
        &[ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Half a sun"))
                .with_component(mass_component(), mass_properties("1.989e30 / 2")),
        ))],
    );
    let id = report.first_created().expect("one object");

    let snapshot = experiment.snapshot();
    let value = &snapshot
        .object(id)
        .expect("exists")
        .component(&mass_component())
        .expect("attached")
        .properties[&property("mass")];

    // The source is the intent; the number is derived from it.
    assert_eq!(value.source(), Some("1.989e30 / 2"));
    assert_eq!(value.si_value(), Some(9.945e29));
}

#[test]
fn a_unit_bearing_quantity_is_stored_in_canonical_si() {
    let gram = *kagami_catalog::quantity::lookup("g").expect("known unit");
    let mut properties = BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::in_unit("2.7", gram));

    let (experiment, report) = commit(
        &Experiment::new(),
        &[ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Sample")).with_component(mass_component(), properties),
        ))],
    );
    let id = report.first_created().expect("one object");
    let snapshot = experiment.snapshot();
    assert_eq!(
        snapshot
            .object(id)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .properties[&property("mass")]
            .si_value(),
        Some(2.7e-3)
    );
}

#[test]
fn a_non_finite_result_is_never_stored() {
    let experiment = Experiment::new();
    for expression in ["1 / 0", "1e308 * 1e308"] {
        let rejection = update(
            &experiment,
            &[ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(name("Bad"))
                    .with_component(mass_component(), mass_properties(expression)),
            ))],
            &schemas(),
            &Limits::DEFAULT,
        )
        .expect_err("must not resolve");
        assert_eq!(
            rejection.code(),
            "expression_unresolved",
            "for {expression}"
        );
    }
    assert_eq!(experiment.snapshot().object_count(), 0);
}

#[test]
fn an_oversized_batch_is_refused_before_anything_is_applied() {
    let limits = Limits {
        max_commands_per_batch: 2,
        ..Limits::DEFAULT
    };
    let experiment = Experiment::new();
    let rejection = update(
        &experiment,
        &[create("a"), create("b"), create("c")],
        &schemas(),
        &limits,
    )
    .expect_err("over the batch limit");

    assert_eq!(rejection, Rejection::BatchTooLarge { found: 3, limit: 2 });
    // Not even an identity was minted.
    assert_eq!(experiment.counters().objects_minted(), 0);
}

#[test]
fn an_over_full_experiment_is_refused_as_a_whole() {
    let limits = Limits {
        max_objects: 2,
        ..Limits::DEFAULT
    };
    let experiment = Experiment::new();
    let rejection = update(
        &experiment,
        &[create("a"), create("b"), create("c")],
        &schemas(),
        &limits,
    )
    .expect_err("over the object limit");

    // The bound is on the finished candidate, so a batch cannot stay under it
    // at each step and exceed it in aggregate.
    assert_eq!(rejection, Rejection::TooManyObjects { found: 3, limit: 2 });
}

#[test]
fn undo_restores_contents_forwards_and_never_reuses_an_identity() {
    let schemas = schemas();
    let limits = Limits::DEFAULT;
    let mut history = EditHistory::default();

    let experiment = Experiment::new();
    history.record(experiment.checkpoint(), "Add object", None);
    let (experiment, report) = commit(&experiment, &[create("Earth")]);
    let first = report.first_created().expect("one object");
    assert_eq!(experiment.revision(), ExperimentRevision::INITIAL.next());

    // Undo: the contents come back, but as a *later* revision.
    let restoration = history.undo(experiment.checkpoint()).expect("one entry");
    assert_eq!(restoration.label, "Add object");
    let (undone, _) = restore(&experiment, &restoration.checkpoint, &schemas, &limits)
        .expect("still representable")
        .adopt();

    assert_eq!(undone.snapshot().object_count(), 0);
    assert!(undone.revision() > experiment.revision());
    // The identity counter did not move backwards, so undoing a creation
    // frees nothing.
    assert_eq!(undone.counters().objects_minted(), 1);

    // Creating again therefore cannot inherit the removed object's identity.
    let (recreated, report) = commit(&undone, &[create("Earth")]);
    let second = report.first_created().expect("one object");
    assert_ne!(second, first);
    assert!(recreated.snapshot().object(first).is_none());
}

#[test]
fn redo_reapplies_what_undo_reversed() {
    let schemas = schemas();
    let limits = Limits::DEFAULT;
    let mut history = EditHistory::default();

    let start = Experiment::new();
    history.record(start.checkpoint(), "Add object", None);
    let (added, _) = commit(&start, &[create("Earth")]);

    let restoration = history.undo(added.checkpoint()).expect("one entry");
    let (undone, _) = restore(&added, &restoration.checkpoint, &schemas, &limits)
        .expect("valid")
        .adopt();
    assert_eq!(undone.snapshot().object_count(), 0);

    let restoration = history.redo(undone.checkpoint()).expect("one undone entry");
    let (redone, _) = restore(&undone, &restoration.checkpoint, &schemas, &limits)
        .expect("valid")
        .adopt();
    assert_eq!(redone.snapshot().object_count(), 1);
    assert!(redone.revision() > undone.revision());
}

#[test]
fn one_gesture_is_one_history_entry_however_many_commits_it_makes() {
    let mut history = EditHistory::default();
    let gesture = GestureId::new(1);

    let (mut experiment, report) = commit(&Experiment::new(), &[create("Earth")]);
    let id = report.first_created().expect("one object");
    history.clear();

    // A drag: one command per frame, all inside one gesture.
    for frame in 0..64 {
        history.record(experiment.checkpoint(), "Move object", Some(gesture));
        let translation = Vector3::new(f64::from(frame), 0.0, 0.0).expect("finite");
        let (next, _) = commit(
            &experiment,
            &[ExperimentCommand::SetTransform {
                object: id,
                transform: Transform::at(translation),
            }],
        );
        experiment = next;
    }

    assert_eq!(history.len(), 1, "a drag must undo as one step");
    assert_eq!(history.undo_label(), Some("Move object"));

    // Undoing it returns to where the gesture started, not to the previous
    // frame.
    let restoration = history.undo(experiment.checkpoint()).expect("one entry");
    let (undone, _) = restore(
        &experiment,
        &restoration.checkpoint,
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("valid")
    .adopt();
    assert_eq!(
        undone.snapshot().object(id).expect("exists").transform,
        Transform::IDENTITY
    );
}

#[test]
fn restoring_contents_whose_plugin_was_uninstalled_preserves_them() {
    let limits = Limits::DEFAULT;
    let (experiment, report) = commit(
        &Experiment::new(),
        &[ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Earth"))
                .with_component(mass_component(), mass_properties("5.972e24")),
        ))],
    );
    let id = report.first_created().expect("one object");
    let checkpoint = experiment.checkpoint();

    // The plugin contributing `inertial_mass` is uninstalled mid-session.
    // Refusing the restore would mean uninstalling a plugin silently disables
    // undo, so the captured contents come back and the component is reported
    // unavailable — exactly as loading the same document here would.
    let without_mass_plugin = SchemaRegistry::new();
    let (restored, _) = restore(&experiment, &checkpoint, &without_mass_plugin, &limits)
        .expect("absent plugins do not make captured contents unrestorable")
        .adopt();

    let snapshot = restored.snapshot();
    let object = snapshot.object(id).expect("preserved");
    assert_eq!(
        object
            .component(&mass_component())
            .expect("still attached")
            .properties[&property("mass")]
            .source(),
        Some("5.972e24"),
        "the authored intent survives its plugin"
    );
    assert_eq!(
        object.participation(&without_mass_plugin),
        Participation::Unavailable
    );
}

#[test]
fn restoring_contents_an_installed_schema_refuses_is_refused() {
    let limits = Limits::DEFAULT;
    let (experiment, _) = commit(
        &Experiment::new(),
        &[ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Earth"))
                .with_component(mass_component(), mass_properties("5.972e24")),
        ))],
    );
    let checkpoint = experiment.checkpoint();

    // The plugin is still installed, but now declares `mass` as a length.
    // Reinstating the captured values would assert numbers that declaration
    // rejects, and nothing here may reinterpret them to fit.
    let reshaped = SchemaRegistry::new().with(
        ComponentSchema::new(mass_component(), SchemaVersion(2)).with_property(
            property("mass"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::LENGTH,
            }),
        ),
    );
    let rejection = restore(&experiment, &checkpoint, &reshaped, &limits)
        .expect_err("the installed declaration refuses these values");
    assert_eq!(rejection.code(), "component_incompatible");
}

#[test]
fn setup_edits_are_ordinary_commands() {
    let (experiment, _) = commit(
        &Experiment::new(),
        &[
            ExperimentCommand::SetTimeStep(
                kagami_document::TimeStep::new(1.0e-9).expect("positive"),
            ),
            ExperimentCommand::SetPluginEnabled {
                plugin: plugin(),
                enabled: true,
            },
        ],
    );

    let snapshot = experiment.snapshot();
    assert_eq!(snapshot.setup().time_step.seconds(), 1.0e-9);
    assert!(snapshot.setup().plugins.is_enabled(&plugin()));

    let (experiment, _) = commit(
        &experiment,
        &[ExperimentCommand::SetPluginEnabled {
            plugin: plugin(),
            enabled: false,
        }],
    );
    assert!(experiment.snapshot().setup().plugins.is_empty());
}

#[test]
fn a_raw_identity_resolves_only_while_its_object_exists() {
    let (experiment, report) = commit(&Experiment::new(), &[create("Earth")]);
    let id = report.first_created().expect("one object");
    let raw = id.get();

    // This is how an MCP adapter turns a wire integer into an identity.
    assert_eq!(experiment.snapshot().resolve_object(raw), Some(id));
    assert_eq!(experiment.snapshot().resolve_object(raw + 1), None);

    let (experiment, _) = commit(&experiment, &[ExperimentCommand::RemoveObject(id)]);
    assert_eq!(experiment.snapshot().resolve_object(raw), None);
}

#[test]
fn objects_can_be_found_by_the_component_they_carry() {
    let (experiment, _) = commit(
        &Experiment::new(),
        &[
            ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(name("Massive"))
                    .with_component(mass_component(), mass_properties("1")),
            )),
            create("Inert"),
        ],
    );

    let snapshot = experiment.snapshot();
    let component = mass_component();
    let carriers: Vec<_> = snapshot
        .objects_with(&component)
        .map(|(_, object, _)| object.name.as_str())
        .collect();
    assert_eq!(carriers, vec!["Massive"]);
}
