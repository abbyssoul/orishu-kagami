use super::*;
use crate::model::run_command::{RunCommand, RunCommandOutcome, RunPhase};
use crate::{
    client::{bearer_auth::BearerMiddleware, cbor_middleware::CborContentMiddleware},
    model::{
        ApiResponse, ResponseData,
        run::{RunIdentity, WorkloadEpoch},
        run_load::{LoadOutcome, LoadRefusal},
    },
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

fn command_intent() -> RunCommandRequest {
    RunCommandRequest::new(
        "step-1".parse().unwrap(),
        descriptor(&intent()).identity().clone(),
        0,
        RunCommand::Step,
    )
    .unwrap()
}

fn object_fixture(query: &ObjectObservationRequest) -> (Vec<u8>, orishu_workload::ArtifactDigest) {
    use orishu_plugin::{FiniteF64, execution::*};
    let mut objects = Vec::new();
    encode_batch::<ObjectState>(&[], &mut objects, BulkLimits::default()).unwrap();
    let mut forces = Vec::new();
    encode_batch::<Force>(&[], &mut forces, BulkLimits::default()).unwrap();
    let source = SnapshotSource::Committed {
        workload: query.run().workload_id(),
        run: RunDescriptor::new(query.run().clone()).digest().unwrap(),
        epoch: query.run().workload_epoch().get(),
        boundary: query.boundary(),
        time_seconds: FiniteF64::new(0.5).unwrap(),
    };
    let mut bytes = Vec::new();
    let digest = encode_object_observation(
        &source,
        &objects,
        (query.boundary() != 0).then_some(forces.as_slice()),
        &mut bytes,
        object_limits(),
    )
    .unwrap();
    (bytes, digest)
}

fn field_fixture() -> (FieldObservationRequest, FieldObservation, Vec<u8>, Vec<u8>) {
    use orishu_plugin::{ArtifactDigest, Declaration, FiniteF64, Limits, Payload, execution::*};
    let intent =
        FieldObservationRequest::new(command_intent().run().clone(), 1, "field".parse().unwrap());
    let digest = ArtifactDigest::sha256_of(b"fixture");
    let mut context: InstanceContext = serde_json::from_value(serde_json::json!({
        "apiVersion":INSTANCE_SCHEMA,"instance":"field","kernel":digest,
        "contribution":{"release":digest,"extensionPoint":"orishu.compute.field-models/v1","localId":"field"},
        "scientific":{"name":"org.test.field","version":1,"digest":digest},
        "executionContract":"orishu:simulation/field@1",
        "stateFormat":{"id":"org.test.state","version":1},
        "profile":"orishu.force-then-integrate/v1",
        "configuration":{"schema":"org.test.config/v1","byteLength":3,"valueCount":1,"digest":digest},
        "domain":{"schema":"org.test.domain/v1","byteLength":6,"valueCount":1,"digest":digest},
        "couplings":[],"observables":[],"computePrecision":"binary64",
        "bounds":{"projectionRecords":5,"stateBytes":1000,"samplePoints":100,"sampleChannels":4}
    })).unwrap();
    let schema = orishu_plugin::ObservableSchema {
        name: "org.test.potential".parse().unwrap(),
        version: 1.try_into().unwrap(),
        requirements: vec![],
        meaning: "test scalar potential".into(),
        shape: orishu_plugin::Shape::Scalar,
        dimension: orishu_plugin::Dimension::DIMENSIONLESS,
        frame: "world".into(),
        axes: vec![],
        conventions: "SI".into(),
    };
    let contract = Payload::Observables(Declaration {
        scientific: schema.clone(),
        presentation: None,
    })
    .contract_ref(&Limits::default())
    .unwrap();
    let channel = SampleChannel { contract, schema };
    context.observables.push(ObservableBinding {
        slot: "potential".parse().unwrap(),
        channel: channel.clone(),
        quality_flags: 1,
    });
    let field = FieldObservation::new(
        SampleSnapshot {
            source: SnapshotSource::Committed {
                workload: intent.run().workload_id(),
                run: RunDescriptor::new(intent.run().clone()).digest().unwrap(),
                epoch: intent.run().workload_epoch().get(),
                boundary: 1,
                time_seconds: FiniteF64::new(0.5).unwrap(),
            },
            state: InputIdentity::of("org.test.state/v1".parse().unwrap(), 1, b"field"),
        },
        context,
    )
    .unwrap();
    let metadata = field.request(9, vec![channel]).unwrap();
    let mut query = vec![];
    encode_sample_request(
        &metadata,
        &[SamplePoint {
            id: 5,
            position_metres: [FiniteF64::new(0.0).unwrap(); 3],
        }],
        &mut query,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let request = SampleRequest::read(
        &query,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let mut bytes = vec![];
    let mut output = SampleOutput::new(
        &mut bytes,
        &request,
        &field.context,
        SampleLimits::default(),
    )
    .unwrap();
    output
        .valid(0, 0, &[FiniteF64::new(42.0).unwrap()], 1)
        .unwrap();
    output.finish().unwrap();
    (intent, field, query, bytes)
}
fn observation_response(media: &str, bytes: Vec<u8>) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", media)
        .insert_header(
            OBJECT_OBSERVATION_DIGEST_HEADER,
            orishu_workload::ArtifactDigest::sha256_of(&bytes).to_string(),
        )
        .set_body_bytes(bytes)
}

#[tokio::test]
async fn field_queries_keep_exact_intent_points_context_and_channels() {
    let (intent, field, query, bytes) = field_fixture();
    let server = MockServer::start().await;
    for (route, request_media, response_media, payload) in [
        (
            "/api/v1/run/field",
            "application/cbor",
            FIELD_OBSERVATION_MEDIA_TYPE,
            field.to_cbor().unwrap(),
        ),
        (
            "/api/v1/run/samples",
            FIELD_SAMPLE_REQUEST_MEDIA_TYPE,
            FIELD_SAMPLE_RESPONSE_MEDIA_TYPE,
            bytes.clone(),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .and(header("authorization", "Bearer operator-secret"))
            .and(header("content-type", request_media))
            .and(header("accept", response_media))
            .respond_with(observation_response(response_media, payload))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = client(&server);
    assert_eq!(
        client.scientific().field(&intent).await.unwrap(),
        Some(field.clone())
    );
    assert_eq!(
        client
            .scientific()
            .samples(&intent, &field, &query)
            .await
            .unwrap(),
        Some(bytes)
    );
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        ciborium::from_reader::<FieldObservationRequest, _>(&calls[0].body[..]).unwrap(),
        intent
    );
    let size = u32::from_be_bytes(calls[1].body[..4].try_into().unwrap()) as usize;
    assert_eq!(
        ciborium::from_reader::<FieldObservationRequest, _>(&calls[1].body[4..4 + size]).unwrap(),
        intent
    );
    assert_eq!(&calls[1].body[4 + size..], query);
}

#[tokio::test]
async fn field_client_rejects_cross_scope_descriptors_and_query_substitution() {
    use orishu_plugin::{ArtifactDigest, FiniteF64, execution::*};
    let (intent, field, query, bytes) = field_fixture();
    for case in 0..7 {
        let mut wrong = field.clone();
        match case {
            0 => wrong.context.instance = "other-field".parse().unwrap(),
            1 => {
                if let SnapshotSource::Committed { run, .. } = &mut wrong.snapshot.source {
                    *run = ArtifactDigest::sha256_of(b"another formation");
                }
            }
            2 => {
                if let SnapshotSource::Committed { workload, .. } = &mut wrong.snapshot.source {
                    *workload = ArtifactDigest::sha256_of(b"another root")
                        .to_string()
                        .parse()
                        .unwrap();
                }
            }
            3 => {
                if let SnapshotSource::Committed { epoch, .. } = &mut wrong.snapshot.source {
                    *epoch += 1;
                }
            }
            4 => {
                if let SnapshotSource::Committed { boundary, .. } = &mut wrong.snapshot.source {
                    *boundary += 1;
                }
            }
            _ => {}
        }
        let mut payload = wrong.to_cbor().unwrap();
        if case == 5 {
            payload.push(0);
        }
        if case == 6 {
            payload = vec![0; MAX_FIELD_OBSERVATION_BYTES + 1];
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(observation_response(FIELD_OBSERVATION_MEDIA_TYPE, payload))
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            client(&server).scientific().field(&intent).await.is_err(),
            "{case}"
        );
    }
    for case in 0..3 {
        let mut payload = bytes.clone();
        match case {
            0 => payload[4] ^= 1, // request digest even with a matching transport digest
            1 => payload.truncate(payload.len() - 1),
            2 => payload[36..40].copy_from_slice(&32_u32.to_le_bytes()),
            _ => unreachable!(),
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(observation_response(
                FIELD_SAMPLE_RESPONSE_MEDIA_TYPE,
                payload,
            ))
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            client(&server)
                .scientific()
                .samples(&intent, &field, &query)
                .await
                .is_err()
        );
    }
    let server = MockServer::start().await;
    for case in 0..4 {
        let mut wrong = field.clone();
        match case {
            0 => wrong.snapshot.state.digest = ArtifactDigest::sha256_of(b"another state"),
            1 => wrong.context.kernel = ArtifactDigest::sha256_of(b"another model"),
            2 => {
                if let SnapshotSource::Committed { time_seconds, .. } = &mut wrong.snapshot.source {
                    *time_seconds = FiniteF64::new(0.75).unwrap();
                }
            }
            3 => wrong.context.bounds.sample_points = 0,
            _ => unreachable!(),
        }
        assert!(matches!(
            client(&server)
                .scientific()
                .samples(&intent, &wrong, &query)
                .await,
            Err(ScientificError::Input(_))
        ));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn fields_distinguish_absence_stale_capacity_and_missing_api() {
    let (intent, field, query, _) = field_fixture();
    for (status, code) in [
        (404, "RunUnavailable"),
        (404, "NotFound"),
        (409, "StaleBoundary"),
        (503, "ObserverBusy"),
        (503, "SamplingUnavailable"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response(
                status,
                &ApiResponse::Error {
                    code: code.into(),
                    message: code.into(),
                },
            ))
            .expect(2)
            .mount(&server)
            .await;
        let client = client(&server);
        let descriptor = client.scientific().field(&intent).await;
        let samples = client.scientific().samples(&intent, &field, &query).await;
        if code == "RunUnavailable" {
            assert!(descriptor.unwrap().is_none());
            assert!(samples.unwrap().is_none());
        } else {
            assert!(descriptor.is_err());
            assert!(samples.is_err());
        }
    }
}

#[tokio::test]
async fn objects_preserve_exact_read_intent_and_validate_payload_without_retry() {
    let query = ObjectObservationRequest::new(command_intent().run().clone(), 1);
    let (bytes, digest) = object_fixture(&query);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/run/objects"))
        .and(header("authorization", "Bearer operator-secret"))
        .and(header("content-type", "application/cbor"))
        .and(header("accept", OBJECT_OBSERVATION_MEDIA_TYPE))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(bytes.clone())
                .insert_header("content-type", OBJECT_OBSERVATION_MEDIA_TYPE)
                .insert_header(OBJECT_OBSERVATION_DIGEST_HEADER, digest.to_string()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = client(&server)
        .scientific()
        .objects(&query)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.bytes(), bytes);
    assert_eq!(result.digest(), digest);
    assert_eq!(result.view().force_evaluation_boundary(), Some(0));
    assert!(query.matches(result.view().source()));
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        ciborium::from_reader::<ObjectObservationRequest, _>(&calls[0].body[..]).unwrap(),
        query
    );
}

#[tokio::test]
async fn objects_reject_cross_scope_corruption_missing_digest_and_wrong_media() {
    let query = ObjectObservationRequest::new(command_intent().run().clone(), 1);
    for case in 0..10 {
        let fixture_query = if case == 0 {
            ObjectObservationRequest::new(query.run().clone(), 2)
        } else if case >= 7 {
            ObjectObservationRequest::new(
                RunIdentity::new(
                    if case == 7 {
                        "other-formation".parse().unwrap()
                    } else {
                        query.run().formation_id().clone()
                    },
                    if case == 8 {
                        orishu_workload::ArtifactDigest::sha256_of(b"other-root")
                            .to_string()
                            .parse()
                            .unwrap()
                    } else {
                        query.run().workload_id()
                    },
                    WorkloadEpoch::new(if case == 9 {
                        2
                    } else {
                        query.run().workload_epoch().get()
                    }),
                ),
                query.boundary(),
            )
        } else {
            query.clone()
        };
        let (mut bytes, digest) = object_fixture(&fixture_query);
        if case == 1 {
            *bytes.last_mut().unwrap() ^= 1;
        }
        if case == 5 {
            // Keep the fixture transport internally consistent: an explicit
            // mismatched Content-Length makes Hyper panic before the client can
            // exercise its header ceiling.
            bytes.resize(MAX_OBJECT_RESPONSE_BYTES + 1, 0);
        }
        let mut response = ResponseTemplate::new(if case == 6 { 202 } else { 200 })
            .set_body_bytes(bytes)
            .insert_header(
                "content-type",
                if case == 3 {
                    "application/cbor"
                } else {
                    OBJECT_OBSERVATION_MEDIA_TYPE
                },
            );
        if case != 2 {
            response = response.insert_header(OBJECT_OBSERVATION_DIGEST_HEADER, digest.to_string());
        }
        if case == 4 {
            response = response.insert_header("content-encoding", "gzip");
        }
        if case == 5 {
            response = response.insert_header(
                "content-length",
                (MAX_OBJECT_RESPONSE_BYTES + 1).to_string(),
            );
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            client(&server).scientific().objects(&query).await.is_err(),
            "case {case}"
        );
    }
}

#[tokio::test]
async fn objects_distinguish_absence_stale_capacity_and_missing_api() {
    let query = ObjectObservationRequest::new(command_intent().run().clone(), 1);
    for (status, code) in [
        (404, "RunUnavailable"),
        (404, "NotFound"),
        (409, "StaleBoundary"),
        (503, "ObserverBusy"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response(
                status,
                &ApiResponse::Error {
                    code: code.into(),
                    message: code.into(),
                },
            ))
            .expect(1)
            .mount(&server)
            .await;
        let result = client(&server).scientific().objects(&query).await;
        if code == "RunUnavailable" {
            assert!(result.unwrap().is_none());
        } else {
            assert!(result.is_err());
        }
    }
}
fn command_fact(request: &RunCommandRequest) -> RunCommandReceipt {
    RunCommandReceipt::new(
        request.clone(),
        "historic-node".parse().unwrap(),
        RunCommandState::Finished(RunCommandOutcome::Applied {
            status: RunStatus::new(
                RunDescriptor::new(request.run().clone()),
                request.expected_boundary() + 1,
                0.5,
                RunPhase::Ready,
            )
            .unwrap(),
        }),
    )
    .unwrap()
}

#[tokio::test]
async fn objects_refuse_truncated_missing_length_and_stalled_wire_bodies() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let query = ObjectObservationRequest::new(command_intent().run().clone(), 1);
    for case in 0..3 {
        let (bytes, digest) = object_fixture(&query);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (done, stop) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            let mut byte = [0];
            while !head.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).await.unwrap();
                head.push(byte[0]);
                assert!(head.len() <= 8192);
            }
            let length = if case == 2 {
                String::new()
            } else {
                format!("Content-Length: {}\r\n", bytes.len())
            };
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {OBJECT_OBSERVATION_MEDIA_TYPE}\r\n{OBJECT_OBSERVATION_DIGEST_HEADER}: {digest}\r\n{length}Connection: close\r\n\r\n").as_bytes()).await.unwrap();
            stream.write_all(&bytes[..1]).await.unwrap();
            if case != 0 {
                let _ = stop.await;
            }
        });
        let client = HttpClusterClient {
            client: reqwest_middleware::ClientBuilder::new(
                super::super::control_transport(reqwest::Client::builder())
                    .build()
                    .unwrap(),
            )
            .build(),
            base_url: format!("http://{address}/api/v1/").parse().unwrap(),
        };
        let result =
            tokio::time::timeout(Duration::from_secs(8), client.scientific().objects(&query))
                .await
                .unwrap();
        if case == 1 {
            assert!(matches!(result, Err(ScientificError::Deadline)));
        } else {
            assert!(result.is_err());
        }
        let _ = done.send(());
        server.await.unwrap();
    }
}

