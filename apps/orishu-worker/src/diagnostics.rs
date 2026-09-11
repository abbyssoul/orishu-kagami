//! Optional loopback diagnostics. Fixed routes and finite, unlabelled series.
use orishu_worker::{health::Readiness, runtime::RunningWorker};
use salvo::http::Method;
use salvo::prelude::*;
use std::fmt::Write;
use std::sync::Arc;

mod logs;
mod requests;
pub(super) use requests::{Accounting, Requests};
#[cfg(feature = "otlp-tracing")]
mod traces;
#[cfg(feature = "otlp-tracing")]
pub(super) use traces::Traces;

#[derive(Clone, Copy)]
enum Route {
    Metrics,
    Live,
    Ready,
    Startup,
}

impl Route {
    fn raw_path(self) -> &'static str {
        match self {
            Self::Metrics => "/metrics",
            Self::Live => "/livez",
            Self::Ready => "/readyz",
            Self::Startup => "/startupz",
        }
    }
}

fn reject(res: &mut Response, status: StatusCode, reason: &'static str) {
    res.headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    res.status_code(status);
    res.render(Text::Plain(reason));
}

#[handler]
async fn not_found(res: &mut Response) {
    reject(res, StatusCode::NOT_FOUND, "not found\n");
}

struct Diagnostic {
    log: Option<orishu_worker::operational_log::Log>,
    worker: Arc<RunningWorker>,
    requests: Arc<Requests>,
    route: Route,
    #[cfg(feature = "otlp-tracing")]
    traces: Option<Traces>,
}

#[handler]
impl Diagnostic {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        res.headers_mut()
            .insert("cache-control", "no-store".parse().unwrap());
        // Routing may normalize/decode paths; it must not create diagnostics
        // aliases. Reject before even reading local health or metric snapshots.
        if req.uri().path() != self.route.raw_path() {
            reject(res, StatusCode::NOT_FOUND, "not found\n");
            return;
        }
        if req.uri().query().is_some() {
            reject(
                res,
                StatusCode::BAD_REQUEST,
                "query parameters unsupported\n",
            );
            return;
        }
        if req.method() != Method::GET {
            res.headers_mut().insert("allow", "GET".parse().unwrap());
            reject(res, StatusCode::METHOD_NOT_ALLOWED, "method unsupported\n");
            return;
        }
        let health = self.worker.health();
        let live = health.owner.is_responsive();
        let ready = health.readiness == Readiness::Ready;
        if matches!(self.route, Route::Metrics) {
            // Static names, no identity/path/peer labels, no registry scans.
            // These are measured local health states, not placeholders for
            // unimplemented simulation/transport instruments.
            let mut body = format!(
                "# HELP orishu_worker_owner_responsive Supervised owner responsiveness.\n# TYPE orishu_worker_owner_responsive gauge\norishu_worker_owner_responsive {}\n# HELP orishu_worker_ready Local process readiness.\n# TYPE orishu_worker_ready gauge\norishu_worker_ready {}\n# HELP orishu_worker_startup_complete Latched local initialization.\n# TYPE orishu_worker_startup_complete gauge\norishu_worker_startup_complete {}\n",
                u8::from(live),
                u8::from(ready),
                u8::from(health.startup_complete),
            );
            let counters = self.worker.owner_counters();
            for (name, help, value) in [
                (
                    "swim_packets_received",
                    "Decoded SWIM packets, not successful probes.",
                    counters.traffic.swim,
                ),
                (
                    "anti_entropy_packets_received",
                    "Decoded pull packets, not completed rounds.",
                    counters.traffic.anti_entropy,
                ),
                (
                    "gossip_items_received",
                    "Decoded envelope and pull-reply items, including repeats.",
                    counters.traffic.gossip_items,
                ),
                (
                    "peer_decode_rejections",
                    "Membership packets refused by owner session binding or decoding.",
                    counters.peer_decode_rejections,
                ),
                (
                    "admissions_accepted",
                    "New members inserted by local admission, regardless of reply delivery.",
                    counters.admissions.accepted,
                ),
                (
                    "admissions_rejected",
                    "Local core admission refusals, excluding pre-core failures.",
                    counters.admissions.rejected,
                ),
                (
                    "admission_assignment_replays",
                    "Validated retained assignments encoded and rebound, not delivered replies.",
                    counters.admissions.assignment_replays,
                ),
                (
                    "membership_transitions",
                    "Completed core transitions including periodic work.",
                    counters.transitions,
                ),
                (
                    "stale_inputs",
                    "Inputs rejected by lifecycle generation fencing.",
                    counters.stale_inputs,
                ),
                (
                    "core_diagnostics",
                    "Structured core diagnostic outcomes.",
                    counters.diagnostics,
                ),
                (
                    "foreign_gossip",
                    "Foreign gossip items handed off without a workload owner.",
                    counters.foreign_gossip,
                ),
                (
                    "reliable_replies",
                    "Completed reliable replies, not accepted domain operations.",
                    counters.completed_exchanges,
                ),
                (
                    "send_failures",
                    "Missing routes, send overload and transport failures.",
                    counters.failed_sends,
                ),
            ] {
                writeln!(body, "# HELP orishu_worker_{name}_total {help}\n# TYPE orishu_worker_{name}_total counter\norishu_worker_{name}_total {value}")
                    .expect("formatting into a string");
            }
            let pressure = self.worker.owner_pressure();
            if let Some(counters) = self.worker.formation_counters() {
                for event in orishu_worker::formation_metrics::CatchupEvent::ALL {
                    let (suffix, help) = event.descriptor();
                    let name = format!("orishu_worker_catchup_{suffix}");
                    writeln!(
                        body,
                        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {}",
                        counters.get_catchup(event)
                    )
                    .unwrap();
                }
                let registry = counters.registry_pressure();
                for (name, value) in [
                    ("slots_in_use", registry.slots_in_use),
                    ("slots_capacity", registry.slots_capacity),
                    (
                        "provisional_slots_in_use",
                        registry.provisional_slots_in_use,
                    ),
                    (
                        "provisional_slots_capacity",
                        registry.provisional_slots_capacity,
                    ),
                ] {
                    writeln!(body, "# HELP orishu_worker_peer_registry_{name} Retained registry budget; not live sockets or membership.\n# TYPE orishu_worker_peer_registry_{name} gauge\norishu_worker_peer_registry_{name} {value}").unwrap();
                }
                for event in orishu_worker::formation_metrics::Event::ALL {
                    let (suffix, help) = event.descriptor();
                    let name = format!("orishu_worker_membership_{suffix}");
                    writeln!(
                        body,
                        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {}",
                        counters.get(event)
                    )
                    .unwrap();
                }
            }
            if let Some(counters) = self.worker.peer_ingress_counters() {
                write_peer_ingress(&mut body, &counters);
            }
            if let Some(counters) = self.worker.peer_exchange_counters() {
                write_peer_exchanges(&mut body, &counters, self.worker.peer_exchange_pressure());
            }
            if let Some(counters) = self.worker.peer_dial_counters() {
                write_peer_dials(&mut body, &counters, self.worker.peer_dial_pressure());
            }
            if let Some(counters) = self.worker.peer_traffic_counters() {
                for event in orishu_worker::peer::traffic::Event::ALL {
                    let (suffix, help) = event.descriptor();
                    let name = format!("orishu_worker_peer_{suffix}");
                    writeln!(
                        body,
                        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {}",
                        counters.get(event)
                    )
                    .unwrap();
                }
            }
            for (lane, reading) in [
                ("peer", pressure.peer),
                ("control", pressure.control),
                ("completion", pressure.completion),
                ("shutdown", pressure.shutdown),
            ] {
                for (suffix, help, value) in [
                    (
                        "in_use",
                        "Occupied lane slots including reserved permits.",
                        reading.slots_in_use,
                    ),
                    (
                        "capacity",
                        "Configured lane slot capacity.",
                        reading.capacity,
                    ),
                ] {
                    writeln!(body, "# HELP orishu_worker_{lane}_slots_{suffix} {help}\n# TYPE orishu_worker_{lane}_slots_{suffix} gauge\norishu_worker_{lane}_slots_{suffix} {value}")
                        .expect("formatting into a string");
                }
            }
            self.requests.write(&mut body);
            if let Some(log) = &self.log {
                logs::write(&mut body, &log.stats());
            }
            #[cfg(feature = "otlp-tracing")]
            if let Some(traces) = &self.traces {
                traces.write(&mut body);
            }
            res.headers_mut().insert(
                "content-type",
                "text/plain; version=0.0.4; charset=utf-8".parse().unwrap(),
            );
            res.write_body(body)
                .expect("bounded in-memory diagnostics body");
            return;
        }
        let (ok, reason) = match self.route {
            Route::Live => (live, "owner not responsive\n"),
            Route::Ready => (ready, "local service not ready\n"),
            Route::Startup => (health.startup_complete, "initializing\n"),
            Route::Metrics => unreachable!(),
        };
        res.status_code(if ok {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        });
        res.render(Text::Plain(if ok { "ok\n" } else { reason }));
    }
}

