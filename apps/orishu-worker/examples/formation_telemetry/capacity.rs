//! Bounded colocated API capacity diagnostic, distinct from overhead acceptance.
//! Storage is O(local workers * samples per worker), acquired before READY.
use super::*;

const PER_WORKER_CAP: usize = 1_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkTarget {
    endpoint: std::net::SocketAddr,
    formation: FormationId,
    node: NodeId,
    certificate: PathBuf,
    token_file: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkConfig {
    schema_version: u32,
    global_workers: Option<usize>,
    first_role: usize,
    targets: Vec<NetworkTarget>,
    clients_per_worker: usize,
    rate_per_worker: Option<usize>,
}

impl NetworkConfig {
    fn local_shape(&self) -> Result<Config, Error> {
        if !matches!(self.schema_version, 2..=4)
            || (self.schema_version == 2 && self.global_workers.is_some())
            || (self.schema_version >= 3 && self.global_workers.is_none())
        {
            return Err("invalid network capacity profile".into());
        }
        for (index, target) in self.targets.iter().enumerate() {
            if target.endpoint.ip().is_unspecified()
                || target.endpoint.ip().is_multicast()
                || !(9440..=9443).contains(&target.endpoint.port())
                || !target.certificate.is_absolute()
                || !target.token_file.is_absolute()
                || self.targets[..index]
                    .iter()
                    .any(|t| t.endpoint == target.endpoint)
            {
                return Err("invalid network capacity target".into());
            }
        }
        let config = Config {
            schema_version: self.schema_version - 1,
            global_workers: self.global_workers,
            first_role: self.first_role,
            // Only the existing summary validator consumes these identities;
            // network clients below never open these synthetic socket paths.
            targets: self
                .targets
                .iter()
                .enumerate()
                .map(|(index, t)| Target {
                    socket: PathBuf::from(format!("/network-identity-{index}")),
                    formation: t.formation.clone(),
                    node: t.node.clone(),
                })
                .collect(),
            clients_per_worker: self.clients_per_worker,
            rate_per_worker: self.rate_per_worker,
        };
        config.validate()?;
        Ok(config)
    }
}

pub(super) async fn run_network(path: PathBuf) -> Result<(), Error> {
    use orishu::client::Credentials;
    let network: NetworkConfig = serde_json::from_slice(&read_load_config(path)?)?;
    let config = network.local_shape()?;
    let mut clients = Vec::with_capacity(config.targets.len() * config.clients_per_worker);
    for target in network.targets {
        // This private lab profile accepts files, never credential argv/env.
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(&target.token_file)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("operator credential must be a private regular file".into());
        }
        let mut token = String::new();
        std::fs::File::open(&target.token_file)?
            .take(129)
            .read_to_string(&mut token)?;
        let token = token.trim();
        if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("invalid operator credential shape".into());
        }
        if std::fs::metadata(&target.certificate)?.len() > 16_384 {
            return Err("certificate byte cap".into());
        }
        for _ in 0..config.clients_per_worker {
            clients.push(HttpClusterClient::new(
                ClusterAddress::Ip(target.endpoint),
                HttClientOptions {
                    timeout: Some(Duration::from_secs(1)),
                    credentials: Some(Credentials::Token(token.to_owned())),
                    tls_cert: Some(target.certificate.clone()),
                    ..Default::default()
                },
            )?);
        }
    }
    run_clients(config, clients, true).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    global_workers: Option<usize>,
    first_role: usize,
    targets: Vec<Target>,
    clients_per_worker: usize,
    /// Null is bounded-concurrency, unpaced load; otherwise offered requests/s/worker.
    rate_per_worker: Option<usize>,
}

impl Config {
    fn total(&self) -> usize {
        self.global_workers.unwrap_or(16)
    }

    fn validate(&self) -> Result<(), Error> {
        let local = self.targets.len();
        let shape = match self.schema_version {
            1 => self.global_workers.is_none() && local == 4,
            2 | 3 => self.global_workers.is_some() && matches!(local, 1 | 4),
            _ => false,
        };
        if !shape
            || !(3..=20).contains(&self.total())
            || !self.total().is_multiple_of(local)
            || !(3..=5).contains(&(self.total() / local))
            || !self.first_role.is_multiple_of(local)
            || self.first_role >= self.total()
            || !matches!(self.clients_per_worker, 2 | 8 | 32 | 64)
            || self
                .rate_per_worker
                .is_some_and(|rate| !(100..=100_000).contains(&rate))
        {
            return Err("invalid capacity profile".into());
        }
        for (index, target) in self.targets.iter().enumerate() {
            if !target.socket.is_absolute()
                || target.socket.as_os_str().len() > 100
                || self
                    .targets
                    .iter()
                    .take(index)
                    .any(|other| other.socket == target.socket || other.node == target.node)
                || target.formation != self.targets[0].formation
            {
                return Err("invalid capacity target identity".into());
            }
        }
        Ok(())
    }
}