#[tokio::test]
async fn commands_preserve_exact_intent_correlate_receipts_and_status_without_retry() {
    let request = command_intent();
    let receipt = command_fact(&request);
    let RunCommandState::Finished(RunCommandOutcome::Applied { status }) = receipt.state() else {
        unreachable!()
    };
    let query = RunStatusRequest::new(request.run().clone());
    let server = MockServer::start().await;
    for (route, data) in [
        (
            "run-commands",
            ResponseData::RunCommandReceipt(receipt.clone()),
        ),
        (
            "run-commands/lookup",
            ResponseData::RunCommandReceipt(receipt.clone()),
        ),
        ("run/status", ResponseData::RunStatus(status.clone())),
    ] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v1/{route}")))
            .and(header("content-type", "application/cbor"))
            .and(header("authorization", "Bearer operator-secret"))
            .respond_with(response(200, &ok(data)))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = client(&server);
    assert_eq!(
        client.scientific().command(&request).await.unwrap(),
        receipt
    );
    assert_eq!(
        client.scientific().command_lookup(&request).await.unwrap(),
        Some(receipt.clone())
    );
    assert_eq!(
        client.scientific().status(&query).await.unwrap(),
        Some(status.clone())
    );
    for call in server.received_requests().await.unwrap() {
        assert_eq!(call.headers["content-length"], call.body.len().to_string());
        if call.url.path().ends_with("status") {
            assert_eq!(
                ciborium::from_reader::<RunStatusRequest, _>(&call.body[..]).unwrap(),
                query
            );
        } else {
            assert_eq!(
                ciborium::from_reader::<RunCommandRequest, _>(&call.body[..]).unwrap(),
                request
            );
        }
    }
}