#[cfg(test)]
pub(super) fn router(
    worker: Arc<RunningWorker>,
    config: &crate::config::ObservabilityConfig,
    requests: Arc<Requests>,
) -> Router {
    router_with_telemetry(
        worker,
        config,
        requests,
        None,
        #[cfg(feature = "otlp-tracing")]
        None,
    )
}

pub(super) fn router_with_telemetry(
    worker: Arc<RunningWorker>,
    config: &crate::config::ObservabilityConfig,
    requests: Arc<Requests>,
    log: Option<orishu_worker::operational_log::Log>,
    #[cfg(feature = "otlp-tracing")] traces: Option<Traces>,
) -> Router {
    let mut router = Router::new().goal(not_found);
    for (path, route) in [
        ("metrics", Route::Metrics),
        ("livez", Route::Live),
        ("readyz", Route::Ready),
        ("startupz", Route::Startup),
    ] {
        let enabled = match route {
            Route::Metrics => config.metrics_enabled(),
            Route::Live | Route::Ready | Route::Startup => config.probes_enabled(),
        };
        if !enabled {
            continue;
        }
        // Method rejection belongs to diagnostics, not framework error pages.
        router = router.push(Router::with_path(path).goal(Diagnostic {
            log: log.clone(),
            worker: worker.clone(),
            requests: requests.clone(),
            route,
            #[cfg(feature = "otlp-tracing")]
            traces: traces.clone(),
        }));
    }
    router.push(Router::with_path("{**rest}").goal(not_found))
}