/// Integer slot arithmetic preserves exactly rate * 10 offered arrivals across
/// clients, even when the rate is not divisible by concurrency. Missed slots
/// are counted, never queued or replayed. Tokio timer granularity is observable
/// generator delay, not a claim of sub-millisecond wakeup precision.
#[derive(Clone, Copy)]
struct Plan {
    rate: usize,
    clients: usize,
    client: usize,
}

impl Plan {
    fn slots(self) -> usize {
        (self.rate * 10)
            .saturating_sub(self.client)
            .div_ceil(self.clients)
    }

    fn due(self, slot: usize) -> Duration {
        Duration::from_nanos(
            ((slot * self.clients + self.client) as u64 * 1_000_000_000) / self.rate as u64,
        )
    }

    fn next(self, elapsed: Duration, minimum: usize) -> usize {
        let global = elapsed.as_nanos() * self.rate as u128 / 1_000_000_000;
        let local = global.saturating_sub(self.client as u128) / self.clients as u128;
        minimum.max(local.min(self.slots() as u128) as usize)
    }
}

/// Fixed categories only: never serialize an error's hostile message or URL.
fn error_category(error: &orishu::client::ClientError) -> (&'static str, Option<u16>) {
    use orishu::client::ClientError;
    match error {
        ClientError::DataSerialization(_) => ("serialization", None),
        ClientError::ConnectionFailed(_) => ("connection", None),
        ClientError::TransportError(message) => {
            if let Some(status) = message
                .strip_prefix("empty response body with status ")
                .and_then(|s| s.parse::<u16>().ok())
                .filter(|s| (100..=599).contains(s))
            {
                ("transport_empty_body", Some(status))
            } else if message.starts_with("failed to deserialize response body: ") {
                ("transport_body_decode", None)
            } else if message.starts_with("failed to read response body: ") {
                ("transport_body_read", None)
            } else {
                ("transport_other", None)
            }
        }
        ClientError::AuthenticationRequired => ("authentication", None),
        ClientError::AuthorizationDenied => ("authorization", None),
        ClientError::ResourceNotFound(_) => ("not_found", None),
        ClientError::UnexpectedResponseType(_) => ("unexpected_response", None),
        ClientError::ApiError { status, .. } => ("api", Some(*status)),
        ClientError::InvalidState(_) => ("invalid_state", None),
    }
}

#[derive(Clone, serde::Serialize)]
struct ErrorSample {
    kind: &'static str,
    /// ClientError status; 0 or non-HTTP values remain possible in this API.
    status: Option<u16>,
    request_started_us: u64,
    completed_us: u64,
    duration_us: u64,
    after_window: bool,
}

fn error_details(samples: &[ErrorSample]) -> serde_json::Value {
    let mut counts = std::collections::BTreeMap::new();
    for sample in samples {
        *counts
            .entry((sample.kind, sample.status))
            .or_insert(0_usize) += 1;
    }
    serde_json::json!({
        "counts": counts.into_iter().map(|((kind, status), count)|
            serde_json::json!({"kind":kind,"status":status,"count":count})).collect::<Vec<_>>(),
        "first": samples.iter().min_by_key(|s| s.completed_us),
    })
}

struct ResultRow {
    role: usize,
    latency: Vec<u64>,
    delay: Vec<u64>,
    skipped: usize,
    tail: usize,
    errors: usize,
    invalid: usize,
    capped: bool,
    error: Option<ErrorSample>,
}

struct ClientRun {
    client: HttpClusterClient,
    target: Target,
    role: usize,
    global_workers: usize,
    cap: usize,
    plan: Option<Plan>,
    ready: Arc<Barrier>,
    start: watch::Receiver<Option<Instant>>,
}

