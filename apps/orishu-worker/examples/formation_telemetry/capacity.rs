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
    first_role: usize,
    targets: Vec<NetworkTarget>,
    clients_per_worker: usize,
    rate_per_worker: Option<usize>,
}

impl NetworkConfig {
    fn local_shape(&self) -> Result<Config, Error> {
        if self.schema_version != 2 || self.targets.len() != 4 {
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
            schema_version: 1,
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
    let mut clients = Vec::with_capacity(4 * config.clients_per_worker);
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
    first_role: usize,
    targets: Vec<Target>,
    clients_per_worker: usize,
    /// Null is bounded-concurrency, unpaced load; otherwise offered requests/s/worker.
    rate_per_worker: Option<usize>,
}

impl Config {
    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1
            || !matches!(self.first_role, 0 | 4 | 8 | 12)
            || self.targets.len() != 4
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

struct ResultRow {
    role: usize,
    latency: Vec<u64>,
    delay: Vec<u64>,
    skipped: usize,
    tail: usize,
    errors: usize,
    invalid: usize,
    capped: bool,
}

struct ClientRun {
    client: HttpClusterClient,
    target: Target,
    role: usize,
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
    };
    for _ in 0..16 {
        validate(&client.cluster().summary().await?, &run.target, 16)?;
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
            Err(_) => {
                row.errors += 1;
                break;
            }
            Ok(view) if validate(&view, &run.target, 16).is_err() => {
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
    let ready = Arc::new(Barrier::new(4 * config.clients_per_worker + 1));
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
    let mut results = Vec::with_capacity(4 * config.clients_per_worker);
    while let Some(result) = tasks.join_next().await {
        results.push(result??);
    }
    let mut workers = Vec::with_capacity(4);
    for role in config.first_role..config.first_role + 4 {
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
        workers.push(serde_json::json!({
            "role": role, "latency": summarize(&mut latency), "scheduling_delay": summarize(&mut delay),
            "scheduled_arrivals": config.rate_per_worker.map(|rate| rate * 10),
            "skipped_arrivals": skipped, "tail_requests": tail,
            "transport_errors": errors, "invalid_responses": invalid, "capped": capped,
        }));
    }
    println!(
        "{}",
        serde_json::json!({
            "schema_version": if network { 2 } else { 1 },
            "kind": if network { "pi-network-capacity-load" } else { "pi-capacity-load" }, "global_workers": 16,
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
    let mut clients = Vec::with_capacity(4 * config.clients_per_worker);
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
