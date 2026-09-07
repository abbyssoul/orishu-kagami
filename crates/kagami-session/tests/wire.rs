//! Golden fixtures for the session's adapter boundary, and one end-to-end
//! pass through it.
//!
//! The property worth asserting is not that these types serialise — it is that
//! a message reaching the authority through them is subject to exactly the
//! rules an in-process caller is. The last test therefore drives a whole
//! submission as an adapter would: decode, submit, encode the outcome.
//!
//! Set `KAGAMI_WIRE_BLESS=1` to rewrite the fixtures after an intentional
//! change to the representation.

mod support;

use std::path::PathBuf;

use kagami_document::{ExperimentCommand, ExperimentRevision, Limits, ObjectSpec, WireCommand};
use kagami_session::wire::{WireChange, WireSessionCommand};
use kagami_session::{
    ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    WireAcceptance, WireEnvelope, WireEvent, WireRejection,
};
use serde::{Serialize, de::DeserializeOwned};
use support::{create, mass_component, mass_properties, name, schemas};

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
        "the wire shape of `{name}` changed; if that is intended, re-bless the fixture and say \
         so wherever the representation is documented"
    );

    let parsed: T = serde_json::from_str(&expected).expect("fixture must deserialize");
    assert_eq!(&parsed, value, "`{name}` must round-trip");
}

fn envelope() -> ExperimentCommandEnvelope {
    ExperimentCommandEnvelope::new(
        CommandId::new("ui-1").expect("valid"),
        ActorId::new("ui").expect("valid"),
        SessionCommand::Edit(vec![create("Earth")]),
    )
    .guarded_by(ExperimentRevision::INITIAL)
}

#[test]
fn a_guarded_submission_pins_its_shape() {
    assert_fixture(
        "envelope",
        &WireEnvelope::of(&envelope()).expect("a guarded edit has a wire form"),
    );
}

#[test]
fn an_acceptance_pins_its_shape() {
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    let accepted = authority.submit(envelope()).expect("accepted");
    assert_fixture("acceptance", &WireAcceptance::of(&accepted));
}

#[test]
fn a_refusal_reports_a_stable_code_beside_its_explanation() {
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    authority.submit(envelope()).expect("accepted");

    // Guarded against a revision that has moved on.
    let rejection = authority
        .submit(
            ExperimentCommandEnvelope::new(
                CommandId::new("ui-2").expect("valid"),
                ActorId::new("ui").expect("valid"),
                SessionCommand::Edit(vec![create("Moon")]),
            )
            .guarded_by(ExperimentRevision::INITIAL),
        )
        .expect_err("the experiment moved on");
    assert_fixture("rejection", &WireRejection::of(&rejection));
}

#[test]
fn every_submission_kind_survives_a_round_trip() {
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    authority.submit(envelope()).expect("accepted");
    let snapshot = authority.snapshot();
    let earth = snapshot.resolve_object(0).expect("the created object");

    let envelopes = [
        envelope(),
        ExperimentCommandEnvelope::new(
            CommandId::new("ui-2").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Edit(vec![ExperimentCommand::RemoveObject(earth)]),
        ),
        ExperimentCommandEnvelope::new(
            CommandId::new("mcp-1").expect("valid"),
            ActorId::new("agent").expect("valid"),
            SessionCommand::Undo,
        ),
        ExperimentCommandEnvelope::new(
            CommandId::new("mcp-2").expect("valid"),
            ActorId::new("agent").expect("valid"),
            SessionCommand::Redo,
        ),
        ExperimentCommandEnvelope::new(
            CommandId::new("ui-3").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::BeginInteractiveEdit,
        ),
        ExperimentCommandEnvelope::new(
            CommandId::new("ui-4").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::EndInteractiveEdit,
        )
        .within(kagami_document::GestureId::new(7)),
    ];

    for envelope in envelopes {
        let encoded = WireEnvelope::of(&envelope).expect("every one of these has a wire form");
        let json = serde_json::to_string(&encoded).expect("encodes");
        let decoded: WireEnvelope = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, encoded, "the JSON form must round-trip");
        assert_eq!(
            decoded.into_envelope(&snapshot).expect("resolves"),
            envelope,
            "the submission must survive the representation unchanged"
        );
    }
}

