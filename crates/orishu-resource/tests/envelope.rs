//! The envelope's wire contract, asserted against real serialized bytes.
//!
//! Field order, an absent versus present `status`, and the unknown-field
//! policy are the three things a consumer's canonical encoding and its
//! diagnostics depend on, so they are tested through `serde_json` and
//! `serde_yaml` rather than by inspecting a Rust value. A test that only ever
//! round-trips in memory would not notice the envelope emitting its fields in
//! a different order.

use orishu_resource::{
    AllowUnknown, ApiVersion, DenyUnknown, Kind, NoStatus, Resource, ResourceHeader,
};
use serde::{Deserialize, Serialize};

// ── two resource shapes, standing in for the two real consumers ─────────────

/// Orishu-shaped: a human name plus an optional system-assigned ID.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObjectMeta {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    domain_type: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    node_count: u32,
}

type Permissive = Resource<ObjectMeta, Spec, Status>;

/// Kagami-shaped: a catalog and a template name, and no status at all.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogMeta {
    catalog: String,
    name: String,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    components: Vec<String>,
}

type Strict = Resource<CatalogMeta, CatalogSpec, NoStatus, DenyUnknown>;

fn api_version() -> ApiVersion {
    ApiVersion::from_static("example.dev/v1")
}

fn kind() -> Kind {
    Kind::from_static("Widget")
}

fn permissive() -> Permissive {
    Resource::new(
        api_version(),
        kind(),
        ObjectMeta {
            name: "one".to_owned(),
            id: None,
        },
        Spec {
            domain_type: "electromagnetic".to_owned(),
        },
    )
}

// ── field spelling and order ───────────────────────────────────────────────

#[test]
fn the_fields_are_spelled_and_ordered_exactly_as_the_format_declares() {
    assert_eq!(
        serde_json::to_string(&permissive()).unwrap(),
        r#"{"apiVersion":"example.dev/v1","kind":"Widget","metadata":{"name":"one"},"spec":{"domainType":"electromagnetic"}}"#
    );
}

#[test]
fn a_status_is_emitted_last_when_present() {
    let resource = permissive().with_status(Status { node_count: 3 });
    assert_eq!(
        serde_json::to_string(&resource).unwrap(),
        r#"{"apiVersion":"example.dev/v1","kind":"Widget","metadata":{"name":"one"},"spec":{"domainType":"electromagnetic"},"status":{"nodeCount":3}}"#
    );
}

#[test]
fn an_absent_status_is_absent_rather_than_null() {
    let yaml = serde_yaml::to_string(&permissive()).unwrap();
    assert!(!yaml.contains("status"), "{yaml}");
    assert_eq!(
        yaml,
        "apiVersion: example.dev/v1\nkind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: electromagnetic\n"
    );
}

#[test]
fn a_resource_without_a_status_type_never_encodes_one() {
    let document = Strict::new(
        ApiVersion::from_static("kagami.catalog/v1"),
        Kind::from_static("ObjectTemplate"),
        CatalogMeta {
            catalog: "planets".to_owned(),
            name: "sun".to_owned(),
        },
        CatalogSpec {
            components: vec!["mass".to_owned()],
        },
    );
    assert_eq!(
        serde_yaml::to_string(&document).unwrap(),
        "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata:\n  catalog: planets\n  name: sun\nspec:\n  components:\n  - mass\n"
    );
}

// ── round trips ────────────────────────────────────────────────────────────

#[test]
fn a_resource_round_trips_through_json() {
    let original = permissive().with_status(Status { node_count: 7 });
    let encoded = serde_json::to_string(&original).unwrap();
    let decoded: Permissive = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(decoded.api_version(), &api_version());
    assert_eq!(decoded.kind(), &kind());
}

#[test]
fn a_resource_round_trips_through_yaml() {
    let original = permissive();
    let encoded = serde_yaml::to_string(&original).unwrap();
    let decoded: Permissive = serde_yaml::from_str(&encoded).unwrap();
    assert_eq!(decoded, original);
    assert!(decoded.status.is_none());
}