#[tokio::test]
async fn command_clients_reject_cross_intent_and_incorrect_http_facts() {
    let request = command_intent();
    let other = RunCommandRequest::new(
        "other".parse().unwrap(),
        request.run().clone(),
        0,
        RunCommand::Step,
    )
    .unwrap();
    for (status, receipt) in [
        (200, command_fact(&other)),
        (202, command_fact(&request)),
        (201, command_fact(&request)),
        (409, command_fact(&request)),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response(
                status,
                &ok(ResponseData::RunCommandReceipt(receipt)),
            ))
            .expect(2)
            .mount(&server)
            .await;
        let client = client(&server);
        assert!(client.scientific().command(&request).await.is_err());
        assert!(client.scientific().command_lookup(&request).await.is_err());
    }
    let wrong = RunStatus::new(
        RunDescriptor::new(RunIdentity::new(
            request.run().formation_id().clone(),
            request.run().workload_id(),
            WorkloadEpoch::new(2),
        )),
        1,
        0.5,
        RunPhase::Ready,
    )
    .unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(response(200, &ok(ResponseData::RunStatus(wrong))))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        client(&server)
            .scientific()
            .status(&RunStatusRequest::new(request.run().clone()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn command_lookup_and_status_distinguish_absence_from_missing_api_and_uncertainty() {
    let request = command_intent();
    for code in [
        "OperationNotFound",
        "RunUnavailable",
        "NotFound",
        "OutcomeUnknown",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response(
                404,
                &ApiResponse::Error {
                    code: code.into(),
                    message: code.into(),
                },
            ))
            .expect(2)
            .mount(&server)
            .await;
        let client = client(&server);
        let receipt = client.scientific().command_lookup(&request).await;
        assert_eq!(receipt.is_ok(), code == "OperationNotFound");
        let status = client
            .scientific()
            .status(&RunStatusRequest::new(request.run().clone()))
            .await;
        assert_eq!(status.is_ok(), code == "RunUnavailable");
    }
}

#[test]
fn finite_float_profile_is_explicit_and_keeps_structural_preflight() {
    let fact = encoded(&ok(ResponseData::RunCommandReceipt(command_fact(
        &command_intent(),
    ))));
    assert!(codec::validate(&fact).is_err());
    assert!(codec::validate_profile(&fact, true).is_ok());
    for end in 0..fact.len() {
        assert!(codec::validate_profile(&fact[..end], true).is_err());
    }
    for bytes in [
        b"\xf9\x7c\x00".as_slice(),
        b"\xfa\x7f\x80\x00\x00",
        b"\xfb\x7f\xf8\x00\x00\x00\x00\x00\x00",
        b"\xf4",
        b"\xbf\x61x\x00\x61x\x01\xff",
        b"\x9a\xff\xff\xff\xff",
    ] {
        assert!(codec::validate_profile(bytes, true).is_err());
    }
    for bytes in [
        b"\xf9\x38\x00".as_slice(),
        b"\xfa\x3f\x00\x00\x00",
        b"\xfb\x3f\xe0\x00\x00\x00\x00\x00\x00",
    ] {
        assert!(codec::validate_profile(bytes, true).is_ok());
        assert!(codec::validate(bytes).is_err());
    }
}

fn client(server: &MockServer) -> HttpClusterClient {
    HttpClusterClient {
        client: reqwest_middleware::ClientBuilder::new(
            super::super::control_transport(reqwest::Client::builder())
                .build()
                .unwrap(),
        )
        .with(CborContentMiddleware::default())
        .with(BearerMiddleware::with_token("operator-secret"))
        .build(),
        base_url: format!("{}/api/v1/", server.uri()).parse().unwrap(),
    }
}
fn intent() -> LoadRequest {
    LoadRequest::new(
        "load-1".parse().unwrap(),
        "formation-a".parse().unwrap(),
        format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
    )
}
fn descriptor(request: &LoadRequest) -> RunDescriptor {
    RunDescriptor::new(RunIdentity::new(
        request.formation_id().clone(),
        request.workload_id(),
        WorkloadEpoch::new(1),
    ))
}
fn receipt(request: &LoadRequest, state: LoadState) -> LoadReceipt {
    LoadReceipt::new(request.clone(), "historic-node".parse().unwrap(), state).unwrap()
}
fn encoded(value: &impl serde::Serialize) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).unwrap();
    bytes
}
fn response(status: u16, value: &ApiResponse) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(encoded(value), "application/cbor")
}
fn ok(data: ResponseData) -> ApiResponse {
    ApiResponse::Ok { data: Some(data) }
}

