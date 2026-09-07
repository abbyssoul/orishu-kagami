//! Document variables, exercised through the public API.
//!
//! These are K2's acceptance cases. The behaviour they pin down is that a
//! definition is *authored intent* — what a save writes is the expression,
//! and every magnitude anyone reads is derived from it — and that one accepted
//! edit reprices every dependant atomically or is refused whole.

use std::collections::BTreeMap;

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, PluginId, PropertyKind,
    PropertyName, PropertySchema, SchemaRegistry, SchemaVersion,
};
use kagami_document::{
    AuthoredValue, DisplayName, Experiment, ExperimentCommand, Limits, ObjectId, ObjectSpec,
    VariableId, VariableSpec, update,
};
use orishu_variables::{Name, Namespace};

fn mass_component() -> ComponentTypeId {
    ComponentTypeId::new(
        PluginId::new("kagami.mass_sources").expect("valid identifier"),
        ComponentName::new("inertial_mass").expect("valid identifier"),
    )
}

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("valid identifier")
}

fn variable_name(value: &str) -> Name {
    Name::new(value).expect("valid identifier")
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

fn label(value: &str) -> DisplayName {
    DisplayName::new(value).expect("valid label")
}

fn define(name: &str, expression: &str) -> ExperimentCommand {
    ExperimentCommand::DefineVariable(Box::new(VariableSpec::new(variable_name(name), expression)))
}

/// An object whose mass is authored as `expression`.
fn massive(name: &str, expression: &str) -> ExperimentCommand {
    let mut properties = BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::si(expression));
    ExperimentCommand::CreateObject(Box::new(
        ObjectSpec::new(label(name)).with_component(mass_component(), properties),
    ))
}

fn commit(experiment: &Experiment, commands: &[ExperimentCommand]) -> Experiment {
    update(experiment, commands, &schemas(), &Limits::DEFAULT)
        .expect("batch should be accepted")
        .adopt()
        .0
}

/// The stored SI magnitude of an object's mass.
fn mass_of(experiment: &Experiment, object: ObjectId) -> f64 {
    experiment
        .snapshot()
        .object(object)
        .expect("exists")
        .component(&mass_component())
        .expect("attached")
        .properties[&property("mass")]
        .si_value()
        .expect("a quantity")
}

