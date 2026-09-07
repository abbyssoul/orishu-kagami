//! Golden canonical bytes and root digests for the workload model.
//!
//! These exist because workload identity is a contract with everything
//! downstream — O-WASM, storage, provenance, artifact transfer, and Kagami's
//! compiler all agree on a root digest or they do not interoperate. A change to
//! a field name, a field's presence, or the encoder's behaviour must therefore
//! show up as a diff in a checked-in file rather than as a silently different
//! identity.
//!
//! Each fixture keeps three files:
//!
//! - `<name>.workload.yaml` — the authored form, checked in as source.
//! - `canonical/<name>.cbor.hex` — the canonical bytes.
//! - `canonical/<name>.digest` — the root workload digest.
//!
//! Regenerate deliberately, never to make a red test green:
//!
//! ```sh
//! BLESS_WORKLOAD_FIXTURES=1 cargo test -p orishu-workload --test canonical
//! ```
//!
//! and review the resulting diff as a change to workload identity.

mod support;

use std::path::{Path, PathBuf};

use orishu_workload::{
    ArtifactDigest, Limits, ScalarValue, ToCanonical, authoring,
    canonical::{self, CanonicalValue},
    closure::{self, InMemoryBlobs},
};
use support::{Cbor, decode};

// ── golden-file plumbing ─────────────────────────────────────────────────────

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn authored(name: &str) -> String {
    let path = fixtures().join(format!("{name}.workload.yaml"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Compares `actual` against the checked-in golden, or rewrites it when
/// blessing.
fn golden(name: &str, actual: &str) {
    let path = fixtures().join("canonical").join(name);
    if std::env::var_os("BLESS_WORKLOAD_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().expect("fixture directory"))
            .expect("create the fixture directory");
        std::fs::write(&path, actual).expect("write the fixture");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}\n\
             capture it with BLESS_WORKLOAD_FIXTURES=1 and review the diff",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "the canonical form of `{name}` changed; this is a change to workload identity, \
         not a refactor"
    );
}

/// Hex, wrapped at 32 bytes a line so a diff points at a region rather than at
/// one enormous line.
fn wrapped_hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for chunk in bytes.chunks(32) {
        for byte in chunk {
            text.push_str(&format!("{byte:02x}"));
        }
        text.push('\n');
    }
    text
}

/// Asserts a fixture's canonical bytes and digest, and returns its manifest.
fn assert_canonical(name: &str) -> orishu_workload::WorkloadManifest {
    let limits = Limits::DEFAULT;
    let manifest = authoring::parse_str(&authored(name), &limits)
        .unwrap_or_else(|error| panic!("{name} parses: {error}"));

    let bytes = canonical::canonical_bytes(&manifest, &limits).expect("it encodes");
    golden(&format!("{name}.cbor.hex"), &wrapped_hex(&bytes));

    let digest = canonical::workload_digest(&manifest, &limits).expect("it digests");
    golden(&format!("{name}.digest"), &format!("{digest}\n"));

    // The digest is over exactly these bytes, not over some other rendering of
    // the manifest. Asserting it here means a fixture pair can never drift into
    // describing two different things.
    assert_eq!(
        digest.to_string(),
        ArtifactDigest::sha256_of(&bytes).to_string(),
        "the root digest must be the digest of the canonical bytes"
    );

    manifest
}

// ── the fixtures ─────────────────────────────────────────────────────────────

#[test]
fn the_minimal_workload_keeps_its_canonical_form() {
    assert_canonical("minimal");
}

#[test]
fn the_two_component_graph_keeps_its_canonical_form() {
    assert_canonical("two-component-graph");
}

// ── the encoding profile, verified by decoding it ────────────────────────────
//
// A hand-written encoder asserted only against its own output would agree with
// itself about anything. These decode the real canonical bytes with an
// independent reader and check the RFC 8949 §4.2 properties the profile claims.

#[test]
fn the_canonical_bytes_decode_as_deterministic_cbor() {
    for name in ["minimal", "two-component-graph"] {
        let manifest = authoring::parse_str(&authored(name), &Limits::DEFAULT).expect("it parses");
        let bytes = canonical::canonical_bytes(&manifest, &Limits::DEFAULT).expect("it encodes");
        let (value, consumed) = decode(&bytes).expect("the canonical bytes decode");
        assert_eq!(
            consumed,
            bytes.len(),
            "{name} left {} trailing bytes",
            bytes.len() - consumed
        );
        value.assert_deterministic(name);
    }
}

#[test]
fn the_root_is_a_map_of_exactly_the_four_envelope_fields() {
    let manifest = authoring::parse_str(&authored("minimal"), &Limits::DEFAULT).expect("it parses");
    let bytes = canonical::canonical_bytes(&manifest, &Limits::DEFAULT).expect("it encodes");
    let (value, _) = decode(&bytes).expect("it decodes");
    let Cbor::Map(entries) = &value else {
        panic!("the canonical root must be a map, got {value:?}")
    };
    let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_text()).collect();
    // Sorted bytewise on encoded keys, which for equal-length distinct strings
    // is plain lexicographic order.
    assert_eq!(keys, ["kind", "spec", "metadata", "apiVersion"]);
}