async fn drive(mut run: ClientRun) -> Result<ResultRow, Error> {
    let client = run.client;
    let mut row = ResultRow {
        role: run.role,
        latency: Vec::with_capacity(run.cap),
        delay: Vec::with_capacity(if run.plan.is_some() { run.cap } else { 0 }),
        skipped: 0,
        tail: 0,
        errors: 0,
        invalid: 0,
        capped: false,
        error: None,
    };
    for _ in 0..16 {
        validate(
            &client.cluster().summary().await?,
            &run.target,
            run.global_workers,
        )?;
    }
    run.ready.wait().await;
    run.start.changed().await?;
    let started = (*run.start.borrow()).ok_or("capacity start missing")?;
    let end = started + WINDOW;
    let mut slot = 0;
    while Instant::now() < end {
        let due = if let Some(plan) = run.plan {
            if slot == plan.slots() {
                break;
            }
            tokio::time::sleep_until(started + plan.due(slot)).await;
            let next = plan.next(Instant::now().duration_since(started), slot);
            row.skipped += next - slot;
            slot = next;
            if slot == plan.slots() || Instant::now() >= end {
                break;
            }
            let due = started + plan.due(slot);
            slot += 1;
            Some(due)
        } else {
            None
        };
        if row.latency.len() == run.cap {
            row.capped = true;
            break;
        }
        let request = Instant::now();
        let response = client.cluster().summary().await;
        let completed = Instant::now();
        match response {
            Err(error) => {
                row.errors += 1;
                let (kind, status) = error_category(&error);
                let request_started_us = request.duration_since(started).as_micros().try_into()?;
                let completed_us = completed.duration_since(started).as_micros().try_into()?;
                row.error = Some(ErrorSample {
                    kind,
                    status,
                    request_started_us,
                    completed_us,
                    duration_us: completed_us - request_started_us,
                    after_window: completed > end,
                });
                break;
            }
            Ok(view) if validate(&view, &run.target, run.global_workers).is_err() => {
                row.invalid += 1;
                break;
            }
            Ok(_) if completed > end => row.tail += 1,
            Ok(_) => {
                row.latency
                    .push(completed.duration_since(request).as_nanos().try_into()?);
                if let Some(due) = due {
                    row.delay
                        .push(request.duration_since(due).as_nanos().try_into()?);
                }
            }
        }
    }
    if let Some(plan) = run.plan {
        row.skipped += plan.slots() - slot;
    }
    Ok(row)
}

pub(super) async fn run(path: PathBuf) -> Result<(), Error> {
    let config: Config = serde_json::from_slice(&read_load_config(path)?)?;
    config.validate()?;
    let clients = build_clients(&config)?;
    run_clients(config, clients, false).await
}

async fn run_clients(
    config: Config,
    clients: Vec<HttpClusterClient>,
    network: bool,
) -> Result<(), Error> {
    let ready = Arc::new(Barrier::new(
        config.targets.len() * config.clients_per_worker + 1,
    ));
    let (start, signal) = watch::channel(None);
    let mut tasks = JoinSet::new();
    // ClientBuilder does synchronous setup. Finish ALL construction before
    // spawning any request task, so it cannot starve in-flight warmup requests
    // on the probe's two executor threads. Construction remains outside timing.
    let mut clients = clients.into_iter();
    for (index, target) in config.targets.iter().enumerate() {
        for client in 0..config.clients_per_worker {
            let plan = config.rate_per_worker.map(|rate| Plan {
                rate,
                clients: config.clients_per_worker,
                client,
            });
            tasks.spawn(drive(ClientRun {
                client: clients.next().ok_or("missing constructed client")?,
                target: target.clone(),
                role: config.first_role + index,
                global_workers: config.total(),
                cap: plan.map_or(PER_WORKER_CAP / config.clients_per_worker, Plan::slots),
                plan,
                ready: ready.clone(),
                start: signal.clone(),
            }));
        }
    }
    tokio::select! {
        _ = ready.wait() => {},
        result = tasks.join_next() => {
            result.ok_or("missing capacity task")???;
            return Err("capacity client finished before READY".into());
        }
    }
    println!("READY");
    std::io::stdout().flush()?;
    std::io::stdin().read_exact(&mut [0_u8; 1])?;
    let (started, start_mark) = ClockMark::capture()?;
    start.send(Some(started))?;
    tokio::time::sleep_until(started + WINDOW).await;
    let (ended, end_mark) = ClockMark::capture()?;
    println!("END");
    std::io::stdout().flush()?;
    std::io::stdin().read_exact(&mut [0_u8; 1])?;
    let mut results = Vec::with_capacity(config.targets.len() * config.clients_per_worker);
    while let Some(result) = tasks.join_next().await {
        results.push(result??);
    }
    let mut workers = Vec::with_capacity(config.targets.len());
    for role in config.first_role..config.first_role + config.targets.len() {
        let mut latency = Vec::new();
        let mut delay = Vec::new();
        let (mut skipped, mut tail, mut errors, mut invalid, mut capped) = (0, 0, 0, 0, false);
        for row in results.iter_mut().filter(|row| row.role == role) {
            latency.append(&mut row.latency);
            delay.append(&mut row.delay);
            skipped += row.skipped;
            tail += row.tail;
            errors += row.errors;
            invalid += row.invalid;
            capped |= row.capped;
        }
        let mut worker = serde_json::json!({
            "role": role, "latency": summarize(&mut latency), "scheduling_delay": summarize(&mut delay),
            "scheduled_arrivals": config.rate_per_worker.map(|rate| rate * 10),
            "skipped_arrivals": skipped, "tail_requests": tail,
            "transport_errors": errors, "invalid_responses": invalid, "capped": capped,
        });
        if config.schema_version == 3 {
            let samples = results
                .iter()
                .filter(|r| r.role == role)
                .filter_map(|r| r.error.clone())
                .collect::<Vec<_>>();
            worker["client_error_details"] = error_details(&samples);
        }
        workers.push(worker);
    }
    println!(
        "{}",
        serde_json::json!({
            "schema_version": config.schema_version + u32::from(network),
            "kind": if network { "pi-network-capacity-load" } else { "pi-capacity-load" }, "global_workers": config.total(),
            "first_role": config.first_role, "clients_per_worker": config.clients_per_worker,
            "rate_per_worker": config.rate_per_worker, "seconds": 10, "workers": workers,
            "start": start_mark, "end_marker": end_mark,
            "end_marker_elapsed_ns": u64::try_from(ended.duration_since(started).as_nanos())?,
        })
    );
    Ok(())
}