#[test]
fn decoding_does_not_depend_on_the_order_the_document_was_written_in() {
    let reordered: Permissive = serde_yaml::from_str(
        "spec:\n  domainType: electromagnetic\nkind: Widget\nmetadata:\n  name: one\napiVersion: example.dev/v1\n",
    )
    .unwrap();
    assert_eq!(reordered, permissive());
    // ...and re-encoding puts it back into canonical order, which is what
    // makes a content fingerprint independent of the authored layout.
    assert_eq!(
        serde_yaml::to_string(&reordered).unwrap(),
        serde_yaml::to_string(&permissive()).unwrap()
    );
}

// ── unknown versions and kinds ─────────────────────────────────────────────

#[test]
fn an_unknown_future_resource_decodes_as_a_header_and_is_refused_by_expect() {
    let header: ResourceHeader = serde_yaml::from_str(
        "apiVersion: example.dev/v9\nkind: Widget\nspec:\n  somethingNew: [1, 2, 3]\n",
    )
    .unwrap();

    let error = header.expect(&api_version(), &kind()).unwrap_err();
    assert!(error.is_version_mismatch());
    assert_eq!(error.found_api_version.as_str(), "example.dev/v9");
    assert_eq!(error.expected_api_version.as_str(), "example.dev/v1");
    assert_eq!(
        error.to_string(),
        r#"expected apiVersion="example.dev/v1" kind="Widget", got apiVersion="example.dev/v9" kind="Widget""#
    );
}

