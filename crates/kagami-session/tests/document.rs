//! The experiment file, exercised as the sharing protocol it is.
//!
//! ADR 0012 makes sending someone a file the first collaboration model, so the
//! properties worth pinning down are a colleague's: they open it and see the
//! same experiment, *including the expressions* rather than the numbers those
//! happened to resolve to; a document from a newer build is declined instead of
//! half-read; and a document naming a plugin they have not installed still
//! opens.
//!
//! Set `KAGAMI_WIRE_BLESS=1` to rewrite the fixtures after an intentional
//! change to the format — which is also a `format_version` change.

mod support;

use std::path::PathBuf;

use kagami_catalog::{PropertyName, SchemaRegistry};
use kagami_document::{
    AuthoredValue, DisplayName, Experiment, ExperimentCommand, Limits, ObjectShape, ObjectSpec,
    Transform, VariableSpec, Vector3, update,
};
use kagami_session::{
    AuthoringView, CameraPose, DocumentMetadata, ExperimentDocument, FORMAT_VERSION,
    MAX_CAMERA_DISTANCE_UNITS, MIN_FORMAT_VERSION, Projection, SceneScale, StoredDefaultView,
    decode_document,
};
use orishu_variables::Name;
use serde::{Serialize, de::DeserializeOwned};
use support::{mass_component, schemas};

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("valid identifier")
}

fn metadata_at(revision: u64) -> DocumentMetadata {
    DocumentMetadata {
        generator: "kagami 0.1.0".to_owned(),
        created: "2026-09-07T10:00:00Z".to_owned(),
        saved: "2026-09-07T11:30:00Z".to_owned(),
        saved_revision: revision,
    }
}

/// Metadata naming the revision `experiment` is at.
fn metadata_for(experiment: &Experiment) -> DocumentMetadata {
    metadata_at(experiment.revision().get())
}

/// A document of `experiment` at the default authoring view.
///
/// Most tests here are about the experiment section, and the view they save is
/// beside the point — but it is never *absent*, because a save always records
/// the view in force (ADR 0022).
fn document_of(experiment: &Experiment) -> ExperimentDocument {
    document_with(
        experiment,
        &AuthoringView::default(),
        metadata_for(experiment),
    )
}

/// A document of `experiment` seen through `view`, stamped with `metadata`.
fn document_with(
    experiment: &Experiment,
    view: &AuthoringView,
    metadata: DocumentMetadata,
) -> ExperimentDocument {
    ExperimentDocument::of(experiment, &experiment.snapshot(), view, metadata)
}

/// An experiment with a variable, an expression that reads it, and a shape.
fn authored() -> Experiment {
    let mut properties = std::collections::BTreeMap::new();
    properties.insert(property("mass"), AuthoredValue::si("mass_of_sun / 2"));
    update(
        &Experiment::new(),
        &[
            ExperimentCommand::DefineVariable(Box::new(
                VariableSpec::new(
                    Name::new("mass_of_sun").expect("valid identifier"),
                    "1.989e30 kg",
                )
                .described("IAU nominal solar mass"),
            )),
            ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("Half a sun").expect("valid label"))
                    .with_component(mass_component(), properties)
                    .with_shape(ObjectShape::sphere(6.9634e8).expect("valid radius"))
                    .with_transform(Transform::at(Vector3::new(1.0, 2.0, 3.0).expect("finite"))),
            )),
            ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
                DisplayName::new("Marker").expect("valid label"),
            ))),
        ],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("accepted")
    .adopt()
    .0
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"))
}

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
        "the `{name}` document shape changed; that is a format change, so advance \
         `FORMAT_VERSION` and re-bless the fixture"
    );

    let parsed: T = serde_json::from_str(&expected).expect("fixture must deserialize");
    assert_eq!(&parsed, value, "`{name}` must round-trip");
}

#[test]
fn a_document_pins_its_shape() {
    let experiment = authored();
    assert_fixture("experiment", &document_of(&experiment));
}

#[test]
fn an_empty_document_pins_its_shape() {
    let experiment = Experiment::new();
    assert_fixture("experiment_empty", &document_of(&experiment));
}