#[test]
fn no_location_bearing_key_reaches_the_canonical_form() {
    // ADR 0010's central rule, asserted against the bytes rather than against
    // the struct definition: if someone adds a `uri` field to a descriptor and
    // wires it into `ToCanonical`, this fails.
    let manifest = authoring::parse_str(&authored("two-component-graph"), &Limits::DEFAULT)
        .expect("it parses");
    let bytes = canonical::canonical_bytes(&manifest, &Limits::DEFAULT).expect("it encodes");
    let (value, _) = decode(&bytes).expect("it decodes");

    let forbidden = [
        "uri",
        "url",
        "path",
        "location",
        "source",
        "registry",
        "tag",
        "peer",
        "host",
        "credential",
        "token",
        "cachePath",
        "inline",
        "data",
    ];
    let mut found = Vec::new();
    value.walk_keys(&mut |key| {
        if forbidden.contains(&key) {
            found.push(key.to_owned());
        }
    });
    assert!(
        found.is_empty(),
        "a workload's canonical form must contain no retrieval location, but it has: {found:?}"
    );
}

#[test]
fn no_runtime_status_key_reaches_the_canonical_form() {
    let manifest = authoring::parse_str(&authored("two-component-graph"), &Limits::DEFAULT)
        .expect("it parses");
    let bytes = canonical::canonical_bytes(&manifest, &Limits::DEFAULT).expect("it encodes");
    let (value, _) = decode(&bytes).expect("it decodes");

    let forbidden = [
        "status",
        "phase",
        "epoch",
        "simulationTime",
        "partitionMap",
        "convergenceMetrics",
        "uid",
        "namespace",
        "workloadId",
        "resourceVersion",
    ];
    let mut found = Vec::new();
    value.walk_keys(&mut |key| {
        if forbidden.contains(&key) {
            found.push(key.to_owned());
        }
    });
    assert!(
        found.is_empty(),
        "runtime status and cluster-assigned identity must not affect workload identity, \
         but the canonical form contains: {found:?}"
    );
}

// ── the encoding is a codec, and it is injective ─────────────────────────────
//
// Round-tripping is not a convenience test. An encoder that quietly dropped a
// field would give two different workloads one digest; decoding is what makes
// that detectable, so these are identity-correctness tests.

#[test]
fn every_fixture_survives_a_round_trip_through_its_canonical_form() {
    for name in ["minimal", "two-component-graph"] {
        let limits = Limits::DEFAULT;
        let original = authoring::parse_str(&authored(name), &limits).expect("it parses");
        let bytes = canonical::canonical_bytes(&original, &limits).expect("it encodes");

        let recovered = canonical::manifest_from_canonical_bytes(&bytes, &limits)
            .unwrap_or_else(|error| panic!("{name} must decode: {error}"));

        assert_eq!(
            recovered, original,
            "{name} did not survive a canonical round trip"
        );
        assert_eq!(
            canonical::canonical_bytes(&recovered, &limits).expect("it re-encodes"),
            bytes,
            "{name} re-encoded to different bytes"
        );
    }
}

