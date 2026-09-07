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
    ArtifactDigest, Limits, ScalarValue, authoring, canonical,
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
