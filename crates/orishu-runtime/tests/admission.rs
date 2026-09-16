//! Real selected releases -> canonical workload -> independent runtime admission.
use orishu_plugin::{execution::*, resolution::*, selected, workload, *};
use orishu_runtime::*;
use std::{collections::BTreeSet, sync::Arc};
#[path = "reference_support/admission.rs"]
mod admission_fixture;
mod reference_support;
use admission_fixture::*;
#[test]
fn no_field_profile_exports_only_integrator_and_drifts_without_fabricated_force_input() {
    let mut fixture = Fixture::new();
    fixture.execution.fields.clear();
    let euler = fixture.releases[1]
        .contribution_ref(&"euler".parse().unwrap())
        .unwrap();
    let entries: Vec<_> = fixture
        .releases
        .iter()
        .map(|release| InventoryEntry {
            release,
            enabled: true,
            is_default: true,
        })
        .collect();
    let inventory = Inventory::new(1, &entries, Default::default()).unwrap();
    let ResolutionOutcome::Resolved { selection, .. } = inventory
        .resolve(&ResolutionRequest {
            expected_inventory_revision: 1,
            roots: vec![euler.clone()],
            bindings: vec![],
        })
        .unwrap()
    else {
        panic!("integrator resolves")
    };
    fixture.selection = selected::compile(
        &selection,
        &[selected::SelectedKernel {
            instance_id: "euler".parse().unwrap(),
            contribution: euler,
            execution_contract: ExecutionContractId::Dynamics,
        }],
        &fixture.releases.iter().collect::<Vec<_>>(),
        &borrowed(&fixture.blobs),
        Default::default(),
    )
    .unwrap();
    let mut objects: Vec<_> = Batch::<ObjectState>::read(
        &fixture.blobs[&fixture.execution.objects.digest],
        BulkLimits::default(),
    )
    .unwrap()
    .iter()
    .collect();
    objects[1].kinematics.velocity_metres_per_second[0] = n(1.0);
    fixture.execution.objects = put(&mut fixture.blobs, &packet(&objects));
    let compiled = fixture.compile();
    let exported = fixture.export(&compiled);
    assert!(!exported.contains_key(&ArtifactDigest::sha256_of(GRAVITY)));
    let host = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    let mut admitted = admit(
        host,
        compiled.manifest_bytes(),
        &borrowed(&exported),
        scope(&compiled),
        &BTreeSet::new(),
        Default::default(),
        control(),
    )
    .unwrap();
    admitted.run.advance(control()).unwrap();
    let objects: Vec<_> =
        Batch::<ObjectState>::read(&admitted.run.state().objects().bytes, BulkLimits::default())
            .unwrap()
            .iter()
            .collect();
    assert_eq!(objects[1].kinematics.position_metres[0], n(2.5));
}
fn scope(compiled: &workload::CompiledWorkload) -> RunScope {
    RunScope {
        workload: compiled.verified().root(),
        run: ArtifactDigest::sha256_of(b"server-owned test run descriptor"),
        epoch: 1,
    }
}
#[test]
fn actual_selected_workload_runs_without_plugins_and_preserves_captured_state() {
    let fixture = Fixture::new();
    let compiled = fixture.compile();
    let exported = fixture.export(&compiled);
    assert!(!exported.contains_key(&ArtifactDigest::sha256_of(
        b"unused native-looking executable"
    )));
    let host = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    let mut admitted = admit(
        host,
        compiled.manifest_bytes(),
        &borrowed(&exported),
        scope(&compiled),
        &BTreeSet::new(),
        Default::default(),
        control(),
    )
    .unwrap();
    assert_eq!(
        admitted.run.state().fields()[0].bytes.as_ref(),
        fixture.blobs[&fixture.execution.fields[0].kernel.state.digest]
    );
    admitted.run.advance(control()).unwrap();
    let objects: Vec<_> =
        Batch::<ObjectState>::read(&admitted.run.state().objects().bytes, BulkLimits::default())
            .unwrap()
            .iter()
            .collect();
    assert_eq!(objects[0].kinematics.position_metres[0], n(0.0));
    assert_eq!(
        objects[1].kinematics.velocity_metres_per_second[0],
        n(-0.1875)
    );
    assert_eq!(objects[1].kinematics.position_metres[0], n(1.90625));
    assert_eq!(admitted.run.state().boundary(), 1);
    assert_eq!(admitted.workload.root(), compiled.verified().root());
}
#[test]
fn profile_refuses_tampered_graph_extra_or_missing_blobs_and_denied_resources() {
    let fixture = Fixture::new();
    let compiled = fixture.compile();
    let exported = fixture.export(&compiled);
    let m = compiled.verified().manifest();
    for case in 0..4 {
        let mut m = m.clone();
        match case {
            0 => m
                .spec
                .compute
                .step_plan
                .invocations
                .last_mut()
                .unwrap()
                .depends_on
                .clear(),
            1 => m.spec.compute.components[0].model_id = "other".parse().unwrap(),
            2 => m.spec.artifacts.push(m.spec.artifacts[0].clone()),
            _ => m.spec.compute.workload_graph_profile = "other/v1".parse().unwrap(),
        }
        assert!(workload::verify(m, &borrowed(&exported), Default::default()).is_err());
    }
    for digest in exported.keys() {
        let mut missing = borrowed(&exported);
        missing.remove(digest);
        assert!(workload::verify(m.clone(), &missing, Default::default()).is_err());
    }
    let host = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    let denied = BTreeSet::from([ArtifactDigest::sha256_of(GRAVITY)]);
    assert!(
        admit(
            host.clone(),
            compiled.manifest_bytes(),
            &borrowed(&exported),
            scope(&compiled),
            &denied,
            Default::default(),
            control()
        )
        .is_err()
    );
    assert!(
        admit(
            host,
            compiled.manifest_bytes(),
            &borrowed(&exported),
            scope(&compiled),
            &BTreeSet::new(),
            AdmissionLimits {
                executable_bytes: 1,
                ..Default::default()
            },
            control()
        )
        .is_err()
    );
}
#[test]
fn captured_opaque_state_is_numerically_validated_not_reinitialized_at_admission() {
    let mut fixture = Fixture::new();
    let state = &mut fixture.execution.fields[0].kernel.state;
    let mut invalid = fixture.blobs[&state.digest].clone();
    invalid[0] ^= 255;
    state.digest = ArtifactDigest::sha256_of(&invalid);
    fixture.blobs.insert(state.digest, invalid);
    let compiled = fixture.compile(); // valid opaque bytes/closure, not valid physics
    let exported = fixture.export(&compiled);
    let host = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    assert!(
        admit(
            host,
            compiled.manifest_bytes(),
            &borrowed(&exported),
            scope(&compiled),
            &BTreeSet::new(),
            Default::default(),
            control()
        )
        .is_err()
    );
}

