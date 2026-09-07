//! Actual executable/client-path checks; no in-process substitute for a worker.
#![cfg(unix)]

use orishu::client::{
    ClientApi, ClusterAddress, ClusterApi, MembershipApi,
    http_client::{HttClientOptions, HttpClusterClient},
};
use orishu::model::cluster::{Participation, Summary};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Worker(Child);

#[cfg(feature = "observability")]
#[path = "support/peer_ingress_metrics.rs"]
mod peer_ingress_metrics;

#[cfg(feature = "otlp-tracing")]
#[path = "support/tracing_security.rs"]
mod tracing_security;

#[cfg(feature = "otlp-tracing")]
#[path = "support/tracing_pressure.rs"]
mod tracing_pressure;

#[cfg(all(
    target_os = "linux",
    feature = "observability",
    feature = "otlp-tracing"
))]
#[path = "support/telemetry_overhead.rs"]
mod telemetry_overhead;

#[cfg(feature = "otlp-tracing")]
#[tokio::test]
async fn tracing_worker_operations_reach_collector_only_when_enabled() {
    use prost::Message;
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(15), async {
        for (enabled, sample_ppm) in [(false, "1000000"), (true, "0"), (true, "1000000")] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("worker.sock");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
            for (key, _) in std::env::vars_os() {
                if key.to_string_lossy().starts_with("ORISHU_") { command.env_remove(key); }
            }
            #[cfg(feature = "observability")]
            let diagnostics_address = {
                let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let address = reservation.local_addr().unwrap();
                command.args(["--observability.enabled", "true", "--observability.bind", &address.to_string()]);
                address
            };
            let mut worker = Worker(command
                .arg("--state-dir").arg(&state).arg("--listen.clients").arg(&socket)
                .args(["--name", "authored-secret-marker", "--tracing.enabled", if enabled { "true" } else { "false" },
                    "--tracing.endpoint", &endpoint, "--tracing.sample-ppm", sample_ppm,
                    "--tracing.batch-size", "1", "--tracing.export-max-bytes", "1024",
                    "--tracing.export-timeout-ms", "200", "--tracing.shutdown-timeout-ms", "100"])
                .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let first = summary(&mut worker, &socket).await;
            #[cfg(feature = "observability")]
            {
                let response = diagnostics_request_bounded(diagnostics_address, "/metrics", 32768).await;
                assert!(response.starts_with("HTTP/1.1 200"));
                assert!(!response.contains("authored-secret-marker"));
                let trace_count = response.lines().filter(|line| line.starts_with("orishu_worker_trace_")).count();
                assert_eq!(trace_count, if enabled { 12 } else { 0 });
                if enabled && sample_ppm == "0" {
                    assert!(response.contains("orishu_worker_trace_sampled_out_total 1\n"));
                    assert!(response.contains("orishu_worker_trace_enqueued_total 0\n"));
                }
            }
            if enabled && sample_ppm != "0" {
                let (mut stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept()).await.unwrap().unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    assert!(head.len() < 4096);
                    head.push(stream.read_u8().await.unwrap());
                }
                let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                assert!(head.starts_with("post /v1/traces http/1.1\r\n"));
                let length: usize = head.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse().unwrap();
                assert!(length <= 1024);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                assert!(!String::from_utf8_lossy(&body).contains("authored-secret-marker"));
                let request = opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest::decode(body.as_slice()).unwrap();
                let spans = &request.resource_spans[0].scope_spans[0].spans;
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].name, "orishu.client.request");
                assert_ne!(spans[0].trace_id, vec![0; 16]);
                assert!(spans[0].parent_span_id.is_empty());
                assert_eq!(spans[0].attributes.len(), 1);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n").await.unwrap();
            } else {
                assert!(tokio::time::timeout(Duration::from_millis(200), listener.accept()).await.is_err());
            }
            drop(listener);
            let after_outage = summary(&mut worker, &socket).await;
            assert_eq!(after_outage.formation_id, first.formation_id);
            assert_eq!(after_outage.member_count, first.member_count);
            worker.terminate().await;
        }
    }).await.expect("real worker tracing acceptance deadline");
}