#[test]
fn an_adapter_reaches_the_same_authority_through_the_representation() {
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);

    // Everything an adapter has: bytes, and a read it composed against.
    let message = r#"{
        "commandId": "mcp-1",
        "actor": "agent",
        "expectedRevision": 0,
        "command": {
            "kind": "edit",
            "commands": [{"command": "createObject", "spec": {"name": "Earth"}}]
        }
    }"#;

    let decoded: WireEnvelope = serde_json::from_str(message).expect("well-formed message");
    let envelope = decoded
        .into_envelope(&authority.snapshot())
        .expect("no identity to resolve");
    let accepted = authority.submit(envelope.clone()).expect("accepted");

    assert_eq!(authority.snapshot().object_count(), 1);
    let encoded = WireAcceptance::of(&accepted);
    assert_eq!(encoded.command_id.as_str(), "mcp-1");
    assert!(matches!(encoded.change, WireChange::Edited { .. }));

    // Idempotency holds across the boundary too: the identical message is the
    // identical request.
    let replayed = authority
        .submit(
            serde_json::from_str::<WireEnvelope>(message)
                .expect("well-formed")
                .into_envelope(&authority.snapshot())
                .expect("resolves"),
        )
        .expect("replayed");
    assert!(replayed.replayed);
    assert_eq!(authority.snapshot().object_count(), 1);
}

#[test]
fn a_message_cannot_widen_an_identity_bound() {
    let oversized = format!(
        r#"{{"commandId": "{}", "actor": "ui", "command": {{"kind": "undo"}}}}"#,
        "a".repeat(kagami_session::MAX_IDENTITY_BYTES + 1)
    );
    assert!(serde_json::from_str::<WireEnvelope>(&oversized).is_err());
    assert!(
        serde_json::from_str::<WireEnvelope>(
            r#"{"commandId": "", "actor": "ui", "command": {"kind": "undo"}}"#
        )
        .is_err()
    );
}

#[test]
fn a_capability_event_carries_no_command_identity() {
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("ui-1").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Edit(vec![ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(name("Earth")).with_component(mass_component(), mass_properties()),
            ))]),
        ))
        .expect("accepted");
    let before = authority.view().last_event;
    authority.adopt_schemas(
        kagami_catalog::SchemaRegistry::new(),
        ActorId::new("plugins").expect("valid"),
    );

    let event = authority
        .events_since(before)
        .next()
        .expect("one published event");
    let encoded = WireEvent::of(event);
    assert_eq!(encoded.command_id, None);
    // An absent identity is omitted rather than encoded as null, so a reader
    // cannot mistake it for an identity it failed to parse.
    let json = serde_json::to_string(&encoded).expect("encodes");
    assert!(!json.contains("commandId"), "{json}");
    assert_fixture("capability_event", &encoded);
}

#[test]
fn an_edit_message_naming_no_such_object_is_refused_before_the_authority_sees_it() {
    let authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    let message = r#"{
        "commandId": "mcp-1",
        "actor": "agent",
        "command": {"kind": "edit", "commands": [{"command": "removeObject", "object": 7}]}
    }"#;
    let decoded: WireEnvelope = serde_json::from_str(message).expect("well-formed message");
    let error = decoded
        .into_envelope(&authority.snapshot())
        .expect_err("there is no object 7");
    assert_eq!(error.code(), "unknown_object");
}

#[test]
fn the_document_and_session_representations_stay_separable() {
    // The session's shape wraps the document's rather than restating it, so a
    // command added to the model does not need a second definition here.
    let command = WireCommand::of(&create("Earth"));
    let session = WireSessionCommand::Edit {
        commands: vec![command.clone()],
    };
    let json = serde_json::to_value(&session).expect("encodes");
    assert_eq!(
        json["commands"][0],
        serde_json::to_value(&command).expect("encodes")
    );
}