#[tokio::test]
async fn complete_upload_preserves_media_framing_and_original_intent_without_retry() {
    let server = MockServer::start().await;
    let request = intent();
    let pending = receipt(&request, LoadState::Pending);
    Mock::given(method("POST"))
        .and(path("/api/v1/run-loads"))
        .and(header("content-type", RUN_LOAD_MEDIA_TYPE))
        .and(header("authorization", "Bearer operator-secret"))
        .respond_with(response(
            202,
            &ok(ResponseData::RunLoadReceipt(pending.clone())),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let bundle = vec![42; 80_000];
    assert_eq!(
        client(&server)
            .scientific()
            .submit(&request, bundle.clone())
            .await
            .unwrap(),
        pending
    );
    let calls = server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 1);
    let body = &calls[0].body;
    let count = u32::from_be_bytes(body[..4].try_into().unwrap()) as usize;
    assert_eq!(
        ciborium::from_reader::<LoadRequest, _>(&body[4..4 + count]).unwrap(),
        request
    );
    assert_eq!(&body[4 + count..], bundle);
    assert_eq!(calls[0].headers["content-length"], body.len().to_string());
    assert!(!calls[0].headers.contains_key("transfer-encoding"));
}

#[tokio::test]
async fn receipt_states_are_facts_and_lookup_only_maps_the_exact_not_found_code() {
    let request = intent();
    for state in [
        LoadState::Pending,
        LoadState::Finished(LoadOutcome::Indeterminate),
        LoadState::Finished(LoadOutcome::Refused {
            reason: LoadRefusal::InvalidWorkload,
        }),
        LoadState::Finished(LoadOutcome::Accepted {
            descriptor: descriptor(&request),
        }),
    ] {
        let server = MockServer::start().await;
        let fact = receipt(&request, state);
        Mock::given(method("POST"))
            .and(path("/api/v1/run-loads/lookup"))
            .and(header("content-type", "application/cbor"))
            .respond_with(response(
                200,
                &ok(ResponseData::RunLoadReceipt(fact.clone())),
            ))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            client(&server).scientific().lookup(&request).await.unwrap(),
            Some(fact)
        );
    }
    for (status, code, absent) in [
        (404, "OperationNotFound", true),
        (404, "NotFound", false),
        (409, "OperationConflict", false),
        (503, "OutcomeUnknown", false),
        (401, "Unauthorized", false),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response(
                status,
                &ApiResponse::Error {
                    code: code.into(),
                    message: code.into(),
                },
            ))
            .expect(1)
            .mount(&server)
            .await;
        let result = client(&server).scientific().lookup(&request).await;
        if absent {
            assert_eq!(result.unwrap(), None);
        } else {
            assert!(
                matches!(result, Err(ScientificError::Http { status: found, .. }) if found == status)
            );
        }
    }
}

