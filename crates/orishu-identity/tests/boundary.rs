//! Deserialization is a boundary check on hostile input, not a trusting decode.
//!
//! Every identity, label, and fingerprint reaches this crate from a peer, a
//! client, or a file. The contract is that a malformed value fails to parse
//! rather than entering the domain as an unchecked `String` or a wrong-length
//! digest. These tests drive that path through the public serde surface in both
//! a human-readable and a binary codec, so a regression cannot hide behind the
//! constructor's own unit tests.

use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, MembershipTombstone, NodeId, VersionTuple,
    WorkerName,
};

/// A CBOR byte string of `len` bytes, so a wrong-length digest can be presented
/// exactly as the binary wire would.
fn cbor_byte_string(len: usize) -> Vec<u8> {
    let mut cbor = Vec::new();
    if len < 24 {
        cbor.push(0x40 | len as u8); // major type 2, immediate length
    } else {
        cbor.push(0x58); // major type 2, 1-byte length follows
        cbor.push(len as u8);
    }
    cbor.extend(std::iter::repeat_n(0xAB, len));
    cbor
}

#[test]
fn empty_identity_is_refused_on_read() {
    let error = serde_json::from_str::<NodeId>("\"\"").unwrap_err();
    assert!(error.to_string().contains("must not be empty"));
}

#[test]
fn oversized_identity_is_refused_on_read() {
    let oversized = format!("\"{}\"", "n".repeat(200));
    assert!(serde_json::from_str::<FormationId>(&oversized).is_err());
}

#[test]
fn identity_with_whitespace_is_refused_on_read() {
    // An identity admits only printable non-whitespace ASCII, so a value a
    // label would accept is still refused as an identity.
    assert!(serde_json::from_str::<NodeId>("\"node 1\"").is_err());
}

#[test]
fn label_with_surrounding_whitespace_is_refused_on_read() {
    // Padding would make two visually identical labels compare unequal.
    let error = serde_json::from_str::<WorkerName>("\" west\"").unwrap_err();
    assert!(error.to_string().contains("whitespace"));
}

#[test]
fn label_with_a_control_character_is_refused_on_read() {
    assert!(serde_json::from_str::<ClusterName>("\"west\\u0000cluster\"").is_err());
}

#[test]
fn fingerprint_of_the_wrong_hex_length_is_refused_in_json() {
    assert!(serde_json::from_str::<CertFingerprint>("\"a1a1\"").is_err());
}

#[test]
fn fingerprint_from_a_short_integer_array_is_refused_in_json() {
    assert!(serde_json::from_str::<CertFingerprint>("[1,2,3]").is_err());
}

#[test]
fn fingerprint_from_a_short_cbor_byte_string_is_refused() {
    let short = cbor_byte_string(16);
    assert!(ciborium::from_reader::<CertFingerprint, _>(short.as_slice()).is_err());
}

#[test]
fn fingerprint_from_an_oversized_cbor_byte_string_is_refused() {
    let oversized = cbor_byte_string(33);
    assert!(ciborium::from_reader::<CertFingerprint, _>(oversized.as_slice()).is_err());
}

#[test]
fn fingerprint_of_the_exact_length_still_parses() {
    // A guard so the wrong-length assertions above are not passing for an
    // unrelated reason.
    let exact = cbor_byte_string(32);
    assert!(ciborium::from_reader::<CertFingerprint, _>(exact.as_slice()).is_ok());
}

#[test]
fn version_tuple_missing_its_actor_is_refused() {
    assert!(serde_json::from_str::<VersionTuple>(r#"{"epoch":1,"counter":0}"#).is_err());
}

#[test]
fn composite_validation_propagates_to_a_nested_identity() {
    // A tombstone whose nested node id is empty must be refused as a whole: the
    // inner validating deserialization is not skipped because it is nested.
    let json = r#"{
        "nodeId": "",
        "name": "worker-a",
        "certFingerprint": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        "removalMode": "force",
        "version": {"epoch": 1, "counter": 0, "actor": "node-seed"}
    }"#;
    assert!(serde_json::from_str::<MembershipTombstone>(json).is_err());
}