#[cfg(not(feature = "otlp-tracing"))]
#[tokio::test]
async fn tracing_omitted_capability_refuses_valid_destination_before_io() {
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let socket = root.path().join("worker.sock");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
    let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ORISHU_") {
            command.env_remove(key);
        }
    }
    let mut worker = Worker(
        command
            .arg("--state-dir")
            .arg(&state)
            .arg("--listen.clients")
            .arg(&socket)
            .args(["--tracing.enabled", "true", "--tracing.endpoint", &endpoint])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let status = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(status) = worker.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(status.code(), Some(2));
    let mut stderr = String::new();
    worker
        .0
        .stderr
        .take()
        .unwrap()
        .take(8193)
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stderr.len() <= 8192 && stderr.contains("OTLP tracing is not available in this build"));
    assert!(!stderr.contains(&endpoint));
    assert!(!state.exists() && !socket.exists());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn tracing_memory_budget_precedence_is_validated_before_startup() {
    use std::io::Read;
    for (yaml, environment, flag, valid, invalid) in [
        (
            "endpoint",
            "ORISHU_TRACING_ENDPOINT",
            "--tracing.endpoint",
            "https://collector.invalid/v1/traces",
            "https://seed-user:seed-secret@collector.invalid/v1/traces",
        ),
        (
            "activeSpanCapacity",
            "ORISHU_TRACING_ACTIVE_SPAN_CAPACITY",
            "--tracing.active-span-capacity",
            "128",
            "0",
        ),
        (
            "exportMaxBytes",
            "ORISHU_TRACING_EXPORT_MAX_BYTES",
            "--tracing.export-max-bytes",
            "1048576",
            "4194305",
        ),
        (
            "responseMaxBytes",
            "ORISHU_TRACING_RESPONSE_MAX_BYTES",
            "--tracing.response-max-bytes",
            "16384",
            "65537",
        ),
    ] {
        for (file, env, cli, accepted) in [
            (invalid, None, None, false),
            (invalid, Some(valid), None, true),
            (valid, Some(invalid), Some(valid), true),
            (valid, Some(valid), Some(invalid), false),
        ] {
            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("worker.yml");
            std::fs::write(
                &config,
                format!("spec:\n  tracing:\n    enabled: false\n    {yaml}: {file}\n"),
            )
            .unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("api.sock");
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
            for (key, _) in std::env::vars_os() {
                if key.to_string_lossy().starts_with("ORISHU_") {
                    command.env_remove(key);
                }
            }
            command
                .arg("--config")
                .arg(&config)
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .args(["--accepts.peers", "true"]);
            if let Some(value) = env {
                command.env(environment, value);
            }
            if let Some(value) = cli {
                command.args([flag, value]);
            }
            let mut child = Worker(
                command
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let status = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if let Some(status) = child.0.try_wait().unwrap() {
                        break status;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("tracing memory budget startup deadline");
            assert_eq!(status.code(), Some(2));
            let mut error = String::new();
            child
                .0
                .stderr
                .take()
                .unwrap()
                .take(8193)
                .read_to_string(&mut error)
                .unwrap();
            let expected = if accepted {
                "peer admission requires"
            } else {
                &flag[2..]
            };
            assert!(error.len() <= 8192 && error.contains(expected), "{error}");
            assert!(!error.contains("seed-secret") && !error.contains("seed-user"));
            assert!(!state.exists() && !socket.exists());
        }
    }
}

#[tokio::test]
async fn tracing_credential_settings_validate_without_reading_files() {
    use std::io::Read;
    for (env_key, cli_key, endpoint, expected) in [
        (
            None,
            None,
            "https://collector.invalid/v1/traces",
            "configured together",
        ),
        (
            Some("missing-key.pem"),
            None,
            "https://collector.invalid/v1/traces",
            "peer admission requires",
        ),
        (
            Some(""),
            Some("missing-key.pem"),
            "https://collector.invalid/v1/traces",
            "peer admission requires",
        ),
        (
            Some("missing-key.pem"),
            Some(""),
            "https://collector.invalid/v1/traces",
            "tracing.client-key-file",
        ),
        (
            Some("missing-key.pem"),
            None,
            "http://127.0.0.1:4318/v1/traces",
            "explicit HTTPS endpoint",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("worker.yml");
        std::fs::write(&config, format!("spec:\n  tracing:\n    enabled: false\n    endpoint: {endpoint}\n    caFile: missing-ca.pem\n    clientCertFile: missing-cert.pem\n    bearerTokenFile: missing-token\n")).unwrap();
        let state = root.path().join("state");
        let socket = root.path().join("api.sock");
        let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
        command.current_dir(root.path());
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ORISHU_") {
                command.env_remove(key);
            }
        }
        command
            .arg("--config")
            .arg(&config)
            .arg("--state-dir")
            .arg(&state)
            .arg("--listen.clients")
            .arg(&socket)
            .args(["--accepts.peers", "true"]);
        if let Some(value) = env_key {
            command.env("ORISHU_TRACING_CLIENT_KEY_FILE", value);
        }
        if let Some(value) = cli_key {
            command.args(["--tracing.client-key-file", value]);
        }
        let mut child = Worker(
            command
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let status = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("collector credential validation deadline");
        assert_eq!(status.code(), Some(2));
        let mut error = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .take(8193)
            .read_to_string(&mut error)
            .unwrap();
        assert!(error.len() <= 8192 && error.contains(expected), "{error}");
        assert!(!error.contains("missing-token") && !error.contains("missing-key.pem"));
        assert!(!state.exists() && !socket.exists());
    }
}

#[tokio::test]
async fn tracing_configuration_precedence_and_capability_fail_before_state_creation() {
    use std::io::Read;
    for (file_enabled, env_enabled, cli_enabled, expected) in [
        (false, None, None, "peer admission requires"),
        (false, Some("true"), None, "OTLP tracing is not available"),
        (true, Some("false"), None, "peer admission requires"),
        (
            false,
            Some("true"),
            Some("false"),
            "peer admission requires",
        ),
        (
            true,
            Some("false"),
            Some("true"),
            "OTLP tracing is not available",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("worker.yml");
        std::fs::write(&config, format!("spec:\n  tracing:\n    enabled: {file_enabled}\n    samplePpm: 0\n    queueCapacity: 32\n    batchSize: 16\n")).unwrap();
        let state = root.path().join("state");
        let socket = root.path().join("api.sock");
        let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ORISHU_") {
                command.env_remove(key);
            }
        }
        command
            .arg("--config")
            .arg(&config)
            .arg("--state-dir")
            .arg(&state)
            .arg("--listen.clients")
            .arg(&socket)
            // A deliberate later config error proves disabled tracing validation
            // completed without opening sockets or creating state.
            .args(["--accepts.peers", "true"]);
        if let Some(value) = env_enabled {
            command.env("ORISHU_TRACING_ENABLED", value);
        }
        if let Some(value) = cli_enabled {
            command.args(["--tracing.enabled", value]);
        }
        let mut child = Worker(
            command
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let status = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("tracing startup-validation deadline");
        assert_eq!(status.code(), Some(2));
        let mut error = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .take(8193)
            .read_to_string(&mut error)
            .unwrap();
        assert!(error.len() <= 8192 && error.contains(expected), "{error}");
        assert!(!state.exists() && !socket.exists());
    }
}

#[cfg(not(feature = "formation-fault-test"))]
#[tokio::test]
async fn ordinary_executable_rejects_all_formation_fault_controls_before_startup() {
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--test-lose-next-join-ack",
        "--test-crash-after-join",
        "--test-remove-after-join",
        "--test-block-after-join",
        "--test-eject-peer-on-signal",
        "--test-drop-next-departure",
    ] {
        let state = root.path().join("state");
        let socket = root.path().join("api.sock");
        let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ORISHU_") {
                command.env_remove(key);
            }
        }
        let mut child = Worker(
            command
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .arg(flag)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let status = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fault flag refusal deadline");
        assert_eq!(status.code(), Some(2), "{flag}");
        let mut error = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .take(8193)
            .read_to_string(&mut error)
            .unwrap();
        assert!(error.len() <= 8192);
        assert!(
            error.contains("unexpected argument") && error.contains(flag),
            "{error}"
        );
        assert!(!state.exists(), "fault flag created credentials/state");
        assert!(!socket.exists(), "fault flag opened a client endpoint");
    }
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn unfinished_diagnostics_requests_do_not_prevent_process_shutdown() {
    unfinished_diagnostics_requests(true, 8).await;
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn unfinished_diagnostics_requests_expire_without_process_shutdown() {
    unfinished_diagnostics_requests(false, 8).await;
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn diagnostics_full_connection_budget_preserves_operator_control_and_recovers() {
    unfinished_diagnostics_requests(false, 16).await;
}

#[cfg(feature = "observability")]
async fn unfinished_diagnostics_requests(shutdown: bool, connections: usize) {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(if shutdown { 8 } else { 12 }), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let state = root.path().join("state");
        let socket = root.path().join("api.sock");
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let mut worker = Worker(Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
            .arg("--state-dir").arg(&state).arg("--listen.clients").arg(&socket)
            .args(["--observability.enabled", "true", "--observability.bind", &address.to_string(),
                   "--observability.metrics", "true", "--observability.probes", "true"])
            .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
        let initial = summary(&mut worker, &socket).await;
        assert!(diagnostics_request(address, "/metrics").await.starts_with("HTTP/1.1 200"));
        let mut held = Vec::new();
        let pressure_started = tokio::time::Instant::now();
        for _ in 0..connections {
            let mut connection = tokio::net::TcpStream::connect(address).await.unwrap();
            // Complete one persistent request first to prove this connection
            // was accepted by the real diagnostics server, not only its backlog.
            connection.write_all(b"GET /startupz HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(1), async {
                let mut byte = [0];
                while !response.ends_with(b"\r\n\r\n") {
                    connection.read_exact(&mut byte).await.unwrap();
                    response.push(byte[0]);
                    assert!(response.len() <= 8192);
                }
                assert!(response.starts_with(b"HTTP/1.1 200"));
                let mut body = [0; 3];
                connection.read_exact(&mut body).await.unwrap();
                assert_eq!(&body, b"ok\n");
            }).await.expect("accepted diagnostics connection deadline");
            connection.write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nX-Held:").await.unwrap();
            held.push(connection);
        }
        if connections == 16 {
            assert!(pressure_started.elapsed() < Duration::from_secs(2), "capacity setup must precede head expiry");
            let outcome = tokio::time::timeout(Duration::from_millis(500), async {
                let mut extra = tokio::net::TcpStream::connect(address).await?;
                extra.write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await?;
                let mut byte = [0];
                assert_eq!(extra.read(&mut byte).await?, 0, "over-budget connection received response bytes");
                Ok::<(), std::io::Error>(())
            }).await;
            match outcome {
                Ok(Ok(())) | Err(_) => {}, // Closed or still waiting; never a served request.
                Ok(Err(error)) if matches!(error.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionRefused) => {},
                result => panic!("unexpected over-budget connection outcome: {result:?}"),
            }
        } else {
            assert!(diagnostics_request(address, "/livez").await.starts_with("HTTP/1.1 200"));
        }
        assert_eq!(summary(&mut worker, &socket).await.formation_id, initial.formation_id);
        let client = HttpClusterClient::new(ClusterAddress::UnixSocket(socket.clone()), HttClientOptions {
            credentials: Some(orishu::client::Credentials::Token(std::fs::read_to_string(state.join("operator.token")).unwrap().trim().to_owned())),
            ..Default::default()
        }).unwrap();
        for (operation, locked) in [("held-diagnostics-lock", true), ("held-diagnostics-unlock", false)] {
            let receipt = tokio::time::timeout(Duration::from_secs(1),
                client.membership().set_lock(&orishu::model::cluster::LockRequest {
                    schema_version: 1,
                    operation_id: operation.parse().unwrap(),
                    formation_id: initial.formation_id.clone(),
                    locked,
                })).await.expect("operator control must progress with held diagnostics clients").unwrap();
            assert_eq!(receipt.locked, locked);
        }
        if connections == 16 {
            assert!(pressure_started.elapsed() < Duration::from_secs(4), "operator progress must precede five-second head expiry");
        }
        if shutdown {
            worker.terminate().await;
            assert!(!socket.exists(), "shutdown did not clean up its client socket");
        }
        let expiry = tokio::time::Instant::now() + Duration::from_secs(if shutdown { 1 } else { 7 });
        for mut connection in held {
            tokio::time::timeout_at(expiry, async {
                let mut bytes = Vec::new();
                match (&mut connection).take(8193).read_to_end(&mut bytes).await {
                    Ok(_) => {},
                    Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {},
                    result => panic!("unfinished connection remained open or produced an unexpected response: {result:?}"),
                }
                assert!(bytes.len() <= 8192);
                assert!(bytes.is_empty() || bytes.starts_with(b"HTTP/1.1 408"), "unexpected incomplete-request response");
            }).await.expect("server must close held diagnostics connections within the shared deadline");
        }
        if !shutdown {
            assert!(worker.0.try_wait().unwrap().is_none());
            assert!(diagnostics_request(address, "/livez").await.starts_with("HTTP/1.1 200"));
            assert!(diagnostics_request(address, "/metrics").await.starts_with("HTTP/1.1 200"));
            assert_eq!(summary(&mut worker, &socket).await.formation_id, initial.formation_id);
            worker.terminate().await;
        }
    }).await.expect("unfinished diagnostics shutdown deadline");
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn diagnostics_reject_oversized_headers_and_preserve_normal_access() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(8), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let state = root.path().join("state");
        let socket = root.path().join("api.sock");
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let mut worker = Worker(
            Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .args([
                    "--observability.enabled",
                    "true",
                    "--observability.bind",
                    &address.to_string(),
                    "--observability.metrics",
                    "true",
                    "--observability.probes",
                    "true",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let initial = summary(&mut worker, &socket).await;
        assert!(
            diagnostics_request(address, "/metrics")
                .await
                .starts_with("HTTP/1.1 200")
        );
        let headers = |extra: usize| {
            format!(
                "GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}\r\n",
                (0..extra)
                    .map(|index| format!("X-{index}: value\r\n"))
                    .collect::<String>()
            )
        };
        let excessive_bytes = format!(
            "GET /metrics HTTP/1.1\r\nHost: localhost\r\nX-Large: {}\r\n\r\n",
            "x".repeat(9000)
        );
        for (request, status) in [
            (headers(30), 200),
            (headers(31), 431),
            (headers(40), 431),
            (excessive_bytes, 431),
        ] {
            tokio::time::timeout(Duration::from_secs(2), async {
                let mut connection = tokio::net::TcpStream::connect(address).await.unwrap();
                connection.write_all(request.as_bytes()).await.unwrap();
                let mut response = Vec::new();
                let limit = if status == 200 { 32768 } else { 8192 };
                connection
                    .take(limit + 1)
                    .read_to_end(&mut response)
                    .await
                    .unwrap();
                assert!(response.len() as u64 <= limit);
                let response = String::from_utf8(response).unwrap();
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status}")),
                    "{response}"
                );
                assert_eq!(response.contains("orishu_worker_"), status == 200);
            })
            .await
            .expect("diagnostics header rejection deadline");
            assert!(
                diagnostics_request(address, "/metrics")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
            assert!(
                diagnostics_request(address, "/livez")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
            assert_eq!(
                summary(&mut worker, &socket).await.formation_id,
                initial.formation_id
            );
        }
        worker.terminate().await;
    })
    .await
    .expect("diagnostics hostile-header fixture deadline");
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn diagnostics_bind_precedence_uses_only_the_selected_address() {
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(Duration::from_secs(12), async {
        for source in ["file", "environment", "cli"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("api.sock");
            let config = root.path().join("worker.yaml");
            // All candidates are distinct, occupied sockets until selection.
            let file = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let environment = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let cli = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addresses = [
                file.local_addr().unwrap(),
                environment.local_addr().unwrap(),
                cli.local_addr().unwrap(),
            ];
            std::fs::write(
                &config,
                format!(
                    "spec:\n  observability:\n    enabled: true\n    bind: '{}'\n",
                    addresses[0]
                ),
            )
            .unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
            command
                .arg("--config")
                .arg(&config)
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .env_remove("ORISHU_OBSERVABILITY_ENABLED")
                .env_remove("ORISHU_OBSERVABILITY_BIND")
                .env_remove("ORISHU_OBSERVABILITY_METRICS")
                .env_remove("ORISHU_OBSERVABILITY_PROBES")
                .stdout(Stdio::null())
                .stderr(Stdio::inherit());
            let selected = match source {
                "file" => 0,
                "environment" => {
                    command.env("ORISHU_OBSERVABILITY_BIND", addresses[1].to_string());
                    1
                }
                "cli" => {
                    command.env("ORISHU_OBSERVABILITY_BIND", addresses[1].to_string());
                    command.args(["--observability.bind", &addresses[2].to_string()]);
                    2
                }
                _ => unreachable!(),
            };
            let mut held = [Some(file), Some(environment), Some(cli)];
            drop(held[selected].take());
            let mut worker = Worker(command.spawn().unwrap());
            let initial = summary(&mut worker, &socket).await;
            loop {
                if diagnostics_request(addresses[selected], "/readyz")
                    .await
                    .starts_with("HTTP/1.1 200")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(
                diagnostics_request(addresses[selected], "/metrics")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
            for (index, listener) in held.iter().enumerate() {
                if index != selected {
                    assert_eq!(
                        listener.as_ref().unwrap().local_addr().unwrap(),
                        addresses[index]
                    );
                }
            }
            assert_eq!(
                summary(&mut worker, &socket).await.formation_id,
                initial.formation_id
            );
            worker.terminate().await;
        }
    })
    .await
    .expect("diagnostics bind precedence deadline");
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn occupied_diagnostics_port_fails_before_worker_identity_and_client_bind() {
    use std::{io::Read, os::unix::fs::PermissionsExt};
    tokio::time::timeout(Duration::from_secs(3), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let state = root.path().join("state");
        let socket = root.path().join("api.sock");
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = occupied.local_addr().unwrap();
        let mut worker = Worker(
            Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .args([
                    "--observability.enabled",
                    "true",
                    "--observability.bind",
                    &address.to_string(),
                    "--observability.metrics",
                    "true",
                    "--observability.probes",
                    "true",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let status = loop {
            if let Some(status) = worker.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let mut error = String::new();
        worker
            .0
            .stderr
            .take()
            .unwrap()
            .take(8192)
            .read_to_string(&mut error)
            .unwrap();
        assert_eq!(status.code(), Some(2), "{error}");
        assert!(
            error.contains("cannot bind diagnostics listener"),
            "{error}"
        );
        assert!(!error.contains("panicked"));
        assert!(!state.exists(), "bind failure created worker credentials");
        assert!(!socket.exists(), "bind failure created client socket");
        // The conflicting socket belongs to the fixture and remains untouched.
        assert_eq!(occupied.local_addr().unwrap(), address);
    })
    .await
    .expect("occupied diagnostics bind deadline");
}

#[tokio::test]
async fn malformed_diagnostics_configuration_exits_before_credentials() {
    use std::io::Read;
    for field in ["enabled", "bind", "metrics", "probes", "enabld"] {
        for source in ["file", "environment", "cli"] {
            if field == "enabld" && source != "file" {
                continue;
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                let root = tempfile::tempdir().unwrap();
                let state = root.path().join("state");
                let socket = root.path().join("api.sock");
                let config = root.path().join("worker.yaml");
                let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
                command
                    .arg("--state-dir")
                    .arg(&state)
                    .arg("--listen.clients")
                    .arg(&socket)
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped());
                for variable in ["ENABLED", "BIND", "METRICS", "PROBES"] {
                    command.env_remove(format!("ORISHU_OBSERVABILITY_{variable}"));
                }
                match source {
                    "file" => {
                        std::fs::write(
                            &config,
                            format!("spec:\n  observability:\n    {field}: not-a-valid-value\n"),
                        )
                        .unwrap();
                        command.arg("--config").arg(&config);
                    }
                    "environment" => {
                        command.env(
                            format!("ORISHU_OBSERVABILITY_{}", field.to_uppercase()),
                            "not-a-valid-value",
                        );
                    }
                    "cli" => {
                        command.args([
                            format!("--observability.{field}"),
                            "not-a-valid-value".to_owned(),
                        ]);
                    }
                    _ => unreachable!(),
                }
                let mut worker = Worker(command.spawn().unwrap());
                let status = loop {
                    if let Some(status) = worker.0.try_wait().unwrap() {
                        break status;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                };
                let mut error = String::new();
                worker
                    .0
                    .stderr
                    .take()
                    .unwrap()
                    .take(8192)
                    .read_to_string(&mut error)
                    .unwrap();
                assert_eq!(status.code(), Some(2), "{source}/{field}: {error}");
                assert!(
                    error.contains(if source == "file" {
                        "failed to parse config"
                    } else {
                        "invalid value"
                    }),
                    "{source}/{field}: {error}"
                );
                assert!(
                    !state.exists(),
                    "malformed configuration created credentials"
                );
                assert!(
                    !socket.exists(),
                    "malformed configuration opened client socket"
                );
            })
            .await
            .unwrap_or_else(|_| panic!("{source}/{field}: configuration rejection deadline"));
        }
    }
}

#[tokio::test]
async fn diagnostics_enablement_precedence_and_invalid_config_are_checked_at_startup() {
    use std::{io::Read, os::unix::fs::PermissionsExt};
    // An invalid remote bind makes effective enablement observable before
    // credentials are created. Disabled cases retain an occupied loopback
    // bind instead, proving startup does not open the optional listener.
    for (name, file_enabled, environment, cli, enabled) in [
        ("file-enables", true, None, None, true),
        ("environment-disables", true, Some("false"), None, false),
        ("environment-enables", false, Some("true"), None, true),
        ("cli-disables", true, Some("true"), Some("false"), false),
        ("cli-enables", false, Some("false"), Some("true"), true),
    ] {
        tokio::time::timeout(Duration::from_secs(5), async {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("api.sock");
            let config = root.path().join("worker.yaml");
            let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let bind = if enabled {
                "0.0.0.0:9168".to_owned()
            } else {
                reservation.local_addr().unwrap().to_string()
            };
            std::fs::write(
                &config,
                format!(
                    "spec:\n  observability:\n    enabled: {file_enabled}\n    bind: '{bind}'\n"
                ),
            )
            .unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
            command
                .arg("--config")
                .arg(&config)
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .env_remove("ORISHU_OBSERVABILITY_ENABLED")
                .env_remove("ORISHU_OBSERVABILITY_BIND")
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            if let Some(value) = environment {
                command.env("ORISHU_OBSERVABILITY_ENABLED", value);
            }
            if let Some(value) = cli {
                command.args(["--observability.enabled", value]);
            }
            let mut worker = Worker(command.spawn().unwrap());
            if enabled {
                let status = loop {
                    if let Some(status) = worker.0.try_wait().unwrap() {
                        break status;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                };
                let mut error = String::new();
                worker
                    .0
                    .stderr
                    .take()
                    .unwrap()
                    .take(8192)
                    .read_to_string(&mut error)
                    .unwrap();
                assert_eq!(status.code(), Some(2), "{name}: {error}");
                let expected = if cfg!(feature = "observability") {
                    "loopback bind"
                } else {
                    "build feature"
                };
                assert!(error.contains(expected), "{name}: {error}");
                assert!(
                    !state.exists(),
                    "{name}: rejected config created credentials"
                );
            } else {
                assert_eq!(summary(&mut worker, &socket).await.member_count, 1);
                assert_eq!(reservation.local_addr().unwrap().to_string(), bind);
                worker.terminate().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{name}: bounded startup deadline"));
    }
}

#[test]
fn diagnostics_reject_unsupported_exposure_before_creating_state() {
    for bind in ["0.0.0.0:9168", "192.0.2.1:9168"] {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let result = Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
            .args([
                "--state-dir",
                state.to_str().unwrap(),
                "--observability.enabled",
                "true",
                "--observability.bind",
                bind,
            ])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(message.contains(if cfg!(feature = "observability") {
            "loopback bind"
        } else {
            "build feature"
        }));
        assert!(!state.exists());
    }
}

#[cfg(feature = "observability")]
async fn diagnostics_request(address: std::net::SocketAddr, path: &str) -> String {
    diagnostics_request_bounded(address, path, 32768).await
}

#[cfg(feature = "observability")]
async fn diagnostics_request_bounded(
    address: std::net::SocketAddr,
    path: &str,
    max_bytes: u64,
) -> String {
    diagnostics_request_raw(
        address,
        &format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
        max_bytes,
    )
    .await
}

#[cfg(feature = "observability")]
async fn diagnostics_request_raw(
    address: std::net::SocketAddr,
    request: &str,
    max_bytes: u64,
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut connection = loop {
            match tokio::net::TcpStream::connect(address).await {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("diagnostics connection failed: {error}"),
            }
        };
        connection.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        connection
            .take(max_bytes + 1)
            .read_to_end(&mut response)
            .await
            .unwrap();
        assert!(response.len() as u64 <= max_bytes);
        String::from_utf8(response).unwrap()
    })
    .await
    .expect("bounded diagnostics response")
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn diagnostics_direct_methods_paths_and_errors_are_bounded_and_isolated() {
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(Duration::from_secs(20), async {
        for (metrics, probes) in [(true, true), (true, false), (false, true)] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("api.sock");
            let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = reservation.local_addr().unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
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
                .args([
                    "--observability.enabled",
                    "true",
                    "--observability.bind",
                    &address.to_string(),
                    "--observability.metrics",
                    &metrics.to_string(),
                    "--observability.probes",
                    &probes.to_string(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // With tracing compiled in, zero local sampling makes any accidental
            // client span visible in sampled_out without exporting secret input.
            #[cfg(feature = "otlp-tracing")]
            let collector = {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                command.args([
                    "--tracing.enabled",
                    "true",
                    "--tracing.sample-ppm",
                    "0",
                    "--tracing.endpoint",
                    &format!("http://{}/v1/traces", listener.local_addr().unwrap()),
                ]);
                listener
            };
            drop(reservation);
            let mut worker = Worker(command.spawn().unwrap());
            let initial = summary(&mut worker, &socket).await;
            let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
            let before = if metrics {
                Some(diagnostics_request(address, "/metrics").await)
            } else {
                None
            };
            for (path, enabled) in [
                ("/metrics", metrics),
                ("/livez", probes),
                ("/readyz", probes),
                ("/startupz", probes),
            ] {
                for query in ["", "?", "?private-query-marker=1"] {
                    for method in [
                        "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE", "BREW",
                    ] {
                        if enabled && query.is_empty() && method == "GET" {
                            continue;
                        }
                        let status = if !enabled {
                            404
                        } else if !query.is_empty() {
                            400
                        } else {
                            405
                        };
                        assert_diagnostics_error(
                            address,
                            method,
                            &format!("{path}{query}"),
                            token.trim(),
                            status,
                        )
                        .await;
                    }
                }
            }
            for path in [
                "/",
                "/private-path-marker",
                "/api/v1/cluster/token",
                "/api/v1/cluster/lock",
                "/metrics/",
                "//metrics",
                "/%6detrics",
                "/metrics%2f",
                "/METRICS",
                "/metrics/../livez",
            ] {
                for method in ["GET", "HEAD", "POST"] {
                    assert_diagnostics_error(address, method, path, token.trim(), 404).await;
                }
            }
            if let Some(before) = before {
                let after = diagnostics_request(address, "/metrics").await;
                let client_or_trace = |response: &str| -> Vec<String> {
                    response
                        .lines()
                        .filter(|line| {
                            line.starts_with("orishu_worker_client_")
                                || line.starts_with("orishu_worker_trace_")
                        })
                        .map(str::to_owned)
                        .collect()
                };
                assert!(!client_or_trace(&before).is_empty());
                #[cfg(feature = "otlp-tracing")]
                {
                    assert_eq!(
                        before
                            .lines()
                            .filter(|line| line.starts_with("orishu_worker_trace_"))
                            .count(),
                        12
                    );
                    let sampled = before
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("orishu_worker_trace_sampled_out_total ")
                        })
                        .unwrap();
                    assert!(
                        sampled.parse::<u64>().unwrap() > 0,
                        "positive client sampling control required"
                    );
                }
                assert_eq!(
                    client_or_trace(&before),
                    client_or_trace(&after),
                    "diagnostic errors must not enter client accounting or tracing"
                );
            }
            #[cfg(feature = "otlp-tracing")]
            assert!(
                tokio::time::timeout(Duration::from_millis(50), collector.accept())
                    .await
                    .is_err()
            );
            if probes {
                for path in ["/livez", "/readyz", "/startupz"] {
                    assert!(
                        diagnostics_request(address, path)
                            .await
                            .starts_with("HTTP/1.1 200")
                    );
                }
            }
            let after = summary(&mut worker, &socket).await;
            assert_eq!(after.formation_id, initial.formation_id);
            assert_eq!(after.source_node_id, initial.source_node_id);
            assert_eq!(after.member_count, initial.member_count);
            assert_eq!(after.membership_locked, initial.membership_locked);
            worker.terminate().await;
        }
    })
    .await
    .expect("direct diagnostics error matrix deadline");
}

#[cfg(feature = "observability")]
async fn assert_diagnostics_error(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    token: &str,
    status: u16,
) {
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAccept: text/html, application/json\r\nAuthorization: Bearer {token}\r\nX-Private: private-header-marker\r\nTraceparent: 00-11111111111111111111111111111111-2222222222222222-01\r\nContent-Length: 19\r\n\r\nprivate-body-marker"
    );
    let response = diagnostics_request_raw(address, &request, 2048).await;
    assert!(
        response.starts_with(&format!("HTTP/1.1 {status} ")),
        "{method} {path}: unexpected status"
    );
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let head = head.to_ascii_lowercase();
    assert!(
        head.lines().any(|line| line == "cache-control: no-store"),
        "{method} {path}: errors must not be cached"
    );
    assert!(
        head.contains("content-type: text/plain"),
        "{method} {path}: fixed plain-text error required"
    );
    assert_eq!(head.contains("allow: get"), status == 405);
    assert!(body.len() <= 64);
    if method == "HEAD" {
        assert!(body.is_empty());
    } else {
        assert_eq!(
            body,
            match status {
                400 => "query parameters unsupported\n",
                404 => "not found\n",
                405 => "method unsupported\n",
                _ => unreachable!(),
            }
        );
    }
    for secret in [
        token,
        "private-body-marker",
        "private-query-marker",
        "private-path-marker",
        "private-header-marker",
        "11111111111111111111111111111111",
        "orishu_worker_",
        "<!DOCTYPE",
    ] {
        assert!(
            !response.contains(secret),
            "{method}: reflected or unexpected response data"
        );
    }
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn loopback_diagnostics_expose_real_health_and_runtime_disable_binds_nothing() {
    use diagnostics_request as request;
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(Duration::from_secs(10), async {
        for enabled in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let socket = root.path().join("api.sock");
            let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = reservation.local_addr().unwrap();
            let reservation = if enabled {
                drop(reservation);
                None
            } else {
                Some(reservation)
            };
            let mut worker = Worker(
                Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
                    .args([
                        "--state-dir",
                        state.to_str().unwrap(),
                        "--listen.clients",
                        socket.to_str().unwrap(),
                        "--observability.enabled",
                        if enabled { "true" } else { "false" },
                        "--observability.bind",
                        &address.to_string(),
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let initial = summary(&mut worker, &socket).await;
            if enabled {
                // Summary may become available just before process startup latches.
                loop {
                    if request(address, "/readyz")
                        .await
                        .starts_with("HTTP/1.1 200")
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                for path in ["/livez", "/startupz"] {
                    let response = request(address, path).await;
                    assert!(response.starts_with("HTTP/1.1 200"));
                    assert!(response.ends_with("ok\n"));
                }
                let metrics = request(address, "/metrics").await;
                assert!(metrics.starts_with("HTTP/1.1 200"));
                assert!(metrics.contains("text/plain; version=0.0.4"));
                for name in ["owner_responsive", "ready", "startup_complete"] {
                    assert!(metrics.contains(&format!("# TYPE orishu_worker_{name} gauge\n")));
                    assert!(metrics.contains(&format!("orishu_worker_{name} 1\n")));
                }
                let value = |response: &str, name: &str| -> u64 {
                    let prefix = format!("orishu_worker_client_requests_{name} ");
                    response.lines().find_map(|line| line.strip_prefix(&prefix)).unwrap().parse().unwrap()
                };
                let before = value(&metrics, "completed_total");
                assert!(before > 0, "the real initial client summary is accounted");
                assert_eq!(value(&metrics, "in_flight"), 0);
                // Diagnostics are on a separate service and cannot recursively
                // increase the client request counters.
                let repeated = request(address, "/metrics").await;
                assert_eq!(value(&repeated, "completed_total"), before);
                let rejected_before = value(&metrics, "rejected_total");
                for (path, status) in [("/api/v1/cluster/token", 401), ("/private-hostile-path", 404), ("/api/v1/cluster/lock", 405)] {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
                    stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                    let mut bytes = Vec::new();
                    stream.take(8193).read_to_end(&mut bytes).await.unwrap();
                    assert!(bytes.len() <= 8192);
                    assert!(bytes.starts_with(format!("HTTP/1.1 {status}").as_bytes()));
                }
                let after = request(address, "/metrics").await;
                assert_eq!(value(&after, "completed_total"), before + 3);
                assert_eq!(value(&after, "rejected_total"), rejected_before + 3);
                assert_eq!(value(&after, "failed_total"), 0);
                assert_eq!(value(&after, "cancelled_total"), 0);
                assert_eq!(value(&after, "in_flight"), 0);
                assert!(!after.contains("private-hostile-path"));
                assert!(after.split_once("\r\n\r\n").unwrap().1.len() < 32768);
                let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
                assert!(!metrics.contains(token.trim()));
                assert!(
                    request(address, "/metrics?peer=untrusted")
                        .await
                        .starts_with("HTTP/1.1 400")
                );
                assert!(
                    request(address, "/api/v1/cluster/token")
                        .await
                        .starts_with("HTTP/1.1 404")
                );
                assert_eq!(
                    summary(&mut worker, &socket).await.formation_id,
                    initial.formation_id
                );
            } else {
                assert!(
                    reservation.is_some(),
                    "disabled worker starts while diagnostics port remains occupied"
                );
            }
            worker.terminate().await;
        }
    })
    .await
    .expect("diagnostics process fixture deadline");
}

#[cfg(feature = "observability")]
#[tokio::test]
async fn diagnostics_route_groups_follow_file_environment_and_cli() {
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(Duration::from_secs(20), async {
        for (metrics, probes) in [(true, true), (true, false), (false, true)] {
            for source in ["file", "environment", "cli"] {
                let root = tempfile::tempdir().unwrap();
                std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
                let state = root.path().join("state");
                let socket = root.path().join("api.sock");
                let config = root.path().join("worker.yaml");
                let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let address = reservation.local_addr().unwrap();
                let file_metrics = if source == "file" { metrics } else { !metrics };
                let file_probes = if source == "file" { probes } else { !probes };
                std::fs::write(&config, format!(
                    "spec:\n  observability:\n    enabled: true\n    bind: '{address}'\n    metrics: {file_metrics}\n    probes: {file_probes}\n"
                )).unwrap();
                let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
                command.arg("--config").arg(&config)
                    .arg("--state-dir").arg(&state)
                    .arg("--listen.clients").arg(&socket)
                    .env_remove("ORISHU_OBSERVABILITY_ENABLED")
                    .env_remove("ORISHU_OBSERVABILITY_BIND")
                    .env_remove("ORISHU_OBSERVABILITY_METRICS")
                    .env_remove("ORISHU_OBSERVABILITY_PROBES")
                    .stdout(Stdio::null()).stderr(Stdio::inherit());
                if source != "file" {
                    let env_metrics = if source == "environment" { metrics } else { !metrics };
                    let env_probes = if source == "environment" { probes } else { !probes };
                    command.env("ORISHU_OBSERVABILITY_METRICS", env_metrics.to_string())
                        .env("ORISHU_OBSERVABILITY_PROBES", env_probes.to_string());
                }
                if source == "cli" {
                    command.args(["--observability.metrics", &metrics.to_string(), "--observability.probes", &probes.to_string()]);
                }
                drop(reservation);
                let mut worker = Worker(command.spawn().unwrap());
                let initial = summary(&mut worker, &socket).await;
                for (path, enabled) in [("/metrics", metrics), ("/livez", probes), ("/readyz", probes), ("/startupz", probes)] {
                    let response = loop {
                        let response = diagnostics_request(address, path).await;
                        if enabled && response.starts_with("HTTP/1.1 503") {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            continue;
                        }
                        break response;
                    };
                    let status = if enabled { 200 } else { 404 };
                    assert!(response.starts_with(&format!("HTTP/1.1 {status}")), "{source}, metrics={metrics}, probes={probes}, {path}: {response}");
                    if !enabled {
                        assert!(!response.contains("orishu_worker_"));
                        assert!(!response.ends_with("ok\n"));
                    }
                }
                assert!(diagnostics_request(address, "/api/v1/cluster/token").await.starts_with("HTTP/1.1 404"));
                assert_eq!(summary(&mut worker, &socket).await.formation_id, initial.formation_id);
                worker.terminate().await;
            }
        }
    }).await.expect("diagnostic route matrix deadline");
}

async fn h2_frame(
    stream: &mut tokio::net::UnixStream,
    kind: u8,
    flags: u8,
    id: u32,
    payload: &[u8],
) {
    use tokio::io::AsyncWriteExt;
    assert!(payload.len() <= 16384);
    let length = (payload.len() as u32).to_be_bytes();
    let mut header = Vec::from(&length[1..]);
    header.extend_from_slice(&[kind, flags]);
    header.extend_from_slice(&id.to_be_bytes());
    stream.write_all(&header).await.unwrap();
    stream.write_all(payload).await.unwrap();
}

async fn read_h2_frame(stream: &mut tokio::net::UnixStream) -> (u8, u8, u32, Vec<u8>) {
    use tokio::io::AsyncReadExt;
    let mut head = [0; 9];
    stream.read_exact(&mut head).await.unwrap();
    let length = u32::from_be_bytes([0, head[0], head[1], head[2]]) as usize;
    assert!(length <= 16384, "bounded HTTP/2 fixture frame");
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    (
        head[3],
        head[4],
        u32::from_be_bytes(head[5..9].try_into().unwrap()) & 0x7fff_ffff,
        body,
    )
}

#[tokio::test]
async fn http2_active_partial_frame_expires_despite_progress() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(18), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        let initial = summary(&mut worker, &socket).await;
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        h2_frame(&mut stream, 4, 0, 0, &[]).await;
        let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
        assert_eq!((kind, flags, id), (4, 0, 0));
        h2_frame(&mut stream, 4, 1, 0, &[]).await;
        // A legal-sized HEADERS frame whose declared payload never completes.
        // No application handler can see this request. The peer supplies only
        // one payload byte per second until the server expires assembly.
        stream
            .write_all(&[0, 0x40, 0, 1, 5, 0, 0, 0, 1])
            .await
            .unwrap();
        let started = tokio::time::Instant::now();
        let mut received = 0;
        tokio::time::timeout(Duration::from_secs(7), async {
            loop {
                stream.write_all(&[0x82]).await.unwrap();
                let until = tokio::time::Instant::now() + Duration::from_secs(1);
                loop {
                    let mut bytes = [0; 1024];
                    match tokio::time::timeout_at(until, stream.read(&mut bytes)).await {
                        Err(_) => break,
                        Ok(Ok(0)) => return,
                        Ok(Ok(count)) => {
                            received += count;
                            assert!(received <= 8192);
                        }
                        Ok(Err(error)) => {
                            assert!(matches!(
                                error.kind(),
                                std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::BrokenPipe
                            ));
                            return;
                        }
                    }
                }
            }
        })
        .await
        .expect("server must expire active assembly within five seconds plus scheduling margin");
        assert!(started.elapsed() >= Duration::from_secs(4));
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        drop(stream);
        worker.terminate().await;
    })
    .await
    .expect("finite active-partial-frame diagnostic budget");
}

#[tokio::test]
async fn http2_completed_assembly_preserves_long_lived_multiplexing() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;

    tokio::time::timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        let initial = summary(&mut worker, &socket).await;
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        h2_frame(&mut stream, 4, 0, 0, &[]).await;
        assert_eq!(read_h2_frame(&mut stream).await.0, 4);
        h2_frame(&mut stream, 4, 1, 0, &[]).await;
        // Fragment the frame header, then finish the logical block on a later
        // CONTINUATION. Both finish inside the declared absolute budget.
        stream.write_all(&[0, 0, 1, 1]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        stream.write_all(&[1, 0, 0, 0, 1, 0x82]).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut remaining = vec![0x86, 0x04];
        let path = b"/api/v1/cluster";
        remaining.push(path.len() as u8);
        remaining.extend_from_slice(path);
        remaining.extend_from_slice(b"\x01\x09localhost");
        h2_frame(&mut stream, 9, 4, 1, &remaining).await;
        let mut get = vec![0x82];
        get.extend_from_slice(&remaining);
        h2_frame(&mut stream, 1, 5, 3, &get).await;
        let mut completed = std::collections::BTreeSet::new();
        let mut statuses = std::collections::BTreeSet::new();
        let mut received = 0;
        while completed.len() < 2 {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            received += body.len() + 9;
            assert!(received <= 8192);
            if id == 0 {
                assert_eq!(kind, 4);
                continue;
            }
            assert!(matches!(id, 1 | 3));
            if kind == 1 {
                assert_eq!(body.first(), Some(&0x88)); // :status 200
                statuses.insert(id);
            }
            if flags & 1 != 0 {
                completed.insert(id);
            }
        }
        assert_eq!(statuses, completed);
        for value in 0_u64..6 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let ping = value.to_be_bytes();
            h2_frame(&mut stream, 6, 0, 0, &ping).await;
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            assert_eq!((kind, flags, id), (6, 1, 0));
            assert_eq!(body, ping);
        }
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        drop(stream);
        worker.terminate().await;
    })
    .await
    .expect("completed assembly and multiplexing budget");
}

#[tokio::test]
async fn tls_http2_active_partial_frame_expires_despite_progress() {
    exercise_http2_deadline(true, Http2DeadlineCase::PartialFrame).await;
}

#[tokio::test]
async fn tls_http2_continued_header_block_has_one_absolute_budget() {
    exercise_http2_deadline(true, Http2DeadlineCase::HeaderBlock).await;
}

#[tokio::test]
async fn http2_continued_header_block_has_one_absolute_budget() {
    exercise_http2_deadline(false, Http2DeadlineCase::HeaderBlock).await;
}

#[tokio::test]
async fn tls_http2_withheld_response_credit_expires_despite_pings() {
    exercise_http2_deadline(true, Http2DeadlineCase::Response).await;
}

#[derive(Clone, Copy)]
enum Http2DeadlineCase {
    PartialFrame,
    HeaderBlock,
    Response,
}

async fn exercise_http2_deadline(tls: bool, case: Http2DeadlineCase) {
    use std::os::unix::fs::PermissionsExt;

    fn trickle(
        stream: &mut (impl std::io::Read + std::io::Write),
        case: Http2DeadlineCase,
        token: &str,
    ) {
        let continuation = matches!(case, Http2DeadlineCase::HeaderBlock);
        let response = matches!(case, Http2DeadlineCase::Response);
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\0\0\0\x04\0\0\0\0\0")
            .unwrap();
        if response {
            // Zero response DATA credit, followed by a fully authenticated
            // summary request. Request completion precedes delivery timing.
            stream
                .write_all(&[0, 0, 6, 4, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0])
                .unwrap();
        }
        stream.flush().unwrap();
        let mut settings = [0; 9];
        stream.read_exact(&mut settings).unwrap();
        assert_eq!(settings[3], 4);
        let length = u32::from_be_bytes([0, settings[0], settings[1], settings[2]]) as usize;
        assert!(length <= 16384);
        stream.read_exact(&mut vec![0; length]).unwrap();
        stream.write_all(&[0, 0, 0, 4, 1, 0, 0, 0, 0]).unwrap();
        if response {
            let mut headers = b"\x82\x87\x04\x0f/api/v1/cluster\x01\x09localhost".to_vec();
            let value = format!("Bearer {token}");
            assert!(value.len() < 127);
            headers.extend_from_slice(b"\x00\x0dauthorization");
            headers.push(value.len() as u8);
            headers.extend_from_slice(value.as_bytes());
            let mut frame = vec![0, 0, headers.len() as u8, 1, 5, 0, 0, 0, 1];
            frame.extend_from_slice(&headers);
            stream.write_all(&frame).unwrap();
            stream.flush().unwrap();
            let mut saw_response = false;
            for _ in 0..8 {
                let mut head = [0; 9];
                stream.read_exact(&mut head).unwrap();
                let size = u32::from_be_bytes([0, head[0], head[1], head[2]]) as usize;
                assert!(size <= 16384);
                let mut body = vec![0; size];
                stream.read_exact(&mut body).unwrap();
                if head[3] == 1 {
                    assert_eq!(&head[5..], &[0, 0, 0, 1]);
                    assert_eq!(head[4] & 1, 0);
                    assert_eq!(body.first(), Some(&0x88));
                    saw_response = true;
                    break;
                }
                assert_eq!(head[3], 4);
            }
            assert!(saw_response);
        } else if continuation {
            stream
                .write_all(&[0, 0, 1, 1, 1, 0, 0, 0, 1, 0x82])
                .unwrap();
        } else {
            stream.write_all(&[0, 0x40, 0, 1, 5, 0, 0, 0, 1]).unwrap();
        }
        stream.flush().unwrap();
        let started = std::time::Instant::now();
        let mut next = started + Duration::from_secs(1);
        let mut sent = 0;
        let mut received = 0;
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(7),
                "absolute assembly expiry"
            );
            if std::time::Instant::now() >= next && (!continuation || sent < 3) {
                let write = if response {
                    stream.write_all(b"\0\0\x08\x06\0\0\0\0\0blocked!")
                } else if continuation {
                    // Stay below the decoder's continuation-count limit so
                    // elapsed time, not frame count, must end this connection.
                    stream.write_all(&[0, 0, 0, 9, 0, 0, 0, 0, 1])
                } else {
                    stream.write_all(&[0x82])
                };
                if let Err(error) = write.and_then(|()| stream.flush()) {
                    assert!(matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::UnexpectedEof
                    ));
                    break;
                }
                sent += 1;
                next += Duration::from_secs(1);
            }
            let mut bytes = [0; 1024];
            match stream.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => {
                    received += count;
                    assert!(received <= 8192);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => {
                    assert!(
                        matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::BrokenPipe
                        ),
                        "{error}"
                    );
                    break;
                }
            }
        }
        assert!(sent >= 3);
        assert!(
            started.elapsed() >= Duration::from_secs(4),
            "premature protocol rejection is not deadline evidence"
        );
    }

    tokio::time::timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let state = root.path().join("state");
        let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let cert = root.path().join("server.pem");
        let key = root.path().join("server.key");
        std::fs::write(&cert, certificate.cert.pem()).unwrap();
        std::fs::write(&key, certificate.signing_key.serialize_pem()).unwrap();
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let mut worker = Worker(
            Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
                .arg("--state-dir")
                .arg(&state)
                .arg("--listen.clients")
                .arg(&socket)
                .args([
                    "--listen.clients",
                    &address.to_string(),
                    "--tls-cert",
                    cert.to_str().unwrap(),
                    "--tls-key",
                    key.to_str().unwrap(),
                ])
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
            ClusterAddress::Ip(address),
            HttClientOptions {
                tls_cert: Some(cert),
                credentials: Some(orishu::client::Credentials::Token(token.clone())),
                timeout: Some(Duration::from_secs(2)),
                ..Default::default()
            },
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while client.cluster().summary().await.is_err() {
                assert!(worker.0.try_wait().unwrap().is_none());
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let path = socket.clone();
        tokio::task::spawn_blocking(move || {
            if tls {
                let mut roots = rustls::RootCertStore::empty();
                roots.add(certificate.cert.der().clone()).unwrap();
                let mut config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
                    rustls::crypto::aws_lc_rs::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
                config.alpn_protocols = vec![b"h2".to_vec()];
                let socket =
                    std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let connection = rustls::ClientConnection::new(
                    std::sync::Arc::new(config),
                    rustls::pki_types::ServerName::IpAddress(address.ip().into()),
                )
                .unwrap();
                let mut stream = rustls::StreamOwned::new(connection, socket);
                trickle(&mut stream, case, &token);
                assert_eq!(stream.conn.alpn_protocol(), Some(b"h2".as_slice()));
            } else {
                let mut stream = std::os::unix::net::UnixStream::connect(path).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                trickle(&mut stream, case, &token);
            }
        })
        .await
        .unwrap();
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        assert_eq!(
            client.cluster().summary().await.unwrap().formation_id,
            initial.formation_id
        );
        worker.terminate().await;
    })
    .await
    .expect("assembly transport fixture budget");
}

#[tokio::test]
async fn http2_silent_partial_headers_frames_and_responses_expire() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    tokio::time::timeout(Duration::from_secs(18), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        let initial = summary(&mut worker, &socket).await;
        let mut held = Vec::new();
        for case in 0..3 {
            let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
            stream
                .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
                .await
                .unwrap();
            // All cases withhold stream response credit. Case 2 completes
            // the request and proves the response reached the blocked state.
            h2_frame(&mut stream, 4, 0, 0, &[0, 4, 0, 0, 0, 0]).await;
            let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
            assert_eq!((kind, flags, id), (4, 0, 0));
            h2_frame(&mut stream, 4, 1, 0, &[]).await;
            let mut get = vec![0x82, 0x86, 0x04];
            let path = b"/api/v1/cluster";
            get.push(path.len() as u8);
            get.extend_from_slice(path);
            get.extend_from_slice(b"\x01\x09localhost");
            match case {
                0 => h2_frame(&mut stream, 1, 1, 1, &get).await,
                1 => {
                    // Complete frame header declares two bytes, only one
                    // arrives. The decoder cannot dispatch this frame.
                    stream
                        .write_all(&[0, 0, 2, 1, 5, 0, 0, 0, 1, 0x82])
                        .await
                        .unwrap();
                }
                2 => {
                    h2_frame(&mut stream, 1, 5, 1, &get).await;
                    loop {
                        let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
                        if id == 1 {
                            assert_eq!(kind, 1);
                            assert_eq!(flags & 1, 0);
                            break;
                        }
                        assert_eq!(id, 0);
                        assert_eq!(kind, 4);
                    }
                }
                _ => unreachable!(),
            }
            let early = tokio::time::timeout(Duration::from_millis(100), async {
                let mut bytes = [0; 1024];
                let mut total = 0;
                loop {
                    let count = stream.read(&mut bytes).await.unwrap();
                    assert_ne!(
                        count, 0,
                        "fixture must remain open, not fail parsing immediately"
                    );
                    total += count;
                    assert!(total <= 8192);
                }
            })
            .await;
            assert!(
                early.is_err(),
                "silent connection remains pending before idle expiry"
            );
            held.push(stream);
        }
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
        for mut stream in held {
            // Never send END_HEADERS, missing payload, WINDOW_UPDATE or
            // cancellation. EOF/reset must be caused by the live server.
            tokio::time::timeout_at(deadline, async {
                let mut total = 0;
                let mut bytes = [0; 1024];
                loop {
                    match stream.read(&mut bytes).await {
                        Ok(0) => break,
                        Ok(count) => {
                            total += count;
                            assert!(total <= 8192, "bounded terminal response bytes");
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                        Err(error) => panic!("unexpected idle connection error: {error}"),
                    }
                }
            })
            .await
            .expect("silent HTTP/2 connection must expire without client cancellation");
        }
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        worker.terminate().await;
    })
    .await
    .expect("HTTP/2 silent-client fixture deadline");
}

#[tokio::test]
async fn http2_continuation_budget_and_stream_binding_are_enforced() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;

    tokio::time::timeout(Duration::from_secs(10), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        let initial = summary(&mut worker, &socket).await;
        // None: legitimate fragmented head; Some: required GOAWAY reason.
        for expected in [None, Some(11_u32), Some(1_u32)] {
            let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
            stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await.unwrap();
            h2_frame(&mut stream, 4, 0, 0, &[]).await;
            let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
            assert_eq!((kind, flags, id), (4, 0, 0));
            h2_frame(&mut stream, 4, 1, 0, &[]).await;
            let mut get = vec![0x82, 0x86, 0x04];
            let path = b"/api/v1/cluster";
            get.push(path.len() as u8);
            get.extend_from_slice(path);
            get.extend_from_slice(b"\x01\x09localhost");
            // END_STREAM, but deliberately no END_HEADERS. No handler may
            // receive this request until the same-stream block is completed.
            h2_frame(&mut stream, 1, 1, 1, &get).await;
            if expected == Some(1) {
                h2_frame(&mut stream, 9, 4, 3, &[]).await;
            } else {
                for _ in 0..5 {
                    h2_frame(&mut stream, 9, 0, 1, &[]).await;
                }
                // A final sixth frame is allowed; a sixth non-final frame
                // exceeds the pinned decoder's continuation-work budget.
                h2_frame(&mut stream, 9, if expected.is_none() { 4 } else { 0 }, 1, &[]).await;
            }
            tokio::time::timeout(Duration::from_secs(1), async {
                let mut response = Vec::new();
                for _ in 0..16 {
                    let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
                    if kind == 7 {
                        assert_eq!(id, 0);
                        assert!(body.len() >= 8);
                        assert_eq!(Some(u32::from_be_bytes(body[4..8].try_into().unwrap())), expected);
                        return;
                    }
                    if id == 0 {
                        assert_eq!(kind, 4);
                        continue;
                    }
                    assert!(expected.is_none(), "incomplete/invalid head must not reach a handler");
                    assert_eq!(id, 1);
                    assert!(kind == 1 || kind == 0);
                    if kind == 0 {
                        response.extend_from_slice(&body);
                        assert!(response.len() <= 8192);
                    }
                    if flags & 1 != 0 {
                        let envelope: orishu::model::ApiResponse = orishu_worker::peer::codec::decode(&response).unwrap();
                        assert!(matches!(envelope, orishu::model::ApiResponse::Ok { data: Some(orishu::model::ResponseData::ClusterSummary(view)) }
                            if view.formation_id == initial.formation_id && view.source_node_id == initial.source_node_id));
                        return;
                    }
                }
                panic!("bounded frame budget without expected terminal outcome");
            }).await.expect("fragmented-header acceptance/rejection deadline");
            assert_eq!(summary(&mut worker, &socket).await.formation_id, initial.formation_id);
        }
        worker.terminate().await;
    }).await.expect("HTTP/2 continuation fixture deadline");
}

#[tokio::test]
async fn http2_connection_response_credit_is_enforced_and_recovers() {
    exercise_connection_response_credit(false).await;
}

#[tokio::test]
async fn http2_exhausted_connection_credit_expires_despite_pings() {
    exercise_connection_response_credit(true).await;
}

async fn exercise_connection_response_credit(expire: bool) {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        let initial = summary(&mut worker, &socket).await;
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await.unwrap();
        h2_frame(&mut stream, 4, 0, 0, &[]).await;
        let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
        assert_eq!((kind, flags, id), (4, 0, 0));
        h2_frame(&mut stream, 4, 1, 0, &[]).await;
        let mut get = vec![0x82, 0x86, 0x04];
        let path = b"/api/v1/cluster";
        get.push(path.len() as u8);
        get.extend_from_slice(path);
        get.extend_from_slice(b"\x01\x09localhost");
        let mut credit = 65535_usize;
        let mut pending = std::collections::BTreeMap::new();
        let mut next = 0;
        // Consume the default connection credit with finite legitimate reads,
        // without sending any connection WINDOW_UPDATE. Each stream has its
        // own full initial credit, so this cannot be a stream-window stall.
        for requested in (1..1024).step_by(2) {
            h2_frame(&mut stream, 1, 5, requested, &get).await;
            let mut response = Vec::new();
            loop {
                let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
                if id == 0 { assert_eq!(kind, 4); continue; }
                assert_eq!(id, requested);
                assert!(kind == 1 || kind == 0);
                if kind == 0 {
                    assert_eq!(flags & 8, 0, "fixture expects unpadded DATA");
                    assert!(body.len() <= credit, "server exceeded connection credit");
                    credit -= body.len();
                    response.extend_from_slice(&body);
                    assert!(response.len() <= 8192);
                }
                if flags & 1 != 0 {
                    let envelope: orishu::model::ApiResponse = orishu_worker::peer::codec::decode(&response).unwrap();
                    assert!(matches!(envelope, orishu::model::ApiResponse::Ok { data: Some(orishu::model::ResponseData::ClusterSummary(view)) }
                        if view.formation_id == initial.formation_id && view.member_count == 1));
                    break;
                }
                if credit == 0 { pending.insert(requested, response); break; }
            }
            if credit == 0 { next = requested + 2; break; }
        }
        assert_eq!(credit, 0, "finite requests must actually exhaust credit");
        assert_ne!(next, 0);
        h2_frame(&mut stream, 1, 5, next, &get).await;
        loop {
            let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
            if id == next { assert_eq!(kind, 1); assert_eq!(flags & 1, 0); break; }
            assert_eq!(id, 0, "no stream may emit DATA at zero connection credit");
        }
        pending.insert(next, Vec::new());
        // A PING still progresses with no DATA credit; its ACK is an ordered
        // wire observation, not a sleep offered as proof of a flow-control stall.
        h2_frame(&mut stream, 6, 0, 0, b"connzero").await;
        loop {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            assert_eq!(id, 0, "DATA must not bypass exhausted connection credit");
            if kind == 6 { assert_eq!(flags, 1); assert_eq!(body, b"connzero"); break; }
        }
        assert_eq!(summary(&mut worker, &socket).await.member_count, 1);
        if expire {
            await_response_credit_expiry(&mut stream).await;
            assert_eq!(summary(&mut worker, &socket).await.formation_id, initial.formation_id);
            worker.terminate().await;
            return;
        }
        h2_frame(&mut stream, 8, 0, 0, &65535_u32.to_be_bytes()).await;
        while !pending.is_empty() {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            if id == 0 { assert_eq!(kind, 4); continue; }
            assert_eq!(kind, 0);
            let response = pending.get_mut(&id).expect("only blocked responses resume");
            response.extend_from_slice(&body);
            assert!(response.len() <= 8192);
            if flags & 1 != 0 {
                let envelope: orishu::model::ApiResponse = orishu_worker::peer::codec::decode(response).unwrap();
                assert!(matches!(envelope, orishu::model::ApiResponse::Ok { data: Some(orishu::model::ResponseData::ClusterSummary(view)) }
                    if view.formation_id == initial.formation_id && view.source_node_id == initial.source_node_id));
                pending.remove(&id);
            }
        }
        worker.terminate().await;
        drop(stream);
    }).await.expect("HTTP/2 connection-credit fixture deadline");
}