fn write_peer_exchanges(
    body: &mut String,
    counters: &orishu_worker::peer::exchange_metrics::ExchangeSnapshot,
    (in_use, capacity): (usize, usize),
) {
    use orishu_worker::peer::exchange_metrics::{DURATION_BOUNDS, ExchangeOutcome, ExchangeRole};
    for (suffix, kind, help, value) in [
        (
            "slots_in_use",
            "gauge",
            "Occupied reliable exchange slots.",
            in_use as u64,
        ),
        (
            "slots_capacity",
            "gauge",
            "Configured reliable exchange slot capacity.",
            capacity as u64,
        ),
        (
            "bytes_sent_total",
            "counter",
            "Terminal exchanges' stream bytes queued; not delivered.",
            counters.bytes_sent,
        ),
        (
            "bytes_received_total",
            "counter",
            "Terminal exchanges' consumed stream bytes, including invalid input.",
            counters.bytes_received,
        ),
    ] {
        let name = format!("orishu_worker_peer_reliable_{suffix}");
        writeln!(
            body,
            "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
        )
        .unwrap();
    }
    for role in ExchangeRole::ALL {
        let snapshot = counters.role(role);
        for outcome in ExchangeOutcome::ALL {
            let name = format!(
                "orishu_worker_peer_reliable_{}_{}_total",
                role.name(),
                outcome.name()
            );
            writeln!(body, "# HELP {name} Terminal reliable exchange outcomes; not domain decisions.\n# TYPE {name} counter\n{name} {}", snapshot.outcome(outcome)).unwrap();
        }
        let name = format!(
            "orishu_worker_peer_reliable_{}_duration_seconds",
            role.name()
        );
        writeln!(body, "# HELP {name} Exchange lifetime including failures and cancellation.\n# TYPE {name} histogram").unwrap();
        let mut count = 0_u64;
        for (index, value) in snapshot.buckets.iter().enumerate() {
            count = count.saturating_add(*value);
            let le = DURATION_BOUNDS
                .get(index)
                .map_or("+Inf", |(_, label)| *label);
            writeln!(body, "{name}_bucket{{le=\"{le}\"}} {count}").unwrap();
        }
        writeln!(
            body,
            "{name}_sum {}.{:06}\n{name}_count {count}",
            snapshot.duration_micros / 1_000_000,
            snapshot.duration_micros % 1_000_000
        )
        .unwrap();
    }
}

fn write_peer_dials(
    body: &mut String,
    counters: &orishu_worker::peer::dial_metrics::Snapshot,
    (in_use, capacity): (usize, usize),
) {
    use orishu_worker::peer::dial_metrics::{DURATION_BOUNDS, Outcome, Stage};
    for (suffix, kind, help, value) in [
        (
            "slots_in_use",
            "gauge",
            "Occupied outbound attempt slots.",
            in_use as u64,
        ),
        (
            "slots_capacity",
            "gauge",
            "Configured outbound attempt slots.",
            capacity as u64,
        ),
        (
            "capacity_refused_total",
            "counter",
            "Outbound dials refused before a stage starts.",
            counters.capacity_refused,
        ),
    ] {
        let name = format!("orishu_worker_peer_outbound_{suffix}");
        writeln!(
            body,
            "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
        )
        .unwrap();
    }
    for stage in Stage::ALL {
        let snapshot = counters.stage(stage);
        for outcome in Outcome::ALL {
            let name = format!(
                "orishu_worker_peer_outbound_{}_{}_total",
                stage.name(),
                outcome.name()
            );
            writeln!(body, "# HELP {name} Terminal outbound stages; not owner acceptance.\n# TYPE {name} counter\n{name} {}", snapshot.outcome(outcome)).unwrap();
        }
        let name = format!(
            "orishu_worker_peer_outbound_{}_duration_seconds",
            stage.name()
        );
        writeln!(body, "# HELP {name} Stage lifetime including failures and cancellation.\n# TYPE {name} histogram").unwrap();
        let mut count = 0_u64;
        for (i, value) in snapshot.buckets.iter().enumerate() {
            count = count.saturating_add(*value);
            let le = DURATION_BOUNDS.get(i).map_or("+Inf", |(_, label)| *label);
            writeln!(body, "{name}_bucket{{le=\"{le}\"}} {count}").unwrap();
        }
        writeln!(
            body,
            "{name}_sum {}.{:06}\n{name}_count {count}",
            snapshot.duration_micros / 1_000_000,
            snapshot.duration_micros % 1_000_000
        )
        .unwrap();
    }
}