#[test]
fn a_manifest_recovered_from_canonical_bytes_keeps_its_identity() {
    let limits = Limits::DEFAULT;
    let original =
        authoring::parse_str(&authored("two-component-graph"), &limits).expect("it parses");
    let bytes = canonical::canonical_bytes(&original, &limits).expect("it encodes");
    let recovered = canonical::manifest_from_canonical_bytes(&bytes, &limits).expect("it decodes");
    assert_eq!(
        canonical::workload_digest(&recovered, &limits).expect("digest"),
        canonical::workload_digest(&original, &limits).expect("digest"),
    );
}

#[test]
fn the_decoder_refuses_what_the_profile_forbids() {
    let limits = Limits::DEFAULT;
    // Each of these is a valid CBOR document that is not *canonical* CBOR.
    // Accepting any of them would mean two byte strings decode to one value
    // while their digests disagree about which one they identify.
    let cases: [(&str, Vec<u8>); 6] = [
        ("an indefinite-length array", vec![0x9f, 0x01, 0xff]),
        ("a non-shortest integer", vec![0x18, 0x01]),
        ("a binary16 float", vec![0xf9, 0x3c, 0x00]),
        ("null", vec![0xf6]),
        ("a tag", vec![0xc0, 0x00]),
        (
            "out-of-order map keys",
            // {"b": 1, "a": 2}
            vec![0xa2, 0x61, 0x62, 0x01, 0x61, 0x61, 0x02],
        ),
    ];
    for (what, bytes) in cases {
        assert!(
            canonical::decode(&bytes, &limits).is_err(),
            "{what} must be refused"
        );
    }
}

#[test]
fn the_decoder_refuses_negative_zero_and_duplicate_keys() {
    let limits = Limits::DEFAULT;
    let mut negative_zero = vec![0xfb];
    negative_zero.extend_from_slice(&(-0.0f64).to_be_bytes());
    assert!(canonical::decode(&negative_zero, &limits).is_err());

    // {"a": 1, "a": 2}
    let duplicate = vec![0xa2, 0x61, 0x61, 0x01, 0x61, 0x61, 0x02];
    assert!(canonical::decode(&duplicate, &limits).is_err());
}

/// Re-encodes a fixture's canonical form with one field rewritten.
///
/// The result is well-formed canonical CBOR — sorted keys, definite lengths —
/// so nothing in the byte-level profile objects to it. Only the model
/// conversion can, which is the point of these tests.
fn with_root_field(name: &str, field: &str, value: CanonicalValue) -> Vec<u8> {
    let manifest = authoring::parse_str(&authored(name), &Limits::DEFAULT).expect("it parses");
    let CanonicalValue::Map(spec_root) = manifest.to_canonical().expect("it canonicalises") else {
        panic!("the canonical root is a map");
    };
    let mut entries: Vec<_> = spec_root.entries().to_vec();
    entries.retain(|(key, _)| key != &CanonicalValue::text(field));
    entries.push((CanonicalValue::text(field), value));
    let tampered = CanonicalValue::Map(canonical::CanonicalMap::new(entries).expect("distinct"));
    canonical::encode(&tampered, &Limits::DEFAULT).expect("it encodes")
}