#[tokio::test]
async fn http2_withheld_response_credit_expires_despite_pings() {
    exercise_response_delivery(false).await;
}

#[tokio::test]
async fn http2_reset_and_completed_response_cancel_delivery_deadlines() {
    exercise_response_delivery(true).await;
}

async fn exercise_response_delivery(cancel: bool) {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        let initial = summary(&mut worker, &socket).await;
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        h2_frame(&mut stream, 4, 0, 0, &[0, 4, 0, 0, 0, 0]).await;
        assert_eq!(read_h2_frame(&mut stream).await.0, 4);
        h2_frame(&mut stream, 4, 1, 0, &[]).await;
        h2_frame(
            &mut stream,
            1,
            5,
            1,
            b"\x82\x86\x04\x0f/api/v1/cluster\x01\x09localhost",
        )
        .await;
        // Observe response HEADERS before starting the delivery assertion:
        // this is not an unfinished request or a handler still computing.
        loop {
            let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
            if id == 1 {
                assert_eq!(kind, 1);
                assert_eq!(flags & 1, 0);
                break;
            }
            assert_eq!(kind, 4);
        }
        if cancel {
            h2_frame(&mut stream, 3, 0, 1, &8_u32.to_be_bytes()).await;
            h2_frame(&mut stream, 4, 0, 0, &[0, 4, 0, 0, 255, 255]).await;
            h2_frame(
                &mut stream,
                1,
                5,
                3,
                b"\x82\x86\x04\x0f/api/v1/cluster\x01\x09localhost",
            )
            .await;
            let mut body = Vec::new();
            loop {
                let (kind, flags, id, payload) = read_h2_frame(&mut stream).await;
                if id == 0 {
                    assert_eq!(kind, 4);
                    continue;
                }
                assert_eq!(id, 3);
                if kind == 1 {
                    assert_eq!(payload.first(), Some(&0x88));
                } else {
                    assert_eq!(kind, 0);
                    body.extend_from_slice(&payload);
                    assert!(body.len() <= 8192);
                }
                if flags & 1 != 0 {
                    break;
                }
            }
            let envelope: orishu::model::ApiResponse =
                orishu_worker::peer::codec::decode(&body).unwrap();
            assert!(matches!(envelope, orishu::model::ApiResponse::Ok { .. }));
            for _ in 0..6 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                h2_frame(&mut stream, 6, 0, 0, b"cleared!").await;
                let (kind, flags, id, bytes) = read_h2_frame(&mut stream).await;
                assert_eq!((kind, flags, id), (6, 1, 0));
                assert_eq!(bytes, b"cleared!");
            }
        } else {
            await_response_credit_expiry(&mut stream).await;
        }
        assert_eq!(
            summary(&mut worker, &socket).await.formation_id,
            initial.formation_id
        );
        drop(stream);
        worker.terminate().await;
    })
    .await
    .expect("withheld-credit fixture budget");
}