fn build_clients(config: &Config) -> Result<Vec<HttpClusterClient>, Error> {
    config.validate()?;
    let mut clients = Vec::with_capacity(config.targets.len() * config.clients_per_worker);
    for target in &config.targets {
        for _ in 0..config.clients_per_worker {
            clients.push(HttpClusterClient::new(
                ClusterAddress::UnixSocket(target.socket.clone()),
                HttClientOptions {
                    timeout: Some(Duration::from_secs(1)),
                    ..Default::default()
                },
            )?);
        }
    }
    Ok(clients)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_receipts_are_bounded_categories_without_hostile_text() {
        use orishu::client::ClientError;
        let (kind, status) = error_category(&ClientError::ApiError {
            status: 503,
            message: "SECRET http://private/".into(),
        });
        let later = ErrorSample {
            kind,
            status,
            request_started_us: 100,
            completed_us: 200,
            duration_us: 100,
            after_window: false,
        };
        let earlier = ErrorSample {
            completed_us: 150,
            duration_us: 50,
            ..later.clone()
        };
        let value = error_details(&[later, earlier]);
        assert_eq!(value["counts"][0]["count"], 2);
        assert_eq!(value["first"]["completed_us"], 150);
        assert!(!value.to_string().contains("SECRET"));
        assert!(!value.to_string().contains("private"));
        assert_eq!(
            error_details(&[]),
            serde_json::json!({"counts":[],"first":null})
        );
        assert_eq!(
            error_category(&ClientError::TransportError(
                "empty response body with status 999 SECRET".into()
            )),
            ("transport_other", None)
        );
        assert_eq!(
            error_category(&ClientError::TransportError(
                "failed to deserialize response body: SECRET".into()
            )),
            ("transport_body_decode", None)
        );
        assert_eq!(
            error_category(&ClientError::ConnectionFailed("SECRET".into())),
            ("connection", None)
        );
    }

    #[tokio::test]
    async fn classifies_empty_503_through_the_real_http_client() {
        use std::os::unix::net::UnixListener;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("api.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0_u8; 8192];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let client = HttpClusterClient::new(
            ClusterAddress::UnixSocket(path),
            HttClientOptions {
                timeout: Some(Duration::from_secs(2)),
                ..Default::default()
            },
        )
        .unwrap();
        let error = client.cluster().summary().await.err().unwrap();
        server.join().unwrap();
        assert_eq!(error_category(&error), ("transport_empty_body", Some(503)));
    }

    #[test]
    fn versioned_profiles_bound_hosts_workers_and_global_identity() {
        for local in [1, 4] {
            for hosts in [3, 4, 5] {
                let targets = (0..local)
                    .map(|slot| {
                        serde_json::json!({
                            "endpoint": format!("192.0.2.11:{}", 9440 + slot),
                            "formation": "formation", "node": format!("node-{slot}"),
                            "certificate": "/tmp/cert", "token_file": "/tmp/token"
                        })
                    })
                    .collect::<Vec<_>>();
                let value = serde_json::json!({"schema_version": 3,
                    "global_workers": hosts * local, "first_role": (hosts - 1) * local,
                    "targets": targets, "clients_per_worker": 32, "rate_per_worker": 5000});
                let config = serde_json::from_value::<NetworkConfig>(value.clone())
                    .unwrap()
                    .local_shape()
                    .unwrap();
                assert_eq!(config.total(), hosts * local);
                let mut detailed = value.clone();
                detailed["schema_version"] = 4.into();
                assert_eq!(
                    serde_json::from_value::<NetworkConfig>(detailed)
                        .unwrap()
                        .local_shape()
                        .unwrap()
                        .schema_version,
                    3
                );
                for total in [0, 2, 21, usize::MAX] {
                    let mut bad = value.clone();
                    bad["global_workers"] = total.into();
                    assert!(
                        serde_json::from_value::<NetworkConfig>(bad)
                            .unwrap()
                            .local_shape()
                            .is_err()
                    );
                }
                let mut old = value;
                old["schema_version"] = 2.into();
                assert!(
                    serde_json::from_value::<NetworkConfig>(old)
                        .unwrap()
                        .local_shape()
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn network_profile_rejects_bad_versions_duplicates_and_unbounded_endpoints() {
        let targets = (0..4)
            .map(|slot| {
                serde_json::json!({
                    "endpoint": format!("127.0.0.1:{}", 9440 + slot),
                    "formation": "formation", "node": format!("node-{slot}"),
                    "certificate": "/tmp/cert", "token_file": "/tmp/token"
                })
            })
            .collect::<Vec<_>>();
        let value = serde_json::json!({"schema_version": 2, "first_role": 0,
            "targets": targets, "clients_per_worker": 32, "rate_per_worker": 5000});
        assert!(
            serde_json::from_value::<NetworkConfig>(value.clone())
                .unwrap()
                .local_shape()
                .is_ok()
        );
        for mutation in 0..6 {
            let mut bad = value.clone();
            match mutation {
                0 => bad["schema_version"] = 1.into(),
                1 => bad["targets"][1]["endpoint"] = "127.0.0.1:9440".into(),
                2 => bad["targets"][0]["endpoint"] = "0.0.0.0:9440".into(),
                3 => bad["targets"][0]["token_file"] = "relative".into(),
                4 => bad["targets"][0]["endpoint"] = "127.0.0.1:80".into(),
                _ => bad["targets"][0]["formation"] = "other".into(),
            }
            assert!(
                serde_json::from_value::<NetworkConfig>(bad)
                    .unwrap()
                    .local_shape()
                    .is_err()
            );
        }
        let mut bad = value;
        bad["targets"][0]["token"] = "inline-credential".into();
        assert!(serde_json::from_value::<NetworkConfig>(bad).is_err());
    }

    #[test]
    fn rate_slots_are_exact_and_bounded_for_every_supported_concurrency() {
        for rate in [100, 312, 1250, 5000, 100_000] {
            for clients in [2, 8, 32, 64] {
                let mut slots = 0;
                for client in 0..clients {
                    let plan = Plan {
                        rate,
                        clients,
                        client,
                    };
                    slots += plan.slots();
                    assert!(plan.due(plan.slots() - 1) < WINDOW);
                    assert!(plan.due(plan.slots()) >= WINDOW);
                    assert_eq!(plan.next(Duration::from_secs(100), 0), plan.slots());
                    assert!(plan.next(plan.due(2), 2) >= 2);
                }
                assert_eq!(slots, rate * 10);
                assert!(slots <= PER_WORKER_CAP);
            }
        }
    }

    #[test]
    fn profile_rejects_wrong_topology_before_clients_are_created() {
        let config = Config {
            schema_version: 1,
            global_workers: None,
            first_role: 0,
            targets: vec![],
            clients_per_worker: 32,
            rate_per_worker: Some(5000),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn all_clients_can_be_constructed_without_sending_a_warmup_request() {
        let targets = (0..4)
            .map(|index| {
                serde_json::json!({
                    "socket": format!("/tmp/not-running-capacity-worker-{index}.sock"),
                    "formation": "formation", "node": format!("node-{index}")
                })
            })
            .collect::<Vec<_>>();
        let config: Config = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "first_role": 12, "targets": targets,
            "clients_per_worker": 32, "rate_per_worker": 5000
        }))
        .unwrap();
        // No server exists. Success proves this cold phase performs no request;
        // run() completes this call before creating any drive() task.
        assert_eq!(build_clients(&config).unwrap().len(), 128);
    }
}
