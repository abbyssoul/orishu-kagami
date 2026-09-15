mod support;
use orishu_plugin::*;
use serde_json::json;
use std::collections::BTreeMap;
use support::*;

#[test]
fn all_six_payloads_and_independent_providers_roundtrip_without_inventory() {
    let l = Limits::default();
    let items = declarations();
    for (_, p) in &items {
        let bytes = p.canonical_bytes(&l).unwrap();
        let decoded = payload_from_cbor(p.point(), &bytes, &l).unwrap();
        assert_eq!(decoded.canonical_bytes(&l).unwrap(), bytes);
        assert_eq!(
            decoded.contract_ref(&l).unwrap(),
            p.contract_ref(&l).unwrap()
        );
        let independently_decoded: ciborium::Value =
            ciborium::from_reader(bytes.as_slice()).unwrap();
        assert!(matches!(independently_decoded, ciborium::Value::Map(_)));
    }
    // Solver B's complete declaration is valid even before vocabulary A is installed.
    let (a, ab) = release("org.example.vocabulary", &items[..5], &[]);
    let (b, bb) = release(
        "org.example.solvers",
        &items[5..],
        &[b"classical-kernel-fixture", b"euler-kernel-fixture"],
    );
    for (r, blobs) in [(a, ab), (b, bb)] {
        r.validate(&l)
            .unwrap()
            .verify_all(&borrowed(&blobs))
            .unwrap();
        assert_eq!(
            release_from_cbor(&r.canonical_bytes(&l).unwrap(), &l)
                .unwrap()
                .release_id(&l)
                .unwrap(),
            r.release_id(&l).unwrap()
        );
        let author = serde_json::to_vec(&r).unwrap();
        assert_eq!(release_from_json(&author, &l).unwrap(), r);
    }
}

#[test]
fn presentation_changes_payload_and_release_but_not_scientific_identity() {
    let l = Limits::default();
    let mut p = declarations().remove(2).1;
    let original = p.clone();
    let before = p.contract_ref(&l).unwrap();
    if let Payload::Observables(d) = &mut p {
        d.presentation = Some(Annotations {
            label: Some("Gravity arrows".into()),
            ..Default::default()
        });
    }
    assert_eq!(before, p.contract_ref(&l).unwrap());
    assert_ne!(
        original.canonical_bytes(&l).unwrap(),
        p.canonical_bytes(&l).unwrap()
    );
    let (a, _) = release("org.example.a", &[("channel", original)], &[]);
    let (b, _) = release("org.example.a", &[("channel", p.clone())], &[]);
    assert_ne!(a.release_id(&l).unwrap(), b.release_id(&l).unwrap());
    if let Payload::Observables(d) = &mut p {
        d.scientific.dimension = Dimension::FORCE;
    }
    assert_ne!(before, p.contract_ref(&l).unwrap());
}

#[test]
fn sets_sort_but_semantic_property_and_axis_order_is_preserved() {
    let l = Limits::default();
    let (mut r, _) = release("org.example.a", &declarations()[..5], &[]);
    let before = r.canonical_bytes(&l).unwrap();
    r.0.spec.contributions.reverse();
    r.0.spec.artifacts.reverse();
    assert_eq!(before, r.canonical_bytes(&l).unwrap());
    let mut p = declarations().remove(5).1;
    let before = p.contract_ref(&l).unwrap();
    if let Payload::FieldModels(d) = &mut p {
        d.scientific.requirements.reverse();
    }
    assert_eq!(before, p.contract_ref(&l).unwrap());
}