#[test]
fn saving_and_opening_reproduces_the_same_experiment() {
    let experiment = authored();
    let document = document_of(&experiment);
    let json = serde_json::to_string(&document).expect("encodes");

    let decoded: ExperimentDocument = serde_json::from_str(&json).expect("decodes");
    let reopened = decoded
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect("a document this build wrote");

    // Identities, counters, definitions, sources and setup all survive.
    let before = experiment.snapshot();
    let after = reopened.snapshot();
    assert_eq!(after.objects(), before.objects());
    assert_eq!(after.variables(), before.variables());
    assert_eq!(after.setup(), before.setup());
    assert_eq!(reopened.counters(), experiment.counters());
}

#[test]
fn the_expression_survives_rather_than_the_number_it_resolved_to() {
    let experiment = authored();
    let document = document_of(&experiment);
    let json = serde_json::to_value(&document).expect("encodes");

    // The file carries the author's intent and nothing derived from it. A
    // resolved magnitude in the file would be a number a later reader could
    // not check against its own source.
    let text = json.to_string();
    assert!(text.contains("mass_of_sun / 2"), "{text}");
    assert!(!text.contains("siValue"), "{text}");
    assert!(!text.contains("9.945e29"), "{text}");

    // And reopening re-derives it.
    let reopened = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect("valid");
    let id = reopened
        .snapshot()
        .resolve_object(0)
        .expect("the first object");
    assert_eq!(
        reopened
            .snapshot()
            .object(id)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .properties[&property("mass")]
            .si_value(),
        Some(1.989e30 / 2.0)
    );
}

#[test]
fn re_encoding_the_same_state_and_metadata_is_byte_identical() {
    let experiment = authored();
    let once = serde_json::to_string_pretty(&document_of(&experiment)).expect("encodes");

    // Round-tripping through the file and re-encoding must produce the same
    // bytes: a re-save that silently reordered or dropped anything would make
    // a shared file diff-noisy or lossy.
    let reopened = serde_json::from_str::<ExperimentDocument>(&once)
        .expect("decodes")
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect("valid");
    // The same authored content and the same supplied metadata: the same
    // bytes. A shared file that churned on every re-save would make the diff
    // useless, which is the whole collaboration model under ADR 0012.
    let twice = serde_json::to_string_pretty(&document_with(
        &reopened,
        &AuthoringView::default(),
        metadata_at(experiment.revision().get()),
    ))
    .expect("encodes");

    assert_eq!(once, twice);
}

#[test]
fn a_document_from_a_newer_build_is_declined_rather_than_half_read() {
    let experiment = authored();
    let mut document = document_of(&experiment);
    document.format_version = FORMAT_VERSION + 1;

    let error = document
        .clone()
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("this build cannot know what a later version means");
    assert_eq!(error.code(), "unsupported_format_version");

    // There is no version zero, and no "lower is probably safe" rule.
    document.format_version = 0;
    assert_eq!(
        document
            .into_experiment(&schemas(), &Limits::DEFAULT)
            .expect_err("no such version")
            .code(),
        "unsupported_format_version"
    );
}

#[test]
fn a_document_of_another_format_is_refused_before_anything_is_interpreted() {
    let experiment = authored();
    let mut document = document_of(&experiment);
    document.format = "fieldcad.scene".to_owned();
    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("not a Kagami experiment");
    assert_eq!(error.code(), "wrong_format");
}

#[test]
fn a_document_naming_an_uninstalled_plugin_still_opens() {
    let experiment = authored();
    let document = document_of(&experiment);

    // The colleague has no mass plugin. The document is not broken; their
    // installation is incomplete.
    let reopened = document
        .into_experiment(&SchemaRegistry::new(), &Limits::DEFAULT)
        .expect("an absent plugin is not a load failure");

    let snapshot = reopened.snapshot();
    let id = snapshot.resolve_object(0).expect("the first object");
    let stored = snapshot
        .object(id)
        .expect("preserved")
        .component(&mass_component())
        .expect("still attached");

    // Every authored field survives, and no magnitude was invented for it.
    assert_eq!(
        stored.properties[&property("mass")].source(),
        Some("mass_of_sun / 2")
    );
    assert_eq!(stored.properties[&property("mass")].si_value(), None);
    assert!(!stored.properties[&property("mass")].is_priced());

    // And it is reported, so nothing tries to compile it.
    let report = kagami_document::CapabilityReport::of(&snapshot, &SchemaRegistry::new());
    assert!(!report.is_complete());
    assert_eq!(
        report.gap(id, &mass_component()).expect("a gap").code(),
        "schema_not_installed"
    );
}

