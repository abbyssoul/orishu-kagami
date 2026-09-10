//! Authenticated parent adoption through the executable API and actual OTLP bytes.
use super::*;
use orishu_worker::trace_context::TraceParent;
use prost::Message;
use std::os::unix::fs::PermissionsExt;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

pub(super) async fn receive(
    listener: &TcpListener,
    token: &str,
) -> opentelemetry_proto::tonic::trace::v1::Span {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        assert!(head.len() < 4096);
        head.push(stream.read_u8().await.unwrap());
    }
    let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
    assert!(head.starts_with("post /v1/traces http/1.1\r\n"));
    assert!(!head.contains(token));
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(length <= 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(!text.contains(token) && !text.contains("secret-marker"));
    let mut request =
        opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest::decode(
            body.as_slice(),
        )
        .unwrap();
    assert_eq!(request.resource_spans.len(), 1);
    assert_eq!(request.resource_spans[0].scope_spans.len(), 1);
    let spans = &mut request.resource_spans[0].scope_spans[0].spans;
    assert_eq!(spans.len(), 1);
    let span = spans.pop().unwrap();
    assert!(matches!(
        span.name.as_str(),
        "orishu.client.request" | "orishu.peer.exchange" | "orishu.admission"
    ));
    assert_eq!(span.attributes.len(), 1);
    assert_eq!(span.flags & 1, 1);
    assert!(span.trace_state.is_empty());
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
        )
        .await
        .unwrap();
    span
}

pub(super) async fn request(socket: &Path, mutation: bool, headers: &str) -> Vec<u8> {
    let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
    let route = if mutation {
        "POST /api/v1/cluster/lock"
    } else {
        "GET /api/v1/cluster"
    };
    stream.write_all(format!("{route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n{headers}\r\n").as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.take(8193).read_to_end(&mut response).await.unwrap();
    assert!(response.len() <= 8192);
    let split = response
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap();
    let head = String::from_utf8_lossy(&response[..split]).to_ascii_lowercase();
    assert!(
        !head.contains("traceparent") && !head.contains("tracestate") && !head.contains("baggage")
    );
    response
}

#[tokio::test]
async fn authenticated_context_reaches_collector_and_disabled_modes_ignore_it() {
    tokio::time::timeout(Duration::from_secs(20), async {
        for (enabled, sampling) in [(true, "1000000"), (true, "0"), (false, "1000000")] {
            let emitting = enabled && sampling != "0";
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("worker.sock");
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
            for (key, _) in std::env::vars_os() {
                if key.to_string_lossy().starts_with("ORISHU_") { command.env_remove(key); }
            }
            let mut worker = Worker(command.arg("--state-dir").arg(&state)
                .arg("--listen.clients").arg(&socket)
                .args(["--name", "authored-secret-marker", "--tracing.enabled", if enabled { "true" } else { "false" },
                    "--tracing.endpoint", &endpoint, "--tracing.sample-ppm", sampling,
                    "--tracing.batch-size", "1", "--tracing.export-max-bytes", "1024",
                    "--tracing.export-timeout-ms", "1000", "--tracing.shutdown-timeout-ms", "100"])
                .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let before = summary(&mut worker, &socket).await;
            let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
            let token = token.trim();
            if emitting { assert!(receive(&listener, token).await.parent_span_id.is_empty()); }
            let auth = format!("Authorization: Bearer {token}\r\n");
            let parent = format!("TrAcEpArEnT: {PARENT}\r\n");
            let expected = TraceParent::parse(PARENT.as_bytes()).unwrap();
            for (label, headers, adopted, mutation) in [
                ("valid", format!("{auth}{parent}tracestate: vendor=secret-marker\r\nbaggage: secret-marker\r\n"), true, false),
                ("unsampled", format!("{auth}traceparent: {}00\r\n", &PARENT[..53]), true, false),
                ("future", format!("{auth}traceparent: 01{}-secret-marker\r\n", &PARENT[2..]), true, false),
                ("missing auth", parent.clone(), false, false),
                ("wrong auth", format!("Authorization: Bearer wrong-secret-marker\r\n{parent}"), false, false),
                ("duplicate auth", format!("{auth}{auth}{parent}"), false, false),
                ("missing parent", auth.clone(), false, false),
                ("duplicate parent", format!("{auth}{parent}{parent}"), false, false),
                ("combined parent", format!("{auth}traceparent: {PARENT},{PARENT}\r\n"), false, false),
                ("oversized parent", format!("{auth}traceparent: 01{}-{}\r\n", &PARENT[2..], "x".repeat(73)), false, false),
                ("malformed", format!("{auth}traceparent: invalid-secret-marker\r\n"), false, false),
                ("unauthorized mutation", parent.clone(), false, true),
                ("duplicate-auth mutation", format!("{auth}{auth}{parent}"), false, true),
                ("independent root", String::new(), false, false),
            ] {
                let response = request(&socket, mutation, &headers).await;
                assert!(response.starts_with(if mutation { b"HTTP/1.1 401" } else { b"HTTP/1.1 200" }), "{label}");
                if emitting {
                    let span = receive(&listener, token).await;
                    assert_eq!(span.attributes[0].key, "orishu.outcome");
                    assert_eq!(span.attributes[0].value.as_ref().unwrap().value,
                        Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                            if mutation { "rejected" } else { "completed" }.into()
                        )), "{label}");
                    if adopted {
                        assert_eq!(span.trace_id, expected.trace_id(), "{label}");
                        assert_eq!(span.parent_span_id, expected.span_id(), "{label}");
                    } else {
                        assert_ne!(span.trace_id, expected.trace_id(), "{label}");
                        assert!(span.parent_span_id.is_empty(), "{label}");
                    }
                    assert_ne!(span.span_id, expected.span_id(), "{label}");
                }
            }
            let after = summary(&mut worker, &socket).await;
            assert_eq!(after, before, "diagnostic inputs changed formation state");
            if emitting { assert!(receive(&listener, token).await.parent_span_id.is_empty()); }
            worker.terminate().await;
            assert!(tokio::time::timeout(Duration::from_millis(200), listener.accept()).await.is_err());
        }
    }).await.expect("authenticated trace context process deadline");
}

