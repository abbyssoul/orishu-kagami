mod formatter;

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use formatter::Formatter;
use orishu::client::http_client::{HttClientOptions, HttpClusterClient};
use orishu::client::{
    BlocklistApi, CheckpointApi, ClientApi, ClusterAddress, ClusterApi, GraveyardApi,
    MembershipApi, ResultsApi, WorkloadApi,
};
use orishu::model::audit::AuditFilter;
use orishu::model::blocklist::{
    self, BlocklistAddRequest, BlocklistIdentityMatcher, BlocklistNetworkMatcher,
};
use orishu::model::checkpoint::{self, CheckpointId};
use orishu::model::cluster::{
    EventFilter, JoinIntent, JoinToken, LogFilter, LogLevel, MembersSelector,
};
use orishu::model::node::{InspectSource, MemberState, NodeId, RemoveMode};
use orishu::model::result::{self, ResultId};
use orishu::model::tombstones;
use orishu::model::workload::{
    self, SimulationFrame, SimulationStateTransitionMode, WorkloadStatus,
};

// ── CLI root ──────────────────────────────────────────────────────────────────

/// orishu-ctl: command-line tool for administering an orishu cluster.
#[derive(Debug, Parser)]
#[command(name = "orishuctl")]
#[command(about = "Command-line tool for administering an orishu cluster", long_about = None)]
struct Cli {
    /// Cluster address: a Unix socket path, IP:port, or hostname[:port].
    /// Defaults to `$XDG_RUNTIME_DIR/orishu/worker.sock`.
    #[arg(
        short = 'H',
        long,
        global = true,
        value_parser = ClusterAddress::parse,
        env = "ORISHU_HOST"
    )]
    host: Option<ClusterAddress>,

    /// Output format.
    #[arg(short, long, global = true, value_enum, default_value_t = OutputMode::Table)]
    output: OutputMode,

    /// Request timeout (e.g. 5s, 1min).
    #[arg(long, global = true, value_parser = humantime::parse_duration, default_value = "5s")]
    timeout: Duration,

    /// Path to TLS certificate PEM file (enables TLS on TCP listeners).
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// Path to TLS private key PEM file.
    #[arg(long)]
    tls_key: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

// ── Shared option types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, ValueEnum)]
enum OutputMode {
    Json,
    Yaml,
    Table,
}

impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputMode::Json => write!(f, "json"),
            OutputMode::Yaml => write!(f, "yaml"),
            OutputMode::Table => write!(f, "table"),
        }
    }
}

/// CLI-level log level filter. Kept separate from the library type so that clap
/// (and its `ValueEnum` derive) does not become a dependency of the `orishu` crate.
#[derive(Debug, Clone, ValueEnum)]
enum LogLevelArg {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevelArg> for LogLevel {
    fn from(l: LogLevelArg) -> Self {
        match l {
            LogLevelArg::Trace => LogLevel::Trace,
            LogLevelArg::Debug => LogLevel::Debug,
            LogLevelArg::Info => LogLevel::Info,
            LogLevelArg::Warn => LogLevel::Warn,
            LogLevelArg::Error => LogLevel::Error,
        }
    }
}

/// Controls where node manifest data is fetched from during `inspect`.
#[derive(Debug, Clone, ValueEnum)]
enum InspectSourceArg {
    /// Fetch directly from the target node.
    Direct,
    /// Return from another node's gossip-based view.
    Indirect,
    /// Try direct first, fall back to gossip-cached if unreachable (default).
    BestEffort,
}

impl From<InspectSourceArg> for InspectSource {
    fn from(s: InspectSourceArg) -> Self {
        match s {
            InspectSourceArg::Direct => InspectSource::Direct,
            InspectSourceArg::Indirect => InspectSource::Indirect,
            InspectSourceArg::BestEffort => InspectSource::BestEffort,
        }
    }
}

/// CLI wrapper for tombstone removal mode filter.
#[derive(Debug, Clone, ValueEnum)]
enum RemovalModeArg {
    Graceful,
    Force,
    Dead,
    Blocklist,
}

impl From<RemovalModeArg> for orishu::model::tombstones::RemovalMode {
    fn from(m: RemovalModeArg) -> Self {
        match m {
            RemovalModeArg::Graceful => Self::Graceful,
            RemovalModeArg::Force => Self::Force,
            RemovalModeArg::Dead => Self::Dead,
            RemovalModeArg::Blocklist => Self::Blocklist,
        }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid RFC 3339 timestamp: {e}"))
}

const AFTER_TIME_HELP: &str = "Limit to items at or after this RFC 3339 timestamp.";
const BEFORE_TIME_HELP: &str = "Limit to items before this RFC 3339 timestamp.";
const AFTER_TIME_LONG_HELP: &str = "Limit to items at or after this RFC 3339 timestamp.\n\nUse either --after or --before alone for an open-ended time range.";
const BEFORE_TIME_LONG_HELP: &str = "Limit to items before this RFC 3339 timestamp.\n\nUse either --after or --before alone for an open-ended time range.";

// ── Command tree ──────────────────────────────────────────────────────────────

#[derive(Debug, Subcommand)]
enum Commands {
    // ── Workload subcommand group ─────────────────────────────────────────────
    /// Run a simulation from a workload manifest file (JSON or YAML).
    ///
    /// Use `-` to read the manifest from stdin.
    Run {
        /// Path to the workload manifest file, or `-` for stdin.
        source: String,
    },