#[test]
fn installing_the_plugin_later_prices_the_value_on_the_next_edit() {
    let experiment = authored();
    let document = document_of(&experiment);
    let reopened = document
        .into_experiment(&SchemaRegistry::new(), &Limits::DEFAULT)
        .expect("loads without the plugin");
    let id = reopened
        .snapshot()
        .resolve_object(0)
        .expect("the first object");

    // The plugin arrives. Adopting a schema does not reach into authored
    // state, so the value is still unpriced and says so.
    let report = kagami_document::CapabilityReport::of(&reopened.snapshot(), &schemas());
    assert_eq!(
        report.gap(id, &mass_component()).expect("a gap").code(),
        "value_not_priced"
    );

    // An ordinary edit touching the component is the repair.
    let (edited, _) = update(
        &reopened,
        &[ExperimentCommand::SetShape {
            object: id,
            shape: None,
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("an unrelated object edit does not touch the component")
    .adopt();
    // That edit did not touch the component, so it is still unpriced.
    assert!(!kagami_document::CapabilityReport::of(&edited.snapshot(), &schemas()).is_complete());

    let (repriced, _) = update(
        &edited,
        &[ExperimentCommand::SetComponentProperty {
            object: id,
            component: mass_component(),
            property: property("mass"),
            value: AuthoredValue::si("mass_of_sun / 2"),
        }],
        &schemas(),
        &Limits::DEFAULT,
    )
    .expect("the component is governed again")
    .adopt();

    assert!(kagami_document::CapabilityReport::of(&repriced.snapshot(), &schemas()).is_complete());
    assert_eq!(
        repriced
            .snapshot()
            .object(id)
            .expect("exists")
            .component(&mass_component())
            .expect("attached")
            .properties[&property("mass")]
            .si_value(),
        Some(1.989e30 / 2.0)
    );
}

#[test]
fn a_document_cannot_name_an_identity_its_own_counters_never_minted() {
    let experiment = authored();
    let mut document = document_of(&experiment);
    // Two objects were minted; claim a third.
    document.experiment.objects[0].id = 99;

    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("object 99 was never allocated");
    assert_eq!(error.code(), "unallocated_identity");
}

#[test]
fn a_document_cannot_repeat_an_identity() {
    let experiment = authored();
    let mut document = document_of(&experiment);
    document.experiment.objects[1].id = document.experiment.objects[0].id;

    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("two objects with one identity");
    assert_eq!(error.code(), "duplicate_identity");
}

#[test]
fn a_document_whose_expression_does_not_resolve_is_refused() {
    let experiment = authored();
    let mut document = document_of(&experiment);
    document.experiment.variables.clear();

    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("the definition its object reads is gone");
    assert_eq!(error.code(), "expression_unresolved");
}

#[test]
fn a_version_one_document_loads_through_its_explicit_conversion() {
    // The version-1 fixtures are the ones this build shipped before ADR
    // 0022's view section existed. They are kept as *load* fixtures precisely
    // so the conversion has something real to convert.
    for name in ["experiment_v1", "experiment_empty_v1"] {
        let bytes = std::fs::read(fixture_path(name)).expect("the version-1 fixture");
        let document = decode_document(&bytes).expect("version 1 is still readable");

        // Converted up: everything above the codec deals with one shape.
        assert_eq!(document.format_version, FORMAT_VERSION);
        // Version 1 remembered no view, and that absence is defined rather
        // than a missing value to invent.
        assert_eq!(document.default_view, None);

        document
            .into_experiment(&schemas(), &Limits::DEFAULT)
            .expect("a version-1 experiment is still a valid experiment");
    }
}

#[test]
fn a_version_one_document_may_not_carry_a_field_version_one_never_had() {
    // The reason each version gets its own DTO: a producer that added the view
    // section without advancing the version would otherwise have it silently
    // dropped on the next re-save.
    let bytes = std::fs::read(fixture_path("experiment_empty_v1")).expect("the fixture");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    value["defaultView"] = serde_json::json!({ "version": 1 });
    let smuggled = serde_json::to_vec(&value).expect("encodes");

    assert_eq!(
        decode_document(&smuggled)
            .expect_err("version 1 has no such field")
            .code(),
        "malformed_document"
    );
}

#[test]
fn the_saved_view_round_trips_independently_of_the_experiment() {
    let experiment = authored();
    let camera = CameraPose::new(
        Vector3::new(4.0, -5.0, 6.0).expect("finite"),
        120.0,
        0.75,
        -0.25,
    )
    .expect("finite");
    let view = AuthoringView::new(Projection::Orthographic, SceneScale::METRE, camera)
        .expect("representable");

    let document = document_with(&experiment, &view, metadata_for(&experiment));
    let bytes = serde_json::to_vec(&document).expect("encodes");
    let decoded = decode_document(&bytes).expect("this build wrote it");

    assert_eq!(
        decoded
            .default_view
            .as_ref()
            .expect("a view was saved")
            .decode()
            .expect("this build's view version"),
        view,
        "projection and pose come back exactly as they were saved"
    );

    // And it is genuinely independent: the experiment section is unchanged by
    // which camera happened to be pointing at it.
    let plain = document_of(&experiment);
    assert_eq!(decoded.experiment, plain.experiment);
}

#[test]
fn re_encoding_the_same_state_and_view_is_byte_identical() {
    let experiment = authored();
    let view = AuthoringView::new(
        Projection::Orthographic,
        SceneScale::METRE,
        CameraPose::new(Vector3::ZERO, 42.0, 0.5, 0.25).expect("finite"),
    )
    .expect("representable");
    let metadata = metadata_for(&experiment);

    let once = serde_json::to_string_pretty(&document_with(&experiment, &view, metadata.clone()))
        .expect("encodes");
    let twice = serde_json::to_string_pretty(&document_with(&experiment, &view, metadata))
        .expect("encodes");

    assert_eq!(
        once, twice,
        "a re-save that churned the view section would make a shared file's diff useless"
    );
}

#[test]
fn no_saved_view_whatsoever_can_stop_an_experiment_from_opening() {
    // ADR 0022's non-blocking policy, exercised against the whole section
    // rather than only its body. A missing `version` used to fail
    // *whole-document* deserialization, so a presentational field nobody reads
    // could make an experiment unopenable — the one thing this section is not
    // allowed to do.
    let experiment = authored();
    let bytes = serde_json::to_vec(&document_of(&experiment)).expect("encodes");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");

    let hostile = [
        // No version at all: there is no version whose rules a body could be
        // checked against.
        (
            "view_without_version",
            serde_json::json!({ "projection": "perspective" }),
        ),
        // A version that is not a number, or not one.
        (
            "view_without_version",
            serde_json::json!({ "version": "one" }),
        ),
        ("view_without_version", serde_json::json!({ "version": -1 })),
        // A version from a newer build.
        (
            "unsupported_view_version",
            serde_json::json!({ "version": 99 }),
        ),
        // This version, wrong body.
        (
            "malformed_view",
            serde_json::json!({ "version": 1, "camera": "somewhere" }),
        ),
        (
            "malformed_view",
            serde_json::json!({ "version": 1, "projection": "perspective" }),
        ),
        // A field version 1 does not define.
        (
            "malformed_view",
            serde_json::json!({
                "version": 1, "projection": "perspective",
                "camera": { "target": [0, 0, 0], "distance": 18, "yaw": 0, "pitch": 0 },
                "follow": 3
            }),
        ),
        // Not even an object.
        ("view_without_version", serde_json::json!(42)),
        ("view_without_version", serde_json::json!("perspective")),
        ("view_without_version", serde_json::json!([1, 2, 3])),
    ];

    for (code, section) in hostile {
        value["defaultView"] = section.clone();
        let bytes = serde_json::to_vec(&value).expect("encodes");

        let decoded = decode_document(&bytes)
            .unwrap_or_else(|error| panic!("the document must still decode: {section} — {error}"));
        assert_eq!(
            decoded
                .default_view
                .as_ref()
                .expect("the section is retained as it was found")
                .decode()
                .expect_err("this build cannot use it")
                .code(),
            code,
            "{section}"
        );

        // And the experiment opens regardless.
        decoded
            .into_experiment(&schemas(), &Limits::DEFAULT)
            .unwrap_or_else(|error| {
                panic!("presentation cannot break an experiment: {section} — {error}")
            });
    }
}

#[test]
fn an_explicitly_null_view_is_simply_no_saved_view() {
    let experiment = authored();
    let bytes = serde_json::to_vec(&document_of(&experiment)).expect("encodes");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    value["defaultView"] = serde_json::Value::Null;
    let bytes = serde_json::to_vec(&value).expect("encodes");

    let decoded = decode_document(&bytes).expect("null is an absence, not a failure");
    assert_eq!(decoded.default_view, None);
}

#[test]
fn a_version_is_checked_before_any_content_is_interpreted() {
    let experiment = authored();
    let bytes = serde_json::to_vec(&document_of(&experiment)).expect("encodes");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");

    // A newer version is named as such rather than reported as garbage. That
    // is the whole reason the version is read from a permissive header first:
    // this build cannot know what version 3 means by a field it recognises,
    // and "malformed" would send the holder of the file looking for corruption
    // that is not there.
    for version in [FORMAT_VERSION + 1, 7, 0] {
        value["formatVersion"] = serde_json::json!(version);
        let bytes = serde_json::to_vec(&value).expect("encodes");
        assert_eq!(
            decode_document(&bytes)
                .expect_err("outside the supported range")
                .code(),
            "unsupported_format_version",
            "version {version}"
        );
    }

    // Both ends of the supported range do load.
    for version in [MIN_FORMAT_VERSION, FORMAT_VERSION] {
        value["formatVersion"] = serde_json::json!(version);
        let bytes = serde_json::to_vec(&value).expect("encodes");
        let outcome = decode_document(&bytes);
        // Version 1 refuses this particular body because it carries the view
        // section version 1 never had — but it refuses it as *malformed*,
        // having accepted the version, which is the distinction being pinned.
        if version == MIN_FORMAT_VERSION {
            assert_eq!(
                outcome.expect_err("v1 has no view section").code(),
                "malformed_document"
            );
        } else {
            outcome.expect("the current version");
        }
    }

    // And the format identifier is checked before the version, so a file that
    // is not a Kagami experiment at all says so.
    value["formatVersion"] = serde_json::json!(FORMAT_VERSION + 1);
    value["format"] = serde_json::json!("fieldcad.scene");
    let bytes = serde_json::to_vec(&value).expect("encodes");
    assert_eq!(
        decode_document(&bytes).expect_err("not this format").code(),
        "wrong_format",
        "format is checked first, so a foreign file is not blamed on its version"
    );
}

#[test]
fn a_coordinate_survives_the_file_bit_for_bit() {
    // Regression: serde_json's default float parser is fast rather than
    // exact, so an ordinary authored coordinate came back one bit away from
    // what was written. Nothing caught it because every fixture used
    // exactly-representable values like 1.0 and 2.0. The workspace enables
    // serde_json's `float_roundtrip` feature for this, and this test is what
    // notices if that is ever dropped — it applies to positions, velocities
    // and radii, not just to the camera that exposed it.
    let awkward = [
        -0.41076253398077245_f64,
        0.1,
        1.0 / 3.0,
        6.371e6 * (1.0 / 7.0),
        f64::MIN_POSITIVE,
        std::f64::consts::PI,
    ];
    for value in awkward {
        let vector = Vector3::new(value, -value, value * 3.0).expect("finite");
        let experiment = update(
            &Experiment::new(),
            &[ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("Somewhere").expect("valid label"))
                    .with_transform(Transform::at(vector)),
            ))],
            &schemas(),
            &Limits::DEFAULT,
        )
        .expect("accepted")
        .adopt()
        .0;

        let camera = CameraPose::new(vector, 100.0 + value, value, value / 4.0).expect("finite");
        let view = AuthoringView::new(Projection::Perspective, SceneScale::METRE, camera)
            .expect("representable");
        let bytes = serde_json::to_vec_pretty(&document_with(
            &experiment,
            &view,
            metadata_for(&experiment),
        ))
        .expect("encodes");

        let decoded = decode_document(&bytes).expect("this build wrote it");
        assert_eq!(
            decoded
                .default_view
                .as_ref()
                .expect("a view")
                .decode()
                .expect("version 1"),
            view,
            "camera pose must survive exactly: {value:?}"
        );

        let reopened = decoded
            .into_experiment(&schemas(), &Limits::DEFAULT)
            .expect("valid");
        assert_eq!(
            reopened.snapshot().objects(),
            experiment.snapshot().objects(),
            "an authored coordinate must survive exactly: {value:?}"
        );
    }
}