#[test]
fn unknown_payload_is_verified_without_parsing_and_corruption_is_not_dormancy() {
    let l = Limits::default();
    let (mut r, mut blobs) = release("org.example.a", &declarations()[..1], &[]);
    let bytes = b"not a parseable payload";
    let digest = ArtifactDigest::sha256_of(bytes);
    blobs.insert(digest, bytes.to_vec());
    r.0.spec.artifacts.push(Artifact {
        digest,
        size_bytes: bytes.len() as u64,
        media_type: "application/unknown".into(),
    });
    r.0.spec.contributions.push(Contribution {
        local_id: "future".parse().unwrap(),
        extension_point: "org.example.future/v2".parse().unwrap(),
        payload: digest,
        requirements: vec![],
        annotations: None,
    });
    let v = r.validate(&l).unwrap();
    let checked = v.verify_all(&borrowed(&blobs)).unwrap();
    assert!(checked[&"future".parse().unwrap()].payload().is_none());
    assert_eq!(checked[&"future".parse().unwrap()].digest(), digest);
    blobs.insert(digest, b"corrupt".to_vec());
    assert_eq!(
        v.verify_all(&borrowed(&blobs)).unwrap_err().code,
        ErrorCode::IntegrityMismatch
    );
}

#[test]
fn local_targets_must_match_exact_semantic_identity() {
    let l = Limits::default();
    let (mut r, blobs) = release("org.example.a", &declarations()[..5], &[]);
    let c =
        r.0.spec
            .contributions
            .iter_mut()
            .find(|c| c.local_id.as_str() == "gravity")
            .unwrap();
    c.requirements[0] = Requirement::Local {
        slot: "acceleration".parse().unwrap(),
        local_contribution: "acceleration".parse().unwrap(),
    };
    r.validate(&l)
        .unwrap()
        .verify_all(&borrowed(&blobs))
        .unwrap();
    let c =
        r.0.spec
            .contributions
            .iter_mut()
            .find(|c| c.local_id.as_str() == "gravity")
            .unwrap();
    c.requirements[0] = Requirement::Local {
        slot: "acceleration".parse().unwrap(),
        local_contribution: "mass".parse().unwrap(),
    };
    assert!(
        r.validate(&l)
            .unwrap()
            .verify_all(&borrowed(&blobs))
            .is_err()
    );
}

