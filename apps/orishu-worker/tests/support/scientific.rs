use super::*;
use orishu::model::{ApiResponse, ResponseData, cluster::LockRequest, run_load::*};
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "../../../../crates/orishu-runtime/tests/reference_support/admission.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../../../../crates/orishu-runtime/tests/reference_support/mod.rs"]
mod reference_support;

fn start_scientific(state: &Path, socket: &Path, enabled: bool) -> Worker {
    let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("ORISHU_") {
            command.env_remove(name);
        }
    }
    Worker(
        command
            .arg("--state-dir")
            .arg(state)
            .arg("--listen.clients")
            .arg(socket)
            .args([
                "--scientific.enabled",
                if enabled { "true" } else { "false" },
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}
fn cbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
    orishu_worker::peer::codec::encode(value).unwrap()
}
fn decode(bytes: &[u8]) -> ApiResponse {
    orishu_worker::peer::codec::decode(bytes).unwrap()
}
fn headers(token: &str, media: &str) -> String {
    format!("Authorization: Bearer {token}\r\nContent-Type: {media}\r\n")
}
async fn send(
    socket: &Path,
    method: &str,
    path: &str,
    headers: &str,
    body: &[u8],
    announced: Option<u64>,
) -> (u16, Vec<u8>) {
    tokio::time::timeout(Duration::from_secs(8), async {
        let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
        let head = format!("{method} {path} HTTP/1.1\r\nHost: local\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n", announced.unwrap_or(body.len() as u64));
        stream.write_all(head.as_bytes()).await.unwrap();
        if !body.is_empty() { stream.write_all(body).await.unwrap(); }
        let mut bytes = Vec::new();
        stream.take(16385).read_to_end(&mut bytes).await.unwrap();
        assert!(bytes.len() <= 16384);
        let split = bytes.windows(4).position(|value| value == b"\r\n\r\n").expect("HTTP response");
        let head = std::str::from_utf8(&bytes[..split]).unwrap();
        let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        assert!(!head.to_ascii_lowercase().contains("transfer-encoding: chunked"));
        (status, bytes[split + 4..].to_vec())
    }).await.expect("bounded scientific response")
}
async fn lookup(socket: &Path, token: &str, request: &LoadRequest) -> Option<LoadReceipt> {
    let (status, body) = send(
        socket,
        "POST",
        "/api/v1/run-loads/lookup",
        &headers(token, "application/cbor"),
        &cbor(request),
        None,
    )
    .await;
    if status == 404 {
        return None;
    }
    assert_eq!(status, 200, "{:?}", decode(&body));
    match decode(&body) {
        ApiResponse::Ok {
            data: Some(ResponseData::RunLoadReceipt(receipt)),
        } => Some(receipt),
        other => panic!("{other:?}"),
    }
}
async fn finished(socket: &Path, token: &str, request: &LoadRequest) -> LoadReceipt {
    tokio::time::timeout(Duration::from_secs(80), async {
        loop {
            if let Some(receipt) = lookup(socket, token, request).await
                && receipt.state() != &LoadState::Pending
            {
                return receipt;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("scientific admission finished")
}
async fn lock(socket: &Path, token: &str, formation: &orishu_membership::FormationId) {
    let body = cbor(&LockRequest {
        schema_version: 1,
        operation_id: "lock-science".parse().unwrap(),
        formation_id: formation.clone(),
        locked: true,
    });
    let (status, _) = send(
        socket,
        "POST",
        "/api/v1/cluster/lock",
        &headers(token, "application/cbor"),
        &body,
        None,
    )
    .await;
    assert_eq!(status, 200);
}
fn framed(request: &LoadRequest, archive: &[u8]) -> Vec<u8> {
    let metadata = cbor(request);
    let mut body = (metadata.len() as u32).to_be_bytes().to_vec();
    body.extend(metadata);
    body.extend_from_slice(archive);
    body
}
fn root() -> orishu_workload::WorkloadDigest {
    format!("sha256:{}", "00".repeat(32)).parse().unwrap()
}

/// A real HTTP/2 GET with no Content-Length, including a possible DATA body.
async fn current_h2(socket: &Path, token: &str, data: &[u8]) -> ApiResponse {
    tokio::time::timeout(Duration::from_secs(8), async {
        let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        h2_frame(&mut stream, 4, 0, 0, &[]).await;
        let mut head = vec![0x82, 0x86]; // GET, http scheme.
        for (name, value) in [
            (":path", "/api/v1/run".to_owned()),
            (":authority", "localhost".to_owned()),
            ("authorization", format!("Bearer {token}")),
        ] {
            assert!(name.len() < 127 && value.len() < 127);
            head.extend_from_slice(&[0, name.len() as u8]);
            head.extend_from_slice(name.as_bytes());
            head.push(value.len() as u8);
            head.extend_from_slice(value.as_bytes());
        }
        h2_frame(&mut stream, 1, 4, 1, &head).await;
        h2_frame(&mut stream, 0, 1, 1, data).await;
        let mut bytes = Vec::new();
        let mut status_seen = false;
        for _ in 0..32 {
            let (kind, flags, id, payload) = read_h2_frame(&mut stream).await;
            if id == 0 {
                assert_eq!(kind, 4);
                if flags & 1 == 0 {
                    h2_frame(&mut stream, 4, 1, 0, &[]).await;
                }
                continue;
            }
            assert_eq!(id, 1);
            match kind {
                1 => {
                    // Static-table :status 200 and 400, on a fresh HPACK context.
                    assert_eq!(payload[0], if data.is_empty() { 0x88 } else { 0x8c });
                    status_seen = true;
                }
                0 => {
                    bytes.extend_from_slice(&payload);
                    assert!(bytes.len() <= 8192);
                }
                _ => panic!("unexpected HTTP/2 response frame"),
            }
            if flags & 1 != 0 {
                assert!(status_seen);
                return decode(&bytes);
            }
        }
        panic!("HTTP/2 response frame budget exhausted");
    })
    .await
    .expect("bounded current-run HTTP/2 response")
}

#[tokio::test]
async fn scientific_api_authenticates_bounds_framing_and_is_opt_in() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = dir.path().join("state");
    let socket = dir.path().join("api.sock");
    let mut worker = start_scientific(&state, &socket, false);
    summary(&mut worker, &socket).await;
    assert!(!state.join("workload-load-receipts.cbor").exists());
    assert_eq!(
        send(&socket, "GET", "/api/v1/run", "", &[], None).await.0,
        404
    );
    drop(worker);
    let mut worker = start_scientific(&state, &socket, true);
    let formation = summary(&mut worker, &socket).await.formation_id;
    let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
    for route in [
        "/api/v1/run-loads",
        "/api/v1/run-loads/lookup",
        "/api/v1/run",
    ] {
        let method = if route == "/api/v1/run" {
            "GET"
        } else {
            "POST"
        };
        for auth in [
            String::new(),
            headers("wrong", "application/cbor"),
            format!("Authorization: Bearer {token}\r\nAuthorization: Bearer {token}\r\n"),
        ] {
            assert_eq!(
                send(&socket, method, route, &auth, &[], Some(128 * 1024 * 1024))
                    .await
                    .0,
                401
            );
        }
    }
    let request = LoadRequest::new("malformed".parse().unwrap(), formation, root());
    let upload_headers = headers(&token, "application/vnd.orishu.run-load.v1");
    for (body, extra, announced, expected) in [
        (vec![], String::new(), Some(129 * 1024 * 1024), 413),
        (vec![0; 4], String::new(), None, 400),
        (vec![255; 5], String::new(), None, 400),
        (vec![0, 0, 0, 1, 255, 0], String::new(), None, 400),
        (
            framed(&request, &[0]),
            "Content-Encoding: gzip\r\n".into(),
            None,
            400,
        ),
        (
            framed(&request, &[0]),
            "If-Match: arbitrary\r\n".into(),
            None,
            400,
        ),
        (
            framed(&request, &[0]),
            "Trailer: arbitrary\r\n".into(),
            None,
            400,
        ),
        (
            framed(&request, &[0]),
            "Content-Type: application/cbor\r\n".into(),
            None,
            400,
        ),
    ] {
        assert_eq!(
            send(
                &socket,
                "POST",
                "/api/v1/run-loads",
                &(upload_headers.clone() + &extra),
                &body,
                announced
            )
            .await
            .0,
            expected
        );
        assert!(
            lookup(&socket, &token, &request).await.is_none(),
            "invalid framing must not reserve admission"
        );
    }
    let (status, body) = send(
        &socket,
        "GET",
        "/api/v1/run",
        &headers(&token, "application/cbor"),
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(matches!(
        decode(&body),
        ApiResponse::Ok {
            data: Some(ResponseData::RetainedRun(None))
        }
    ));
    // A valid body in an unlocked formation is a known refusal; no silent lock.
    assert!(matches!(
        current_h2(&socket, &token, &[]).await,
        ApiResponse::Ok { .. }
    ));
    assert!(!matches!(
        current_h2(&socket, &token, &[1]).await,
        ApiResponse::Ok { .. }
    ));
    send(
        &socket,
        "POST",
        "/api/v1/run-loads",
        &upload_headers,
        &framed(&request, &[0]),
        None,
    )
    .await;
    assert_eq!(
        finished(&socket, &token, &request).await.state(),
        &LoadState::Finished(LoadOutcome::Refused {
            reason: LoadRefusal::FormationUnavailable
        })
    );
    assert!(!summary(&mut worker, &socket).await.membership_locked);
    // The same application authentication/lookup path over actual HTTP/2.
    for authorized in [false, true] {
        let path = socket.clone();
        let request = cbor(&request);
        let credentials = format!(
            "Authorization: Bearer {}",
            if authorized { &token } else { "wrong" }
        );
        let bytes = tokio::task::spawn_blocking(move || {
            let mut stream = std::os::unix::net::UnixStream::connect(path).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            super::h2_authorization_response(
                &mut stream,
                "run-loads/lookup",
                &credentials,
                &request,
                false,
                authorized,
            )
        })
        .await
        .unwrap();
        assert!(matches!(decode(&bytes), ApiResponse::Ok { .. }) == authorized);
    }
}

#[tokio::test]
async fn real_scientific_upload_retains_acceptance_and_restart_recovers_pending() {
    let fixture = fixture::Fixture::new();
    let compiled = fixture.compile();
    let blobs = fixture.export(&compiled);
    let archive = orishu_plugin::workload::bundle::pack(
        compiled.manifest_bytes(),
        &fixture::borrowed(&blobs),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = dir.path().join("state");
    let socket = dir.path().join("api.sock");
    let mut worker = start_scientific(&state, &socket, true);
    let first = summary(&mut worker, &socket).await;
    let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
    lock(&socket, &token, &first.formation_id).await;
    let request = LoadRequest::new(
        "portable-load".parse().unwrap(),
        first.formation_id.clone(),
        compiled.verified().root(),
    );
    let client = orishu::client::http_client::HttpClusterClient::new(
        orishu::client::ClusterAddress::UnixSocket(socket.clone()),
        orishu::client::http_client::HttClientOptions {
            credentials: Some(orishu::client::Credentials::Token(token.clone())),
            ..Default::default()
        },
    )
    .unwrap();
    let upload_headers = headers(&token, "application/vnd.orishu.run-load.v1");
    // Deliberately discard the response body. The client knows the original ID,
    // so it retrieves the receipt instead of inventing a new operation.
    let _ = client.scientific().submit(&request, archive).await.unwrap();
    let accepted = finished(&socket, &token, &request).await;
    assert_eq!(
        client.scientific().lookup(&request).await.unwrap(),
        Some(accepted.clone())
    );
    let LoadState::Finished(LoadOutcome::Accepted { descriptor }) = accepted.state() else {
        panic!("{accepted:?}");
    };
    assert_eq!(
        descriptor.identity().workload_id(),
        compiled.verified().root()
    );
    assert_eq!(descriptor.identity().workload_epoch().get(), 1);
    assert_eq!(
        client.scientific().current().await.unwrap(),
        Some(descriptor.clone())
    );
    let occupied = summary(&mut worker, &socket).await;
    assert_eq!(occupied.schema_version, 2);
    assert_eq!(
        occupied.workload,
        orishu::model::cluster::FormationWorkload::Scientific
    );
    let (status, body) = send(
        &socket,
        "GET",
        "/api/v1/run",
        &headers(&token, "application/cbor"),
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        matches!(decode(&body), ApiResponse::Ok { data: Some(ResponseData::RetainedRun(Some(current))) } if current == *descriptor)
    );
    let (status, body) = send(
        &socket,
        "POST",
        "/api/v1/run-loads",
        &upload_headers,
        &framed(&request, &[0]),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        matches!(decode(&body), ApiResponse::Ok { data: Some(ResponseData::RunLoadReceipt(receipt)) } if receipt == accepted)
    );
    let conflict = LoadRequest::new(
        request.operation_id().clone(),
        request.formation_id().clone(),
        root(),
    );
    assert_eq!(
        send(
            &socket,
            "POST",
            "/api/v1/run-loads/lookup",
            &headers(&token, "application/cbor"),
            &cbor(&conflict),
            None
        )
        .await
        .0,
        409
    );
    drop(worker);
    let mut worker = start_scientific(&state, &socket, true);
    let second = summary(&mut worker, &socket).await;
    assert_ne!(second.formation_id, first.formation_id);
    assert_eq!(second.schema_version, 1);
    assert_eq!(
        second.workload,
        orishu::model::cluster::FormationWorkload::None
    );
    assert_eq!(
        lookup(&socket, &token, &request).await,
        Some(accepted.clone())
    );
    assert_eq!(
        client.scientific().lookup(&request).await.unwrap(),
        Some(accepted)
    );
    assert_eq!(client.scientific().current().await.unwrap(), None);
    let (_, body) = send(
        &socket,
        "GET",
        "/api/v1/run",
        &headers(&token, "application/cbor"),
        &[],
        None,
    )
    .await;
    assert!(matches!(
        decode(&body),
        ApiResponse::Ok {
            data: Some(ResponseData::RetainedRun(None))
        }
    ));
    lock(&socket, &token, &second.formation_id).await;
    let pending = LoadRequest::new(
        "crash-pending".parse().unwrap(),
        second.formation_id,
        compiled.verified().root(),
    );
    let prefix = framed(&pending, &[0]);
    let mut input = tokio::net::UnixStream::connect(&socket).await.unwrap();
    input.write_all(format!("POST /api/v1/run-loads HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n{upload_headers}\r\n", prefix.len() + 100).as_bytes()).await.unwrap();
    input.write_all(&prefix).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if lookup(&socket, &token, &pending).await.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(worker);
    drop(input);
    let mut worker = start_scientific(&state, &socket, true);
    summary(&mut worker, &socket).await;
    assert_eq!(
        lookup(&socket, &token, &pending).await.unwrap().state(),
        &LoadState::Finished(LoadOutcome::Indeterminate)
    );
}

#[tokio::test]
async fn scientific_configuration_precedence_and_invalid_values_are_explicit() {
    for (file, environment, cli, enabled) in [
        (true, "false", None, false),
        (false, "true", None, true),
        (true, "true", Some("false"), false),
        (false, "false", Some("true"), true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let state = dir.path().join("state");
        let socket = dir.path().join("api.sock");
        let config = dir.path().join("worker.yaml");
        std::fs::write(
            &config,
            format!("spec:\n  scientific:\n    enabled: {file}\n"),
        )
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ORISHU_") {
                command.env_remove(name);
            }
        }
        command
            .env("ORISHU_SCIENTIFIC_ENABLED", environment)
            .arg("--config")
            .arg(&config)
            .arg("--state-dir")
            .arg(&state)
            .arg("--listen.clients")
            .arg(&socket);
        if let Some(cli) = cli {
            command.args(["--scientific.enabled", cli]);
        }
        let mut worker = Worker(
            command
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        summary(&mut worker, &socket).await;
        assert_eq!(state.join("workload-load-receipts.cbor").exists(), enabled);
        assert_eq!(
            send(&socket, "GET", "/api/v1/run", "", &[], None).await.0,
            if enabled { 401 } else { 404 }
        );
    }
    for yaml in [
        "spec:\n  scientific:\n    enabeld: true\n",
        "spec:\n  scientific:\n    enabled: invalid\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("worker.yaml");
        let state = dir.path().join("state");
        std::fs::write(&config, yaml).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
            .arg("--config")
            .arg(&config)
            .arg("--state-dir")
            .arg(&state)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!state.exists());
    }
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let output = Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
        .env("ORISHU_SCIENTIFIC_ENABLED", "invalid")
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!state.exists());
}

#[tokio::test]
async fn scientific_lookup_requires_operator_authority_over_tls_http1_and_http2() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let cert = dir.path().join("server.pem");
    let key = dir.path().join("server.key");
    std::fs::write(&cert, certificate.cert.pem()).unwrap();
    std::fs::write(&key, certificate.signing_key.serialize_pem()).unwrap();
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let state = dir.path().join("state");
    let socket = dir.path().join("api.sock");
    let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("ORISHU_") {
            command.env_remove(name);
        }
    }
    let mut worker = Worker(
        command
            .arg("--state-dir")
            .arg(&state)
            .arg("--listen.clients")
            .arg(&socket)
            .args([
                "--listen.clients",
                &address.to_string(),
                "--scientific.enabled",
                "true",
            ])
            .arg("--tls-cert")
            .arg(&cert)
            .arg("--tls-key")
            .arg(&key)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let formation = summary(&mut worker, &socket).await.formation_id;
    let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
    let request = LoadRequest::new("tls-known-refusal".parse().unwrap(), formation, root());
    send(
        &socket,
        "POST",
        "/api/v1/run-loads",
        &headers(&token, "application/vnd.orishu.run-load.v1"),
        &framed(&request, &[0]),
        None,
    )
    .await;
    let receipt = finished(&socket, &token, &request).await;
    // The product client also exercises configured trust and operator credentials,
    // independently of the explicit HTTP/1 + HTTP/2 wire fixtures below.
    for authorized in [false, true] {
        let client = orishu::client::http_client::HttpClusterClient::new(
            orishu::client::ClusterAddress::Ip(address),
            orishu::client::http_client::HttClientOptions {
                credentials: Some(orishu::client::Credentials::Token(if authorized {
                    token.clone()
                } else {
                    "wrong".into()
                })),
                tls_cert: Some(cert.clone()),
                timeout: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        )
        .unwrap();
        let result = client.scientific().lookup(&request).await;
        if authorized {
            assert_eq!(result.unwrap(), Some(receipt.clone()));
        } else {
            assert!(matches!(
                result,
                Err(orishu::client::http_client::ScientificError::Http { status: 401, .. })
            ));
        }
    }
    for http2 in [false, true] {
        for authorized in [false, true] {
            let mut roots = rustls::RootCertStore::empty();
            roots.add(certificate.cert.der().clone()).unwrap();
            let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
            config.alpn_protocols = vec![if http2 {
                b"h2".to_vec()
            } else {
                b"http/1.1".to_vec()
            }];
            let request = cbor(&request);
            let credential = if authorized {
                token.clone()
            } else {
                "wrong".into()
            };
            let bytes = tokio::task::spawn_blocking(move || {
                use std::io::{Read, Write};
                let socket = std::net::TcpStream::connect(address).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap(); socket.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
                let connection = rustls::ClientConnection::new(Arc::new(config), "127.0.0.1".try_into().unwrap()).unwrap();
                let mut stream = rustls::StreamOwned::new(connection, socket);
                if http2 { return super::h2_authorization_response(&mut stream, "run-loads/lookup", &format!("Authorization: Bearer {credential}"), &request, true, authorized); }
                write!(stream, "POST /api/v1/run-loads/lookup HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nAuthorization: Bearer {credential}\r\n\r\n", request.len()).unwrap();
                stream.write_all(&request).unwrap(); stream.flush().unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") { assert!(head.len() < 8192); let mut byte = [0]; stream.read_exact(&mut byte).unwrap(); head.push(byte[0]); }
                let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                assert!(head.starts_with(if authorized { "http/1.1 200" } else { "http/1.1 401" }));
                let length: usize = head.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse().unwrap();
                assert!(length <= 4096); let mut bytes = vec![0; length]; stream.read_exact(&mut bytes).unwrap(); bytes
            }).await.unwrap();
            match decode(&bytes) {
                ApiResponse::Ok {
                    data: Some(ResponseData::RunLoadReceipt(found)),
                } if authorized => assert_eq!(found, receipt),
                ApiResponse::Error { code, .. } if !authorized => assert_eq!(code, "Unauthorized"),
                other => panic!("{other:?}"),
            }
        }
    }
}
