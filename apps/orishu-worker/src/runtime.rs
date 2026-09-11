//! Worker-owned process lifetime and membership bootstrap.
//!
//! Durable credentials outlive a process; formation state never does. All
//! summaries are derived from the core rather than a parallel membership table.

use orishu::model::cluster::{FormationWorkload, Participation, Summary, SummaryView};
use orishu_membership::{
    Accepts, AdmissionPolicy, Capabilities, ClusterName, FormationId, Limits, LocalIdentity,
    Membership, NodeCapacity, NodeId, ProtocolVersion, WorkerName, model::Endpoints,
};

use crate::credentials::{CredentialError, SecretToken, WorkerCredentials};

/// Startup errors leave existing credential files intact.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// Private identity could not be loaded or generated.
    #[error(transparent)]
    Credential(#[from] CredentialError),
    /// The configured public advertisement is invalid.
    #[error(transparent)]
    Configuration(#[from] orishu_membership::ValidationError),
    /// Peer listener could not be initialized.
    #[error("peer listener startup failed: {0}")]
    Peer(String),
    /// The selected peer interface could not be applied to a peer socket.
    #[error("peer network placement failed: {0}")]
    Placement(#[from] crate::net_placement::PlacementError),
}

/// One worker's authoritative process-local state and exclusive credential lock.
pub struct WorkerRuntime {
    credentials: WorkerCredentials,
    membership: Membership,
    participation: Participation,
    // No API access until admission and operator authorization are wired.
    _join_token: SecretToken,
    peer_endpoint: Option<quinn::Endpoint>,
    /// Interface every peer socket is constrained to, including the outbound
    /// join endpoint created before admission.
    peer_interface: Option<crate::net_placement::InterfaceName>,
    peer_ingress: crate::peer::ingress::IngressMetrics,
    peer_exchanges: crate::peer::exchange_metrics::ExchangeMetrics,
    peer_traffic: crate::peer::traffic::Traffic,
    peer_dials: crate::peer::dial_metrics::DialMetrics,
    formation_metrics: crate::formation_metrics::FormationMetrics,
}

impl WorkerRuntime {
    /// Start a fresh standalone formation, retaining only the certificate and
    /// operator credential. No peers/engines are advertised before their adapters
    /// are running. Reusing a label or state directory cannot restore membership.
    pub fn standalone(
        credentials: WorkerCredentials,
        worker_name: WorkerName,
        cluster_name: ClusterName,
        clients: Vec<String>,
    ) -> Result<Self, StartupError> {
        let formation =
            FormationId::new(SecretToken::generate()?.expose()).expect("hex is valid identity");
        let node_id =
            NodeId::new(SecretToken::generate()?.expose()).expect("hex is valid identity");
        let local = LocalIdentity {
            node_id,
            worker_name,
            cert_fingerprint: credentials.identity.fingerprint(),
            protocol: ProtocolVersion::CURRENT,
            endpoints: Endpoints {
                peers: vec![],
                clients: clients
                    .into_iter()
                    .map(orishu_membership::Address)
                    .collect(),
            },
            accepts: Accepts {
                clients: true,
                peers: false,
                work: false,
            },
            capacity: NodeCapacity {
                peers: 0,
                clients: 128,
            },
            capabilities: Capabilities {
                cpu_cores: std::thread::available_parallelism()
                    .map_or(0, |count| u32::try_from(count.get()).unwrap_or(u32::MAX)),
                architecture: std::env::consts::ARCH.to_owned(),
                // No workload engine/storage is ready in this control-plane slice.
                ..Capabilities::default()
            },
        };
        let membership = Membership::standalone(
            formation,
            cluster_name,
            local,
            AdmissionPolicy {
                accepts_peers: false,
                ..AdmissionPolicy::default()
            },
            Limits::default(),
        )?;
        Ok(Self {
            credentials,
            membership,
            participation: Participation::Standalone,
            _join_token: SecretToken::generate()?,
            peer_endpoint: None,
            peer_interface: None,
            peer_ingress: Default::default(),
            peer_exchanges: Default::default(),
            peer_traffic: Default::default(),
            peer_dials: Default::default(),
            formation_metrics: Default::default(),
        })
    }

    /// Enable bounded handshake, reliable-exchange and traffic counters before startup.
    /// Disabling metrics allocates no counter storage. This enables no listener.
    #[cfg(feature = "observability")]
    pub fn with_peer_metrics(mut self, enabled: bool) -> Self {
        self.peer_ingress = if enabled {
            crate::peer::ingress::IngressMetrics::enabled()
        } else {
            Default::default()
        };
        self.peer_exchanges = if enabled {
            crate::peer::exchange_metrics::ExchangeMetrics::enabled()
        } else {
            Default::default()
        };
        self.peer_traffic = if enabled {
            crate::peer::traffic::Traffic::enabled()
        } else {
            Default::default()
        };
        self.peer_dials = if enabled {
            crate::peer::dial_metrics::DialMetrics::enabled()
        } else {
            Default::default()
        };
        self.formation_metrics = if enabled {
            crate::formation_metrics::FormationMetrics::enabled()
        } else {
            Default::default()
        };
        self
    }

    /// Bind an explicitly configured peer endpoint before advertising it. The
    /// listener does not itself enable peer admission.
    ///
    /// `interface`, when present, constrains this socket and every later peer
    /// socket to one device. It is retained so the outbound join endpoint
    /// cannot be created unplaced.
    pub fn bind_peer(
        mut self,
        bind: std::net::SocketAddr,
        advertise: Option<std::net::SocketAddr>,
        interface: Option<crate::net_placement::InterfaceName>,
    ) -> Result<Self, StartupError> {
        let advertised = advertise.unwrap_or(bind);
        if self.peer_endpoint.is_some()
            || advertised.ip().is_unspecified()
            || advertised.ip().is_multicast()
            || (advertise.is_some() && advertised.port() == 0)
        {
            return Err(StartupError::Peer(
                "invalid or duplicate peer advertisement".to_owned(),
            ));
        }
        let config = self
            .credentials
            .identity
            .server_config()
            .map_err(|error| StartupError::Peer(error.to_string()))?;
        // Equivalent to `quinn::Endpoint::server` apart from the device
        // binding: the same default endpoint config and async runtime.
        let socket = crate::net_placement::placed_udp_socket(
            bind,
            interface.as_ref(),
            crate::net_placement::DualStack::PlatformDefault,
        )?;
        let quic_runtime = quinn::default_runtime()
            .ok_or_else(|| StartupError::Peer("no async runtime found".to_owned()))?;
        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(config),
            socket,
            quic_runtime,
        )
        .map_err(|error| StartupError::Peer(error.to_string()))?;
        let actual = endpoint
            .local_addr()
            .map_err(|error| StartupError::Peer(error.to_string()))?;
        let mut local = self.membership.local().clone();
        local.endpoints.peers = vec![orishu_membership::Address(
            advertise.unwrap_or(actual).to_string(),
        )];
        self.membership = Membership::standalone(
            self.membership.formation().clone(),
            self.membership.cluster_name().clone(),
            local,
            AdmissionPolicy {
                accepts_peers: false,
                ..Default::default()
            },
            Limits::default(),
        )?;
        self.peer_endpoint = Some(endpoint);
        self.peer_interface = interface;
        Ok(self)
    }

    /// Configure the standalone introducer role before starting the owner.
    /// Binding alone never enables admission; enabled admission requires a
    /// listener. Subsequent adoption still withholds target token authority.
    pub fn with_peer_admission(mut self, enabled: bool) -> Result<Self, StartupError> {
        if enabled && self.peer_endpoint.is_none() {
            return Err(StartupError::Peer(
                "peer admission requires an explicit peer listener".to_owned(),
            ));
        }
        let mut local = self.membership.local().clone();
        local.accepts.peers = enabled;
        local.capacity.peers = if enabled { 64 } else { 0 };
        self.membership = Membership::standalone(
            self.membership.formation().clone(),
            self.membership.cluster_name().clone(),
            local,
            AdmissionPolicy {
                accepts_peers: enabled,
                ..self.membership.policy().clone()
            },
            self.membership.limits().clone(),
        )?;
        Ok(self)
    }

    /// Seed a two-page admission baseline through validated domain commands.
    /// Development-only fixture setup, before the owner and peer dispatcher run.
    #[cfg(feature = "formation-fault-test")]
    pub fn test_with_paginated_baseline(mut self) -> Self {
        use orishu_membership::{
            Command, Message, VersionTuple,
            model::{BlocklistAction, BlocklistEntry, BlocklistKey},
        };
        for index in 0..40 {
            let entry = BlocklistEntry {
                key: BlocklistKey::Name(format!("baseline-fixture-{index:02}").parse().unwrap()),
                action: BlocklistAction::Block,
                version: VersionTuple {
                    epoch: 0,
                    counter: 1,
                    actor: self.membership.local_id().clone(),
                },
                added_by: "baseline-fixture".into(),
            };
            self.membership = orishu_membership::update(
                self.membership,
                Message::Local(Command::UpdateBlocklist(entry)),
            )
            .model;
        }
        assert_eq!(self.membership.blocklist().len(), 40);
        self
    }

    /// Fresh projection of this worker's local model. Not convergence evidence.
    pub fn summary(&self) -> Summary {
        project_summary(&self.membership, self.participation)
    }

    /// Move the core into its single owner task. HTTP handlers retain only the
    /// published view and credential check, never a mutable membership reference.
    pub fn start(
        self,
    ) -> (
        RunningWorker,
        tokio::task::JoinHandle<Result<(), crate::driver::DriverError>>,
    ) {
        let (handle, task) = crate::driver::spawn_standalone_observed(
            self.membership,
            Some(self._join_token),
            self.peer_exchanges,
            self.peer_traffic,
            self.formation_metrics,
        );
        let outbound_endpoint = self.peer_endpoint.clone();
        let task = if let Some(endpoint) = self.peer_endpoint {
            let mut dispatcher = crate::peer::server::spawn_observed(
                endpoint,
                handle.clone(),
                self.peer_ingress.clone(),
            );
            let owner = handle.clone();
            tokio::spawn(async move {
                let mut task = task;
                tokio::select! {
                    result = &mut task => {
                        let _ = dispatcher.await;
                        result.map_err(|_| crate::driver::DriverError::PeerAdapterUnavailable)?
                    }
                    _ = &mut dispatcher => {
                        finish_after_peer_listener(owner, task).await
                    }
                }
            })
        } else {
            task
        };
        (
            RunningWorker {
                process_health: Default::default(),
                stopping: std::sync::atomic::AtomicBool::new(false),
                outbound_endpoint: std::sync::Mutex::new(outbound_endpoint),
                peer_interface: self.peer_interface,
                join_jobs: std::sync::Mutex::new(tokio::task::JoinSet::new()),
                peer_maintenance: std::sync::Mutex::new(None),
                catchup_maintenance: std::sync::Mutex::new(None),
                join_reconnect_maintenance: std::sync::Mutex::new(None),
                dialer: crate::peer::dial::Dialer::with_metrics(self.peer_dials),
                handle,
                credentials: self.credentials,
                #[cfg(feature = "observability")]
                peer_ingress: self.peer_ingress,
            },
            task,
        )
    }

    /// Public certificate fingerprint, never the private identity or token.
    pub fn fingerprint(&self) -> orishu_membership::CertFingerprint {
        self.credentials.identity.fingerprint()
    }

    /// Check only this worker's operator credential, never its join token.
    pub fn authorize_operator(&self, candidate: &str) -> bool {
        self.credentials.operator.matches(candidate)
    }
}

async fn finish_after_peer_listener(
    owner: crate::driver::Handle,
    task: tokio::task::JoinHandle<Result<(), crate::driver::DriverError>>,
) -> Result<(), crate::driver::DriverError> {
    // Shutdown acknowledges after publishing Stopping, before owner resources
    // and the watch sender are necessarily dropped. Listener exit in that
    // interval is expected; preserve the actual owner result.
    if owner.view().map_or(true, |view| {
        view.summary.participation == Participation::Stopping
    }) {
        return task
            .await
            .map_err(|_| crate::driver::DriverError::PeerAdapterUnavailable)?;
    }
    let _ = owner.shutdown().await;
    let _ = task.await;
    Err(crate::driver::DriverError::PeerAdapterUnavailable)
}

pub(crate) fn project_summary(membership: &Membership, participation: Participation) -> Summary {
    Summary {
        schema_version: 1,
        formation_id: membership.formation().clone(),
        cluster_name: membership.cluster_name().clone(),
        source_node_id: membership.local_id().clone(),
        member_count: membership.members().len(),
        alive_count: membership.alive_count(),
        membership_locked: membership.membership_locked(),
        participation,
        introducer_ready: false,
        workload: FormationWorkload::None,
        view: SummaryView::LocalAtRequest,
    }
}

/// IO-facing process state, with no membership mutation access.
pub struct RunningWorker {
    #[cfg(feature = "observability")]
    peer_ingress: crate::peer::ingress::IngressMetrics,
    process_health: crate::health::ProcessHealthState,
    join_reconnect_maintenance: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    catchup_maintenance: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    peer_maintenance: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    stopping: std::sync::atomic::AtomicBool,
    outbound_endpoint: std::sync::Mutex<Option<quinn::Endpoint>>,
    /// Constrains the lazily created outbound join endpoint. Configuration
    /// requires a peer listener whenever a peer interface is selected, so this
    /// is normally carried by an already placed endpoint; it is retained so the
    /// lazy path cannot create an unplaced socket if that rule ever changes.
    peer_interface: Option<crate::net_placement::InterfaceName>,
    join_jobs: std::sync::Mutex<tokio::task::JoinSet<()>>,
    dialer: crate::peer::dial::Dialer,
    handle: crate::driver::Handle,
    credentials: WorkerCredentials,
}

impl RunningWorker {
    /// Install the bounded exporter queue once at startup, before opening client
    /// service. Returns false if already installed; never changes domain state.
    #[cfg(feature = "otlp-tracing")]
    pub fn install_trace_queue(&self, queue: crate::trace_export::SpanQueue) -> bool {
        self.handle.install_trace_queue(queue)
    }

    /// Fixed owner deadline/abandonment observations, including after shutdown.
    #[cfg(feature = "observability")]
    pub fn formation_counters(&self) -> Option<crate::formation_metrics::Snapshot> {
        self.handle.formation_counters()
    }

    /// Optional outbound-attempt/TLS measurements, without owner work.
    #[cfg(feature = "observability")]
    pub fn peer_dial_counters(&self) -> Option<crate::peer::dial_metrics::Snapshot> {
        self.dialer.counters()
    }

    /// Occupied and configured outbound-attempt slots, without owner work.
    #[cfg(feature = "observability")]
    pub fn peer_dial_pressure(&self) -> (usize, usize) {
        self.dialer.pressure()
    }

    /// Optional fixed datagram/pre-pool IO readings, without owner work.
    #[cfg(feature = "observability")]
    pub fn peer_traffic_counters(&self) -> Option<crate::peer::traffic::Snapshot> {
        self.handle.packet_io().snapshot()
    }

    /// Optional process-lifetime inbound peer counters, without owner IO.
    #[cfg(feature = "observability")]
    pub fn peer_ingress_counters(&self) -> Option<crate::peer::ingress::IngressSnapshot> {
        self.peer_ingress.snapshot()
    }

    /// Fixed-size reliable-exchange counters, available only with collection on.
    #[cfg(feature = "observability")]
    pub fn peer_exchange_counters(
        &self,
    ) -> Option<crate::peer::exchange_metrics::ExchangeSnapshot> {
        self.handle.exchange_pool().counters()
    }

    /// Occupied and configured reliable-exchange slots, without owner work.
    #[cfg(feature = "observability")]
    pub fn peer_exchange_pressure(&self) -> (usize, usize) {
        self.handle.exchange_pool().pressure()
    }

    /// Current bounded-lane occupancy, including reserved permits. These local
    /// readings do not establish owner responsiveness or cluster capacity.
    pub fn owner_pressure(&self) -> crate::driver::OwnerPressure {
        self.handle.pressure()
    }

    /// Latest published process-lifetime owner counters, including after owner
    /// closure. Constant-size copy with no peer enumeration or owner request.
    pub fn owner_counters(&self) -> crate::driver::OwnerCounters {
        self.handle.counters()
    }

    /// Called by the process composition root after every configured listener
    /// and required local role has initialized. Latches for this process only.
    pub fn mark_initialized(&self) {
        self.process_health.initialized();
    }

    /// Scope a required listener task. Any exit (including panic/cancellation)
    /// withholds readiness; shutdown remains a distinct higher-priority reason.
    pub fn required_role(self: &std::sync::Arc<Self>) -> RequiredRole {
        RequiredRole(self.clone())
    }

    /// Read local probe inputs without peer, storage, exporter or owner IO.
    pub fn health(&self) -> crate::health::ProcessHealth {
        self.process_health.snapshot(
            self.handle.health(tokio::time::Instant::now()),
            self.stopping.load(std::sync::atomic::Ordering::Acquire),
        )
    }
    /// Hold acknowledged owner shutdown until the fixture releases or drops
    /// the channel. Development-only Rust seam; no network or CLI control.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_hold_shutdown(&self, release: tokio::sync::oneshot::Receiver<()>) {
        self.handle.hold_shutdown(release).await;
    }

    /// Pause the running owner until the fixture releases or drops the channel.
    /// Development-only Rust seam; neither changes state nor pauses HTTP IO.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_hold_progress(&self, release: tokio::sync::oneshot::Receiver<()>) {
        self.handle.hold_progress(release).await;
    }

    /// Corrupt the next baseline reply for the zero-based page index and report
    /// its encoding. Earlier pages and non-page replies remain unchanged.
    /// Development-only Rust seam; no CLI/network control or health mutation.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_corrupt_catchup_page(
        &self,
        index: u16,
    ) -> tokio::sync::oneshot::Receiver<()> {
        self.handle.corrupt_catchup_page(index).await
    }

    /// Drop the next transition's encoded departure datagrams in a fault build.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_drop_next_departure(&self) {
        self.handle.drop_next_departure().await;
    }
    /// Send a development-only ejection fixture to the recorded introducer or sole peer.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_eject_peer_fixture(
        &self,
    ) -> Result<orishu_membership::NodeId, crate::driver::DriverError> {
        self.handle.eject_peer_fixture().await
    }
    /// Development-only certificate block after insertion, before the ACK.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_block_after_next_join(
        &self,
    ) -> tokio::sync::oneshot::Receiver<orishu_membership::NodeId> {
        self.handle.block_after_next_join().await
    }
    /// Development-only removal of the accepted assignment before its ACK.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_remove_after_next_join(
        &self,
    ) -> tokio::sync::oneshot::Receiver<orishu_membership::NodeId> {
        self.handle.remove_after_next_join().await
    }
    /// Development-only abrupt process exit after insertion, before the JoinAck.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_crash_after_next_join(&self) {
        self.handle.crash_after_next_join().await;
    }
    /// Arm one development-only post-insertion ACK loss, before opening client listeners.
    #[cfg(feature = "formation-fault-test")]
    pub async fn test_lose_next_join_ack(
        &self,
    ) -> tokio::sync::oneshot::Receiver<orishu_membership::NodeId> {
        self.handle.lose_next_join_ack().await
    }
    /// Repair only an explicitly pending join, under its original target pin
    /// and budget. This never discovers a cluster or starts a new admission.
    fn start_join_reconnect_maintenance(self: &std::sync::Arc<Self>) {
        let mut slot = self
            .join_reconnect_maintenance
            .lock()
            .expect("join reconnect lock");
        if slot.is_some() || self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let runtime = std::sync::Arc::downgrade(self);
        *slot = Some(tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut jobs = tokio::task::JoinSet::new();
            let mut generation = None;
            loop {
                tokio::select! {
                    _ = tick.tick() => {},
                    _ = jobs.join_next(), if !jobs.is_empty() => { continue; }
                }
                let Some(runtime) = runtime.upgrade() else {
                    break;
                };
                if runtime.stopping.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let Ok(view) = runtime.handle.view() else {
                    break;
                };
                if generation != Some(view.generation)
                    || view.summary.participation != Participation::Joining
                {
                    jobs.shutdown().await;
                    generation = Some(view.generation);
                }
                if !jobs.is_empty() || view.summary.participation != Participation::Joining {
                    continue;
                }
                let endpoint = runtime
                    .outbound_endpoint
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                let Some(endpoint) = endpoint else {
                    continue;
                };
                let Ok(reply) = runtime.handle.prepare_join_reconnect() else {
                    continue;
                };
                let Ok(Ok(Some(job))) =
                    tokio::time::timeout(std::time::Duration::from_secs(1), reply).await
                else {
                    continue;
                };
                jobs.spawn(async move {
                    let run = async {
                        if let Ok(pending) = runtime
                            .dialer
                            .handshake(
                                &endpoint,
                                &runtime.credentials.identity,
                                &job.local,
                                &job.target,
                                &runtime.handle.exchange_pool(),
                            )
                            .await
                        {
                            job.finish(pending);
                        }
                    };
                    tokio::select! {
                        biased;
                        _ = runtime.handle.closed() => {},
                        _ = tokio::time::timeout(std::time::Duration::from_secs(16), run) => {},
                    }
                });
            }
            jobs.shutdown().await;
        }));
    }
    /// Supervise post-admission catch-up using the owner's bounded attempt
    /// policy. Repeated calls are harmless; this never initiates admission.
    pub fn start_catchup_maintenance(self: &std::sync::Arc<Self>) {
        let mut slot = self.catchup_maintenance.lock().expect("catch-up lock");
        if slot.is_some() || self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let notifications = self.handle.clone();
        let runtime = std::sync::Arc::downgrade(self);
        *slot = Some(tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut jobs = tokio::task::JoinSet::new();
            let mut generation = None;
            loop {
                tokio::select! {
                    _ = tick.tick() => {},
                    _ = notifications.first_catchup_route() => {},
                    _ = jobs.join_next(), if !jobs.is_empty() => { continue; }
                }
                let Some(runtime) = runtime.upgrade() else {
                    break;
                };
                if runtime.stopping.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let Ok(view) = runtime.handle.view() else {
                    break;
                };
                if generation != Some(view.generation)
                    || view.summary.participation != Participation::CatchingUp
                {
                    jobs.shutdown().await;
                    generation = Some(view.generation);
                }
                if !jobs.is_empty() || view.summary.participation != Participation::CatchingUp {
                    continue;
                }
                let Ok(reply) = runtime.handle.prepare_catchup() else {
                    continue;
                };
                let Ok(Ok(Some(job))) =
                    tokio::time::timeout(std::time::Duration::from_secs(1), reply).await
                else {
                    continue;
                };
                let handle = runtime.handle.clone();
                jobs.spawn(async move {
                    let pool = handle.exchange_pool();
                    tokio::select! {
                        _ = handle.closed() => {},
                        _ = job.execute(&pool) => {},
                    }
                });
            }
            jobs.shutdown().await;
        }));
    }

    /// Start one bounded connection-maintenance loop after sharing the runtime.
    /// No listener is created here; an explicit join may provide an outgoing
    /// endpoint later. Repeated calls do not create additional schedulers.
    pub fn start_peer_maintenance(self: &std::sync::Arc<Self>) {
        self.start_join_reconnect_maintenance();
        let mut slot = self.peer_maintenance.lock().expect("maintenance lock");
        if slot.is_some() || self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let notifications = self.handle.clone();
        let runtime = std::sync::Arc::downgrade(self);
        *slot = Some(tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut tasks = tokio::task::JoinSet::new();
            let mut active = std::collections::BTreeSet::new();
            let mut after = None;
            let mut generation = None;
            loop {
                tokio::select! {
                    _ = tick.tick() => {},
                    _ = notifications.formation_adopted() => {},
                    result = tasks.join_next(), if !tasks.is_empty() => {
                        if let Some(Ok(node)) = result { active.remove(&node); }
                        continue;
                    }
                }
                let Some(runtime) = runtime.upgrade() else {
                    break;
                };
                if runtime.stopping.load(std::sync::atomic::Ordering::Acquire)
                    || runtime.handle.view().is_err()
                {
                    break;
                }
                let Ok(view) = runtime.handle.view() else {
                    break;
                };
                let current = view.generation;
                if generation != Some(current) {
                    tasks.abort_all();
                    while tasks.join_next().await.is_some() {}
                    active.clear();
                    after = None;
                    generation = Some(current);
                }
                if tasks.len() >= 64 {
                    continue;
                }
                let endpoint = runtime
                    .outbound_endpoint
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                let Some(endpoint) = endpoint else {
                    continue;
                };
                // Use the existing four handshake slots, without a waiter
                // queue or repeated cursor wrap inside one tick. All owner
                // queries share the same one-second preparation deadline.
                let query_deadline =
                    tokio::time::Instant::now() + std::time::Duration::from_secs(1);
                for _ in 0..4 {
                    if tasks.len() >= 64 {
                        break;
                    }
                    let Ok(reply) = runtime.handle.member_dial(after.clone()) else {
                        break;
                    };
                    let Ok(Ok(page)) = tokio::time::timeout_at(query_deadline, reply).await else {
                        break;
                    };
                    after = page.after;
                    if page.generation != current {
                        break;
                    }
                    let Some(plan) = page.plan else {
                        break;
                    };
                    if !active.insert(plan.node.clone()) {
                        continue;
                    }
                    let runtime = runtime.clone();
                    let endpoint = endpoint.clone();
                    tasks.spawn(async move {
                        let run = async {
                            let pool = runtime.handle.exchange_pool();
                            let pending_result = runtime
                                .dialer
                                .member_handshake(
                                    &endpoint,
                                    &runtime.credentials.identity,
                                    &plan.local,
                                    &plan.target,
                                    &pool,
                                )
                                .await;
                            let pending = pending_result.ok()?;
                            let (connection, bytes) = pending.into_parts();
                            let connection = crate::peer::server::ConnectionLease(connection);
                            let reply = runtime
                                .handle
                                .accept_member_handshake_reply(current, connection.0.clone(), bytes)
                                .ok()?;
                            let accepted_result =
                                tokio::time::timeout(std::time::Duration::from_secs(5), reply)
                                    .await
                                    .ok()?
                                    .ok()?;
                            let accepted = accepted_result.ok()?;
                            crate::peer::server::serve_registered(
                                &connection.0,
                                &runtime.handle,
                                current,
                                accepted.session,
                                &pool,
                            )
                            .await;
                            Some(())
                        };
                        tokio::select! {
                            _ = runtime.handle.closed() => {},
                            _ = run => {},
                        }
                        plan.node
                    });
                }
            }
            tasks.shutdown().await;
        }));
    }
    /// Submit only after authenticating operator authority. Once scheduled, the
    /// worker owns the job even if the requesting HTTP connection disappears.
    pub async fn submit_join(
        self: &std::sync::Arc<Self>,
        request: orishu::model::cluster::JoinRequest,
    ) -> Result<orishu::model::cluster::JoinOperation, JoinSubmitError> {
        self.submit_join_with_parent(request, None).await
    }

    /// Authorized IO adapter entry point with optional request-local ancestry.
    /// Context is discarded without an installed tracing queue and never enters
    /// the operation identity, request digest or retained operation receipt.
    pub async fn submit_join_with_parent(
        self: &std::sync::Arc<Self>,
        request: orishu::model::cluster::JoinRequest,
        parent: Option<crate::trace_context::TraceParent>,
    ) -> Result<orishu::model::cluster::JoinOperation, JoinSubmitError> {
        if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return Err(crate::driver::DriverError::Closed.into());
        }
        let prepared = self
            .handle
            .prepare_join_with_parent(request, parent)?
            .await
            .map_err(|_| crate::driver::DriverError::Closed)??;
        match prepared {
            crate::driver::PreparedJoin::Replay(operation) => Ok(*operation),
            crate::driver::PreparedJoin::Start(job) => {
                let operation = job.operation().clone();
                let mut jobs = self
                    .join_jobs
                    .lock()
                    .map_err(|_| crate::driver::DriverError::Closed)?;
                if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(crate::driver::DriverError::Closed.into());
                }
                while jobs.try_join_next().is_some() {}
                if jobs.len() >= 4 {
                    return Err(crate::driver::DriverError::Overloaded.into());
                }
                let runtime = self.clone();
                jobs.spawn(async move {
                    let run = async {
                        let Ok(target) =
                            crate::peer::dial::IntroducerTarget::try_from(job.material())
                        else {
                            return;
                        };
                        let endpoint = {
                            let Ok(mut slot) = runtime.outbound_endpoint.lock() else {
                                return;
                            };
                            if slot.is_none() {
                                let Ok(address) = job.material().peer_endpoints[0]
                                    .parse::<std::net::SocketAddr>()
                                else {
                                    return;
                                };
                                let bind = if address.is_ipv4() {
                                    "0.0.0.0:0"
                                } else {
                                    "[::]:0"
                                };
                                // Equivalent to `quinn::Endpoint::client`, plus
                                // the peer role's device constraint: the first
                                // join must not leave over an excluded
                                // interface just because no listener exists.
                                let Ok(socket) = crate::net_placement::placed_udp_socket(
                                    bind.parse().expect("static socket"),
                                    runtime.peer_interface.as_ref(),
                                    crate::net_placement::DualStack::Request,
                                ) else {
                                    return;
                                };
                                let Some(quic_runtime) = quinn::default_runtime() else {
                                    return;
                                };
                                let Ok(endpoint) = quinn::Endpoint::new(
                                    quinn::EndpointConfig::default(),
                                    None,
                                    socket,
                                    quic_runtime,
                                ) else {
                                    return;
                                };
                                *slot = Some(endpoint);
                            }
                            slot.as_ref().expect("installed endpoint").clone()
                        };
                        if let Ok(pending) = runtime
                            .dialer
                            .handshake(
                                &endpoint,
                                &runtime.credentials.identity,
                                job.local(),
                                &target,
                                &runtime.handle.exchange_pool(),
                            )
                            .await
                        {
                            job.finish(pending);
                        }
                    };
                    tokio::select! {
                        biased;
                        _ = runtime.handle.closed() => {},
                        _ = tokio::time::timeout(std::time::Duration::from_secs(16), run) => {},
                    }
                });
                Ok(operation)
            }
        }
    }

    /// Read the current retained operation after authenticating the caller.
    pub async fn join_status(
        &self,
        id: orishu::model::cluster::OperationId,
    ) -> Result<Option<orishu::model::cluster::JoinOperation>, crate::driver::DriverError> {
        self.handle
            .join_status(id)?
            .await
            .map_err(|_| crate::driver::DriverError::Closed)
    }
    /// Read issuer evidence after authenticating operator authority.
    pub async fn inspect_admission(
        &self,
        request: orishu::model::cluster::AdmissionInspectionRequest,
    ) -> Result<orishu::model::cluster::AdmissionInspection, crate::driver::DriverError> {
        self.handle
            .inspect_admission(request)?
            .await
            .map_err(|_| crate::driver::DriverError::Closed)
    }
    /// Caller must authenticate operator authority before requesting this secret.
    pub fn join_material(
        &self,
    ) -> Result<
        tokio::sync::oneshot::Receiver<Option<orishu::model::cluster::JoinMaterial>>,
        crate::driver::DriverError,
    > {
        self.handle.join_material()
    }
    /// Fetch a bounded current local page; None rejects a stale continuation.
    pub fn list(
        &self,
        formation: Option<FormationId>,
        after: Option<NodeId>,
    ) -> Result<
        tokio::sync::oneshot::Receiver<Option<orishu::model::node::MembershipPage>>,
        crate::driver::DriverError,
    > {
        self.handle.list(formation, after)
    }
    /// Read one member through the serialized owner's bounded control lane.
    pub fn inspect(
        &self,
        node: NodeId,
    ) -> Result<
        tokio::sync::oneshot::Receiver<Option<orishu::model::node::Inspection>>,
        crate::driver::DriverError,
    > {
        self.handle.inspect(node)
    }
    /// Queue a request only after the client adapter has authenticated it.
    pub fn leave_operation(
        &self,
        request: orishu::model::cluster::LeaveRequest,
    ) -> Result<
        tokio::sync::oneshot::Receiver<
            Result<orishu::model::cluster::LeaveReceipt, crate::driver::LeaveError>,
        >,
        crate::driver::DriverError,
    > {
        if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return Err(crate::driver::DriverError::Closed);
        }
        self.handle.leave_operation(request)
    }

    /// Queue a request only after the client adapter has authenticated it.
    pub fn lock_operation(
        &self,
        request: orishu::model::cluster::LockRequest,
    ) -> Result<
        tokio::sync::oneshot::Receiver<
            Result<orishu::model::cluster::LockReceipt, crate::driver::LockError>,
        >,
        crate::driver::DriverError,
    > {
        self.handle.lock_operation(request)
    }

    /// A stopped owner cannot serve a stale projection as a fresh summary.
    pub fn summary(&self) -> Result<Summary, crate::driver::DriverError> {
        Ok(self.handle.view()?.summary)
    }
    /// Authenticate against the worker-local operator credential.
    pub fn authorize_operator(&self, candidate: &str) -> bool {
        self.credentials.operator.matches(candidate)
    }
    /// Stop the owner through its reserved control path.
    pub async fn shutdown(&self) -> Result<(), crate::driver::DriverError> {
        self.stopping
            .store(true, std::sync::atomic::Ordering::Release);
        let reconnect = self
            .join_reconnect_maintenance
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(task) = reconnect {
            task.abort();
            let _ = task.await;
        }
        let catchup = self
            .catchup_maintenance
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(task) = catchup {
            task.abort();
            let _ = task.await;
        }
        let maintenance = self
            .peer_maintenance
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(task) = maintenance {
            task.abort();
            let _ = task.await;
        }
        let jobs = self
            .join_jobs
            .lock()
            .ok()
            .map(|mut jobs| std::mem::take(&mut *jobs));
        if let Some(mut jobs) = jobs {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), jobs.shutdown()).await;
        }
        let outcome = self.handle.shutdown().await;
        if let Ok(endpoint) = self.outbound_endpoint.lock()
            && let Some(endpoint) = endpoint.as_ref()
        {
            endpoint.close(0_u32.into(), b"worker stopping");
        }
        outcome
    }
}