    /// Manage workload lifecycle and runtime state.
    Workload {
        #[command(subcommand)]
        command: WorkloadCommands,
    },

    // ── Results operations ───────────────────────────────────────────────────────
    /// Manage stored simulation result artifacts.
    Results {
        #[command(subcommand)]
        command: ResultsCommands,
    },

    // ── Checkpoints operations ────────────────────────────────────────────────────
    /// Manage simulation checkpoints.
    Checkpoints {
        #[command(subcommand)]
        command: CheckpointsCommands,
    },

    // ── Cluster subcommand group ──────────────────────────────────────────────
    /// Cluster-level operations: summary, membership lock/unlock.
    Cluster {
        #[command(subcommand)]
        command: ClusterCommands,
    },

    // ── Blocklist subcommand group ────────────────────────────────────────────
    /// Manage the cluster blocklist (identities and network ranges).
    Blocklist {
        #[command(subcommand)]
        command: BlocklistCommands,
    },

    // ── Tombstones subcommand group ───────────────────────────────────────────
    /// Manage removed node tombstones (graveyard).
    Tombstones {
        #[command(subcommand)]
        command: TombstoneCommands,
    },

    // ── Token subcommand group ───────────────────────────────────────────────
    /// Obtain and manage cluster join tokens.
    Token {
        /// Rotate the cluster join token.
        ///
        /// Invalidates the current token and generates a new one.
        #[arg(long)]
        rotate: bool,
    },

    // ── Node operations ───────────────────────────────────────────────────────
    /// List all cluster nodes with identity, version, advertised participation flags, and health.
    Ls {
        /// Filter by membership state: Alive, Suspected, Dead, Removed.
        #[arg(long)]
        state: Option<String>,
        /// Filter by node name.
        #[arg(long)]
        name: Option<String>,
        /// Filter by capability role: introducer or worker.
        #[arg(long)]
        role: Option<String>,
    },

    /// Show detailed information for a specific node.
    Inspect {
        node_id: NodeId,
        /// Control where the node manifest is fetched from.
        #[arg(long, value_enum, default_value_t = InspectSourceArg::BestEffort)]
        source: InspectSourceArg,
    },

    /// Run a connectivity and health diagnostic against a node.
    ///
    /// Without --from: direct check from this client's connected node to TARGET.
    /// With --from: asks SOURCE to produce the diagnostic report for TARGET —
    /// useful for diagnosing network partitions and asymmetric routing.
    Diagnose {
        node_id: NodeId,
        /// Ask this node to perform the check instead (indirect diagnostic).
        #[arg(long)]
        from: Option<NodeId>,
    },

    /// Remove a node from the cluster.
    ///
    /// By default the node finishes in-flight work and transfers result data
    /// to replicas before exiting. Use --force to drop it immediately and
    /// instruct peers to reject its pending data.
    ///
    /// Requires Tier 2 credentials.
    Rm {
        node_id: NodeId,
        /// Drop the node immediately without draining work or transferring results.
        /// Peers will reject any pending data from this node.
        #[arg(long)]
        force: bool,
    },

    /// Command the local worker to join an existing cluster.
    ///
    /// Provide an introducer address and a join token obtained from the target cluster.
    /// If the worker is already in a cluster, it will leave first and then join the new one.
    Join {
        /// Address of an introducer node (host:port).
        address: String,
        /// Join token obtained from the target cluster.
        #[arg(long)]
        token: String,
    },

    /// Command the local worker to leave its current cluster.
    ///
    /// The node gracefully leaves (drains in-flight work, transfers data to
    /// replicas, announces Leave to peers) and becomes a standalone cluster of one.
    Leave,

    // ── Observability ─────────────────────────────────────────────────────────
    /// Review recent administrative events with actor, target, and outcome.
    ///
    /// Requires Tier 2 credentials.
    Audit {
        /// Filter by event type name.
        #[arg(long = "type")]
        event_type: Option<String>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
    },

    /// Show recent logs from the cluster.
    Logs {
        /// Filter by minimum log level.
        #[arg(long)]
        level: Option<LogLevelArg>,

        /// Filter by components.
        #[arg(long)]
        component: Option<String>,

        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,

        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
    },

    /// List recent cluster events (node joins/leaves, workload transitions, etc.).
    Events {
        /// Filter by event type name.
        #[arg(long = "type")]
        event_type: Option<String>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
    },

    /// Print client and library version.
    Version,
}

#[derive(Debug, Subcommand)]
enum ClusterCommands {
    /// Cluster-level summary: node count, membership lock state, workload phase.
    Info,

    /// Lock cluster membership, preventing new nodes from joining.
    ///
    /// Already-connected nodes are unaffected. Idempotent.
    /// Requires Tier 2 credentials.
    Lock,

