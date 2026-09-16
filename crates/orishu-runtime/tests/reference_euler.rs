//! Numerical evidence through the independently compiled Component, not a native
//! substitute. This is not yet whole-run/worker admission or coupled-field proof.
use orishu_plugin::{ArtifactDigest, ExecutionContractId, FiniteF64, execution::*};
use orishu_runtime::{
    Buffer, CompiledKernel, DynamicsOperation, DynamicsResult, KernelRejection, OperationControl,
    OutputExtent, Sandbox, SandboxLimits, StepContext,
};
use std::time::Duration;
mod reference_support;

const CODE: &[u8] = include_bytes!("fixtures/euler.component.wasm");
const HISTORY: &str = "org.orishu.reference.euler.history/v1";
fn n(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}
fn body(id: u64, mass: f64, x: f64, v: f64) -> DynamicEntity {
    DynamicEntity {
        id: EntityId(id),
        inertial_mass_kilograms: n(mass),
        kinematics: Kinematics {
            position_metres: [n(x), n(0.0), n(0.0)],
            velocity_metres_per_second: [n(v), n(0.0), n(0.0)],
        },
    }
}
fn force(id: u64, x: f64) -> Force {
    Force {
        id: EntityId(id),
        newtons: [n(x), n(0.0), n(0.0)],
    }
}
fn raw(schema: &str, bytes: Vec<u8>, count: u64) -> Buffer {
    Buffer {
        schema: schema.into(),
        bytes: bytes.into(),
        value_count: count,
    }
}
fn packet<R: BulkRecord>(records: &[R]) -> Buffer {
    let mut bytes = vec![];
    encode_batch(records, &mut bytes, BulkLimits::default()).unwrap();
    raw(R::SCHEMA, bytes, records.len() as u64)
}
fn shape<R: BulkRecord>(count: usize) -> OutputExtent<'static> {
    OutputExtent {
        schema: R::SCHEMA,
        bytes: packet_bytes::<R>(count, BulkLimits::default()).unwrap(),
        values: count as u64,
    }
}
fn history_shape(count: usize) -> OutputExtent<'static> {
    OutputExtent {
        schema: HISTORY,
        bytes: 16 + 8 * count,
        values: count as u64,
    }
}
fn validation(entities: &Buffer, dt: f64) -> Buffer {
    let config = reference_support::euler_config(4096);
    let ctx = reference_support::context(CODE, ExecutionContractId::Dynamics, &config, 4096);
    reference_support::validation(&ctx, &config, entities, dt)
}
fn step(boundary: u64, dt: f64) -> StepContext {
    StepContext {
        workload: "numerical-euler".into(),
        run: "proof".into(),
        epoch: 0,
        boundary,
        invocation: boundary + 1,
        partition: "whole".into(),
        coverage: "whole".into(),
        time_seconds: boundary as f64 * dt,
        dt_seconds: dt,
    }
}
struct Fixture {
    host: Sandbox,
    kernel: CompiledKernel,
}
impl Fixture {
    fn new() -> Self {
        let host = Sandbox::new(SandboxLimits::default()).unwrap();
        let kernel = host
            .compile(
                CODE,
                ArtifactDigest::sha256_of(CODE),
                ExecutionContractId::Dynamics,
            )
            .unwrap();
        Self { host, kernel }
    }
    fn run(&self, op: DynamicsOperation<'_>) -> wasmtime::Result<DynamicsResult> {
        let config = reference_support::euler_config(4096);
        let context =
            reference_support::context(CODE, ExecutionContractId::Dynamics, &config, 4096);
        self.host.invoke_dynamics_bound(
            &self.kernel,
            &context,
            config,
            op,
            OperationControl::new(Duration::from_secs(2)).unwrap(),
        )
    }
    fn initialize(&self, entities: &Buffer) -> Buffer {
        match self
            .run(DynamicsOperation::InitializeHistory {
                entities: entities.clone(),
                history: history_shape(entities.value_count as usize),
            })
            .unwrap()
        {
            DynamicsResult::History(history) => history,
            _ => panic!("expected history"),
        }
    }
    fn integrate(
        &self,
        entities: Buffer,
        history: Buffer,
        forces: Buffer,
        context: &StepContext,
    ) -> wasmtime::Result<(Buffer, Buffer)> {
        let count = entities.value_count as usize;
        match self.run(DynamicsOperation::Integrate {
            step: context,
            validation: validation(&entities, context.dt_seconds),
            entities,
            forces,
            prior_history: history,
            next_entities: shape::<DynamicEntity>(count),
            history: history_shape(count),
        })? {
            DynamicsResult::Integrated { entities, history } => Ok((entities, history)),
            _ => panic!("expected integration"),
        }
    }
}