/// Lifetime guard for a configured required process role, not cluster authority.
pub struct RequiredRole(std::sync::Arc<RunningWorker>);

impl Drop for RequiredRole {
    fn drop(&mut self) {
        self.0.process_health.role_failed();
    }
}

/// Submission failures never include secret request or peer error payloads.
#[derive(Debug, thiserror::Error)]
pub enum JoinSubmitError {
    #[error(transparent)]
    Driver(#[from] crate::driver::DriverError),
    #[error(transparent)]
    Operation(#[from] crate::join_operations::OperationError),
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_catchup_reacts_to_adoption_and_route_without_waiting_for_ticks() {
        let root = tempfile::tempdir().unwrap();
        let create = |name: &str| {
            WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join(name)).unwrap(),
                WorkerName::new(name).unwrap(),
                ClusterName::new("cluster").unwrap(),
                vec![],
            )
            .unwrap()
            .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
            .unwrap()
            .with_peer_admission(true)
            .unwrap()
            .start()
        };
        let (target, target_task) = create("target");
        let (source, source_task) = create("source");
        let source = std::sync::Arc::new(source);
        source.start_peer_maintenance();
        source.start_catchup_maintenance();
        // Consume initial interval ticks while still standalone. Admission
        // and a real registered TLS route must wake the first attempt themselves.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let request = orishu::model::cluster::JoinRequest {
            schema_version: 1,
            operation_id: "prompt-catchup".parse().unwrap(),
            formation_id: source.summary().unwrap().formation_id,
            material: target.join_material().unwrap().await.unwrap().unwrap(),
        };
        let completed = tokio::time::timeout(std::time::Duration::from_millis(800), async {
            source.submit_join(request).await.unwrap();
            while source.summary().unwrap().participation != Participation::Joined {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .is_ok();
        source.shutdown().await.unwrap();
        target.shutdown().await.unwrap();
        assert_eq!(source_task.await.unwrap(), Ok(()));
        assert_eq!(target_task.await.unwrap(), Ok(()));
        assert!(
            completed,
            "first catch-up should not wait for either one-second tick"
        );
    }

    #[cfg(feature = "observability")]
    #[tokio::test]
    async fn member_maintenance_uses_the_existing_four_dial_slots_in_one_tick() {
        let root = tempfile::tempdir().unwrap();
        let mut runtimes = Vec::new();
        for index in 0..5 {
            let name = format!("node-{index}");
            let runtime = WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join(&name)).unwrap(),
                WorkerName::new(&name).unwrap(),
                ClusterName::new("cluster").unwrap(),
                vec![],
            )
            .unwrap()
            .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
            .unwrap()
            .with_peer_admission(true)
            .unwrap()
            .with_peer_metrics(true);
            runtimes.push(runtime);
        }
        // Pre-admitted fixture isolates connection scheduling; every actual
        // connection must still pass the production TLS/member handshake.
        let formation = runtimes[0].membership.formation().clone();
        let locals: Vec<_> = runtimes
            .iter()
            .enumerate()
            .map(|(index, runtime)| {
                let mut local = runtime.membership.local().clone();
                local.node_id = NodeId::new(format!("node-{index}")).unwrap();
                local
            })
            .collect();
        let members: Vec<_> = locals
            .iter()
            .map(|local| {
                Membership::standalone(
                    formation.clone(),
                    ClusterName::new("cluster").unwrap(),
                    local.clone(),
                    Default::default(),
                    Limits::default(),
                )
                .unwrap()
                .member(&local.node_id)
                .unwrap()
                .clone()
            })
            .collect();
        for (runtime, local) in runtimes.iter_mut().zip(locals) {
            runtime.membership = Membership::standalone(
                formation.clone(),
                ClusterName::new("cluster").unwrap(),
                local,
                runtime.membership.policy().clone(),
                Limits::default(),
            )
            .unwrap();
            for member in &members {
                orishu_membership::testing::insert_member(&mut runtime.membership, member.clone());
            }
        }
        let (workers, tasks): (Vec<_>, Vec<_>) = runtimes
            .into_iter()
            .map(|runtime| {
                let (worker, task) = runtime.start();
                (std::sync::Arc::new(worker), task)
            })
            .unzip();
        workers[0].start_peer_maintenance();
        let completed = tokio::time::timeout(std::time::Duration::from_millis(800), async {
            while workers[0]
                .formation_counters()
                .unwrap()
                .registry_pressure()
                .slots_in_use
                != 4
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .is_ok();
        for worker in workers {
            worker.shutdown().await.unwrap();
        }
        for task in tasks {
            assert_eq!(task.await.unwrap(), Ok(()));
        }
        assert!(
            completed,
            "four free handshake slots should be used before the second tick"
        );
    }

    #[tokio::test]
    async fn adopted_joiner_prioritizes_its_known_introducer_once_before_canonical_scan() {
        let root = tempfile::tempdir().unwrap();
        let create = |name: &str| {
            WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join(name)).unwrap(),
                WorkerName::new(name).unwrap(),
                ClusterName::new("cluster").unwrap(),
                vec![],
            )
            .unwrap()
        };
        let mut target = create("target");
        // Every generated hexadecimal assigned ID sorts after this prefix.
        // The ordinary canonical dial rule therefore skips this introducer.
        let mut local = target.membership.local().clone();
        local.node_id = NodeId::new("0").unwrap();
        target.membership = Membership::standalone(
            target.membership.formation().clone(),
            target.membership.cluster_name().clone(),
            local,
            target.membership.policy().clone(),
            target.membership.limits().clone(),
        )
        .unwrap();
        let start = |runtime: WorkerRuntime| {
            runtime
                .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
                .unwrap()
                .with_peer_admission(true)
                .unwrap()
                .start()
        };
        let (target, target_task) = start(target);
        let (source, source_task) = start(create("source"));
        let source = std::sync::Arc::new(source);
        let request = orishu::model::cluster::JoinRequest {
            schema_version: 1,
            operation_id: "preferred-introducer".parse().unwrap(),
            formation_id: source.summary().unwrap().formation_id,
            material: target.join_material().unwrap().await.unwrap().unwrap(),
        };
        source.submit_join(request).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while source.summary().unwrap().participation != Participation::CatchingUp {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // No maintenance loop has run: test the production owner's first plan,
        // after actual serialized admission, not a fabricated catch-up state.
        let first = source.handle.member_dial(None).unwrap().await.unwrap();
        let second = source.handle.member_dial(None).unwrap().await.unwrap();
        source.shutdown().await.unwrap();
        target.shutdown().await.unwrap();
        assert_eq!(source_task.await.unwrap(), Ok(()));
        assert_eq!(target_task.await.unwrap(), Ok(()));
        assert_eq!(
            first.plan.map(|plan| plan.node),
            Some(NodeId::new("0").unwrap())
        );
        assert!(
            first.after.is_none(),
            "priority does not skip the ordinary cursor"
        );
        assert!(
            second.plan.is_none(),
            "the exception is consumed once, not a retry loop"
        );
    }

    #[tokio::test]
    async fn explicit_introducer_runtime_admits_but_adoption_withholds_readiness() {
        exercise_catchup_lifecycle(CatchupCase::Retry).await;
    }

    #[tokio::test]
    async fn ejection_fences_prepared_catchup_and_its_late_cancellation() {
        exercise_catchup_lifecycle(CatchupCase::EjectCancelled).await;
    }

    #[tokio::test]
    async fn ejection_fences_successful_catchup_completion() {
        exercise_catchup_lifecycle(CatchupCase::EjectCompleted).await;
    }

    #[tokio::test]
    async fn leave_fences_successful_catchup_completion() {
        exercise_catchup_lifecycle(CatchupCase::LeaveCompleted).await;
    }

    #[tokio::test]
    async fn shutdown_fences_successful_catchup_completion() {
        exercise_catchup_lifecycle(CatchupCase::ShutdownCompleted).await;
    }

    #[tokio::test]
    async fn source_session_loss_refuses_successful_catchup_then_retries() {
        exercise_catchup_lifecycle(CatchupCase::DisconnectCompleted).await;
    }

    #[tokio::test]
    async fn exhausted_catchup_attempts_remain_non_introducing() {
        exercise_catchup_lifecycle(CatchupCase::Exhausted).await;
    }

    #[tokio::test]
    async fn adoption_deadline_refuses_successful_catchup_completion() {
        exercise_catchup_lifecycle(CatchupCase::ExpiredCompleted).await;
    }

    #[tokio::test]
    async fn pending_join_redials_original_introducer_after_pre_insertion_disconnect() {
        exercise_catchup_lifecycle(CatchupCase::ReconnectBeforeAdmission).await;
    }

    #[tokio::test]
    async fn cancelled_join_reconnect_jobs_release_reservation_without_resetting_budget() {
        exercise_catchup_lifecycle(CatchupCase::ReconnectExhausted).await;
    }

    #[tokio::test]
    async fn shutdown_fences_successful_join_reconnect_handshake() {
        exercise_catchup_lifecycle(CatchupCase::ReconnectShutdown).await;
    }

    #[tokio::test]
    async fn replacement_fences_successful_join_reconnect_and_preserves_new_operation() {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            exercise_catchup_lifecycle(CatchupCase::ReconnectReplacement),
        )
        .await
        .expect("late reconnect replacement fixture deadline");
    }

