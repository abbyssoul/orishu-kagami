use orishu::model::{run::*, run_command::*};

fn identity(epoch: u64) -> RunIdentity {
    RunIdentity::new(
        "formation-a".parse().unwrap(),
        format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
        WorkloadEpoch::new(epoch),
    )
}
fn request(command: RunCommand) -> RunCommandRequest {
    RunCommandRequest::new("command-1".parse().unwrap(), identity(1), 3, command).unwrap()
}
fn status(request: &RunCommandRequest) -> RunStatus {
    let (boundary, phase) = match request.command() {
        RunCommand::Step => (4, RunPhase::Ready),
        RunCommand::Finish => (3, RunPhase::Finished),
    };
    RunStatus::new(
        RunDescriptor::new(request.run().clone()),
        boundary,
        2.0,
        phase,
    )
    .unwrap()
}
fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert_eq!(&serde_json::from_slice::<T>(&bytes).unwrap(), value);
    let mut bytes = vec![];
    ciborium::into_writer(value, &mut bytes).unwrap();
    assert_eq!(&ciborium::from_reader::<T, _>(&bytes[..]).unwrap(), value);
}

#[test]
fn commands_status_and_all_receipt_states_round_trip() {
    let query = RunStatusRequest::new(identity(1));
    roundtrip(&query);
    let json = serde_json::to_string(&query).unwrap();
    for bad in [
        json.replace("request/v1", "request/v2"),
        json.replace("\"run\":", "\"unknown\":0,\"run\":"),
        json.replace(
            "\"run\":",
            "\"apiVersion\":\"orishu.run-status-request/v1\",\"run\":",
        ),
    ] {
        assert!(serde_json::from_str::<RunStatusRequest>(&bad).is_err());
    }
    for command in [RunCommand::Step, RunCommand::Finish] {
        let request = request(command);
        roundtrip(&request);
        let status = status(&request);
        roundtrip(&status);
        for state in [
            RunCommandState::Pending,
            RunCommandState::Finished(RunCommandOutcome::Indeterminate),
            RunCommandState::Finished(RunCommandOutcome::Applied { status }),
            RunCommandState::Finished(RunCommandOutcome::Refused {
                reason: RunCommandRefusal::Busy,
            }),
            RunCommandState::Finished(RunCommandOutcome::Refused {
                reason: RunCommandRefusal::StaleBoundary { actual: 4 },
            }),
        ] {
            roundtrip(
                &RunCommandReceipt::new(request.clone(), "node-a".parse().unwrap(), state).unwrap(),
            );
        }
    }
}

#[test]
fn application_cannot_claim_another_run_boundary_or_action() {
    for command in [RunCommand::Step, RunCommand::Finish] {
        let request = request(command);
        let good = status(&request);
        for bad in [
            RunStatus::new(
                RunDescriptor::new(identity(2)),
                good.boundary(),
                2.0,
                good.phase(),
            )
            .unwrap(),
            RunStatus::new(good.descriptor().clone(), 8, 2.0, good.phase()).unwrap(),
            RunStatus::new(
                good.descriptor().clone(),
                good.boundary(),
                2.0,
                if good.phase() == RunPhase::Ready {
                    RunPhase::Finished
                } else {
                    RunPhase::Ready
                },
            )
            .unwrap(),
        ] {
            let state = RunCommandState::Finished(RunCommandOutcome::Applied { status: bad });
            assert!(
                RunCommandReceipt::new(request.clone(), "node-a".parse().unwrap(), state.clone())
                    .is_err()
            );
            let wire = serde_json::json!({"apiVersion":"orishu.run-command-receipt/v1",
                "request":request,"sourceNodeId":"node-a","state":state});
            assert!(serde_json::from_value::<RunCommandReceipt>(wire).is_err());
        }
        assert!(
            RunCommandReceipt::new(
                request.clone(),
                "node-a".parse().unwrap(),
                RunCommandState::Finished(RunCommandOutcome::Refused {
                    reason: RunCommandRefusal::StaleBoundary {
                        actual: request.expected_boundary()
                    }
                })
            )
            .is_err()
        );
    }
}

#[test]
fn request_and_status_invariants_apply_to_untrusted_serde() {
    for epoch in [0, 1] {
        let result = RunCommandRequest::new(
            "op".parse().unwrap(),
            identity(epoch),
            u64::MAX,
            RunCommand::Step,
        );
        assert!(result.is_err());
    }
    assert!(
        RunCommandRequest::new(
            "op".parse().unwrap(),
            identity(1),
            u64::MAX,
            RunCommand::Finish
        )
        .is_ok()
    );
    let request = request(RunCommand::Step);
    let json = serde_json::to_string(&request).unwrap();
    for bad in [
        json.replace("request/v1", "request/v2"),
        json.replace("\"step\"", "\"pause\""),
        json.replace("\"workloadEpoch\":1", "\"workloadEpoch\":0"),
        json.replace(
            "\"expectedBoundary\":3",
            &format!("\"expectedBoundary\":{}", u64::MAX),
        ),
        json.replace(
            "\"operationId\":",
            "\"operationId\":\"other\",\"operationId\":",
        ),
        json.replace("\"command\":", "\"plugin\":\"newtonian\",\"command\":"),
    ] {
        assert!(
            serde_json::from_str::<RunCommandRequest>(&bad).is_err(),
            "{bad}"
        );
    }
    let good = status(&request);
    let json = serde_json::to_string(&good).unwrap();
    for bad in [
        json.replace("status/v1", "status/v2"),
        json.replace("2.0", "-1.0"),
        json.replace("\"ready\"", "\"running\""),
        json.replace("\"workloadEpoch\":1", "\"workloadEpoch\":0"),
    ] {
        assert!(serde_json::from_str::<RunStatus>(&bad).is_err());
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(RunStatus::new(good.descriptor().clone(), 3, value, RunPhase::Ready).is_err());
        let wire = ciborium::value::Value::Map(vec![
            ("apiVersion".into(), "orishu.run-status/v1".into()),
            (
                "descriptor".into(),
                ciborium::value::Value::serialized(good.descriptor()).unwrap(),
            ),
            ("boundary".into(), 3.into()),
            ("timeSeconds".into(), ciborium::value::Value::Float(value)),
            ("phase".into(), "ready".into()),
        ]);
        let mut bytes = vec![];
        ciborium::into_writer(&wire, &mut bytes).unwrap();
        assert!(ciborium::from_reader::<RunStatus, _>(&bytes[..]).is_err());
    }
}