#[test]
fn real_component_uses_inertial_mass_and_kick_then_drift_not_host_equations() {
    let fixture = Fixture::new();
    let initial = packet(&[body(1, 2.0, 0.0, 0.0), body(2, 1.0, 0.0, 0.0)]);
    let history = fixture.initialize(&initial);
    let (next, _) = fixture
        .integrate(
            initial.clone(),
            history,
            packet(&[force(1, 4.0), force(2, -4.0)]),
            &step(0, 0.5),
        )
        .unwrap();
    let values: Vec<_> = next
        .scientific_batch::<DynamicEntity>(BulkLimits::default())
        .unwrap()
        .iter()
        .collect();
    assert_eq!(values, &[body(1, 2.0, 0.5, 1.0), body(2, 1.0, -1.0, -2.0)]);
    let momentum: f64 = values
        .iter()
        .map(|e| e.inertial_mass_kilograms.get() * e.kinematics.velocity_metres_per_second[0].get())
        .sum();
    assert_eq!(momentum, 0.0);
    assert_eq!(
        initial
            .scientific_batch::<DynamicEntity>(BulkLimits::default())
            .unwrap()
            .get(0)
            .unwrap()
            .kinematics
            .position_metres[0],
        n(0.0)
    );
    let initial = packet(&[body(9, 7.0, 10.0, 3.0)]);
    let history = fixture.initialize(&initial);
    let (next, _) = fixture
        .integrate(initial, history, packet(&[force(9, 0.0)]), &step(0, 0.5))
        .unwrap();
    assert_eq!(
        next.scientific_batch::<DynamicEntity>(BulkLimits::default())
            .unwrap()
            .get(0),
        Some(body(9, 7.0, 11.5, 3.0))
    );
}

#[test]
fn constant_force_converges_at_first_order_through_actual_wasm_calls() {
    let fixture = Fixture::new();
    let mut errors = vec![];
    for steps in [10, 20] {
        let dt = 1.0 / steps as f64;
        let mut entities = packet(&[body(1, 2.0, 0.0, 0.0)]);
        let mut history = fixture.initialize(&entities);
        for boundary in 0..steps {
            (entities, history) = fixture
                .integrate(
                    entities,
                    history,
                    packet(&[force(1, 2.0)]),
                    &step(boundary, dt),
                )
                .unwrap();
        }
        let entity = entities
            .scientific_batch::<DynamicEntity>(BulkLimits::default())
            .unwrap()
            .get(0)
            .unwrap();
        assert!((entity.kinematics.velocity_metres_per_second[0].get() - 1.0).abs() < 1e-12);
        let position = entity.kinematics.position_metres[0].get();
        assert!((position - (0.5 + 0.5 / steps as f64)).abs() < 1e-12);
        errors.push((position - 0.5).abs());
    }
    assert!((errors[0] / errors[1] - 2.0).abs() < 1e-10);
}

