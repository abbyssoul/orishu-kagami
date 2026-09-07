use super::*;
use orishu_worker::{
    credentials::WorkerCredentials,
    peer::{
        exchange::{ExchangePool, Phase},
        ingress::IngressEvent,
        tls::{PeerIdentity, SERVER_NAME},
    },
};

#[tokio::test]
async fn real_worker_peer_handshake_failures_reach_optional_scrapes() {
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(Duration::from_secs(20), async {
        for metrics in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = root.path().join("state");
            let credentials = WorkerCredentials::load_or_create(&state).unwrap();
            let certificate = credentials.identity.certificate().to_vec();
            drop(credentials);
            let socket = root.path().join("api.sock");
            let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let diagnostic = reservation.local_addr().unwrap();
            let peer_reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let peer = peer_reservation.local_addr().unwrap();
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
                    "--listen.peers",
                    &peer.to_string(),
                    "--observability.enabled",
                    "true",
                    "--observability.bind",
                    &diagnostic.to_string(),
                    "--observability.metrics",
                    if metrics { "true" } else { "false" },
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            drop((reservation, peer_reservation));
            let mut worker = Worker(command.spawn().unwrap());
            let before = summary(&mut worker, &socket).await;
            let applicant = PeerIdentity::generate().unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let connection = client
                .connect_with(
                    applicant.client_config(&certificate).unwrap(),
                    peer,
                    SERVER_NAME,
                )
                .unwrap()
                .await
                .unwrap();
            assert!(
                ExchangePool::new(1)
                    .unwrap()
                    .request(&connection, &[0xf5], Phase::Handshake)
                    .await
                    .is_err()
            );
            connection.closed().await;
            if metrics {
                let response = loop {
                    let response = diagnostics_request(diagnostic, "/metrics").await;
                    if response.contains("orishu_worker_peer_inbound_connection_slots_in_use 0\n") {
                        break response;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                };
                assert!(response.starts_with("HTTP/1.1 200"));
                let body = response.split_once("\r\n\r\n").unwrap().1;
                assert_eq!(
                    body.lines().filter(|line| !line.starts_with('#')).count(),
                    151
                );
                for (name, value) in [
                    ("slots_in_use", 0),
                    ("slots_capacity", 64),
                    ("provisional_slots_in_use", 0),
                    ("provisional_slots_capacity", 16),
                ] {
                    assert!(body.contains(&format!(
                        "# TYPE orishu_worker_peer_registry_{name} gauge\n"
                    )));
                    assert!(
                        body.contains(&format!("orishu_worker_peer_registry_{name} {value}\n"))
                    );
                }
                for (name, value) in [
                    ("tls_slots_in_use", 0),
                    ("tls_slots_capacity", 16),
                    ("connection_slots_in_use", 0),
                    ("connection_slots_capacity", 64),
                ] {
                    assert!(
                        body.contains(&format!("# TYPE orishu_worker_peer_inbound_{name} gauge\n"))
                    );
                    assert!(body.contains(&format!("orishu_worker_peer_inbound_{name} {value}\n")));
                }
                for event in IngressEvent::ALL {
                    let (name, _) = event.descriptor();
                    let expected = u8::from(matches!(
                        event,
                        IngressEvent::TlsCompleted | IngressEvent::HandshakeFailed
                    ));
                    assert!(
                        body.contains(&format!(
                            "orishu_worker_peer_inbound_{name}_total {expected}\n"
                        )),
                        "{name}"
                    );
                }
                for event in orishu_worker::peer::traffic::Event::ALL {
                    let (name, _) = event.descriptor();
                    assert!(
                        body.contains(&format!("orishu_worker_peer_{name} 0\n")),
                        "{name}"
                    );
                }
                assert!(body.contains("orishu_worker_peer_outbound_slots_capacity 4\n"));
                for event in orishu_worker::formation_metrics::Event::ALL {
                    let (suffix, _) = event.descriptor();
                    assert!(body.contains(&format!("orishu_worker_membership_{suffix} 0\n")));
                }
                for event in orishu_worker::formation_metrics::CatchupEvent::ALL {
                    let (suffix, _) = event.descriptor();
                    assert!(body.contains(&format!("orishu_worker_catchup_{suffix} 0\n")));
                }
                for line in body.lines().filter(|line| {
                    line.starts_with("orishu_worker_peer_outbound_")
                        && !line.starts_with("orishu_worker_peer_outbound_slots_capacity ")
                }) {
                    assert_eq!(line.split_once(' ').unwrap().1.parse::<f64>().unwrap(), 0.0);
                }
                for name in [
                    "admissions_accepted",
                    "admissions_rejected",
                    "peer_decode_rejections",
                ] {
                    assert!(body.contains(&format!("orishu_worker_{name}_total 0\n")));
                }
                for (name, value) in [
                    ("serve_failed_total", 1),
                    ("serve_duration_seconds_count", 1),
                    ("request_duration_seconds_count", 0),
                    ("bytes_received_total", 5),
                    ("bytes_sent_total", 0),
                    ("slots_in_use", 0),
                    ("slots_capacity", 64),
                ] {
                    assert!(
                        body.contains(&format!("orishu_worker_peer_reliable_{name} {value}\n")),
                        "{name}"
                    );
                }
                let operator = std::fs::read_to_string(state.join("operator.token")).unwrap();
                assert!(!body.contains(operator.trim()));
                // Conservative maximum text width, including optional traces:
                // integers use at most 20 digits; only duration sums/totals
                // need another 7 bytes. The parser control below uses the same
                // catalogue types and actual saturating duration ceiling.
                let maximum: usize = body
                    .lines()
                    .map(|line| {
                        if line.starts_with('#') {
                            line.len() + 1
                        } else {
                            let name = line.split_once(' ').unwrap().0;
                            let fractional =
                                name.ends_with("_sum") || name.ends_with("_seconds_total");
                            name.len() + 1 + 20 + usize::from(fractional) * 7 + 1
                        }
                    })
                    .sum();
                assert!(
                    maximum + 4096 < 32768,
                    "maximum base {maximum} plus trace catalogue"
                );
                if let Some(path) = std::env::var_os("ORISHU_TEST_PROMTOOL") {
                    use tokio::io::AsyncWriteExt;
                    // A conservative maximum-width parser control complements
                    // the real values above. Keep all histogram buckets/counts
                    // equal; durations use the saturating microsecond ceiling.
                    let maximum_body: String = body
                        .lines()
                        .map(|line| {
                            if line.starts_with('#') {
                                format!("{line}\n")
                            } else {
                                let (name, _) = line.split_once(' ').unwrap();
                                let value = if name.ends_with("_sum")
                                    || name.ends_with("_seconds_total")
                                {
                                    format!("{}.{:06}", u64::MAX / 1_000_000, u64::MAX % 1_000_000)
                                } else {
                                    u64::MAX.to_string()
                                };
                                format!("{name} {value}\n")
                            }
                        })
                        .collect();
                    assert!(maximum_body.len() + 4096 < 32768);
                    for payload in [body, maximum_body.as_str()] {
                        let mut parser = tokio::process::Command::new(&path)
                            .args(["check", "metrics"])
                            .stdin(Stdio::piped())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .kill_on_drop(true)
                            .spawn()
                            .unwrap();
                        parser
                            .stdin
                            .take()
                            .unwrap()
                            .write_all(payload.as_bytes())
                            .await
                            .unwrap();
                        let result =
                            tokio::time::timeout(Duration::from_secs(2), parser.wait_with_output())
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
                // Retain a real TLS-complete connection without its application
                // handshake; HTTP must expose one task and no TLS permit.
                let held = client
                    .connect_with(
                        applicant.client_config(&certificate).unwrap(),
                        peer,
                        SERVER_NAME,
                    )
                    .unwrap()
                    .await
                    .unwrap();
                for expected in [1, 0] {
                    if expected == 0 {
                        held.close(0_u32.into(), b"release measured slot");
                    }
                    loop {
                        let response = diagnostics_request(diagnostic, "/metrics").await;
                        if response.contains(&format!(
                            "orishu_worker_peer_inbound_connection_slots_in_use {expected}\n"
                        )) && response
                            .contains("orishu_worker_peer_inbound_tls_slots_in_use 0\n")
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    assert!(
                        diagnostics_request(diagnostic, "/readyz")
                            .await
                            .starts_with("HTTP/1.1 200")
                    );
                    let current = summary(&mut worker, &socket).await;
                    assert_eq!(current.formation_id, before.formation_id);
                    assert_eq!(current.source_node_id, before.source_node_id);
                }
            } else {
                assert!(
                    diagnostics_request(diagnostic, "/metrics")
                        .await
                        .starts_with("HTTP/1.1 404")
                );
            }
            // A real application handshake now registers the same applicant.
            // It is still not an admitted member and cannot change membership.
            let registered = client
                .connect_with(
                    applicant.client_config(&certificate).unwrap(),
                    peer,
                    SERVER_NAME,
                )
                .unwrap()
                .await
                .unwrap();
            let mut local = orishu_membership::testing::standalone("scrape-applicant")
                .local()
                .clone();
            local.cert_fingerprint = applicant.fingerprint();
            let request =
                orishu_worker::peer::handshake::request(&local, before.formation_id.clone(), true)
                    .unwrap();
            let reply = ExchangePool::new(1)
                .unwrap()
                .request(&registered, &request, Phase::Handshake)
                .await
                .unwrap();
            assert!(!reply.is_empty());
            let operator = std::fs::read_to_string(state.join("operator.token")).unwrap();
            let authorized = HttpClusterClient::new(
                ClusterAddress::UnixSocket(socket.clone()),
                HttClientOptions {
                    credentials: Some(orishu::client::Credentials::Token(
                        operator.trim().to_owned(),
                    )),
                    timeout: Some(Duration::from_secs(1)),
                    ..Default::default()
                },
            )
            .unwrap();
            for expected in [1, 0] {
                if expected == 0 {
                    registered.close(0_u32.into(), b"release registered slot");
                }
                if metrics {
                    loop {
                        let response = diagnostics_request(diagnostic, "/metrics").await;
                        if response.contains(&format!(
                            "orishu_worker_peer_registry_slots_in_use {expected}\n"
                        )) && response.contains(&format!(
                            "orishu_worker_peer_registry_provisional_slots_in_use {expected}\n"
                        )) {
                            assert!(
                                response
                                    .contains("orishu_worker_peer_registry_slots_capacity 64\n")
                            );
                            assert!(response.contains(
                                "orishu_worker_peer_registry_provisional_slots_capacity 16\n"
                            ));
                            assert!(
                                response.contains("orishu_worker_admissions_accepted_total 0\n")
                            );
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                }
                let request = orishu::model::cluster::LockRequest {
                    schema_version: 1,
                    operation_id: format!("registry-control-{expected}").parse().unwrap(),
                    formation_id: before.formation_id.clone(),
                    locked: expected == 1,
                };
                assert_eq!(
                    authorized
                        .membership()
                        .set_lock(&request)
                        .await
                        .unwrap()
                        .locked,
                    request.locked
                );
            }
            assert!(
                diagnostics_request(diagnostic, "/readyz")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
            let after = summary(&mut worker, &socket).await;
            assert_eq!(after.formation_id, before.formation_id);
            assert_eq!(after.source_node_id, before.source_node_id);
            assert_eq!(after.member_count, 1);
            client.close(0_u32.into(), b"fixture complete");
            worker.terminate().await;
        }
    })
    .await
    .expect("optional real-worker peer metrics and unchanged domain state");
}