/// A view at `scale`, `units` render units from the origin.
fn view_at(scale: SceneScale, units: f64) -> AuthoringView {
    AuthoringView::new(
        Projection::Orthographic,
        scale,
        CameraPose::new(
            Vector3::new(2.0 * scale.metres(), 0.0, 0.0).expect("finite"),
            units * scale.metres(),
            0.25,
            0.5,
        )
        .expect("names a camera"),
    )
    .expect("reachable at this scale")
}

#[test]
fn a_nanometre_view_pins_its_shape() {
    let experiment = Experiment::new();
    assert_fixture(
        "experiment_view_nanometre",
        &document_with(
            &experiment,
            &view_at(SceneScale::NANOMETRE, 20.0),
            metadata_for(&experiment),
        ),
    );
}

#[test]
fn an_astronomical_view_pins_its_shape() {
    let experiment = Experiment::new();
    assert_fixture(
        "experiment_view_astronomical",
        &document_with(
            &experiment,
            &view_at(SceneScale::ASTRONOMICAL_UNIT, 20.0),
            metadata_for(&experiment),
        ),
    );
}

#[test]
fn extreme_scale_views_round_trip_without_legacy_metre_clamping() {
    // The pinned fixtures above guard the shape; this guards the *values*. A
    // build that still bounded the camera in absolute metres would clamp the
    // nanometre camera up to a metre and the astronomical one down to two
    // kilometres, and both would survive a shape check unnoticed.
    for (name, scale) in [
        ("experiment_view_nanometre", SceneScale::NANOMETRE),
        (
            "experiment_view_astronomical",
            SceneScale::ASTRONOMICAL_UNIT,
        ),
    ] {
        let bytes = std::fs::read(fixture_path(name)).expect("the fixture");
        let document = decode_document(&bytes).expect("this build wrote it");
        let view = document
            .default_view
            .as_ref()
            .expect("a view was saved")
            .decode()
            .expect("this build's section version");

        assert_eq!(view.scale(), scale, "{name}");
        assert_eq!(view, view_at(scale, 20.0), "{name}");
        // Twenty render units at this scale, not clamped to anything in metres.
        let expected = 20.0 * scale.metres();
        assert!(
            (view.camera().distance() - expected).abs() < expected * 1e-12,
            "{name}: {} m, expected {expected} m",
            view.camera().distance()
        );
    }
}

