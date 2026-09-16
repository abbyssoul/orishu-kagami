use orishu_plugin::{ArtifactDigest, ExecutionContractId};
use orishu_runtime::{Buffer, OutputExtent, Sandbox, SandboxLimits};
use std::sync::Arc;

const FIELD: &[u8] = include_bytes!("fixtures/field.component.wasm");
fn buffer(schema: &str, bytes: &[u8]) -> Buffer {
    Buffer {
        schema: schema.into(),
        value_count: 1,
        bytes: Arc::from(bytes),
    }
}

#[test]
fn actual_component_initialization_preserves_inputs_and_returns_explicit_state() {
    let host = Sandbox::new(SandboxLimits::default()).unwrap();
    let kernel = host
        .compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Field,
        )
        .unwrap();
    let natural = buffer("fixture.domain/v1", &1.5_f64.to_le_bytes());
    let result = host
        .initialize_field(
            &kernel,
            buffer("fixture.context/v1", &[]),
            buffer("fixture.config/v1", &[0]),
            natural.clone(),
            OutputExtent {
                schema: "fixture.state/v1",
                bytes: 8,
                values: 1,
            },
        )
        .unwrap();
    assert_eq!(&*result.bytes, &1.5_f64.to_le_bytes());
    assert_eq!(&*natural.bytes, &1.5_f64.to_le_bytes());
    assert!(!Arc::ptr_eq(&natural.bytes, &result.bytes));
    assert_eq!(result.schema, "fixture.state/v1");
}

#[test]
fn traps_loops_missing_finish_and_invalid_writes_return_no_candidate() {
    let limits = SandboxLimits {
        fuel: 100_000,
        ..SandboxLimits::default()
    };
    let host = Sandbox::new(limits).unwrap();
    let kernel = host
        .compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Field,
        )
        .unwrap();
    let prior = buffer("fixture.domain/v1", &[1, 2, 3, 4]);
    for mode in 1..=6 {
        let result = host.initialize_field(
            &kernel,
            buffer("fixture.context/v1", &[]),
            buffer("fixture.config/v1", &[mode]),
            prior.clone(),
            OutputExtent {
                schema: "fixture.state/v1",
                bytes: 4,
                values: 1,
            },
        );
        assert!(result.is_err(), "mode {mode}");
        if mode == 1 {
            assert!(matches!(
                result.unwrap_err().downcast_ref::<wasmtime::Trap>(),
                Some(wasmtime::Trap::OutOfFuel)
            ));
        }
        assert_eq!(&*prior.bytes, &[1, 2, 3, 4]);
    }
}

#[test]
fn deadlines_and_cancellation_are_independent_per_operation() {
    use orishu_runtime::{Interruption, OperationControl};
    use std::time::{Duration, Instant};
    let host = Sandbox::new(SandboxLimits {
        fuel: u64::MAX,
        ..SandboxLimits::default()
    })
    .unwrap();
    let kernel = host
        .compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Field,
        )
        .unwrap();
    let run = |mode, control| {
        host.initialize_field_with_control(
            &kernel,
            buffer("fixture.context/v1", &[]),
            buffer("fixture.config/v1", &[mode]),
            buffer("fixture.domain/v1", &[1]),
            OutputExtent {
                schema: "fixture.state/v1",
                bytes: 1,
                values: 1,
            },
            control,
        )
    };
    let started = Instant::now();
    let error = run(1, OperationControl::new(Duration::from_millis(20)).unwrap()).unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<Interruption>(),
            Some(Interruption::DeadlineExceeded)
        ),
        "{error:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let cancelled = OperationControl::new(Duration::from_secs(2)).unwrap();
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| run(1, cancelled.clone()));
        // Successful work on the same engine has its own deadline/token.
        assert!(run(0, OperationControl::new(Duration::from_secs(1)).unwrap()).is_ok());
        cancelled.cancel();
        let error = pending.join().unwrap().unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<Interruption>(),
                Some(Interruption::Cancelled)
            ),
            "{error:?}"
        );
    });
    assert!(run(0, OperationControl::new(Duration::from_secs(1)).unwrap()).is_ok());
    let parent = orishu_runtime::Cancellation::default();
    let linked = OperationControl::new(Duration::from_secs(2))
        .unwrap()
        .with_parent(parent.clone())
        .unwrap();
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| run(1, linked));
        assert!(run(0, OperationControl::new(Duration::from_secs(1)).unwrap()).is_ok());
        parent.cancel();
        let error = pending.join().unwrap().unwrap_err();
        assert_eq!(
            error.downcast_ref::<Interruption>(),
            Some(&Interruption::Cancelled)
        );
    });
}

#[test]
fn bytes_contract_imports_exports_and_bounds_are_independently_checked() {
    let host = Sandbox::new(SandboxLimits::default()).unwrap();
    assert!(
        host.compile(
            FIELD,
            ArtifactDigest::sha256_of(b"wrong"),
            ExecutionContractId::Field
        )
        .is_err()
    );
    assert!(
        host.compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Dynamics
        )
        .is_err()
    );
    for source in [
        "(module)",
        "(component)",
        "(component (import \"wasi:clocks/clock@0.2.0\" (instance)))",
    ] {
        let bytes = wat::parse_str(source).unwrap();
        assert!(
            host.compile(
                &bytes,
                ArtifactDigest::sha256_of(&bytes),
                ExecutionContractId::Field
            )
            .is_err()
        );
    }
    let tiny = Sandbox::new(SandboxLimits {
        component_bytes: FIELD.len() - 1,
        ..SandboxLimits::default()
    })
    .unwrap();
    assert!(
        tiny.compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Field
        )
        .is_err()
    );
    let tiny = Sandbox::new(SandboxLimits {
        memory_bytes: 64 * 1024,
        ..SandboxLimits::default()
    })
    .unwrap();
    let kernel = tiny
        .compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Field,
        )
        .unwrap();
    assert!(
        tiny.initialize_field(
            &kernel,
            buffer("fixture.context/v1", &[]),
            buffer("fixture.config/v1", &[0]),
            buffer("fixture.domain/v1", &[1]),
            OutputExtent {
                schema: "fixture.state/v1",
                bytes: 1,
                values: 1
            }
        )
        .is_err()
    );
}