#[test]
fn an_explicitly_encoded_empty_collection_is_refused() {
    // "There are none" has exactly one spelling: absence. Were a
    // present-but-empty list accepted, two byte strings would decode to one
    // manifest, and re-encoding would reproduce only one of them — so the
    // digest over the other would name a workload nobody could rebuild.
    let limits = Limits::DEFAULT;
    let manifest = authoring::parse_str(&authored("minimal"), &limits).expect("it parses");
    let CanonicalValue::Map(root) = manifest.to_canonical().expect("canonicalises") else {
        panic!("the canonical root is a map");
    };

    // Rebuild `metadata` with an empty `labels` map written out explicitly.
    let metadata = root
        .entries()
        .iter()
        .find(|(key, _)| key == &CanonicalValue::text("metadata"))
        .map(|(_, value)| value.clone())
        .expect("metadata is present");
    let CanonicalValue::Map(metadata) = metadata else {
        panic!("metadata is a map");
    };
    let mut metadata_entries: Vec<_> = metadata.entries().to_vec();
    metadata_entries.push((
        CanonicalValue::text("labels"),
        CanonicalValue::Map(canonical::CanonicalMap::new(Vec::new()).expect("empty is distinct")),
    ));
    let tampered_metadata =
        CanonicalValue::Map(canonical::CanonicalMap::new(metadata_entries).expect("distinct keys"));

    let bytes = with_root_field("minimal", "metadata", tampered_metadata);
    let error = canonical::manifest_from_canonical_bytes(&bytes, &limits)
        .expect_err("an explicitly empty map is not canonical");
    assert!(
        error.to_string().contains("omitting the field"),
        "the error should say how an empty collection is spelled, got: {error}"
    );
}

#[test]
fn an_explicitly_encoded_empty_list_is_refused() {
    let limits = Limits::DEFAULT;
    let manifest =
        authoring::parse_str(&authored("two-component-graph"), &limits).expect("it parses");
    let CanonicalValue::Map(root) = manifest.to_canonical().expect("canonicalises") else {
        panic!("the canonical root is a map");
    };
    // `spec.compute.placementConstraints` is present in this fixture; replacing
    // it with an empty list is well-formed CBOR and must still be refused.
    let spec = root
        .entries()
        .iter()
        .find(|(key, _)| key == &CanonicalValue::text("spec"))
        .map(|(_, value)| value.clone())
        .expect("spec is present");
    let CanonicalValue::Map(spec) = spec else {
        panic!("spec is a map")
    };
    let mut spec_entries: Vec<_> = spec.entries().to_vec();
    for (key, value) in &mut spec_entries {
        if key == &CanonicalValue::text("compute") {
            let CanonicalValue::Map(compute) = value else {
                panic!("compute is a map")
            };
            let mut compute_entries: Vec<_> = compute.entries().to_vec();
            for (key, value) in &mut compute_entries {
                if key == &CanonicalValue::text("placementConstraints") {
                    *value = CanonicalValue::Array(Vec::new());
                }
            }
            *value = CanonicalValue::Map(
                canonical::CanonicalMap::new(compute_entries).expect("distinct"),
            );
        }
    }
    let tampered_spec =
        CanonicalValue::Map(canonical::CanonicalMap::new(spec_entries).expect("distinct"));

    let bytes = with_root_field("two-component-graph", "spec", tampered_spec);
    assert!(
        canonical::manifest_from_canonical_bytes(&bytes, &limits).is_err(),
        "an explicitly empty list is not canonical"
    );
}

#[test]
fn decoding_accepts_only_a_manifests_own_canonical_form() {
    // The property, stated directly: whatever `manifest_from_canonical_bytes`
    // returns, the bytes it was given must be exactly what encoding that
    // manifest produces. This is the backstop that holds even for a second
    // spelling nobody has thought of yet.
    let limits = Limits::DEFAULT;
    for name in ["minimal", "two-component-graph"] {
        let bytes = canonical::canonical_bytes(
            &authoring::parse_str(&authored(name), &limits).expect("parses"),
            &limits,
        )
        .expect("encodes");
        let recovered =
            canonical::manifest_from_canonical_bytes(&bytes, &limits).expect("it decodes");
        assert_eq!(
            canonical::canonical_bytes(&recovered, &limits).expect("re-encodes"),
            bytes,
            "{name} is not its own canonical form"
        );
    }
}