/// A sun definition and a planet whose mass reads it.
fn solar_system() -> (Experiment, VariableId, ObjectId) {
    let (experiment, report) = update(
        &Experiment::new(),
        &[
            define("mass_of_sun", "1.989e30 kg"),
            massive("Half a sun", "mass_of_sun / 2"),
        ],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();
    let variable = report.first_variable().expect("one definition");
    let object = report.first_created().expect("one object");
    (experiment, variable, object)
}

#[test]
fn a_property_may_be_authored_in_terms_of_a_definition() {
    let (experiment, _, object) = solar_system();

    // The source is the intent; the magnitude is derived from it.
    let stored = experiment.snapshot();
    let value = &stored
        .object(object)
        .expect("exists")
        .component(&mass_component())
        .expect("attached")
        .properties[&property("mass")];
    assert_eq!(value.source(), Some("mass_of_sun / 2"));
    assert_eq!(value.si_value(), Some(1.989e30 / 2.0));

    // And the definition itself is retained as written.
    let definition = stored.variables().values().next().expect("one definition");
    assert_eq!(definition.expression, "1.989e30 kg");
    assert_eq!(definition.qualified_name(), "mass_of_sun");
}

#[test]
fn a_definition_and_its_use_may_arrive_in_one_batch() {
    // The batch above did exactly this. Stated as its own case because the
    // alternative — pricing each command as it is applied — would make it
    // impossible, and nothing else would notice.
    let (_, _, object) = solar_system();
    assert!(object.get() < u64::MAX);
}

#[test]
fn redefining_a_variable_reprices_every_dependant_in_one_revision() {
    let (experiment, variable, object) = solar_system();
    let before = experiment.revision();

    let experiment = commit(
        &experiment,
        &[ExperimentCommand::SetVariableExpression {
            variable,
            expression: "2e30 kg".to_owned(),
        }],
    );

    assert_eq!(mass_of(&experiment, object), 1e30);
    assert_eq!(
        experiment.revision(),
        before.next(),
        "one edit is one revision however much it repriced"
    );
    // The authored source is untouched: only its projection moved.
    assert_eq!(
        experiment
            .snapshot()
            .object(object)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .properties[&property("mass")]
            .source(),
        Some("mass_of_sun / 2")
    );
}

#[test]
fn a_definition_may_read_another_definition() {
    let (experiment, report) = update(
        &Experiment::new(),
        &[
            define("mass_of_sun", "1.989e30 kg"),
            define("mass_of_half_sun", "mass_of_sun / 2"),
            massive("Quarter", "mass_of_half_sun / 2"),
        ],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();
    let object = report.first_created().expect("one object");
    assert_eq!(mass_of(&experiment, object), 1.989e30 / 4.0);

    // Changing the root reprices through the chain.
    let sun = report.created_variables[0];
    let experiment = commit(
        &experiment,
        &[ExperimentCommand::SetVariableExpression {
            variable: sun,
            expression: "4e30 kg".to_owned(),
        }],
    );
    assert_eq!(mass_of(&experiment, object), 1e30);
}

#[test]
fn a_rejected_variable_edit_changes_nothing_at_all() {
    let (experiment, variable, object) = solar_system();
    let before = experiment.clone();

    // A cycle: the definition would read itself.
    let rejection = update(
        &experiment,
        &[ExperimentCommand::SetVariableExpression {
            variable,
            expression: "mass_of_sun * 2".to_owned(),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("a definition cannot read itself");
    assert_eq!(rejection.code(), "variable_unresolved");

    assert_eq!(experiment, before);
    assert_eq!(mass_of(&experiment, object), 1.989e30 / 2.0);
}

#[test]
fn an_unknown_name_is_reported_rather_than_defaulted() {
    let rejection = update(
        &Experiment::new(),
        &[massive("Earth", "mass_of_sun / 2")],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("nothing defines that name");
    assert_eq!(rejection.code(), "expression_unresolved");
}

#[test]
fn a_dimension_a_definition_carries_reaches_the_property_that_reads_it() {
    // The definition is a *length*, so a mass property may not read it. This
    // is the check the declared-unit form could never make.
    let rejection = update(
        &Experiment::new(),
        &[
            define("orbit_radius", "1 AU"),
            massive("Earth", "orbit_radius"),
        ],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("a length is not a mass");
    assert_eq!(rejection.code(), "expression_dimension_mismatch");
}

#[test]
fn renaming_a_definition_rewrites_every_expression_that_used_it() {
    let (experiment, variable, object) = solar_system();

    let experiment = commit(
        &experiment,
        &[ExperimentCommand::RenameVariable {
            variable,
            name: variable_name("solar_mass"),
        }],
    );

    // The identity survives, so nothing keyed by it rebinds.
    let snapshot = experiment.snapshot();
    let definition = snapshot.variable(variable).expect("still there");
    assert_eq!(definition.qualified_name(), "solar_mass");

    // And the dependant now refers to the new name, with its meaning intact.
    assert_eq!(
        snapshot
            .object(object)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .properties[&property("mass")]
            .source(),
        Some("solar_mass / 2")
    );
    assert_eq!(mass_of(&experiment, object), 1.989e30 / 2.0);
}

#[test]
fn a_rename_onto_a_taken_name_is_refused_whole() {
    let (experiment, report) = update(
        &Experiment::new(),
        &[define("a", "1"), define("b", "2")],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();
    let before = experiment.clone();

    let rejection = update(
        &experiment,
        &[ExperimentCommand::RenameVariable {
            variable: report.created_variables[0],
            name: variable_name("b"),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("`b` is taken");
    assert_eq!(rejection.code(), "variable_name_taken");
    assert_eq!(experiment, before);
}

#[test]
fn removing_a_referenced_definition_is_refused_and_names_the_dependant() {
    let (experiment, variable, _) = solar_system();
    let before = experiment.clone();

    let rejection = update(
        &experiment,
        &[ExperimentCommand::RemoveVariable(variable)],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("a property still reads it");
    assert_eq!(rejection.code(), "expression_unresolved");
    assert!(
        rejection.to_string().contains("inertial_mass.mass"),
        "the refusal must name the dependant: {rejection}"
    );
    assert_eq!(experiment, before);
}

#[test]
fn removing_a_definition_and_its_references_together_is_accepted() {
    let (experiment, variable, object) = solar_system();

    // The same batch clears the reference, so nothing is left dangling.
    let experiment = commit(
        &experiment,
        &[
            ExperimentCommand::SetComponentProperty {
                object,
                component: mass_component(),
                property: property("mass"),
                value: AuthoredValue::si("1e30 kg"),
            },
            ExperimentCommand::RemoveVariable(variable),
        ],
    );

    assert_eq!(experiment.snapshot().variable_count(), 0);
    assert_eq!(mass_of(&experiment, object), 1e30);
}

#[test]
fn an_edit_costs_the_affected_closure_rather_than_the_document() {
    // One definition and one object that reads it, plus a growing crowd of
    // definitions and objects that do not.
    let mut commands = vec![define("used", "2 kg"), massive("Dependant", "used")];
    for index in 0..40 {
        commands.push(define(&format!("spare{index}"), "1"));
        commands.push(massive(&format!("Other {index}"), "1e30 kg"));
    }
    let (experiment, report) = update(&Experiment::new(), &commands, &schemas(), &Limits::DEFAULT)
        .expect("accepted")
        .adopt();
    let used = report.created_variables[0];

    let (_, report) = update(
        &experiment,
        &[ExperimentCommand::SetVariableExpression {
            variable: used,
            expression: "3 kg".to_owned(),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();

    // 41 definitions and 41 objects exist; the edit disturbed exactly one of
    // each, and that is what it paid for.
    assert_eq!(experiment.snapshot().variable_count(), 41);
    assert_eq!(experiment.snapshot().object_count(), 41);
    assert_eq!(report.work.variables, 1);
    assert_eq!(report.work.properties, 1);
}

#[test]
fn an_edit_that_touches_no_definition_evaluates_none_of_them() {
    let (experiment, _, object) = solar_system();
    let (_, report) = update(
        &experiment,
        &[ExperimentCommand::RenameObject {
            object,
            name: label("Renamed"),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();

    assert_eq!(report.work, Default::default());
}

#[test]
fn a_root_definition_may_not_shadow_a_unit() {
    let rejection = update(
        &Experiment::new(),
        &[define("kg", "1")],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("`kg` already means kilograms");
    assert_eq!(rejection.code(), "variable_shadows_unit");

    // In a namespace it is unambiguous, because reaching it means writing
    // `physics.kg`.
    let experiment = commit(
        &Experiment::new(),
        &[ExperimentCommand::DefineVariable(Box::new(
            VariableSpec::new(variable_name("K"), "8.99e9")
                .in_namespace(Namespace::new("electricity")),
        ))],
    );
    assert_eq!(experiment.snapshot().variable_count(), 1);
}

#[test]
fn a_duplicate_name_is_refused() {
    let rejection = update(
        &Experiment::new(),
        &[define("a", "1"), define("a", "2")],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect_err("`a` is defined twice");
    assert_eq!(rejection.code(), "variable_name_taken");
}

#[test]
fn a_description_is_authored_intent_and_never_affects_resolution() {
    let (experiment, report) = update(
        &Experiment::new(),
        &[ExperimentCommand::DefineVariable(Box::new(
            VariableSpec::new(variable_name("mass_of_sun"), "1.989e30 kg")
                .described("IAU nominal solar mass"),
        ))],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt();
    let variable = report.first_variable().expect("one definition");
    assert_eq!(
        experiment
            .snapshot()
            .variable(variable)
            .expect("exists")
            .description
            .as_deref(),
        Some("IAU nominal solar mass")
    );

    let experiment = commit(
        &experiment,
        &[ExperimentCommand::SetVariableDescription {
            variable,
            description: None,
        }],
    );
    assert_eq!(
        experiment
            .snapshot()
            .variable(variable)
            .expect("exists")
            .description,
        None
    );
}

#[test]
fn identities_survive_a_rename_and_are_never_reused() {
    let (experiment, variable, _) = solar_system();
    let experiment = commit(
        &experiment,
        &[ExperimentCommand::RenameVariable {
            variable,
            name: variable_name("solar_mass"),
        }],
    );
    assert_eq!(experiment.counters().variables_minted(), 1);

    // Removing and defining again does not recycle the handle.
    let experiment = commit(
        &experiment,
        &[
            ExperimentCommand::SetComponentProperty {
                object: experiment
                    .snapshot()
                    .objects()
                    .keys()
                    .next()
                    .copied()
                    .expect("one object"),
                component: mass_component(),
                property: property("mass"),
                value: AuthoredValue::si("1 kg"),
            },
            ExperimentCommand::RemoveVariable(variable),
            define("solar_mass", "1.989e30 kg"),
        ],
    );
    let reborn = experiment
        .snapshot()
        .variables()
        .keys()
        .next()
        .copied()
        .expect("one definition");
    assert_ne!(reborn, variable);
    assert_eq!(experiment.counters().variables_minted(), 2);
}

#[test]
fn a_bounded_document_refuses_an_over_full_variable_graph() {
    let limits = Limits {
        max_variables: 2,
        ..Limits::DEFAULT
    };
    let rejection = update(
        &Experiment::new(),
        &[define("a", "1"), define("b", "2"), define("c", "3")],
        &schemas(),
        &limits,
    )
    .expect_err("over the variable limit");
    assert_eq!(rejection.code(), "too_many_variables");
}

#[test]
fn a_raw_identity_resolves_only_while_its_definition_exists() {
    let (experiment, variable, object) = solar_system();
    let raw = variable.get();
    assert_eq!(experiment.snapshot().resolve_variable(raw), Some(variable));
    assert_eq!(experiment.snapshot().resolve_variable(raw + 1), None);

    let experiment = commit(
        &experiment,
        &[
            ExperimentCommand::SetComponentProperty {
                object,
                component: mass_component(),
                property: property("mass"),
                value: AuthoredValue::si("1 kg"),
            },
            ExperimentCommand::RemoveVariable(variable),
        ],
    );
    assert_eq!(experiment.snapshot().resolve_variable(raw), None);
}