    #[tokio::test]
    async fn lost_join_ack_recovers_original_assignment_after_redial() {
        exercise_catchup_lifecycle(CatchupCase::LostAck).await;
    }

    #[tokio::test]
    async fn runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown() {
        use orishu::model::cluster::LeaveRequest;
        use std::{sync::Arc, time::Duration};
        tokio::time::timeout(Duration::from_secs(16), async {
            for leave in [true, false] {
                let root = tempfile::tempdir().unwrap();
                let mut configured = WorkerRuntime::standalone(
                    WorkerCredentials::load_or_create(&root.path().join("worker")).unwrap(),
                    WorkerName::new("maintenance-pressure").unwrap(),
                    ClusterName::new("maintenance-pressure").unwrap(),
                    vec![],
                )
                .unwrap()
                .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
                .unwrap();
                let mut sinks = Vec::new();
                // Arrange an already-admitted formation before starting the real
                // runtime. Names sort after its random hex ID, so maintenance's
                // canonical lower-ID dialing rule selects these four routes.
                for index in 0..4 {
                    let sink = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
                    let mut member =
                        orishu_membership::testing::member(&format!("zz-silent-{index}"), 1);
                    member.cert_fingerprint = crate::peer::tls::PeerIdentity::generate()
                        .unwrap()
                        .fingerprint();
                    member.endpoints.peers = vec![orishu_membership::Address(
                        sink.local_addr().unwrap().to_string(),
                    )];
                    orishu_membership::testing::insert_member(&mut configured.membership, member);
                    sinks.push(sink);
                }
                configured.participation = Participation::Joined;
                let (runtime, owner) = configured.start();
                let runtime = Arc::new(runtime);
                let before = runtime.handle.view().unwrap();
                runtime.start_peer_maintenance();
                let mut packet = [0; 2048];
                tokio::time::timeout(Duration::from_secs(4), async {
                    for sink in &sinks {
                        assert!(sink.recv_from(&mut packet).await.unwrap().0 > 0);
                    }
                })
                .await
                .expect("maintenance itself dials all four silent routes");
                assert_eq!(runtime.dialer.available_attempts(), 0);
                assert_eq!(runtime.summary().unwrap().member_count, 5);
                tokio::time::timeout(Duration::from_secs(2), async {
                    if leave {
                        let receipt = runtime
                            .leave_operation(LeaveRequest {
                                schema_version: 1,
                                operation_id: "leave-saturated-maintenance".parse().unwrap(),
                                formation_id: before.summary.formation_id.clone(),
                            })
                            .unwrap()
                            .await
                            .unwrap()
                            .unwrap();
                        assert!(receipt.changed);
                        assert_ne!(receipt.current.formation_id, before.summary.formation_id);
                        assert_ne!(
                            receipt.current.source_node_id,
                            before.summary.source_node_id
                        );
                        // Generation change must drain maintenance without the
                        // test touching its JoinSet or aborting its task.
                        while runtime.dialer.available_attempts() != 4 {
                            tokio::task::yield_now().await;
                        }
                        let current = runtime.handle.view().unwrap();
                        assert_ne!(current.generation, before.generation);
                        assert_eq!(current.summary.member_count, 1);
                        assert_eq!(current.summary.participation, Participation::Standalone);
                    }
                    runtime.shutdown().await.unwrap();
                    assert_eq!(owner.await.unwrap(), Ok(()));
                    assert_eq!(runtime.dialer.available_attempts(), 4);
                    assert!(runtime.peer_maintenance.lock().unwrap().is_none());
                })
                .await
                .expect("real lifecycle cancels pending maintenance within two seconds");
            }
        })
        .await
        .expect("runtime maintenance lifecycle fixture stays bounded");
    }