async fn await_response_credit_expiry(stream: &mut tokio::net::UnixStream) {
    let started = tokio::time::Instant::now();
    let mut pings = tokio::time::interval(Duration::from_secs(1));
    let mut header = [0; 9];
    let mut seen = 0;
    let mut header_len = 0;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(7), async {
        loop {
            tokio::select! {
                _ = pings.tick() => {
                    if let Err(error) = stream.write_all(b"\0\0\x08\x06\0\0\0\0\0blocked!").await {
                        assert!(matches!(error.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset));
                        break;
                    }
                },
                read = stream.read(&mut header[header_len..]) => {
                    match read {
                        Err(error) => {
                            assert!(matches!(error.kind(), std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset));
                            break;
                        }
                        Ok(0) => break,
                        Ok(count) => {
                            header_len += count;
                            if header_len < header.len() { continue; }
                            let length = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
                            assert!(length <= 16384);
                            let mut body = vec![0; length];
                            stream.read_exact(&mut body).await.unwrap();
                            assert_eq!(header[3], 6, "no response DATA may bypass zero credit");
                            assert_eq!(header[4], 1);
                            assert_eq!(body, b"blocked!");
                            seen += 1;
                            assert!(seen <= 8);
                            header_len = 0;
                        }
                    }
                }
            }
        }
    }).await.expect("response must expire despite active connection traffic");
    assert!(seen >= 3 && started.elapsed() >= Duration::from_secs(4));
}

