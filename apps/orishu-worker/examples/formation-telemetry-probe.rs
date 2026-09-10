//! Linux measurement tooling, never linked into the worker executable.
//!
//! Load clients use the public typed client. The separately spawned collector
//! decodes real OTLP; neither process's CPU is attributed to workers.
use clap::{Parser, Subcommand};
use orishu::client::{
    ClientApi, ClusterAddress, ClusterApi,
    http_client::{HttClientOptions, HttpClusterClient},
};
use orishu::model::cluster::{FormationId, Summary};
use orishu::model::node::NodeId;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{Barrier, watch},
    task::JoinSet,
    time::Instant,
};

#[path = "formation_telemetry/collector.rs"]
mod collector;
#[path = "formation_telemetry/observer.rs"]
mod observer;

type Error = Box<dyn std::error::Error + Send + Sync>;
const SAMPLE_CAP: usize = 500_000;
const WINDOW: Duration = Duration::from_secs(10);

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Warm all clients, print READY, wait for one stdin byte, measure ten seconds.
    Load {
        config: PathBuf,
        /// Fixed 500 requests/s/worker; omitted retains the historical saturation profile.
        #[arg(long)]
        fixed_rate: bool,
    },
    /// Bounded loopback protobuf receiver; print final receipt on SIGTERM.
    Collect { workers: usize },
    /// Diagnostic only: bounded stdin queries over persistent public clients.
    Observe { root: PathBuf, workers: usize },
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    socket: PathBuf,
    formation: FormationId,
    node: NodeId,
}

fn validate(view: &Summary, target: &Target, workers: usize) -> Result<(), Error> {
    if view.formation_id != target.formation
        || view.source_node_id != target.node
        || view.member_count != workers
        || view.alive_count != workers
        || view.membership_locked
        || !view.introducer_ready
    {
        return Err("load response changed formation, membership, readiness or policy".into());
    }
    Ok(())
}

#[derive(Serialize)]
struct Latency {
    requests: usize,
    median_us: Option<f64>,
    p95_us: Option<f64>,
}

fn summarize(samples: &mut [u64]) -> Latency {
    samples.sort_unstable();
    Latency {
        requests: samples.len(),
        median_us: samples.get(samples.len() / 2).map(|n| *n as f64 / 1000.0),
        p95_us: samples
            .get(samples.len().saturating_sub(1) * 95 / 100)
            .map(|n| *n as f64 / 1000.0),
    }
}

struct Samples {
    role: usize,
    latency: Vec<u64>,
    tail_requests: usize,
    capped: bool,
    scheduled_latency: Vec<u64>,
    scheduling_delay: Vec<u64>,
    skipped_arrivals: usize,
}

/// Absolute, phase-staggered arrivals with no queue or replay of missed slots.
#[derive(Clone, Copy)]
struct ArrivalPlan {
    phase: Duration,
}

impl ArrivalPlan {
    const PERIOD: Duration = Duration::from_millis(4);
    const SLOTS: usize = 2500;

    fn new(client: usize, clients: usize) -> Self {
        Self {
            phase: Self::PERIOD.mul_f64(client as f64 / clients as f64),
        }
    }

    fn due(self, slot: usize) -> Duration {
        self.phase + Self::PERIOD * slot as u32
    }

    fn next_slot(self, elapsed: Duration, minimum: usize) -> usize {
        let current = elapsed.saturating_sub(self.phase).as_nanos() / Self::PERIOD.as_nanos();
        minimum.max(current.min(Self::SLOTS as u128) as usize)
    }
}

async fn client(
    target: Target,
    role: usize,
    workers: usize,
    ready: Arc<Barrier>,
    mut start: watch::Receiver<Option<Instant>>,
    plan: Option<ArrivalPlan>,
) -> Result<Samples, Error> {
    let client = HttpClusterClient::new(
        ClusterAddress::UnixSocket(target.socket.clone()),
        HttClientOptions {
            timeout: Some(Duration::from_secs(1)),
            ..Default::default()
        },
    )?;
    // Storage is acquired before warmup/barrier. Only sample writes in the loop;
    // public HTTP/serde allocations are part of the real client workload.
    let cap = if plan.is_some() {
        ArrivalPlan::SLOTS
    } else {
        SAMPLE_CAP
    };
    let mut latency = Vec::with_capacity(cap);
    let mut scheduled_latency = Vec::with_capacity(if plan.is_some() { cap } else { 0 });
    let mut scheduling_delay = Vec::with_capacity(if plan.is_some() { cap } else { 0 });
    for _ in 0..64 {
        validate(&client.cluster().summary().await?, &target, workers)?;
    }
    ready.wait().await;
    start.changed().await?;
    let started = (*start.borrow()).ok_or("missing shared start")?;
    tokio::time::sleep_until(started).await;
    let end = started + WINDOW;
    let mut tail_requests = 0;
    let mut capped = false;
    let mut slot = 0;
    let mut skipped_arrivals = 0;
    while Instant::now() < end {
        let due = if let Some(plan) = plan {
            if slot == ArrivalPlan::SLOTS {
                break;
            }
            tokio::time::sleep_until(started + plan.due(slot)).await;
            let next = plan.next_slot(Instant::now().duration_since(started), slot);
            skipped_arrivals += next - slot;
            slot = next;
            if slot == ArrivalPlan::SLOTS || Instant::now() >= end {
                break;
            }
            let due = started + plan.due(slot);
            slot += 1;
            Some(due)
        } else {
            None
        };
        if latency.len() == cap {
            capped = true;
            break;
        }
        let request = Instant::now();
        let view = client.cluster().summary().await?;
        let completed = Instant::now();
        validate(&view, &target, workers)?;
        if completed <= end {
            latency.push(completed.duration_since(request).as_nanos().try_into()?);
            if let Some(due) = due {
                scheduled_latency.push(completed.duration_since(due).as_nanos().try_into()?);
                scheduling_delay.push(request.duration_since(due).as_nanos().try_into()?);
            }
        } else {
            tail_requests += 1;
        }
    }
    if plan.is_some() {
        skipped_arrivals += ArrivalPlan::SLOTS - slot;
    }
    Ok(Samples {
        role,
        latency,
        tail_requests,
        capped,
        scheduled_latency,
        scheduling_delay,
        skipped_arrivals,
    })
}