#[tokio::test]
async fn cross_identity_and_misleading_http_outcomes_never_become_acceptance() {
    let request = intent();
    let altered = [
        LoadRequest::new(
            "other".parse().unwrap(),
            request.formation_id().clone(),
            request.workload_id(),
        ),
        LoadRequest::new(
            request.operation_id().clone(),
            "other".parse().unwrap(),
            request.workload_id(),
        ),
        LoadRequest::new(
            request.operation_id().clone(),
            request.formation_id().clone(),
            format!("sha256:{}", "02".repeat(32)).parse().unwrap(),
        ),
    ];
    for other in altered {
        let server = MockServer::start().await;
        let wrong = receipt(
            &other,
            LoadState::Finished(LoadOutcome::Accepted {
                descriptor: descriptor(&other),
            }),
        );
        Mock::given(method("POST"))
            .respond_with(response(200, &ok(ResponseData::RunLoadReceipt(wrong))))
            .expect(2)
            .mount(&server)
            .await;
        let client = client(&server);
        assert!(matches!(
            client.scientific().submit(&request, vec![1]).await,
            Err(ScientificError::Protocol(_))
        ));
        assert!(matches!(
            client.scientific().lookup(&request).await,
            Err(ScientificError::Protocol(_))
        ));
    }
    for status in [201, 202, 204, 302, 409, 500] {
        let server = MockServer::start().await;
        let fact = receipt(
            &request,
            LoadState::Finished(LoadOutcome::Accepted {
                descriptor: descriptor(&request),
            }),
        );
        Mock::given(method("POST"))
            .respond_with(response(status, &ok(ResponseData::RunLoadReceipt(fact))))
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            client(&server)
                .scientific()
                .submit(&request, vec![1])
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn current_descriptor_is_live_discovery_not_a_historical_receipt() {
    for epoch in [None, Some(0), Some(1)] {
        let server = MockServer::start().await;
        let request = intent();
        let run = epoch.map(|n| {
            RunDescriptor::new(RunIdentity::new(
                request.formation_id().clone(),
                request.workload_id(),
                WorkloadEpoch::new(n),
            ))
        });
        Mock::given(method("GET"))
            .and(path("/api/v1/run"))
            .and(header("authorization", "Bearer operator-secret"))
            .respond_with(response(200, &ok(ResponseData::RetainedRun(run.clone()))))
            .expect(1)
            .mount(&server)
            .await;
        let result = client(&server).scientific().current().await;
        if epoch == Some(0) {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), run);
        }
    }
}

