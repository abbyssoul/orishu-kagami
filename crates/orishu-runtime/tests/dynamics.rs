use orishu_plugin::{ArtifactDigest, ExecutionContractId};
use orishu_runtime::{
    Buffer, CompiledKernel, DynamicsOperation, DynamicsResult, KernelRejection, OperationControl,
    OutputExtent, Sandbox, SandboxLimits, StepContext,
};
use std::{sync::Arc, time::Duration};

const DYNAMICS: &[u8] = include_bytes!("fixtures/dynamics.component.wasm");
fn buffer(schema: &str, bytes: &[u8], values: u64) -> Buffer {
    Buffer {
        schema: schema.into(),
        bytes: Arc::from(bytes),
        value_count: values,
    }
}
fn entities(ids: &[u8]) -> Buffer {
    buffer("fixture.entities/v1", ids, ids.len() as u64)
}
fn history(bytes: &[u8]) -> Buffer {
    buffer("fixture.history/v1", bytes, bytes.len() as u64 / 2)
}
fn history_shape(count: usize) -> OutputExtent<'static> {
    OutputExtent {
        schema: "fixture.history/v1",
        bytes: count * 2,
        values: count as u64,
    }
}
fn context() -> StepContext {
    StepContext {
        workload: "fixture-workload".into(),
        run: "fixture-run".into(),
        epoch: 0,
        boundary: 0,
        invocation: 1,
        partition: "whole".into(),
        coverage: "whole".into(),
        time_seconds: 0.0,
        dt_seconds: 0.01,
    }
}
fn validation() -> Buffer {
    buffer("fixture.validation/v1", &[], 0)
}
fn history_result(result: DynamicsResult) -> Buffer {
    match result {
        DynamicsResult::History(state) => state,
        _ => panic!("expected history"),
    }
}
fn integrated_history(result: DynamicsResult, expected_ids: &[u8]) -> Buffer {
    match result {
        DynamicsResult::Integrated { entities, history } => {
            assert_eq!(&*entities.bytes, expected_ids);
            assert_eq!(entities.value_count, expected_ids.len() as u64);
            history
        }
        _ => panic!("expected both integration outputs"),
    }
}
struct Fixture {
    host: Sandbox,
    kernel: CompiledKernel,
}
impl Fixture {
    fn new() -> Self {
        let host = Sandbox::new(SandboxLimits {
            fuel: 500_000,
            ..SandboxLimits::default()
        })
        .unwrap();
        let kernel = host
            .compile(
                DYNAMICS,
                ArtifactDigest::sha256_of(DYNAMICS),
                ExecutionContractId::Dynamics,
            )
            .unwrap();
        Self { host, kernel }
    }
    fn run(&self, mode: u8, operation: DynamicsOperation<'_>) -> wasmtime::Result<DynamicsResult> {
        self.host.invoke_dynamics(
            &self.kernel,
            buffer("fixture.context/v1", &[], 0),
            buffer("fixture.config/v1", &[mode], 1),
            operation,
            OperationControl::new(Duration::from_secs(1)).unwrap(),
        )
    }
    fn integrate(
        &self,
        mode: u8,
        step: &StepContext,
        ids: &[u8],
        prior: Buffer,
    ) -> wasmtime::Result<DynamicsResult> {
        self.run(
            mode,
            DynamicsOperation::Integrate {
                step,
                entities: entities(ids),
                forces: buffer("fixture.forces/v1", &[], 0),
                prior_history: prior,
                validation: validation(),
                next_entities: OutputExtent {
                    schema: "fixture.entities/v1",
                    bytes: ids.len(),
                    values: ids.len() as u64,
                },
                history: history_shape(ids.len()),
            },
        )
    }
}