#[test]
fn checkpoint_and_birth_death_history_preserve_numerical_continuation() {
    let fixture = Fixture::new();
    let initial = packet(&[body(1, 2.0, 0.0, 0.0)]);
    let history = fixture.initialize(&initial);
    let (first, history) = fixture
        .integrate(initial, history, packet(&[force(1, 2.0)]), &step(0, 0.25))
        .unwrap();
    let checkpoint = match fixture
        .run(DynamicsOperation::Checkpoint {
            history: history.clone(),
            output: history_shape(1),
        })
        .unwrap()
    {
        DynamicsResult::History(h) => h,
        _ => panic!("expected checkpoint"),
    };
    // A fresh engine and separately compiled instance cannot reuse guest-local state.
    let restarted = Fixture::new();
    let restored = match restarted
        .run(DynamicsOperation::Restore {
            checkpoint,
            validation: validation(&first, 0.25),
            output: history_shape(1),
        })
        .unwrap()
    {
        DynamicsResult::History(h) => h,
        _ => panic!("expected restored history"),
    };
    let uninterrupted = fixture
        .integrate(
            first.clone(),
            history.clone(),
            packet(&[force(1, 2.0)]),
            &step(1, 0.25),
        )
        .unwrap();
    let resumed = restarted
        .integrate(
            first.clone(),
            restored,
            packet(&[force(1, 2.0)]),
            &step(1, 0.25),
        )
        .unwrap();
    assert_eq!(uninterrupted.0.bytes, resumed.0.bytes);
    assert_eq!(uninterrupted.1.bytes, resumed.1.bytes);

    let birth = body(2, 3.0, 4.0, 2.0);
    let survivor = first
        .scientific_batch::<DynamicEntity>(BulkLimits::default())
        .unwrap()
        .get(0)
        .unwrap();
    let joined = packet(&[survivor, birth]);
    let grown = match fixture
        .run(DynamicsOperation::TransitionEntities {
            step: &step(1, 0.25),
            births: packet(&[birth]),
            deaths: packet::<EntityId>(&[]),
            prior_history: history.clone(),
            validation: validation(&joined, 0.25),
            history: history_shape(2),
        })
        .unwrap()
    {
        DynamicsResult::History(h) => h,
        _ => panic!("expected history"),
    };
    assert!(
        fixture
            .integrate(
                joined.clone(),
                history.clone(),
                packet(&[force(1, 0.0), force(2, 0.0)]),
                &step(1, 0.25)
            )
            .is_err()
    );
    let (next, grown) = fixture
        .integrate(
            joined,
            grown,
            packet(&[force(1, 0.0), force(2, 0.0)]),
            &step(1, 0.25),
        )
        .unwrap();
    assert_eq!(
        next.scientific_batch::<DynamicEntity>(BulkLimits::default())
            .unwrap()
            .get(1),
        Some(body(2, 3.0, 4.5, 2.0))
    );
    let retired = match fixture
        .run(DynamicsOperation::TransitionEntities {
            step: &step(2, 0.25),
            births: packet::<DynamicEntity>(&[]),
            deaths: packet(&[EntityId(1)]),
            prior_history: grown,
            validation: validation(&packet(&[body(2, 3.0, 4.5, 2.0)]), 0.25),
            history: history_shape(1),
        })
        .unwrap()
    {
        DynamicsResult::History(h) => h,
        _ => panic!("expected history"),
    };
    assert!(
        fixture
            .integrate(
                packet(&[body(2, 3.0, 4.5, 2.0)]),
                retired,
                packet(&[force(2, 0.0)]),
                &step(2, 0.25)
            )
            .is_ok()
    );
}

#[test]
fn malformed_force_membership_history_and_numerical_overflow_return_no_candidates() {
    let fixture = Fixture::new();
    let initial = packet(&[body(1, 1.0, 0.0, 0.0)]);
    let history = fixture.initialize(&initial);
    for forces in [
        packet::<Force>(&[]),
        packet(&[force(2, 0.0)]),
        packet(&[force(1, 0.0), force(2, 0.0)]),
    ] {
        assert!(
            fixture
                .integrate(initial.clone(), history.clone(), forces, &step(0, 0.25))
                .is_err()
        );
    }
    let error = fixture
        .integrate(
            initial.clone(),
            history.clone(),
            packet(&[force(1, f64::MAX)]),
            &step(0, 2.0),
        )
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<KernelRejection>(),
        Some(&KernelRejection::NumericalFailure)
    );
    for (births, deaths) in [
        (packet(&[body(1, 1.0, 0.0, 0.0)]), packet::<EntityId>(&[])),
        (packet::<DynamicEntity>(&[]), packet(&[EntityId(9)])),
        (packet(&[body(1, 1.0, 0.0, 0.0)]), packet(&[EntityId(1)])),
    ] {
        assert!(
            fixture
                .run(DynamicsOperation::TransitionEntities {
                    step: &step(0, 0.25),
                    births,
                    deaths,
                    prior_history: history.clone(),
                    validation: validation(&initial, 0.25),
                    history: history_shape(1)
                })
                .is_err()
        );
    }
    let mut corrupt = history.clone();
    corrupt.bytes = vec![0; history.bytes.len()].into();
    assert!(
        fixture
            .integrate(
                initial.clone(),
                corrupt,
                packet(&[force(1, 0.0)]),
                &step(0, 0.25)
            )
            .is_err()
    );
    let (valid, _) = fixture
        .integrate(
            initial.clone(),
            history,
            packet(&[force(1, 0.0)]),
            &step(0, 0.25),
        )
        .unwrap();
    assert_eq!(valid.bytes, initial.bytes);
}