#[tokio::test]
async fn malformed_oversized_unknown_and_wrong_media_replies_are_bounded_refusals() {
    let request = intent();
    let valid = encoded(&ok(ResponseData::RunLoadReceipt(receipt(
        &request,
        LoadState::Pending,
    ))));
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut nested = vec![];
    for _ in 0..14 {
        nested.extend_from_slice(b"\xa1\x61x");
    }
    nested.push(0);
    let mut extra = serde_json::to_value(ok(ResponseData::RunLoadReceipt(receipt(
        &request,
        LoadState::Pending,
    ))))
    .unwrap();
    extra["Ok"]["extra"] = serde_json::json!(1);
    for (bytes, media) in [
        (trailing, "application/cbor"),
        (nested, "application/cbor"),
        (vec![0; MAX_RESPONSE_BYTES + 1], "application/cbor"),
        (valid.clone(), "application/json"),
        (encoded(&extra), "application/cbor"),
        (encoded(&ApiResponse::Ok { data: None }), "application/cbor"),
        (
            b"\xa1\x62Ok\xbf\x64data\xf6\x64data\xf6\xff".to_vec(),
            "application/cbor",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes, media))
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            client(&server).scientific().lookup(&request).await,
            Err(ScientificError::Protocol(_))
        ));
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(valid, "application/cbor")
                .insert_header("Content-Encoding", "gzip"),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(client(&server).scientific().lookup(&request).await.is_err());
    assert!(matches!(
        client(&server).scientific().submit(&request, vec![]).await,
        Err(ScientificError::Input(_))
    ));
}

