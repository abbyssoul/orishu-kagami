//! Whole-boundary evidence through real field and Dynamics Components. These
//! synthetic contribution pins are not selected-release/workload admission proof.
use orishu_plugin::{ArtifactDigest, Dimension, ExecutionContractId, FiniteF64, execution::*};
use orishu_runtime::*;
use std::{sync::Arc, time::Duration};
mod reference_support;
const GRAVITY: &[u8] = include_bytes!("fixtures/newtonian.component.wasm");
const EULER: &[u8] = include_bytes!("fixtures/euler.component.wasm");
fn n(v: f64) -> FiniteF64 {
    FiniteF64::new(v).unwrap()
}
fn control() -> OperationControl {
    OperationControl::new(Duration::from_secs(10)).unwrap()
}
fn packet<R: BulkRecord>(values: &[R]) -> Buffer {
    let mut bytes = vec![];
    encode_batch(values, &mut bytes, BulkLimits::default()).unwrap();
    Buffer {
        schema: R::SCHEMA.into(),
        value_count: values.len() as u64,
        bytes: bytes.into(),
    }
}
fn object(id: u64, x: f64, v: f64, mass: Option<f64>) -> ObjectState {
    ObjectState {
        id: EntityId(id),
        kinematics: Kinematics {
            position_metres: [n(x), n(0.0), n(0.0)],
            velocity_metres_per_second: [n(v), n(0.0), n(0.0)],
        },
        inertial_mass_kilograms: mass.map(n),
    }
}
fn objects() -> Vec<ObjectState> {
    vec![
        object(1, 0.0, 10.0, None),
        object(2, 2.0, 0.0, Some(4.0)),
        object(3, 9.0, 17.0, None),
    ]
}
fn scope() -> RunScope {
    RunScope {
        workload: ArtifactDigest::sha256_of(b"synthetic workload")
            .to_string()
            .parse()
            .unwrap(),
        run: ArtifactDigest::sha256_of(b"synthetic run"),
        epoch: 1,
    }
}
struct Fixture {
    host: Arc<Sandbox>,
    gravity: Arc<CompiledKernel>,
    euler: Arc<CompiledKernel>,
}
impl Fixture {
    fn new() -> Self {
        let host = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
        let gravity = Arc::new(
            host.compile(
                GRAVITY,
                ArtifactDigest::sha256_of(GRAVITY),
                ExecutionContractId::Field,
            )
            .unwrap(),
        );
        let euler = Arc::new(
            host.compile(
                EULER,
                ArtifactDigest::sha256_of(EULER),
                ExecutionContractId::Dynamics,
            )
            .unwrap(),
        );
        Self {
            host,
            gravity,
            euler,
        }
    }
    fn program(&self, objects: &[ObjectState], gs: &[f64]) -> RunProgram {
        let fields = gs
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let configuration = reference_support::resolved(vec![
                    (
                        "capacity",
                        ConfigurationValue::Quantity {
                            value_si: n(8.0),
                            dimension: Dimension::DIMENSIONLESS,
                        },
                    ),
                    (
                        "gravitational-constant",
                        ConfigurationValue::Quantity {
                            value_si: n(*g),
                            dimension: Dimension::new([3, -1, -2, 0, 0, 0, 0]),
                        },
                    ),
                    (
                        "exclusion-radius",
                        ConfigurationValue::Quantity {
                            value_si: n(0.0),
                            dimension: Dimension::LENGTH,
                        },
                    ),
                    (
                        "boundary",
                        ConfigurationValue::Text {
                            value: "isolated".into(),
                        },
                    ),
                ]);
                let mut context = reference_support::context(
                    GRAVITY,
                    ExecutionContractId::Field,
                    &configuration,
                    8,
                );
                context.instance = format!("field-{i}").parse().unwrap();
                let coupled = objects
                    .iter()
                    .take(2)
                    .enumerate()
                    .map(|(i, o)| CoupledEntity {
                        id: o.id,
                        slot: CouplingSlot(0),
                        kinematics: o.kinematics,
                        has_dynamics: o.inertial_mass_kilograms.is_some(),
                        source_si: (i == 0).then_some(n(2.0)),
                        response_si: (i == 1).then_some(n(3.0)),
                    })
                    .collect::<Vec<_>>();
                RunField {
                    binding: RunKernel {
                        kernel: self.gravity.clone(),
                        context,
                        configuration,
                        domain: reference_support::domain(),
                        state_extent: StateExtent {
                            schema: "org.orishu.reference.newtonian.state/v1".parse().unwrap(),
                            bytes: 400,
                            values: 1,
                        },
                    },
                    coupled: packet(&coupled),
                }
            })
            .collect();
        let configuration = reference_support::euler_config(8);
        let context =
            reference_support::context(EULER, ExecutionContractId::Dynamics, &configuration, 8);
        let count = objects.iter().filter(|o| o.dynamic().is_some()).count();
        RunProgram {
            fields,
            dynamics: RunKernel {
                kernel: self.euler.clone(),
                context,
                configuration,
                domain: reference_support::domain(),
                state_extent: StateExtent {
                    schema: "org.orishu.reference.euler.history/v1".parse().unwrap(),
                    bytes: 16 + 8 * count,
                    values: count as u64,
                },
            },
            timestep_seconds: n(0.5),
        }
    }
    // Explicit authoring capture, separate from the production run constructor.
    fn capture(&self, program: &RunProgram, objects: &[ObjectState]) -> CapturedRunState {
        let fields = program
            .fields
            .iter()
            .map(|f| {
                let b = &f.binding;
                match self
                    .host
                    .invoke_field_bound(
                        &b.kernel,
                        &b.context,
                        b.configuration.clone(),
                        FieldOperation::Initialize {
                            domain: b.domain.clone(),
                            output: OutputExtent {
                                schema: b.state_extent.schema.as_str(),
                                bytes: b.state_extent.bytes,
                                values: b.state_extent.values,
                            },
                        },
                        control(),
                    )
                    .unwrap()
                {
                    FieldResult::State(s) => s,
                    _ => panic!("field defaults"),
                }
            })
            .collect();
        let b = &program.dynamics;
        let dynamic = objects
            .iter()
            .filter_map(ObjectState::dynamic)
            .collect::<Vec<_>>();
        let history = match self
            .host
            .invoke_dynamics_bound(
                &b.kernel,
                &b.context,
                b.configuration.clone(),
                DynamicsOperation::InitializeHistory {
                    entities: packet(&dynamic),
                    history: OutputExtent {
                        schema: b.state_extent.schema.as_str(),
                        bytes: b.state_extent.bytes,
                        values: b.state_extent.values,
                    },
                },
                control(),
            )
            .unwrap()
        {
            DynamicsResult::History(s) => s,
            _ => panic!("history defaults"),
        };
        CapturedRunState {
            objects: packet(objects),
            fields,
            history,
        }
    }
    fn run(&self, objects: &[ObjectState], gs: &[f64]) -> FixedRun {
        let program = self.program(objects, gs);
        let captured = self.capture(&program, objects);
        FixedRun::from_captured(
            self.host.clone(),
            scope(),
            program,
            captured,
            RunLimits::default(),
            control(),
        )
        .unwrap()
    }
}
fn state_objects(state: &CommittedState) -> Vec<ObjectState> {
    state
        .objects()
        .scientific_batch::<ObjectState>(BulkLimits::default())
        .unwrap()
        .iter()
        .collect()
}