#[tokio::test]
async fn tracing_join_exchange_retains_client_parent_and_replay_starts_no_peer_work() {
    use orishu::model::cluster::{JoinOperationState, JoinRequest};
    tokio::time::timeout(Duration::from_secs(30), async {
        for (enabled, sampling) in [(true, "1000000"), (true, "0"), (false, "1000000")] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let target_state = root.path().join("target");
            let target_socket = root.path().join("target.sock");
            let source_state = root.path().join("source");
            let source_socket = root.path().join("source.sock");
            let command = || {
                let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
                for (key, _) in std::env::vars_os() {
                    if key.to_string_lossy().starts_with("ORISHU_") { command.env_remove(key); }
                }
                command.stdout(Stdio::null()).stderr(Stdio::inherit());
                command
            };
            let mut target = Worker(command().arg("--state-dir").arg(&target_state)
                .arg("--listen.clients").arg(&target_socket)
                .args(["--listen.peers", "127.0.0.1:0", "--accepts.peers", "true"]).spawn().unwrap());
            let target_initial = summary(&mut target, &target_socket).await;
            let client = |socket, token| HttpClusterClient::new(ClusterAddress::UnixSocket(socket), HttClientOptions {
                credentials: Some(orishu::client::Credentials::Token(token)), timeout: Some(Duration::from_secs(2)), ..Default::default()
            }).unwrap();
            let target_token = std::fs::read_to_string(target_state.join("operator.token")).unwrap().trim().to_owned();
            let material = client(target_socket.clone(), target_token).get_join_token().await.unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
            let mut source = Worker(command().arg("--state-dir").arg(&source_state)
                .arg("--listen.clients").arg(&source_socket)
                .args(["--tracing.enabled", if enabled { "true" } else { "false" }, "--tracing.endpoint", &endpoint,
                    "--tracing.sample-ppm", sampling, "--tracing.batch-size", "1", "--tracing.export-max-bytes", "1024",
                    "--tracing.export-timeout-ms", "1000", "--tracing.shutdown-timeout-ms", "100"])
                .spawn().unwrap());
            let initial = summary(&mut source, &source_socket).await;
            let token = std::fs::read_to_string(source_state.join("operator.token")).unwrap().trim().to_owned();
            let expected = TraceParent::parse(PARENT.as_bytes()).unwrap();
            let replay_parent = TraceParent::new([3;16], [4;8], true).unwrap();
            let collector_token = token.clone();
            let (received, mut spans) = tokio::sync::mpsc::channel(8);
            let received_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let collector_count = received_count.clone();
            let collector = tokio::spawn(async move {
                for _ in 0..512 {
                    let span = receive(&listener, &collector_token).await;
                    collector_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if span.trace_id == expected.trace_id() || span.trace_id == replay_parent.trace_id() {
                        received.try_send(span).expect("bounded relevant span count");
                    }
                }
                panic!("collector request budget exhausted");
            });
            let join = JoinRequest { schema_version: 1, operation_id: "trace-join".parse().unwrap(),
                formation_id: initial.formation_id, material };
            let body = orishu_worker::peer::codec::encode(&join).unwrap();
            for (parent, replay) in [(expected, false), (replay_parent, true)] {
                let mut stream = tokio::net::UnixStream::connect(&source_socket).await.unwrap();
                let encoded_parent = parent.encode();
                let head = format!("POST /api/v1/membership/joins HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nAuthorization: Bearer {token}\r\ntraceparent: {}\r\n\r\n", body.len(), std::str::from_utf8(&encoded_parent).unwrap());
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
                let mut response = Vec::new();
                stream.take(8193).read_to_end(&mut response).await.unwrap();
                assert!(response.len() <= 8192);
                assert!(response.starts_with(if replay { b"HTTP/1.1 200" } else { b"HTTP/1.1 202" }));
                let authorized = client(source_socket.clone(), token.clone());
                tokio::time::timeout(Duration::from_secs(8), async {
                    loop {
                        let status = authorized.membership().join_status(&join.operation_id).await.unwrap();
                        if matches!(status.state, JoinOperationState::Joined { .. }) { break; }
                        assert!(matches!(status.state, JoinOperationState::Connecting | JoinOperationState::Admitting | JoinOperationState::CatchingUp { .. }));
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }).await.expect("join convergence deadline");
            }
            let formed = summary(&mut source, &source_socket).await;
            assert_eq!(formed.formation_id, target_initial.formation_id);
            assert_eq!(formed.member_count, 2);
            if enabled && sampling != "0" {
                let mut matched = Vec::new();
                for _ in 0..3 {
                    matched.push(tokio::time::timeout(Duration::from_secs(2), spans.recv()).await.unwrap().unwrap());
                }
                let request = matched.iter().find(|span| span.trace_id == expected.trace_id() && span.name == "orishu.client.request").unwrap();
                let peer = matched.iter().find(|span| span.name == "orishu.peer.exchange").unwrap();
                assert_eq!(request.parent_span_id, expected.span_id());
                assert_eq!(peer.trace_id, request.trace_id);
                assert_eq!(peer.parent_span_id, request.span_id);
                assert_ne!(peer.span_id, request.span_id);
                assert_eq!(matched.iter().filter(|span| span.trace_id == replay_parent.trace_id()).count(), 1);
            } else {
                assert_eq!(received_count.load(std::sync::atomic::Ordering::Relaxed), 0);
            }
            assert!(tokio::time::timeout(Duration::from_millis(150), spans.recv()).await.is_err());
            source.terminate().await;
            target.terminate().await;
            collector.abort();
            assert!(collector.await.unwrap_err().is_cancelled());
        }
    }).await.expect("client to owned peer exchange deadline");
}