#[test]
fn whatever_encodes_under_a_limit_decodes_under_the_same_limit() {
    // The symmetry that makes a digest meaningful: if a manifest can be given
    // an identity under some bounds, the bytes that identity is taken over must
    // be readable back under those same bounds. An encoder allowed to outrun
    // its decoder would mint workloads nobody could ever load.
    let manifest =
        authoring::parse_str(&authored("two-component-graph"), &Limits::DEFAULT).expect("parses");
    let natural = canonical::canonical_bytes(&manifest, &Limits::DEFAULT)
        .expect("it encodes under the default")
        .len();

    // Sweep the limit across the boundary, so the transition itself is covered
    // rather than one comfortable value on each side.
    for limit in [
        1,
        natural / 2,
        natural - 1,
        natural,
        natural + 1,
        natural * 2,
    ] {
        let limits = Limits {
            max_manifest_bytes: limit as u64,
            ..Limits::DEFAULT
        };
        match canonical::canonical_bytes(&manifest, &limits) {
            Ok(bytes) => {
                assert!(
                    bytes.len() <= limit,
                    "encoding at limit {limit} produced {} bytes",
                    bytes.len()
                );
                canonical::manifest_from_canonical_bytes(&bytes, &limits).unwrap_or_else(|error| {
                    panic!("bytes encoded at limit {limit} must decode at limit {limit}: {error}")
                });
            }
            Err(error) => assert!(
                matches!(error, canonical::CanonicalError::EncodedTooLarge { .. }),
                "at limit {limit} encoding should fail only on size, got {error}"
            ),
        }
    }
}

#[test]
fn a_digest_is_never_taken_over_bytes_a_reader_would_refuse() {
    // The same property stated where it bites: `workload_digest` must not
    // succeed where decoding the bytes it hashed would fail.
    let manifest = authoring::parse_str(&authored("minimal"), &Limits::DEFAULT).expect("it parses");
    let tight = Limits {
        max_manifest_bytes: 64,
        ..Limits::DEFAULT
    };
    assert!(
        canonical::workload_digest(&manifest, &tight).is_err(),
        "a manifest too large to read back must not be given an identity"
    );
}

#[test]
fn a_document_over_the_byte_limit_is_refused_before_it_is_read() {
    // The collection headers can claim no more than the input holds, so
    // bounding the input is what bounds every allocation underneath — but only
    // if the input is bounded, which it was not.
    let manifest = authoring::parse_str(&authored("minimal"), &Limits::DEFAULT).expect("it parses");
    let bytes = canonical::canonical_bytes(&manifest, &Limits::DEFAULT).expect("it encodes");
    let tight = Limits {
        max_manifest_bytes: 16,
        ..Limits::DEFAULT
    };
    let error = canonical::decode(&bytes, &tight).expect_err("over the limit");
    assert!(
        matches!(error, canonical::CanonicalError::TooLarge { limit: 16, .. }),
        "got {error}"
    );
    assert!(canonical::manifest_from_canonical_bytes(&bytes, &tight).is_err());
}

#[test]
fn the_decoder_refuses_trailing_bytes() {
    // A canonical form is exactly one value. Ignoring a suffix would let two
    // byte strings carry one workload.
    let limits = Limits::DEFAULT;
    let manifest = authoring::parse_str(&authored("minimal"), &limits).expect("it parses");
    let mut bytes = canonical::canonical_bytes(&manifest, &limits).expect("it encodes");
    bytes.push(0x00);
    assert!(canonical::manifest_from_canonical_bytes(&bytes, &limits).is_err());
}

#[test]
fn the_decoder_refuses_a_truncated_document() {
    let limits = Limits::DEFAULT;
    let manifest = authoring::parse_str(&authored("minimal"), &limits).expect("it parses");
    let bytes = canonical::canonical_bytes(&manifest, &limits).expect("it encodes");
    for cut in [1, bytes.len() / 3, bytes.len() / 2, bytes.len() - 1] {
        assert!(
            canonical::manifest_from_canonical_bytes(&bytes[..cut], &limits).is_err(),
            "a document cut at {cut} must be refused"
        );
    }
}