    #[tokio::test]
    async fn simultaneous_admitted_dials_recover_crossed_connections_without_readmission() {
        // Also bound fixture setup, snapshot requests and orderly owner shutdown.
        tokio::time::timeout(
            std::time::Duration::from_secs(45),
            exercise_catchup_lifecycle(CatchupCase::MemberCollision),
        )
        .await
        .expect("collision/recovery fixture completes and shuts down within its budget");
    }

    #[derive(Clone, Copy)]
    enum CatchupCase {
        Retry,
        EjectCancelled,
        EjectCompleted,
        LeaveCompleted,
        ShutdownCompleted,
        DisconnectCompleted,
        Exhausted,
        ExpiredCompleted,
        ReconnectBeforeAdmission,
        ReconnectExhausted,
        ReconnectShutdown,
        ReconnectReplacement,
        LostAck,
        MemberCollision,
    }

    async fn exercise_member_collision(
        a: &std::sync::Arc<RunningWorker>,
        b: &std::sync::Arc<RunningWorker>,
    ) {
        use crate::peer::{dial::MemberTarget, registry::RegistryError};
        use std::time::Duration;

        // Freeze only automatic member dials while arranging crossed sessions.
        // Both real peer dispatchers and serialized membership owners keep running.
        for runtime in [a, b] {
            let task = runtime.peer_maintenance.lock().unwrap().take().unwrap();
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            runtime.handle.disconnect_peers().await;
        }
        let a_model = a.handle.model_snapshot().await;
        let b_model = b.handle.model_snapshot().await;
        let a_view = a.handle.view().unwrap();
        let b_view = b.handle.view().unwrap();
        let a_endpoint = a.outbound_endpoint.lock().unwrap().clone().unwrap();
        let b_endpoint = b.outbound_endpoint.lock().unwrap().clone().unwrap();
        let to_b = MemberTarget::from_model(&a_model, b_model.local_id()).unwrap();
        let to_a = MemberTarget::from_model(&b_model, a_model.local_id()).unwrap();
        let a_pool = a.handle.exchange_pool();
        let b_pool = b.handle.exchange_pool();

        // Await BOTH real responses before submitting either outgoing binding.
        // Thus each registry has accepted its incoming connection while the
        // opposite outgoing completion is held at this explicit barrier.
        let (a_pending, b_pending) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                a.dialer.member_handshake(
                    &a_endpoint,
                    &a.credentials.identity,
                    a_model.local(),
                    &to_b,
                    &a_pool
                ),
                b.dialer.member_handshake(
                    &b_endpoint,
                    &b.credentials.identity,
                    b_model.local(),
                    &to_a,
                    &b_pool
                )
            )
        })
        .await
        .expect("both admitted handshakes reach the completion barrier");
        let (a_connection, a_reply) = a_pending.unwrap().into_parts();
        let (b_connection, b_reply) = b_pending.unwrap().into_parts();
        let a_registration = a
            .handle
            .accept_member_handshake_reply(a_view.generation, a_connection.clone(), a_reply)
            .unwrap();
        let b_registration = b
            .handle
            .accept_member_handshake_reply(b_view.generation, b_connection.clone(), b_reply)
            .unwrap();
        let (a_result, b_result) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(a_registration, b_registration)
        })
        .await
        .unwrap();
        assert!(matches!(a_result.unwrap(), Err(RegistryError::Duplicate)));
        assert!(matches!(b_result.unwrap(), Err(RegistryError::Duplicate)));
        tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(a_connection.closed(), b_connection.closed());
        })
        .await
        .expect("crossed duplicate connections close at both ends");

        // No harness connection replacement: production maintenance must repair
        // the crossed closure and transport new policy plus reliable exchanges.
        a.start_peer_maintenance();
        b.start_peer_maintenance();
        for locked in [true, false] {
            if !locked {
                // Break the recovered route too, with both owners still alive.
                // Only normal maintenance may create its replacement.
                b.handle.disconnect_peers().await;
            }
            let previous_exchanges = a.handle.view().unwrap().completed_exchanges
                + b.handle.view().unwrap().completed_exchanges;
            a.handle
                .set_membership_lock(a_view.generation, a_model.formation().clone(), locked)
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let left = a.handle.view().unwrap();
                    let right = b.handle.view().unwrap();
                    assert_eq!(left.generation, a_view.generation);
                    assert_eq!(right.generation, b_view.generation);
                    assert_eq!(left.summary.source_node_id, *a_model.local_id());
                    assert_eq!(right.summary.source_node_id, *b_model.local_id());
                    assert_eq!(left.summary.formation_id, *a_model.formation());
                    assert_eq!(right.summary.formation_id, *a_model.formation());
                    assert_eq!(left.summary.member_count, 2);
                    assert_eq!(right.summary.member_count, 2);
                    if left.summary.membership_locked == locked
                        && right.summary.membership_locked == locked
                        && left.summary.alive_count == 2
                        && right.summary.alive_count == 2
                        && left.completed_exchanges + right.completed_exchanges > previous_exchanges
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("maintenance repairs collision and carries real member traffic");
        }
        for (runtime, original) in [(a, a_model), (b, b_model)] {
            let current = runtime.handle.model_snapshot().await;
            assert_eq!(
                current.members().keys().collect::<Vec<_>>(),
                original.members().keys().collect::<Vec<_>>()
            );
            for (id, member) in current.members() {
                assert_eq!(
                    member.cert_fingerprint,
                    original.members()[id].cert_fingerprint
                );
            }
            assert!(current.tombstones().is_empty());
        }
    }

    fn assert_catchup_metrics(
        worker: &RunningWorker,
        expected: &[(crate::formation_metrics::CatchupEvent, u64)],
    ) {
        #[cfg(feature = "observability")]
        for event in crate::formation_metrics::CatchupEvent::ALL {
            assert_eq!(
                worker.formation_counters().unwrap().get_catchup(event),
                expected
                    .iter()
                    .find(|(item, _)| *item == event)
                    .map_or(0, |(_, value)| *value),
                "{event:?}"
            );
        }
        let _ = (worker, expected);
    }

    async fn exercise_catchup_lifecycle(case: CatchupCase) {
        use crate::formation_metrics::CatchupEvent as E;
        use orishu::model::cluster::{JoinOperationState, JoinRequest};
        let root = tempfile::tempdir().unwrap();
        let create = |name: &str| {
            WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join(name)).unwrap(),
                WorkerName::new(name).unwrap(),
                ClusterName::new("cluster").unwrap(),
                vec![],
            )
            .unwrap()
        };
        assert!(create("invalid").with_peer_admission(true).is_err());
        let start = |name| {
            let runtime = create(name)
                .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
                .unwrap()
                .with_peer_admission(true)
                .unwrap();
            #[cfg(feature = "observability")]
            let runtime = runtime.with_peer_metrics(true);
            runtime.start()
        };
        let (target, target_task) = start("target");
        let (source, source_task) = start("source");
        let target = std::sync::Arc::new(target);
        let source = std::sync::Arc::new(source);
        target.start_peer_maintenance();
        source.start_peer_maintenance();
        assert!(source.summary().unwrap().introducer_ready);
        let material = target.join_material().unwrap().await.unwrap().unwrap();
        assert!(material.introducer_ready);
        let request = JoinRequest {
            schema_version: 1,
            operation_id: "runtime-adoption".parse().unwrap(),
            formation_id: source.summary().unwrap().formation_id,
            material,
        };
        let disconnected = if matches!(
            case,
            CatchupCase::ReconnectBeforeAdmission
                | CatchupCase::ReconnectExhausted
                | CatchupCase::ReconnectShutdown
                | CatchupCase::ReconnectReplacement
        ) {
            if matches!(
                case,
                CatchupCase::ReconnectExhausted
                    | CatchupCase::ReconnectShutdown
                    | CatchupCase::ReconnectReplacement
            ) {
                // Hold scheduling at the real preparation boundary so each
                // cancellation/budget assertion is deterministic.
                let task = source
                    .join_reconnect_maintenance
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap();
                task.abort();
                let _ = task.await;
            } else {
                source.start_peer_maintenance(); // idempotent reconnect supervision
            }
            Some(target.handle.disconnect_next_join().await)
        } else {
            None
        };
        let lost_ack = if matches!(case, CatchupCase::LostAck) {
            Some(target.handle.lose_next_join_ack().await)
        } else {
            None
        };
        source.submit_join(request.clone()).await.unwrap();
        let original_assignment = if let Some(lost_ack) = lost_ack {
            Some(
                tokio::time::timeout(std::time::Duration::from_secs(3), lost_ack)
                    .await
                    .unwrap()
                    .unwrap(),
            )
        } else {
            None
        };
        let original_reference = if original_assignment.is_some() {
            let status = source
                .join_status(request.operation_id.clone())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.schema_version, 2);
            let reference = status.recovery_reference.unwrap();
            assert_eq!(
                reference.introducer_node_id,
                request.material.introducer_node_id
            );
            assert_eq!(
                reference.introducer_fingerprint,
                request.material.introducer_fingerprint
            );
            assert_eq!(
                reference.applicant_fingerprint,
                source.credentials.identity.fingerprint()
            );
            Some(reference)
        } else {
            None
        };
        if let Some(disconnected) = disconnected {
            let count = tokio::time::timeout(std::time::Duration::from_secs(3), disconnected)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(count, 1, "connection fault occurs before remote admission");
        }
        if matches!(
            case,
            CatchupCase::ReconnectShutdown | CatchupCase::ReconnectReplacement
        ) {
            let job = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    if let Some(job) = source
                        .handle
                        .prepare_join_reconnect()
                        .unwrap()
                        .await
                        .unwrap()
                    {
                        break job;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("reconnect preparation after connection retirement");
            let endpoint = source
                .outbound_endpoint
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .clone();
            let pending = source
                .dialer
                .handshake(
                    &endpoint,
                    &source.credentials.identity,
                    &job.local,
                    &job.target,
                    &source.handle.exchange_pool(),
                )
                .await
                .unwrap();
            // Real TLS/application handshake has succeeded, but the owner has
            // not registered the result or sent another credential-bearing JoinReq.
            if matches!(case, CatchupCase::ReconnectReplacement) {
                let connection = pending.observe_connection();
                assert!(connection.close_reason().is_none());
                let original = source.handle.view().unwrap();
                // Deliberately exercise the owner fence below the public API's
                // busy-operation guard; this does not authorize public leave
                // during an unresolved admission.
                source
                    .handle
                    .try_submit(
                        original.generation,
                        orishu_membership::Message::Local(orishu_membership::Command::Leave {
                            replacement_formation: "reconnect-replacement".parse().unwrap(),
                            replacement_node_id: "replacement-node".parse().unwrap(),
                            replacement_cluster_name: "replacement-cluster".parse().unwrap(),
                        }),
                    )
                    .unwrap();
                tokio::time::timeout(std::time::Duration::from_secs(1), async {
                    while source.handle.view().unwrap().generation == original.generation {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("owner lifecycle replacement");
                let new_request = JoinRequest {
                    schema_version: 1,
                    operation_id: "new-after-reconnect".parse().unwrap(),
                    formation_id: source.summary().unwrap().formation_id,
                    material: target.join_material().unwrap().await.unwrap().unwrap(),
                };
                let crate::driver::PreparedJoin::Start(new_job) = source
                    .handle
                    .prepare_join(new_request.clone())
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap()
                else {
                    panic!("fresh operation preparation");
                };
                let before = source.handle.view().unwrap();
                let old_status = source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap();
                // Neither replacing the lifecycle nor preparing another job
                // owns this still-unregistered connection.
                assert!(connection.close_reason().is_none());
                job.finish(pending);
                assert!(matches!(
                    tokio::time::timeout(std::time::Duration::from_secs(1), connection.closed())
                        .await
                        .expect("stale reconnect disposed"),
                    quinn::ConnectionError::LocallyClosed
                ));
                assert_eq!(
                    source
                        .join_status(new_request.operation_id.clone())
                        .await
                        .unwrap()
                        .unwrap()
                        .state,
                    JoinOperationState::Connecting
                );
                assert_eq!(
                    source
                        .join_status(request.operation_id.clone())
                        .await
                        .unwrap()
                        .unwrap(),
                    old_status
                );
                let after = source.handle.view().unwrap();
                assert_eq!(after.generation, before.generation);
                assert_eq!(after.summary, before.summary);
                assert_eq!(target.summary().unwrap().member_count, 1);
                drop(new_job);
                assert_eq!(
                    source
                        .join_status(new_request.operation_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .state,
                    JoinOperationState::FailedBeforeAdmission
                );
                source.shutdown().await.unwrap();
                target.shutdown().await.unwrap();
                assert_eq!(source_task.await.unwrap(), Ok(()));
                assert_eq!(target_task.await.unwrap(), Ok(()));
                return;
            }
            tokio::time::timeout(std::time::Duration::from_secs(2), source.shutdown())
                .await
                .expect("held reconnect completion does not block shutdown")
                .unwrap();
            assert_eq!(source_task.await.unwrap(), Ok(()));
            job.finish(pending);
            assert!(source.summary().is_err());
            assert_eq!(target.summary().unwrap().member_count, 1);
            target.shutdown().await.unwrap();
            assert_eq!(target_task.await.unwrap(), Ok(()));
            return;
        }
        if matches!(case, CatchupCase::ReconnectExhausted) {
            for index in 0..8 {
                let job = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    loop {
                        if let Some(job) = source
                            .handle
                            .prepare_join_reconnect()
                            .unwrap()
                            .await
                            .unwrap()
                        {
                            break job;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("bounded reconnect preparation");
                assert_eq!(job.target.formation, request.material.formation_id);
                assert_eq!(job.target.node, request.material.introducer_node_id);
                assert_eq!(
                    job.target.fingerprint,
                    request.material.introducer_fingerprint
                );
                assert!(
                    source
                        .handle
                        .prepare_join_reconnect()
                        .unwrap()
                        .await
                        .unwrap()
                        .is_none(),
                    "only one active reconnect job ({index})"
                );
                drop(job); // next control reply is ordered after the failure completion
            }
            assert!(
                source
                    .handle
                    .prepare_join_reconnect()
                    .unwrap()
                    .await
                    .unwrap()
                    .is_none()
            );
            source.start_join_reconnect_maintenance();
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            assert!(
                source
                    .handle
                    .prepare_join_reconnect()
                    .unwrap()
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(source.summary().unwrap().formation_id, request.formation_id);
            assert_eq!(target.summary().unwrap().member_count, 1);
            source.shutdown().await.unwrap();
            target.shutdown().await.unwrap();
            assert_eq!(source_task.await.unwrap(), Ok(()));
            assert_eq!(target_task.await.unwrap(), Ok(()));
            return;
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let status = source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap();
                if let JoinOperationState::CatchingUp { node_id } = status.state {
                    let summary = source.summary().unwrap();
                    assert_eq!(summary.source_node_id, node_id);
                    assert_eq!(summary.formation_id, request.material.formation_id);
                    assert!(!summary.introducer_ready);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("runtime join reaches adoption");
        if let Some(assigned) = original_assignment {
            assert_eq!(source.summary().unwrap().source_node_id, assigned);
            assert_eq!(
                target.summary().unwrap().member_count,
                2,
                "replayed acceptance must not insert another member"
            );
            let replay = source.submit_join(request.clone()).await.unwrap();
            assert_eq!(replay.recovery_reference, original_reference);
            let inspection = target
                .inspect_admission(orishu::model::cluster::AdmissionInspectionRequest {
                    schema_version: 1,
                    formation_id: request.material.formation_id.clone(),
                    reference: original_reference.unwrap(),
                })
                .await
                .unwrap();
            assert_eq!(
                inspection.outcome,
                orishu::model::cluster::AdmissionInspectionOutcome::CurrentMember {
                    node_id: assigned
                }
            );
            assert!(matches!(
                replay.state,
                JoinOperationState::CatchingUp { .. }
            ));
        }
        assert_eq!(target.summary().unwrap().member_count, 2);
        let locked = target
            .handle
            .set_membership_lock(
                target.handle.view().unwrap().generation,
                target.summary().unwrap().formation_id,
                true,
            )
            .unwrap()
            .await
            .unwrap();
        assert!(locked.is_ok());
        let converged = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !source.summary().unwrap().membership_locked {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(
            converged.is_ok(),
            "target {:?}; source {:?}",
            target.handle.view(),
            source.handle.view()
        );
        source.handle.disconnect_peers().await;
        target
            .handle
            .set_membership_lock(
                target.handle.view().unwrap().generation,
                target.summary().unwrap().formation_id,
                false,
            )
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let a = target.summary().unwrap();
                let b = source.summary().unwrap();
                if !a.membership_locked
                    && !b.membership_locked
                    && a.alive_count == 2
                    && b.alive_count == 2
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("connection loss recovers without admission or manual dials");
        if matches!(case, CatchupCase::MemberCollision) {
            exercise_member_collision(&target, &source).await;
        }
        assert_eq!(
            source.summary().unwrap().participation,
            Participation::CatchingUp
        );
        assert!(source.join_material().unwrap().await.unwrap().is_none());
        let cancelled = source
            .handle
            .prepare_catchup()
            .unwrap()
            .await
            .unwrap()
            .expect("registered source");
        assert!(
            source
                .handle
                .prepare_catchup()
                .unwrap()
                .await
                .unwrap()
                .is_none(),
            "one active catch-up job"
        );
        assert_catchup_metrics(&source, &[(E::Started, 1)]);
        let mut held = None;
        let mut cancelled = Some(cancelled);
        if matches!(
            case,
            CatchupCase::EjectCompleted
                | CatchupCase::LeaveCompleted
                | CatchupCase::ShutdownCompleted
                | CatchupCase::DisconnectCompleted
                | CatchupCase::ExpiredCompleted
        ) {
            let job = cancelled.take().unwrap();
            let pool = source.handle.exchange_pool();
            let (fetched, receive) = tokio::sync::oneshot::channel();
            let (release, resume) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                job.execute_paused(&pool, fetched, resume).await;
            });
            tokio::time::timeout(std::time::Duration::from_secs(5), receive)
                .await
                .expect("verified real transfer reaches completion barrier")
                .unwrap();
            assert_eq!(
                source.summary().unwrap().participation,
                Participation::CatchingUp
            );
            assert!(source.join_material().unwrap().await.unwrap().is_none());
            held = Some((release, task));
        }
        if matches!(case, CatchupCase::ExpiredCompleted) {
            let before = source.handle.view().unwrap();
            source.handle.expire_catchup().await;
            let (release, task) = held.take().unwrap();
            release.send(()).unwrap();
            task.await.unwrap();
            assert!(matches!(
                source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                JoinOperationState::CatchUpFailed { .. }
            ));
            let after = source.handle.view().unwrap();
            assert_catchup_metrics(
                &source,
                &[
                    (E::Started, 1),
                    (E::TransferValidated, 1),
                    (E::OwnerFenced, 1),
                ],
            );
            assert_eq!(after.generation, before.generation);
            assert_eq!(after.summary.formation_id, before.summary.formation_id);
            assert_eq!(after.summary.source_node_id, before.summary.source_node_id);
            assert_eq!(after.summary.participation, Participation::CatchingUp);
            assert!(!after.summary.introducer_ready);
            assert!(source.join_material().unwrap().await.unwrap().is_none());
            assert!(
                source
                    .handle
                    .prepare_catchup()
                    .unwrap()
                    .await
                    .unwrap()
                    .is_none()
            );
            source.shutdown().await.unwrap();
            target.shutdown().await.unwrap();
            assert_eq!(source_task.await.unwrap(), Ok(()));
            assert_eq!(target_task.await.unwrap(), Ok(()));
            return;
        }
        if matches!(case, CatchupCase::DisconnectCompleted) {
            let generation = source.handle.view().unwrap().generation;
            source.handle.disconnect_peers().await;
            assert_eq!(source.handle.view().unwrap().generation, generation);
            let (release, task) = held.take().unwrap();
            release.send(()).unwrap();
            task.await.unwrap();
            // The formation generation still matches, but the original
            // authorized session is gone. A replacement session is not enough.
            assert!(source.join_material().unwrap().await.unwrap().is_none());
            assert_eq!(
                source.summary().unwrap().participation,
                Participation::CatchingUp
            );
        }
        if matches!(
            case,
            CatchupCase::LeaveCompleted | CatchupCase::ShutdownCompleted
        ) {
            let before = source.handle.view().unwrap();
            let (release, task) = held.take().unwrap();
            if matches!(case, CatchupCase::ShutdownCompleted) {
                tokio::time::timeout(std::time::Duration::from_secs(2), source.shutdown())
                    .await
                    .expect("reserved completion cannot prevent shutdown")
                    .unwrap();
                assert_eq!(source_task.await.unwrap(), Ok(()));
                release.send(()).unwrap();
                task.await.unwrap();
                assert!(
                    source.summary().is_err(),
                    "late completion cannot revive owner"
                );
            } else {
                let receipt = source
                    .leave_operation(orishu::model::cluster::LeaveRequest {
                        schema_version: 1,
                        operation_id: "leave-before-catchup-completion".parse().unwrap(),
                        formation_id: before.summary.formation_id.clone(),
                    })
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert!(receipt.changed);
                assert_eq!(receipt.current.participation, Participation::Standalone);
                assert_ne!(receipt.current.formation_id, before.summary.formation_id);
                assert_ne!(
                    receipt.current.source_node_id,
                    before.summary.source_node_id
                );
                assert_ne!(source.handle.view().unwrap().generation, before.generation);
                let replacement = source.join_material().unwrap().await.unwrap().unwrap();
                assert!(replacement.token.expose() != request.material.token.expose());
                release.send(()).unwrap();
                task.await.unwrap();
                // Same FIFO control lane: this reply proves the late completion
                // was consumed, without sleeping or racing a watch projection.
                let after = source.join_material().unwrap().await.unwrap().unwrap();
                assert_eq!(source.summary().unwrap(), receipt.current);
                assert_eq!(after.formation_id, replacement.formation_id);
                assert!(after.token.expose() == replacement.token.expose());
                assert!(
                    source
                        .handle
                        .prepare_catchup()
                        .unwrap()
                        .await
                        .unwrap()
                        .is_none()
                );
                source.shutdown().await.unwrap();
                assert_eq!(source_task.await.unwrap(), Ok(()));
            }
            target.shutdown().await.unwrap();
            assert_eq!(target_task.await.unwrap(), Ok(()));
            assert_catchup_metrics(
                &source,
                &[
                    (E::Started, 1),
                    (E::TransferValidated, 1),
                    (
                        if matches!(case, CatchupCase::ShutdownCompleted) {
                            E::OwnerAbandoned
                        } else {
                            E::OwnerFenced
                        },
                        1,
                    ),
                ],
            );
            return;
        }
        if matches!(
            case,
            CatchupCase::EjectCancelled | CatchupCase::EjectCompleted
        ) {
            // Domain/owner boundary test: a validated baseline removes self
            // while the runtime still holds a reserved catch-up completion.
            // Ordinary gossip's serialized wire path is covered in registry.
            let before = source.handle.view().unwrap();
            let baseline = orishu_membership::AdmissionBaseline {
                schema_version: 1,
                formation_id: before.summary.formation_id.clone(),
                snapshot: 1_000_000,
                policy: None,
                blocklist: vec![],
                tombstones: vec![orishu_membership::MembershipTombstone {
                    node_id: before.summary.source_node_id.clone(),
                    name: "source".parse().unwrap(),
                    cert_fingerprint: source.credentials.identity.fingerprint(),
                    removal_mode: orishu_membership::RemovalMode::Force,
                    version: orishu_membership::testing::version(1_000_000),
                    cleared: false,
                    reason: None,
                }],
            };
            source
                .handle
                .try_submit(
                    before.generation,
                    orishu_membership::Message::Local(
                        orishu_membership::Command::InstallAdmissionBaseline(baseline),
                    ),
                )
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while source.summary().unwrap().participation != Participation::Ejected {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("baseline self-removal ejects during catch-up");
            assert_ne!(source.handle.view().unwrap().generation, before.generation);
            let ejected = source.handle.view().unwrap();
            if let Some((release, task)) = held {
                release.send(()).unwrap();
                task.await.unwrap();
            }
            drop(cancelled); // Its reserved old-generation completion arrives late.
            // FIFO barrier also covers the successful completion path.
            assert!(source.join_material().unwrap().await.unwrap().is_none());
            assert_catchup_metrics(
                &source,
                &[
                    (E::Started, 1),
                    (E::OwnerFenced, 1),
                    (
                        if matches!(case, CatchupCase::EjectCompleted) {
                            E::TransferValidated
                        } else {
                            E::TransferCancelled
                        },
                        1,
                    ),
                ],
            );
            assert_eq!(source.summary().unwrap(), ejected.summary);
            assert_eq!(
                source.handle.view().unwrap().transitions,
                ejected.transitions
            );
            source.start_catchup_maintenance();
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            assert_eq!(
                source.summary().unwrap().participation,
                Participation::Ejected
            );
            assert!(!source.summary().unwrap().introducer_ready);
            assert!(source.join_material().unwrap().await.unwrap().is_none());
            assert!(matches!(
                source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                JoinOperationState::CatchUpFailed { .. }
            ));
            source.shutdown().await.unwrap();
            target.shutdown().await.unwrap();
            assert_eq!(source_task.await.unwrap(), Ok(()));
            assert_eq!(target_task.await.unwrap(), Ok(()));
            return;
        }
        drop(cancelled);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let status = source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap();
                if matches!(status.state, JoinOperationState::CatchUpFailed { .. }) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled job has a retained failure outcome");
        if matches!(case, CatchupCase::Exhausted) {
            // The first failed preparation above also consumes the same
            // three-attempt budget. Dropping a job uses its real reserved lane.
            for _ in 0..2 {
                let job = source
                    .handle
                    .prepare_catchup()
                    .unwrap()
                    .await
                    .unwrap()
                    .expect("remaining bounded attempt");
                drop(job);
                assert!(matches!(
                    source
                        .join_status(request.operation_id.clone())
                        .await
                        .unwrap()
                        .unwrap()
                        .state,
                    JoinOperationState::CatchUpFailed { .. }
                ));
            }
            assert!(
                source
                    .handle
                    .prepare_catchup()
                    .unwrap()
                    .await
                    .unwrap()
                    .is_none()
            );
            source.start_catchup_maintenance();
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            assert!(
                source
                    .handle
                    .prepare_catchup()
                    .unwrap()
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(source.join_material().unwrap().await.unwrap().is_none());
            assert_eq!(
                source.summary().unwrap().formation_id,
                request.material.formation_id
            );
            assert_eq!(
                source.summary().unwrap().participation,
                Participation::CatchingUp
            );
            assert!(matches!(
                source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                JoinOperationState::CatchUpFailed { .. }
            ));
            source.shutdown().await.unwrap();
            target.shutdown().await.unwrap();
            assert_eq!(source_task.await.unwrap(), Ok(()));
            assert_eq!(target_task.await.unwrap(), Ok(()));
            assert_catchup_metrics(
                &source,
                &[
                    (E::Started, 3),
                    (E::TransferCancelled, 3),
                    (E::OwnerNotAdopted, 3),
                ],
            );
            return;
        }
        source.start_catchup_maintenance();
        source.start_catchup_maintenance(); // Starting twice must not duplicate jobs.
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while source.summary().unwrap().participation != Participation::Joined {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("owner installs verified state and credential");
        assert!(source.summary().unwrap().introducer_ready);
        assert!(matches!(
            source
                .join_status(request.operation_id.clone())
                .await
                .unwrap()
                .unwrap()
                .state,
            JoinOperationState::Joined { .. }
        ));
        let material = source.join_material().unwrap().await.unwrap().unwrap();
        let original = target.join_material().unwrap().await.unwrap().unwrap();
        assert!(material.introducer_ready);
        assert_eq!(material.token.expose(), original.token.expose());
        assert_catchup_metrics(
            &source,
            if matches!(case, CatchupCase::DisconnectCompleted) {
                &[
                    (E::Started, 2),
                    (E::TransferValidated, 2),
                    (E::OwnerFenced, 1),
                    (E::OwnerAdopted, 1),
                ]
            } else {
                &[
                    (E::Started, 2),
                    (E::TransferValidated, 1),
                    (E::TransferCancelled, 1),
                    (E::OwnerNotAdopted, 1),
                    (E::OwnerAdopted, 1),
                ]
            },
        );
        assert!(
            source
                .handle
                .prepare_catchup()
                .unwrap()
                .await
                .unwrap()
                .is_none()
        );
        source.shutdown().await.unwrap();
        target.shutdown().await.unwrap();
        assert_eq!(source_task.await.unwrap(), Ok(()));
        assert_eq!(target_task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn process_health_tracks_initialization_required_task_loss_and_shutdown() {
        use crate::health::{OwnerHealth, Readiness};
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            let root = tempfile::tempdir().unwrap();
            let (worker, owner) = WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join("worker")).unwrap(),
                WorkerName::new("health").unwrap(),
                ClusterName::new("health").unwrap(),
                vec![],
            )
            .unwrap()
            .start();
            let worker = std::sync::Arc::new(worker);
            assert!(!worker.health().startup_complete);
            assert_eq!(worker.health().readiness, Readiness::Initializing);
            let role = worker.required_role();
            let (started, running) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let _role = role;
                started.send(()).unwrap();
                std::future::pending::<()>().await;
            });
            running.await.unwrap();
            worker.mark_initialized();
            while worker.health().owner == OwnerHealth::Starting {
                tokio::task::yield_now().await;
            }
            assert_eq!(worker.health().readiness, Readiness::Ready);
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            let failed = worker.health();
            assert!(failed.startup_complete);
            assert!(failed.owner.is_responsive());
            assert_eq!(failed.readiness, Readiness::RequiredRoleUnavailable);
            worker.shutdown().await.unwrap();
            assert_eq!(owner.await.unwrap(), Ok(()));
            let stopped = worker.health();
            assert!(stopped.startup_complete);
            assert_eq!(stopped.owner, OwnerHealth::Closed);
            assert_eq!(stopped.readiness, Readiness::Stopping);
        })
        .await
        .expect("process health lifecycle deadline");
    }

    #[tokio::test]
    async fn late_successful_initial_handshake_is_closed_after_leave_or_shutdown() {
        use crate::driver::PreparedJoin;
        use crate::peer::dial::IntroducerTarget;
        use orishu::model::cluster::{JoinOperationState, JoinRequest};
        use orishu_membership::{Command, Message};
        use std::time::Duration;

        tokio::time::timeout(Duration::from_secs(15), async {
            for stop in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let create = |name: &str| {
                    WorkerRuntime::standalone(
                        WorkerCredentials::load_or_create(&root.path().join(name)).unwrap(),
                        WorkerName::new(name).unwrap(),
                        ClusterName::new("cluster").unwrap(),
                        vec![],
                    )
                    .unwrap()
                };
                let (target, target_task) = create("target")
                    .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
                    .unwrap()
                    .with_peer_admission(true)
                    .unwrap()
                    .start();
                let (source, source_task) = create("source").start();
                let initial = source.handle.view().unwrap();
                let request = JoinRequest {
                    schema_version: 1,
                    operation_id: "late-handshake".parse().unwrap(),
                    formation_id: initial.summary.formation_id.clone(),
                    material: target.join_material().unwrap().await.unwrap().unwrap(),
                };
                let PreparedJoin::Start(job) = source
                    .handle
                    .prepare_join(request.clone())
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap()
                else {
                    panic!("new join preparation");
                };
                let endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
                let pending = source
                    .dialer
                    .handshake(
                        &endpoint,
                        &source.credentials.identity,
                        job.local(),
                        &IntroducerTarget::try_from(job.material()).unwrap(),
                        &source.handle.exchange_pool(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    endpoint.open_connections(),
                    1,
                    "hold an actual successful handshake"
                );
                let observed = pending.observe_connection();
                if stop {
                    source.shutdown().await.unwrap();
                    source.handle.closed().await;
                } else {
                    source
                        .handle
                        .try_submit(
                            initial.generation,
                            Message::Local(Command::Leave {
                                replacement_formation: "replacement".parse().unwrap(),
                                replacement_node_id: "replacement-node".parse().unwrap(),
                                replacement_cluster_name: "replacement-cluster".parse().unwrap(),
                            }),
                        )
                        .unwrap();
                    tokio::time::timeout(Duration::from_secs(1), async {
                        while source.summary().unwrap().formation_id == initial.summary.formation_id
                        {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .expect("owner completes lifecycle replacement");
                }
                // Deliver genuine TLS/handshake IO through the original reserved
                // completion only after the lifecycle has ended. Never close the
                // endpoint from the test to manufacture successful disposal.
                job.finish(pending);
                assert!(matches!(
                    tokio::time::timeout(Duration::from_secs(1), observed.closed())
                        .await
                        .expect("stale handshake closes promptly"),
                    quinn::ConnectionError::LocallyClosed
                ));
                tokio::time::timeout(Duration::from_secs(3), endpoint.wait_idle())
                    .await
                    .expect("stale handshake connection must be closed by completion disposal");
                assert_eq!(endpoint.open_connections(), 0);
                assert_eq!(target.summary().unwrap().member_count, 1);
                if !stop {
                    let status = source
                        .join_status(request.operation_id)
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(status.state, JoinOperationState::FailedBeforeAdmission);
                    let view = source.summary().unwrap();
                    assert_eq!(view.formation_id.as_str(), "replacement");
                    assert_eq!(view.source_node_id.as_str(), "replacement-node");
                    assert_eq!(view.participation, Participation::Standalone);
                    source.shutdown().await.unwrap();
                }
                target.shutdown().await.unwrap();
                assert_eq!(source_task.await.unwrap(), Ok(()));
                assert_eq!(target_task.await.unwrap(), Ok(()));
            }
        })
        .await
        .expect("late initial-handshake lifecycle fixture deadline");
    }

    #[tokio::test]
    async fn runtime_owns_join_dial_and_retains_failure_for_exact_replay() {
        use orishu::model::cluster::{JoinOperationState, JoinRequest};
        let root = tempfile::tempdir().unwrap();
        let create = |name: &str| {
            WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&root.path().join(name)).unwrap(),
                WorkerName::new(name).unwrap(),
                ClusterName::new("cluster").unwrap(),
                vec![],
            )
            .unwrap()
        };
        let (target, target_task) = create("target")
            .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
            .unwrap()
            .start();
        let (source, source_task) = create("source").start();
        let source = std::sync::Arc::new(source);
        assert!(source.outbound_endpoint.lock().unwrap().is_none());
        let mut material = target.join_material().unwrap().await.unwrap().unwrap();
        material.introducer_fingerprint = source.credentials.identity.fingerprint();
        let request = JoinRequest {
            schema_version: 1,
            operation_id: "runtime-dial".parse().unwrap(),
            formation_id: source.summary().unwrap().formation_id,
            material,
        };
        let receipt = source.submit_join(request.clone()).await.unwrap();
        assert_eq!(receipt.state, JoinOperationState::Connecting);
        tokio::time::timeout(std::time::Duration::from_secs(4), async {
            loop {
                if source
                    .join_status(request.operation_id.clone())
                    .await
                    .unwrap()
                    .unwrap()
                    .state
                    == JoinOperationState::FailedBeforeAdmission
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("bad TLS pin finishes owned job without admission");
        assert_eq!(
            source.submit_join(request).await.unwrap().state,
            JoinOperationState::FailedBeforeAdmission
        );
        assert_eq!(target.summary().unwrap().member_count, 1);
        assert_eq!(
            source.summary().unwrap().participation,
            Participation::Standalone
        );
        assert!(source.outbound_endpoint.lock().unwrap().is_some());
        source.shutdown().await.unwrap();
        target.shutdown().await.unwrap();
        assert_eq!(source_task.await.unwrap(), Ok(()));
        assert_eq!(target_task.await.unwrap(), Ok(()));
        assert!(source.join_jobs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unexpected_peer_listener_exit_still_fails_and_stops_owner() {
        let (owner, task) = crate::driver::spawn_standalone(
            orishu_membership::testing::standalone("listener-loss"),
        );
        let observer = owner.clone();
        assert_eq!(
            finish_after_peer_listener(owner, task).await,
            Err(crate::driver::DriverError::PeerAdapterUnavailable)
        );
        assert!(observer.view().is_err());
    }

    #[tokio::test]
    async fn peer_listener_exit_during_acknowledged_shutdown_preserves_owner_success() {
        use std::{future::Future, task::Poll};
        let (owner, task) =
            crate::driver::spawn_standalone(orishu_membership::testing::standalone("shutdown"));
        let (release, held) = tokio::sync::oneshot::channel();
        owner.hold_shutdown(held).await;
        owner.shutdown().await.unwrap();
        assert_eq!(
            owner.view().unwrap().summary.participation,
            Participation::Stopping
        );
        // Poll the production listener-exit branch while the real owner has
        // acknowledged shutdown but cannot yet drop its published view.
        let completion = finish_after_peer_listener(owner, task);
        tokio::pin!(completion);
        std::future::poll_fn(|cx| {
            assert!(completion.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        release.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), completion)
                .await
                .unwrap(),
            Ok(())
        );
    }

    #[tokio::test]
    async fn explicit_peer_bind_advertises_actual_port_and_follows_owner_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = WorkerRuntime::standalone(
            WorkerCredentials::load_or_create(&directory.path().join("state")).unwrap(),
            WorkerName::new("worker").unwrap(),
            ClusterName::new("cluster").unwrap(),
            vec![],
        )
        .unwrap()
        .bind_peer("127.0.0.1:0".parse().unwrap(), None, None)
        .unwrap();
        let address = runtime
            .peer_endpoint
            .as_ref()
            .unwrap()
            .local_addr()
            .unwrap();
        assert_ne!(address.port(), 0);
        assert_eq!(
            runtime.membership.local().endpoints.peers[0].0,
            address.to_string()
        );
        assert!(!runtime.membership.local().accepts.peers);
        let applicant = crate::peer::tls::PeerIdentity::generate().unwrap();
        let client_config = applicant
            .client_config(runtime.credentials.identity.certificate())
            .unwrap();
        let (worker, task) = runtime.start();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            let connection = client
                .connect_with(client_config, address, crate::peer::tls::SERVER_NAME)
                .unwrap()
                .await
                .unwrap();
            worker.shutdown().await.unwrap();
            assert_eq!(task.await.unwrap(), Ok(()));
            connection.closed().await;
        })
        .await
        .unwrap();
        client.close(0_u32.into(), b"test complete");
    }

    #[test]
    fn restart_keeps_certificate_but_never_restores_formation_or_node() {
        let directory = tempfile::tempdir().unwrap();
        let start = || {
            WorkerRuntime::standalone(
                WorkerCredentials::load_or_create(&directory.path().join("state")).unwrap(),
                WorkerName::new("same-name").unwrap(),
                ClusterName::new("same-label").unwrap(),
                vec![],
            )
            .unwrap()
        };
        let first = start();
        let initial = first.summary();
        let cert = first.fingerprint();
        drop(first);
        let second = start();
        let restarted = second.summary();
        assert_eq!(cert, second.fingerprint());
        assert_ne!(initial.formation_id, restarted.formation_id);
        assert_ne!(initial.source_node_id, restarted.source_node_id);
        assert_eq!(initial.cluster_name, restarted.cluster_name);
        assert_eq!(restarted.member_count, 1);
        assert_eq!(restarted.participation, Participation::Standalone);
        assert!(!restarted.introducer_ready);
        assert!(!restarted.membership_locked);
        assert_eq!(restarted.workload, FormationWorkload::None);
    }
}