#[test]
fn a_version_one_view_section_converts_without_moving_the_envelope_version() {
    // The whole point of versioning this section separately: adding a scene
    // scale to it is a *section* change. A file written before scales existed
    // still loads, and the envelope's own version is untouched by the fact.
    let bytes = std::fs::read(fixture_path("experiment_view_v1")).expect("the fixture");
    let document = decode_document(&bytes).expect("the envelope is unaffected");
    assert_eq!(document.format_version, FORMAT_VERSION);

    let section = document.default_view.as_ref().expect("a v1 section");
    assert_eq!(section.declared_version(), Some(1));

    let view = section.decode().expect("version 1 still converts");
    // Its own values came through, so this is a conversion and not a default
    // quietly substituted for a section this build gave up on.
    assert_eq!(view.projection(), Projection::Orthographic);
    assert_eq!(view.camera().distance(), 42.0);
    assert_eq!(view.camera().target().x(), 1.5);
    // And version 1 *meant* one metre per render unit, which is the default.
    assert_eq!(view.scale(), SceneScale::METRE);
}

#[test]
fn the_scale_round_trips_and_re_bounds_the_camera_it_was_saved_with() {
    let experiment = authored();
    // A camera two hundred astronomical units out — reachable at AU scale and
    // nowhere near reachable at metre scale.
    let camera = CameraPose::new(
        Vector3::new(3.0e11, 0.0, 0.0).expect("finite"),
        200.0 * SceneScale::ASTRONOMICAL_UNIT.metres(),
        0.3,
        0.2,
    )
    .expect("names a camera");
    let view = AuthoringView::new(
        Projection::Orthographic,
        SceneScale::ASTRONOMICAL_UNIT,
        camera,
    )
    .expect("reachable at this scale");

    let bytes = serde_json::to_vec(&document_with(
        &experiment,
        &view,
        metadata_for(&experiment),
    ))
    .expect("encodes");
    let decoded = decode_document(&bytes).expect("this build wrote it");
    let restored = decoded
        .default_view
        .as_ref()
        .expect("a view was saved")
        .decode()
        .expect("this build's version");

    assert_eq!(
        restored, view,
        "the scale and its camera come back together"
    );
    assert_eq!(restored.scale(), SceneScale::ASTRONOMICAL_UNIT);
    assert!(
        restored.camera().distance() > 1.0e13,
        "an astronomical camera must not be clipped to a room-sized limit: {} m",
        restored.camera().distance()
    );
}