#[tokio::test]
async fn http2_zero_response_window_isolated_and_recovers() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let state = root.path().join("state");
        let mut worker = start(&state, &socket);
        let initial = summary(&mut worker, &socket).await;
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        // SETTINGS_INITIAL_WINDOW_SIZE = 0: HEADERS may flow, DATA may not.
        h2_frame(&mut stream, 4, 0, 0, &[0, 4, 0, 0, 0, 0]).await;
        let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
        assert_eq!((kind, flags, id), (4, 0, 0));
        h2_frame(&mut stream, 4, 1, 0, &[]).await;
        let mut get = vec![0x82, 0x86, 0x04];
        let path = b"/api/v1/cluster";
        get.push(path.len() as u8);
        get.extend_from_slice(path);
        get.extend_from_slice(b"\x01\x09localhost");
        for requested in (1..32).step_by(2) {
            h2_frame(&mut stream, 1, 5, requested, &get).await;
            loop {
                let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
                if id == requested {
                    assert_eq!(kind, 1, "response must start with HEADERS");
                    assert_eq!(flags & 1, 0, "zero-credit response must remain open");
                    break;
                }
                assert_eq!(id, 0, "DATA must not pass zero stream credit");
            }
        }
        h2_frame(&mut stream, 1, 5, 33, &get).await;
        loop {
            let (kind, _, id, body) = read_h2_frame(&mut stream).await;
            if id == 33 {
                assert_eq!(kind, 3);
                assert_eq!(body, 7_u32.to_be_bytes());
                break;
            }
            assert_eq!(id, 0, "blocked responses must retain stream capacity");
        }
        let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
        let client = HttpClusterClient::new(
            ClusterAddress::UnixSocket(socket.clone()),
            HttClientOptions {
                credentials: Some(orishu::client::Credentials::Token(token.trim().to_owned())),
                timeout: Some(Duration::from_secs(1)),
                ..Default::default()
            },
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            assert_eq!(client.cluster().summary().await.unwrap().member_count, 1);
            assert!(
                client
                    .membership()
                    .set_lock(&orishu::model::cluster::LockRequest {
                        schema_version: 1,
                        operation_id: "window-lock".parse().unwrap(),
                        formation_id: initial.formation_id.clone(),
                        locked: true,
                    })
                    .await
                    .unwrap()
                    .locked
            );
        })
        .await
        .expect("window-blocked responses must not block independent control");
        h2_frame(&mut stream, 3, 0, 1, &8_u32.to_be_bytes()).await;
        h2_frame(&mut stream, 6, 0, 0, b"window01").await;
        loop {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            assert_eq!(id, 0);
            if kind == 6 {
                assert_eq!(flags, 1);
                assert_eq!(body, b"window01");
                break;
            }
        }
        h2_frame(&mut stream, 1, 5, 35, &get).await;
        loop {
            let (kind, flags, id, _) = read_h2_frame(&mut stream).await;
            if id == 35 {
                assert_eq!(kind, 1);
                assert_eq!(flags & 1, 0);
                break;
            }
            assert_eq!(id, 0);
        }
        // Return credit to one old response and the replacement independently.
        // No other stalled stream receives credit or is closed by the fixture.
        for requested in [3, 35] {
            h2_frame(&mut stream, 8, 0, requested, &65535_u32.to_be_bytes()).await;
            let mut response = Vec::new();
            loop {
                let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
                if id == requested {
                    assert_eq!(kind, 0);
                    response.extend_from_slice(&body);
                    assert!(response.len() <= 8192);
                    if flags & 1 != 0 {
                        break;
                    }
                } else {
                    assert_eq!(id, 0, "uncredited streams must not send DATA");
                }
            }
            let envelope: orishu::model::ApiResponse =
                orishu_worker::peer::codec::decode(&response).unwrap();
            let orishu::model::ApiResponse::Ok {
                data: Some(orishu::model::ResponseData::ClusterSummary(view)),
            } = envelope
            else {
                panic!("expected real summary")
            };
            assert_eq!(view.formation_id, initial.formation_id);
            assert_eq!(view.source_node_id, initial.source_node_id);
            assert_eq!(view.membership_locked, requested == 35);
        }
        worker.terminate().await; // Fourteen response streams still have zero credit.
        drop(stream);
    })
    .await
    .expect("HTTP/2 response-window fixture deadline");
}

