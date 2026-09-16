use orishu::model::{
    run::{RunDescriptor, RunIdentity, WorkloadEpoch},
    run_load::*,
};

fn request() -> LoadRequest {
    LoadRequest::new(
        "load-1".parse().unwrap(),
        "formation-a".parse().unwrap(),
        format!("sha256:{}", "00".repeat(32)).parse().unwrap(),
    )
}

#[test]
fn shared_receipts_round_trip_and_reject_cross_identity_acceptance() {
    let request = request();
    for state in [
        LoadState::Pending,
        LoadState::Finished(LoadOutcome::Indeterminate),
        LoadState::Finished(LoadOutcome::Refused {
            reason: LoadRefusal::InvalidWorkload,
        }),
        LoadState::Finished(LoadOutcome::Accepted {
            descriptor: RunDescriptor::new(RunIdentity::new(
                request.formation_id().clone(),
                request.workload_id(),
                WorkloadEpoch::new(1),
            )),
        }),
    ] {
        let receipt = LoadReceipt::new(request.clone(), "node-a".parse().unwrap(), state).unwrap();
        let json = serde_json::to_vec(&receipt).unwrap();
        assert_eq!(
            serde_json::from_slice::<LoadReceipt>(&json).unwrap(),
            receipt
        );
        let mut cbor = Vec::new();
        ciborium::into_writer(&receipt, &mut cbor).unwrap();
        assert_eq!(
            ciborium::from_reader::<LoadReceipt, _>(&cbor[..]).unwrap(),
            receipt
        );
    }
    for (formation, digest, epoch) in [
        ("formation-b", request.workload_id(), 1),
        (
            "formation-a",
            format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
            1,
        ),
        ("formation-a", request.workload_id(), 0),
    ] {
        let state = LoadState::Finished(LoadOutcome::Accepted {
            descriptor: RunDescriptor::new(RunIdentity::new(
                formation.parse().unwrap(),
                digest,
                WorkloadEpoch::new(epoch),
            )),
        });
        assert!(
            LoadReceipt::new(request.clone(), "node-a".parse().unwrap(), state.clone()).is_err()
        );
        let wire = serde_json::json!({"apiVersion":"orishu.run-load-receipt/v1", "request":request, "sourceNodeId":"node-a", "state":state});
        assert!(serde_json::from_value::<LoadReceipt>(wire).is_err());
    }
}

#[test]
fn strict_version_shape_and_identifiers_have_no_distribution_authority() {
    let request = request();
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        format!(
            r#"{{"apiVersion":"orishu.run-load-request/v1","operationId":"load-1","formationId":"formation-a","workloadId":"sha256:{}"}}"#,
            "00".repeat(32)
        )
    );
    for bad in [
        json.replace("/v1", "/v2"),
        json.replace("load-1", ""),
        json.replace("load-1", "../../load"),
        json.replace("formation-a", " "),
        json.replace("sha256:", "SHA256:"),
        json.replace("\"operationId\":", "\"length\":4,\"operationId\":"),
        json.replace(
            "\"operationId\":",
            "\"operationId\":\"load-2\",\"operationId\":",
        ),
    ] {
        assert!(serde_json::from_str::<LoadRequest>(&bad).is_err());
    }
    let receipt = LoadReceipt::new(request, "node-a".parse().unwrap(), LoadState::Pending).unwrap();
    let json = serde_json::to_string(&receipt).unwrap();
    for bad in [
        json.replace("receipt/v1", "receipt/v2"),
        json.replace("\"pending\"", "\"unknown\""),
        json.replace(
            "\"pending\"",
            "\"pending\",\"outcome\":{\"state\":\"indeterminate\"}",
        ),
        json.replace(
            "\"sourceNodeId\":",
            "\"sourceNodeId\":\"other\",\"sourceNodeId\":",
        ),
        json.replace(
            "\"sourceNodeId\":",
            "\"cachePath\":\"/tmp\",\"sourceNodeId\":",
        ),
    ] {
        assert!(serde_json::from_str::<LoadReceipt>(&bad).is_err(), "{bad}");
    }
}