#[test]
fn a_saved_scale_cannot_be_zero_negative_or_absent() {
    let experiment = authored();
    let bytes = serde_json::to_vec(&document_of(&experiment)).expect("encodes");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");

    // A ratio has no nearest sensible value to clamp towards, so unlike every
    // camera bound these are refused — as a *view* failure, which never stops
    // the experiment from opening.
    for broken in [
        serde_json::json!(0.0),
        serde_json::json!(-1.0),
        serde_json::json!("1 nm"),
        serde_json::Value::Null,
    ] {
        value["defaultView"]["scale"] = broken.clone();
        let bytes = serde_json::to_vec(&value).expect("encodes");
        let decoded = decode_document(&bytes).expect("the document still decodes");
        assert_eq!(
            decoded
                .default_view
                .as_ref()
                .expect("a section")
                .decode()
                .expect_err("not a scale")
                .code(),
            "malformed_view",
            "{broken}"
        );
        decoded
            .into_experiment(&schemas(), &Limits::DEFAULT)
            .expect("a bad scale cannot break an experiment");
    }

    // A version-2 section without a scale is not version 2.
    value["defaultView"] = serde_json::json!({
        "version": 2, "projection": "perspective",
        "camera": { "target": [0, 0, 0], "distance": 18, "yaw": 0, "pitch": 0 }
    });
    let bytes = serde_json::to_vec(&value).expect("encodes");
    assert_eq!(
        decode_document(&bytes)
            .expect("the document decodes")
            .default_view
            .as_ref()
            .expect("a section")
            .decode()
            .expect_err("version 2 declares a scale")
            .code(),
        "malformed_view"
    );
}

