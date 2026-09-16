use orishu_workload::{v3, *};
use std::{cell::Cell, collections::BTreeMap};

fn artifact(role: &str, content: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor {
        role: role.parse().unwrap(),
        digest: ArtifactDigest::sha256_of(content),
        size_bytes: content.len() as u64,
        media_type: "application/octet-stream".parse().unwrap(),
        schema: None,
    }
}
fn fixture() -> (v3::WorkloadManifest, InMemoryBlobs) {
    let mut blobs = InMemoryBlobs::new();
    for value in [b"code".as_slice(), b"selection", b"execution", b"state"] {
        blobs.insert(value);
    }
    let component = ComponentInstance {
        instance_id: "kernel".parse().unwrap(),
        artifact: artifact("component", b"code"),
        plugin_id: "org.example.plugin".parse().unwrap(),
        model_id: "model".parse().unwrap(),
        schema_id: "state/v1".parse().unwrap(),
        engine: "wasm-component".parse().unwrap(),
        lifecycle: "orishu:simulation/field@1".parse().unwrap(),
        roles: vec![],
        state_ownership: vec![],
        config: BTreeMap::new(),
        limits: BTreeMap::new(),
    };
    let compute = ComputeSpec {
        workload_graph_profile: "orishu.force-then-integrate/v1".parse().unwrap(),
        components: vec![component],
        channels: vec![],
        placement_constraints: vec![],
        step_plan: StepPlan {
            profile: "orishu.force-then-integrate/v1".parse().unwrap(),
            invocations: vec![StepInvocation {
                invocation_id: "advance".parse().unwrap(),
                instance: "kernel".parse().unwrap(),
                phase_id: "advance".parse().unwrap(),
                inputs: vec![],
                outputs: vec![],
                depends_on: vec![],
            }],
        },
    };
    (
        v3::manifest(
            WorkloadMeta::new("example".parse().unwrap()),
            v3::WorkloadSpec {
                compute,
                selection: artifact(v3::SELECTION_ROLE, b"selection"),
                execution: artifact(v3::EXECUTION_ROLE, b"execution"),
                artifacts: vec![artifact("initial-conditions", b"state")],
                requirements: WorkloadRequirements::default(),
            },
        ),
        blobs,
    )
}
struct Unreachable(Cell<usize>);
impl BlobSource for Unreachable {
    fn read_blob(&self, _: &ArtifactDigest, _: &mut BlobVerifier<'_>) -> bool {
        self.0.set(self.0.get() + 1);
        false
    }
}
#[test]
fn v3_roundtrip_closure_and_identity_without_fabricated_global_domain() {
    let (m, blobs) = fixture();
    let limits = Limits::DEFAULT;
    let bytes = v3::canonical_bytes(&m, &limits).unwrap();
    assert_eq!(v3::from_canonical_bytes(&bytes, &limits).unwrap(), m);
    let json = serde_json::to_string(&m).unwrap();
    assert_eq!(
        serde_json::from_str::<v3::WorkloadManifest>(&json).unwrap(),
        m
    );
    assert!(!json.contains("spaceMetres"));
    assert!(!json.contains("domain"));
    let closure = v3::validate_closure(&m, &blobs, &limits).unwrap();
    assert_eq!(closure.root(), v3::workload_digest(&m, &limits).unwrap());
    assert_eq!(closure.len(), 4);
    // Reviewed v3 projection; changing this is a format/fixture decision.
    assert_eq!(
        closure.root().to_string(),
        "sha256:3b0a022aa73c4bc422cdcbbc89a951d78005ae3d0f65d4abf4c36622d145e9ad"
    );
    for change in ["selection", "execution", "graph"] {
        let mut other = m.clone();
        match change {
            "selection" => other.spec.selection = artifact(v3::SELECTION_ROLE, b"different"),
            "execution" => other.spec.execution = artifact(v3::EXECUTION_ROLE, b"different"),
            _ => other.spec.compute.components[0].model_id = "different".parse().unwrap(),
        }
        assert_ne!(
            v3::workload_digest(&other, &limits).unwrap(),
            closure.root()
        );
    }
}
#[test]
fn v2_and_v3_readers_do_not_claim_each_others_formats() {
    let limits = Limits::DEFAULT;
    let (m, _) = fixture();
    let bytes = v3::canonical_bytes(&m, &limits).unwrap();
    assert!(canonical::manifest_from_canonical_bytes(&bytes, &limits).is_err());
    let old =
        authoring::parse_str(include_str!("fixtures/minimal.workload.yaml"), &limits).unwrap();
    let old_bytes = canonical::canonical_bytes(&old, &limits).unwrap();
    assert!(v3::from_canonical_bytes(&old_bytes, &limits).is_err());
    let mut tree = canonical::decode(&bytes, &limits).unwrap();
    let CanonicalValue::Map(map) = tree else {
        unreachable!()
    };
    let mut entries = map.entries().to_vec();
    entries
        .iter_mut()
        .find(|(k, _)| *k == CanonicalValue::text("apiVersion"))
        .unwrap()
        .1 = CanonicalValue::text("orishu.dev/v2");
    tree = CanonicalValue::Map(CanonicalMap::new(entries).unwrap());
    assert!(
        v3::from_canonical_bytes(&canonical::encode(&tree, &limits).unwrap(), &limits).is_err()
    );
}
#[test]
fn malformed_graph_roles_and_bounds_never_reach_blob_source() {
    let (m, _) = fixture();
    let limits = Limits::DEFAULT;
    for case in 0..6 {
        let mut m = m.clone();
        match case {
            0 => m.spec.selection.role = "component".parse().unwrap(),
            1 => m.spec.artifacts.push(m.spec.execution.clone()),
            2 => m.spec.compute.step_plan.invocations[0].instance = "absent".parse().unwrap(),
            3 => m.spec.compute.step_plan.profile = "other/v1".parse().unwrap(),
            4 => m.spec.artifacts[0].size_bytes = limits.max_artifact_bytes + 1,
            _ => m
                .spec
                .compute
                .components
                .push(m.spec.compute.components[0].clone()),
        }
        let source = Unreachable(Cell::new(0));
        assert!(v3::validate_closure(&m, &source, &limits).is_err());
        assert_eq!(source.0.get(), 0);
    }
    for limits in [
        Limits {
            max_artifacts: 3,
            ..limits
        },
        Limits {
            max_components: 0,
            ..limits
        },
        Limits {
            max_manifest_bytes: 1,
            ..limits
        },
        Limits {
            max_aggregate_declared_bytes: 1,
            ..limits
        },
    ] {
        let source = Unreachable(Cell::new(0));
        assert!(v3::validate_closure(&m, &source, &limits).is_err());
        assert_eq!(source.0.get(), 0);
    }
}
#[test]
fn truncation_unknown_fields_typed_collection_limits_and_corruption_fail() {
    let (m, _) = fixture();
    let limits = Limits::DEFAULT;
    let bytes = v3::canonical_bytes(&m, &limits).unwrap();
    for end in 0..bytes.len() {
        assert!(v3::from_canonical_bytes(&bytes[..end], &limits).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(v3::from_canonical_bytes(&trailing, &limits).is_err());
    assert!(
        v3::from_canonical_bytes(
            &bytes,
            &Limits {
                max_components: 0,
                ..limits
            }
        )
        .is_err()
    );
    assert!(
        v3::from_canonical_bytes(
            &bytes,
            &Limits {
                max_artifacts: 0,
                ..limits
            }
        )
        .is_err()
    );
    let json = serde_json::to_string(&m)
        .unwrap()
        .replacen("{", "{\"status\":{},", 1);
    assert!(serde_json::from_str::<v3::WorkloadManifest>(&json).is_err());
    assert!(v3::validate_closure(&m, &InMemoryBlobs::new(), &limits).is_err());
    struct Wrong;
    impl BlobSource for Wrong {
        fn read_blob(&self, _: &ArtifactDigest, verifier: &mut BlobVerifier<'_>) -> bool {
            verifier.write(b"wrong");
            true
        }
    }
    assert!(v3::validate_closure(&m, &Wrong, &limits).is_err());
}

#[test]
fn aggregate_overflow_is_refused_even_when_policy_allows_u64_max() {
    let (mut m, _) = fixture();
    m.spec.selection.size_bytes = u64::MAX;
    let limits = Limits {
        max_artifact_bytes: u64::MAX,
        max_aggregate_declared_bytes: u64::MAX,
        ..Limits::DEFAULT
    };
    let source = Unreachable(Cell::new(0));
    let report = v3::validate_closure(&m, &source, &limits).unwrap_err();
    assert!(
        report
            .errors()
            .contains(&ClosureError::AggregateSizeOverflow)
    );
    assert_eq!(source.0.get(), 0);
}