#[test]
fn scientific_capture_must_match_selected_context_objects_and_independent_budgets() {
    let fixture = Fixture::new();
    // Change content-addressed inputs together with their digests. These are
    // scientific inconsistencies, not merely a corrupt-byte/hash test.
    for case in 0..7 {
        let mut execution = fixture.execution.clone();
        let mut blobs = fixture.blobs.clone();
        let field = &mut execution.fields[0];
        match case {
            0..=3 => {
                let mut coupled: Vec<_> = Batch::<CoupledEntity>::read(
                    &blobs[&field.coupled.digest],
                    BulkLimits::default(),
                )
                .unwrap()
                .iter()
                .collect();
                match case {
                    0 => coupled[1].kinematics.position_metres[0] = n(99.0),
                    1 => coupled[1].has_dynamics = false,
                    2 => coupled[1].slot = CouplingSlot(1),
                    _ => coupled[1].source_si = None,
                }
                field.coupled = put(&mut blobs, &packet(&coupled));
            }
            4 => execution.objects.value_count += 1,
            5 => field.kernel.state.schema = "other.state/v1".parse().unwrap(),
            _ => {
                let mut context =
                    InstanceContext::from_cbor(&blobs[&field.kernel.context.digest]).unwrap();
                context.bounds.projection_records = 1;
                field.kernel.context = put(
                    &mut blobs,
                    &Buffer {
                        schema: INSTANCE_SCHEMA.into(),
                        value_count: 1,
                        bytes: context.to_cbor().unwrap().into(),
                    },
                );
            }
        }
        assert!(
            workload::compile(
                orishu_workload::WorkloadMeta::new("invalid-capture".parse().unwrap()),
                &fixture.selection,
                &execution,
                &borrowed(&blobs),
                Default::default(),
            )
            .is_err(),
            "scientific mutation {case} accepted"
        );
    }
    let compiled = fixture.compile();
    let exported = fixture.export(&compiled);
    for case in 0..5 {
        let mut limits = workload::ProfileLimits::default();
        match case {
            0 => limits.fields = 0,
            1 => limits.objects = 1,
            2 => limits.couplings = 1,
            3 => limits.input_bytes = 1,
            _ => limits.state_bytes = 1,
        }
        assert!(
            workload::verify(
                compiled.verified().manifest().clone(),
                &borrowed(&exported),
                limits,
            )
            .is_err(),
            "budget {case} ignored"
        );
    }
}

#[test]
fn opaque_initialization_uses_kernel_size_not_host_capacity_and_rejects_excess() {
    let fixture = Fixture::new();
    let context = InstanceContext::from_cbor(
        &fixture.blobs[&fixture.execution.fields[0].kernel.context.digest],
    )
    .unwrap();
    let get = |id: &InputIdentity| Buffer {
        schema: id.schema.to_string(),
        value_count: id.value_count,
        bytes: fixture.blobs[&id.digest].clone().into(),
    };
    let host = Sandbox::new(SandboxLimits::default()).unwrap();
    let kernel = host
        .compile(GRAVITY, context.kernel, ExecutionContractId::Field)
        .unwrap();
    let schema = format!(
        "{}/v{}",
        context.state_format.id, context.state_format.version
    );
    let original = get(&fixture.execution.fields[0].kernel.state);
    for (bytes, values) in [(8192, 1024), (1024 * 1024, 2048)] {
        let FieldResult::State(captured) = host
            .invoke_field_bound(
                &kernel,
                &context,
                get(&context.configuration),
                FieldOperation::InitializeBounded {
                    domain: get(&context.domain),
                    output: OutputCapacity {
                        schema: &schema,
                        bytes,
                        values,
                    },
                },
                control(),
            )
            .unwrap()
        else {
            panic!("captured field")
        };
        assert_eq!(captured.bytes, original.bytes);
        assert_eq!(captured.value_count, original.value_count);
    }
    for (bytes, values, schema) in [
        (original.bytes.len() - 1, 1024, schema.as_str()),
        (8192, 0, schema.as_str()),
        (8192, 1024, "wrong.state/v1"),
        (
            context.bounds.state_bytes as usize + 1,
            1024,
            schema.as_str(),
        ),
    ] {
        assert!(
            host.invoke_field_bound(
                &kernel,
                &context,
                get(&context.configuration),
                FieldOperation::InitializeBounded {
                    domain: get(&context.domain),
                    output: OutputCapacity {
                        schema,
                        bytes,
                        values
                    },
                },
                control(),
            )
            .is_err()
        );
    }
}