#[test]
fn the_decoder_refuses_a_length_header_larger_than_the_document() {
    // The cheapest defence against a header claiming billions of elements:
    // every element costs at least a byte, so a length past the remaining
    // input is a lie whatever follows. Without this the reader would try to
    // reserve capacity for it first.
    let limits = Limits::DEFAULT;
    // An array header claiming 2^32 items, followed by nothing.
    let bytes = vec![0x9a, 0xff, 0xff, 0xff, 0xff];
    assert!(canonical::decode(&bytes, &limits).is_err());
    // The same for a map and for a text string.
    assert!(canonical::decode(&[0xba, 0xff, 0xff, 0xff, 0xff], &limits).is_err());
    assert!(canonical::decode(&[0x7a, 0xff, 0xff, 0xff, 0xff], &limits).is_err());
}

#[test]
fn the_decoder_bounds_the_number_of_values_it_will_build() {
    let manifest =
        authoring::parse_str(&authored("two-component-graph"), &Limits::DEFAULT).expect("parses");
    let bytes = canonical::canonical_bytes(&manifest, &Limits::DEFAULT).expect("it encodes");
    let tight = Limits {
        max_canonical_values: 8,
        ..Limits::DEFAULT
    };
    assert!(canonical::manifest_from_canonical_bytes(&bytes, &tight).is_err());
}

#[test]
fn the_decoder_refuses_a_document_that_is_not_a_workload() {
    let limits = Limits::DEFAULT;
    // Structurally fine canonical CBOR, but not this schema.
    let bytes = canonical::encode(
        &CanonicalValue::Map(
            canonical::CanonicalMap::fields([("hello", Some(CanonicalValue::text("world")))])
                .expect("distinct keys"),
        ),
        &limits,
    )
    .expect("it encodes");
    let error = canonical::manifest_from_canonical_bytes(&bytes, &limits)
        .expect_err("that is not a workload");
    // The error should locate the problem, not merely announce one.
    assert!(
        error.to_string().contains('$'),
        "the error should carry a path, got: {error}"
    );
}

#[test]
fn the_decoder_refuses_a_field_the_schema_does_not_define() {
    // The same answer the authoring path gives, for the same reason: in an
    // identity-bearing document a key nobody reads is either a mistake or
    // something being carried past the digest.
    let limits = Limits::DEFAULT;
    let manifest = authoring::parse_str(&authored("minimal"), &limits).expect("it parses");
    let CanonicalValue::Map(root) = manifest.to_canonical().expect("it canonicalises") else {
        panic!("the canonical root is a map");
    };
    let mut entries: Vec<_> = root.entries().to_vec();
    entries.push((
        CanonicalValue::text("status"),
        CanonicalValue::text("Running"),
    ));
    let tampered = CanonicalValue::Map(canonical::CanonicalMap::new(entries).expect("distinct"));
    let bytes = canonical::encode(&tampered, &limits).expect("it encodes");

    let error = canonical::manifest_from_canonical_bytes(&bytes, &limits)
        .expect_err("an unknown field is refused");
    assert!(
        error.to_string().contains("status"),
        "the error should name the field, got: {error}"
    );
}

// ── what changes identity, and what does not ─────────────────────────────────

/// The field's initial state, and a re-authored version of it.
///
/// Deliberately the same length, so the substitution below changes only the
/// digest. A replacement of a different size would also trip the declared-size
/// check, and the test would pass for the wrong reason.
const FIELD_INITIAL_STATE: &[u8] = b"field-initial-state";
const FIELD_INITIAL_STATE_V2: &[u8] = b"field-init-state-v2";

