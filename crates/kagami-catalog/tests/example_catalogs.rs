//! End-to-end behaviour over the catalogs this repository actually ships.
//!
//! These are deliberately not synthetic fixtures. `etc/catalogs` is what a
//! user copies into a Kagami installation, so testing against it means the
//! shipped data cannot drift away from the format, and the acceptance
//! properties — deterministic loading, isolated failure, availability driven
//! by installed schemas, and catalog-independent instantiation — are asserted
//! on the same bytes a scientist would edit.

mod support;

use std::fs;
use std::path::Path;

use kagami_catalog::{
    CatalogSet, Dimension, InstantiationRequest, Limits, LoadResult, ObjectPropertyValue,
    PropertyName, SchemaRegistry, UnavailableReason, load_directory,
};
use orishu_variables::Namespace;
use support::{component, example_catalog_root, full_registry, identity, mass_only_registry};

const EARTH_MASS_KG: f64 = 5.97e24;
const EARTH_RADIUS_M: f64 = 6.378e6;
const ELECTRON_CHARGE_C: f64 = -1.602_176_634e-19;

fn shipped(registry: &SchemaRegistry) -> CatalogSet {
    load_directory(&example_catalog_root(), registry, &Limits::DEFAULT)
}

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("a valid property name")
}

fn si_value(set: &CatalogSet, catalog: &str, template: &str, reference: &str) -> f64 {
    let scope = identity(catalog, template);
    let kagami_catalog::Resolution::Visible(binding) = set.projection().resolve(reference, &scope)
    else {
        panic!("`{reference}` should resolve from {scope}");
    };
    set.projection()
        .value(binding)
        .expect("a published binding evaluates")
}

#[test]
fn every_shipped_entry_loads_against_the_schemas_it_was_written_for() {
    let set = shipped(&full_registry());
    let summary = set.summary();
    assert_eq!(
        (summary.invalid, summary.unavailable),
        (0, 0),
        "shipped catalogs must be valid and available: {:#?}",
        set.entries()
            .iter()
            .filter(|entry| !entry.result.is_available())
            .collect::<Vec<_>>()
    );
    assert!(summary.available >= 45, "{summary:?}");
    assert!(set.file_errors().is_empty(), "{:?}", set.file_errors());
}

#[test]
fn loading_the_same_directory_twice_produces_the_same_report() {
    let registry = full_registry();
    assert_eq!(shipped(&registry).entries(), shipped(&registry).entries());
}

#[test]
fn files_and_documents_are_reported_in_a_stable_order() {
    let set = shipped(&full_registry());
    let sources: Vec<String> = set
        .entries()
        .iter()
        .map(|entry| entry.source.to_string())
        .collect();
    let mut sorted = sources.clone();
    sorted.sort();
    assert_eq!(
        sources.first().map(String::as_str),
        Some("particles.yaml#1"),
        "the first file in path order is read first"
    );
    assert_eq!(sources.len(), sorted.len());
}

#[test]
fn authored_units_are_published_as_canonical_si() {
    let set = shipped(&full_registry());
    assert_eq!(
        si_value(&set, "planets", "earth", "planets.earth.mass"),
        EARTH_MASS_KG
    );
    assert_eq!(
        si_value(&set, "planets", "earth", "planets.earth.radius"),
        EARTH_RADIUS_M
    );
    assert_eq!(
        si_value(&set, "particles", "electron", "particles.electron.charge"),
        ELECTRON_CHARGE_C
    );
}

#[test]
fn a_property_resolves_by_both_its_canonical_and_concise_names() {
    let set = shipped(&full_registry());
    assert_eq!(
        si_value(&set, "planets", "earth", "planets.earth.inertial_mass.mass"),
        si_value(&set, "planets", "earth", "planets.earth.mass")
    );
}

#[test]
fn a_missing_plugin_leaves_only_the_entries_that_need_it_unavailable() {
    let set = shipped(&mass_only_registry());
    let electron = set
        .get(&identity("particles", "electron"))
        .expect("the entry is preserved, not dropped");
    let LoadResult::Unavailable { reasons, template } = &electron.result else {
        panic!("expected unavailable, got {:?}", electron.result);
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            UnavailableReason::UnknownComponentType { component: found }
                if *found == component("kagami.electromagnetic_sources", "charge_source")
        )),
        "{reasons:?}"
    );
    // Preserved in full, so installing the plugin later needs no edit.
    assert_eq!(template.spec.components.len(), 4);

    assert!(
        set.get(&identity("planets", "earth"))
            .expect("planets is unaffected")
            .result
            .is_available()
    );
}