#[tokio::test]
async fn http2_stream_capacity_is_advertised_enforced_and_reclaimed() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let state = root.path().join("state");
        let mut worker = start(&state, &socket);
        summary(&mut worker, &socket).await;
        let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        h2_frame(&mut stream, 4, 0, 0, &[]).await;
        let (kind, flags, id, settings) = read_h2_frame(&mut stream).await;
        assert_eq!((kind, flags, id), (4, 0, 0));
        assert_eq!(settings.len() % 6, 0);
        let settings: std::collections::BTreeMap<_, _> = settings
            .chunks_exact(6)
            .map(|entry| {
                (
                    u16::from_be_bytes(entry[..2].try_into().unwrap()),
                    u32::from_be_bytes(entry[2..].try_into().unwrap()),
                )
            })
            .collect();
        assert_eq!(
            settings.get(&3),
            Some(&16),
            "explicit concurrent stream limit"
        );
        assert_eq!(
            settings.get(&6),
            Some(&8192),
            "explicit decoded header limit"
        );
        assert_eq!(settings.get(&4).copied().unwrap_or(65535), 65535);
        assert_eq!(settings.get(&5).copied().unwrap_or(16384), 16384);
        h2_frame(&mut stream, 4, 1, 0, &[]).await;
        // HPACK static pseudo-headers and literal, non-indexed sensitive fields.
        // HTTP/2 stream occupancy is proven by refusal of the seventeenth
        // stream, not HTTP/1-specific informational-response behavior.
        let mut headers = vec![0x83, 0x86, 0x04]; // POST, http, literal :path
        let path = b"/api/v1/cluster/lock";
        headers.push(path.len() as u8);
        headers.extend_from_slice(path);
        for (name, value) in [
            (":authority", "localhost".to_owned()),
            ("content-type", "application/cbor".to_owned()),
            ("content-length", "4096".to_owned()),
            ("authorization", format!("Bearer {}", token.trim())),
        ] {
            assert!(name.len() < 127 && value.len() < 127);
            headers.extend_from_slice(&[0, name.len() as u8]);
            headers.extend_from_slice(name.as_bytes());
            headers.push(value.len() as u8);
            headers.extend_from_slice(value.as_bytes());
        }
        let started = tokio::time::Instant::now();
        for id in (1..32).step_by(2) {
            h2_frame(&mut stream, 1, 4, id, &headers).await;
        }
        h2_frame(&mut stream, 1, 4, 33, &headers).await;
        loop {
            let (kind, _, id, body) = read_h2_frame(&mut stream).await;
            if id == 33 {
                assert_eq!(kind, 3);
                assert_eq!(body, 7_u32.to_be_bytes());
                break;
            }
            assert_eq!(id, 0);
        }
        assert_eq!(summary(&mut worker, &socket).await.member_count, 1);
        h2_frame(&mut stream, 3, 0, 1, &8_u32.to_be_bytes()).await;
        h2_frame(&mut stream, 6, 0, 0, b"cancel01").await;
        loop {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            assert_eq!(id, 0);
            if kind == 6 { assert_eq!(flags, 1); assert_eq!(body, b"cancel01"); break; }
        }
        h2_frame(&mut stream, 1, 4, 35, &headers).await;
        h2_frame(&mut stream, 0, 1, 35, &[0; 4096]).await;
        let mut response = Vec::new();
        loop {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            if id == 35 {
                assert!(kind == 1 || kind == 0, "replacement must not reset");
                if kind == 0 { response.extend_from_slice(&body); assert!(response.len() <= 8192); }
                if flags & 1 != 0 { break; }
            } else {
                assert_eq!(id, 0);
            }
        }
        let response: orishu::model::ApiResponse = orishu_worker::peer::codec::decode(&response).unwrap();
        assert!(matches!(response, orishu::model::ApiResponse::Error { code, .. } if code == "InvalidRequest"),
            "reclaimed stream must reach authenticated body validation, not overload");
        // A decoded header list above 8 KiB is refused, even though the
        // encoded HEADERS frame fits the separately advertised 16 KiB cap.
        let mut get = vec![0x82, 0x86, 0x04];
        let path = b"/api/v1/cluster";
        get.push(path.len() as u8);
        get.extend_from_slice(path);
        get.extend_from_slice(b"\x01\x09localhost");
        let mut oversized = get.clone();
        oversized.extend_from_slice(b"\x00\x06x-long");
        oversized.extend_from_slice(&[127, 169, 69]); // HPACK non-Huffman length 9000.
        oversized.resize(oversized.len() + 9000, b'x');
        h2_frame(&mut stream, 1, 5, 37, &oversized).await;
        loop {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            if id == 37 {
                assert_eq!((kind, flags), (1, 5));
                // Golden HPACK literal with indexed :status name (8), then
                // Huffman "431". No prior response has indexed that value.
                assert_eq!(body, [0x48, 0x83, 0x69, 0x90, 0xff]);
                break;
            } else { assert_eq!(id, 0); }
        }
        h2_frame(&mut stream, 1, 5, 39, &get).await;
        let mut response = Vec::new();
        loop {
            let (kind, flags, id, body) = read_h2_frame(&mut stream).await;
            if id == 39 {
                assert!(kind == 1 || kind == 0);
                if kind == 0 { response.extend_from_slice(&body); assert!(response.len() <= 8192); }
                if flags & 1 != 0 { break; }
            } else { assert_eq!(id, 0); }
        }
        let response: orishu::model::ApiResponse = orishu_worker::peer::codec::decode(&response).unwrap();
        assert!(matches!(response, orishu::model::ApiResponse::Ok { data: Some(orishu::model::ResponseData::ClusterSummary(view)) }
            if view.member_count == 1 && !view.membership_locked));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "body expiry masked stream reclamation"
        );
        worker.terminate().await; // Fifteen unfinished HTTP/2 streams remain held.
        drop(stream);
    })
    .await
    .expect("HTTP/2 stream capacity fixture deadline");
}

async fn read_http_summary(stream: &mut tokio::net::UnixStream) {
    use tokio::io::AsyncReadExt;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(stream.read_u8().await.unwrap());
        assert!(head.len() <= 8192);
    }
    let head = String::from_utf8(head).unwrap();
    assert!(head.starts_with("HTTP/1.1 200 "));
    let length: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .expect("bounded response length");
    assert!(length <= 65536);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
}

#[tokio::test]
async fn incomplete_headers_bound_connections_and_expire() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    const GET: &[u8] = b"GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n\r\n";
    tokio::time::timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = root.path().join("api.sock");
        let mut worker = start(&root.path().join("state"), &socket);
        summary(&mut worker, &socket).await;
        let started = tokio::time::Instant::now();
        let mut held = Vec::new();
        // A real completed request proves each socket was accepted, not merely
        // connected to the OS backlog. Keep one connection for legitimate reads.
        for _ in 0..64 {
            let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
            stream.write_all(GET).await.unwrap();
            read_http_summary(&mut stream).await;
            held.push(stream);
        }
        let mut control = held.pop().unwrap();
        for stream in &mut held {
            stream
                .write_all(b"GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n")
                .await
                .unwrap();
        }
        let mut waiting = tokio::net::UnixStream::connect(&socket).await.unwrap();
        waiting.write_all(GET).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), waiting.read_u8())
                .await
                .is_err(),
            "excess connection was admitted at full capacity"
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            control.write_all(GET).await.unwrap();
            read_http_summary(&mut control).await;
            drop(held.pop());
            read_http_summary(&mut waiting).await;
        })
        .await
        .expect("status and released capacity must progress");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "expiry masked capacity release"
        );
        // Retain incomplete clients until server-driven expiry. EOF or reset is
        // terminal; bounded HTTP timeout errors may precede EOF.
        tokio::time::timeout(Duration::from_secs(7), async {
            for stream in &mut held {
                let mut bytes = 0;
                let mut buffer = [0; 1024];
                loop {
                    match stream.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(count) => {
                            bytes += count;
                            assert!(bytes <= 8192);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                        Err(error) => panic!("header expiry IO: {error}"),
                    }
                }
            }
        })
        .await
        .expect("incomplete headers must expire");
        assert!(summary(&mut worker, &socket).await.member_count == 1);
        worker.terminate().await;
    })
    .await
    .expect("header pressure fixture deadline");
}

#[tokio::test]
async fn oversized_and_excessive_headers_are_rejected_before_handlers() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.path().join("api.sock");
    let mut worker = start(&root.path().join("state"), &socket);
    summary(&mut worker, &socket).await;
    for headers in [
        format!("X-Large: {}\r\n", "x".repeat(9000)),
        "X-Count: x\r\n".repeat(33),
    ] {
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
            let request =
                format!("GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n{headers}\r\n");
            if let Err(error) = stream.write_all(request.as_bytes()).await {
                assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
            }
            let mut response = Vec::new();
            let result = stream.take(8193).read_to_end(&mut response).await;
            if let Err(error) = result {
                // Early header rejection may reset the read side while unread
                // request bytes remain. It must still have sent the real 431.
                assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
            }
            assert!(response.len() <= 8192);
            assert!(
                response.starts_with(b"HTTP/1.1 431 "),
                "must receive actual header rejection"
            );
        })
        .await
        .expect("header rejection deadline");
    }
    assert_eq!(summary(&mut worker, &socket).await.member_count, 1);
    worker.terminate().await;
}

#[tokio::test]
async fn shutdown_cancels_unfinished_client_requests() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = root.path().join("state");
    let socket = root.path().join("api.sock");
    let mut worker = start(&state, &socket);
    summary(&mut worker, &socket).await;
    let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
    let mut body = tokio::net::UnixStream::connect(&socket).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        let headers = format!(
            "POST /api/v1/membership/leaves HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/cbor\r\nContent-Length: 4096\r\nExpect: 100-continue\r\nAuthorization: Bearer {}\r\n\r\n",
            token.trim()
        );
        body.write_all(headers.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            response.push(body.read_u8().await.unwrap());
            assert!(response.len() <= 1024);
        }
        assert!(response.starts_with(b"HTTP/1.1 100 Continue\r\n"));
        body.write_all(b"\xa0").await.unwrap();
    })
    .await
    .expect("authenticated body admission deadline");
    let mut headers = tokio::net::UnixStream::connect(&socket).await.unwrap();
    headers
        .write_all(b"GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .unwrap();
    worker.terminate().await;
    // Retain both sockets until process exit; test cleanup cannot unblock it.
    assert!(!socket.exists());
    drop((body, headers));
}