fn write_peer_ingress(body: &mut String, counters: &orishu_worker::peer::ingress::IngressSnapshot) {
    for event in orishu_worker::peer::ingress::IngressEvent::ALL {
        let (name, help) = event.descriptor();
        let value = counters.get(event);
        writeln!(body, "# HELP orishu_worker_peer_inbound_{name}_total {help}\n# TYPE orishu_worker_peer_inbound_{name}_total counter\norishu_worker_peer_inbound_{name}_total {value}").unwrap();
    }
    let pressure = counters.pressure();
    for (name, value) in [
        ("tls_slots_in_use", pressure.tls_slots_in_use),
        ("tls_slots_capacity", pressure.tls_slots_capacity),
        ("connection_slots_in_use", pressure.connection_slots_in_use),
        (
            "connection_slots_capacity",
            pressure.connection_slots_capacity,
        ),
    ] {
        writeln!(body, "# HELP orishu_worker_peer_inbound_{name} Current accepting adapter budget; zero without an adapter.\n# TYPE orishu_worker_peer_inbound_{name} gauge\norishu_worker_peer_inbound_{name} {value}").unwrap();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use orishu_worker::{
        credentials::WorkerCredentials, health::OwnerHealth, runtime::WorkerRuntime,
    };
    use std::{path::Path, time::Duration};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn get(socket: &Path, path: &str) -> String {
        tokio::time::timeout(Duration::from_secs(1), async {
            let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
            stream
                .write_all(
                    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .await
                .unwrap();
            let mut bytes = Vec::new();
            stream.take(32769).read_to_end(&mut bytes).await.unwrap();
            assert!(bytes.len() <= 32768, "bounded diagnostic response");
            String::from_utf8(bytes).unwrap()
        })
        .await
        .expect("diagnostic request deadline")
    }

    async fn assert_health(socket: &Path, live: bool, ready: bool, startup: bool) {
        for (path, success) in [("/livez", live), ("/readyz", ready), ("/startupz", startup)] {
            let response = get(socket, path).await;
            let status = if success { 200 } else { 503 };
            assert!(
                response.starts_with(&format!("HTTP/1.1 {status}")),
                "{path}: {response}"
            );
            assert!(response.contains("cache-control: no-store\r\n"));
            assert!(response.contains("content-type: text/plain"));
            if success {
                assert!(response.ends_with("ok\n"));
            }
        }
        let metrics = get(socket, "/metrics").await;
        assert!(metrics.starts_with("HTTP/1.1 200"));
        assert!(metrics.split_once("\r\n\r\n").unwrap().1.len() < 32768);
        let body = metrics.split_once("\r\n\r\n").unwrap().1;
        let maximum_bytes: usize = body
            .lines()
            .map(|line| {
                if line.starts_with('#') || line.is_empty() {
                    line.len() + 1
                } else {
                    // All current counts/gauges/buckets are integers. Only
                    // duration sums/totals have fractional seconds; reserve
                    // their decimal point and six digits, not for every value.
                    let name = line.split_once(' ').unwrap().0;
                    let fractional = name.ends_with("_sum") || name.ends_with("_seconds_total");
                    name.len() + 1 + 20 + usize::from(fractional) * 7 + 1
                }
            })
            .sum();
        // The trace-only maximum catalogue is separately bounded below 4 KiB.
        assert!(
            maximum_bytes + 4096 < 32768,
            "combined maximum exposition: {maximum_bytes} + 4096"
        );
        for (lane, capacity) in [
            ("peer", 64),
            ("control", 16),
            ("completion", 64),
            ("shutdown", 1),
        ] {
            let value = |suffix: &str| -> usize {
                let prefix = format!("orishu_worker_{lane}_slots_{suffix} ");
                assert!(metrics.contains(&format!(
                    "# TYPE orishu_worker_{lane}_slots_{suffix} gauge\n"
                )));
                metrics
                    .lines()
                    .find_map(|line| line.strip_prefix(&prefix))
                    .unwrap()
                    .parse()
                    .unwrap()
            };
            assert_eq!(value("capacity"), capacity);
            assert!(value("in_use") <= capacity);
        }
        for name in [
            "swim_packets_received",
            "anti_entropy_packets_received",
            "gossip_items_received",
            "peer_decode_rejections",
            "admissions_accepted",
            "admissions_rejected",
            "admission_assignment_replays",
            "membership_transitions",
            "stale_inputs",
            "core_diagnostics",
            "foreign_gossip",
            "reliable_replies",
            "send_failures",
        ] {
            assert!(metrics.contains(&format!("# TYPE orishu_worker_{name}_total counter\n")));
            counter(&metrics, name);
        }
        for (name, value) in [
            ("owner_responsive", live),
            ("ready", ready),
            ("startup_complete", startup),
        ] {
            assert!(metrics.contains(&format!("\norishu_worker_{name} {}\n", u8::from(value))));
        }
    }

    fn counter(response: &str, name: &str) -> u64 {
        let prefix = format!("orishu_worker_{name}_total ");
        response
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .unwrap()
            .parse()
            .unwrap()
    }

    #[cfg(feature = "formation-fault-test")]
    #[tokio::test]
    async fn http_reads_cannot_hide_stalled_owner_shutdown() {
        http_owner_stall(true).await;
    }

    #[tokio::test]
    async fn http_stalled_metrics_reader_expires_without_blocking_probes_or_control() {
        http_stalled_metrics_reader(16).await;
    }

    #[tokio::test]
    async fn http_stalled_metrics_reader_reclaims_the_only_connection_slot() {
        http_stalled_metrics_reader(1).await;
    }

    async fn http_stalled_metrics_reader(capacity: usize) {
        use crate::client_pressure_tests::{CountRequests, ObserveFirst, WriteEvidence};
        use orishu::client::{ClientApi, ClusterApi, MembershipApi};
        use std::sync::atomic::{AtomicUsize, Ordering};

        tokio::time::timeout(Duration::from_secs(12), async {
            let root = tempfile::tempdir().unwrap();
            let state = root.path().join("state");
            let (worker, owner) = WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&state).unwrap(),
                "metrics-reader".parse().unwrap(),
                "metrics-reader".parse().unwrap(),
                vec![],
            )
            .unwrap()
            .with_peer_metrics(true)
            .start();
            let worker = Arc::new(worker);
            let socket = root.path().join("diagnostics.sock");
            let evidence = Arc::new(WriteEvidence::default());
            let acceptor = ObserveFirst {
                inner: UnixListener::new(socket.clone()).bind().await,
                first: Some(evidence.clone()),
            };
            let server = crate::client_server(acceptor).max_connections(capacity);
            let handle = server.handle();
            let calls = Arc::new(AtomicUsize::new(0));
            let diagnostics = router(worker.clone(), &Default::default(), Default::default())
                .hoop(CountRequests(calls.clone()));
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(server.try_serve(diagnostics));

            // Real authenticated operator handlers on their separate listener.
            let operator_socket = root.path().join("operator.sock");
            let operator =
                crate::client_server(UnixListener::new(operator_socket.clone()).bind().await);
            let operator_handle = operator.handle();
            let operator_routes = Router::new()
                .push(
                    Router::with_path("api/v1/cluster").get(crate::ClusterSummaryHandler {
                        runtime: worker.clone(),
                        local: true,
                    }),
                )
                .push(Router::with_path("api/v1/cluster/lock").post(
                    crate::MembershipMutationHandler {
                        kind: crate::MembershipMutation::Lock,
                        runtime: worker.clone(),
                        capacity: Arc::new(tokio::sync::Semaphore::new(16)),
                    },
                ));
            tasks.spawn(operator.try_serve(operator_routes));
            worker.mark_initialized();
            while worker.health().owner == OwnerHealth::Starting {
                tokio::task::yield_now().await;
            }

            let mut reader = tokio::net::UnixStream::connect(&socket).await.unwrap();
            const REQUESTS: usize = 1024;
            // Actual bounded metrics, not an inflated/synthetic response.
            let requests = b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n".repeat(REQUESTS);
            tokio::time::timeout(Duration::from_secs(1), reader.write_all(&requests))
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while !evidence.pending.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("must witness pending diagnostics transport write");
            let pressure_started = tokio::time::Instant::now();
            assert!(evidence.bytes.load(Ordering::SeqCst) > 0);
            assert!(!evidence.dropped.load(Ordering::SeqCst));

            // One-slot variant proves actual capacity reclamation: this probe
            // cannot be served until the original accepted connection is gone.
            let queued = if capacity == 1 {
                let mut queued = tokio::net::UnixStream::connect(&socket).await.unwrap();
                queued
                    .write_all(
                        b"GET /readyz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
                let mut byte = [0];
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), queued.read(&mut byte))
                        .await
                        .is_err(),
                    "full diagnostics capacity served a probe"
                );
                Some(queued)
            } else {
                assert_health(&socket, true, true, true).await;
                None
            };
            let client = orishu::client::http_client::HttpClusterClient::new(
                orishu::client::ClusterAddress::UnixSocket(operator_socket),
                orishu::client::http_client::HttClientOptions {
                    credentials: Some(orishu::client::Credentials::Token(
                        std::fs::read_to_string(state.join("operator.token"))
                            .unwrap()
                            .trim()
                            .to_owned(),
                    )),
                    ..Default::default()
                },
            )
            .unwrap();
            let initial = tokio::time::timeout(Duration::from_secs(1), async {
                let initial = client.cluster().summary().await.unwrap();
                for (operation, locked) in [
                    ("metrics-pressure-lock", true),
                    ("metrics-pressure-unlock", false),
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
                }
                initial
            })
            .await
            .expect("operator control must progress during diagnostics backpressure");

            let stalled_calls = calls.load(Ordering::SeqCst);
            let stalled_bytes = evidence.bytes.load(Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(calls.load(Ordering::SeqCst), stalled_calls);
            assert_eq!(evidence.bytes.load(Ordering::SeqCst), stalled_bytes);
            assert!(
                stalled_calls > 0,
                "pressure must reach the diagnostics handler"
            );
            assert!(
                stalled_calls < REQUESTS,
                "entire pipeline was generated despite backpressure"
            );
            assert!(
                stalled_bytes < 1_048_576,
                "transport buffering exceeded fixture bound"
            );
            assert!(!evidence.dropped.load(Ordering::SeqCst));
            tokio::time::timeout(Duration::from_secs(7), async {
                while !evidence.dropped.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("write expiry must reclaim unread diagnostics connection");
            assert!(evidence.timed_out.load(Ordering::SeqCst));
            assert!(pressure_started.elapsed() >= Duration::from_secs(4));
            if let Some(queued) = queued {
                let mut response = Vec::new();
                tokio::time::timeout(
                    Duration::from_secs(1),
                    queued.take(8193).read_to_end(&mut response),
                )
                .await
                .unwrap()
                .unwrap();
                assert!(response.len() <= 8192);
                assert!(response.starts_with(b"HTTP/1.1 200 "));
                assert!(response.ends_with(b"ok\n"));
            }
            // Healthy reader and exact identity controls, before the fixture
            // drains/closes the stalled client or requests worker shutdown.
            assert_health(&socket, true, true, true).await;
            let after = client.cluster().summary().await.unwrap();
            assert_eq!(after.formation_id, initial.formation_id);
            assert_eq!(after.source_node_id, initial.source_node_id);
            assert_eq!(after.member_count, initial.member_count);
            assert!(!after.membership_locked);
            let mut response = Vec::new();
            if let Err(error) = reader.take(1_048_577).read_to_end(&mut response).await {
                assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
            }
            assert!(response.len() <= 1_048_576);
            assert!(response.starts_with(b"HTTP/1.1 200 "));
            assert!(
                String::from_utf8_lossy(&response).contains("# TYPE orishu_worker_ready gauge")
            );
            worker.shutdown().await.unwrap();
            owner.await.unwrap().unwrap();
            handle.stop_graceful(Some(Duration::from_secs(1)));
            operator_handle.stop_graceful(Some(Duration::from_secs(1)));
            while let Some(result) = tasks.join_next().await {
                result.unwrap().unwrap();
            }
        })
        .await
        .expect("diagnostics slow-reader fixture deadline");
    }

    #[cfg(feature = "formation-fault-test")]
    #[tokio::test]
    async fn http_probes_detect_running_owner_stall_and_recovery() {
        http_owner_stall(false).await;
    }

    #[cfg(feature = "formation-fault-test")]
    async fn http_owner_stall(shutdown: bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            let root = tempfile::tempdir().unwrap();
            let (worker, owner) = WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join("state")).unwrap(),
                "stalled-http".parse().unwrap(),
                "stalled-http".parse().unwrap(),
                vec![],
            )
            .unwrap()
            .start();
            let worker = Arc::new(worker);
            let socket = root.path().join("diagnostics.sock");
            let acceptor = UnixListener::new(socket.clone()).bind().await;
            let server = crate::client_server(acceptor).max_connections(16);
            let handle = server.handle();
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(server.try_serve(router(
                worker.clone(),
                &Default::default(),
                Default::default(),
            )));
            while worker.health().owner == OwnerHealth::Starting {
                tokio::task::yield_now().await;
            }
            worker.mark_initialized();
            assert_health(&socket, true, true, true).await;
            let original = worker.summary().unwrap();
            let (release, held) = tokio::sync::oneshot::channel();
            if shutdown {
                worker.test_hold_shutdown(held).await;
                worker.shutdown().await.unwrap();
                assert_eq!(worker.health().owner, OwnerHealth::Stopping);
            } else {
                worker.test_hold_progress(held).await;
                assert_eq!(worker.health().owner, OwnerHealth::Healthy);
            }
            assert_health(&socket, true, !shutdown, true).await;
            let held_transitions = worker.owner_counters().transitions;

            // The real owner cannot tick while held. Repeated HTTP reads must
            // neither refresh supervision nor need the owner's control lane.
            let mut reads = 0;
            tokio::time::timeout(
                orishu_worker::health::OWNER_PROGRESS_DEADLINE + Duration::from_secs(1),
                async {
                    while worker.health().owner != OwnerHealth::Stalled {
                        assert!(get(&socket, "/metrics").await.starts_with("HTTP/1.1 200"));
                        // A probe may cross the deadline between requests;
                        // before it, the running owner still has grace time.
                        let live = get(&socket, "/livez").await;
                        assert!(
                            live.starts_with("HTTP/1.1 200") || live.starts_with("HTTP/1.1 503")
                        );
                        let ready = get(&socket, "/readyz").await;
                        assert!(
                            ready.starts_with("HTTP/1.1 503")
                                || (!shutdown && ready.starts_with("HTTP/1.1 200"))
                        );
                        reads += 1;
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                },
            )
            .await
            .expect("HTTP reads cannot extend the owner supervision deadline");
            assert!(reads > 1);
            assert!(!owner.is_finished(), "stalled is distinct from closed");
            assert_health(&socket, false, false, true).await;
            assert_eq!(worker.owner_counters().transitions, held_transitions);
            release.send(()).unwrap();
            if !shutdown {
                tokio::time::timeout(Duration::from_secs(2), async {
                    while worker.health().owner != OwnerHealth::Healthy {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("resumed owner publishes its next supervision tick");
                assert_health(&socket, true, true, true).await;
                let recovered = worker.summary().unwrap();
                assert_eq!(recovered.formation_id, original.formation_id);
                assert_eq!(recovered.source_node_id, original.source_node_id);
                assert_eq!(recovered.participation, original.participation);
                assert!(worker.owner_counters().transitions > held_transitions);
                worker.shutdown().await.unwrap();
            }
            assert_eq!(owner.await.unwrap(), Ok(()));
            assert_eq!(worker.health().owner, OwnerHealth::Closed);
            assert_health(&socket, false, false, true).await;
            // Deliberately retained HTTP service proves probe semantics, not
            // the executable supervisor's shutdown ordering.
            handle.stop_graceful(Some(Duration::from_secs(1)));
            tasks.join_next().await.unwrap().unwrap().unwrap();
        })
        .await
        .expect("stalled-owner HTTP fixture deadline");
    }

    #[tokio::test]
    async fn http_probes_track_initialization_role_failure_and_closed_owner() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let root = tempfile::tempdir().unwrap();
            let (worker, owner) = WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join("state")).unwrap(),
                "http-health".parse().unwrap(),
                "http-health".parse().unwrap(),
                vec![],
            )
            .unwrap()
            .start();
            let worker = Arc::new(worker);
            // Keep a production router reachable around lifecycle transitions.
            // Unix avoids port reservation races; executable TCP coverage lives
            // in standalone.rs. This is not executable shutdown-order evidence.
            let socket = root.path().join("diagnostics.sock");
            let acceptor = UnixListener::new(socket.clone()).bind().await;
            let server = crate::client_server(acceptor).max_connections(16);
            let handle = server.handle();
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(server.try_serve(router(
                worker.clone(),
                &Default::default(),
                Default::default(),
            )));
            while worker.health().owner == OwnerHealth::Starting {
                tokio::task::yield_now().await;
            }
            assert_health(&socket, true, false, false).await;
            let role = worker.required_role();
            let mut roles = tokio::task::JoinSet::new();
            roles.spawn(async move {
                let _role = role;
                std::future::pending::<()>().await;
            });
            worker.mark_initialized();
            assert_health(&socket, true, true, true).await;
            for (operation, locked) in [("health-lock", true), ("health-unlock", false)] {
                let before = counter(&get(&socket, "/metrics").await, "membership_transitions");
                let receipt = worker
                    .lock_operation(orishu::model::cluster::LockRequest {
                        schema_version: 1,
                        operation_id: operation.parse().unwrap(),
                        formation_id: worker.summary().unwrap().formation_id,
                        locked,
                    })
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(receipt.locked, locked);
                // Enforcing membership policy safely is a healthy local role.
                assert_health(&socket, true, true, true).await;
                assert!(
                    counter(&get(&socket, "/metrics").await, "membership_transitions") > before
                );
            }
            let before_leave = counter(&get(&socket, "/metrics").await, "membership_transitions");
            let old_formation = worker.summary().unwrap().formation_id;
            let leave = worker
                .leave_operation(orishu::model::cluster::LeaveRequest {
                    schema_version: 1,
                    operation_id: "health-leave".parse().unwrap(),
                    formation_id: old_formation.clone(),
                })
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            assert!(!leave.changed, "standalone leave is an explicit no-op");
            assert_eq!(worker.summary().unwrap().formation_id, old_formation);
            assert!(
                counter(&get(&socket, "/metrics").await, "membership_transitions") >= before_leave
            );
            roles.abort_all();
            assert!(roles.join_next().await.unwrap().unwrap_err().is_cancelled());
            assert_health(&socket, true, false, true).await;
            // Reinitialization must not hide the failed required role.
            worker.mark_initialized();
            assert_health(&socket, true, false, true).await;
            worker.shutdown().await.unwrap();
            assert_eq!(owner.await.unwrap(), Ok(()));
            assert_health(&socket, false, false, true).await;
            let retained = worker.owner_counters();
            let metrics = get(&socket, "/metrics").await;
            for (name, expected) in [
                ("swim_packets_received", retained.traffic.swim),
                (
                    "anti_entropy_packets_received",
                    retained.traffic.anti_entropy,
                ),
                ("gossip_items_received", retained.traffic.gossip_items),
                ("peer_decode_rejections", retained.peer_decode_rejections),
                ("membership_transitions", retained.transitions),
                ("stale_inputs", retained.stale_inputs),
                ("core_diagnostics", retained.diagnostics),
                ("foreign_gossip", retained.foreign_gossip),
                ("reliable_replies", retained.completed_exchanges),
                ("send_failures", retained.failed_sends),
            ] {
                assert_eq!(counter(&metrics, name), expected);
            }
            handle.stop_graceful(Some(Duration::from_secs(1)));
            tasks.join_next().await.unwrap().unwrap().unwrap();
        })
        .await
        .expect("HTTP health lifecycle deadline");
    }

    #[tokio::test]
    async fn http_readiness_waits_for_real_admission_catchup() {
        exercise_http_catchup(CatchupFault::None).await;
    }

    #[cfg(feature = "formation-fault-test")]
    #[tokio::test]
    async fn http_catchup_failure_stays_live_unready_and_recovers() {
        exercise_http_catchup(CatchupFault::Stalled).await;
    }

    #[cfg(feature = "formation-fault-test")]
    #[tokio::test]
    async fn http_malformed_catchup_page_stays_live_unready_and_recovers() {
        exercise_http_catchup(CatchupFault::MalformedPage).await;
    }

    #[cfg(feature = "formation-fault-test")]
    #[tokio::test]
    async fn http_partial_catchup_stays_live_unready_and_recovers() {
        exercise_http_catchup(CatchupFault::PartialBaseline).await;
    }

    #[cfg(feature = "formation-fault-test")]
    #[tokio::test]
    async fn http_pre_adoption_join_stays_live_unready_with_original_identity() {
        exercise_http_catchup(CatchupFault::PreAdoption).await;
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum CatchupFault {
        None,
        #[cfg(feature = "formation-fault-test")]
        Stalled,
        #[cfg(feature = "formation-fault-test")]
        MalformedPage,
        #[cfg(feature = "formation-fault-test")]
        PartialBaseline,
        #[cfg(feature = "formation-fault-test")]
        PreAdoption,
    }

    async fn exercise_http_catchup(fault: CatchupFault) {
        use orishu::model::cluster::{JoinRequest, Participation};
        tokio::time::timeout(Duration::from_secs(40), async {
            let root = tempfile::tempdir().unwrap();
            let start = |name: &str| {
                let runtime = WorkerRuntime::standalone(
                    WorkerCredentials::load_or_create(&root.path().join(name)).unwrap(),
                    name.parse().unwrap(),
                    "health-catchup".parse().unwrap(),
                    vec![],
                )
                .unwrap()
                .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
                .unwrap()
                .with_peer_admission(true)
                .unwrap()
                .with_peer_metrics(true);
                #[cfg(feature = "formation-fault-test")]
                let runtime = if fault == CatchupFault::PartialBaseline && name == "target" {
                    runtime.test_with_paginated_baseline()
                } else {
                    runtime
                };
                runtime.start()
            };
            let (source, source_task) = start("source");
            let (target, target_task) = start("target");
            let source = Arc::new(source);
            let target = Arc::new(target);
            source.start_peer_maintenance();
            target.start_peer_maintenance();
            source.mark_initialized();
            target.mark_initialized();
            let socket = root.path().join("catchup-health.sock");
            let server = crate::client_server(UnixListener::new(socket.clone()).bind().await);
            let server_handle = server.handle();
            let serving = tokio::spawn(server.try_serve(router(
                source.clone(),
                &Default::default(),
                Default::default(),
            )));
            while source.health().owner == OwnerHealth::Starting {
                tokio::task::yield_now().await;
            }
            assert_health(&socket, true, true, true).await;
            let target_id = target.summary().unwrap().formation_id;
            let material = target.join_material().unwrap().await.unwrap().unwrap();
            #[cfg(feature = "formation-fault-test")]
            let original = source.summary().unwrap();
            #[cfg(feature = "formation-fault-test")]
            let admission_lost_ack = if fault == CatchupFault::PreAdoption {
                Some(target.test_lose_next_join_ack().await)
            } else {
                None
            };
            source
                .submit_join(JoinRequest {
                    schema_version: 1,
                    operation_id: "http-catchup".parse().unwrap(),
                    formation_id: source.summary().unwrap().formation_id,
                    material,
                })
                .await
                .unwrap();
            #[cfg(feature = "formation-fault-test")]
            let assigned_before_adoption = if let Some(inserted) = admission_lost_ack {
                let assigned = tokio::time::timeout(Duration::from_secs(5), inserted)
                    .await
                    .expect("real issuer insertion before lost ACK")
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(2), async {
                    while source.summary().unwrap().participation != Participation::Joining {
                        tokio::task::yield_now().await;
                    }
                    let current = source.summary().unwrap();
                    assert_eq!(current.formation_id, original.formation_id);
                    assert_eq!(current.source_node_id, original.source_node_id);
                    assert_ne!(current.formation_id, target_id);
                    assert_eq!(current.member_count, 1);
                    assert!(!current.introducer_ready);
                    assert_health(&socket, true, false, true).await;
                })
                .await
                .expect("pre-adoption probe check before ordinary recovery");
                Some(assigned)
            } else {
                None
            };
            // Admission uses real pinned QUIC and the production owner. Defer
            // starting maintenance, rather than assigning a synthetic health
            // phase, to hold the pre-baseline boundary deterministically.
            while source.summary().unwrap().participation != Participation::CatchingUp {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(source.summary().unwrap().formation_id, target_id);
            #[cfg(feature = "formation-fault-test")]
            if let Some(assigned) = assigned_before_adoption {
                assert_eq!(source.summary().unwrap().source_node_id, assigned);
            }
            assert!(!source.summary().unwrap().introducer_ready);
            assert_eq!(target.summary().unwrap().member_count, 2);
            assert_health(&socket, true, false, true).await;
            #[cfg(feature = "formation-fault-test")]
            let release = if fault == CatchupFault::Stalled {
                // Adoption retires provisional sessions. Wait for actual
                // decoded member traffic on both owners before pausing the
                // issuer, so this holds a catch-up exchange rather than its
                // prerequisite admitted-session handshake.
                while source.owner_counters().traffic.swim == 0
                    || target.owner_counters().traffic.swim == 0
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let (release, held) = tokio::sync::oneshot::channel();
                target.test_hold_progress(held).await;
                Some(release)
            } else {
                None
            };
            #[cfg(feature = "formation-fault-test")]
            let corrupted = if matches!(
                fault,
                CatchupFault::MalformedPage | CatchupFault::PartialBaseline
            ) {
                // The receiver requests page one only after validating page
                // zero. Reaching this indexed fault proves a valid prefix.
                let index = u16::from(fault == CatchupFault::PartialBaseline);
                Some(target.test_corrupt_catchup_page(index).await)
            } else {
                None
            };
            let before = source.summary().unwrap();
            let metrics = get(&socket, "/metrics").await;
            for event in orishu_worker::formation_metrics::CatchupEvent::ALL {
                let (suffix, _) = event.descriptor();
                assert_eq!(
                    counter(
                        &metrics,
                        &format!("catchup_{}", suffix.strip_suffix("_total").unwrap())
                    ),
                    0
                );
            }
            source.start_catchup_maintenance();
            #[cfg(feature = "formation-fault-test")]
            if matches!(
                fault,
                CatchupFault::Stalled | CatchupFault::MalformedPage | CatchupFault::PartialBaseline
            ) {
                let started = tokio::time::Instant::now();
                if let Some(corrupted) = corrupted {
                    tokio::time::timeout(Duration::from_secs(5), corrupted)
                        .await
                        .expect("real page response must reach corruption seam")
                        .unwrap();
                }
                loop {
                    let operation = source
                        .join_status("http-catchup".parse().unwrap())
                        .await
                        .unwrap()
                        .unwrap();
                    let failed = matches!(
                        operation.state,
                        orishu::model::cluster::JoinOperationState::CatchUpFailed { .. }
                    );
                    let current = source.summary().unwrap();
                    assert_eq!(current.formation_id, before.formation_id);
                    assert_eq!(current.source_node_id, before.source_node_id);
                    assert!(!current.introducer_ready);
                    assert_health(&socket, true, false, true).await;
                    if failed {
                        let metrics = get(&socket, "/metrics").await;
                        assert_eq!(counter(&metrics, "catchup_started"), 1);
                        assert_eq!(counter(&metrics, "catchup_transfer_validated"), 0);
                        assert_eq!(counter(&metrics, "catchup_owner_adopted"), 0);
                        assert_eq!(
                            counter(&metrics, "catchup_owner_not_adopted")
                                + counter(&metrics, "catchup_owner_fenced"),
                            1
                        );
                        assert_eq!(
                            counter(
                                &metrics,
                                if fault == CatchupFault::Stalled {
                                    "catchup_transfer_unavailable"
                                } else {
                                    "catchup_transfer_invalid"
                                }
                            ),
                            1
                        );
                        break;
                    }
                    assert!(
                        started.elapsed() < Duration::from_secs(12),
                        "faulted transfer must fail within exchange bounds"
                    );
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                if let Some(release) = release {
                    assert!(
                        started.elapsed() >= Duration::from_secs(4),
                        "observe a stalled exchange, not immediate setup refusal"
                    );
                    release.send(()).unwrap();
                }
            }
            #[cfg(not(feature = "formation-fault-test"))]
            assert!(fault == CatchupFault::None);
            while source.summary().unwrap().participation != Participation::Joined {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(source.summary().unwrap().introducer_ready);
            assert_eq!(source.summary().unwrap().formation_id, before.formation_id);
            assert_eq!(
                source.summary().unwrap().source_node_id,
                before.source_node_id
            );
            assert_health(&socket, true, true, true).await;
            let metrics = get(&socket, "/metrics").await;
            assert_eq!(counter(&metrics, "catchup_transfer_validated"), 1);
            assert_eq!(counter(&metrics, "catchup_owner_adopted"), 1);
            assert_eq!(
                counter(&metrics, "catchup_started"),
                counter(&metrics, "catchup_owner_adopted")
                    + counter(&metrics, "catchup_owner_not_adopted")
                    + counter(&metrics, "catchup_owner_fenced")
                    + counter(&metrics, "catchup_owner_abandoned")
            );
            source.shutdown().await.unwrap();
            target.shutdown().await.unwrap();
            assert_eq!(source_task.await.unwrap(), Ok(()));
            assert_eq!(target_task.await.unwrap(), Ok(()));
            server_handle.stop_graceful(Some(Duration::from_secs(1)));
            serving.await.unwrap().unwrap();
        })
        .await
        .expect("real catch-up HTTP health deadline");
    }
}