#[test]
fn a_template_local_helper_is_private_and_a_declared_public_one_is_not() {
    let set = shipped(&full_registry());
    let outsider = identity("planets", "earth");
    assert!(matches!(
        set.projection()
            .resolve("scenarios.scaled_earth.earth_mass", &outsider),
        kagami_catalog::Resolution::Private { .. }
    ));
    assert!(matches!(
        set.projection()
            .resolve("scenarios.constants.gravitational_constant", &outsider),
        kagami_catalog::Resolution::Visible(_)
    ));
}

#[test]
fn a_cross_catalog_reference_resolves_through_the_shared_evaluator() {
    let set = shipped(&full_registry());
    assert_eq!(
        si_value(
            &set,
            "scenarios",
            "scaled_earth",
            "scenarios.scaled_earth.mass"
        ),
        EARTH_MASS_KG,
        "the default mass_fraction of 1 reproduces Earth's mass"
    );
}

#[test]
fn dimensioned_parameters_authored_in_different_units_compose_in_canonical_si() {
    let set = shipped(&full_registry());
    // 500 kg of structure plus 1.5 t of propellant is 2000 kg, without the
    // authored expression converting anything.
    assert_eq!(
        si_value(
            &set,
            "scenarios",
            "orbital_probe",
            "scenarios.orbital_probe.mass"
        ),
        2_000.0
    );
    assert_eq!(
        si_value(
            &set,
            "scenarios",
            "orbital_probe",
            "scenarios.orbital_probe.propellant_mass"
        ),
        1_500.0
    );
}

/// A parameter authored in a scaling unit is materialised in SI: 500 kg plus
/// 1.5 t is a two-tonne probe, not a 501.5 kg one.
#[test]
fn a_parameter_authored_in_a_scaling_unit_materializes_in_si() {
    let set = shipped(&full_registry());
    let candidate = kagami_catalog::materialize(
        &set,
        &full_registry(),
        &InstantiationRequest::new(
            identity("scenarios", "orbital_probe"),
            Namespace::new("objects.probe_1"),
        ),
    )
    .expect("orbital_probe is available");

    let propellant = candidate
        .definitions
        .iter()
        .find(|(name, _)| name.as_str().ends_with("propellant_mass"))
        .expect("the parameter is captured")
        .1;
    assert_eq!(propellant.si_value, 1_500.0);
    assert_eq!(
        candidate.resolve_standalone().unwrap()["inertial_mass.mass"],
        2_000.0
    );
}

/// The same rule for a definition reached *transitively*: capturing another
/// template's binding must copy the published SI magnitude, not the authored
/// one. Copying `1.5` from a value declared in tonnes would make the new
/// object 1.5 kg without any diagnostic at all.
#[test]
fn a_transitively_captured_definition_carries_its_si_magnitude() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("units.yaml"),
        r#"---
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: units, name: ballast}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: {expression: "1.5", unit: t}}
---
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: units, name: pair}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "units.ballast.mass * 2"}
"#,
    )
    .unwrap();

    let registry = mass_only_registry();
    let set = load_directory(directory.path(), &registry, &Limits::DEFAULT);
    assert_eq!(
        si_value(&set, "units", "ballast", "units.ballast.mass"),
        1_500.0
    );

    let candidate = kagami_catalog::materialize(
        &set,
        &registry,
        &InstantiationRequest::new(identity("units", "pair"), Namespace::new("objects.o")),
    )
    .expect("pair is available");
    assert_eq!(
        candidate.resolve_standalone().unwrap()["inertial_mass.mass"],
        3_000.0
    );
}