/// Re-authors the field's starting state, as editing an experiment would.
fn with_changed_initial_conditions(document: &str) -> String {
    let replaced = document.replace(
        &ArtifactDigest::sha256_of(FIELD_INITIAL_STATE).to_string(),
        &ArtifactDigest::sha256_of(FIELD_INITIAL_STATE_V2).to_string(),
    );
    assert_ne!(replaced, document, "the substitution must have applied");
    replaced
}

#[test]
fn changing_initial_conditions_changes_the_root_but_not_the_components() {
    let limits = Limits::DEFAULT;
    let original =
        authoring::parse_str(&authored("two-component-graph"), &limits).expect("it parses");
    let changed_document = with_changed_initial_conditions(&authored("two-component-graph"));
    let changed = authoring::parse_str(&changed_document, &limits).expect("it parses");

    assert_ne!(
        canonical::workload_digest(&original, &limits).expect("digest"),
        canonical::workload_digest(&changed, &limits).expect("digest"),
        "new initial conditions are a new workload"
    );

    // The point of a content-addressed closure: a worker that already holds the
    // component blobs fetches only the new input.
    let original_components: Vec<_> = original
        .spec
        .compute
        .components
        .iter()
        .map(|component| component.artifact.clone())
        .collect();
    let changed_components: Vec<_> = changed
        .spec
        .compute
        .components
        .iter()
        .map(|component| component.artifact.clone())
        .collect();
    assert_eq!(
        original_components, changed_components,
        "changing an input must not disturb component descriptors"
    );
}

#[test]
fn a_changed_label_is_a_different_workload() {
    // Labels are identity-bearing here, unlike a server annotation. That is the
    // contract, so it gets a test rather than a comment.
    let limits = Limits::DEFAULT;
    let original =
        authoring::parse_str(&authored("two-component-graph"), &limits).expect("it parses");
    let relabelled = authoring::parse_str(
        &authored("two-component-graph").replace("regime: relativistic", "regime: newtonian"),
        &limits,
    )
    .expect("it parses");
    assert_ne!(
        canonical::workload_digest(&original, &limits).expect("digest"),
        canonical::workload_digest(&relabelled, &limits).expect("digest"),
    );
}

#[test]
fn a_reordered_step_plan_is_a_different_workload() {
    // Array order is the author's and is committed to: two plans listing the
    // same nodes in a different sequence are not obviously the same schedule,
    // and silently equating them would hide a real edit.
    let limits = Limits::DEFAULT;
    let mut manifest =
        authoring::parse_str(&authored("two-component-graph"), &limits).expect("it parses");
    let before = canonical::workload_digest(&manifest, &limits).expect("digest");
    manifest.spec.compute.step_plan.invocations.reverse();
    let after = canonical::workload_digest(&manifest, &limits).expect("digest");
    assert_ne!(before, after);
}

#[test]
fn an_empty_collection_and_an_absent_one_are_one_workload() {
    // Otherwise a serializer that started emitting `roles: []` would change
    // every workload's identity without any authored change.
    let limits = Limits::DEFAULT;
    let original = authoring::parse_str(&authored("minimal"), &limits).expect("it parses");
    let mut explicit = original.clone();
    explicit.spec.compute.components[0].limits.clear();
    explicit.spec.compute.placement_constraints.clear();
    assert_eq!(
        canonical::workload_digest(&original, &limits).expect("digest"),
        canonical::workload_digest(&explicit, &limits).expect("digest"),
    );
}

#[test]
fn config_map_order_does_not_reach_the_identity() {
    let limits = Limits::DEFAULT;
    let document = authored("two-component-graph");
    let swapped = document.replace(
        "          permittivity: 8.8541878128e-12\n          haloDepth: 1\n",
        "          haloDepth: 1\n          permittivity: 8.8541878128e-12\n",
    );
    assert_ne!(swapped, document, "the substitution must have applied");
    assert_eq!(
        canonical::workload_digest(
            &authoring::parse_str(&document, &limits).expect("parses"),
            &limits
        )
        .expect("digest"),
        canonical::workload_digest(
            &authoring::parse_str(&swapped, &limits).expect("parses"),
            &limits
        )
        .expect("digest"),
    );
}