#[test]
fn a_saved_pose_is_rebounded_when_it_is_read_back() {
    // The file is untrusted input, so the pose bounds are enforced on the way
    // in and not merely on construction in Rust code.
    let section: StoredDefaultView = serde_json::from_str(
        r#"{"version":1,"projection":"orthographic",
            "camera":{"target":[0,0,0],"distance":1e12,"yaw":0,"pitch":0}}"#,
    )
    .expect("the section decodes");
    let view = section.decode().expect("version 1");
    assert_eq!(view.projection(), Projection::Orthographic);
    // Version 1 predates scene scales and meant one metre per unit, so the
    // reachable limit here is the render-unit bound at the default scale.
    assert_eq!(view.scale(), SceneScale::METRE);
    assert_eq!(
        view.camera().distance(),
        MAX_CAMERA_DISTANCE_UNITS * SceneScale::METRE.metres()
    );

    // A pose that names no camera at all is refused, and refusing it is a
    // *view* failure rather than a document one.
    let broken: StoredDefaultView = serde_json::from_str(
        r#"{"version":1,"projection":"perspective",
            "camera":{"target":[0,0,0],"distance":null,"yaw":0,"pitch":0}}"#,
    )
    .expect("the section decodes");
    assert_eq!(
        broken.decode().expect_err("no camera").code(),
        "malformed_view"
    );
}

#[test]
fn hostile_documents_produce_bounded_errors_rather_than_panics() {
    let refused = [
        // Truncated.
        "{\"format\":\"kagami.experiment\"",
        // Wrong types.
        r#"{"format":42,"formatVersion":1,"metadata":{},"experiment":{}}"#,
        // A field this version does not define: a producer that added it
        // without advancing the version would otherwise lose it on re-save.
        r#"{"format":"kagami.experiment","formatVersion":1,
            "metadata":{"generator":"g","created":"c","saved":"s"},
            "experiment":{"revision":0,"counters":{"objects":0,"variables":0},
            "setup":{"domain":{"lower":[-0.5,-0.5,-0.5],"upper":[0.5,0.5,0.5],
            "cells":[32,32,32],"boundary":["periodic","periodic","periodic"]},
            "timeStep":0.001,"plugins":[]},"surprise":true}}"#,
        // Deeply nested.
        &format!("{}{}", "[".repeat(512), "]".repeat(512)),
    ];
    for message in refused {
        // Through the real entry point: bytes reach this build as bytes, and
        // whatever `decode_document` does with them is what actually happens.
        assert!(
            decode_document(message.as_bytes()).is_err(),
            "decoding must refuse: {message}"
        );
        assert!(
            serde_json::from_str::<ExperimentDocument>(message).is_err(),
            "decoding must refuse: {message}"
        );
    }
}