/// A template is data. Two templates with different names but the same content
/// must produce byte-identical component data, because nothing anywhere may
/// branch on a template, catalog, or file name — ADR 0008's central rule and
/// the reason a "particle species" enum is a non-goal.
#[test]
fn a_template_name_carries_no_behaviour() {
    let directory = tempfile::tempdir().unwrap();
    let body = "spec:\n  components:\n  - type: {plugin: kagami.mass_sources, name: \
                inertial_mass}\n    properties:\n      mass: {quantity: \"1.0e24\"}\n";
    fs::write(
        directory.path().join("twins.yaml"),
        format!(
            "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata: {{catalog: twins, \
             name: electron}}\n{body}---\napiVersion: kagami.catalog/v1\nkind: \
             ObjectTemplate\nmetadata: {{catalog: twins, name: jupiter}}\n{body}"
        ),
    )
    .unwrap();

    let registry = mass_only_registry();
    let set = load_directory(directory.path(), &registry, &Limits::DEFAULT);
    let materialize_named = |name: &str| {
        kagami_catalog::materialize(
            &set,
            &registry,
            &InstantiationRequest::new(identity("twins", name), Namespace::new("objects.o")),
        )
        .expect("both templates are available")
        .components
    };

    assert_eq!(materialize_named("electron"), materialize_named("jupiter"));
    // Their content fingerprints differ only because their identities do.
    assert_ne!(
        set.get(&identity("twins", "electron"))
            .and_then(|entry| entry.fingerprint),
        set.get(&identity("twins", "jupiter"))
            .and_then(|entry| entry.fingerprint)
    );
}

#[test]
fn instantiating_produces_an_object_that_resolves_without_the_catalog() {
    let set = shipped(&full_registry());
    let request = InstantiationRequest::new(
        identity("planets", "earth"),
        Namespace::new("objects.earth_1"),
    );
    let candidate =
        kagami_catalog::materialize(&set, &full_registry(), &request).expect("earth is available");

    let ObjectPropertyValue::Quantity {
        si_value,
        dimension,
        ..
    } = &candidate.components[0].properties[&property("mass")]
    else {
        panic!("mass should be a quantity");
    };
    assert_eq!(*si_value, EARTH_MASS_KG);
    assert_eq!(*dimension, Dimension::MASS);

    let standalone = candidate
        .resolve_standalone()
        .expect("the candidate is self-contained");
    assert_eq!(standalone["inertial_mass.mass"], EARTH_MASS_KG);
    assert_eq!(standalone["sphere.radius"], EARTH_RADIUS_M);
}

#[test]
fn instantiation_records_provenance_and_the_schema_versions_it_checked() {
    let set = shipped(&full_registry());
    let request = InstantiationRequest::new(
        identity("planets", "earth"),
        Namespace::new("objects.earth_1"),
    );
    let candidate = kagami_catalog::materialize(&set, &full_registry(), &request).unwrap();

    assert_eq!(candidate.provenance.identity, identity("planets", "earth"));
    assert_eq!(
        candidate.provenance.api_version,
        kagami_catalog::API_VERSION
    );
    assert_eq!(
        candidate.provenance.source.file,
        Path::new("planets.yaml"),
        "provenance is catalog-relative, never a machine-local absolute path"
    );
    let versions: Vec<_> = candidate
        .components
        .iter()
        .map(|component| component.schema_version.0)
        .collect();
    assert_eq!(versions, vec![1, 1, 2]);
}

#[test]
fn a_parameter_binding_changes_only_that_instantiation() {
    let set = shipped(&full_registry());
    let mut request = InstantiationRequest::new(
        identity("scenarios", "scaled_earth"),
        Namespace::new("objects.half_earth"),
    );
    request.bindings.insert(
        kagami_catalog::ParameterName::new("mass_fraction").unwrap(),
        "0.5".to_owned(),
    );
    let candidate = kagami_catalog::materialize(&set, &full_registry(), &request).unwrap();

    let standalone = candidate.resolve_standalone().unwrap();
    assert!((standalone["inertial_mass.mass"] - EARTH_MASS_KG * 0.5).abs() < 1.0e18);
    // The catalog's own projection is untouched by the binding.
    assert_eq!(
        si_value(
            &set,
            "scenarios",
            "scaled_earth",
            "scenarios.scaled_earth.mass"
        ),
        EARTH_MASS_KG
    );
}