#[tokio::test]
async fn real_inspection_reports_identity_and_rejects_unsupported_sources() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.path().join("api.sock");
    let mut worker = start(&root.path().join("state"), &socket);
    let initial = summary(&mut worker, &socket).await;
    let client = HttpClusterClient::new(
        ClusterAddress::UnixSocket(socket),
        HttClientOptions {
            timeout: Some(Duration::from_secs(2)),
            ..Default::default()
        },
    )
    .unwrap();
    let inspection = client
        .membership()
        .get(
            &initial.source_node_id,
            Some(orishu::model::node::InspectSource::Indirect),
        )
        .await
        .unwrap();
    assert_eq!(inspection.formation_id, initial.formation_id);
    assert_eq!(inspection.node_id, initial.source_node_id);
    assert_eq!(inspection.source_node_id, initial.source_node_id);
    assert_eq!(inspection.liveness, orishu::model::node::MemberState::Alive);
    assert_eq!(
        inspection.view,
        orishu::model::cluster::SummaryView::LocalAtRequest
    );
    assert!(!inspection.accepts.peers);
    let members = client.membership().list(&Default::default()).await.unwrap();
    assert_eq!(members, vec![inspection.clone()]);
    assert!(
        client
            .membership()
            .list(&orishu::model::cluster::MembersSelector {
                name: Some("not-this-worker".into()),
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        client
            .membership()
            .list(&orishu::model::cluster::MembersSelector {
                member_state: Some("invalid-state".into()),
                ..Default::default()
            })
            .await
            .is_err()
    );
    assert_eq!(
        client
            .membership()
            .get(&inspection.node_id, None)
            .await
            .unwrap(),
        inspection
    );
    assert!(
        client
            .membership()
            .get(&"unknown-member".parse().unwrap(), None)
            .await
            .is_err()
    );
    for source in [
        orishu::model::node::InspectSource::Direct,
        orishu::model::node::InspectSource::BestEffort,
    ] {
        assert!(
            client
                .membership()
                .get(&inspection.node_id, Some(source))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn every_mutation_rejects_wrong_credential_classes_before_state_change() {
    exercise_mutation_credentials(false, false).await;
}

#[tokio::test]
async fn tls_mutations_reject_wrong_credential_classes_before_state_change() {
    exercise_mutation_credentials(true, false).await;
}

#[tokio::test]
async fn http2_mutations_reject_wrong_credential_classes_before_state_change() {
    exercise_mutation_credentials(false, true).await;
}

#[tokio::test]
async fn tls_http2_mutations_reject_wrong_credential_classes_before_state_change() {
    exercise_mutation_credentials(true, true).await;
}

fn h2_authorization_response<S: std::io::Read + std::io::Write>(
    stream: &mut S,
    route: &str,
    credential: &str,
    body: &[u8],
    tls: bool,
    accepted: bool,
) -> Vec<u8> {
    fn frame(out: &mut Vec<u8>, kind: u8, flags: u8, id: u32, body: &[u8]) {
        assert!(body.len() <= 16384);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        out.extend_from_slice(&[kind, flags]);
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(body);
    }
    fn literal(out: &mut Vec<u8>, name: &str, value: &str) {
        assert!(name.len() < 127 && value.len() < 127);
        out.extend_from_slice(&[0, name.len() as u8]);
        out.extend_from_slice(name.as_bytes());
        out.push(value.len() as u8);
        out.extend_from_slice(value.as_bytes());
    }
    let mut headers = vec![0x83, if tls { 0x87 } else { 0x86 }]; // POST, scheme.
    literal(&mut headers, ":path", &format!("/api/v1/{route}"));
    literal(&mut headers, ":authority", "localhost");
    literal(&mut headers, "content-type", "application/cbor");
    literal(&mut headers, "content-length", &body.len().to_string());
    for line in credential.lines() {
        let (name, value) = line.split_once(':').unwrap();
        literal(&mut headers, &name.to_ascii_lowercase(), value.trim());
    }
    let mut request = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".to_vec();
    frame(&mut request, 4, 0, 0, &[]);
    frame(&mut request, 1, 4, 1, &headers);
    frame(&mut request, 0, 1, 1, body);
    stream.write_all(&request).unwrap();
    let mut response = Vec::new();
    let mut status_seen = false;
    for _ in 0..32 {
        let mut header = [0; 9];
        stream.read_exact(&mut header).unwrap();
        let length = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
        assert!(length <= 16384);
        let mut payload = vec![0; length];
        stream.read_exact(&mut payload).unwrap();
        let id = u32::from_be_bytes(header[5..].try_into().unwrap()) & 0x7fff_ffff;
        if id == 0 {
            assert_eq!(header[3], 4, "only connection SETTINGS expected");
            if header[4] & 1 == 0 {
                let mut ack = Vec::new();
                frame(&mut ack, 4, 1, 0, &[]);
                stream.write_all(&ack).unwrap();
            }
            continue;
        }
        assert_eq!(id, 1);
        match header[3] {
            1 => {
                // First response on a fresh HPACK context: literal :status
                // with Huffman "401". An error body alone is insufficient.
                if accepted {
                    // Static :status 200 or literal Huffman :status 202.
                    assert!(
                        payload.starts_with(&[0x88])
                            || payload.starts_with(&[0x48, 0x82, 0x10, 0x05]),
                        "HTTP/2 must report 200 or 202"
                    );
                } else {
                    assert!(
                        payload.starts_with(&[0x48, 0x82, 0x68, 0x01]),
                        "HTTP/2 must report 401"
                    );
                }
                assert_eq!(header[4] & 4, 4);
                status_seen = true;
            }
            0 => {
                response.extend_from_slice(&payload);
                assert!(response.len() <= 8192);
            }
            _ => panic!("unexpected HTTP/2 response frame"),
        }
        if header[4] & 1 != 0 {
            assert!(status_seen);
            return response;
        }
    }
    panic!("HTTP/2 response frame budget exhausted");
}

async fn exercise_mutation_credentials(tls: bool, http2: bool) {
    use orishu::model::cluster::{JoinRequest, LeaveRequest, LockRequest};
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let target_state = root.path().join("target");
        let target_socket = root.path().join("target.sock");
        let mut target = Worker(Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
            .args(["--state-dir", target_state.to_str().unwrap(), "--listen.clients", target_socket.to_str().unwrap(),
                   "--listen.peers", "127.0.0.1:0", "--accepts.peers", "true"])
            .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
        let target_initial = summary(&mut target, &target_socket).await;
        let target_token = std::fs::read_to_string(target_state.join("operator.token")).unwrap().trim().to_owned();
        let client = |socket, token| HttpClusterClient::new(ClusterAddress::UnixSocket(socket), HttClientOptions {
            credentials: Some(orishu::client::Credentials::Token(token)), timeout: Some(Duration::from_secs(2)), ..Default::default()
        }).unwrap();
        let material = client(target_socket.clone(), target_token.clone()).get_join_token().await.unwrap();
        assert!(material.introducer_ready);
        let source_state = root.path().join("source");
        let source_socket = root.path().join("source.sock");
        let tls_setup = if tls {
            let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
            let cert_path = root.path().join("source.pem");
            let key_path = root.path().join("source.key");
            std::fs::write(&cert_path, certificate.cert.pem()).unwrap();
            std::fs::write(&key_path, certificate.signing_key.serialize_pem()).unwrap();
            let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = reservation.local_addr().unwrap();
            drop(reservation);
            let mut roots = rustls::RootCertStore::empty();
            roots.add(certificate.cert.der().clone()).unwrap();
            let mut config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
                .with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth();
            config.alpn_protocols = vec![if http2 { b"h2".to_vec() } else { b"http/1.1".to_vec() }];
            Some((address, cert_path, key_path, std::sync::Arc::new(config)))
        } else { None };
        let mut source = if let Some((address, cert, key, _)) = &tls_setup {
            Worker(Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
                .args(["--state-dir", source_state.to_str().unwrap(), "--listen.clients", source_socket.to_str().unwrap(),
                       "--listen.clients", &address.to_string(), "--tls-cert", cert.to_str().unwrap(), "--tls-key", key.to_str().unwrap()])
                .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap())
        } else { start(&source_state, &source_socket) };
        let initial = summary(&mut source, &source_socket).await;
        let token = std::fs::read_to_string(source_state.join("operator.token")).unwrap().trim().to_owned();
        let authorized = if let Some((address, cert, _, _)) = &tls_setup {
            HttpClusterClient::new(ClusterAddress::Ip(*address), HttClientOptions {
                tls_cert: Some(cert.clone()), credentials: Some(orishu::client::Credentials::Token(token.clone())),
                timeout: Some(Duration::from_secs(2)), ..Default::default()
            }).unwrap()
        } else { client(source_socket.clone(), token.clone()) };
        tokio::time::timeout(Duration::from_secs(3), async {
            while authorized.cluster().summary().await.is_err() {
                assert!(source.0.try_wait().unwrap().is_none());
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("authenticated listener readiness");
        let lock = LockRequest { schema_version: 1, operation_id: "auth-lock".parse().unwrap(), formation_id: initial.formation_id.clone(), locked: true };
        let unlock = LockRequest { operation_id: "auth-unlock".parse().unwrap(), locked: false, ..lock.clone() };
        let leave = LeaveRequest { schema_version: 1, operation_id: "auth-leave".parse().unwrap(), formation_id: initial.formation_id.clone() };
        let join = JoinRequest { schema_version: 1, operation_id: "auth-join".parse().unwrap(), formation_id: initial.formation_id.clone(), material: material.clone() };
        let requests = [
            ("cluster/lock", orishu_worker::peer::codec::encode(&lock).unwrap()),
            ("cluster/lock", orishu_worker::peer::codec::encode(&unlock).unwrap()),
            ("membership/leaves", orishu_worker::peer::codec::encode(&leave).unwrap()),
            ("membership/joins", orishu_worker::peer::codec::encode(&join).unwrap()),
        ];
        let join_token = material.token.expose();
        let credentials = [
            String::new(),
            format!("Authorization: Bearer {}\r\n", "0".repeat(64)),
            format!("Authorization: Bearer {target_token}\r\n"),
            format!("Authorization: Bearer {join_token}\r\n"),
            format!("Authorization: Bearer {token}\r\nAuthorization: Bearer {token}\r\n"),
            format!("Authorization: Bearer {token}\r\nAuthorization: Bearer {join_token}\r\n"),
            format!("Authorization: Bearer {join_token}\r\nAuthorization: Bearer {token}\r\n"),
            format!("Authorization: Basic {token}\r\n"),
        ];
        for (route, body) in &requests {
            for credential in &credentials {
                tokio::time::timeout(Duration::from_secs(2), async {
                    let head = format!("POST /api/v1/{route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/cbor\r\n{credential}Content-Length: {}\r\n\r\n", body.len());
                    let response = if http2 {
                        let tls_setup = tls_setup.as_ref().map(|(addr, _, _, config)| (*addr, config.clone()));
                        let path = source_socket.clone();
                        let route = route.to_string();
                        let credential = credential.clone();
                        let body = body.clone();
                        tokio::task::spawn_blocking(move || {
                            if let Some((address, config)) = tls_setup {
                                let socket = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
                                socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                                socket.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                                let connection = rustls::ClientConnection::new(config, rustls::pki_types::ServerName::IpAddress(address.ip().into())).unwrap();
                                let mut stream = rustls::StreamOwned::new(connection, socket);
                                let response = h2_authorization_response(&mut stream, &route, &credential, &body, true, false);
                                assert_eq!(stream.conn.alpn_protocol(), Some(b"h2".as_slice()));
                                response
                            } else {
                                let mut stream = std::os::unix::net::UnixStream::connect(path).unwrap();
                                stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                                stream.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                                h2_authorization_response(&mut stream, &route, &credential, &body, false, false)
                            }
                        }).await.unwrap()
                    } else if let Some((address, _, _, config)) = &tls_setup {
                        let address = *address;
                        let config = config.clone();
                        let mut request = head.into_bytes();
                        request.extend_from_slice(body);
                        tokio::task::spawn_blocking(move || {
                            use std::io::{Read, Write};
                            let socket = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
                            socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                            socket.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                            let connection = rustls::ClientConnection::new(config, rustls::pki_types::ServerName::IpAddress(address.ip().into())).unwrap();
                            let mut stream = rustls::StreamOwned::new(connection, socket);
                            stream.write_all(&request).unwrap();
                            let mut response = Vec::new();
                            if let Err(error) = (&mut stream).take(8193).read_to_end(&mut response) {
                                assert!(matches!(error.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof));
                            }
                            assert_eq!(stream.conn.alpn_protocol(), Some(b"http/1.1".as_slice()));
                            response
                        }).await.unwrap()
                    } else {
                    let mut stream = tokio::net::UnixStream::connect(&source_socket).await.unwrap();
                    stream.write_all(head.as_bytes()).await.unwrap();
                    if let Err(error) = stream.write_all(body).await { assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe); }
                    let mut response = Vec::new();
                    if let Err(error) = stream.take(8193).read_to_end(&mut response).await { assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset); }
                    response
                    };
                    assert!(response.len() <= 8192);
                    if http2 {
                        let envelope: orishu::model::ApiResponse = orishu_worker::peer::codec::decode(&response).unwrap();
                        assert!(matches!(envelope, orishu::model::ApiResponse::Error { code, .. } if code == "Unauthorized"));
                    } else { assert!(response.starts_with(b"HTTP/1.1 401 "), "expected authorization rejection"); }
                    for secret in [token.as_str(), target_token.as_str(), join_token] {
                        assert!(!response.windows(secret.len()).any(|bytes| bytes == secret.as_bytes()), "response disclosed credential");
                    }
                }).await.expect("mutation authorization deadline");
                let current = summary(&mut source, &source_socket).await;
                assert_eq!(current.formation_id, initial.formation_id);
                assert_eq!(current.source_node_id, initial.source_node_id);
                assert_eq!(current.participation, Participation::Standalone);
                assert_eq!(current.member_count, 1);
                assert!(!current.membership_locked);
            }
        }
        assert_eq!(summary(&mut target, &target_socket).await.member_count, target_initial.member_count);
        let status = authorized.membership().join_status(&join.operation_id).await;
        assert!(matches!(status,
            Err(orishu::client::ClientError::ApiError { status: 404, .. })));
        // Correct local authority still works with the rejected request IDs:
        // unauthorized requests must not consume operation history either.
        assert!(authorized.membership().set_lock(&lock).await.unwrap().locked);
        assert!(!authorized.membership().set_lock(&unlock).await.unwrap().locked);
        let receipt = authorized.membership().leave(&leave).await.unwrap();
        assert_eq!(receipt.operation_id, leave.operation_id);
        assert_eq!(receipt.previous_formation_id, initial.formation_id);
        assert!(!receipt.changed, "authorized leave of an already standalone worker is a no-op");
        if http2 {
            let h2_lock = LockRequest { operation_id: "h2-lock".parse().unwrap(), ..lock.clone() };
            let h2_unlock = LockRequest { operation_id: "h2-unlock".parse().unwrap(), ..unlock.clone() };
            let h2_leave = LeaveRequest { operation_id: "h2-leave".parse().unwrap(), ..leave.clone() };
            for (index, (route, body)) in [
                ("cluster/lock", orishu_worker::peer::codec::encode(&h2_lock).unwrap()),
                ("cluster/lock", orishu_worker::peer::codec::encode(&h2_unlock).unwrap()),
                ("membership/leaves", orishu_worker::peer::codec::encode(&h2_leave).unwrap()),
                ("membership/joins", orishu_worker::peer::codec::encode(&join).unwrap()),
            ].into_iter().enumerate() {
                let tls = tls_setup.as_ref().map(|(address, _, _, config)| (*address, config.clone()));
                let path = source_socket.clone();
                let credential = format!("Authorization: Bearer {token}\r\n");
                let bytes = tokio::task::spawn_blocking(move || {
                    if let Some((address, config)) = tls {
                        let socket = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
                        socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                        socket.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                        let connection = rustls::ClientConnection::new(config, rustls::pki_types::ServerName::IpAddress(address.ip().into())).unwrap();
                        let mut stream = rustls::StreamOwned::new(connection, socket);
                        let result = h2_authorization_response(&mut stream, route, &credential, &body, true, true);
                        assert_eq!(stream.conn.alpn_protocol(), Some(b"h2".as_slice()));
                        result
                    } else {
                        let mut stream = std::os::unix::net::UnixStream::connect(path).unwrap();
                        stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                        stream.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                        h2_authorization_response(&mut stream, route, &credential, &body, false, true)
                    }
                }).await.unwrap();
                use orishu::model::{ApiResponse, ResponseData};
                match orishu_worker::peer::codec::decode::<ApiResponse>(&bytes).unwrap() {
                    ApiResponse::Ok { data: Some(ResponseData::LockReceipt(receipt)) } if index < 2 => {
                        assert_eq!(receipt.locked, index == 0);
                        assert_eq!(summary(&mut source, &source_socket).await.membership_locked, index == 0);
                    }
                    ApiResponse::Ok { data: Some(ResponseData::LeaveReceipt(receipt)) } if index == 2 => assert!(!receipt.changed),
                    ApiResponse::Ok { data: Some(ResponseData::JoinOperation(operation)) } if index == 3 => {
                        use orishu::model::cluster::JoinOperationState as State;
                        assert_eq!(operation.operation_id, join.operation_id);
                        assert!(matches!(operation.state, State::Connecting | State::Admitting | State::CatchingUp { .. } | State::Joined { .. }));
                    }
                    _ => panic!("wrong authorized HTTP/2 outcome"),
                }
            }
        }
        source.terminate().await;
        target.terminate().await;
    }).await.expect("credential matrix fixture deadline");
}

#[tokio::test]
async fn real_lock_api_requires_operator_authority_and_replays_without_relocking() {
    use orishu::model::cluster::LockRequest;
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = root.path().join("state");
    let socket = root.path().join("api.sock");
    let mut worker = start(&state, &socket);
    let initial = summary(&mut worker, &socket).await;
    let token = std::fs::read_to_string(state.join("operator.token")).unwrap();
    let client = |token: Option<String>| {
        HttpClusterClient::new(
            ClusterAddress::UnixSocket(socket.clone()),
            HttClientOptions {
                credentials: token.map(orishu::client::Credentials::Token),
                timeout: Some(Duration::from_secs(2)),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let request = LockRequest {
        schema_version: 1,
        operation_id: "lock-001".parse().unwrap(),
        formation_id: initial.formation_id.clone(),
        locked: true,
    };
    assert!(client(None).membership().set_lock(&request).await.is_err());
    assert!(
        client(Some("0".repeat(64)))
            .membership()
            .set_lock(&request)
            .await
            .is_err()
    );
    assert!(!summary(&mut worker, &socket).await.membership_locked);
    let authorized = client(Some(token.clone()));
    let receipt = authorized.membership().set_lock(&request).await.unwrap();
    assert!(receipt.locked);
    assert_eq!(receipt.source_node_id, initial.source_node_id);
    assert!(summary(&mut worker, &socket).await.membership_locked);
    let unlock = LockRequest {
        operation_id: "unlock-002".parse().unwrap(),
        locked: false,
        ..request.clone()
    };
    assert!(
        !authorized
            .membership()
            .set_lock(&unlock)
            .await
            .unwrap()
            .locked
    );
    assert_eq!(
        authorized.membership().set_lock(&request).await.unwrap(),
        receipt
    );
    assert!(!summary(&mut worker, &socket).await.membership_locked);
    assert!(
        authorized
            .membership()
            .set_lock(&LockRequest {
                locked: false,
                ..request.clone()
            })
            .await
            .is_err()
    );
    let stale = LockRequest {
        formation_id: "wrong-formation".parse().unwrap(),
        ..request.clone()
    };
    assert!(authorized.membership().set_lock(&stale).await.is_err());
    let leave = orishu::model::cluster::LeaveRequest {
        schema_version: 1,
        operation_id: "api-leave-noop".parse().unwrap(),
        formation_id: initial.formation_id.clone(),
    };
    assert!(client(None).membership().leave(&leave).await.is_err());
    assert!(
        client(Some("incorrect".into()))
            .membership()
            .leave(&leave)
            .await
            .is_err()
    );
    let receipt = authorized.membership().leave(&leave).await.unwrap();
    assert!(!receipt.changed);
    assert_eq!(receipt.current.source_node_id, initial.source_node_id);
    assert_eq!(
        authorized.membership().leave(&leave).await.unwrap(),
        receipt
    );
    let bytes = orishu_worker::peer::codec::encode(&request).unwrap();
    for route in ["cluster/lock", "membership/leaves"] {
        for (body, headers, expected) in [
            (
                bytes.clone(),
                format!(
                    "Authorization: Bearer {token}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/cbor\r\n"
                ),
                401,
            ),
            (
                vec![b'a'; 4097],
                format!("Authorization: Bearer {token}\r\nContent-Type: application/cbor\r\n"),
                400,
            ),
            (
                vec![0xa0],
                format!("Authorization: Bearer {token}\r\nContent-Type: application/cbor\r\n"),
                400,
            ),
            (
                bytes.clone(),
                format!("Authorization: Bearer {token}\r\nContent-Type: text/plain\r\n"),
                415,
            ),
            (
                bytes.clone(),
                format!(
                    "Authorization: Bearer {token}\r\nContent-Type: application/cbor\r\nIf-Match: test\r\n"
                ),
                400,
            ),
        ] {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let response = tokio::time::timeout(Duration::from_secs(2), async {
            let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
            let head = format!("POST /api/v1/{route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{headers}Content-Length: {}\r\n\r\n", body.len());
            stream.write_all(head.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
            let mut response = Vec::new();
            stream.take(8192).read_to_end(&mut response).await.unwrap();
            response
        }).await.unwrap();
            assert!(
                String::from_utf8_lossy(&response).starts_with(&format!("HTTP/1.1 {expected} "))
            );
            assert!(!String::from_utf8_lossy(&response).contains(&token));
        }
    }
    assert!(!summary(&mut worker, &socket).await.membership_locked);
    worker.terminate().await;
}

impl Worker {
    async fn terminate(&mut self) {
        let pid = rustix::process::Pid::from_raw(i32::try_from(self.0.id()).unwrap()).unwrap();
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    assert!(status.success(), "graceful worker termination failed");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("worker shutdown deadline");
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(state: &Path, socket: &Path) -> Worker {
    Worker(
        Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
            .args([
                "--state-dir",
                state.to_str().unwrap(),
                "--listen.clients",
                socket.to_str().unwrap(),
                "--name",
                "same-worker",
                "--cluster-name",
                "same-cluster",
            ])
            .env("ORISHU_CLUSTER_NAME", "environment-must-not-override-cli")
            .env("ORISHU_STATE_DIR", "/unusable/environment/path")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}

async fn summary(worker: &mut Worker, socket: &Path) -> Summary {
    let client = HttpClusterClient::new(
        ClusterAddress::UnixSocket(socket.to_owned()),
        HttClientOptions::default(),
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            assert!(
                worker.0.try_wait().unwrap().is_none(),
                "worker exited before serving summary"
            );
            if let Ok(summary) = client.cluster().summary().await {
                return summary;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("worker summary deadline")
}

#[tokio::test]
async fn real_workers_start_distinct_and_restart_without_restoring_membership() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let state_a = root.path().join("state-a");
    let socket_a = root.path().join("a.sock");
    let mut a = start(&state_a, &socket_a);
    let mut b = start(&root.path().join("state-b"), &root.path().join("b.sock"));
    let initial = summary(&mut a, &socket_a).await;
    let other = summary(&mut b, &root.path().join("b.sock")).await;
    assert_eq!(initial.cluster_name, other.cluster_name);
    assert_ne!(initial.formation_id, other.formation_id);
    assert_ne!(initial.source_node_id, other.source_node_id);
    assert_eq!(initial.member_count, 1);
    assert_eq!(initial.alive_count, 1);
    assert_eq!(initial.participation, Participation::Standalone);
    assert!(!initial.introducer_ready);
    assert_eq!(
        std::fs::metadata(&socket_a).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // Another process must not take ownership of a running worker's credentials.
    let status = Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
        .args([
            "--state-dir",
            state_a.to_str().unwrap(),
            "--listen.clients",
            root.path().join("duplicate.sock").to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
    assert_eq!(
        summary(&mut a, &socket_a).await.formation_id,
        initial.formation_id
    );

    a.terminate().await;
    drop(a);
    assert!(!socket_a.exists(), "graceful shutdown cleans its socket");
    let mut restarted = start(&state_a, &socket_a);
    let fresh = summary(&mut restarted, &socket_a).await;
    assert_ne!(fresh.formation_id, initial.formation_id);
    assert_ne!(fresh.source_node_id, initial.source_node_id);
    assert_eq!(fresh.participation, Participation::Standalone);
    drop(restarted); // SIGKILL leaves a stale socket, unlike graceful shutdown.
    assert!(socket_a.exists());
    let mut recovered = start(&state_a, &socket_a);
    let recovered_summary = summary(&mut recovered, &socket_a).await;
    assert_ne!(recovered_summary.formation_id, fresh.formation_id);
}

#[test]
fn plaintext_tcp_client_listener_is_an_explicit_configuration_error() {
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
        .args([
            "--state-dir",
            root.path().to_str().unwrap(),
            "--listen.clients",
            "127.0.0.1:0",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("require a TLS certificate and key"));
}

#[tokio::test]
async fn remote_summary_requires_tls_and_worker_operator_token() {
    exercise_remote_summary(false).await;
}

#[tokio::test]
async fn unfinished_tls_handshakes_expire_without_blocking_authenticated_control() {
    tokio::time::timeout(Duration::from_secs(25), exercise_remote_summary(true))
        .await
        .expect("TLS handshake pressure fixture deadline");
}

async fn exercise_remote_summary(unfinished_handshakes: bool) {
    let root = tempfile::tempdir().unwrap();
    let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
    let cert_path = root.path().join("server.pem");
    let key_path = root.path().join("server.key");
    std::fs::write(&cert_path, certificate.cert.pem()).unwrap();
    std::fs::write(&key_path, certificate.signing_key.serialize_pem()).unwrap();
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let state = root.path().join("state");
    let mut worker = Worker(
        Command::new(env!("CARGO_BIN_EXE_orishu-worker"))
            .args([
                "--state-dir",
                state.to_str().unwrap(),
                "--listen.clients",
                &address.to_string(),
                "--tls-cert",
                cert_path.to_str().unwrap(),
                "--tls-key",
                key_path.to_str().unwrap(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let token = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            assert!(worker.0.try_wait().unwrap().is_none());
            if let Ok(token) = std::fs::read_to_string(state.join("operator.token")) {
                break token;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let client = |token: Option<String>| {
        HttpClusterClient::new(
            ClusterAddress::Ip(address),
            HttClientOptions {
                tls_cert: Some(cert_path.clone()),
                credentials: token.map(orishu::client::Credentials::Token),
                timeout: Some(Duration::from_secs(1)),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let authorized = client(Some(token.clone()));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            assert!(worker.0.try_wait().unwrap().is_none());
            if let Ok(summary) = authorized.cluster().summary().await {
                assert_eq!(summary.member_count, 1);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("authenticated HTTPS summary");
    if unfinished_handshakes {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut silent = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut partial = tokio::net::TcpStream::connect(address).await.unwrap();
        let started = tokio::time::Instant::now();
        // A plausible TLS handshake record with an incomplete payload. The
        // adapter must enforce its handshake budget before any HTTP exists.
        partial.write_all(&[22, 3, 3, 0x40, 0]).await.unwrap();
        let mut silent_byte = [0];
        assert!(
            tokio::time::timeout(Duration::from_secs(1), silent.read(&mut silent_byte))
                .await
                .is_err(),
            "silent connection must be held, not rejected immediately"
        );
        let mut partial_byte = [0];
        // Keep supplying ciphertext so a transport inactivity deadline alone
        // cannot establish the absolute handshake bound.
        loop {
            tokio::select! {
                result = partial.read(&mut partial_byte) => {
                    match result {
                        Ok(0) => {},
                        Err(error) if matches!(error.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof) => {},
                        other => panic!("unexpected partial TLS result: {other:?}"),
                    }
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // A write racing closure may fail; only the read branch
                    // establishes the observed server-side termination.
                    let _ = partial.write_all(&[0]).await;
                    assert_eq!(authorized.cluster().summary().await.unwrap().member_count, 1);
                }
            }
            assert!(
                started.elapsed() < Duration::from_secs(7),
                "partial TLS exceeded handshake budget"
            );
        }
        assert!(
            started.elapsed() >= Duration::from_secs(4),
            "premature TLS parser rejection"
        );
        // The pinned server's five-second protocol-detection read wraps its
        // lazy TLS handshake and expires before the ten-second TLS fallback.
        assert!(started.elapsed() < Duration::from_secs(7));
        let result = tokio::time::timeout(Duration::from_secs(1), silent.read(&mut silent_byte))
            .await
            .expect("silent TLS deadline");
        match result {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
                ) => {}
            other => panic!("unexpected silent TLS result: {other:?}"),
        }
        assert_eq!(
            client(Some(token))
                .cluster()
                .summary()
                .await
                .unwrap()
                .member_count,
            1
        );
        worker.terminate().await;
        return;
    }
    assert_eq!(
        authorized
            .membership()
            .list(&Default::default())
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        client(None)
            .membership()
            .list(&Default::default())
            .await
            .is_err()
    );
    let identity = authorized.cluster().summary().await.unwrap().source_node_id;
    assert!(authorized.membership().get(&identity, None).await.is_ok());
    assert!(
        client(None)
            .membership()
            .get(&identity, None)
            .await
            .is_err()
    );
    assert!(
        client(Some("0".repeat(64)))
            .membership()
            .get(&identity, None)
            .await
            .is_err()
    );
    assert!(client(None).cluster().summary().await.is_err());
    assert!(
        client(Some("0".repeat(64)))
            .cluster()
            .summary()
            .await
            .is_err()
    );
}