#[test]
fn kernel_checks_admissibility_and_supports_explicit_empty_membership() {
    let fixture = Fixture::new();
    let entities = packet(&[body(1, 1.0, 0.0, 0.0)]);
    let history = fixture.initialize(&entities);
    for invalid in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let error = fixture
            .run(DynamicsOperation::Validate {
                history: history.clone(),
                inputs: validation(&entities, invalid),
            })
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<orishu_runtime::ContextRejection>(),
            Some(&orishu_runtime::ContextRejection::Validation)
        );
    }
    let empty = packet::<DynamicEntity>(&[]);
    let cleared = match fixture
        .run(DynamicsOperation::TransitionEntities {
            step: &step(0, 0.25),
            births: empty.clone(),
            deaths: packet(&[EntityId(1)]),
            prior_history: history,
            validation: validation(&empty, 0.25),
            history: history_shape(0),
        })
        .unwrap()
    {
        DynamicsResult::History(h) => h,
        _ => panic!("expected empty history"),
    };
    assert_eq!(cleared.value_count, 0);
    let (next, history) = fixture
        .integrate(empty.clone(), cleared, packet::<Force>(&[]), &step(1, 0.25))
        .unwrap();
    assert_eq!(next.bytes, empty.bytes);
    assert_eq!(history.value_count, 0);
    assert_eq!(history.bytes.len(), 16);
}

#[test]
fn bound_host_refuses_substituted_artifact_provider_config_timestep_and_projection() {
    use orishu_runtime::ContextRejection;
    let fixture = Fixture::new();
    let entities = packet(&[body(1, 1.0, 0.0, 0.0)]);
    let history = fixture.initialize(&entities);
    let config = reference_support::euler_config(4096);
    let context = reference_support::context(CODE, ExecutionContractId::Dynamics, &config, 4096);
    let step = step(0, 0.25);
    let valid = validation(&entities, 0.25);
    let invoke = |ctx: &InstanceContext, cfg: Buffer, validation: Buffer| {
        fixture.host.invoke_dynamics_bound(
            &fixture.kernel,
            ctx,
            cfg,
            DynamicsOperation::Integrate {
                step: &step,
                entities: entities.clone(),
                forces: packet(&[force(1, 0.0)]),
                prior_history: history.clone(),
                validation,
                next_entities: shape::<DynamicEntity>(1),
                history: history_shape(1),
            },
            OperationControl::new(Duration::from_secs(2)).unwrap(),
        )
    };
    let mut other = context.clone();
    other.kernel = ArtifactDigest::sha256_of(b"different code");
    assert_eq!(
        invoke(&other, config.clone(), valid.clone())
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::Kernel)
    );
    other = context.clone();
    other.contribution.release = ArtifactDigest::sha256_of(b"other provider")
        .to_string()
        .parse()
        .unwrap();
    assert_eq!(
        invoke(&other, config.clone(), valid.clone())
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::Validation)
    );
    let other_config = reference_support::euler_config(4095);
    assert_eq!(
        invoke(&context, other_config, valid.clone())
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::Input)
    );
    let wrong_dt = validation(&entities, 0.5);
    assert_eq!(
        invoke(&context, config.clone(), wrong_dt)
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::Validation)
    );
    let other_entities = packet(&[body(1, 1.0, 100.0, 0.0)]);
    assert_eq!(
        invoke(&context, config.clone(), validation(&other_entities, 0.25))
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::Validation)
    );
    let mut wrong_count = valid.clone();
    wrong_count.value_count = 2;
    assert_eq!(
        invoke(&context, config.clone(), wrong_count)
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::Validation)
    );
    other = context.clone();
    other.bounds.state_bytes = 1;
    assert_eq!(
        invoke(&other, config.clone(), valid.clone())
            .unwrap_err()
            .downcast_ref::<ContextRejection>(),
        Some(&ContextRejection::State)
    );
    match invoke(&context, config, valid).unwrap() {
        DynamicsResult::Integrated {
            entities: next,
            history: next_history,
        } => {
            assert_eq!(next.bytes, entities.bytes);
            assert_eq!(next_history.bytes, history.bytes);
        }
        _ => panic!("expected integrated candidates"),
    }
}