    /// Unlock cluster membership, re-enabling new node admission.
    ///
    /// Idempotent. Requires Tier 2 credentials.
    Unlock,
}

#[derive(Debug, Subcommand)]
enum BlocklistCommands {
    /// List all current blocklist entries.
    Ls {
        /// Filter by cluster-assigned node ID.
        #[arg(long = "id")]
        ids: Vec<String>,

        /// Filter by worker name.
        #[arg(long = "name")]
        names: Vec<String>,

        /// Filter by certificate fingerprint.
        #[arg(long = "cert")]
        certs: Vec<String>,

        /// Filter by host IP.
        #[arg(long = "hostname")]
        hosts: Vec<String>,

        /// Filter by CIDR range.
        #[arg(long = "cidr")]
        cidrs: Vec<String>,

        /// Filter by who added the entry.
        #[arg(long = "added-by")]
        added_by: Vec<String>,

        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
    },

    /// Add a blocklist entry. Identity and network dimensions can be combined.
    ///
    /// At least one flag must be provided. Identity flags (--id, --name, --cert)
    /// are mutually exclusive. Network flags (--host, --cidr) are mutually exclusive.
    ///
    /// If the target matches a connected node and membership is not locked, the node
    /// is immediately disconnected. Requires Tier 2 credentials.
    Add {
        /// Block by cluster-assigned node ID (UUID).
        #[arg(long, group = "identity")]
        id: Option<NodeId>,
        /// Block by worker name.
        #[arg(long, group = "identity")]
        name: Option<String>,
        /// Block by mTLS certificate fingerprint.
        #[arg(long, group = "identity")]
        cert: Option<String>,
        /// Block a specific host IP.
        #[arg(long, group = "network")]
        hostname: Option<String>,
        /// Block a CIDR range (e.g. 10.0.0.0/8).
        #[arg(long, group = "network")]
        cidr: Option<String>,
    },