#[test]
fn root_metadata_duplicates_nulls_unknowns_and_wrong_versions_are_rejected() {
    let l = Limits::default();
    let (r, _) = release("org.example.a", &[], &[]);
    let mut value = serde_json::to_value(&r).unwrap();
    for key in ["status", "releaseId", "extra"] {
        value[key] = json!(false);
        assert!(release_from_json(&serde_json::to_vec(&value).unwrap(), &l).is_err());
        value.as_object_mut().unwrap().remove(key);
    }
    value["metadata"]["description"] = json!(null);
    assert!(release_from_json(&serde_json::to_vec(&value).unwrap(), &l).is_err());
    assert!(
        release_from_json(
            br#"{"apiVersion":"orishu.plugin/v1","apiVersion":"orishu.plugin/v1"}"#,
            &l
        )
        .is_err()
    );
    value = serde_json::to_value(r).unwrap();
    value["apiVersion"] = json!("orishu.plugin/v2");
    assert_eq!(
        release_from_json(&serde_json::to_vec(&value).unwrap(), &l)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
}

#[test]
fn json_collection_refusal_precedes_deserializing_rejected_entry() {
    let l = Limits {
        max_contributions: 0,
        ..Default::default()
    };
    // It is deliberately not valid JSON at the rejected entry. Reading that
    // value would fail syntax; the correct result is the collection bound.
    let e = release_from_json(br#"{"spec":{"contributions":[THIS_IS_NOT_JSON"#, &l).unwrap_err();
    assert_eq!(e.code, ErrorCode::LimitExceeded);
    assert_eq!(e.path, "contributions");

    let l = Limits {
        max_object_fields: 0,
        ..Default::default()
    };
    let e = release_from_json(br#"{"an unterminated key"#, &l).unwrap_err();
    assert_eq!(e.code, ErrorCode::LimitExceeded);
}

#[test]
fn a_tight_list_budget_does_not_reduce_fixed_object_field_budget() {
    let p = declarations().remove(0).1;
    let l = Limits {
        max_schema_items: 1,
        ..Default::default()
    };
    let bytes = p.canonical_bytes(&l).unwrap();
    assert!(payload_from_cbor(KnownPoint::Components, &bytes, &l).is_ok());
}

#[test]
fn scalar_matrix_roles_and_execution_contract_mismatches_fail() {
    let l = Limits::default();
    let mut items = declarations();
    if let Payload::Observables(d) = &mut items[2].1 {
        d.scientific.shape = Shape::Matrix {
            rows: 17,
            columns: 3,
        };
    }
    assert!(items[2].1.canonical_bytes(&l).is_err());
    if let Payload::Components(d) = &mut items[1].1 {
        d.scientific.bindings.inertial_mass = Some("missing".parse().unwrap());
    }
    assert!(items[1].1.validate(&l).is_err());
    if let Payload::FieldModels(d) = &mut items[5].1 {
        d.scientific.execution_contract = ExecutionContractId::Dynamics;
    }
    assert_eq!(
        items[5].1.validate(&l).unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
}

#[test]
fn size_depth_value_budgets_and_canonical_hostile_forms_fail() {
    let l = Limits::default();
    let (r, _) = release("org.example.a", &[], &[]);
    let bytes = r.canonical_bytes(&l).unwrap();
    assert_eq!(
        release_from_cbor(
            &bytes,
            &Limits {
                max_manifest_bytes: bytes.len() - 1,
                ..l
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        release_from_cbor(&bytes, &Limits { max_values: 1, ..l })
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        release_from_cbor(&bytes, &Limits { max_depth: 0, ..l })
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    for b in [
        &[0x9f, 0xff][..],
        &[0xf6],
        &[0xa1, 0x61, b'x', 0xf9, 0, 0],
        &[0x9a, 0xff, 0xff, 0xff, 0xff],
    ] {
        assert!(release_from_cbor(b, &l).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(release_from_cbor(&trailing, &l).is_err());
    for end in 0..bytes.len() {
        assert!(release_from_cbor(&bytes[..end], &l).is_err());
    }
    assert!(release_from_json(br#"{"x":[[[[[[0]]]]]]}"#, &Limits { max_depth: 2, ..l }).is_err());
}

#[test]
fn duplicates_missing_descriptors_and_declared_byte_overflow_fail() {
    let l = Limits::default();
    let (r, _) = release("org.example.a", &declarations()[..1], &[]);
    let mut duplicate = r.clone();
    duplicate
        .0
        .spec
        .contributions
        .push(duplicate.0.spec.contributions[0].clone());
    assert!(duplicate.validate(&l).is_err());
    let mut duplicate = r.clone();
    duplicate
        .0
        .spec
        .artifacts
        .push(duplicate.0.spec.artifacts[0].clone());
    assert!(duplicate.validate(&l).is_err());
    let mut missing = r.clone();
    missing.0.spec.artifacts.clear();
    assert_eq!(
        missing.validate(&l).unwrap_err().code,
        ErrorCode::InvalidSelection
    );
    assert_eq!(
        r.validate(&Limits {
            max_declared_bytes: 1,
            ..l
        })
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        r.validate(&l)
            .unwrap()
            .verify_all(&BTreeMap::new())
            .unwrap_err()
            .code,
        ErrorCode::InvalidSelection
    );
}

#[test]
fn identifier_grammar_and_domain_separation() {
    for s in [
        "",
        "A.a",
        "a..b",
        ".a",
        "a_",
        "a/b",
        "a.b.c.d.e.f.g.h.i",
        "é",
    ] {
        assert!(PluginId::new(s).is_err());
    }
    assert!(LocalContributionId::new(&"a".repeat(65)).is_err());
    assert!(ExtensionPointId::new("orishu.model.fields/v01").is_err());
    assert!(ExtensionPointId::new("orishu.model.fields/v2").is_ok());
    assert!("sha256:ABC".parse::<PluginReleaseId>().is_err());
}

#[test]
fn direct_values_cannot_bypass_tight_limits_or_expression_syntax() {
    let l = Limits::default();
    let (r, _) = release("org.example.a", &[], &[]);
    assert_eq!(
        r.validate(&Limits {
            max_manifest_bytes: 1,
            ..l
        })
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        r.validate(&Limits { max_values: 1, ..l }).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
    let mut p = declarations().remove(0).1;
    assert_eq!(
        p.validate(&Limits {
            max_payload_bytes: 1,
            ..l
        })
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    if let Payload::Components(d) = &mut p
        && let PropertyType::Quantity {
            default_expression, ..
        } = &mut d.scientific.properties[0].schema
    {
        *default_expression = Some("1 + )".into());
    }
    assert_eq!(p.validate(&l).unwrap_err().code, ErrorCode::Malformed);
}

#[test]
fn scientific_dependencies_defaults_and_order_are_identity_bearing() {
    let l = Limits::default();
    let mut p = declarations().remove(0).1;
    let before = p.contract_ref(&l).unwrap();
    if let Payload::Components(d) = &mut p {
        if let PropertyType::Quantity {
            default_expression, ..
        } = &mut d.scientific.properties[0].schema
        {
            *default_expression = Some("2 kg".into());
        }
        d.scientific.properties.push(Property {
            id: "label".parse().unwrap(),
            required: false,
            schema: PropertyType::Text {
                max_bytes: 12,
                default: None,
            },
        });
    }
    assert_ne!(p.contract_ref(&l).unwrap(), before);
    let before = p.contract_ref(&l).unwrap();
    if let Payload::Components(d) = &mut p {
        d.scientific.properties.reverse();
    }
    assert_ne!(p.contract_ref(&l).unwrap(), before);
    let mut p = declarations().remove(5).1;
    let before = p.contract_ref(&l).unwrap();
    if let Payload::FieldModels(d) = &mut p {
        d.scientific.requirements[0].contract.version = 2.try_into().unwrap();
    }
    assert_ne!(p.contract_ref(&l).unwrap(), before);
}

#[test]
fn one_artifact_cannot_declare_two_execution_contracts() {
    let l = Limits::default();
    let mut items = declarations();
    let kernel = items[5].1.kernel().unwrap();
    if let Payload::Integrators(d) = &mut items[6].1 {
        d.scientific.kernel = kernel;
    }
    let (r, blobs) = release("org.example.a", &items[5..], &[b"classical-kernel-fixture"]);
    let error = r
        .validate(&l)
        .unwrap()
        .verify_all(&borrowed(&blobs))
        .unwrap_err();
    assert_eq!(error.path, "kernel");
}

#[test]
fn noncanonical_set_order_and_float_spelling_are_not_alternate_identities() {
    use orishu_workload::canonical::{self, CanonicalMap, CanonicalValue as V};
    let l = Limits::default();
    let (r, _) = release("org.example.a", &declarations()[..2], &[]);
    let limits = orishu_workload::Limits::DEFAULT;
    let V::Map(root) = canonical::decode(&r.canonical_bytes(&l).unwrap(), &limits).unwrap() else {
        panic!()
    };
    let mut root = root.entries().to_vec();
    let (_, V::Map(spec)) = root
        .iter_mut()
        .find(|(k, _)| *k == V::text("spec"))
        .unwrap()
    else {
        panic!()
    };
    let mut fields = spec.entries().to_vec();
    let (_, V::Array(contributions)) = fields
        .iter_mut()
        .find(|(k, _)| *k == V::text("contributions"))
        .unwrap()
    else {
        panic!()
    };
    contributions.reverse();
    *spec = CanonicalMap::new(fields).unwrap();
    let encoded = canonical::encode(&V::Map(CanonicalMap::new(root).unwrap()), &limits).unwrap();
    assert!(release_from_cbor(&encoded, &l).is_err());
}
