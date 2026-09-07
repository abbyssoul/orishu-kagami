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
use kagami_session::{DocumentMetadata, ExperimentDocument, FORMAT_VERSION};
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
    assert_fixture(
        "experiment",
        &ExperimentDocument::of(
            &experiment,
            &experiment.snapshot(),
            metadata_for(&experiment),
        ),
    );
}

#[test]
fn an_empty_document_pins_its_shape() {
    let experiment = Experiment::new();
    assert_fixture(
        "experiment_empty",
        &ExperimentDocument::of(
            &experiment,
            &experiment.snapshot(),
            metadata_for(&experiment),
        ),
    );
}

#[test]
fn saving_and_opening_reproduces_the_same_experiment() {
    let experiment = authored();
    let document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
    let document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
    let once = serde_json::to_string_pretty(&ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    ))
    .expect("encodes");

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
    let twice = serde_json::to_string_pretty(&ExperimentDocument::of(
        &reopened,
        &reopened.snapshot(),
        metadata_at(experiment.revision().get()),
    ))
    .expect("encodes");

    assert_eq!(once, twice);
}

#[test]
fn a_document_from_a_newer_build_is_declined_rather_than_half_read() {
    let experiment = authored();
    let mut document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
    let mut document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
    document.format = "fieldcad.scene".to_owned();
    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("not a Kagami experiment");
    assert_eq!(error.code(), "wrong_format");
}

#[test]
fn a_document_naming_an_uninstalled_plugin_still_opens() {
    let experiment = authored();
    let document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );

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
    let document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
    let mut document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
    let mut document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
    document.experiment.objects[1].id = document.experiment.objects[0].id;

    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("two objects with one identity");
    assert_eq!(error.code(), "duplicate_identity");
}

#[test]
fn a_document_whose_expression_does_not_resolve_is_refused() {
    let experiment = authored();
    let mut document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
    document.experiment.variables.clear();

    let error = document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect_err("the definition its object reads is gone");
    assert_eq!(error.code(), "expression_unresolved");
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
        assert!(
            serde_json::from_str::<ExperimentDocument>(message).is_err(),
            "decoding must refuse: {message}"
        );
    }
}

#[test]
fn a_persisted_revision_is_provenance_rather_than_a_starting_point() {
    let experiment = authored();
    let document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
    let document = ExperimentDocument::of(
        &experiment,
        &experiment.snapshot(),
        metadata_for(&experiment),
    );
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