    /// Remove a blocklist entry by its entry ID (shown in `blocklist ls` output).
    ///
    /// Does not itself re-admit the node; it must go through the normal admission flow.
    /// Requires Tier 2 credentials.
    Rm {
        /// Blocklist entry ID (from `blocklist ls` output).
        entry_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TombstoneCommands {
    /// List retained tombstones for removed nodes.
    Ls {
        /// Filter by exact node name.
        #[arg(long)]
        name: Option<String>,

        /// Filter by removal mode (graceful, force, dead, blocklist).
        #[arg(long = "mode")]
        removal_mode: Vec<RemovalModeArg>,

        /// Filter by operator identity that removed the node.
        #[arg(long = "removed-by")]
        removed_by: Vec<String>,

        /// Filter entries after this time (RFC3339).
        #[arg(long, value_parser = parse_datetime, help = AFTER_TIME_HELP)]
        after: Option<DateTime<Utc>>,

        /// Filter entries before this time (RFC3339).
        #[arg(long, value_parser = parse_datetime, help = BEFORE_TIME_HELP)]
        before: Option<DateTime<Utc>>,
    },
    /// Clear a tombstone, allowing a previously removed node to attempt re-joining.
    Rm {
        /// The cluster-assigned node ID to clear.
        node_id: NodeId,
    },
}

#[derive(Debug, Subcommand)]
enum WorkloadCommands {
    /// Load a workload manifest into the cluster.
    ///
    /// SOURCE is a path to a workload manifest file (JSON or YAML), or `-` to read from stdin.
    Load {
        /// Path to the workload manifest file, or `-` for stdin.
        source: String,
    },

    /// Unload the current workload definition from the cluster.
    Unload {
        /// Force unload even if a simulation is currently running with this workload. Workers will halt immediately without saving a consistent state snapshot.
        #[arg(long)]
        force: bool,
    },

    /// Check workload compatibility against cluster nodes without loading it.
    ///
    /// Evaluates the referenced manifest against every node and reports
    /// which nodes can satisfy the workload's requirements (hardware,
    /// runtime ABI version, and execution profile) and artifact trust policy.
    /// This is a read-only operation with no side effects on the cluster.
    Check { source: String },

    /// Command the cluster to begin or resume simulation. Idempotent if already running.
    ///
    /// By default the cluster waits for all eligible nodes to report Ready
    /// before starting computation simultaneously. Use --force to start
    /// immediately: each node begins as soon as it has loaded the workload.
    /// Late-joining nodes synchronize from a catch-up snapshot.
    ///
    /// Use --resume to reset the simulation to a checkpoint first, then issue
    /// the normal start request.
    Start {
        /// Start immediately without waiting for all nodes to be ready.
        /// Each node begins computation as soon as it loads the workload;
        /// late-joining nodes synchronize from a catch-up snapshot.
        #[arg(long)]
        force: bool,
        /// Resume from a checkpoint by resetting to the given checkpoint ID first,
        /// then starting the simulation.
        #[arg(long)]
        resume: Option<CheckpointId>,
        /// Checkpoint every N steps during the run.
        #[arg(long)]
        checkpoint: Option<u64>,
    },

    /// Command the cluster to advance a simulation by a specific number of steps.
    Step {
        /// Number of steps to advance the simulation state by.
        #[arg(short = 'n', long = "steps", default_value = "1")]
        number_steps: u128,

        /// Checkpoint every N steps during stepped execution.
        #[arg(long)]
        checkpoint: Option<u64>,
    },

    /// Stop the simulation.
    ///
    /// By default workers complete their current time step and save a
    /// resumable checkpoint plus a result artifact before halting.
    Stop {
        /// Halt immediately without saving a consistent state snapshot.
        #[arg(long)]
        force: bool,
    },

    /// Reset simulation state to a previously recorded checkpoint.
    ///
    /// The simulation must be in Ready or Stopped state. After resetting,
    /// the simulation remains in Stopped state — use `start` or `step` to proceed.
    Reset {
        /// Checkpoint ID to reset to.
        checkpoint_id: CheckpointId,
    },

    /// Show current simulation phase, time, convergence metrics, and partition map.
    Status,

    /// Connect to a loaded simulation and receive a live stream of state updates.
    Stream,
}

#[derive(Debug, Subcommand)]
enum CheckpointsCommands {
    /// List stored checkpoints.
    Ls {
        /// Filter by workload definition ID.
        #[arg(long)]
        workload_id: Option<String>,
        /// Filter by workload name (matches all runs of that workload).
        #[arg(long)]
        workload_name: Option<String>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
        /// Filter by resumability (true = only resumable, false = only non-resumable).
        #[arg(long)]
        resumable: Option<bool>,
    },

    /// Get checkpoint metadata, or download checkpoint data with --download.
    /// If the argument is omitted or `-` the artifact is written to stdout!
    Get {
        /// Checkpoint ID.
        checkpoint_id: CheckpointId,
        /// Download checkpoint data instead of displaying metadata.
        /// Use "-" to write to stdout.
        #[arg(long, short = 'O')]
        download: Option<PathBuf>,
    },

    /// Permanently delete given checkpoint. Produces an audit event.
    ///
    /// Requires Tier 2 credentials.
    Rm { checkpoint_id: CheckpointId },

    /// Bulk delete checkpoints matching the given criteria.
    ///
    /// Produces one audit event per deleted checkpoint. Requires Tier 2 credentials.
    Purge {
        /// Delete only checkpoints from this workload definition ID.
        #[arg(long)]
        workload_id: Option<String>,
        /// Delete only checkpoints from this workload name.
        #[arg(long)]
        workload_name: Option<String>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
        /// Filter by resumability (true = only resumable, false = only non-resumable).
        #[arg(long)]
        resumable: Option<bool>,
    },
}

#[derive(Debug, Subcommand)]
enum ResultsCommands {
    /// List stored result artifacts in reverse-chronological order.
    Ls {
        /// Filter by workload definition ID.
        #[arg(long)]
        workload_id: Option<String>,
        /// Filter by workload name (matches all runs of that workload).
        #[arg(long)]
        workload_name: Option<String>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
    },

    /// Retrieve result metadata, or download result data with --download.
    /// If the argument is omitted or `-` the artifact is written to stdout!
    Get {
        result_id: ResultId,
        /// Download result data instead of displaying metadata.
        /// Use "-" to write to stdout.
        #[arg(long, short = 'O')]
        download: Option<PathBuf>,
    },

    /// Permanently delete a result artifact. Produces an audit event.
    ///
    /// Requires Tier 2 credentials.
    Rm { result_id: ResultId },

    /// Bulk delete result artifacts matching the given criteria.
    ///
    /// Produces one audit event per deleted artifact. Requires Tier 2 credentials.
    Purge {
        /// Delete only results from this workload definition ID.
        #[arg(long)]
        workload_id: Option<String>,
        /// Delete only results from this workload name.
        #[arg(long)]
        workload_name: Option<String>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = AFTER_TIME_HELP,
            long_help = AFTER_TIME_LONG_HELP
        )]
        after: Option<DateTime<Utc>>,
        #[arg(
            long,
            value_parser = parse_datetime,
            help = BEFORE_TIME_HELP,
            long_help = BEFORE_TIME_LONG_HELP
        )]
        before: Option<DateTime<Utc>>,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let fmt = Formatter::new(args.output);

    let host = args.host.unwrap_or_default();

    let client_result = HttpClusterClient::new(
        host,
        HttClientOptions {
            credentials: None,
            timeout: Some(args.timeout),
            tls_cert: args.tls_cert,
            tls_key: args.tls_key,
        },
    );
    if let Err(e) = client_result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }

    let client = client_result.unwrap();
    if let Err(e) = run(args.command, &client, &fmt).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ── Manifest loading ─────────────────────────────────────────────────────────

fn load_manifest(source: &str) -> Result<workload::Manifest, Box<dyn std::error::Error>> {
    let label = if source == "-" { "<stdin>" } else { source };
    let manifest = if source == "-" {
        let reader = std::io::BufReader::new(std::io::stdin().lock());
        workload::Manifest::from_reader(reader)
    } else {
        let file = std::fs::File::open(source)
            .map_err(|e| format!("failed to open manifest file '{source}': {e}"))?;
        let reader = std::io::BufReader::new(file);
        workload::Manifest::from_reader(reader)
    }
    .map_err(|e| format!("failed to parse manifest '{label}': {e}"))?;
    Ok(manifest)
}

// ── Workload status formatting ────────────────────────────────────────────────

fn format_manifest_metadata(m: &orishu::model::manifest::ObjectMeta) -> Value {
    let mut v = json!({
        "name": m.name,
        "namespace": m.namespace.clone().unwrap_or("".to_string()),
        "labels": m.labels,
    });

    if let Some(namespace) = m.namespace.as_ref() {
        v.as_object_mut()
            .unwrap()
            .insert("namespace".into(), namespace.as_str().into());
    }

    v
}

fn format_workload_status(ws: Option<&WorkloadStatus>) -> Value {
    match ws {
        Some(s) => {
            match s {
                WorkloadStatus::NotLoaded => json!({ "phase": "NotLoaded" }),

                WorkloadStatus::Loading => json!({ "phase": "Loading" }),

                WorkloadStatus::Ready => json!({ "phase": "Ready" }),

                WorkloadStatus::Running {
                    // started_at,
                    simulation_time,
                    convergence_metrics,
                    partition_map,
                } => json!({
                    "phase": "Running",
                    // "started_at": started_at.to_rfc3339(),
                    "simulation_time": simulation_time,
                    "convergence_metrics": convergence_metrics,
                    "partitions": partition_map,
                }),

                WorkloadStatus::Stopped {
                    simulation_time,
                    convergence_metrics,
                    partition_map,
                } => json!({
                    "phase": "Stopped",
                    "simulation_time": simulation_time,
                    "convergence_metrics": convergence_metrics,
                    "partitions": partition_map,
                }),

                WorkloadStatus::Error { message } => {
                    json!({
                        "phase": "Error",
                        "error": message,
                    })
                }
            }
        }
        None => json!({ "phase": "unknown" }),
    }
}

// ── Command dispatch ──────────────────────────────────────────────────────────

async fn run(
    command: Commands,
    client: &impl ClientApi,
    fmt: &Formatter,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        // ── Version ───────────────────────────────────────────────────────────
        Commands::Version => {
            fmt.message(&format!("orishuctl {}", orishu::library_version()));
        }

        // ── Node operations ───────────────────────────────────────────────────
        Commands::Ls { state, name, role } => {
            let nodes = client
                .membership()
                .list(&MembersSelector {
                    member_state: state,
                    name,
                    role,
                })
                .await?;
            let values: Vec<Value> = nodes
                .iter()
                .map(|n| {
                    json!({
                        "id": n.metadata.name,
                        "state": format!("{:?}", n.status.as_ref().map_or(&MemberState::Removed, |s| &s.member)),
                    })
                })
                .collect();
            fmt.list(values);
        }

        Commands::Inspect { node_id, source } => {
            let inspect_source: InspectSource = source.into();
            let node = client
                .membership()
                .get(&node_id, Some(inspect_source))
                .await?;
            fmt.record(json!({
                "id": node.metadata.name,
                "host": node.spec.listen.clients,
                "state": format!("{:?}", node.status.as_ref().map_or(&MemberState::Removed, |s| &s.member)),
                "accepts.clients": node.spec.accepts.clients,
                "accepts.peers": node.spec.accepts.peers,
                "accepts.work": node.spec.accepts.work,
                "limits.clients": node.spec.limits.clients.unwrap_or_default(),
                "limits.peers": node.spec.limits.peers.unwrap_or_default(),
                "connected.peers": node.status.as_ref().map_or(0, |s| s.connected.peers),
                "connected.clients": node.status.as_ref().map_or(0, |s| s.connected.clients),
            }));
        }

        Commands::Diagnose { node_id, from } => {
            let result = client.diagnose_node(node_id, from).await?;
            fmt.record(json!({
                "target": result.target_id.to_string(),
                "source": result.source_id.as_ref().map_or_else(String::new, |id| id.to_string()),
                "reachable": result.reachable,
                "latency_ms": result.latency_ms,
                "details": result.details,
            }));
        }

        Commands::Rm { node_id, force } => {
            let mode = if force {
                RemoveMode::Force
            } else {
                RemoveMode::Graceful
            };
            client.membership().remove(&node_id, &mode).await?;
            if force {
                fmt.message(&format!("Node {} force-removed.", node_id));
            } else {
                fmt.message(&format!("Node {} removal initiated (graceful).", node_id));
            }
        }

        // ── Join / Leave ─────────────────────────────────────────────────────
        Commands::Join { address, token } => {
            let intent = JoinIntent {
                addresses: vec![ClusterAddress::parse(&address)?],
                token: JoinToken {
                    token,
                    expires_at: None,
                },
            };
            let result = client.membership().join(&intent).await?;
            fmt.record(json!({
                "nodeId": result.node_id.to_string(),
                "admittedBy": result.admitted_by.to_string(),
                "cluster": result.cluster.metadata.name,
            }));
        }

        Commands::Leave => {
            let result = client.membership().leave().await?;
            fmt.record(json!({
                "previousCluster": result.previous_cluster,
                "previousNodeId": result.previous_node_id,
            }));
        }

        // ── Cluster subcommands ───────────────────────────────────────────────
        Commands::Cluster { command } => match command {
            ClusterCommands::Info => {
                let s = client.cluster().summary().await?;
                fmt.record(json!({
                    "nodes": s.status.map_or(0, |status| status.node_count),
                    "locked": s.spec.membership_locked,
                    "workload": if let Some(workload) = s.spec.workload {
                        json!({
                            "meta": format_manifest_metadata(&workload.metadata),
                            "status": format_workload_status(workload.status.as_ref())
                        })
                    } else { format_workload_status(Some(&WorkloadStatus::NotLoaded)) },
                }));
            }
            ClusterCommands::Lock => {
                client.membership().lock().await?;
                fmt.message("Cluster membership locked.");
            }
            ClusterCommands::Unlock => {
                client.membership().unlock().await?;
                fmt.message("Cluster membership unlocked.");
            }
        },

        // ── Token subcommands ────────────────────────────────────────────────
        Commands::Token { rotate } => {
            if rotate {
                let token = client.create_join_token().await?;
                fmt.message("Join token successfully rotated.");
                fmt.record(json!({
                    "token": token.token,
                    "expiresAt": token.expires_at.map(|t| t.to_rfc3339()),
                }));
            } else {
                let token = client.get_join_token().await?;
                fmt.record(json!({
                    "token": token.token,
                    "expiresAt": token.expires_at.map(|t| t.to_rfc3339()),
                }));
            }
        }

        // ── Blocklist subcommands ─────────────────────────────────────────────
        Commands::Blocklist { command } => {
            match command {
                BlocklistCommands::Ls {
                    ids,
                    names,
                    certs,
                    hosts,
                    cidrs,
                    added_by,
                    after,
                    before,
                } => {
                    let filter = blocklist::Filter {
                        ids,
                        names,
                        certs,
                        hosts,
                        cidrs,
                        after,
                        before,
                        added_by,
                    };

                    let entries = client.blocklist().list(&filter).await?;
                    let values: Vec<Value> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "entry_id": e.entry_id,
                            "identity": e.identity.as_ref().map(|i| match i {
                                BlocklistIdentityMatcher::Id(v) => format!("id:{}", v),
                                BlocklistIdentityMatcher::Name(v) => format!("name:{}", v),
                                BlocklistIdentityMatcher::CertFingerprint(v) => format!("cert:{}", v),
                            }),
                            "network": e.network.as_ref().map(|n| match n {
                                BlocklistNetworkMatcher::Host(v) => format!("host:{}", v),
                                BlocklistNetworkMatcher::Cidr(v) => format!("cidr:{}", v),
                            }),
                            "added_at": e.added_at.to_string(),
                            "added_by": e.added_by,
                        })
                    })
                    .collect();
                    fmt.list(values);
                }
                BlocklistCommands::Add {
                    id,
                    name,
                    cert,
                    hostname,
                    cidr,
                } => {
                    let identity = id
                        .map(BlocklistIdentityMatcher::Id)
                        .or(name.map(BlocklistIdentityMatcher::Name))
                        .or(cert.map(BlocklistIdentityMatcher::CertFingerprint));

                    let network = hostname
                        .map(BlocklistNetworkMatcher::Host)
                        .or(cidr.map(BlocklistNetworkMatcher::Cidr));

                    if identity.is_none() && network.is_none() {
                        return Err("at least one of --id, --name, --cert, --host, or --cidr must be provided".into());
                    }

                    let req = BlocklistAddRequest { identity, network };
                    let result = client.blocklist().add(&req).await?;
                    fmt.record(json!({
                        "entry_id": result.entry.entry_id,
                        "effect": format!("{:?}", result.effect),
                    }));
                }
                BlocklistCommands::Rm { entry_id } => {
                    client.blocklist().remove(&entry_id).await?;
                    fmt.message(&format!("Blocklist entry {} removed.", entry_id));
                }
            }
        }

        // ── Workload subcommands ──────────────────────────────────────────────
        Commands::Run { source } => {
            let manifest = load_manifest(&source)?;
            client.workload().load(&manifest).await?;
            client
                .workload()
                .start(&SimulationStateTransitionMode::WaitForAll, None)
                .await?;
            fmt.message("Workload loaded and simulation started.");
        }

        // ── Tombstones subcommands ────────────────────────────────────────────
        Commands::Tombstones { command } => match command {
            TombstoneCommands::Ls {
                name,
                removal_mode,
                removed_by,
                after,
                before,
            } => {
                let parsed_modes: Vec<_> = removal_mode.into_iter().map(Into::into).collect();

                let filter = tombstones::TombstoneFilter {
                    name,
                    removal_mode: parsed_modes,
                    removed_by,
                    after,
                    before,
                    cursor: None,
                    limit: None,
                };
                let entries = client.graveyard().list(&filter).await?;
                if entries.is_empty() {
                    fmt.message("No tombstones found.");
                } else {
                    let values: Vec<Value> = entries
                        .iter()
                        .map(|e| {
                            json!({
                                "node_id": e.node_id.to_string(),
                                "name": e.name,
                                "cert_fingerprint": e.cert_fingerprint.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
                                "removed_at": e.removed_at.to_rfc3339(),
                                "removed_by": e.removed_by,
                                "removal_mode": format!("{:?}", e.removal_mode),
                                "reason": e.reason,
                            })
                        })
                        .collect();
                    fmt.list(values);
                }
            }
            TombstoneCommands::Rm { node_id } => {
                client.graveyard().clear(&node_id).await?;
                fmt.message(&format!("Tombstone for node {} cleared.", node_id));
            }
        },

        Commands::Workload { command } => match command {
            WorkloadCommands::Load { source } => {
                let manifest = load_manifest(&source)?;
                client.workload().load(&manifest).await?;
                fmt.message("Workload loaded.");
            }
            WorkloadCommands::Unload { force } => {
                let mode = if force {
                    SimulationStateTransitionMode::Immediate
                } else {
                    SimulationStateTransitionMode::WaitForAll
                };
                client.workload().unload(&mode).await?;
                fmt.message("Workload unloaded.");
            }
            WorkloadCommands::Check { source } => {
                let manifest = load_manifest(&source)?;

                // TODO: implement workload compatibility check
                // - Fetch and parse the manifest from `source`
                // - Display per-node eligibility report

                let report = client.workload().check(&manifest).await?;

                fmt.message(&format!(
                    "Nodes capable to accept the workload: '{}'",
                    report.nodes_compatible
                ));
            }
            WorkloadCommands::Start {
                force,
                resume,
                checkpoint,
            } => {
                if let Some(checkpoint_id) = resume {
                    client.workload().reset(checkpoint_id).await?;
                }
                let mode = if force {
                    SimulationStateTransitionMode::Immediate
                } else {
                    SimulationStateTransitionMode::WaitForAll
                };
                client.workload().start(&mode, checkpoint).await?;
                if force {
                    fmt.message("Simulation started (immediate; not waiting for all nodes).");
                } else {
                    fmt.message("Simulation started.");
                }
            }
            WorkloadCommands::Step {
                number_steps,
                checkpoint,
            } => {
                client.workload().step(number_steps, checkpoint).await?;
                let cp_msg = match checkpoint {
                    Some(n) => format!(", checkpoint every {} step(s)", n),
                    None => String::new(),
                };
                fmt.message(&format!("Advancing for {} step(s){}", number_steps, cp_msg));
            }
            WorkloadCommands::Stop { force } => {
                let mode = if force {
                    SimulationStateTransitionMode::Immediate
                } else {
                    SimulationStateTransitionMode::WaitForAll
                };
                client.workload().stop(&mode).await?;
                if force {
                    fmt.message("Simulation force-stopped.");
                } else {
                    fmt.message("Simulation stopped (graceful).");
                }
            }
            WorkloadCommands::Reset { checkpoint_id } => {
                client.workload().reset(checkpoint_id).await?;
                fmt.message("Simulation state reset to checkpoint.");
            }
            WorkloadCommands::Status => {
                let s = client.workload().get().await?;
                fmt.record(s.map_or(json!({ "phase": "NotLoaded" }), |s| {
                    format_workload_status(s.status.as_ref())
                }))
            }
            WorkloadCommands::Stream => {
                let mut stream = client.workload().stream().await?;
                while let Some(frame) = stream.next().await {
                    match frame? {
                        SimulationFrame::Snapshot(s) => {
                            let mut v = format_workload_status(Some(&s));
                            v.as_object_mut()
                                .unwrap()
                                .insert("type".into(), json!("snapshot"));
                            fmt.record(v);
                        }
                        SimulationFrame::Delta {
                            simulation_time, ..
                        } => {
                            fmt.record(json!({
                                "type": "delta",
                                "simulation_time": simulation_time,
                            }));
                        }
                        SimulationFrame::EndOfStream { reason } => {
                            fmt.record(json!({
                                "type": "end",
                                "reason": format!("{:?}", reason),
                            }));
                            break;
                        }
                    }
                }
            }
        },

        // ── Checkpoint subcommands ──────────────────────────────────────────
        Commands::Checkpoints { command } => match command {
            CheckpointsCommands::Ls {
                workload_id,
                workload_name,
                after,
                before,
                resumable,
            } => {
                let filter = checkpoint::Filter {
                    workload_id,
                    workload_name,
                    after,
                    before,
                    resumable,
                };
                let checkpoints = client.checkpoints().list(&filter).await?;
                let values: Vec<Value> = checkpoints
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id.to_string(),
                            "workload_id": c.workload_id,
                            "epoch": c.workload_epoch,
                            "recorded_at": c.recorded_at.to_string(),
                            "simulation_time": c.simulation_time,
                            "graceful": c.graceful,
                            "resumable": c.resumable,
                        })
                    })
                    .collect();
                fmt.list(values);
            }
            CheckpointsCommands::Get {
                checkpoint_id,
                download,
            } => {
                if let Some(path) = download {
                    if &path != "-" {
                        let mut file = tokio::fs::File::create(&path).await?;
                        client
                            .checkpoints()
                            .download(&checkpoint_id, &mut file)
                            .await?;
                        fmt.message(&format!("Checkpoint saved to {}.", path.display()));
                    } else {
                        let mut stdout = tokio::io::stdout();
                        client
                            .checkpoints()
                            .download(&checkpoint_id, &mut stdout)
                            .await?;
                    }
                } else {
                    let record = client.checkpoints().get(&checkpoint_id).await?;
                    fmt.record(json!({
                        "id": record.id.to_string(),
                        "workload_id": record.workload_id,
                        "epoch": record.workload_epoch,
                        "recorded_at": record.recorded_at.to_string(),
                        "simulation_time": record.simulation_time,
                        "graceful": record.graceful,
                        "resumable": record.resumable,
                    }));
                }
            }
            CheckpointsCommands::Rm { checkpoint_id } => {
                client.checkpoints().delete(&checkpoint_id).await?;
                fmt.message(&format!("Checkpoint {} deleted.", checkpoint_id));
            }
            CheckpointsCommands::Purge {
                workload_id,
                workload_name,
                after,
                before,
                resumable,
            } => {
                let filter = checkpoint::Filter {
                    workload_id,
                    workload_name,
                    after,
                    before,
                    resumable,
                };
                let count = client.checkpoints().purge(&filter).await?;
                fmt.message(&format!("Purged {} checkpoint(s).", count.count));
            }
        },

        // ── Results management subcommands ──────────────────────────────────────────────
        Commands::Results { command } => match command {
            ResultsCommands::Ls {
                workload_id,
                workload_name,
                after,
                before,
            } => {
                let filter = result::Filter {
                    workload_id,
                    workload_name,
                    after,
                    before,
                };
                let results = client.results().list(&filter).await?;
                let values: Vec<Value> = results
                    .iter()
                    .map(|r| {
                        json!({
                            "id": r.id.to_string(),
                            "workload_id": r.workload_id,
                            "recorded_at": r.recorded_at.to_string(),
                            "simulation_time_range": r.simulation_time_range,
                            "size_bytes": r.total_size_bytes,
                            "graceful": r.graceful,
                        })
                    })
                    .collect();
                fmt.list(values);
            }
            ResultsCommands::Get {
                result_id,
                download,
            } => {
                if let Some(path) = download {
                    if &path != "-" {
                        let mut file = tokio::fs::File::create(&path).await?;
                        client.results().download(&result_id, &mut file).await?;
                        fmt.message(&format!("Results saved to {}.", path.display()));
                    } else {
                        let mut stdout = tokio::io::stdout();
                        client.results().download(&result_id, &mut stdout).await?;
                    }
                } else {
                    let record = client.results().get(&result_id).await?;
                    fmt.record(json!({
                        "id": record.id.to_string(),
                        "workload_id": record.workload_id,
                        "started_at": record.started_at.to_string(),
                        "recorded_at": record.recorded_at.to_string(),
                        "simulation_time_range": record.simulation_time_range,
                        "graceful": record.graceful,
                        "size_bytes": record.total_size_bytes,
                    }));
                }
            }
            ResultsCommands::Rm { result_id } => {
                client.results().delete(&result_id).await?;
                fmt.message(&format!("Result {} deleted.", result_id));
            }
            ResultsCommands::Purge {
                workload_id,
                workload_name,
                after,
                before,
            } => {
                let filter = result::Filter {
                    workload_id,
                    workload_name,
                    after,
                    before,
                };
                let count = client.results().purge(&filter).await?;
                fmt.message(&format!("Purged {} result(s).", count.count));
            }
        },

        // ── Observability ─────────────────────────────────────────────────────
        Commands::Logs {
            level,
            component,
            after,
            before,
        } => {
            let filter = LogFilter {
                before,
                after,
                level: level.map(Into::into),
                component: component.clone(),
            };
            let lines = client.cluster().logs(&filter).await?;
            let values: Vec<Value> = lines
                .iter()
                .map(|l| {
                    json!({
                        "timestamp": l.timestamp.to_string(),
                        "level": format!("{:?}", l.level),
                        "component": l.component,
                        "node_id": l.node_id,
                        "message": l.message,
                    })
                })
                .collect();
            fmt.list(values);
        }

        Commands::Events {
            event_type,
            after,
            before,
        } => {
            let filter = EventFilter {
                after,
                before,
                event_types: event_type.into_iter().collect(),
            };
            let events = client.cluster().events(&filter).await?;
            let values: Vec<Value> = events
                .iter()
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp.to_string(),
                        "event_type": e.event_type,
                        "target_id": e.target_id,
                    })
                })
                .collect();
            fmt.list(values);
        }

        Commands::Audit {
            event_type,
            after,
            before,
        } => {
            let filter = AuditFilter {
                after,
                before,
                event_types: event_type.into_iter().collect(),
            };
            let events = client.cluster().audit_log(&filter).await?;
            let values: Vec<Value> = events
                .iter()
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp.to_string(),
                        "TYPE": format!("{:?}", e.event_type),
                        "ACTOR": e.actor,
                        "OBJECT": e.target_id,
                        "outcome": format!("{:?}", e.outcome),
                    })
                })
                .collect();
            fmt.list(values);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
