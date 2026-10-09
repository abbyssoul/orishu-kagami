use orishu_plugin::{authoring_lock::*, resolution::*, *};
use serde_json::json;

fn reference(n: usize) -> ContributionRef {
    ContributionRef {
        release: format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
        extension_point: KnownPoint::Components.as_str().parse().unwrap(),
        local_id: format!("component-{n:03}").parse().unwrap(),
    }
}
fn binding(from: usize, to: usize) -> ProviderBinding {
    ProviderBinding {
        requirement: RequirementKey {
            consumer: reference(from),
            slot: format!("slot-{to:03}").parse().unwrap(),
        },
        provider: reference(to),
    }
}
fn graph(count: usize, roots: &[usize], edges: &[(usize, usize)]) -> Selection {
    let mut bindings: Vec<_> = edges.iter().map(|&(a, b)| binding(a, b)).collect();
    bindings.sort_by(|a, b| a.requirement.cmp(&b.requirement));
    Selection {
        roots: roots.iter().map(|&n| reference(n)).collect(),
        contributions: (0..count).map(reference).collect(),
        bindings,
    }
}
fn refuse(selection: Selection) {
    assert_eq!(
        SelectionLock::new(selection, LockLimits::default())
            .unwrap_err()
            .code,
        ErrorCode::InvalidSelection
    );
}

#[test]
fn scope_copies_exact_reachable_closure_without_inventing_roots_or_choices() {
    let limits = LockLimits::default();
    let original =
        SelectionLock::new(graph(5, &[0, 1], &[(0, 2), (1, 2), (1, 3), (2, 4)]), limits).unwrap();
    let scoped = original.for_roots(&[reference(0)], limits).unwrap();
    assert_eq!(
        scoped.selection().contributions,
        vec![reference(0), reference(2), reference(4)]
    );
    assert_eq!(
        scoped.selection().bindings,
        vec![binding(0, 2), binding(2, 4)]
    );
    let other = original.for_roots(&[reference(1)], limits).unwrap();
    assert_eq!(scoped.merge(&other, limits).unwrap(), original);
    assert_eq!(other.merge(&scoped, limits).unwrap(), original);
    assert_eq!(scoped.merge(&scoped, limits).unwrap(), scoped);
    assert!(
        original
            .for_roots(&[], limits)
            .unwrap()
            .selection()
            .contributions
            .is_empty()
    );
    assert_eq!(
        original
            .for_roots(&[reference(2)], limits)
            .unwrap()
            .selection()
            .contributions,
        vec![reference(2), reference(4)]
    );
    for bad in [
        vec![reference(5)],
        vec![reference(1), reference(0)],
        vec![reference(0), reference(0)],
    ] {
        assert!(original.for_roots(&bad, limits).is_err());
    }
    assert_eq!(original.selection().contributions.len(), 5);
}

#[test]
fn merge_refuses_conflicts_and_missing_edges_for_common_consumers() {
    let limits = LockLimits::default();
    let left = SelectionLock::new(graph(2, &[0], &[(0, 1)]), limits).unwrap();
    let missing = SelectionLock::new(graph(1, &[0], &[]), limits).unwrap();
    let CompositionError::Conflict(conflict) = left.merge(&missing, limits).unwrap_err() else {
        panic!("expected conflict")
    };
    assert_eq!(conflict.requirement, binding(0, 1).requirement);
    assert_eq!(conflict.existing, Some(reference(1)));
    assert_eq!(conflict.incoming, None);
    let CompositionError::Conflict(conflict) = missing.merge(&left, limits).unwrap_err() else {
        panic!("expected reverse conflict")
    };
    assert_eq!(conflict.existing, None);
    assert_eq!(conflict.incoming, Some(reference(1)));
    let mut different = left.selection().clone();
    different.contributions[1] = reference(2);
    different.bindings[0].provider = reference(2);
    let different = SelectionLock::new(different, limits).unwrap();
    let CompositionError::Conflict(conflict) = left.merge(&different, limits).unwrap_err() else {
        panic!("expected provider conflict")
    };
    assert_eq!(conflict.existing, Some(reference(1)));
    assert_eq!(conflict.incoming, Some(reference(2)));
}

#[test]
fn composition_rechecks_receiving_and_union_limits() {
    let limits = LockLimits::default();
    let whole = SelectionLock::new(graph(2, &[0, 1], &[]), limits).unwrap();
    let a = whole.for_roots(&[reference(0)], limits).unwrap();
    let b = whole.for_roots(&[reference(1)], limits).unwrap();
    for small in [
        LockLimits { items: 1, ..limits },
        LockLimits { work: 4, ..limits },
        LockLimits {
            bytes: a.canonical_byte_length(),
            ..limits
        },
    ] {
        assert!(a.merge(&b, small).is_err());
        assert!(whole.for_roots(&[reference(0)], small).is_err());
    }
}

