//! Three real workers; independently identified collector ingress per worker.
use super::*;
use opentelemetry_proto::tonic::{
    common::v1::any_value::Value,
    trace::v1::{Span, span::SpanKind},
};
use orishu::model::cluster::{JoinOperationState, JoinRequest};
use orishu_worker::trace_context::TraceParent;
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Node {
    worker: Worker,
    socket: PathBuf,
    token: String,
    client: HttpClusterClient,
    initial: Summary,
}

struct Collectors(Vec<tokio::task::JoinHandle<()>>);
impl Drop for Collectors {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

async fn submit(node: &Node, request: &JoinRequest, parent: TraceParent, replay: bool) {
    let bytes = orishu_worker::peer::codec::encode(request).unwrap();
    let encoded = parent.encode();
    let head = format!(
        "POST /api/v1/membership/joins HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nAuthorization: Bearer {}\r\ntraceparent: {}\r\n\r\n",
        bytes.len(),
        node.token,
        std::str::from_utf8(&encoded).unwrap()
    );
    let mut stream = tokio::net::UnixStream::connect(&node.socket).await.unwrap();
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(&bytes).await.unwrap();
    let mut response = Vec::new();
    stream.take(8193).read_to_end(&mut response).await.unwrap();
    assert!(response.len() <= 8192);
    assert!(response.starts_with(if replay {
        b"HTTP/1.1 200"
    } else {
        b"HTTP/1.1 202"
    }));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = node
                .client
                .membership()
                .join_status(&request.operation_id)
                .await
                .unwrap();
            if let JoinOperationState::Joined { node_id } = status.state {
                let summary = node.client.cluster().summary().await.unwrap();
                assert_eq!(summary.source_node_id, node_id);
                assert_eq!(summary.formation_id, request.material.formation_id);
                assert!(summary.introducer_ready);
                break;
            }
            assert!(matches!(
                status.state,
                JoinOperationState::Connecting
                    | JoinOperationState::Admitting
                    | JoinOperationState::CatchingUp { .. }
            ));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("identified join/catch-up deadline");
}

fn assert_chain(
    spans: &[(usize, Span)],
    source: usize,
    receiver: usize,
    parent: TraceParent,
    server_expected: bool,
) {
    let correlated: Vec<_> = spans
        .iter()
        .filter(|(_, span)| span.trace_id == parent.trace_id())
        .collect();
    assert_eq!(correlated.len(), if server_expected { 3 } else { 2 });
    let request = correlated
        .iter()
        .find(|(role, span)| *role == source && span.name == "orishu.client.request")
        .unwrap();
    let peer = correlated
        .iter()
        .find(|(role, span)| *role == source && span.name == "orishu.peer.exchange")
        .unwrap();
    assert_eq!(request.1.parent_span_id, parent.span_id());
    assert_eq!(peer.1.parent_span_id, request.1.span_id);
    assert_eq!(peer.1.kind, SpanKind::Client as i32);
    if server_expected {
        let server = correlated
            .iter()
            .find(|(role, span)| *role == receiver && span.name == "orishu.admission")
            .unwrap();
        assert_eq!(server.1.parent_span_id, peer.1.span_id);
        assert_ne!(server.1.span_id, peer.1.span_id);
        assert_eq!(server.1.kind, SpanKind::Server as i32);
        assert_eq!(server.1.attributes[0].key, "orishu.outcome");
        assert_eq!(
            server.1.attributes[0].value.as_ref().unwrap().value,
            Some(Value::StringValue("completed".into()))
        );
    }
}

#[tokio::test]
async fn tracing_three_workers_receive_causal_parents_with_independent_runtime_controls() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let minimal = std::env::var_os("ORISHU_TEST_MINIMAL_WORKER");
        for (b_enabled, b_sampling, use_minimal) in [
            (true, "1000000", false),
            (true, "0", false),
            (false, "1000000", false),
            (true, "1000000", true),
        ] {
            if use_minimal && minimal.is_none() {
                continue;
            }
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let parents = [
                TraceParent::new([1; 16], [2; 8], true).unwrap(),
                TraceParent::new([3; 16], [4; 8], true).unwrap(),
                TraceParent::new([5; 16], [6; 8], true).unwrap(),
            ];
            let (received, mut receipt) = tokio::sync::mpsc::channel::<(usize, Span)>(16);
            let mut collectors = Collectors(Vec::new());
            let mut nodes = Vec::new();
            for role in 0..3 {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
                let state = root.path().join(format!("state-{role}"));
                let socket = root.path().join(format!("node-{role}.sock"));
                let binary = if role == 0 && use_minimal {
                    PathBuf::from(minimal.as_ref().unwrap())
                } else {
                    PathBuf::from(env!("CARGO_BIN_EXE_orishu-worker"))
                };
                let mut command = Command::new(binary);
                for (key, _) in std::env::vars_os() {
                    if key.to_string_lossy().starts_with("ORISHU_") {
                        command.env_remove(key);
                    }
                }
                command
                    .arg("--state-dir")
                    .arg(&state)
                    .arg("--listen.clients")
                    .arg(&socket)
                    .args(["--listen.peers", "127.0.0.1:0", "--accepts.peers", "true"]);
                if !(role == 0 && use_minimal) {
                    command.args([
                        "--tracing.enabled",
                        if role == 1 && !b_enabled {
                            "false"
                        } else {
                            "true"
                        },
                        "--tracing.sample-ppm",
                        if role == 1 { b_sampling } else { "1000000" },
                        "--tracing.endpoint",
                        &endpoint,
                        "--tracing.batch-size",
                        "1",
                        "--tracing.export-max-bytes",
                        "1024",
                        "--tracing.export-timeout-ms",
                        "1000",
                        "--tracing.shutdown-timeout-ms",
                        "100",
                    ]);
                }
                let mut worker = Worker(
                    command
                        .stdout(Stdio::null())
                        .stderr(Stdio::inherit())
                        .spawn()
                        .unwrap(),
                );
                let initial = summary(&mut worker, &socket).await;
                let token = std::fs::read_to_string(state.join("operator.token"))
                    .unwrap()
                    .trim()
                    .to_owned();
                let client = HttpClusterClient::new(
                    ClusterAddress::UnixSocket(socket.clone()),
                    HttClientOptions {
                        credentials: Some(orishu::client::Credentials::Token(token.clone())),
                        timeout: Some(Duration::from_secs(2)),
                        ..Default::default()
                    },
                )
                .unwrap();
                let collector_token = token.clone();
                let sender = received.clone();
                collectors.0.push(tokio::spawn(async move {
                    for _ in 0..1024 {
                        let span = tracing_context::receive(&listener, &collector_token).await;
                        // Only the two causal operations/replay and admission
                        // roots for the disabled-sender control enter this queue.
                        if span.name == "orishu.admission"
                            || parents
                                .iter()
                                .any(|parent| span.trace_id == parent.trace_id())
                        {
                            sender
                                .try_send((role, span))
                                .expect("bounded receipt count");
                        }
                    }
                    panic!("collector request budget exhausted");
                }));
                nodes.push(Node {
                    worker,
                    socket,
                    token,
                    client,
                    initial,
                });
            }
            let mut last = None;
            for source in 1..3 {
                let material = nodes[source - 1].client.get_join_token().await.unwrap();
                assert!(material.introducer_ready);
                let request = JoinRequest {
                    schema_version: 1,
                    operation_id: format!("join-{source}").parse().unwrap(),
                    formation_id: nodes[source].initial.formation_id.clone(),
                    material,
                };
                submit(&nodes[source], &request, parents[source - 1], false).await;
                last = Some(request);
            }
            submit(&nodes[2], last.as_ref().unwrap(), parents[2], true).await;
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let mut complete = true;
                    for node in &nodes {
                        let summary = node.client.cluster().summary().await.unwrap();
                        assert_eq!(summary.formation_id, nodes[0].initial.formation_id);
                        complete &= summary.member_count == 3
                            && summary.alive_count == 3
                            && summary.introducer_ready;
                    }
                    if complete {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("three-worker convergence deadline");
            let b_emitting = b_enabled && b_sampling != "0";
            let count = if b_emitting {
                if use_minimal { 6 } else { 7 }
            } else {
                4
            };
            let mut spans = Vec::new();
            for _ in 0..count {
                spans.push(
                    tokio::time::timeout(Duration::from_secs(2), receipt.recv())
                        .await
                        .unwrap()
                        .unwrap(),
                );
            }
            if b_emitting {
                assert_chain(&spans, 1, 0, parents[0], !use_minimal);
            } else {
                assert!(
                    spans
                        .iter()
                        .all(|(role, span)| *role != 1 && span.trace_id != parents[0].trace_id())
                );
                let root = spans
                    .iter()
                    .find(|(role, span)| *role == 0 && span.name == "orishu.admission")
                    .unwrap();
                assert!(root.1.parent_span_id.is_empty());
            }
            assert_chain(&spans, 2, 1, parents[1], b_emitting);
            let replay: Vec<_> = spans
                .iter()
                .filter(|(_, span)| span.trace_id == parents[2].trace_id())
                .collect();
            assert_eq!(replay.len(), 1);
            assert_eq!(replay[0].0, 2);
            assert_eq!(replay[0].1.name, "orishu.client.request");
            assert!(
                tokio::time::timeout(Duration::from_millis(150), receipt.recv())
                    .await
                    .is_err()
            );
            for node in &mut nodes {
                node.worker.terminate().await;
            }
            for task in collectors.0.drain(..) {
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            }
        }
    })
    .await
    .expect("three-worker trace receipt deadline");
}