#[test]
fn real_field_lifecycle_uses_captured_state_and_isolates_observation_and_failures() {
    use orishu_runtime::{
        FieldOperation, FieldResult, KernelRejection, OperationControl, StepContext,
    };
    use std::time::Duration;
    let host = Sandbox::new(SandboxLimits::default()).unwrap();
    let kernel = host
        .compile(
            FIELD,
            ArtifactDigest::sha256_of(FIELD),
            ExecutionContractId::Field,
        )
        .unwrap();
    let state = buffer("fixture.state/v1", &[1, 2, 3, 4]);
    let validation = || buffer("fixture.validation/v1", &[]);
    let shape = || OutputExtent {
        schema: "fixture.state/v1",
        bytes: 4,
        values: 1,
    };
    let run = |mode, operation| {
        host.invoke_field(
            &kernel,
            buffer("fixture.context/v1", &[]),
            buffer("fixture.config/v1", &[mode]),
            operation,
            OperationControl::new(Duration::from_secs(1)).unwrap(),
        )
    };
    let step = || StepContext {
        workload: "fixture-workload".into(),
        run: "fixture-run".into(),
        epoch: 0,
        boundary: 0,
        invocation: 0,
        partition: "whole".into(),
        coverage: "whole".into(),
        time_seconds: 0.0,
        dt_seconds: 0.01,
    };
    let context = step();
    let advance = || FieldOperation::Advance {
        step: &context,
        prior: state.clone(),
        entities: buffer("fixture.entities/v1", &[]),
        validation: validation(),
        field: shape(),
        forces: OutputExtent {
            schema: "fixture.forces/v1",
            bytes: 0,
            values: 0,
        },
    };
    let checkpoint = || FieldOperation::Checkpoint {
        state: state.clone(),
        output: shape(),
    };
    let restore = || FieldOperation::Restore {
        checkpoint: state.clone(),
        validation: validation(),
        output: shape(),
    };
    let sample = || FieldOperation::Sample {
        snapshot: state.clone(),
        query: buffer("fixture.query/v1", &[]),
        output: shape(),
    };
    // Mode 1 would loop forever if any lifecycle operation secretly initialized.
    for operation in [checkpoint(), restore(), sample(), advance()] {
        let result = run(1, operation).unwrap();
        let output = match result {
            FieldResult::State(output) | FieldResult::Samples(output) => output,
            FieldResult::Advanced { field, forces } => {
                assert!(forces.bytes.is_empty());
                assert_eq!(forces.value_count, 0);
                field
            }
            _ => panic!("unexpected operation result"),
        };
        assert_eq!(&*output.bytes, &*state.bytes);
        assert!(!Arc::ptr_eq(&state.bytes, &output.bytes));
    }
    let result = run(
        15,
        FieldOperation::Validate {
            state: state.clone(),
            inputs: validation(),
        },
    )
    .unwrap();
    assert!(matches!(result, FieldResult::Admissible(Some(advice))
        if advice.upper_bound_seconds == Some(0.01) && advice.recommended_seconds == Some(0.005)));
    for (mode, operation, reason) in [
        (7, advance(), KernelRejection::IncompatibleState),
        (8, advance(), KernelRejection::InadmissibleTimestep),
        (9, checkpoint(), KernelRejection::NumericalFailure),
        (10, restore(), KernelRejection::IncompatibleState),
        (11, advance(), KernelRejection::NumericalFailure),
    ] {
        let error = run(mode, operation).unwrap_err();
        assert_eq!(
            error.downcast_ref::<KernelRejection>(),
            Some(&reason),
            "mode {mode}: {error:?}"
        );
    }
    // A finished field cannot escape with incomplete forces; invalid advice is not admission.
    assert!(run(12, advance()).is_err());
    assert!(run(14, advance()).is_err());
    // Sampling mutates guest-private scratch and then traps; next advance is unaffected.
    assert!(run(13, sample()).is_err());
    assert!(matches!(
        run(0, advance()).unwrap(),
        FieldResult::Advanced { .. }
    ));
    assert_eq!(&*state.bytes, &[1, 2, 3, 4]);
    for bad_dt in [0.0, -0.1, f64::NAN, f64::INFINITY] {
        let mut bad_step = step();
        bad_step.dt_seconds = bad_dt;
        let mut operation = advance();
        if let FieldOperation::Advance { step, .. } = &mut operation {
            *step = &bad_step;
        }
        assert!(
            host.invoke_field(
                &kernel,
                buffer("fixture.context/v1", &[]),
                buffer("fixture.config/v1", &[0]),
                operation,
                OperationControl::new(Duration::from_secs(1)).unwrap()
            )
            .is_err()
        );
    }
}
