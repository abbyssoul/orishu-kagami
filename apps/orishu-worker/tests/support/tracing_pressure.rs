//! One stalled export, one queued record, and real authenticated operator work.
use super::*;
use std::{io::Read, os::unix::fs::PermissionsExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn receive(listener: &tokio::net::TcpListener) -> tokio::net::TcpStream {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        assert!(head.len() < 4096);
        head.push(stream.read_u8().await.unwrap());
    }
    let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(length <= 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    use prost::Message;
    let request =
        opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest::decode(
            body.as_slice(),
        )
        .unwrap();
    assert_eq!(request.resource_spans[0].scope_spans[0].spans.len(), 1);
    stream
}

async fn accept(stream: &mut tokio::net::TcpStream) {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
        )
        .await
        .unwrap();
}

#[cfg(feature = "observability")]
async fn assert_scrape(address: std::net::SocketAddr, enqueued: u64, accepted: u64, token: &str) {
    let response = diagnostics_request_bounded(address, "/metrics", 32768).await;
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("content-type: text/plain; version=0.0.4"));
    assert!(!response.contains(token));
    let body = response.split_once("\r\n\r\n").unwrap().1;
    assert_eq!(
        body.lines().filter(|line| !line.starts_with('#')).count(),
        163
    );
    let traces: Vec<_> = body
        .lines()
        .filter(|line| line.starts_with("orishu_worker_trace_"))
        .collect();
    assert_eq!(traces.len(), 12);
    for (name, value) in [
        ("sampled_out", 0),
        ("active_full", 0),
        ("queue_full", 4),
        ("closed", 0),
        ("invalid_source", 0),
        ("enqueued", enqueued),
        ("accepted", accepted),
        ("rejected", 0),
        ("failed", 0),
        ("encoding_dropped", 0),
        ("shutdown_dropped", 0),
        ("warnings", 0),
    ] {
        assert!(
            traces.contains(&format!("orishu_worker_trace_{name}_total {value}").as_str()),
            "{body}"
        );
    }
    // Optional external parser verification is selected explicitly by the
    // validation command, never an ambient requirement for ordinary tests.
    if let Some(path) = std::env::var_os("ORISHU_TEST_PROMTOOL") {
        let mut child = tokio::process::Command::new(path)
            .args(["check", "metrics"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[tokio::test]
async fn tracing_saturation_preserves_mutations_recovers_and_bounds_shutdown() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let state = root.path().join("state");
        let socket = root.path().join("worker.sock");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
        let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ORISHU_") {
                command.env_remove(name);
            }
        }
        #[cfg(feature = "observability")]
        let address = {
            let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = reservation.local_addr().unwrap();
            command.args([
                "--observability.enabled",
                "true",
                "--observability.bind",
                &address.to_string(),
            ]);
            address
        };
        let mut worker = Worker(
            command
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .args([
                    "--tracing.enabled",
                    "true",
                    "--tracing.endpoint",
                    &endpoint,
                    "--tracing.sample-ppm",
                    "1000000",
                    "--tracing.queue-capacity",
                    "1",
                    "--tracing.batch-size",
                    "1",
                    "--tracing.export-max-bytes",
                    "1024",
                    "--tracing.export-timeout-ms",
                    "10000",
                    "--tracing.shutdown-timeout-ms",
                    "100",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let initial = summary(&mut worker, &socket).await;
        let mut first = receive(&listener).await; // Hold the completed HTTP request unanswered.
        let pressure_start = tokio::time::Instant::now();
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
        let client = HttpClusterClient::new(
            ClusterAddress::UnixSocket(socket.clone()),
            HttClientOptions {
                credentials: Some(orishu::client::Credentials::Token(token.clone())),
                timeout: Some(Duration::from_secs(1)),
                ..Default::default()
            },
        )
        .unwrap();
        for (operation, locked) in [
            ("trace-pressure-lock", true),
            ("trace-pressure-unlock", false),
        ] {
            let receipt = client
                .membership()
                .set_lock(&orishu::model::cluster::LockRequest {
                    schema_version: 1,
                    operation_id: operation.parse().unwrap(),
                    formation_id: initial.formation_id.clone(),
                    locked,
                })
                .await
                .unwrap();
            assert_eq!(receipt.locked, locked);
            let view = summary(&mut worker, &socket).await;
            assert_eq!(view.membership_locked, locked);
            assert_eq!(view.formation_id, initial.formation_id);
        }
        assert!(
            pressure_start.elapsed() < Duration::from_secs(3),
            "control must finish before export timeout"
        );
        #[cfg(feature = "observability")]
        assert_scrape(address, 2, 0, &token).await;
        // Only now relieve pressure. Both retained records must arrive; a new
        // real request then proves export/queue capacity reuse.
        accept(&mut first).await;
        let mut second = receive(&listener).await;
        accept(&mut second).await;
        assert!(!summary(&mut worker, &socket).await.membership_locked);
        let third = receive(&listener).await; // Stall again through process exit.
        #[cfg(feature = "observability")]
        assert_scrape(address, 3, 2, &token).await;
        worker.terminate().await;
        let mut stderr = String::new();
        worker
            .0
            .stderr
            .take()
            .unwrap()
            .take(8193)
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(stderr.len() <= 8192 && !stderr.contains(&token));
        assert!(
            stderr.contains("accepted: 2, rejected: 0, failed: 0"),
            "{stderr}"
        );
        assert!(stderr.contains("shutdown_dropped: 1"), "{stderr}");
        assert!(stderr.contains("queue_full: 4"), "{stderr}");
        assert!(stderr.contains("enqueued: 3"), "{stderr}");
        drop((first, second, third, listener));
    })
    .await
    .expect("worker trace pressure/recovery/shutdown deadline");
}
