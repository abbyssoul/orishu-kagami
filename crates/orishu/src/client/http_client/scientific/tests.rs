use super::*;
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