#[test]
fn a_persisted_revision_is_provenance_rather_than_a_starting_point() {
    let experiment = authored();
    let document = document_of(&experiment);
    // The revision is save provenance, recorded beside the generator and the
    // timestamp rather than inside the authored content.
    assert_eq!(
        document.metadata.saved_revision,
        experiment.revision().get()
    );

    // Opening establishes a fresh history: a file may not rewind or fast
    // forward a live session's monotonic revision.
    let reopened = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect("valid");
    assert_eq!(
        reopened.revision(),
        kagami_document::ExperimentRevision::INITIAL
    );
    // But the counters do come back, so no identity is ever handed out twice.
    assert_eq!(reopened.counters().objects_minted(), 2);
    assert_eq!(reopened.counters().variables_minted(), 1);
}

#[test]
fn opening_a_document_replaces_the_session_atomically() {
    use kagami_session::{
        ActorId, CommandId, DocumentAuthority, DocumentTarget, ExperimentChange,
        ExperimentCommandEnvelope, SessionCommand,
    };

    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    let mut agent = support::Adapter::unguarded("agent");
    agent.edit(&mut authority, vec![support::create("Scratch")]);
    let before = authority.revision();
    assert!(authority.is_dirty());

    let opened = authored();
    let target = DocumentTarget::new("/tmp/orbit.kagami").expect("valid target");
    let accepted = authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("ui-open").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Open {
                experiment: Box::new(opened.clone()),
                target: Some(target.clone()),
                discard_unsaved: true,
            },
        ))
        .expect("the caller decided to discard");

    assert_eq!(accepted.change, ExperimentChange::Replaced { opened: true });
    // The contents are the document's...
    assert_eq!(authority.snapshot().objects(), opened.snapshot().objects());
    assert_eq!(
        authority.snapshot().variables(),
        opened.snapshot().variables()
    );
    // ...but the revision moved *forward*: a file may not rewind a session a
    // view has already caught up with.
    assert_eq!(authority.revision(), before.next());
    assert!(authority.revision() > opened.revision());
    // What is here is what is on disk, so it is clean and named.
    assert!(!authority.is_dirty());
    assert_eq!(authority.target(), Some(&target));
    // And the opening undo of a session does not empty the workspace.
    assert!(!authority.history_status().can_undo());
}

#[test]
fn replacing_a_document_with_unsaved_changes_needs_an_explicit_decision() {
    use kagami_session::{
        ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    };

    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    let mut agent = support::Adapter::unguarded("agent");
    agent.edit(&mut authority, vec![support::create("Unsaved")]);

    let refused = authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("ui-new").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::New {
                discard_unsaved: false,
            },
        ))
        .expect_err("the authority must not decide this for anyone");
    assert_eq!(refused.code(), "unsaved_changes");
    assert_eq!(authority.snapshot().object_count(), 1);

    // Stated explicitly, it proceeds — the same answer a dialog would have
    // collected, supplied as a request field (ADR 0006).
    authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("ui-new-2").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::New {
                discard_unsaved: true,
            },
        ))
        .expect("the caller decided");
    assert_eq!(authority.snapshot().object_count(), 0);
    assert!(!authority.is_dirty());
    assert_eq!(authority.target(), None);
}

#[test]
fn a_document_opened_without_its_plugin_is_reported_by_the_session() {
    use kagami_session::{
        ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    };

    let experiment = authored();
    let document = document_of(&experiment);
    let decoded = document
        .into_experiment(&SchemaRegistry::new(), &Limits::DEFAULT)
        .expect("loads without the plugin");

    let mut authority = DocumentAuthority::new(SchemaRegistry::new(), Limits::DEFAULT);
    authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("ui-open").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Open {
                experiment: Box::new(decoded),
                target: None,
                discard_unsaved: false,
            },
        ))
        .expect("a clean session needs no decision");

    // The session's own projection knows, so a view can say so without
    // recomputing anything.
    assert!(!authority.capabilities().is_complete());
    assert_eq!(authority.view().capabilities.summary().absent, 1);
}