#[test]
fn real_dynamics_history_birth_death_and_restart_preserve_next_step() {
    let fixture = Fixture::new();
    let mut step = context();
    let initial = history_result(
        fixture
            .run(
                0,
                DynamicsOperation::InitializeHistory {
                    entities: entities(&[1, 3]),
                    history: history_shape(2),
                },
            )
            .unwrap(),
    );
    assert_eq!(&*initial.bytes, &[1, 0, 3, 0]);
    assert!(matches!(
        fixture
            .run(
                0,
                DynamicsOperation::Validate {
                    history: initial.clone(),
                    inputs: validation(),
                }
            )
            .unwrap(),
        DynamicsResult::Admissible(None)
    ));
    let advanced = integrated_history(
        fixture
            .integrate(0, &step, &[1, 3], initial.clone())
            .unwrap(),
        &[1, 3],
    );
    assert_eq!(&*advanced.bytes, &[1, 1, 3, 1]);
    assert_eq!(&*initial.bytes, &[1, 0, 3, 0]);
    step.boundary = 1;
    step.time_seconds = 0.01;
    let transitioned = history_result(
        fixture
            .run(
                0,
                DynamicsOperation::TransitionEntities {
                    step: &step,
                    births: entities(&[2]),
                    deaths: entities(&[1]),
                    prior_history: advanced,
                    validation: validation(),
                    history: history_shape(2),
                },
            )
            .unwrap(),
    );
    assert_eq!(&*transitioned.bytes, &[2, 0, 3, 1]);
    // Mode 1 loops in initializeHistory: these operations must use supplied history.
    let checkpoint = history_result(
        fixture
            .run(
                1,
                DynamicsOperation::Checkpoint {
                    history: transitioned.clone(),
                    output: history_shape(2),
                },
            )
            .unwrap(),
    );
    let restored = history_result(
        fixture
            .run(
                1,
                DynamicsOperation::Restore {
                    checkpoint,
                    validation: validation(),
                    output: history_shape(2),
                },
            )
            .unwrap(),
    );
    assert_eq!(&*restored.bytes, &*transitioned.bytes);
    assert!(!Arc::ptr_eq(&restored.bytes, &transitioned.bytes));
    let uninterrupted = integrated_history(
        fixture.integrate(1, &step, &[2, 3], transitioned).unwrap(),
        &[2, 3],
    );
    let restarted = integrated_history(
        fixture.integrate(1, &step, &[2, 3], restored).unwrap(),
        &[2, 3],
    );
    assert_eq!(&*uninterrupted.bytes, &[2, 1, 3, 2]);
    assert_eq!(&*restarted.bytes, &*uninterrupted.bytes);
    // Empty history is explicit and supported, not inferred from a missing entry.
    let empty = history_result(
        fixture
            .run(
                0,
                DynamicsOperation::InitializeHistory {
                    entities: entities(&[]),
                    history: history_shape(0),
                },
            )
            .unwrap(),
    );
    let empty_next = integrated_history(fixture.integrate(0, &step, &[], empty).unwrap(), &[]);
    assert!(empty_next.bytes.is_empty());
    assert!(fixture.integrate(0, &step, &[1], history(&[])).is_err());
}

#[test]
fn dynamics_failures_never_return_partial_candidates_or_replace_history() {
    let fixture = Fixture::new();
    let step = context();
    let prior = history(&[1, 9]);
    for (mode, expected) in [
        (2, KernelRejection::IncompatibleState),
        (3, KernelRejection::InadmissibleTimestep),
        (6, KernelRejection::NumericalFailure),
    ] {
        let error = fixture
            .integrate(mode, &step, &[1], prior.clone())
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<KernelRejection>(),
            Some(&expected),
            "{error:?}"
        );
    }
    for mode in [4, 7, 8] {
        assert!(
            fixture.integrate(mode, &step, &[1], prior.clone()).is_err(),
            "mode {mode}"
        );
    }
    assert!(
        fixture
            .run(
                5,
                DynamicsOperation::TransitionEntities {
                    step: &step,
                    births: entities(&[2]),
                    deaths: entities(&[]),
                    prior_history: prior.clone(),
                    validation: validation(),
                    history: history_shape(2),
                }
            )
            .is_err()
    );
    for (births, deaths) in [
        (&[1][..], &[][..]),
        (&[][..], &[2][..]),
        (&[2, 2][..], &[][..]),
        (&[1][..], &[1][..]),
    ] {
        assert!(
            fixture
                .run(
                    0,
                    DynamicsOperation::TransitionEntities {
                        step: &step,
                        births: entities(births),
                        deaths: entities(deaths),
                        prior_history: prior.clone(),
                        validation: validation(),
                        history: history_shape(1),
                    }
                )
                .is_err()
        );
    }
    for invalid in [
        history(&[1]),
        history(&[1, 0, 1, 0]),
        history(&[0, 0]),
        history(&[2, 0]),
    ] {
        assert!(fixture.integrate(0, &step, &[1], invalid).is_err());
    }
    let next = integrated_history(
        fixture.integrate(0, &step, &[1], prior.clone()).unwrap(),
        &[1],
    );
    assert_eq!(&*next.bytes, &[1, 10]);
    assert_eq!(&*prior.bytes, &[1, 9]);
    let error = fixture
        .run(
            1,
            DynamicsOperation::InitializeHistory {
                entities: entities(&[1]),
                history: history_shape(1),
            },
        )
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(wasmtime::Trap::OutOfFuel)
    ));
}

#[test]
fn dynamics_and_field_contracts_cannot_be_substituted() {
    let fixture = Fixture::new();
    assert!(
        fixture
            .host
            .compile(
                DYNAMICS,
                ArtifactDigest::sha256_of(DYNAMICS),
                ExecutionContractId::Field
            )
            .is_err()
    );
    let field = include_bytes!("fixtures/field.component.wasm");
    let kernel = fixture
        .host
        .compile(
            field,
            ArtifactDigest::sha256_of(field),
            ExecutionContractId::Field,
        )
        .unwrap();
    assert!(
        fixture
            .host
            .invoke_dynamics(
                &kernel,
                validation(),
                validation(),
                DynamicsOperation::InitializeHistory {
                    entities: entities(&[]),
                    history: history_shape(0)
                },
                OperationControl::new(Duration::from_secs(1)).unwrap()
            )
            .is_err()
    );
}