async fn load(path: PathBuf, fixed_rate: bool) -> Result<(), Error> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(32769)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32768 {
        return Err("load configuration exceeds byte cap".into());
    }
    let targets: Vec<Target> = serde_json::from_slice(&bytes)?;
    if !matches!(targets.len(), 3 | 10 | 30) {
        return Err("unsupported worker count".into());
    }
    let workers = targets.len();
    let ready = Arc::new(Barrier::new(2 * workers + 1));
    let (start, signal) = watch::channel(None);
    let mut tasks = JoinSet::new();
    for (role, target) in targets.into_iter().enumerate() {
        for client_index in 0..2 {
            tasks.spawn(client(
                target.clone(),
                role,
                workers,
                ready.clone(),
                signal.clone(),
                fixed_rate.then(|| ArrivalPlan::new(2 * role + client_index, 2 * workers)),
            ));
        }
    }
    // A failed warmup must not strand the barrier until the outer deadline.
    tokio::select! {
        _ = ready.wait() => {},
        result = tasks.join_next() => {
            result.ok_or("missing client task")???;
            return Err("client finished before start".into());
        }
    }
    println!("READY");
    std::io::stdout().flush()?;
    // Outside the measured region, and guarded by the runner's process deadline.
    std::io::stdin().read_exact(&mut [0_u8; 1])?;
    let started = Instant::now();
    start.send(Some(started))?;
    tokio::time::sleep_until(started + WINDOW).await;
    println!("END");
    std::io::stdout().flush()?;
    // Keep /proc available until the runner has taken its end sample. No
    // percentile sorting or report encoding may contaminate that CPU bracket.
    std::io::stdin().read_exact(&mut [0_u8; 1])?;
    // Preserve tail completions separately, never inflate timed throughput.
    let mut results = Vec::with_capacity(workers * 2);
    while let Some(result) = tasks.join_next().await {
        results.push(result??);
    }
    let mut rows = Vec::with_capacity(workers);
    for role in 0..workers {
        let mut matching = results.iter_mut().filter(|result| result.role == role);
        let first = matching.next().ok_or("missing first client")?;
        let second = matching.next().ok_or("missing second client")?;
        let capped = first.capped || second.capped;
        let tail_requests = first.tail_requests + second.tail_requests;
        let skipped_arrivals = first.skipped_arrivals + second.skipped_arrivals;
        // Merge raw samples, not two client percentiles. This happens after END.
        first.latency.append(&mut second.latency);
        let mut row = serde_json::json!({"role": role, "latency": summarize(&mut first.latency),
            "capped": capped, "tail_requests": tail_requests});
        if fixed_rate {
            first
                .scheduled_latency
                .append(&mut second.scheduled_latency);
            first.scheduling_delay.append(&mut second.scheduling_delay);
            row["scheduled_arrivals"] = (2 * ArrivalPlan::SLOTS).into();
            row["skipped_arrivals"] = skipped_arrivals.into();
            row["scheduled_latency"] =
                serde_json::to_value(summarize(&mut first.scheduled_latency))?;
            row["scheduling_delay"] = serde_json::to_value(summarize(&mut first.scheduling_delay))?;
        }
        rows.push(row);
    }
    println!(
        "{}",
        serde_json::json!({"schema_version": if fixed_rate { 3 } else { 2 },
            "arrival_profile": if fixed_rate { "fixed_500_per_worker_v1" } else { "saturation_v2" },
            "seconds": WINDOW.as_secs(), "workers": rows})
    );
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Error> {
    if cfg!(debug_assertions) {
        return Err("measurement requires --release".into());
    }
    tokio::time::timeout(Duration::from_secs(180), async {
        match Args::parse().command {
            Mode::Load { config, fixed_rate } => load(config, fixed_rate).await,
            Mode::Collect { workers } => collector::run(workers).await,
            Mode::Observe { root, workers } => observer::run(root, workers).await,
        }
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrival_plan_staggers_bounds_and_skips_without_catchup() {
        for clients in [6, 20, 60] {
            for index in 0..clients {
                let plan = ArrivalPlan::new(index, clients);
                assert!(plan.due(ArrivalPlan::SLOTS - 1) < WINDOW);
                assert_eq!(plan.next_slot(plan.due(17), 17), 17);
                assert_eq!(plan.next_slot(plan.due(20), 17), 20);
                assert_eq!(plan.next_slot(plan.due(19), 20), 20);
                assert_eq!(
                    plan.next_slot(WINDOW + ArrivalPlan::PERIOD, 0),
                    ArrivalPlan::SLOTS
                );
            }
        }
        assert_eq!(2 * ArrivalPlan::SLOTS, 500 * WINDOW.as_secs() as usize);
    }
    #[test]
    fn percentiles_use_raw_samples_and_preserve_empty() {
        let result = summarize(&mut [1000, 3000, 2000, 9000]);
        assert_eq!(result.requests, 4);
        assert_eq!(result.median_us, Some(3.0));
        assert_eq!(result.p95_us, Some(3.0));
        assert!(summarize(&mut []).p95_us.is_none());
    }
}
