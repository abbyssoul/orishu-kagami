#[allow(dead_code)]
mod support;

use orishu_plugin::{source::SourcePayload, *};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn json_payload(payload: &Payload) -> Value {
    match payload {
        Payload::Components(p) => serde_json::to_value(p),
        Payload::Fields(p) => serde_json::to_value(p),
        Payload::Observables(p) => serde_json::to_value(p),
        Payload::Constants(p) => serde_json::to_value(p),
        Payload::FieldModels(p) => serde_json::to_value(p),
        Payload::Integrators(p) => serde_json::to_value(p),
    }
    .unwrap()
}

#[test]
fn local_aliases_lower_to_the_same_exact_scientific_and_payload_bytes() {
    let limits = Limits::default();
    let declarations = support::declarations();
    let contracts: BTreeMap<LocalContributionId, _> = declarations
        .iter()
        .map(|(id, p)| (id.parse().unwrap(), p.contract_ref(&limits).unwrap()))
        .collect();
    let artifacts = BTreeMap::from([
        (
            "classical".parse().unwrap(),
            ArtifactDigest::sha256_of(b"classical-kernel-fixture"),
        ),
        (
            "euler".parse().unwrap(),
            ArtifactDigest::sha256_of(b"euler-kernel-fixture"),
        ),
    ]);
    for (_, payload) in declarations {
        let mut source = json_payload(&payload);
        for req in source["scientific"]["requirements"].as_array_mut().unwrap() {
            let id = contracts
                .iter()
                .find(|(_, contract)| serde_json::to_value(contract).unwrap() == req["contract"])
                .unwrap()
                .0;
            req["contract"] = json!({"localContribution":id});
        }
        if let Some(kernel) = source["scientific"].get_mut("kernel") {
            let id = artifacts
                .iter()
                .find(|(_, digest)| serde_json::to_value(digest).unwrap() == *kernel)
                .unwrap()
                .0;
            *kernel = json!({"localArtifact":id});
        }
        let draft = SourcePayload::read(
            payload.point(),
            &serde_json::to_vec(&source).unwrap(),
            &limits,
        )
        .unwrap();
        let lowered = draft.compile(&contracts, &artifacts, &limits).unwrap();
        assert_eq!(
            lowered.canonical_bytes(&limits).unwrap(),
            payload.canonical_bytes(&limits).unwrap()
        );
        assert_eq!(
            lowered.contract_ref(&limits).unwrap(),
            payload.contract_ref(&limits).unwrap()
        );
    }
}

#[test]
fn source_reader_rejects_duplicate_keys_null_and_over_budget_inputs_before_lowering() {
    let limits = Limits::default();
    for bytes in [
        br#"{"scientific":{},"scientific":{}}"#.as_slice(),
        br#"{"scientific":null}"#,
        br#"{"scientific":{"requirements":[{"contract":{"localContribution":"a","name":"org.example.a"}}]}}"#,
        br#"{"scientific":{"requirements":[{"contract":{"localContribution":"../a"}}]}}"#,
    ] {
        assert!(SourcePayload::read(KnownPoint::Constants, bytes, &limits).is_err());
    }
    let limits = Limits {
        max_requirements: 1,
        ..limits
    };
    let error = SourcePayload::read(KnownPoint::Constants,
        br#"{"scientific":{"requirements":[{"slot":"one","contract":{"localContribution":"a"}},{broken]}}"#,
        &limits).unwrap_err();
    assert_eq!(error.code, ErrorCode::LimitExceeded);
    let bytes = serde_json::to_vec(&json_payload(&support::declarations()[0].1)).unwrap();
    for limits in [
        Limits {
            max_payload_bytes: bytes.len() - 1,
            ..limits
        },
        Limits {
            max_values: 2,
            ..limits
        },
        Limits {
            max_depth: 1,
            ..limits
        },
    ] {
        assert_eq!(
            SourcePayload::read(KnownPoint::Components, &bytes, &limits)
                .unwrap_err()
                .code,
            ErrorCode::LimitExceeded
        );
    }
}

#[test]
fn aliases_require_explicit_inputs_and_cannot_bypass_scientific_validation() {
    let limits = Limits::default();
    let (_, original) = support::declarations()
        .into_iter()
        .find(|(_, p)| p.point() == KnownPoint::Integrators)
        .unwrap();
    let mut source = json_payload(&original);
    source["scientific"]["kernel"] = json!({"localArtifact":"code"});
    let read = |source: &Value| {
        SourcePayload::read(
            original.point(),
            &serde_json::to_vec(source).unwrap(),
            &limits,
        )
        .unwrap()
    };
    assert!(
        read(&source)
            .compile(&BTreeMap::new(), &BTreeMap::new(), &limits)
            .is_err()
    );
    let artifacts = BTreeMap::from([(
        "code".parse().unwrap(),
        ArtifactDigest::sha256_of(b"euler-kernel-fixture"),
    )]);
    assert_eq!(
        read(&source)
            .compile(&BTreeMap::new(), &artifacts, &limits)
            .unwrap(),
        original
    );
    source["scientific"]["requirements"][0]["contract"] = json!({"localContribution":"dynamics"});
    assert!(
        read(&source)
            .compile(&BTreeMap::new(), &artifacts, &limits)
            .is_err()
    );
    let wrong = BTreeMap::from([(
        "dynamics".parse().unwrap(),
        original.contract_ref(&limits).unwrap(),
    )]);
    source["scientific"]["executionContract"] = json!("orishu:simulation/field@1");
    assert!(read(&source).compile(&wrong, &artifacts, &limits).is_err());
}

#[test]
fn presentation_artifact_alias_does_not_enter_the_scientific_digest() {
    let limits = Limits::default();
    let original = support::declarations().remove(0).1;
    let mut source = json_payload(&original);
    source["presentation"] = json!({"iconArtifact":{"localArtifact":"icon"}});
    let artifacts = BTreeMap::from([("icon".parse().unwrap(), ArtifactDigest::sha256_of(b"icon"))]);
    let lowered = SourcePayload::read(
        original.point(),
        &serde_json::to_vec(&source).unwrap(),
        &limits,
    )
    .unwrap()
    .compile(&BTreeMap::new(), &artifacts, &limits)
    .unwrap();
    assert_eq!(
        lowered.contract_ref(&limits).unwrap(),
        original.contract_ref(&limits).unwrap()
    );
    assert_ne!(
        lowered.canonical_bytes(&limits).unwrap(),
        original.canonical_bytes(&limits).unwrap()
    );
}

#[test]
fn a_compact_alias_does_not_evade_the_expanded_payload_byte_budget() {
    let limits = Limits::default();
    let (_, field) = support::declarations()
        .into_iter()
        .find(|(_, p)| p.point() == KnownPoint::Fields)
        .unwrap();
    let contract = field.requirements()[0].contract.clone();
    let mut source = json_payload(&field);
    source["scientific"]["requirements"][0]["contract"] = json!({"localContribution":"channel"});
    let bytes = serde_json::to_vec(&source).unwrap();
    let limits = Limits {
        max_payload_bytes: bytes.len(),
        ..limits
    };
    let draft = SourcePayload::read(field.point(), &bytes, &limits).unwrap();
    let contracts = BTreeMap::from([("channel".parse().unwrap(), contract)]);
    assert_eq!(
        draft
            .compile(&contracts, &BTreeMap::new(), &limits)
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
}