#[test]
fn preflight_refuses_declared_lengths_duplicates_depth_and_truncation_before_serde() {
    for data in [
        b"\x7b\xff\xff\xff\xff\xff\xff\xff\xff".as_slice(),
        b"\xbf\x61x\x00\x78\x01x\x01\xff",
        b"\xb8\xff",
        b"\xbb\xff\xff\xff\xff\xff\xff\xff\xff\xff",
        b"\x9a\xff\xff\xff\xff",
        b"\xf4",
        b"\xc0\x00",
        b"\x41\x00",
    ] {
        assert!(codec::validate(data).is_err());
    }
    let bytes = encoded(&ok(ResponseData::RunLoadReceipt(receipt(
        &intent(),
        LoadState::Pending,
    ))));
    assert!(codec::validate(&bytes).is_ok());
    for end in 0..bytes.len() {
        assert!(codec::validate(&bytes[..end]).is_err());
    }
}

#[tokio::test]
async fn redirects_never_move_intent_or_credentials_and_deadlines_bound_the_exchange() {
    let source = MockServer::start().await;
    let target = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("Location", format!("{}/api/v1/run-loads", target.uri()))
                .set_body_raw(
                    encoded(&ok(ResponseData::RunLoadReceipt(receipt(
                        &intent(),
                        LoadState::Pending,
                    )))),
                    "application/cbor",
                ),
        )
        .expect(1)
        .mount(&source)
        .await;
    assert!(
        client(&source)
            .scientific()
            .submit(&intent(), vec![1])
            .await
            .is_err()
    );
    assert!(target.received_requests().await.unwrap().is_empty());

    let source = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            response(200, &ok(ResponseData::RetainedRun(None))).set_delay(Duration::from_secs(1)),
        )
        .expect(1)
        .mount(&source)
        .await;
    let client = client(&source);
    let request = client.client.get(client.base_url.join("run").unwrap());
    assert!(matches!(
        exchange(request, Duration::from_millis(100)).await,
        Err(ScientificError::Deadline)
    ));
}