#[test]
fn losing_the_catalog_a_template_depends_on_makes_it_unavailable_but_changes_no_object() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["planets.yaml", "particles.yaml", "scenarios.yaml"] {
        fs::copy(
            example_catalog_root().join(name),
            directory.path().join(name),
        )
        .unwrap();
    }

    let registry = full_registry();
    let before = load_directory(directory.path(), &registry, &Limits::DEFAULT);
    let request = InstantiationRequest::new(
        identity("scenarios", "scaled_earth"),
        Namespace::new("objects.o1"),
    );
    let object = kagami_catalog::materialize(&before, &registry, &request).unwrap();
    let materialized = object.resolve_standalone().unwrap();

    fs::remove_file(directory.path().join("planets.yaml")).unwrap();
    let after = load_directory(directory.path(), &registry, &Limits::DEFAULT);

    let entry = after.get(&identity("scenarios", "scaled_earth")).unwrap();
    let LoadResult::Unavailable { reasons, .. } = &entry.result else {
        panic!("expected unavailable, got {:?}", entry.result);
    };
    assert!(
        reasons
            .iter()
            .any(|reason| matches!(reason, UnavailableReason::MissingDependency { .. })),
        "{reasons:?}"
    );

    // The object materialised before the loss is untouched and still resolves.
    assert_eq!(object.resolve_standalone().unwrap(), materialized);
    assert_eq!(materialized["inertial_mass.mass"], EARTH_MASS_KG);
}

#[test]
fn editing_a_template_does_not_change_an_object_already_materialized_from_it() {
    let directory = tempfile::tempdir().unwrap();
    fs::copy(
        example_catalog_root().join("planets.yaml"),
        directory.path().join("planets.yaml"),
    )
    .unwrap();

    let registry = mass_only_registry();
    let before = load_directory(directory.path(), &registry, &Limits::DEFAULT);
    let request =
        InstantiationRequest::new(identity("planets", "earth"), Namespace::new("objects.o1"));
    let object = kagami_catalog::materialize(&before, &registry, &request).unwrap();

    let text = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();
    fs::write(
        directory.path().join("planets.yaml"),
        text.replace("5.97e+24", "1.0"),
    )
    .unwrap();
    let after = load_directory(directory.path(), &registry, &Limits::DEFAULT);

    assert_eq!(
        si_value(&after, "planets", "earth", "planets.earth.mass"),
        1.0
    );
    assert_eq!(
        object.resolve_standalone().unwrap()["inertial_mass.mass"],
        EARTH_MASS_KG,
        "a template edit never reaches an object that was already materialised"
    );
}

#[test]
fn a_fingerprint_survives_reformatting_and_moving_the_file() {
    let directory = tempfile::tempdir().unwrap();
    fs::copy(
        example_catalog_root().join("planets.yaml"),
        directory.path().join("planets.yaml"),
    )
    .unwrap();
    let registry = mass_only_registry();
    let original = load_directory(directory.path(), &registry, &Limits::DEFAULT)
        .get(&identity("planets", "earth"))
        .and_then(|entry| entry.fingerprint)
        .expect("earth has a fingerprint");

    // Reformat: rename the file, and re-indent one entry's flow mappings into
    // block mappings. Neither is a change of meaning.
    let text = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();
    fs::remove_file(directory.path().join("planets.yaml")).unwrap();
    fs::write(
        directory.path().join("solar-system.yaml"),
        text.replace(
            "      mass: {quantity: {expression: \"5.97e+24\", unit: kg}}",
            "      mass:\n        quantity:\n          unit: kg\n          expression: \"5.97e+24\"",
        ),
    )
    .unwrap();

    let moved = load_directory(directory.path(), &registry, &Limits::DEFAULT)
        .get(&identity("planets", "earth"))
        .and_then(|entry| entry.fingerprint)
        .expect("earth still has a fingerprint");
    assert_eq!(original, moved);
}

#[test]
fn an_adjacent_malformed_file_never_hides_a_valid_one() {
    let directory = tempfile::tempdir().unwrap();
    fs::copy(
        example_catalog_root().join("planets.yaml"),
        directory.path().join("planets.yaml"),
    )
    .unwrap();
    fs::write(
        directory.path().join("broken.yaml"),
        "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata: {catalog: bad, name: \
         entry}\nspec:\n  components:\n  - type: {plugin: kagami.mass_sources, name: \
         inertial_mass}\n    properties:\n      mass: {quantity: {expression: \"1\", unit: \
         furlong}}\n",
    )
    .unwrap();

    let set = load_directory(directory.path(), &mass_only_registry(), &Limits::DEFAULT);
    assert_eq!(set.summary().invalid, 1);
    assert!(set.summary().available >= 40);
    let broken = set.get(&identity("bad", "entry")).unwrap();
    let LoadResult::Invalid { diagnostics } = &broken.result else {
        panic!("expected invalid");
    };
    assert!(
        diagnostics[0].to_string().contains("furlong"),
        "{diagnostics:?}"
    );
}