#[test]
fn embedded_reader_stops_before_a_rejected_collection_entry() {
    let mut de = serde_json::Deserializer::from_str(
        r#"{"apiVersion":"orishu.plugin-authoring-lock/v1","selection":{"roots":[this is not a value"#,
    );
    let error = SelectionLock::deserialize_bounded(
        &mut de,
        LockLimits {
            items: 0,
            ..LockLimits::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("budget"), "{error}");
}

#[test]
fn exact_graph_roundtrips_without_runtime_or_inventory_state() {
    let original = graph(4, &[0], &[(0, 1), (0, 2), (1, 3), (2, 3)]);
    let limits = LockLimits::default();
    let lock = SelectionLock::new(original.clone(), limits).unwrap();
    let bytes = lock.to_cbor(limits).unwrap();
    let decoded = SelectionLock::from_cbor(&bytes, limits).unwrap();
    assert_eq!(decoded, lock);
    let independent: serde_json::Value = ciborium::from_reader(bytes.as_slice()).unwrap();
    let expected = json!({"apiVersion": LOCK_SCHEMA, "selection": original});
    assert_eq!(independent, expected);
    let pretty = serde_json::to_vec_pretty(&expected).unwrap();
    assert_eq!(SelectionLock::from_json(&pretty, limits).unwrap(), lock);
    assert_eq!(
        SelectionLock::from_json(&lock.to_json(limits).unwrap(), limits).unwrap(),
        lock
    );
    assert_eq!(lock.request(42).expected_inventory_revision, 42);
    assert_eq!(lock.request(99).bindings, original.bindings);
    assert_eq!(lock.selection(), &original);
    assert_eq!(independent.as_object().unwrap().len(), 2);
    assert_eq!(independent["selection"].as_object().unwrap().len(), 3);
}

#[test]
fn empty_graph_has_independently_written_canonical_fixture() {
    // Map ordering is encoded-key byte order, not Rust/JSON field order.
    const GOLDEN: &[u8] = b"\xa2\x69selection\xa3\x65roots\x80\x68bindings\x80\x6dcontributions\x80\x6aapiVersion\x78\x1forishu.plugin-authoring-lock/v1";
    let limits = LockLimits::default();
    let lock = SelectionLock::new(graph(0, &[], &[]), limits).unwrap();
    assert_eq!(lock.to_cbor(limits).unwrap(), GOLDEN);
    assert_eq!(SelectionLock::from_cbor(GOLDEN, limits).unwrap(), lock);
    let mut trailing = GOLDEN.to_vec();
    trailing.push(0);
    assert!(SelectionLock::from_cbor(&trailing, limits).is_err());
    // Indefinite-length root map is not the canonical spelling.
    let mut indefinite = GOLDEN.to_vec();
    indefinite[0] = 0xbf;
    indefinite.push(0xff);
    assert!(SelectionLock::from_cbor(&indefinite, limits).is_err());
}

#[test]
fn rejects_noncanonical_sets_foreign_edges_cycles_and_unreachable_members() {
    refuse(graph(2, &[1, 0], &[]));
    refuse(graph(1, &[0, 0], &[]));
    let mut duplicate = graph(1, &[0], &[]);
    duplicate.contributions.push(reference(0));
    refuse(duplicate);
    let mut reversed = graph(2, &[0, 1], &[]);
    reversed.contributions.reverse();
    refuse(reversed);
    let mut duplicate = graph(2, &[0], &[(0, 1)]);
    duplicate.bindings.push(duplicate.bindings[0].clone());
    refuse(duplicate);
    let mut reversed = graph(3, &[0], &[(0, 1), (0, 2)]);
    reversed.bindings.reverse();
    refuse(reversed);
    refuse(graph(1, &[1], &[]));
    refuse(graph(2, &[0], &[(0, 2)]));
    refuse(graph(2, &[0], &[(2, 1)]));
    refuse(graph(2, &[0], &[]));
    refuse(graph(1, &[], &[]));
    refuse(graph(1, &[0], &[(0, 0)]));
    refuse(graph(2, &[0], &[(0, 1), (1, 0)]));
    refuse(graph(3, &[0], &[(1, 2), (2, 1)]));
}

#[test]
fn limits_apply_at_each_boundary_including_longest_shared_dependency_path() {
    let limits = LockLimits::default();
    // Node 3 is also a root: this must not hide the longer 0 -> 1 -> 3 path.
    let lock =
        SelectionLock::new(graph(4, &[0, 3], &[(0, 1), (0, 2), (1, 3), (2, 3)]), limits).unwrap();
    for small in [
        LockLimits { items: 3, ..limits },
        LockLimits { work: 25, ..limits }, // 3*(4+4)+2 = 26
        LockLimits {
            dependency_depth: 2,
            ..limits
        },
        LockLimits { bytes: 1, ..limits },
    ] {
        assert_eq!(
            lock.to_cbor(small).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
        assert_eq!(
            lock.to_json(small).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
        assert_eq!(
            SelectionLock::from_json(&lock.to_json(limits).unwrap(), small)
                .unwrap_err()
                .code,
            ErrorCode::LimitExceeded
        );
        assert_eq!(
            SelectionLock::from_cbor(&lock.to_cbor(limits).unwrap(), small)
                .unwrap_err()
                .code,
            ErrorCode::LimitExceeded
        );
    }
    let permissive = LockLimits {
        dependency_depth: usize::MAX,
        ..limits
    };
    let edges: Vec<_> = (0..63).map(|n| (n, n + 1)).collect();
    SelectionLock::new(graph(64, &[0], &edges), permissive).unwrap();
    let edges: Vec<_> = (0..64).map(|n| (n, n + 1)).collect();
    assert_eq!(
        SelectionLock::new(graph(65, &[0], &edges), permissive)
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    let reverse_chain = SelectionLock::new(graph(3, &[2], &[(2, 1), (1, 0)]), limits).unwrap();
    reverse_chain.validate(limits).unwrap();
}

#[test]
fn json_refuses_excess_entry_before_reading_its_malformed_body() {
    let limits = LockLimits {
        items: 1,
        ..LockLimits::default()
    };
    for (key, first) in [
        ("roots", serde_json::to_string(&reference(0)).unwrap()),
        (
            "contributions",
            serde_json::to_string(&reference(0)).unwrap(),
        ),
        ("bindings", serde_json::to_string(&binding(0, 1)).unwrap()),
    ] {
        let malformed =
            format!(r#"{{"apiVersion":"{LOCK_SCHEMA}","selection":{{"{key}":[{first},{{malformed"#);
        assert_eq!(
            SelectionLock::from_json(malformed.as_bytes(), limits)
                .unwrap_err()
                .code,
            ErrorCode::LimitExceeded
        );
    }
}

#[test]
fn json_rejects_unknown_duplicate_null_version_and_trailing_data() {
    let limits = LockLimits::default();
    let valid = json!({"apiVersion": LOCK_SCHEMA, "selection": graph(1, &[0], &[])});
    for path in ["envelope", "selection", "reference"] {
        let mut value = valid.clone();
        let object = match path {
            "envelope" => &mut value,
            "selection" => &mut value["selection"],
            _ => &mut value["selection"]["roots"][0],
        };
        object["unknown"] = json!(1);
        assert!(SelectionLock::from_json(&serde_json::to_vec(&value).unwrap(), limits).is_err());
    }
    let mut wrong_version = valid.clone();
    wrong_version["apiVersion"] = json!("orishu.plugin-authoring-lock/v2");
    assert_eq!(
        SelectionLock::from_json(&serde_json::to_vec(&wrong_version).unwrap(), limits)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
    let mut null = valid.clone();
    null["selection"]["bindings"] = serde_json::Value::Null;
    assert!(SelectionLock::from_json(&serde_json::to_vec(&null).unwrap(), limits).is_err());
    let text = serde_json::to_string(&valid).unwrap();
    assert!(SelectionLock::from_json(format!("{text} {{}}").as_bytes(), limits).is_err());
    let duplicate = text.replacen("{", &format!(r#"{{"apiVersion":"{LOCK_SCHEMA}","#), 1);
    assert!(SelectionLock::from_json(duplicate.as_bytes(), limits).is_err());
}

#[test]
fn checked_in_schema_has_versioned_shape_and_only_local_reference_targets() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/authoring-lock-v1.schema.json")).unwrap();
    let declarations: serde_json::Value =
        serde_json::from_str(include_str!("../schema/plugin-v1.schema.json")).unwrap();
    assert_eq!(schema["properties"]["apiVersion"]["const"], LOCK_SCHEMA);
    assert_eq!(schema["required"], json!(["apiVersion", "selection"]));
    let selection = &schema["properties"]["selection"];
    assert_eq!(
        selection["required"],
        json!(["roots", "contributions", "bindings"])
    );
    let properties = &selection["properties"];
    let binding = &properties["bindings"]["items"];
    let requirement = &binding["properties"]["requirement"];
    for object in [&schema, selection, binding, requirement] {
        assert_eq!(object["additionalProperties"], false);
    }
    for target in [
        &properties["roots"]["items"],
        &properties["contributions"]["items"],
        &binding["properties"]["provider"],
        &requirement["properties"]["consumer"],
        &requirement["properties"]["slot"],
    ] {
        let path = target["$ref"]
            .as_str()
            .unwrap()
            .strip_prefix("plugin-v1.schema.json#")
            .unwrap();
        assert!(declarations.pointer(path).is_some());
    }
}
