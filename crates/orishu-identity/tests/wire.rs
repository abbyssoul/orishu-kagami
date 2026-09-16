//! The serialized wire contract of the identity types.
//!
//! The peer and client protocols describe these shapes in prose, and
//! `docs/protocol-p2p.md` is the authority for them. Prose drifts; a fixture
//! that must round-trip does not. Each file in `tests/fixtures/` is the
//! canonical JSON form of one type, and the tests assert both directions: the
//! fixture deserializes into the domain type, and the domain type re-serializes
//! to exactly the fixture.
//!
//! JSON pins the *field contract* — names, casing, nesting, and which fields may
//! be omitted — which is identical in both encodings. The one place the
//! encodings genuinely differ is [`CertFingerprint`]: hex text in a
//! human-readable format, a byte string in a binary one. That branch is this
//! crate's own `Serializer::is_human_readable` logic, so it is pinned directly
//! against CBOR bytes rather than left to a consumer's codec.
//!
//! Set `ORISHU_IDENTITY_BLESS=1` to rewrite the JSON fixtures after an
//! intentional contract change. That is a wire-visible change and needs a
//! corresponding update to `docs/protocol-p2p.md`.

use std::path::PathBuf;

use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, MembershipTombstone, NodeId,
    ProtocolRange, ProtocolVersion, RemovalMode, VersionTuple, WorkerName,
};
use serde::{Serialize, de::DeserializeOwned};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"))
}

/// Asserts that `value` serializes to the fixture and the fixture deserializes
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

    if std::env::var("ORISHU_IDENTITY_BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().expect("fixtures directory"))
            .expect("fixture directory must be creatable");
        std::fs::write(&path, &rendered).expect("fixture must be writable");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}. Re-run with ORISHU_IDENTITY_BLESS=1 to create it.",
            path.display()
        )
    });
    assert_eq!(
        rendered, expected,
        "the wire shape of `{name}` changed; if that is intended, update \
         docs/protocol-p2p.md and re-bless the fixture"
    );

    let parsed: T = serde_json::from_str(&expected).expect("fixture must deserialize");
    assert_eq!(&parsed, value, "`{name}` must round-trip");
}

/// A recognisable fingerprint whose hex reveals byte order: `000102…1f`.
fn sample_fingerprint() -> CertFingerprint {
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = index as u8;
    }
    CertFingerprint::from_bytes(bytes)
}

fn sample_tombstone(reason: Option<&str>, cleared: bool) -> MembershipTombstone {
    MembershipTombstone {
        node_id: NodeId::new("node-abc-123").unwrap(),
        name: WorkerName::new("west rack 3").unwrap(),
        cert_fingerprint: sample_fingerprint(),
        removal_mode: RemovalMode::Force,
        version: VersionTuple::initial(4, NodeId::new("node-seed").unwrap()),
        cleared,
        reason: reason.map(str::to_owned),
    }
}

#[test]
fn membership_tombstone_names_every_field_in_camel_case() {
    assert_fixture(
        "membership_tombstone",
        &sample_tombstone(Some("decommissioned"), false),
    );
}

#[test]
fn cleared_tombstone_omits_the_optional_reason() {
    assert_fixture(
        "membership_tombstone_cleared",
        &sample_tombstone(None, true),
    );
}

#[test]
fn version_tuple_is_epoch_counter_actor() {
    let version = VersionTuple {
        epoch: 4,
        counter: 7,
        actor: NodeId::new("node-seed").unwrap(),
    };
    assert_fixture("version_tuple", &version);
}

#[test]
fn protocol_range_is_min_max() {
    let range = ProtocolRange::new(ProtocolVersion(1), ProtocolVersion(3)).unwrap();
    assert_fixture("protocol_range", &range);
}

#[test]
fn identities_and_labels_are_bare_strings_in_json() {
    // No wrapper object, tag, or field name: the new-type is transparent on the
    // wire, so a peer reads a string and this crate validates it.
    assert_eq!(
        serde_json::to_string(&FormationId::new("form-01").unwrap()).unwrap(),
        "\"form-01\""
    );
    assert_eq!(
        serde_json::to_string(&NodeId::new("node-01").unwrap()).unwrap(),
        "\"node-01\""
    );
    assert_eq!(
        serde_json::to_string(&ClusterName::new("west cluster").unwrap()).unwrap(),
        "\"west cluster\""
    );
}

#[test]
fn transparent_numbers_carry_no_wrapper() {
    assert_eq!(serde_json::to_string(&ProtocolVersion(1)).unwrap(), "1");
    assert_eq!(serde_json::to_string(&Incarnation(9)).unwrap(), "9");
}

#[test]
fn fingerprint_is_lowercase_hex_in_json() {
    let json = serde_json::to_string(&sample_fingerprint()).unwrap();
    assert_eq!(
        json,
        "\"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\""
    );
    assert_eq!(
        serde_json::from_str::<CertFingerprint>(&json).unwrap(),
        sample_fingerprint()
    );
}

#[test]
fn fingerprint_is_a_cbor_byte_string_not_an_integer_array() {
    let mut cbor = Vec::new();
    ciborium::into_writer(&sample_fingerprint(), &mut cbor).unwrap();

    // CBOR major type 2 (byte string), length 32: the 1-byte-length prefix
    // `0x58 0x20` then exactly 32 payload bytes. An integer array would be
    // major type 4 (`0x98 …`) and far larger.
    assert_eq!(
        cbor.len(),
        34,
        "a 32-byte byte string is two header bytes plus 32"
    );
    assert_eq!(cbor[0], 0x58, "major type 2 with a 1-byte length");
    assert_eq!(cbor[1], 0x20, "declared length is 32 bytes");
    assert_eq!(&cbor[2..], sample_fingerprint().as_bytes());

    let decoded: CertFingerprint = ciborium::from_reader(cbor.as_slice()).unwrap();
    assert_eq!(decoded, sample_fingerprint());
}

#[test]
fn fingerprint_json_and_cbor_describe_the_same_value() {
    let json = serde_json::to_string(&sample_fingerprint()).unwrap();
    let from_json: CertFingerprint = serde_json::from_str(&json).unwrap();

    let mut cbor = Vec::new();
    ciborium::into_writer(&sample_fingerprint(), &mut cbor).unwrap();
    let from_cbor: CertFingerprint = ciborium::from_reader(cbor.as_slice()).unwrap();

    assert_eq!(from_json, from_cbor);
}