#[tokio::test]
async fn actual_chunked_body_is_capped_without_a_length_and_stalled_body_expires() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    for stalled in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (done, stop) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            let mut byte = [0];
            while !head.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).await.unwrap();
                head.push(byte[0]);
                assert!(head.len() <= 8192);
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/cbor\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
            if stalled {
                stream.write_all(b"1\r\n\xa1\r\n").await.unwrap();
            } else {
                let bytes = vec![0; MAX_RESPONSE_BYTES + 1];
                let chunk = format!("{:x}\r\n", bytes.len());
                stream.write_all(chunk.as_bytes()).await.unwrap();
                stream.write_all(&bytes).await.unwrap();
                stream.write_all(b"\r\n0\r\n\r\n").await.unwrap();
            }
            let _ = stop.await;
        });
        let client = HttpClusterClient {
            client: reqwest_middleware::ClientBuilder::new(
                super::super::control_transport(reqwest::Client::builder())
                    .build()
                    .unwrap(),
            )
            .build(),
            base_url: format!("http://{address}/api/v1/").parse().unwrap(),
        };
        let result = tokio::time::timeout(Duration::from_secs(8), client.scientific().current())
            .await
            .unwrap();
        if stalled {
            assert_eq!(result.unwrap_err(), ScientificError::Deadline);
        } else {
            assert_eq!(
                result.unwrap_err(),
                ScientificError::Protocol("response byte limit")
            );
        }
        let _ = done.send(());
        server.await.unwrap();
    }
}