#[test]
fn an_unknown_kind_is_reported_with_both_discriminators_and_no_borrowed_vocabulary() {
    let header: ResourceHeader =
        serde_json::from_str(r#"{"apiVersion":"example.dev/v1","kind":"Gadget"}"#).unwrap();
    let error = header.expect(&api_version(), &kind()).unwrap_err();
    assert!(!error.is_version_mismatch());
    let message = error.to_string();
    assert!(message.contains(r#"kind="Widget""#), "{message}");
    assert!(message.contains(r#"kind="Gadget""#), "{message}");
    // The whole point of the extraction: no consumer's vocabulary is baked in.
    assert!(!message.contains("Workload"), "{message}");
    assert!(!message.contains("orishu"), "{message}");
}

#[test]
fn a_decoded_resource_can_check_its_own_discriminator() {
    let resource = permissive();
    assert!(resource.expect(&api_version(), &kind()).is_ok());
    assert!(
        resource
            .expect(&api_version(), &Kind::from_static("Gadget"))
            .is_err()
    );
}

// ── unknown fields ─────────────────────────────────────────────────────────

const WITH_EXTRA_FIELD: &str = "apiVersion: example.dev/v1\nkind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: electromagnetic\nsomethingElse: [1, 2]\n";

#[test]
fn a_permissive_resource_ignores_an_unknown_top_level_field() {
    let decoded: Permissive = serde_yaml::from_str(WITH_EXTRA_FIELD).unwrap();
    assert_eq!(decoded, permissive());
}

#[test]
fn a_strict_resource_refuses_an_unknown_top_level_field() {
    let error = serde_yaml::from_str::<Strict>(
        "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata:\n  catalog: planets\n  name: sun\nspec: {}\nsomethingElse: 1\n",
    )
    .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("somethingElse"), "{message}");
    assert!(
        message.contains("apiVersion"),
        "expected the known fields to be listed: {message}"
    );
}

// ── the `status` rule, across both policies ────────────────────────────────
//
// A `status` carrying a value always reaches the status type, so a `NoStatus`
// resource refuses it under either policy. An explicit `status: null` never
// reaches the status type — `Option` resolves null itself — so it follows the
// unknown-field policy, like any other valueless top-level field.
//
// The permissive half of that is a compatibility requirement, not an
// oversight: serde's derive decodes `status: null` as `None`, so the Orishu
// manifest this envelope replaced already accepted it. The strict half is what
// makes `NoStatus` + `DenyUnknown` refuse every `status` spelling, which is
// the pairing `kagami-catalog` uses.

/// The permissive counterpart of `Strict`: no status type, lenient policy.
type PermissiveNoStatus = Resource<CatalogMeta, CatalogSpec, NoStatus, AllowUnknown>;

fn no_status_document(status: &str) -> String {
    format!(
        "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata:\n  catalog: planets\n  \
         name: sun\nspec: {{}}\n{status}"
    )
}

#[test]
fn a_status_carrying_a_value_is_refused_by_a_no_status_resource_under_either_policy() {
    let document = no_status_document("status:\n  phase: Ready\n");

    let strict = serde_yaml::from_str::<Strict>(&document).unwrap_err();
    assert!(strict.to_string().contains("no `status`"), "{strict}");

    let permissive = serde_yaml::from_str::<PermissiveNoStatus>(&document).unwrap_err();
    assert!(
        permissive.to_string().contains("no `status`"),
        "a value always reaches the status type, whatever the policy: {permissive}"
    );
}

#[test]
fn a_strict_resource_refuses_an_explicitly_null_status() {
    // A bare `status:`, which YAML reads as null, and the explicit spelling.
    for status in ["status:\n", "status: null\n"] {
        let error = serde_yaml::from_str::<Strict>(&no_status_document(status)).unwrap_err();
        assert!(
            error.to_string().contains("must not be null"),
            "{status:?} must not be accepted by a strict resource: {error}"
        );
    }
}

#[test]
fn a_permissive_resource_reads_an_explicitly_null_status_as_absent() {
    for status in ["status:\n", "status: null\n"] {
        let document = serde_yaml::from_str::<PermissiveNoStatus>(&no_status_document(status))
            .unwrap_or_else(|error| panic!("{status:?} should decode as absent: {error}"));
        assert!(document.status.is_none());
        // ...and re-encoding does not resurrect the key.
        assert!(!serde_yaml::to_string(&document).unwrap().contains("status"));
    }
}

#[test]
fn the_null_status_rule_is_the_same_for_a_resource_that_has_a_status_type() {
    // Nothing about the rule depends on `NoStatus`; it belongs to the policy.
    const NULL_STATUS: &str = "apiVersion: example.dev/v1\nkind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: electromagnetic\nstatus: null\n";

    let decoded: Permissive = serde_yaml::from_str(NULL_STATUS).unwrap();
    assert_eq!(decoded, permissive());
    assert!(decoded.status.is_none());

    type StrictWithStatus = Resource<ObjectMeta, Spec, Status, DenyUnknown>;
    assert!(serde_yaml::from_str::<StrictWithStatus>(NULL_STATUS).is_err());
}

// ── malformed and missing input ────────────────────────────────────────────

#[test]
fn a_missing_envelope_field_names_the_field() {
    let error = serde_yaml::from_str::<Permissive>(
        "apiVersion: example.dev/v1\nkind: Widget\nspec:\n  domainType: em\n",
    )
    .unwrap_err();
    assert!(error.to_string().contains("metadata"), "{error}");

    let error = serde_yaml::from_str::<Permissive>(
        "kind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: em\n",
    )
    .unwrap_err();
    assert!(error.to_string().contains("apiVersion"), "{error}");
}

#[test]
fn a_duplicate_envelope_field_is_refused_rather_than_last_one_winning() {
    let error = serde_json::from_str::<Permissive>(
        r#"{"apiVersion":"example.dev/v1","kind":"Widget","kind":"Gadget","metadata":{"name":"one"},"spec":{"domainType":"em"}}"#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("duplicate field"), "{error}");
}

#[test]
fn a_body_of_the_wrong_shape_reports_a_structured_error() {
    let error = serde_yaml::from_str::<Permissive>(
        "apiVersion: example.dev/v1\nkind: Widget\nmetadata: not-a-map\nspec:\n  domainType: em\n",
    )
    .unwrap_err();
    assert!(error.to_string().contains("metadata"), "{error}");
}

#[test]
fn an_oversized_discriminator_is_refused_before_the_body_is_decoded() {
    let long = "a".repeat(ApiVersion::MAX_LEN + 1);
    let error = serde_yaml::from_str::<Permissive>(&format!(
        "apiVersion: {long}\nkind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: em\n"
    ))
    .unwrap_err();
    assert!(error.to_string().contains("exceeds"), "{error}");

    let long_kind = "A".repeat(Kind::MAX_LEN + 1);
    let error = serde_yaml::from_str::<ResourceHeader>(&format!(
        "apiVersion: example.dev/v1\nkind: {long_kind}\n"
    ))
    .unwrap_err();
    assert!(error.to_string().contains("exceeds"), "{error}");
}

#[test]
fn a_discriminator_that_is_not_a_name_is_refused() {
    let error = serde_yaml::from_str::<ResourceHeader>(
        "apiVersion: example.dev/v1\nkind: \"not a kind\"\n",
    )
    .unwrap_err();
    assert!(error.to_string().contains("resource kind"), "{error}");
}

// ── construction ───────────────────────────────────────────────────────────

#[test]
fn a_resource_cannot_be_built_without_naming_its_kind() {
    // `Resource` has private discriminator fields and no `Default`, so the
    // only way to make one is to supply both halves. This test documents the
    // property; the compiler enforces it.
    let resource = permissive();
    assert_eq!(resource.kind().as_str(), "Widget");
    assert_eq!(resource.header().kind(), resource.kind());
}

#[test]
fn into_parts_hands_back_everything_without_cloning() {
    let (api_version, kind, metadata, spec, status) = permissive()
        .with_status(Status { node_count: 1 })
        .into_parts();
    assert_eq!(api_version.as_str(), "example.dev/v1");
    assert_eq!(kind.as_str(), "Widget");
    assert_eq!(metadata.name, "one");
    assert_eq!(spec.domain_type, "electromagnetic");
    assert_eq!(status, Some(Status { node_count: 1 }));
}

#[test]
fn the_permissive_policy_is_the_default() {
    fn assert_default<M, S, T>(_: &Resource<M, S, T, AllowUnknown>) {}
    let resource: Resource<ObjectMeta, Spec, Status> = permissive();
    assert_default(&resource);
}

// ── equivalence with the definitions this crate replaced ───────────────────
//
// `Resource`'s serde is written by hand, so "it behaves exactly like the
// derive it replaced" is an assertion rather than a property of the code. The
// twins below are byte-for-byte copies of the two definitions that existed
// before this crate: `orishu::model::manifest::Manifest` and
// `kagami_catalog::document::TemplateDocument`. If the hand-written impl ever
// drifts, an already-serialized Orishu resource or an already-recorded
// catalog fingerprint would silently stop matching; these tests are what make
// that a build failure instead.

/// What `orishu::model::manifest::Manifest<T, S>` was.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyOrishuManifest {
    api_version: String,
    kind: String,
    metadata: ObjectMeta,
    spec: Spec,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<Status>,
}

/// What `kagami_catalog::document::TemplateDocument` was.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyTemplateDocument {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: CatalogMeta,
    spec: CatalogSpec,
}

#[test]
fn the_envelope_encodes_exactly_as_the_orishu_manifest_derive_did() {
    let legacy = LegacyOrishuManifest {
        api_version: "example.dev/v1".to_owned(),
        kind: "Widget".to_owned(),
        metadata: ObjectMeta {
            name: "one".to_owned(),
            id: Some("abc".to_owned()),
        },
        spec: Spec {
            domain_type: "electromagnetic".to_owned(),
        },
        status: None,
    };
    let shared = Resource::<ObjectMeta, Spec, Status>::new(
        api_version(),
        kind(),
        ObjectMeta {
            name: "one".to_owned(),
            id: Some("abc".to_owned()),
        },
        Spec {
            domain_type: "electromagnetic".to_owned(),
        },
    );

    assert_eq!(
        serde_json::to_string(&shared).unwrap(),
        serde_json::to_string(&legacy).unwrap()
    );
    assert_eq!(
        serde_yaml::to_string(&shared).unwrap(),
        serde_yaml::to_string(&legacy).unwrap()
    );
}

#[test]
fn the_envelope_encodes_a_status_exactly_as_the_derive_did() {
    let legacy = LegacyOrishuManifest {
        api_version: "example.dev/v1".to_owned(),
        kind: "Widget".to_owned(),
        metadata: ObjectMeta {
            name: "one".to_owned(),
            id: None,
        },
        spec: Spec {
            domain_type: "em".to_owned(),
        },
        status: Some(Status { node_count: 4 }),
    };
    let shared = Resource::<ObjectMeta, Spec, Status>::new(
        api_version(),
        kind(),
        ObjectMeta {
            name: "one".to_owned(),
            id: None,
        },
        Spec {
            domain_type: "em".to_owned(),
        },
    )
    .with_status(Status { node_count: 4 });

    assert_eq!(
        serde_json::to_string(&shared).unwrap(),
        serde_json::to_string(&legacy).unwrap()
    );
}

#[test]
fn the_envelope_encodes_exactly_as_the_catalog_document_derive_did() {
    const DOCUMENT: &str = "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata:\n  catalog: planets\n  name: sun\nspec:\n  components:\n  - mass\n";

    let legacy: LegacyTemplateDocument = serde_yaml::from_str(DOCUMENT).unwrap();
    let shared: Strict = serde_yaml::from_str(DOCUMENT).unwrap();

    // The canonical bytes a content fingerprint is taken over.
    assert_eq!(
        serde_yaml::to_string(&shared).unwrap(),
        serde_yaml::to_string(&legacy).unwrap()
    );
    assert_eq!(serde_yaml::to_string(&shared).unwrap(), DOCUMENT);
}

#[test]
fn both_definitions_reject_and_accept_the_same_documents() {
    // An unknown top-level field: refused by the strict envelope, exactly as
    // `deny_unknown_fields` refused it.
    const EXTRA: &str = "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata:\n  catalog: planets\n  name: sun\nspec: {}\nextra: 1\n";
    assert!(serde_yaml::from_str::<LegacyTemplateDocument>(EXTRA).is_err());
    assert!(serde_yaml::from_str::<Strict>(EXTRA).is_err());

    // ...and accepted by the permissive one, exactly as the Orishu manifest
    // accepted it.
    assert!(serde_yaml::from_str::<LegacyOrishuManifest>(WITH_EXTRA_FIELD).is_ok());
    assert!(serde_yaml::from_str::<Permissive>(WITH_EXTRA_FIELD).is_ok());

    // A missing `status` is `None` in both, rather than a missing-field error.
    const NO_STATUS: &str = "apiVersion: example.dev/v1\nkind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: em\n";
    assert!(
        serde_yaml::from_str::<LegacyOrishuManifest>(NO_STATUS)
            .unwrap()
            .status
            .is_none()
    );
    assert!(
        serde_yaml::from_str::<Permissive>(NO_STATUS)
            .unwrap()
            .status
            .is_none()
    );

    // An explicit `status: null` is `None` in both too. This is the reason the
    // permissive policy tolerates that spelling: the derive did, so refusing
    // it here would be a wire change smuggled into an extraction.
    const NULL_STATUS: &str = "apiVersion: example.dev/v1\nkind: Widget\nmetadata:\n  name: one\nspec:\n  domainType: em\nstatus: null\n";
    assert!(
        serde_yaml::from_str::<LegacyOrishuManifest>(NULL_STATUS)
            .unwrap()
            .status
            .is_none()
    );
    assert!(
        serde_yaml::from_str::<Permissive>(NULL_STATUS)
            .unwrap()
            .status
            .is_none()
    );
}
