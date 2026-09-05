//! Golden fixtures for the wire-visible membership types.
//!
//! The peer protocol describes these shapes in prose. Prose drifts; a fixture
//! that must round-trip does not. Each file in `tests/fixtures/` is the
//! canonical JSON form of one type, and the tests below assert both directions:
//! the fixture deserializes into the domain type, and the domain type
//! re-serializes to exactly the fixture.
//!
//! JSON rather than CBOR because CBOR is the codec's business, and the codec
//! is deliberately not in this crate. What is pinned here is the *field
//! contract* — names, casing, nesting, and which fields may be omitted — which
//! is identical in both encodings. The one place the encodings differ is
//! binary fields: hashes and fingerprints are hex strings in JSON and byte
//! strings in CBOR, which the types handle through
//! `Serializer::is_human_readable`.
//!
//! Set `ORISHU_MEMBERSHIP_BLESS=1` to rewrite the fixtures after an
//! intentional contract change. That is a wire-visible change and needs a
//! corresponding update to `docs/protocol-p2p.md`.

use std::path::PathBuf;

use orishu_membership::{
    DeltaBody, GossipDelta, MembershipTombstone, antientropy::MembershipTree, testing,
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

    if std::env::var("ORISHU_MEMBERSHIP_BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().expect("fixtures directory"))
            .expect("fixture directory must be creatable");
        std::fs::write(&path, &rendered).expect("fixture must be writable");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}. Re-run with ORISHU_MEMBERSHIP_BLESS=1 to create it.",
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

#[test]
fn membership_update_delta() {
    assert_fixture(
        "gossip_membership_update",
        &GossipDelta {
            hops: 2,
            body: DeltaBody::MembershipUpdate(testing::member("node-0001", 4)),
        },
    );
}

#[test]
fn tombstone_update_delta() {
    assert_fixture(
        "gossip_tombstone_update",
        &GossipDelta {
            hops: 0,
            body: DeltaBody::TombstoneUpdate(testing::tombstone(
                "node-0002",
                orishu_membership::RemovalMode::Force,
            )),
        },
    );
}

#[test]
fn blocklist_update_delta() {
    assert_fixture(
        "gossip_blocklist_update",
        &GossipDelta {
            hops: 1,
            body: DeltaBody::BlocklistUpdate(testing::blocklist_entry("worker-banned")),
        },
    );
}

#[test]
fn foreign_delta_is_relayed_with_its_payload_opaque() {
    assert_fixture(
        "gossip_foreign",
        &GossipDelta {
            hops: 3,
            body: DeltaBody::Foreign(testing::foreign_delta()),
        },
    );
}

#[test]
fn merkle_digest() {
    let digest = MembershipTree::build(&testing::golden_model()).digest();
    assert_fixture("merkle_digest", &digest);
}

#[test]
fn membership_tombstone() {
    assert_fixture(
        "membership_tombstone",
        &testing::tombstone("node-0002", orishu_membership::RemovalMode::Blocklist),
    );
}

#[test]
fn a_cleared_tombstone_is_distinguishable_on_the_wire() {
    let mut cleared = testing::tombstone("node-0002", orishu_membership::RemovalMode::Force);
    cleared.cleared = true;
    let json = serde_json::to_value(&cleared).unwrap();
    assert_eq!(json["cleared"], true);
    assert_eq!(
        serde_json::from_value::<MembershipTombstone>(json).unwrap(),
        cleared
    );
}

#[test]
fn an_absent_cleared_flag_defaults_to_fenced() {
    // Older senders omit the field. Defaulting to `false` keeps the barrier
    // in place; defaulting the other way would let an old peer silently lift
    // every tombstone it gossiped.
    let mut json = serde_json::to_value(testing::tombstone(
        "node-0002",
        orishu_membership::RemovalMode::Force,
    ))
    .unwrap();
    json.as_object_mut().unwrap().remove("cleared");
    let parsed: MembershipTombstone = serde_json::from_value(json).unwrap();
    assert!(!parsed.cleared);
}

#[test]
fn identities_are_validated_when_read_from_the_wire() {
    // A decoder must not be able to produce an empty or whitespace-bearing
    // identity, because every downstream check assumes it cannot happen.
    let mut json = serde_json::to_value(GossipDelta {
        hops: 0,
        body: DeltaBody::MembershipUpdate(testing::member("node-0001", 1)),
    })
    .unwrap();
    json["data"]["id"] = serde_json::Value::String(String::new());
    assert!(serde_json::from_value::<GossipDelta>(json).is_err());
}

#[test]
fn a_fingerprint_of_the_wrong_length_is_rejected() {
    let mut json = serde_json::to_value(GossipDelta {
        hops: 0,
        body: DeltaBody::MembershipUpdate(testing::member("node-0001", 1)),
    })
    .unwrap();
    json["data"]["certFingerprint"] = serde_json::Value::String("abcd".into());
    assert!(serde_json::from_value::<GossipDelta>(json).is_err());
}
