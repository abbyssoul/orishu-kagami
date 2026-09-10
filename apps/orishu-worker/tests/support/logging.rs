//! Executable logging: configuration, real OTLP correlation and blocked output.
use super::*;
use std::{io::Read, os::unix::fs::PermissionsExt};

fn command(root: &Path) -> Command {
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ORISHU_") {
            command.env_remove(key);
        }
    }
    command
        .arg("--state-dir")
        .arg(root.join("state"))
        .arg("--listen.clients")
        .arg(root.join("worker.sock"))
        .args(["--name", "authored-secret-marker"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn output(worker: &mut Worker) -> Vec<serde_json::Value> {
    let mut bytes = Vec::new();
    worker
        .0
        .stdout
        .take()
        .unwrap()
        .take(8193)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 8192);
    assert!(!String::from_utf8_lossy(&bytes).contains("secret-marker"));
    let mut stderr = Vec::new();
    worker
        .0
        .stderr
        .take()
        .unwrap()
        .take(8193)
        .read_to_end(&mut stderr)
        .unwrap();
    assert!(
        stderr.is_empty(),
        "normal runtime emitted unbounded fallback diagnostics"
    );
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .map(|line| {
            assert!(line.len() <= orishu_worker::operational_log::RECORD_BYTES);
            assert_eq!(line.last(), Some(&b'\n'));
            let json: serde_json::Value = serde_json::from_slice(line).unwrap();
            assert_eq!(json["version"], 1);
            assert!(json["unix_nanos"].as_u64().unwrap() > 0);
            json
        })
        .collect()
}

#[tokio::test]
async fn logging_lifecycle_and_file_environment_cli_precedence() {
    tokio::time::timeout(Duration::from_secs(20), async {
        // File enables; environment disables; explicit CLI true wins. The
        // absent case verifies the quiet default without opening a log sink.
        for mode in ["default", "file", "environment", "cli"] {
            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("worker.yaml");
            std::fs::write(
                &config,
                "spec:\n  logging:\n    enabled: true\n    queueRecords: 64\n    shutdownMs: 200\n",
            )
            .unwrap();
            let mut command = command(root.path());
            if mode != "default" {
                command.arg("--config").arg(&config);
            }
            if matches!(mode, "environment" | "cli") {
                command.env("ORISHU_LOGGING_ENABLED", "false");
            }
            if mode == "cli" {
                command.args(["--logging.enabled", "true"]);
            }
            let mut worker = Worker(command.spawn().unwrap());
            assert_eq!(
                summary(&mut worker, &root.path().join("worker.sock"))
                    .await
                    .alive_count,
                1
            );
            worker.terminate().await;
            let records = output(&mut worker);
            if matches!(mode, "default" | "environment") {
                assert!(records.is_empty());
            } else {
                let events: Vec<_> = records
                    .iter()
                    .map(|record| record["event"].as_str().unwrap())
                    .collect();
                assert_eq!(
                    events,
                    [
                        "orishu.worker.ready",
                        "orishu.worker.stopping",
                        "orishu.worker.stopped"
                    ]
                );
                assert!(
                    records
                        .iter()
                        .all(|record| record.get("trace_id").is_none())
                );
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn logging_closed_stdout_does_not_change_worker_health_or_exit() {
    tokio::time::timeout(Duration::from_secs(12), async {
        let root = tempfile::tempdir().unwrap();
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let mut command = command(root.path());
        command
            .args(["--logging.enabled", "true"])
            .stdout(Stdio::from(writer));
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
        let mut worker = Worker(command.spawn().unwrap());
        let socket = root.path().join("worker.sock");
        let before = summary(&mut worker, &socket).await;
        #[cfg(feature = "observability")]
        {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            loop {
                let metrics = diagnostics_request_bounded(address, "/metrics", 32768).await;
                assert_eq!(
                    metrics
                        .lines()
                        .filter(|line| line.starts_with("orishu_worker_log_"))
                        .count(),
                    9
                );
                if metrics.contains("orishu_worker_log_output_failed_total 1\n") {
                    break;
                }
                assert!(tokio::time::Instant::now() < deadline);
                tokio::task::yield_now().await;
            }
            assert!(
                diagnostics_request_bounded(address, "/readyz", 1024)
                    .await
                    .starts_with("HTTP/1.1 200")
            );
        }
        let after = summary(&mut worker, &socket).await;
        assert_eq!(before.formation_id, after.formation_id);
        assert_eq!(before.source_node_id, after.source_node_id);
        assert_eq!(after.alive_count, 1);
        worker.terminate().await;
        let mut stderr = Vec::new();
        worker
            .0
            .stderr
            .take()
            .unwrap()
            .take(4097)
            .read_to_end(&mut stderr)
            .unwrap();
        assert!(stderr.is_empty());
    })
    .await
    .unwrap();
}

#[cfg(feature = "otlp-tracing")]
#[tokio::test]
async fn logging_matches_received_span_and_respects_zero_or_disabled_tracing() {
    tokio::time::timeout(Duration::from_secs(20), async {
        for (logging, enabled, sampling) in [
            (true, true, "1000000"),
            (false, true, "1000000"),
            (true, true, "0"),
            (true, false, "1000000"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
            let mut worker = Worker(
                command(root.path())
                    .args([
                        "--logging.enabled",
                        if logging { "true" } else { "false" },
                        "--tracing.enabled",
                        if enabled { "true" } else { "false" },
                        "--tracing.sample-ppm",
                        sampling,
                        "--tracing.endpoint",
                        &endpoint,
                        "--tracing.batch-size",
                        "1",
                        "--tracing.shutdown-timeout-ms",
                        "100",
                    ])
                    .spawn()
                    .unwrap(),
            );
            summary(&mut worker, &root.path().join("worker.sock")).await;
            let span = if enabled && sampling != "0" {
                let token =
                    std::fs::read_to_string(root.path().join("state/operator.token")).unwrap();
                Some(super::tracing_context::receive(&listener, token.trim()).await)
            } else {
                None
            };
            worker.terminate().await;
            let records = output(&mut worker);
            let operations: Vec<_> = records
                .iter()
                .filter(|record| record["event"] == "orishu.client.request")
                .collect();
            if !logging {
                assert!(span.is_some(), "disabling logging must not disable export");
                assert!(records.is_empty());
                continue;
            }
            if let Some(span) = span {
                assert_eq!(operations.len(), 1);
                let context = orishu_worker::trace_context::TraceParent::new(
                    span.trace_id.try_into().unwrap(),
                    span.span_id.try_into().unwrap(),
                    true,
                )
                .unwrap()
                .encode();
                assert_eq!(
                    operations[0]["trace_id"],
                    std::str::from_utf8(&context[3..35]).unwrap()
                );
                assert_eq!(
                    operations[0]["span_id"],
                    std::str::from_utf8(&context[36..52]).unwrap()
                );
                assert_eq!(operations[0]["unix_nanos"], span.end_time_unix_nano);
                assert_eq!(operations[0]["outcome"], "completed");
            } else {
                assert!(operations.is_empty());
                assert!(
                    records
                        .iter()
                        .all(|record| record.get("trace_id").is_none())
                );
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            }
            assert!(
                records
                    .iter()
                    .any(|record| record["event"] == "orishu.worker.ready")
            );
            assert_eq!(records.last().unwrap()["event"], "orishu.worker.stopped");
            assert_eq!(
                records
                    .iter()
                    .filter(|record| record["event"] == "orishu.trace.accounting")
                    .count(),
                if enabled { 12 } else { 0 }
            );
        }
    })
    .await
    .unwrap();
}

#[cfg(feature = "otlp-tracing")]
#[tokio::test]
async fn logging_unread_stdout_is_held_through_real_worker_shutdown() {
    stdout_pressure(false).await;
}

#[cfg(all(feature = "otlp-tracing", feature = "observability"))]
#[tokio::test]
async fn logging_slow_stdout_reader_recovers_without_worker_restart() {
    stdout_pressure(true).await;
}

#[cfg(feature = "otlp-tracing")]
async fn stdout_pressure(recover: bool) {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = tempfile::tempdir().unwrap();
        // Valid destination, deliberately no accepting collector. Output pressure
        // must not change authority even while the independent exporter fails.
        let collector = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/traces", collector.local_addr().unwrap());
        let mut command = command(root.path());
        command.args([
            "--logging.enabled",
            "true",
            "--logging.queue-records",
            "2",
            "--logging.shutdown-ms",
            "20",
            "--tracing.enabled",
            "true",
            "--tracing.sample-ppm",
            "1000000",
            "--tracing.endpoint",
            &endpoint,
            "--tracing.export-timeout-ms",
            "100",
            "--tracing.shutdown-timeout-ms",
            "100",
        ]);
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
        let mut worker = Worker(command.spawn().unwrap());
        let unread_stdout = worker.0.stdout.take().unwrap();
        let socket = root.path().join("worker.sock");
        let before = summary(&mut worker, &socket).await;
        let client = HttpClusterClient::new(
            ClusterAddress::UnixSocket(socket.clone()),
            HttClientOptions {
                timeout: Some(Duration::from_secs(1)),
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..1024 {
            // Exercise sustained operation production, not 1024 repetitions of
            // the startup helper's reconnect/retry path. Each request must pass.
            let after = client.cluster().summary().await.unwrap();
            assert_eq!(after.formation_id, before.formation_id);
            assert_eq!(after.alive_count, 1);
        }
        #[cfg(feature = "observability")]
        {
            let metrics = diagnostics_request_bounded(address, "/metrics", 32768).await;
            let full: u64 = metrics
                .lines()
                .find_map(|line| line.strip_prefix("orishu_worker_log_queue_full_total "))
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                full > 0,
                "must observe output pressure, not merely assume it"
            );
            let written = metrics
                .lines()
                .find_map(|line| line.strip_prefix("orishu_worker_log_written_total "))
                .unwrap()
                .to_owned();
            for _ in 0..8 {
                assert_eq!(client.cluster().summary().await.unwrap().alive_count, 1);
            }
            let later = diagnostics_request_bounded(address, "/metrics", 32768).await;
            assert_eq!(
                later
                    .lines()
                    .find_map(|line| line.strip_prefix("orishu_worker_log_written_total "))
                    .unwrap(),
                written
            );
            let later_full: u64 = later
                .lines()
                .find_map(|line| line.strip_prefix("orishu_worker_log_queue_full_total "))
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                later_full > full,
                "continued requests must shed while output remains stalled"
            );
            assert!(
                diagnostics_request_bounded(address, "/readyz", 1024)
                    .await
                    .starts_with("HTTP/1.1 200")
            );
        }
        if recover {
            #[cfg(feature = "observability")]
            {
                recover_stdout(&mut worker, unread_stdout, &socket, address, &before).await;
                return;
            }
            #[cfg(not(feature = "observability"))]
            panic!("recovery evidence requires live loss counters");
        }
        worker.terminate().await;
        // Keep both standard streams open and unread throughout real process exit.
        let mut stderr = Vec::new();
        worker
            .0
            .stderr
            .take()
            .unwrap()
            .take(4097)
            .read_to_end(&mut stderr)
            .unwrap();
        assert!(stderr.is_empty());
        drop(unread_stdout);
    })
    .await
    .unwrap();
}

#[cfg(all(feature = "otlp-tracing", feature = "observability"))]
async fn recover_stdout(
    worker: &mut Worker,
    stdout: std::process::ChildStdout,
    socket: &Path,
    address: std::net::SocketAddr,
    before: &Summary,
) {
    fn count(metrics: &str, name: &str) -> u64 {
        metrics
            .lines()
            .find_map(|line| line.strip_prefix(&format!("orishu_worker_log_{name}_total ")))
            .unwrap()
            .parse()
            .unwrap()
    }
    let pressured = diagnostics_request_bounded(address, "/metrics", 32768).await;
    let accepted = count(&pressured, "accepted");
    let written = count(&pressured, "written");
    let lost = count(&pressured, "queue_full");
    assert!(accepted > written && lost > 0);

    // The parent alone resumes the actual pipe reader. A separate bounded
    // reader thread cannot block the async fixture while waiting for process
    // EOF. Worker RAII termination also releases it on fixture failure.
    let (finished, read) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(524_289).read_to_end(&mut bytes);
        let _ = finished.send((result, bytes));
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let metrics = diagnostics_request_bounded(address, "/metrics", 32768).await;
            assert_eq!(count(&metrics, "output_failed"), 0);
            assert_eq!(count(&metrics, "closed"), 0);
            assert_eq!(count(&metrics, "queue_full"), lost);
            if count(&metrics, "written") == accepted {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queued records must drain after the reader resumes");

    // An explicitly authenticated, unique parent proves this is a new record
    // emitted after recovery, not merely the old bytes draining from the pipe.
    let token =
        std::fs::read_to_string(socket.parent().unwrap().join("state/operator.token")).unwrap();
    let token = token.trim();
    let trace = "1234567890abcdef1234567890abcdef";
    let headers =
        format!("Authorization: Bearer {token}\r\ntraceparent: 00-{trace}-1234567890abcdef-01\r\n");
    let response = super::tracing_context::request(socket, false, &headers).await;
    assert!(response.starts_with(b"HTTP/1.1 200"));
    let after = summary(worker, socket).await;
    assert_eq!(after.formation_id, before.formation_id);
    assert_eq!(after.source_node_id, before.source_node_id);
    assert_eq!(after.alive_count, before.alive_count);
    assert_eq!(after.membership_locked, before.membership_locked);
    assert!(
        diagnostics_request_bounded(address, "/readyz", 1024)
            .await
            .starts_with("HTTP/1.1 200")
    );

    worker.terminate().await;
    let (result, bytes) = tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .unwrap()
        .unwrap();
    result.unwrap();
    assert!(
        bytes.len() <= 524_288,
        "bounded whole-fixture output capture"
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(token));
    assert!(!String::from_utf8_lossy(&bytes).contains("secret-marker"));
    let mut recovered = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        assert!(line.len() <= orishu_worker::operational_log::RECORD_BYTES);
        assert_eq!(line.last(), Some(&b'\n'));
        let record: serde_json::Value = serde_json::from_slice(line).unwrap();
        assert_eq!(record["version"], 1);
        if record["trace_id"] == trace {
            assert_eq!(record["event"], "orishu.client.request");
            assert_eq!(record["outcome"], "completed");
            assert_eq!(record["span_id"].as_str().unwrap().len(), 16);
            recovered += 1;
        }
    }
    assert_eq!(
        recovered, 1,
        "new sampled output must be usable after recovery"
    );
    let mut stderr = Vec::new();
    worker
        .0
        .stderr
        .take()
        .unwrap()
        .take(4097)
        .read_to_end(&mut stderr)
        .unwrap();
    assert!(stderr.is_empty());
}