#[test]
fn a_negative_zero_config_value_is_the_same_workload_as_a_positive_zero() {
    let limits = Limits::DEFAULT;
    let mut positive = authoring::parse_str(&authored("minimal"), &limits).expect("it parses");
    let name = "bias".parse().expect("a valid parameter name");
    positive.spec.compute.components[0]
        .config
        .insert(name, ScalarValue::real(0.0).expect("finite"));

    let mut negative = positive.clone();
    let name = "bias".parse().expect("a valid parameter name");
    negative.spec.compute.components[0]
        .config
        .insert(name, ScalarValue::real(-0.0).expect("finite"));

    assert_eq!(
        canonical::workload_digest(&positive, &limits).expect("digest"),
        canonical::workload_digest(&negative, &limits).expect("digest"),
    );
}

#[test]
fn a_non_finite_value_never_becomes_part_of_a_workload() {
    // Rejected at construction rather than at encoding, so a manifest that
    // exists can always be identified and there is no "this workload has no
    // digest" state for a caller to handle.
    assert!(ScalarValue::real(f64::NAN).is_err());
    assert!(ScalarValue::real(f64::INFINITY).is_err());
    assert!(ScalarValue::real(f64::NEG_INFINITY).is_err());
    assert!(ScalarValue::real(1.5).is_ok());
}

// ── the closure, end to end ──────────────────────────────────────────────────

/// The blobs the two-component fixture declares.
fn two_component_blobs() -> InMemoryBlobs {
    [
        b"field-component-wasm".to_vec(),
        b"dynamics-component-wasm".to_vec(),
        b"cavity-mesh".to_vec(),
        FIELD_INITIAL_STATE.to_vec(),
        b"particle-initial-state".to_vec(),
    ]
    .into_iter()
    .collect()
}

#[test]
fn the_two_component_workload_validates_against_its_blobs() {
    let limits = Limits::DEFAULT;
    let manifest =
        authoring::parse_str(&authored("two-component-graph"), &limits).expect("it parses");
    let verified = closure::validate_closure(&manifest, &two_component_blobs(), &limits)
        .unwrap_or_else(|report| panic!("the fixture must be admissible:\n{report}"));

    assert_eq!(
        verified.root(),
        canonical::workload_digest(&manifest, &limits).expect("digest"),
        "the verified closure carries the manifest's own identity"
    );
    // Two components, one geometry, two initial conditions.
    assert_eq!(verified.len(), 5);
}

#[test]
fn a_worker_holding_the_components_needs_only_the_new_input() {
    // The acceptance criterion for content addressing: re-authoring the initial
    // conditions must leave the component blobs reusable.
    let limits = Limits::DEFAULT;
    let changed = authoring::parse_str(
        &with_changed_initial_conditions(&authored("two-component-graph")),
        &limits,
    )
    .expect("it parses");

    // Everything the original workload needed, minus the input that changed.
    let mut held = InMemoryBlobs::new();
    for blob in [
        b"field-component-wasm".to_vec(),
        b"dynamics-component-wasm".to_vec(),
        b"cavity-mesh".to_vec(),
        b"particle-initial-state".to_vec(),
    ] {
        held.insert(blob);
    }

    let report =
        closure::validate_closure(&changed, &held, &limits).expect_err("the new input is missing");
    assert_eq!(
        report.len(),
        1,
        "only the changed input should be outstanding, got:\n{report}"
    );
    assert!(matches!(
        &report.errors()[0],
        orishu_workload::ClosureError::MissingArtifact { role, .. }
            if role.as_str() == orishu_workload::ArtifactRole::INITIAL_CONDITIONS
    ));

    // Supplying just that one blob completes the closure.
    let mut complete = held;
    complete.insert(FIELD_INITIAL_STATE_V2.to_vec());
    assert!(closure::validate_closure(&changed, &complete, &limits).is_ok());
}