#[test]
fn host_commit_gate_sees_only_complete_candidates_and_acceptance_is_final() {
    let fixture = Fixture::new();
    let mut run = fixture.run(&objects(), &[1.0]);
    let prior = run.state().clone();
    let mut visits = 0;
    let error = run
        .advance_with_commit(control(), |identity, candidate| {
            visits += 1;
            assert_eq!(identity, &scope());
            assert_eq!(candidate.boundary(), 1);
            assert!(candidate.forces().is_some());
            assert_eq!(candidate.fields().len(), 1);
            Err(RunRejection::Stopped.into())
        })
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<RunRejection>(),
        Some(&RunRejection::Stopped)
    );
    assert_eq!(visits, 1);
    assert_eq!(
        error.downcast_ref::<RunFailure>().unwrap().phase,
        RunPhase::Publication
    );
    assert_eq!(run.state().boundary(), prior.boundary());
    assert_eq!(run.state().objects().bytes, prior.objects().bytes);
    assert_eq!(run.state().fields()[0].bytes, prior.fields()[0].bytes);
    assert_eq!(run.state().history().bytes, prior.history().bytes);

    let cancelled = control();
    run.advance_with_commit(cancelled.clone(), |_, _| {
        // Once the embedding authority accepts publication, a subsequent cancel
        // cannot roll its private executor back behind the published boundary.
        cancelled.cancel();
        Ok(())
    })
    .unwrap();
    assert_eq!(run.state().boundary(), 1);
    assert!(!run.is_stopped());
    let rejected = control();
    rejected.cancel();
    assert!(
        run.advance_with_commit(rejected, |_, _| panic!("cancelled candidate reached gate"))
            .is_err()
    );
    run.stop();
    assert!(
        run.advance_with_commit(control(), |_, _| panic!("stopped owner reached gate"))
            .is_err()
    );
}
fn same_state(a: &CommittedState, b: &CommittedState) {
    assert_eq!(
        (a.boundary(), a.time_seconds()),
        (b.boundary(), b.time_seconds())
    );
    assert_eq!(a.objects().bytes, b.objects().bytes);
    assert_eq!(a.history().bytes, b.history().bytes);
    assert_eq!(a.fields().len(), b.fields().len());
    for (a, b) in a.fields().iter().zip(b.fields()) {
        assert_eq!(a.bytes, b.bytes);
    }
    assert_eq!(a.forces().map(|f| &f.bytes), b.forces().map(|f| &f.bytes));
}
fn query(lease: &FieldSnapshot, request_id: u64) -> Buffer {
    let metadata = lease.request(
        request_id,
        vec![lease.context().observables[0].channel.clone()],
    );
    let mut bytes = vec![];
    encode_sample_request(
        &metadata,
        &[SamplePoint {
            id: 19,
            position_metres: [n(2.0), n(0.0), n(0.0)],
        }],
        &mut bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    Buffer {
        schema: SAMPLE_REQUEST_SCHEMA.into(),
        value_count: 1,
        bytes: bytes.into(),
    }
}
fn sampled_x(lease: &FieldSnapshot, query: Buffer) -> f64 {
    let request = SampleRequest::read(
        &query.bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let result = lease.sample(query.clone(), control()).unwrap();
    let response = SampleResponse::read(
        &result.bytes,
        &request,
        lease.context(),
        SampleLimits::default(),
    )
    .unwrap();
    match response.cell(0, 0).unwrap() {
        SampleCell::Valid { values, .. } => values.iter().next().unwrap().get(),
        _ => panic!("finite sample"),
    }
}

#[test]
fn real_run_commits_all_fields_forces_objects_and_history_once() {
    let fixture = Fixture::new();
    let initial = objects();
    let mut run = fixture.run(&initial, &[1.0, 2.0]);
    assert!(run.state().forces().is_none());
    let retained = run.state().clone();
    run.advance(control()).unwrap();
    let next = state_objects(run.state());
    assert_eq!(
        (run.state().boundary(), run.state().time_seconds()),
        (1, n(0.5))
    );
    assert_eq!(next[0], initial[0]);
    assert_eq!(next[2], initial[2]);
    assert_eq!(next[1].kinematics.velocity_metres_per_second[0], n(-0.5625));
    assert_eq!(next[1].kinematics.position_metres[0], n(1.71875));
    assert_eq!(
        run.state()
            .forces()
            .unwrap()
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .get(0)
            .unwrap()
            .newtons[0],
        n(-4.5)
    );
    assert_eq!(retained.objects().bytes, packet(&initial).bytes);
    assert_ne!(retained.fields()[0].bytes, run.state().fields()[0].bytes);
    let expected_force = -18.0 / 1.71875f64.powi(2);
    run.advance(control()).unwrap();
    let force = run
        .state()
        .forces()
        .unwrap()
        .scientific_batch::<Force>(BulkLimits::default())
        .unwrap()
        .get(0)
        .unwrap()
        .newtons[0]
        .get();
    assert!((force - expected_force).abs() < 1e-14);
}

#[test]
fn no_fields_means_inertial_motion_not_missing_force_data() {
    let fixture = Fixture::new();
    for objects in [
        vec![object(1, 0.0, 3.0, Some(2.0)), object(2, 7.0, 10.0, None)],
        vec![],
    ] {
        let program = fixture.program(&objects, &[]);
        let captured = fixture.capture(&program, &objects);
        let mut run = FixedRun::from_captured(
            fixture.host.clone(),
            scope(),
            program,
            captured,
            RunLimits {
                couplings: 0,
                ..RunLimits::default()
            },
            control(),
        )
        .unwrap();
        run.advance(control()).unwrap();
        let next = state_objects(run.state());
        if !objects.is_empty() {
            assert_eq!(next[0].kinematics.position_metres[0], n(1.5));
            assert_eq!(next[1], objects[1]);
        }
        assert_eq!(run.state().boundary(), 1);
        for force in run
            .state()
            .forces()
            .unwrap()
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .iter()
        {
            assert_eq!(force.newtons, [n(0.0); 3]);
        }
    }
}

#[test]
fn late_field_integrator_and_candidate_validation_failures_preserve_every_committed_part() {
    let fixture = Fixture::new();
    for phase in [RunPhase::Field, RunPhase::Dynamics, RunPhase::Validation] {
        let mut objects = objects();
        let gs = if phase == RunPhase::Field {
            vec![1.0, 1e308]
        } else {
            vec![1.0]
        };
        if phase == RunPhase::Dynamics {
            objects[1].inertial_mass_kilograms = Some(n(1e-310));
        }
        if phase == RunPhase::Validation {
            objects[1].kinematics.position_metres[0] = n(9.0);
            objects[1].kinematics.velocity_metres_per_second[0] = n(4.0);
        }
        let mut run = fixture.run(&objects, &gs);
        let before = run.state().clone();
        for _ in 0..2 {
            let error = run
                .advance(control())
                .expect_err("must reject entire boundary");
            let attribution = error.downcast_ref::<RunFailure>().unwrap();
            assert_eq!(attribution.phase, phase, "{error:?}");
            assert_eq!(attribution.boundary, 0);
            same_state(&before, run.state());
        }
    }
}

#[test]
fn checkpoint_restore_into_new_engine_preserves_exact_continuation_and_rejects_rebinding() {
    let fixture = Fixture::new();
    let initial = objects();
    let mut run = fixture.run(&initial, &[1.0]);
    run.advance(control()).unwrap();
    let checkpoint = run.checkpoint(control()).unwrap();
    same_state(run.state(), checkpoint.state());
    run.advance(control()).unwrap();
    let fresh = Fixture::new();
    let mut resumed = FixedRun::restore(
        fresh.host.clone(),
        scope(),
        fresh.program(&initial, &[1.0]),
        &checkpoint,
        RunLimits::default(),
        control(),
    )
    .unwrap();
    resumed.advance(control()).unwrap();
    same_state(run.state(), resumed.state());
    let mut other = scope();
    other.epoch += 1;
    assert!(
        FixedRun::restore(
            fresh.host.clone(),
            other,
            fresh.program(&initial, &[1.0]),
            &checkpoint,
            RunLimits::default(),
            control()
        )
        .is_err()
    );
    assert!(
        FixedRun::restore(
            fresh.host.clone(),
            scope(),
            fresh.program(&initial, &[2.0]),
            &checkpoint,
            RunLimits::default(),
            control()
        )
        .is_err()
    );
    let mut other = fresh.program(&initial, &[1.0]);
    other.timestep_seconds = n(0.25);
    assert!(
        FixedRun::restore(
            fresh.host.clone(),
            scope(),
            other,
            &checkpoint,
            RunLimits::default(),
            control()
        )
        .is_err()
    );
    run.stop();
    assert!(run.is_stopped());
    assert_eq!(
        run.advance(control())
            .unwrap_err()
            .downcast_ref::<RunRejection>(),
        Some(&RunRejection::Stopped)
    );
    assert!(run.checkpoint(control()).is_ok());
}

#[test]
fn leased_snapshots_survive_advance_and_observer_limits_or_failures_do_not_block_commit() {
    let fixture = Fixture::new();
    let initial = objects();
    let program = fixture.program(&initial, &[1.0]);
    let captured = fixture.capture(&program, &initial);
    let mut run = FixedRun::from_captured(
        fixture.host.clone(),
        scope(),
        program,
        captured,
        RunLimits {
            snapshots: 1,
            ..RunLimits::default()
        },
        control(),
    )
    .unwrap();
    let old = run.acquire_field(&"field-0".parse().unwrap()).unwrap();
    let old_query = query(&old, 1);
    assert!(run.acquire_field(&"field-0".parse().unwrap()).is_err());
    run.advance(control()).unwrap();
    let worker = std::thread::spawn(move || {
        let x = sampled_x(&old, old_query);
        (x, old)
    });
    run.advance(control()).unwrap();
    let (old_x, old) = worker.join().unwrap();
    assert_eq!(old_x, 0.0);
    drop(old);
    let current = run.acquire_field(&"field-0".parse().unwrap()).unwrap();
    assert_eq!(sampled_x(&current, query(&current, 2)), -0.5);
    let cancelled = control();
    cancelled.cancel();
    assert!(current.sample(query(&current, 3), cancelled).is_err());
    let mut bad_metadata =
        current.request(4, vec![current.context().observables[0].channel.clone()]);
    bad_metadata.snapshot.source = SnapshotSource::Authored {
        revision: ArtifactDigest::sha256_of(b"not this run"),
    };
    let mut bad = vec![];
    encode_sample_request(
        &bad_metadata,
        &[SamplePoint {
            id: 1,
            position_metres: [n(2.0), n(0.0), n(0.0)],
        }],
        &mut bad,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    assert!(
        current
            .sample(
                Buffer {
                    schema: SAMPLE_REQUEST_SCHEMA.into(),
                    value_count: 1,
                    bytes: bad.into()
                },
                control()
            )
            .is_err()
    );
    run.advance(control()).unwrap();
    assert_eq!(run.state().boundary(), 3);
    let before = run.state().clone();
    let cancelled = control();
    cancelled.cancel();
    assert!(run.advance(cancelled).is_err());
    same_state(&before, run.state());
}

#[test]
fn initial_graph_projection_history_and_aggregate_limits_are_independently_checked() {
    let fixture = Fixture::new();
    let initial = objects();
    for case in 0..6 {
        let mut program = fixture.program(&initial, &[1.0, 2.0]);
        let mut captured = fixture.capture(&program, &initial);
        let mut limits = RunLimits::default();
        match case {
            0 => {
                captured.fields.pop();
            }
            1 => program.fields.swap(0, 1),
            2 => {
                let mut rows = initial.clone();
                rows[0].kinematics.position_metres[0] = n(1.0);
                captured.objects = packet(&rows);
            }
            3 => captured.history.bytes = Arc::from([0u8; 24]),
            4 => limits.scientific_bytes = 512,
            _ => limits.couplings = 3,
        }
        assert!(
            FixedRun::from_captured(
                fixture.host.clone(),
                scope(),
                program,
                captured,
                limits,
                control()
            )
            .is_err(),
            "case {case}"
        );
    }
}
