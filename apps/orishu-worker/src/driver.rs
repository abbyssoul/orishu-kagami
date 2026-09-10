//! Serialized membership owner. IO handlers submit bounded, generation-stamped
//! inputs; readers consume a published projection without locking membership.

use crate::{credentials::SecretToken, runtime::project_summary};
use orishu::model::cluster::{
    LeaveReceipt, LeaveRequest, LockReceipt, LockRequest, OperationId, Participation, Summary,
};
use orishu_membership::{
    Command, Effect, EffectOutcome, Liveness, Membership, Message, NodeId, TimerToken,
};
use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::Instant,
};

const MAILBOX_CAPACITY: usize = 64;
const CONTROL_CAPACITY: usize = 16;
const MAX_TIMERS: usize = 512;
const MAX_INLINE_TRANSITIONS: usize = 128;
const MAX_LOCK_OPERATIONS: usize = 1024;
const MAX_LEAVE_OPERATIONS: usize = 64;
const MAX_RELIABLE_SENDS: usize = 64;

type SendCompletion = (
    Generation,
    orishu_membership::SessionId,
    Result<Vec<u8>, crate::peer::exchange::ExchangeError>,
);

/// One process-local generation, advanced when formation identity changes.
/// It also fences core counters that restart from zero after adoption/leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Generation(pub u64);

/// Bounded reader projection, independent of exporter and network dependencies.
#[derive(Debug, Clone)]
pub struct View {
    /// Last tick actually processed by the serialized owner; never refreshed by
    /// reads or exporters. Absent until the owner starts running.
    pub owner_progress: Option<Instant>,
    /// Completed reliable transport replies; not a count of accepted commands.
    pub completed_exchanges: u64,
    /// Missing routes, overloaded sends and transport failures (not delivery ACKs).
    pub failed_sends: u64,
    /// Membership packets refused by owner-side session binding or decoding.
    pub peer_decode_rejections: u64,
    /// Decoded formation traffic, not accepted transitions or convergence.
    pub traffic: TrafficCounters,
    /// Current local formation projection.
    pub summary: Summary,
    /// Generation required on asynchronous input/outcomes.
    pub generation: Generation,
    /// Completed core transitions, for progress monitoring.
    pub transitions: u64,
    /// Rejected old-generation deliveries.
    pub stale_inputs: u64,
    /// Count of structured core diagnostics (payloads are not retained here).
    pub diagnostics: u64,
    /// Foreign gossip handed off without a workload owner in this PoC.
    pub foreign_gossip: u64,
    /// Issuer-side admission outcomes, retained across formation changes.
    pub admissions: AdmissionCounters,
}

/// Bounded issuer-side event counts, without applicant identities or reasons.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionCounters {
    /// New members inserted by local core admission, even if the reply is lost.
    pub accepted: u64,
    /// Local core admission refusals; excludes pre-core transport/codec failures.
    pub rejected: u64,
    /// Validated retained assignments encoded and rebound, not delivered replies.
    pub assignment_replays: u64,
}

/// Constant-size received activity counts, retained for the process lifetime.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TrafficCounters {
    /// Decoded Ping/Ack/PingReq/PingReply/Announce packets, including repeats.
    pub swim: u64,
    /// Decoded PullRequest/PullReply packets, not completed reconciliation rounds.
    pub anti_entropy: u64,
    /// Envelope gossip plus PullReply deltas, including duplicate/ignored items.
    pub gossip_items: u64,
}

impl TrafficCounters {
    fn record(&mut self, input: &orishu_membership::PeerInput) {
        use orishu_membership::PeerBody;
        self.gossip_items = self
            .gossip_items
            .saturating_add(input.context.gossip.len() as u64);
        match &input.body {
            PeerBody::Ping { .. }
            | PeerBody::Ack { .. }
            | PeerBody::PingReq { .. }
            | PeerBody::PingReply { .. }
            | PeerBody::Announce { .. } => self.swim = self.swim.saturating_add(1),
            PeerBody::PullRequest { .. } => {
                self.anti_entropy = self.anti_entropy.saturating_add(1);
            }
            PeerBody::PullReply { deltas, .. } => {
                self.anti_entropy = self.anti_entropy.saturating_add(1);
                self.gossip_items = self.gossip_items.saturating_add(deltas.len() as u64);
            }
            PeerBody::JoinRequest { .. } | PeerBody::JoinReply(_) => {}
        }
    }
}

/// Process-lifetime, saturating owner counters. Copies contain no identities,
/// credentials or collections; reads do not schedule or refresh owner work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerCounters {
    /// Decoded receive activity; independent of core acceptance.
    pub traffic: TrafficCounters,
    /// Membership packets refused by owner-side session binding or decoding;
    /// excludes pre-enqueue, lifecycle, handshake and catch-up failures.
    pub peer_decode_rejections: u64,
    /// Completed core transitions, including periodic/control work.
    pub transitions: u64,
    /// Inputs rejected by lifecycle-generation fencing.
    pub stale_inputs: u64,
    /// Structured core diagnostic outcomes, without diagnostic payloads.
    pub diagnostics: u64,
    /// Foreign gossip items handed off without a workload owner.
    pub foreign_gossip: u64,
    /// Completed reliable replies, not accepted domain operations.
    pub completed_exchanges: u64,
    /// Missing routes, send overload and transport failures.
    pub failed_sends: u64,
    /// Issuer-side admission counts; each saturates at `u64::MAX`.
    pub admissions: AdmissionCounters,
}

/// Occupancy of one bounded owner lane, including reserved capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanePressure {
    /// Queued messages plus outstanding reserved permits, not running tasks.
    pub slots_in_use: usize,
    /// Configured maximum number of slots in this lane.
    pub capacity: usize,
}

/// Constant-size local lane readings; different lanes are sampled separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerPressure {
    /// Decoded peer input lane.
    pub peer: LanePressure,
    /// Operator/control lane, including reserved control completions.
    pub control: LanePressure,
    /// Effect outcome lane, including outstanding verification permits.
    pub completion: LanePressure,
    /// Independently reserved shutdown lane.
    pub shutdown: LanePressure,
}

/// A rejected submission or terminal owner failure. No peer/secret text escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DriverError {
    /// The bounded ingress queue is full; the caller must report overload.
    #[error("membership owner is overloaded")]
    Overloaded,
    /// The owner stopped; readers must not present its last view as fresh.
    #[error("membership owner stopped")]
    Closed,
    /// A requested effect lacks a valid session or formation credential.
    #[error("membership peer session or credential is unavailable")]
    PeerAdapterUnavailable,
    /// A bounded internal work/timer/generation budget was exhausted.
    #[error("membership owner internal limit exceeded")]
    Limit,
    /// Entropy failed; the owner does not substitute predictable randomness.
    #[error("membership owner entropy source failed")]
    Entropy,
}

struct Input {
    generation: Generation,
    message: Message,
}

// Fixed 64-slot peer lane retains inline core input, without per-message boxing.
#[allow(clippy::large_enum_variant)]
enum PeerDelivery {
    Core(Input),
    Packet {
        generation: Generation,
        session: orishu_membership::SessionId,
        transport: crate::peer::wire::Transport,
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<Option<crate::peer::wire::Encoded>, PacketError>>,
    },
    Handshake {
        mode: crate::peer::registry::HandshakeMode,
        generation: Generation,
        connection: PendingConnection,
        bytes: Vec<u8>,
        reply: oneshot::Sender<
            Result<crate::peer::registry::HandshakeReply, crate::peer::registry::RegistryError>,
        >,
    },
}

/// Packet delivery errors contain no peer payload or credential data.
#[derive(Debug, thiserror::Error)]
pub enum PacketError {
    #[error(transparent)]
    Session(#[from] crate::peer::registry::RegistryError),
    #[error(transparent)]
    Owner(#[from] DriverError),
}

struct PacketContext {
    #[cfg(feature = "otlp-tracing")]
    admission_outcome: crate::trace_export::Outcome,
    join_attempt: Option<(
        orishu_membership::CertFingerprint,
        crate::peer::wire::JoinAttempt,
    )>,
    member: Option<NodeId>,
    token: Option<SecretToken>,
    generation: Generation,
    session: orishu_membership::SessionId,
    transport: crate::peer::wire::Transport,
    response: Option<crate::peer::wire::Encoded>,
}

// Registry validation must precede this diagnostic gate. It never authorizes
// admission or replaces the membership core's credential/policy checks.
#[cfg(feature = "otlp-tracing")]
fn admission_trace_parent(
    expected: Option<&SecretToken>,
    presented: Option<&SecretToken>,
    parent: Option<crate::trace_context::TraceParent>,
) -> Option<crate::trace_context::TraceParent> {
    parent.filter(|_| {
        expected.is_some_and(|expected| {
            presented.is_some_and(|presented| expected.matches(presented.expose()))
        })
    })
}

struct JoinTransport {
    parent: Option<crate::trace_context::TraceParent>,
    attempt_id: NodeId,
    emitted: bool,
    session: orishu_membership::SessionId,
    target: orishu_membership::FormationId,
    token: SecretToken,
    route: Option<crate::peer::dial::IntroducerTarget>,
    reconnect_attempts: u8,
    reconnect_pending: Option<u8>,
}

/// Secret-free, single-use reconnect job authorized by the current join owner.
/// Cancellation reports failure through a slot reserved before any dial work.
pub(crate) struct JoinReconnect {
    generation: Generation,
    previous_session: orishu_membership::SessionId,
    id: u8,
    pub local: orishu_membership::LocalIdentity,
    pub target: crate::peer::dial::IntroducerTarget,
    completion: Option<mpsc::OwnedPermit<Control>>,
}

impl JoinReconnect {
    pub fn finish(mut self, pending: crate::peer::dial::PendingHandshake) {
        self.send(Some(pending));
    }

    fn send(&mut self, pending: Option<crate::peer::dial::PendingHandshake>) {
        if let Some(permit) = self.completion.take() {
            send_reserved_control(
                permit,
                Control::JoinReconnectFinished {
                    generation: self.generation,
                    previous_session: self.previous_session,
                    id: self.id,
                    pending,
                },
            );
        }
    }
}

impl Drop for JoinReconnect {
    fn drop(&mut self) {
        self.send(None);
    }
}

struct CatchupAttempt {
    id: u64,
    generation: Generation,
    session: orishu_membership::SessionId,
    source: NodeId,
}

/// A single owner-authorized catch-up transfer. Its reserved completion slot
/// reports failure on cancellation; no caller can fabricate a Completed value.
pub struct CatchupPreparation {
    id: u64,
    generation: Generation,
    binding: crate::peer::catchup::client::Binding,
    connection: quinn::Connection,
    completion: Option<mpsc::OwnedPermit<Control>>,
    observation: Option<crate::formation_metrics::CatchupObservation>,
}
impl CatchupPreparation {
    /// Run bounded receiver IO without holding membership state or owner locks.
    pub async fn execute(mut self, pool: &crate::peer::exchange::ExchangePool) {
        let completed = crate::peer::catchup::client::fetch_observed(
            &self.connection,
            &self.binding,
            pool,
            self.observation.as_mut().expect("one observation per job"),
        )
        .await;
        self.send(completed.ok());
    }
    /// Test-only scheduling barrier after verified real wire IO, before the
    /// reserved owner completion. No fabricated baseline or credential enters.
    #[cfg(test)]
    pub(crate) async fn execute_paused(
        mut self,
        pool: &crate::peer::exchange::ExchangePool,
        fetched: oneshot::Sender<()>,
        release: oneshot::Receiver<()>,
    ) {
        let completed = crate::peer::catchup::client::fetch_observed(
            &self.connection,
            &self.binding,
            pool,
            self.observation.as_mut().unwrap(),
        )
        .await
        .expect("real catch-up transfer succeeds before lifecycle interruption");
        fetched.send(()).expect("test awaits verified transfer");
        tokio::time::timeout(Duration::from_secs(5), release)
            .await
            .expect("test releases completion within a bounded deadline")
            .expect("test releases successful completion");
        self.send(Some(completed));
    }
    fn send(&mut self, completed: Option<crate::peer::catchup::client::Completed>) {
        if let Some(permit) = self.completion.take() {
            let mut observation = self
                .observation
                .take()
                .expect("one observation per completion");
            observation.cancel_if_pending();
            send_reserved_control(
                permit,
                Control::CatchupFinished {
                    id: self.id,
                    generation: self.generation,
                    completed,
                    observation,
                },
            );
        }
    }
}
impl Drop for CatchupPreparation {
    fn drop(&mut self) {
        self.send(None);
    }
}

/// Exact replay never starts another transport operation.
pub enum PreparedJoin {
    Replay(Box<orishu::model::cluster::JoinOperation>),
    Start(Box<JoinPreparation>),
}

/// Single-use shell job with guaranteed completion delivery. Contains secret
/// input deliberately excluded from Debug and from retained operation records.
pub struct JoinPreparation {
    parent: Option<crate::trace_context::TraceParent>,
    operation: orishu::model::cluster::JoinOperation,
    generation: Generation,
    local: orishu_membership::LocalIdentity,
    request: Option<orishu::model::cluster::JoinRequest>,
    completion: Option<mpsc::OwnedPermit<Control>>,
}
impl JoinPreparation {
    /// Initial processing receipt, not peer admission.
    pub fn operation(&self) -> &orishu::model::cluster::JoinOperation {
        &self.operation
    }
    /// Immutable local handshake identity captured at reservation.
    pub fn local(&self) -> &orishu_membership::LocalIdentity {
        &self.local
    }
    /// Validated, operator-supplied material; the dial adapter copies no token.
    pub fn material(&self) -> &orishu::model::cluster::JoinMaterial {
        &self.request.as_ref().expect("live preparation").material
    }
    /// Complete pinned IO; the owner rechecks lifecycle and ACK before joining.
    pub fn finish(mut self, pending: crate::peer::dial::PendingHandshake) {
        self.send(Some(pending));
    }
    fn send(&mut self, pending: Option<crate::peer::dial::PendingHandshake>) {
        if let Some(permit) = self.completion.take() {
            send_reserved_control(
                permit,
                Control::JoinPrepared {
                    parent: self.parent,
                    generation: self.generation,
                    request: self.request.take().expect("single completion"),
                    pending,
                },
            );
        }
    }
}
impl Drop for JoinPreparation {
    fn drop(&mut self) {
        self.send(None);
    }
}

/// Close queued connections if shutdown drops their mailbox before registration.
struct PendingConnection(Option<quinn::Connection>);

impl Drop for PendingConnection {
    fn drop(&mut self) {
        if let Some(connection) = &self.0 {
            connection.close(0_u32.into(), b"handshake abandoned");
        }
    }
}

#[cfg(any(test, feature = "formation-fault-test"))]
#[derive(Clone, Copy)]
enum JoinFault {
    LoseAck,
    #[cfg(feature = "formation-fault-test")]
    Crash,
    #[cfg(feature = "formation-fault-test")]
    Remove,
    #[cfg(feature = "formation-fault-test")]
    Block,
}

// The lane already held inline Input values and remains capped at 16 slots.
// Keep that fixed memory budget instead of allocating a box per local command.
#[allow(clippy::large_enum_variant)]
enum Control {
    Completed(std::sync::Arc<std::sync::Mutex<Option<Control>>>),
    #[cfg(test)]
    ModelSnapshot(oneshot::Sender<Membership>),
    #[cfg(test)]
    TimerSnapshot(oneshot::Sender<Vec<(orishu_membership::TimerToken, Instant)>>),
    #[cfg(test)]
    ObserveSendCompletion {
        result: SendCompletion,
        processed: oneshot::Sender<View>,
    },
    #[cfg(feature = "formation-fault-test")]
    DropNextDeparture(oneshot::Sender<()>),
    #[cfg(feature = "formation-fault-test")]
    EjectPeerFixture(oneshot::Sender<Result<NodeId, DriverError>>),
    #[cfg(feature = "formation-fault-test")]
    HoldProgress {
        release: oneshot::Receiver<()>,
        ready: oneshot::Sender<()>,
    },
    #[cfg(feature = "formation-fault-test")]
    CorruptCatchupPage {
        index: u16,
        observed: oneshot::Sender<()>,
        ready: oneshot::Sender<()>,
    },
    #[cfg(any(test, feature = "formation-fault-test"))]
    HoldShutdown {
        release: oneshot::Receiver<()>,
        ready: oneshot::Sender<()>,
    },
    #[cfg(any(test, feature = "formation-fault-test"))]
    LoseNextJoinAck {
        observed: oneshot::Sender<NodeId>,
        fault: JoinFault,
        reply: oneshot::Sender<()>,
    },
    PrepareJoinReconnect {
        completion: mpsc::OwnedPermit<Control>,
        reply: oneshot::Sender<Option<JoinReconnect>>,
    },
    JoinReconnectFinished {
        generation: Generation,
        previous_session: orishu_membership::SessionId,
        id: u8,
        pending: Option<crate::peer::dial::PendingHandshake>,
    },
    #[cfg(test)]
    DisconnectNextJoin {
        disconnected: oneshot::Sender<usize>,
        reply: oneshot::Sender<()>,
    },
    LeaveOperation {
        generation: Generation,
        request: LeaveRequest,
        reply: oneshot::Sender<Result<LeaveReceipt, LeaveError>>,
    },
    PrepareCatchup {
        completion: mpsc::OwnedPermit<Control>,
        reply: oneshot::Sender<Option<CatchupPreparation>>,
    },
    CatchupFinished {
        id: u64,
        generation: Generation,
        completed: Option<crate::peer::catchup::client::Completed>,
        observation: crate::formation_metrics::CatchupObservation,
    },
    #[cfg(test)]
    DisconnectPeers(oneshot::Sender<()>),
    #[cfg(test)]
    ExpireCatchup(oneshot::Sender<()>),
    MemberDial {
        after: Option<NodeId>,
        reply: oneshot::Sender<MemberDialPage>,
    },
    Input(Input),
    PrepareJoin {
        parent: Option<crate::trace_context::TraceParent>,
        request: orishu::model::cluster::JoinRequest,
        completion: mpsc::OwnedPermit<Control>,
        reply: oneshot::Sender<Result<PreparedJoin, crate::join_operations::OperationError>>,
    },
    JoinPrepared {
        parent: Option<crate::trace_context::TraceParent>,
        generation: Generation,
        request: orishu::model::cluster::JoinRequest,
        pending: Option<crate::peer::dial::PendingHandshake>,
    },
    JoinStatus {
        id: OperationId,
        reply: oneshot::Sender<Option<orishu::model::cluster::JoinOperation>>,
    },
    InspectAdmission {
        request: orishu::model::cluster::AdmissionInspectionRequest,
        reply: oneshot::Sender<orishu::model::cluster::AdmissionInspection>,
    },
    BeginJoin {
        parent: Option<crate::trace_context::TraceParent>,
        generation: Generation,
        source: orishu_membership::FormationId,
        target: orishu_membership::FormationId,
        session: orishu_membership::SessionId,
        token: SecretToken,
        reply: oneshot::Sender<Result<(), DriverError>>,
    },
    JoinMaterial(oneshot::Sender<Option<orishu::model::cluster::JoinMaterial>>),
    List {
        formation: Option<orishu_membership::FormationId>,
        after: Option<orishu_membership::NodeId>,
        reply: oneshot::Sender<Option<orishu::model::node::MembershipPage>>,
    },
    Inspect {
        node: orishu_membership::NodeId,
        reply: oneshot::Sender<Option<orishu::model::node::Inspection>>,
    },
    LockOperation {
        generation: Generation,
        request: LockRequest,
        reply: oneshot::Sender<Result<LockReceipt, LockError>>,
    },
    SetLock {
        generation: Generation,
        formation: orishu_membership::FormationId,
        locked: bool,
        reply: oneshot::Sender<Result<Summary, LockError>>,
    },
}

/// An owned permit may enqueue after receiver destruction. Keep a disposal
/// handle until after publication: if closure preceded the send, take and drop
/// the value here; otherwise receiver processing/drop owns it. Never release
/// the reserved slot and race a fresh try_send for correctness-bearing work.
fn send_reserved_control(permit: mpsc::OwnedPermit<Control>, value: Control) {
    let shared = std::sync::Arc::new(std::sync::Mutex::new(Some(value)));
    let sender = permit.send(Control::Completed(shared.clone()));
    if sender.is_closed() {
        shared.lock().expect("completion disposal lock").take();
    }
}

pub(crate) struct MemberDialPlan {
    pub node: NodeId,
    pub local: orishu_membership::LocalIdentity,
    pub target: crate::peer::dial::MemberTarget,
}

pub(crate) struct MemberDialPage {
    pub generation: Generation,
    pub after: Option<NodeId>,
    pub plan: Option<MemberDialPlan>,
}

/// Outcome of an authorized local policy command, not cluster convergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LockError {
    #[error("operation ID was already used for a different intent")]
    Conflict,
    #[error("formation-local operation history is full")]
    HistoryFull,
    #[error("unsupported lock request schema")]
    InvalidSchema,
    #[error("formation precondition is stale")]
    StaleFormation,
    #[error("worker cannot change policy in its current participation state")]
    Unavailable,
    #[error("membership policy transition was rejected")]
    Rejected,
    #[error(transparent)]
    Driver(#[from] DriverError),
}

/// Secret-free refusal of an authorized leave intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LeaveError {
    #[error("unsupported leave request schema")]
    InvalidSchema,
    #[error("leave operation ID names a different intent")]
    Conflict,
    #[error("leave operation history is full")]
    HistoryFull,
    #[error("leave source formation is stale")]
    StaleFormation,
    #[error("departure requires a resolved admission outcome and an available worker")]
    Unavailable,
    #[error(transparent)]
    Driver(#[from] DriverError),
}

/// Cheap IO-side handle. Only the owner task can mutate `Membership`.
#[derive(Clone)]
pub struct Handle {
    #[cfg(feature = "otlp-tracing")]
    tracing: std::sync::Arc<std::sync::OnceLock<crate::trace_export::SpanQueue>>,
    #[cfg(feature = "observability")]
    formation_metrics: crate::formation_metrics::FormationMetrics,
    exchanges: crate::peer::exchange::ExchangePool,
    packet_io: crate::peer::traffic::Traffic,
    input: mpsc::Sender<PeerDelivery>,
    control: mpsc::Sender<Control>,
    completion: mpsc::Sender<Input>,
    shutdown: mpsc::Sender<oneshot::Sender<()>>,
    view: watch::Receiver<View>,
    formation_adopted: std::sync::Arc<tokio::sync::Notify>,
    first_catchup_route: std::sync::Arc<tokio::sync::Notify>,
}

impl Handle {
    /// Coalesced IO wake-up only; the owner revalidates every resulting plan.
    pub(crate) async fn formation_adopted(&self) {
        self.formation_adopted.notified().await;
    }

    /// Wake the first catch-up when a current admitted route is registered.
    /// Subsequent failed attempts retain the periodic retry cadence.
    pub(crate) async fn first_catchup_route(&self) {
        self.first_catchup_route.notified().await;
    }

    /// Install optional diagnostics once during startup. Cannot replace a live
    /// queue or mutate membership authority; false means already configured.
    #[cfg(feature = "otlp-tracing")]
    pub(crate) fn install_trace_queue(&self, queue: crate::trace_export::SpanQueue) -> bool {
        self.tracing.set(queue).is_ok()
    }

    /// Optional owner deadline/abandonment observations, without owner requests.
    #[cfg(feature = "observability")]
    pub fn formation_counters(&self) -> Option<crate::formation_metrics::Snapshot> {
        self.formation_metrics.snapshot()
    }

    /// Read bounded channel occupancy without locks on domain state or owner
    /// work. Reserved permits count as occupied even before their message is sent.
    pub fn pressure(&self) -> OwnerPressure {
        fn lane<T>(sender: &mpsc::Sender<T>) -> LanePressure {
            let capacity = sender.max_capacity();
            LanePressure {
                slots_in_use: capacity.saturating_sub(sender.capacity()),
                capacity,
            }
        }
        OwnerPressure {
            peer: lane(&self.input),
            control: lane(&self.control),
            completion: lane(&self.completion),
            shutdown: lane(&self.shutdown),
        }
    }

    /// Queue only after authenticating operator authority, including on Unix.
    /// Acceptance survives requester cancellation and exact retry after leave.
    pub fn leave_operation(
        &self,
        request: LeaveRequest,
    ) -> Result<oneshot::Receiver<Result<LeaveReceipt, LeaveError>>, DriverError> {
        let generation = self.view()?.generation;
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::LeaveOperation {
                generation,
                request,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Reserve result capacity before requesting a source-bound transfer job.
    /// No job is returned outside bounded catch-up eligibility or without a route.
    pub fn prepare_catchup(
        &self,
    ) -> Result<oneshot::Receiver<Option<CatchupPreparation>>, DriverError> {
        let completion = self
            .control
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::PrepareCatchup { completion, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Reserve the result before preparing one bounded, secret-free redial.
    pub(crate) fn prepare_join_reconnect(
        &self,
    ) -> Result<oneshot::Receiver<Option<JoinReconnect>>, DriverError> {
        let completion = self
            .control
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::PrepareJoinReconnect { completion, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }

    #[cfg(any(test, feature = "formation-fault-test"))]
    pub(crate) async fn hold_shutdown(&self, release: oneshot::Receiver<()>) {
        let (ready, observed) = oneshot::channel();
        self.control
            .send(Control::HoldShutdown { release, ready })
            .await
            .unwrap();
        observed.await.unwrap();
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn hold_progress(&self, release: oneshot::Receiver<()>) {
        let (ready, observed) = oneshot::channel();
        self.control
            .send(Control::HoldProgress { release, ready })
            .await
            .unwrap();
        observed.await.unwrap();
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn corrupt_catchup_page(&self, index: u16) -> oneshot::Receiver<()> {
        let (observed, receive) = oneshot::channel();
        let (ready, armed) = oneshot::channel();
        self.control
            .send(Control::CorruptCatchupPage {
                index,
                observed,
                ready,
            })
            .await
            .unwrap();
        armed.await.unwrap();
        receive
    }

    #[cfg(test)]
    pub(crate) async fn disconnect_next_join(&self) -> oneshot::Receiver<usize> {
        let (disconnected, observed) = oneshot::channel();
        let (reply, receive) = oneshot::channel();
        self.control
            .send(Control::DisconnectNextJoin {
                disconnected,
                reply,
            })
            .await
            .unwrap();
        receive.await.unwrap();
        observed
    }

    #[cfg(any(test, feature = "formation-fault-test"))]
    pub(crate) async fn lose_next_join_ack(&self) -> oneshot::Receiver<NodeId> {
        self.arm_join_fault(JoinFault::LoseAck).await
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn crash_after_next_join(&self) {
        drop(self.arm_join_fault(JoinFault::Crash).await);
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn remove_after_next_join(&self) -> oneshot::Receiver<NodeId> {
        self.arm_join_fault(JoinFault::Remove).await
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn block_after_next_join(&self) -> oneshot::Receiver<NodeId> {
        self.arm_join_fault(JoinFault::Block).await
    }

    #[cfg(any(test, feature = "formation-fault-test"))]
    async fn arm_join_fault(&self, fault: JoinFault) -> oneshot::Receiver<NodeId> {
        let (observed, receive) = oneshot::channel();
        let (reply, ready) = oneshot::channel();
        self.control
            .send(Control::LoseNextJoinAck {
                observed,
                fault,
                reply,
            })
            .await
            .unwrap();
        ready.await.unwrap();
        receive
    }
    #[cfg(test)]
    pub(crate) async fn disconnect_peers(&self) {
        let (reply, receive) = oneshot::channel();
        self.control
            .send(Control::DisconnectPeers(reply))
            .await
            .unwrap();
        receive.await.unwrap();
    }
    /// Move only the adoption timestamp to its deadline in tests, leaving
    /// current sessions and membership intact to isolate the deadline guard.
    #[cfg(test)]
    pub(crate) async fn expire_catchup(&self) {
        let (reply, receive) = oneshot::channel();
        self.control
            .send(Control::ExpireCatchup(reply))
            .await
            .unwrap();
        receive.await.unwrap();
    }
    /// Inspect at most 64 member records for one missing canonical outbound
    /// route. This is an IO plan, never admission or a durable reservation.
    pub(crate) fn member_dial(
        &self,
        after: Option<NodeId>,
    ) -> Result<oneshot::Receiver<MemberDialPage>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::MemberDial { after, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Reserve completion capacity before accepting work. Dropping the returned
    /// preparation queues a failure, including when its requester disappears.
    pub fn prepare_join(
        &self,
        request: orishu::model::cluster::JoinRequest,
    ) -> Result<
        oneshot::Receiver<Result<PreparedJoin, crate::join_operations::OperationError>>,
        DriverError,
    > {
        self.prepare_join_with_parent(request, None)
    }

    /// Retain at most one IO-only parent on a newly reserved operation. Replays
    /// do not replace it; existing generation/completion fencing remains intact.
    pub(crate) fn prepare_join_with_parent(
        &self,
        request: orishu::model::cluster::JoinRequest,
        parent: Option<crate::trace_context::TraceParent>,
    ) -> Result<
        oneshot::Receiver<Result<PreparedJoin, crate::join_operations::OperationError>>,
        DriverError,
    > {
        #[cfg(feature = "otlp-tracing")]
        let parent = parent.filter(|_| self.tracing.get().is_some());
        #[cfg(not(feature = "otlp-tracing"))]
        let parent = {
            let _ = parent;
            None
        };
        let completion = self
            .control
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::PrepareJoin {
                parent,
                request,
                completion,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }

    /// Read retained operation status from the serialized owner.
    pub fn join_status(
        &self,
        id: OperationId,
    ) -> Result<oneshot::Receiver<Option<orishu::model::cluster::JoinOperation>>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::JoinStatus { id, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Query local issuer evidence; the caller must authenticate operator authority.
    pub fn inspect_admission(
        &self,
        request: orishu::model::cluster::AdmissionInspectionRequest,
    ) -> Result<oneshot::Receiver<orishu::model::cluster::AdmissionInspection>, DriverError> {
        if request.schema_version != 1 {
            return Err(DriverError::Limit);
        }
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::InspectAdmission { request, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Internal authorized-operation seam. Rechecks source/participation and
    /// registered introducer before starting the core attempt; not final admission.
    pub fn begin_join(
        &self,
        generation: Generation,
        source: orishu_membership::FormationId,
        target: orishu_membership::FormationId,
        session: orishu_membership::SessionId,
        token: SecretToken,
    ) -> Result<oneshot::Receiver<Result<(), DriverError>>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::BeginJoin {
                parent: None,
                generation,
                source,
                target,
                session,
                token,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Privileged adapter only: export current formation-bound admission material.
    pub fn join_material(
        &self,
    ) -> Result<oneshot::Receiver<Option<orishu::model::cluster::JoinMaterial>>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::JoinMaterial(reply))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Bounded local page. None means a stale formation or invalid continuation.
    pub fn list(
        &self,
        formation: Option<orishu_membership::FormationId>,
        after: Option<orishu_membership::NodeId>,
    ) -> Result<oneshot::Receiver<Option<orishu::model::node::MembershipPage>>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::List {
                formation,
                after,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Read one member at a serialized owner boundary. Unknown IDs return None;
    /// the bounded control lane reports overload without queuing unbounded work.
    pub fn inspect(
        &self,
        node: orishu_membership::NodeId,
    ) -> Result<oneshot::Receiver<Option<orishu::model::node::Inspection>>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::Inspect { node, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
    /// Shared inbound/outbound reliable-work budget for this worker.
    pub(crate) fn exchange_pool(&self) -> crate::peer::exchange::ExchangePool {
        self.exchanges.clone()
    }

    /// Shared pre-owner IO accounting; no owner message or collection scan.
    pub(crate) fn packet_io(&self) -> &crate::peer::traffic::Traffic {
        &self.packet_io
    }

    /// Supervise IO against the actual owner lifetime, not a separate timer.
    pub async fn closed(&self) {
        let mut view = self.view.clone();
        while view.changed().await.is_ok() {}
    }

    /// Submit raw bounded bytes with an IO-owned session ID, never a wire-claimed
    /// identity. A stream response belongs on the originating request stream;
    /// no response means the core emitted none (not necessarily acceptance).
    pub fn receive_peer_packet(
        &self,
        generation: Generation,
        session: orishu_membership::SessionId,
        transport: crate::peer::wire::Transport,
        bytes: Vec<u8>,
    ) -> Result<
        oneshot::Receiver<Result<Option<crate::peer::wire::Encoded>, PacketError>>,
        PacketError,
    > {
        let maximum = match transport {
            crate::peer::wire::Transport::Stream => crate::peer::codec::MAX_FRAME_BYTES,
            crate::peer::wire::Transport::Datagram => crate::peer::wire::MAX_DATAGRAM_BYTES,
        };
        if bytes.len() > maximum {
            return Err(crate::peer::registry::RegistryError::InvalidPacket.into());
        }
        let (reply, receive) = oneshot::channel();
        self.input
            .try_send(PeerDelivery::Packet {
                generation,
                session,
                transport,
                bytes,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }

    /// Hand a completed TLS connection and bounded first-stream payload to the
    /// live owner. This uses peer capacity, never the reserved operator lane.
    pub fn accept_peer_handshake(
        &self,
        generation: Generation,
        connection: quinn::Connection,
        bytes: Vec<u8>,
    ) -> Result<
        oneshot::Receiver<
            Result<crate::peer::registry::HandshakeReply, crate::peer::registry::RegistryError>,
        >,
        crate::peer::registry::RegistryError,
    > {
        self.register_handshake(
            generation,
            connection,
            bytes,
            crate::peer::registry::HandshakeMode::Incoming,
        )
    }

    /// Bind an outbound peer ACK against current admitted membership.
    pub fn accept_member_handshake_reply(
        &self,
        generation: Generation,
        connection: quinn::Connection,
        bytes: Vec<u8>,
    ) -> Result<
        oneshot::Receiver<
            Result<crate::peer::registry::HandshakeReply, crate::peer::registry::RegistryError>,
        >,
        crate::peer::registry::RegistryError,
    > {
        self.register_handshake(
            generation,
            connection,
            bytes,
            crate::peer::registry::HandshakeMode::MemberReply,
        )
    }

    /// Validate a completed pinned handshake in the current owner generation.
    /// This creates only a provisional JoinReply binding; it does not authorize
    /// or begin an operator join. The join operation must correlate this result.
    pub fn accept_introducer_handshake_reply(
        &self,
        generation: Generation,
        pending: crate::peer::dial::PendingHandshake,
        target: crate::peer::dial::IntroducerTarget,
    ) -> Result<
        oneshot::Receiver<
            Result<crate::peer::registry::HandshakeReply, crate::peer::registry::RegistryError>,
        >,
        crate::peer::registry::RegistryError,
    > {
        let (connection, bytes) = pending.into_parts();
        self.register_handshake(
            generation,
            connection,
            bytes,
            crate::peer::registry::HandshakeMode::IntroducerReply(target),
        )
    }

    fn register_handshake(
        &self,
        generation: Generation,
        connection: quinn::Connection,
        bytes: Vec<u8>,
        mode: crate::peer::registry::HandshakeMode,
    ) -> Result<
        oneshot::Receiver<
            Result<crate::peer::registry::HandshakeReply, crate::peer::registry::RegistryError>,
        >,
        crate::peer::registry::RegistryError,
    > {
        use crate::peer::registry::RegistryError;
        if bytes.len() > crate::peer::handshake::MAX_HANDSHAKE_BYTES {
            connection.close(0_u32.into(), b"handshake too large");
            return Err(RegistryError::Handshake);
        }
        let (reply, receive) = oneshot::channel();
        if let Err(error) = self.input.try_send(PeerDelivery::Handshake {
            mode,
            generation,
            connection: PendingConnection(Some(connection.clone())),
            bytes,
            reply,
        }) {
            connection.close(0_u32.into(), b"owner unavailable");
            return Err(match error {
                mpsc::error::TrySendError::Full(_) => RegistryError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => RegistryError::Unavailable,
            });
        }
        Ok(receive)
    }

    /// Submit an identified authorized request. Replays recover the original
    /// result without applying the intent again; IDs cannot be reused for a
    /// different intent during this worker's participation in the formation.
    pub fn lock_operation(
        &self,
        request: LockRequest,
    ) -> Result<oneshot::Receiver<Result<LockReceipt, LockError>>, DriverError> {
        let generation = self.view()?.generation;
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::LockOperation {
                generation,
                request,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }

    /// Queue an already-authorized lock intent in the same bounded control lane
    /// as other operator commands. Preconditions are checked by the owner, not
    /// against a possibly superseded HTTP-side projection. Dropping the returned
    /// receiver does not undo an accepted command.
    pub fn set_membership_lock(
        &self,
        generation: Generation,
        formation: orishu_membership::FormationId,
        locked: bool,
    ) -> Result<oneshot::Receiver<Result<Summary, LockError>>, DriverError> {
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::SetLock {
                generation,
                formation,
                locked,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }

    /// Reserve completion capacity before starting an asynchronous credential
    /// check. Request/session/generation correlation cannot be changed afterward.
    /// Cancellation emits a negative verdict through the same reserved slot.
    pub fn reserve_verification(
        &self,
        generation: Generation,
        request: orishu_membership::VerificationId,
        session: orishu_membership::SessionId,
        authenticated: bool,
    ) -> Result<VerificationCompletion, DriverError> {
        let permit = self
            .completion
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(VerificationCompletion {
            permit: Some(permit),
            generation,
            request,
            session,
            authenticated,
        })
    }

    /// Submit trusted adapter input. Authentication and command authorization
    /// remain the caller's responsibility; this is not a network API.
    pub fn try_submit(&self, generation: Generation, message: Message) -> Result<(), DriverError> {
        if matches!(&message, Message::Local(_)) {
            return self
                .control
                .try_send(Control::Input(Input {
                    generation,
                    message,
                }))
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                    mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
                });
        }
        if matches!(&message, Message::Peer(_)) {
            return self
                .input
                .try_send(PeerDelivery::Core(Input {
                    generation,
                    message,
                }))
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                    mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
                });
        }
        self.completion
            .try_send(Input {
                generation,
                message,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })
    }

    /// Read without waiting on the core. A stopped owner is an explicit error.
    pub fn view(&self) -> Result<View, DriverError> {
        self.view.has_changed().map_err(|_| DriverError::Closed)?;
        Ok(self.view.borrow().clone())
    }

    /// Copy the latest published aggregates without cloning the membership
    /// projection. After owner closure these retain their last values, not a
    /// fresh progress claim; consumers must inspect health separately.
    pub fn counters(&self) -> OwnerCounters {
        let view = self.view.borrow();
        OwnerCounters {
            traffic: view.traffic,
            peer_decode_rejections: view.peer_decode_rejections,
            transitions: view.transitions,
            stale_inputs: view.stale_inputs,
            diagnostics: view.diagnostics,
            foreign_gossip: view.foreign_gossip,
            completed_exchanges: view.completed_exchanges,
            failed_sends: view.failed_sends,
            admissions: view.admissions,
        }
    }

    /// Read bounded local supervision without sending work to the owner.
    /// This does not establish full process readiness or global convergence.
    pub fn health(&self, now: Instant) -> crate::health::OwnerHealth {
        let closed = self.view.has_changed().is_err();
        let view = self.view.borrow();
        crate::health::OwnerHealth::evaluate(
            view.summary.participation,
            view.owner_progress,
            now,
            closed,
        )
    }

    /// Stop through a separate reserved mailbox, not behind peer/client inputs.
    pub async fn shutdown(&self) -> Result<(), DriverError> {
        let (send, receive) = oneshot::channel();
        self.shutdown
            .send(send)
            .await
            .map_err(|_| DriverError::Closed)?;
        receive.await.map_err(|_| DriverError::Closed)
    }

    #[cfg(test)]
    pub(crate) async fn model_snapshot(&self) -> Membership {
        let (send, receive) = oneshot::channel();
        self.control
            .send(Control::ModelSnapshot(send))
            .await
            .unwrap();
        receive.await.unwrap()
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn eject_peer_fixture(&self) -> Result<NodeId, DriverError> {
        let (send, receive) = oneshot::channel();
        self.control
            .send(Control::EjectPeerFixture(send))
            .await
            .map_err(|_| DriverError::Closed)?;
        receive.await.map_err(|_| DriverError::Closed)?
    }

    #[cfg(feature = "formation-fault-test")]
    pub(crate) async fn drop_next_departure(&self) {
        let (send, receive) = oneshot::channel();
        self.control
            .send(Control::DropNextDeparture(send))
            .await
            .unwrap();
        receive.await.unwrap();
    }
}

/// A single-use, capacity-backed verification completion. Dropping it (including
/// when its async task is aborted) denies verification rather than stranding it.
pub struct VerificationCompletion {
    permit: Option<mpsc::OwnedPermit<Input>>,
    generation: Generation,
    request: orishu_membership::VerificationId,
    session: orishu_membership::SessionId,
    authenticated: bool,
}

impl VerificationCompletion {
    /// Deliver evidence without waiting for a queue slot or risking overload.
    /// This queues an outcome; it is not proof that the core admitted the peer.
    pub fn complete(
        mut self,
        mut evidence: orishu_membership::AdmissionEvidence,
    ) -> Result<(), DriverError> {
        evidence.transport_authenticated &= self.authenticated;
        self.deliver(evidence)
    }

    fn deliver(
        &mut self,
        evidence: orishu_membership::AdmissionEvidence,
    ) -> Result<(), DriverError> {
        let permit = self
            .permit
            .take()
            .expect("single-use verification completion");
        let sender = permit.send(Input {
            generation: self.generation,
            message: Message::Outcome(EffectOutcome::CredentialVerified {
                request: self.request,
                session: self.session,
                evidence,
            }),
        });
        if sender.is_closed() {
            Err(DriverError::Closed)
        } else {
            Ok(())
        }
    }
}

impl Drop for VerificationCompletion {
    fn drop(&mut self) {
        if self.permit.is_some() {
            let _ = self.deliver(orishu_membership::AdmissionEvidence {
                transport_authenticated: self.authenticated,
                token_valid: false,
                source_network_blocked: false,
            });
        }
    }
}

/// Start without admission credentials. Registered member connections may carry
/// peer traffic; missing routes are counted as failures and are not dialed here.
/// Without an installed formation token, admission receives a negative verdict;
/// missing credential catch-up must not terminate the membership owner.
/// The returned task reports terminal failures to the supervising worker.
pub fn spawn_standalone(
    model: Membership,
) -> (Handle, tokio::task::JoinHandle<Result<(), DriverError>>) {
    spawn_owner(
        model,
        None,
        Default::default(),
        Default::default(),
        Default::default(),
    )
}

/// Install one fresh standalone formation's admission credential in the shell.
/// This does not enable a peer listener or imply outbound IO/catch-up readiness.
pub fn spawn_standalone_with_join_token(
    model: Membership,
    token: SecretToken,
) -> (Handle, tokio::task::JoinHandle<Result<(), DriverError>>) {
    spawn_owner(
        model,
        Some(token),
        Default::default(),
        Default::default(),
        Default::default(),
    )
}

pub(crate) fn spawn_standalone_observed(
    model: Membership,
    token: Option<SecretToken>,
    metrics: crate::peer::exchange_metrics::ExchangeMetrics,
    packet_io: crate::peer::traffic::Traffic,
    formation_metrics: crate::formation_metrics::FormationMetrics,
) -> (Handle, tokio::task::JoinHandle<Result<(), DriverError>>) {
    spawn_owner(model, token, metrics, packet_io, formation_metrics)
}

fn spawn_owner(
    model: Membership,
    join_token: Option<SecretToken>,
    metrics: crate::peer::exchange_metrics::ExchangeMetrics,
    packet_io: crate::peer::traffic::Traffic,
    formation_metrics: crate::formation_metrics::FormationMetrics,
) -> (Handle, tokio::task::JoinHandle<Result<(), DriverError>>) {
    let mut summary = project_summary(&model, Participation::Standalone);
    summary.introducer_ready =
        introducer_ready(&model, summary.participation, join_token.is_some());
    let view = View {
        owner_progress: None,
        completed_exchanges: 0,
        failed_sends: 0,
        peer_decode_rejections: 0,
        traffic: TrafficCounters::default(),
        summary,
        generation: Generation(0),
        transitions: 0,
        stale_inputs: 0,
        diagnostics: 0,
        foreign_gossip: 0,
        admissions: AdmissionCounters::default(),
    };
    let (published, receiver) = watch::channel(view.clone());
    let formation_adopted = std::sync::Arc::new(tokio::sync::Notify::new());
    let first_catchup_route = std::sync::Arc::new(tokio::sync::Notify::new());
    let (send, mut input) = mpsc::channel::<PeerDelivery>(MAILBOX_CAPACITY);
    let (control, mut commands) = mpsc::channel::<Control>(CONTROL_CAPACITY);
    let (completion, mut completions) = mpsc::channel::<Input>(MAILBOX_CAPACITY);
    let (shutdown, mut stopping) = mpsc::channel::<oneshot::Sender<()>>(1);
    let exchanges = crate::peer::exchange::ExchangePool::with_metrics(MAX_RELIABLE_SENDS, metrics)
        .expect("fixed valid capacity");
    #[cfg(feature = "otlp-tracing")]
    let tracing = std::sync::Arc::new(std::sync::OnceLock::new());
    let handle = Handle {
        #[cfg(feature = "otlp-tracing")]
        tracing: tracing.clone(),
        #[cfg(feature = "observability")]
        formation_metrics: formation_metrics.clone(),
        exchanges: exchanges.clone(),
        packet_io: packet_io.clone(),
        input: send,
        control,
        completion,
        shutdown,
        view: receiver,
        formation_adopted: formation_adopted.clone(),
        first_catchup_route: first_catchup_route.clone(),
    };
    let task = tokio::spawn(async move {
        let sessions = crate::peer::registry::Registry::observed(formation_metrics.clone());
        let mut owner = Owner {
            formation_adopted,
            first_catchup_route,
            #[cfg(feature = "otlp-tracing")]
            tracing,
            formation_metrics,
            packet_io,
            #[cfg(feature = "formation-fault-test")]
            progress_hold: None,
            #[cfg(feature = "formation-fault-test")]
            corrupt_catchup_page: None,
            #[cfg(any(test, feature = "formation-fault-test"))]
            shutdown_release: None,
            #[cfg(any(test, feature = "formation-fault-test"))]
            lose_next_join_ack: None,
            admission_replays: Default::default(),
            #[cfg(test)]
            disconnect_next_join: None,
            leave_operations: BTreeMap::new(),
            pending_catchup: None,
            next_catchup: 0,
            catchup_attempts: 0,
            catchup_route_notified: false,
            catchup_started: None,
            catchup_route: None,
            catchup_failed: false,
            baseline_result: None,
            admission_baselines: Default::default(),
            join_operations: Default::default(),
            active_join_operation: None,
            outbound_join: None,
            sends: tokio::task::JoinSet::new(),
            exchanges,
            join_token,
            model: Some(model),
            sessions,
            packet: None,
            #[cfg(feature = "formation-fault-test")]
            drop_next_departure: false,
            lock_operations: BTreeMap::new(),
            timers: BTreeMap::new(),
            view,
            published,
        };
        let mut probe = tokio::time::interval(Duration::from_secs(1));
        let mut reconcile = tokio::time::interval(Duration::from_secs(5));
        probe.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut completion_budget = 8_u8;
        let mut control_budget = 4_u8;
        loop {
            let next_timer = owner
                .timers
                .values()
                .min()
                .copied()
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
            tokio::select! {
                // Reserved shutdown and due timers cannot sit behind a peer flood.
                biased;
                request = stopping.recv() => {
                    owner.view.summary.participation = Participation::Stopping;
                    owner.published.send_replace(owner.view.clone());
                    if let Some(reply) = request { let _ = reply.send(()); }
                    #[cfg(any(test, feature = "formation-fault-test"))]
                    if let Some(release) = owner.shutdown_release.take() {
                        let _ = release.await;
                    }
                    return Ok(());
                }
                _ = tokio::time::sleep_until(next_timer), if !owner.timers.is_empty() => {
                    let now = Instant::now();
                    let due: Vec<_> = owner.timers.iter().filter_map(|(token, deadline)| (*deadline <= now).then_some(*token)).take(8).collect();
                    for token in due {
                        owner.timers.remove(&token);
                        owner.apply(Message::Timer(token))?;
                    }
                }
                _ = probe.tick() => {
                    // Supervise this owner, including healthy ejected/stopping
                    // states. A separate exporter timer cannot fake progress.
                    owner.view.owner_progress = Some(Instant::now());
                    owner.published.send_replace(owner.view.clone());
                    if !matches!(owner.view.summary.participation, Participation::Ejected | Participation::Stopping) {
                        owner.apply(Message::Local(Command::StartProbeRound))?;
                    }
                },
                _ = reconcile.tick() => {
                    if !matches!(owner.view.summary.participation, Participation::Ejected | Participation::Stopping) {
                        owner.apply(Message::Local(Command::StartAntiEntropyRound))?;
                    }
                },
                result = owner.sends.join_next(), if !owner.sends.is_empty() && (completion_budget > 0 || (commands.is_empty() && input.is_empty())) => {
                    completion_budget = completion_budget.saturating_sub(1);
                    if let Some(Ok(result)) = result {
                        owner.send_finished(result)?;
                    }
                }
                message = completions.recv(), if completion_budget > 0 || (commands.is_empty() && input.is_empty()) => {
                    let Some(message) = message else { return Ok(()); };
                    completion_budget = completion_budget.saturating_sub(1);
                    owner.deliver(message)?;
                }
                message = commands.recv(), if control_budget > 0 || input.is_empty() => {
                    let Some(message) = message else { return Ok(()); };
                    control_budget = control_budget.saturating_sub(1);
                    completion_budget = 8;
                    owner.control(message)?;
                    #[cfg(feature = "formation-fault-test")]
                    if let Some((release, ready)) = owner.progress_hold.take() {
                        // Pause this owner only, not its async executor or the
                        // HTTP server. No supervision timestamp is fabricated.
                        let _ = ready.send(());
                        let _ = release.await;
                    }
                }
                message = input.recv() => {
                    let Some(message) = message else { return Ok(()); };
                    completion_budget = 8;
                    control_budget = 4;
                    owner.peer(message)?;
                }
            }
        }
    });
    (handle, task)
}

struct Owner {
    formation_adopted: std::sync::Arc<tokio::sync::Notify>,
    first_catchup_route: std::sync::Arc<tokio::sync::Notify>,
    #[cfg(feature = "otlp-tracing")]
    tracing: std::sync::Arc<std::sync::OnceLock<crate::trace_export::SpanQueue>>,
    formation_metrics: crate::formation_metrics::FormationMetrics,
    packet_io: crate::peer::traffic::Traffic,
    #[cfg(feature = "formation-fault-test")]
    corrupt_catchup_page: Option<(u16, oneshot::Sender<()>)>,
    #[cfg(feature = "formation-fault-test")]
    progress_hold: Option<(oneshot::Receiver<()>, oneshot::Sender<()>)>,
    #[cfg(any(test, feature = "formation-fault-test"))]
    shutdown_release: Option<oneshot::Receiver<()>>,
    #[cfg(any(test, feature = "formation-fault-test"))]
    lose_next_join_ack: Option<(oneshot::Sender<NodeId>, JoinFault)>,
    admission_replays: crate::peer::admission_replay::Ledger,
    #[cfg(test)]
    disconnect_next_join: Option<oneshot::Sender<usize>>,
    leave_operations: BTreeMap<OperationId, (LeaveRequest, LeaveReceipt)>,
    pending_catchup: Option<CatchupAttempt>,
    next_catchup: u64,
    catchup_attempts: u8,
    catchup_route_notified: bool,
    catchup_started: Option<Instant>,
    // One admitted reconnect hint, not a credential or readiness grant.
    catchup_route: Option<(NodeId, orishu_membership::CertFingerprint)>,
    catchup_failed: bool,
    baseline_result: Option<(u64, bool, bool)>,
    admission_baselines: crate::peer::catchup::store::Store,
    join_operations: crate::join_operations::JoinOperations,
    active_join_operation: Option<OperationId>,
    outbound_join: Option<JoinTransport>,
    sends: tokio::task::JoinSet<SendCompletion>,
    exchanges: crate::peer::exchange::ExchangePool,
    join_token: Option<SecretToken>,
    // Temporarily taken only while update owns it; never visible to adapters.
    model: Option<Membership>,
    sessions: crate::peer::registry::Registry,
    packet: Option<PacketContext>,
    #[cfg(feature = "formation-fault-test")]
    drop_next_departure: bool,
    lock_operations: BTreeMap<OperationId, (LockRequest, Result<LockReceipt, LockError>)>,
    timers: BTreeMap<TimerToken, Instant>,
    view: View,
    published: watch::Sender<View>,
}

impl Owner {
    fn send_finished(&mut self, result: SendCompletion) -> Result<(), DriverError> {
        match result {
            (generation, session, Ok(bytes)) => {
                if generation == self.view.generation {
                    self.view.completed_exchanges = self.view.completed_exchanges.saturating_add(1);
                }
                let (reply, _receive) = oneshot::channel();
                self.peer(PeerDelivery::Packet {
                    generation,
                    session,
                    transport: crate::peer::wire::Transport::Stream,
                    bytes,
                    reply,
                })?;
            }
            (generation, _, Err(_)) if generation == self.view.generation => {
                self.view.failed_sends = self.view.failed_sends.saturating_add(1);
                self.published.send_replace(self.view.clone());
            }
            _ => {}
        }
        Ok(())
    }

    fn peer(&mut self, message: PeerDelivery) -> Result<(), DriverError> {
        match message {
            PeerDelivery::Core(input) => self.deliver(input),
            PeerDelivery::Packet {
                generation,
                session,
                transport,
                bytes,
                reply,
            } => {
                if generation != self.view.generation
                    || matches!(
                        self.view.summary.participation,
                        Participation::Ejected | Participation::Stopping
                    )
                {
                    let _ = reply.send(Err(crate::peer::registry::RegistryError::Stale.into()));
                    return Ok(());
                }
                if crate::peer::catchup::wire::is_request(&bytes).unwrap_or(false) {
                    let model = self.model.as_ref().expect("installed model");
                    let result = (|| {
                        if transport != crate::peer::wire::Transport::Stream {
                            return Err(crate::peer::registry::RegistryError::InvalidPacket);
                        }
                        let (context, _) = self
                            .sessions
                            .credential_context(model, generation, session)?;
                        if context.formation != *model.formation()
                            || !matches!(
                                context.sender,
                                orishu_membership::SenderIdentity::Admitted(_)
                            )
                        {
                            return Err(crate::peer::registry::RegistryError::InvalidPacket);
                        }
                        let encoded = crate::peer::catchup::wire::serve(
                            &mut self.admission_baselines,
                            crate::peer::catchup::wire::Source {
                                model,
                                generation,
                                peer: &context,
                                ready: self.view.summary.introducer_ready,
                                token: self.join_token.as_ref(),
                            },
                            &bytes,
                            Instant::now().into_std(),
                        )
                        .map_err(|_| crate::peer::registry::RegistryError::InvalidPacket)?;
                        #[cfg(feature = "formation-fault-test")]
                        let encoded = if let Some((index, _)) = &self.corrupt_catchup_page {
                            let (encoded, corrupted) = crate::peer::catchup::wire::corrupt_page(
                                encoded, *index,
                            )
                            .map_err(|_| crate::peer::registry::RegistryError::InvalidPacket)?;
                            if corrupted {
                                let _ = self.corrupt_catchup_page.take().unwrap().1.send(());
                            }
                            encoded
                        } else {
                            encoded
                        };
                        Ok(Some(crate::peer::wire::Encoded {
                            transport,
                            bytes: encoded,
                            deferred_gossip: 0,
                            deferred: Vec::new(),
                        }))
                    })();
                    let _ = reply.send(result.map_err(Into::into));
                    return Ok(());
                }
                let decoded = match self.sessions.decode_packet(
                    self.model.as_ref().expect("installed model"),
                    generation,
                    session,
                    &bytes,
                    transport,
                ) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        self.view.peer_decode_rejections =
                            self.view.peer_decode_rejections.saturating_add(1);
                        // Publish only the aggregate: no membership projection
                        // scan or owner-progress refresh for refused traffic.
                        self.published.send_modify(|view| {
                            view.peer_decode_rejections = self.view.peer_decode_rejections;
                        });
                        let _ = reply.send(Err(error.into()));
                        return Ok(());
                    }
                };
                self.view.traffic.record(&decoded.input);
                self.published.send_modify(|view| {
                    view.traffic = self.view.traffic;
                });
                #[cfg(test)]
                if matches!(
                    decoded.input.body,
                    orishu_membership::PeerBody::JoinRequest { .. }
                ) && let Some(disconnected) = self.disconnect_next_join.take()
                {
                    self.sessions.close_all();
                    let _ = disconnected.send(
                        self.model
                            .as_ref()
                            .expect("installed model")
                            .members()
                            .len(),
                    );
                    let _ = reply.send(Err(DriverError::PeerAdapterUnavailable.into()));
                    return Ok(());
                }
                #[cfg(feature = "otlp-tracing")]
                let active = if decoded.join_attempt.is_some() {
                    self.tracing.get().and_then(|queue| {
                        // Session/generation/source checks already passed. A
                        // valid current join credential is additionally needed
                        // to adopt diagnostic ancestry, never to skip admission.
                        let parent = admission_trace_parent(
                            self.join_token.as_ref(),
                            decoded.join_token.as_ref(),
                            decoded.trace_parent,
                        );
                        queue.try_begin_with_parent(
                            crate::trace_export::Operation::Admission,
                            parent,
                        )
                    })
                } else {
                    None
                };
                if decoded.join_attempt.is_some() {
                    match self.replay_join(&decoded, session) {
                        Ok(Some(encoded)) => {
                            #[cfg(feature = "otlp-tracing")]
                            if let Some(active) = active {
                                active.finish(crate::trace_export::Outcome::Completed);
                            }
                            let _ = reply.send(Ok(Some(encoded)));
                            return Ok(());
                        }
                        Err(error) => {
                            #[cfg(feature = "otlp-tracing")]
                            if let Some(active) = active {
                                active.finish(crate::trace_export::Outcome::Rejected);
                            }
                            let _ = reply.send(Err(error.into()));
                            return Ok(());
                        }
                        Ok(None) => {}
                    }
                }
                self.packet = Some(PacketContext {
                    #[cfg(feature = "otlp-tracing")]
                    admission_outcome: crate::trace_export::Outcome::Completed,
                    join_attempt: decoded
                        .join_attempt
                        .map(|attempt| (decoded.input.context.cert_fingerprint, attempt)),
                    member: match &decoded.input.context.sender {
                        orishu_membership::SenderIdentity::Admitted(node) => Some(node.clone()),
                        _ => None,
                    },
                    token: decoded.join_token,
                    generation,
                    session,
                    transport,
                    response: None,
                });
                // The token remains in the shell's request scope and is never
                // inserted into replayable membership messages or diagnostics.
                let result = self.apply(Message::Peer(decoded.input));
                #[cfg(any(test, feature = "formation-fault-test"))]
                if result.is_ok()
                    && let Some((fingerprint, attempt)) = self
                        .packet
                        .as_ref()
                        .and_then(|packet| packet.join_attempt.as_ref())
                    && let Ok(Some(record)) = self.admission_replays.lookup(*fingerprint, attempt)
                    && let Some((observed, fault)) = self.lose_next_join_ack.take()
                {
                    let assigned = record.assigned.clone();
                    #[cfg(feature = "formation-fault-test")]
                    let applicant_fingerprint = *fingerprint;
                    self.packet.take(); // Discard encoded acceptance before response delivery.
                    #[cfg(feature = "formation-fault-test")]
                    if matches!(fault, JoinFault::Crash) {
                        use std::io::Write;
                        println!("FORMATION_TEST_ISSUER_EXIT {assigned}");
                        let _ = std::io::stdout().flush();
                        std::process::exit(86);
                    }
                    #[cfg(feature = "formation-fault-test")]
                    if matches!(fault, JoinFault::Remove) {
                        self.apply(Message::Local(Command::RemoveMember {
                            node: assigned.clone(),
                            mode: orishu_membership::RemovalMode::Force,
                            reason: None,
                        }))?;
                    }
                    #[cfg(feature = "formation-fault-test")]
                    if matches!(fault, JoinFault::Block) {
                        let actor = self.model.as_ref().unwrap().local_id().clone();
                        self.apply(Message::Local(Command::UpdateBlocklist(
                            orishu_membership::model::BlocklistEntry {
                                key: orishu_membership::model::BlocklistKey::Fingerprint(
                                    applicant_fingerprint,
                                ),
                                action: orishu_membership::model::BlocklistAction::Block,
                                version: orishu_membership::VersionTuple::initial(0, actor),
                                added_by: "formation-fault-test".into(),
                            },
                        )))?;
                    }
                    #[cfg(not(feature = "formation-fault-test"))]
                    let _ = fault;
                    self.sessions.close_all();
                    let _ = observed.send(assigned);
                    let _ = reply.send(Err(DriverError::PeerAdapterUnavailable.into()));
                    return Ok(());
                }
                let packet = self
                    .packet
                    .take()
                    .expect("packet context scoped to one transition");
                #[cfg(feature = "otlp-tracing")]
                if let Some(active) = active {
                    active.finish(if result.is_ok() {
                        packet.admission_outcome
                    } else {
                        crate::trace_export::Outcome::Failed
                    });
                }
                let response = packet.response;
                match result {
                    Ok(()) => {
                        let _ = reply.send(Ok(response));
                        Ok(())
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error.into()));
                        Err(error)
                    }
                }
            }
            PeerDelivery::Handshake {
                mode,
                generation,
                mut connection,
                bytes,
                reply,
            } => {
                let connection = connection.0.take().expect("queued connection owned once");
                let result = if generation != self.view.generation
                    || matches!(
                        self.view.summary.participation,
                        Participation::Ejected | Participation::Stopping
                    ) {
                    connection.close(0_u32.into(), b"stale generation");
                    Err(crate::peer::registry::RegistryError::Stale)
                } else {
                    let model = self.model.as_ref().expect("installed model");
                    match mode {
                        crate::peer::registry::HandshakeMode::Incoming => {
                            self.sessions.accept(model, generation, connection, &bytes)
                        }
                        crate::peer::registry::HandshakeMode::MemberReply => self
                            .sessions
                            .accept_member_reply(model, generation, connection, &bytes),
                        crate::peer::registry::HandshakeMode::IntroducerReply(target) => self
                            .sessions
                            .accept_introducer_reply(model, generation, connection, &bytes, target),
                    }
                };
                if self.view.summary.participation == Participation::CatchingUp
                    && self.catchup_attempts == 0
                    && !self.catchup_route_notified
                    && let Ok(binding) = &result
                    && self
                        .sessions
                        .credential_context(
                            self.model.as_ref().expect("installed model"),
                            self.view.generation,
                            binding.session,
                        )
                        .is_ok_and(|(context, _)| {
                            matches!(
                                context.sender,
                                orishu_membership::SenderIdentity::Admitted(_)
                            )
                        })
                {
                    self.catchup_route_notified = true;
                    self.first_catchup_route.notify_one();
                }
                let _ = reply.send(result);
                Ok(())
            }
        }
    }

    /// Recover only an existing still-authorized assignment, never re-enter
    /// admission or mutate membership. Capacity is checked before new admission.
    fn replay_join(
        &mut self,
        decoded: &crate::peer::wire::Decoded,
        session: orishu_membership::SessionId,
    ) -> Result<Option<crate::peer::wire::Encoded>, DriverError> {
        let attempt = decoded.join_attempt.as_ref().expect("join metadata");
        let fingerprint = decoded.input.context.cert_fingerprint;
        let record = self
            .admission_replays
            .lookup(fingerprint, attempt)
            .map_err(|error| match error {
                crate::peer::admission_replay::Error::Full => DriverError::Overloaded,
                crate::peer::admission_replay::Error::Conflict => {
                    DriverError::PeerAdapterUnavailable
                }
            })?;
        let Some(record) = record else {
            return if decoded.admitted_retry.is_some() {
                Err(DriverError::PeerAdapterUnavailable)
            } else {
                Ok(None)
            };
        };
        let model = self.model.as_ref().expect("installed model");
        if !self.view.summary.introducer_ready
            || decoded
                .admitted_retry
                .as_ref()
                .is_some_and(|node| node != &record.assigned)
            || !self.join_token.as_ref().is_some_and(|expected| {
                decoded
                    .join_token
                    .as_ref()
                    .is_some_and(|presented| expected.matches(presented.expose()))
            })
            || crate::peer::session::validate_member(model, &record.assigned, fingerprint).is_err()
        {
            return Err(DriverError::PeerAdapterUnavailable);
        }
        // Registry decoding already checked current source-network policy. This
        // second binding check keeps replay tied to the current owner/session.
        self.sessions
            .credential_context(model, self.view.generation, session)
            .map_err(|_| DriverError::PeerAdapterUnavailable)?;
        if model.members().len() > model.limits().max_snapshot_members() {
            return Err(DriverError::Overloaded);
        }
        let encoded = crate::peer::wire::encode(
            orishu_membership::OutboundMessage {
                seq: record.seq,
                gossip: vec![],
                body: orishu_membership::OutboundBody::JoinAccepted {
                    formation: model.formation().clone(),
                    cluster_name: model.cluster_name().clone(),
                    assigned: record.assigned.clone(),
                    snapshot: model.members().values().cloned().collect(),
                },
            },
            model.formation().clone(),
            orishu_membership::SenderIdentity::Admitted(model.local_id().clone()),
            None,
        )
        .map_err(|_| DriverError::PeerAdapterUnavailable)?;
        self.sessions
            .promote(model, self.view.generation, session, record.assigned)
            .map_err(|_| DriverError::PeerAdapterUnavailable)?;
        self.view.admissions.assignment_replays =
            self.view.admissions.assignment_replays.saturating_add(1);
        // Replay bypasses apply(), so publish here without refreshing progress.
        self.published.send_replace(self.view.clone());
        Ok(Some(encoded))
    }

    /// Act as a test peer carrying a valid tombstone; intentionally do not
    /// mutate this issuer's model or claim cluster-wide removal convergence.
    #[cfg(feature = "formation-fault-test")]
    fn send_ejection_fixture(&mut self) -> Result<NodeId, DriverError> {
        let model = self.model.as_ref().ok_or(DriverError::Closed)?;
        // A joined fixture may explicitly target its recorded introducer even
        // when another admission is pending. Never select by a display label.
        let introducer = self.active_join_operation.as_ref().and_then(|id| {
            let operation = self.join_operations.get(id)?;
            (operation.target_formation_id == *model.formation())
                .then_some(operation.recovery_reference.as_ref()?)
                .map(|reference| reference.introducer_node_id.clone())
        });
        let mut peers = model.members().values().filter(|member| {
            member.id != *model.local_id()
                && introducer.as_ref().is_none_or(|node| member.id == *node)
        });
        let peer = peers
            .next()
            .ok_or(DriverError::PeerAdapterUnavailable)?
            .clone();
        if peers.next().is_some() || peer.liveness != Liveness::Alive {
            return Err(DriverError::PeerAdapterUnavailable);
        }
        let (_, connection) = self
            .sessions
            .member_connection(model, self.view.generation, &peer.id)
            .ok_or(DriverError::PeerAdapterUnavailable)?;
        let encoded = crate::peer::wire::encode(
            orishu_membership::OutboundMessage {
                seq: 1_000_000,
                body: orishu_membership::OutboundBody::Ping {
                    probe: orishu_membership::ProbeId(1_000_000),
                    incarnation: model.incarnation(),
                },
                gossip: vec![orishu_membership::GossipDelta {
                    hops: 0,
                    body: orishu_membership::DeltaBody::TombstoneUpdate(
                        orishu_membership::MembershipTombstone {
                            node_id: peer.id.clone(),
                            name: peer.name.clone(),
                            cert_fingerprint: peer.cert_fingerprint,
                            removal_mode: orishu_membership::RemovalMode::Force,
                            version: orishu_membership::VersionTuple {
                                epoch: 0,
                                counter: 1_000_000,
                                actor: model.local_id().clone(),
                            },
                            cleared: false,
                            reason: None,
                        },
                    ),
                }],
            },
            model.formation().clone(),
            orishu_membership::SenderIdentity::Admitted(model.local_id().clone()),
            None,
        )
        .map_err(|_| DriverError::Limit)?;
        if encoded.deferred_gossip != 0 {
            return Err(DriverError::Limit);
        }
        connection
            .send_datagram(encoded.bytes.into())
            .map_err(|_| DriverError::PeerAdapterUnavailable)?;
        Ok(peer.id)
    }

    fn control(&mut self, control: Control) -> Result<(), DriverError> {
        let control = match control {
            Control::Completed(shared) => {
                let completed = shared.lock().expect("completion delivery lock").take();
                return match completed {
                    Some(completed) => self.control(completed),
                    None => Ok(()),
                };
            }
            #[cfg(test)]
            Control::ModelSnapshot(reply) => {
                let _ = reply.send(self.model.as_ref().expect("installed model").clone());
                return Ok(());
            }
            #[cfg(test)]
            Control::TimerSnapshot(reply) => {
                let _ = reply.send(
                    self.timers
                        .iter()
                        .map(|(token, deadline)| (*token, *deadline))
                        .collect(),
                );
                return Ok(());
            }
            #[cfg(test)]
            Control::ObserveSendCompletion { result, processed } => {
                self.send_finished(result)?;
                let _ = processed.send(self.view.clone());
                return Ok(());
            }
            #[cfg(feature = "formation-fault-test")]
            Control::DropNextDeparture(reply) => {
                assert!(!self.drop_next_departure);
                self.drop_next_departure = true;
                let _ = reply.send(());
                return Ok(());
            }
            #[cfg(feature = "formation-fault-test")]
            Control::EjectPeerFixture(reply) => {
                let _ = reply.send(self.send_ejection_fixture());
                return Ok(());
            }
            #[cfg(feature = "formation-fault-test")]
            Control::HoldProgress { release, ready } => {
                self.progress_hold = Some((release, ready));
                return Ok(());
            }
            #[cfg(feature = "formation-fault-test")]
            Control::CorruptCatchupPage {
                index,
                observed,
                ready,
            } => {
                self.corrupt_catchup_page = Some((index, observed));
                let _ = ready.send(());
                return Ok(());
            }
            #[cfg(any(test, feature = "formation-fault-test"))]
            Control::HoldShutdown { release, ready } => {
                self.shutdown_release = Some(release);
                let _ = ready.send(());
                return Ok(());
            }
            #[cfg(any(test, feature = "formation-fault-test"))]
            Control::LoseNextJoinAck {
                observed,
                fault,
                reply,
            } => {
                assert!(self.lose_next_join_ack.is_none());
                self.lose_next_join_ack = Some((observed, fault));
                let _ = reply.send(());
                return Ok(());
            }
            #[cfg(test)]
            Control::DisconnectNextJoin {
                disconnected,
                reply,
            } => {
                assert!(self.disconnect_next_join.is_none());
                self.disconnect_next_join = Some(disconnected);
                let _ = reply.send(());
                return Ok(());
            }
            Control::PrepareJoinReconnect { completion, reply } => {
                let model = self.model.as_ref().expect("installed model");
                let eligible = self.view.summary.participation == Participation::Joining
                    && self.outbound_join.as_ref().is_some_and(|join| {
                        join.route.is_some()
                            && join.reconnect_pending.is_none()
                            && join.reconnect_attempts < 8
                            && model.join_attempt().is_some_and(|attempt| {
                                attempt.session == join.session
                                    && attempt.target_formation == join.target
                                    && attempt.attempt < model.limits().max_join_attempts()
                            })
                    });
                if !eligible {
                    let _ = reply.send(None);
                    return Ok(());
                }
                let join = self.outbound_join.as_mut().expect("eligible join");
                if self
                    .sessions
                    .introducer_connection(model, self.view.generation, join.session, &join.target)
                    .is_some()
                {
                    let _ = reply.send(None);
                    return Ok(());
                }
                join.reconnect_attempts += 1;
                join.reconnect_pending = Some(join.reconnect_attempts);
                let job = JoinReconnect {
                    generation: self.view.generation,
                    previous_session: join.session,
                    id: join.reconnect_attempts,
                    local: model.local().clone(),
                    target: join.route.as_ref().expect("eligible route").clone(),
                    completion: Some(completion),
                };
                let _ = reply.send(Some(job));
                return Ok(());
            }
            Control::JoinReconnectFinished {
                generation,
                previous_session,
                id,
                pending,
            } => {
                let model = self.model.as_ref().expect("installed model");
                let valid = generation == self.view.generation
                    && self.view.summary.participation == Participation::Joining
                    && self.outbound_join.as_ref().is_some_and(|join| {
                        join.session == previous_session
                            && join.reconnect_pending == Some(id)
                            && model.join_attempt().is_some_and(|attempt| {
                                attempt.session == previous_session
                                    && attempt.target_formation == join.target
                            })
                    });
                if !valid {
                    return Ok(());
                }
                let join = self.outbound_join.as_mut().expect("matched reconnect");
                join.reconnect_pending = None;
                let Some(pending) = pending else {
                    return Ok(());
                };
                let target = join.route.as_ref().expect("reserved route").clone();
                let target_formation = join.target.clone();
                let (connection, bytes) = pending.into_parts();
                let Ok(binding) = self
                    .sessions
                    .accept_introducer_reply(model, generation, connection, &bytes, target)
                else {
                    return Ok(());
                };
                join.session = binding.session;
                return self.apply(Message::Local(Command::RebindJoin {
                    previous_session,
                    session: binding.session,
                    target_formation,
                }));
            }
            Control::LeaveOperation {
                generation,
                request,
                reply,
            } => {
                let outcome = self.leave_operation(generation, request);
                let fatal = match &outcome {
                    Err(LeaveError::Driver(error)) => Some(*error),
                    _ => None,
                };
                let _ = reply.send(outcome);
                return fatal.map_or(Ok(()), Err);
            }
            Control::PrepareCatchup { completion, reply } => {
                if self.view.summary.participation != Participation::CatchingUp
                    || self.pending_catchup.is_some()
                    || self.catchup_attempts >= 3
                    || self
                        .catchup_started
                        .is_none_or(|start| start.elapsed() >= Duration::from_secs(90))
                {
                    let _ = reply.send(None);
                    return Ok(());
                }
                let model = self.model.as_ref().expect("installed model");
                let mut candidates = Vec::new();
                for member in model
                    .members()
                    .values()
                    .filter(|member| member.accepts.peers && &member.id != model.local_id())
                {
                    if let Some((session, connection)) =
                        self.sessions
                            .member_connection(model, self.view.generation, &member.id)
                    {
                        candidates.push((
                            member.id.clone(),
                            member.cert_fingerprint,
                            session,
                            connection,
                        ));
                    }
                }
                if candidates.is_empty() {
                    let _ = reply.send(None);
                    return Ok(());
                }
                let selected = usize::from(self.catchup_attempts) % candidates.len();
                let (source, fingerprint, session, connection) = candidates.swap_remove(selected);
                self.next_catchup = self.next_catchup.checked_add(1).ok_or(DriverError::Limit)?;
                let id = self.next_catchup;
                self.catchup_attempts += 1;
                self.catchup_failed = false;
                self.pending_catchup = Some(CatchupAttempt {
                    id,
                    generation: self.view.generation,
                    session,
                    source: source.clone(),
                });
                let job = CatchupPreparation {
                    id,
                    generation: self.view.generation,
                    binding: crate::peer::catchup::client::Binding {
                        formation: model.formation().clone(),
                        source,
                        fingerprint,
                        requester: model.local_id().clone(),
                        request: format!("catchup-{}-{id}", self.view.generation.0)
                            .parse()
                            .expect("bounded generated identity"),
                    },
                    connection,
                    completion: Some(completion),
                    observation: Some(self.formation_metrics.start_catchup()),
                };
                self.publish()?;
                let _ = reply.send(Some(job));
                return Ok(());
            }
            Control::CatchupFinished {
                id,
                generation,
                completed,
                mut observation,
            } => {
                if !self
                    .pending_catchup
                    .as_ref()
                    .is_some_and(|attempt| attempt.id == id && attempt.generation == generation)
                    || generation != self.view.generation
                {
                    observation.decide(crate::formation_metrics::CatchupDecision::Fenced);
                    return Ok(());
                }
                let attempt = self.pending_catchup.take().expect("matched attempt");
                self.catchup_failed = false;
                let model = self.model.as_ref().expect("installed model");
                let valid = self.view.summary.participation == Participation::CatchingUp
                    && self
                        .catchup_started
                        .is_some_and(|start| start.elapsed() < Duration::from_secs(90))
                    && self
                        .sessions
                        .credential_context(model, generation, attempt.session)
                        .is_ok_and(|(context, _)| {
                            context.formation == *model.formation()
                                && context.sender
                                    == orishu_membership::SenderIdentity::Admitted(attempt.source)
                        });
                if valid && let Some(completed) = completed {
                    let (baseline, token) = completed.into_parts();
                    let snapshot = baseline.snapshot;
                    self.baseline_result = None;
                    self.apply(Message::Local(Command::InstallAdmissionBaseline(baseline)))?;
                    if self.baseline_result == Some((snapshot, true, false))
                        && self.local_admission_state_safe()
                    {
                        self.join_token = Some(token);
                        self.catchup_failed = false;
                        self.catchup_started = None;
                        self.catchup_route = None;
                        self.view.summary.participation = Participation::Joined;
                    }
                }
                self.catchup_failed = self.view.summary.participation != Participation::Joined;
                observation.decide(if !valid {
                    crate::formation_metrics::CatchupDecision::Fenced
                } else if self.view.summary.participation == Participation::Joined {
                    crate::formation_metrics::CatchupDecision::Adopted
                } else {
                    crate::formation_metrics::CatchupDecision::NotAdopted
                });
                return self.publish();
            }
            Control::PrepareJoin {
                parent,
                request,
                completion,
                reply,
            } => {
                let result = self
                    .join_operations
                    .reserve(&request, &self.view.summary)
                    .map(|reservation| match reservation {
                        crate::join_operations::Reservation::Replay(operation) => {
                            PreparedJoin::Replay(Box::new(operation))
                        }
                        crate::join_operations::Reservation::Start(operation) => {
                            PreparedJoin::Start(Box::new(JoinPreparation {
                                parent,
                                operation,
                                generation: self.view.generation,
                                local: self
                                    .model
                                    .as_ref()
                                    .expect("installed model")
                                    .local()
                                    .clone(),
                                request: Some(request),
                                completion: Some(completion),
                            }))
                        }
                    });
                let _ = reply.send(result);
                return Ok(());
            }
            Control::JoinStatus { id, reply } => {
                let _ = reply.send(self.join_operations.get(&id).cloned());
                return Ok(());
            }
            Control::InspectAdmission { request, reply } => {
                use orishu::model::cluster::{
                    AdmissionInspection, AdmissionInspectionOutcome as Outcome,
                };
                let model = self.model.as_ref().expect("installed model");
                let reference = &request.reference;
                let outcome = if request.formation_id != *model.formation()
                    || reference.introducer_node_id != *model.local_id()
                    || reference.introducer_fingerprint != model.local().cert_fingerprint
                {
                    Outcome::WrongIssuer
                } else if let Some(node_id) = self
                    .admission_replays
                    .assigned(reference.applicant_fingerprint, &reference.attempt_id)
                {
                    if crate::peer::session::validate_member(
                        model,
                        node_id,
                        reference.applicant_fingerprint,
                    )
                    .is_ok()
                    {
                        Outcome::CurrentMember {
                            node_id: node_id.clone(),
                        }
                    } else {
                        Outcome::RetiredOrRestricted {
                            node_id: node_id.clone(),
                        }
                    }
                } else {
                    Outcome::RecordUnavailable
                };
                let _ = reply.send(AdmissionInspection {
                    schema_version: 1,
                    request,
                    source_formation_id: model.formation().clone(),
                    source_node_id: model.local_id().clone(),
                    outcome,
                });
                return Ok(());
            }
            #[cfg(test)]
            Control::DisconnectPeers(reply) => {
                self.sessions.disconnect_all();
                let _ = reply.send(());
                return Ok(());
            }
            #[cfg(test)]
            Control::ExpireCatchup(reply) => {
                assert_eq!(self.view.summary.participation, Participation::CatchingUp);
                self.catchup_started = Some(Instant::now() - Duration::from_secs(90));
                self.publish()?;
                let _ = reply.send(());
                return Ok(());
            }
            Control::MemberDial { after, reply } => {
                use std::ops::Bound::{Excluded, Unbounded};
                let model = self.model.as_ref().expect("installed model");
                let mut page = MemberDialPage {
                    generation: self.view.generation,
                    after: None,
                    plan: None,
                };
                if self.view.summary.participation == Participation::CatchingUp
                    && let Some((node, fingerprint)) = self.catchup_route.take()
                    && model
                        .member(&node)
                        .is_some_and(|member| member.cert_fingerprint == fingerprint)
                    && self
                        .sessions
                        .member_connection(model, self.view.generation, &node)
                        .is_none()
                    && let Ok(target) = crate::peer::dial::MemberTarget::from_model(model, &node)
                {
                    // The admitting peer already knows this assignment. Give
                    // it one fresh admitted handshake before the ordinary scan,
                    // even when the canonical direction would await its dial.
                    // A failed attempt falls back to the normal bounded scan.
                    page.after = after;
                    page.plan = Some(MemberDialPlan {
                        node,
                        local: model.local().clone(),
                        target,
                    });
                    let _ = reply.send(page);
                    return Ok(());
                }
                if matches!(
                    self.view.summary.participation,
                    Participation::Standalone | Participation::CatchingUp | Participation::Joined
                ) {
                    for (node, _) in model
                        .members()
                        .range((after.as_ref().map_or(Unbounded, Excluded), Unbounded))
                        .take(64)
                    {
                        page.after = Some(node.clone());
                        if node == model.local_id()
                            || (!model.local().endpoints.peers.is_empty()
                                && model.local_id() > node)
                            || self
                                .sessions
                                .member_connection(model, self.view.generation, node)
                                .is_some()
                        {
                            continue;
                        }
                        if let Ok(target) = crate::peer::dial::MemberTarget::from_model(model, node)
                        {
                            page.plan = Some(MemberDialPlan {
                                node: node.clone(),
                                local: model.local().clone(),
                                target,
                            });
                            break;
                        }
                    }
                }
                let _ = reply.send(page);
                return Ok(());
            }
            Control::JoinPrepared {
                parent,
                generation,
                request,
                pending,
            } => {
                use orishu::model::cluster::JoinOperationState as State;
                let route = crate::peer::dial::IntroducerTarget::try_from(&request.material).ok();
                let valid = generation == self.view.generation
                    && request.formation_id == self.view.summary.formation_id
                    && self.view.summary.participation == Participation::Standalone;
                let binding = if valid {
                    pending.and_then(|pending| {
                        let target = route.clone()?;
                        let (connection, bytes) = pending.into_parts();
                        self.sessions
                            .accept_introducer_reply(
                                self.model.as_ref().expect("installed model"),
                                generation,
                                connection,
                                &bytes,
                                target,
                            )
                            .ok()
                    })
                } else {
                    None
                };
                let Some(binding) = binding else {
                    self.join_operations
                        .advance(&request.operation_id, State::FailedBeforeAdmission)
                        .map_err(|_| DriverError::Limit)?;
                    return Ok(());
                };
                let operation_id = request.operation_id.clone();
                self.active_join_operation = Some(request.operation_id);
                let (reply, _) = oneshot::channel();
                let result = self.control(Control::BeginJoin {
                    parent,
                    generation,
                    source: request.formation_id,
                    target: request.material.formation_id,
                    session: binding.session,
                    token: SecretToken::parse(request.material.token.expose().to_owned())
                        .map_err(|_| DriverError::Limit)?,
                    reply,
                });
                if let Some(join) = &mut self.outbound_join {
                    self.join_operations
                        .bind_recovery(
                            &operation_id,
                            orishu::model::cluster::JoinRecoveryReference {
                                attempt_id: join.attempt_id.clone(),
                                applicant_fingerprint: self
                                    .model
                                    .as_ref()
                                    .expect("installed model")
                                    .local()
                                    .cert_fingerprint,
                                introducer_node_id: request.material.introducer_node_id,
                                introducer_fingerprint: request.material.introducer_fingerprint,
                            },
                        )
                        .map_err(|_| DriverError::Limit)?;
                    join.route = route;
                }
                return result;
            }
            Control::BeginJoin {
                parent,
                generation,
                source,
                target,
                session,
                token,
                reply,
            } => {
                let model = self.model.as_ref().expect("owner holds model");
                if generation != self.view.generation
                    || &source != model.formation()
                    || self.view.summary.participation != Participation::Standalone
                    || model.join_attempt().is_some()
                    || self
                        .sessions
                        .introducer_connection(model, generation, session, &target)
                        .is_none()
                {
                    let _ = reply.send(Err(DriverError::PeerAdapterUnavailable));
                    return Ok(());
                }
                self.outbound_join = Some(JoinTransport {
                    parent,
                    attempt_id: SecretToken::generate()
                        .map_err(|_| DriverError::Entropy)?
                        .expose()
                        .parse()
                        .map_err(|_| DriverError::Limit)?,
                    route: None,
                    reconnect_attempts: 0,
                    reconnect_pending: None,
                    emitted: false,
                    session,
                    target: target.clone(),
                    token,
                });
                self.view.summary.participation = Participation::Joining;
                let result = self.apply(Message::Local(Command::BeginJoin {
                    session,
                    target_formation: target,
                }));
                let _ = reply.send(result);
                return result;
            }
            Control::JoinMaterial(reply) => {
                let model = self.model.as_ref().expect("owner holds model");
                let material = self
                    .join_token
                    .as_ref()
                    .filter(|_| !model.local().endpoints.peers.is_empty())
                    .map(|token| orishu::model::cluster::JoinMaterial {
                        schema_version: 1,
                        formation_id: model.formation().clone(),
                        introducer_node_id: model.local_id().clone(),
                        introducer_fingerprint: model.local().cert_fingerprint,
                        peer_endpoints: model
                            .local()
                            .endpoints
                            .peers
                            .iter()
                            .map(|a| a.0.clone())
                            .collect(),
                        introducer_ready: self.view.summary.introducer_ready,
                        token: token
                            .expose()
                            .to_owned()
                            .try_into()
                            .expect("validated owner token"),
                    });
                let _ = reply.send(material);
                return Ok(());
            }
            Control::List {
                formation,
                after,
                reply,
            } => {
                let model = self.model.as_ref().expect("owner holds model");
                let valid = !(after.is_some() && formation.is_none())
                    && formation.as_ref().is_none_or(|f| f == model.formation());
                let _ = reply.send(valid.then(|| crate::membership_view::page(model, after)));
                return Ok(());
            }
            Control::Inspect { node, reply } => {
                let model = self.model.as_ref().expect("owner holds model");
                let projection = model
                    .members()
                    .get(&node)
                    .map(|member| crate::membership_view::inspect(model, member));
                let _ = reply.send(projection);
                return Ok(());
            }
            Control::LockOperation {
                generation,
                request,
                reply,
            } => {
                let outcome = self.lock_operation(generation, request);
                let fatal = match &outcome {
                    Err(LockError::Driver(error)) => Some(*error),
                    _ => None,
                };
                let _ = reply.send(outcome);
                return fatal.map_or(Ok(()), Err);
            }
            control => control,
        };
        let Control::SetLock {
            generation,
            formation,
            locked,
            reply,
        } = control
        else {
            if let Control::Input(input) = control {
                return self.deliver(input);
            }
            unreachable!();
        };
        let outcome = self.decide_lock(generation, formation, locked);
        let fatal = match &outcome {
            Err(LockError::Driver(error)) => Some(*error),
            _ => None,
        };
        let _ = reply.send(outcome);
        fatal.map_or(Ok(()), Err)
    }

    fn decide_lock(
        &mut self,
        generation: Generation,
        formation: orishu_membership::FormationId,
        locked: bool,
    ) -> Result<Summary, LockError> {
        if generation != self.view.generation
            || &formation != self.model.as_ref().expect("installed model").formation()
        {
            return Err(LockError::StaleFormation);
        }
        if !matches!(
            self.view.summary.participation,
            Participation::Standalone | Participation::Joined
        ) {
            return Err(LockError::Unavailable);
        }
        // Successful submission is not acceptance: inspect the model only after
        // the complete pure transition and its inline effects have finished.
        self.apply(Message::Local(Command::SetMembershipLock(locked)))?;
        if self.view.summary.membership_locked == locked {
            Ok(self.view.summary.clone())
        } else {
            Err(LockError::Rejected)
        }
    }

    fn lock_operation(
        &mut self,
        generation: Generation,
        request: LockRequest,
    ) -> Result<LockReceipt, LockError> {
        if request.schema_version != 1 {
            return Err(LockError::InvalidSchema);
        }
        if generation != self.view.generation
            || &request.formation_id != self.model.as_ref().expect("installed model").formation()
        {
            return Err(LockError::StaleFormation);
        }
        if let Some((original, result)) = self.lock_operations.get(&request.operation_id) {
            return if original == &request {
                result.clone()
            } else {
                Err(LockError::Conflict)
            };
        }
        if self.lock_operations.len() == MAX_LOCK_OPERATIONS {
            return Err(LockError::HistoryFull);
        }
        let outcome = self
            .decide_lock(generation, request.formation_id.clone(), request.locked)
            .map(|summary| LockReceipt {
                schema_version: 1,
                operation_id: request.operation_id.clone(),
                formation_id: summary.formation_id,
                source_node_id: summary.source_node_id,
                locked: request.locked,
                policy_version: self
                    .model
                    .as_ref()
                    .expect("installed model")
                    .membership_policy()
                    .map(|policy| policy.version.clone()),
            });
        self.lock_operations
            .insert(request.operation_id.clone(), (request, outcome.clone()));
        outcome
    }

    fn leave_operation(
        &mut self,
        generation: Generation,
        request: LeaveRequest,
    ) -> Result<LeaveReceipt, LeaveError> {
        if request.schema_version != 1 {
            return Err(LeaveError::InvalidSchema);
        }
        if let Some((original, receipt)) = self.leave_operations.get(&request.operation_id) {
            return if original == &request {
                Ok(receipt.clone())
            } else {
                Err(LeaveError::Conflict)
            };
        }
        if generation != self.view.generation
            || request.formation_id != self.view.summary.formation_id
        {
            return Err(LeaveError::StaleFormation);
        }
        if matches!(
            self.view.summary.participation,
            Participation::Joining | Participation::JoinUnresolved | Participation::Stopping
        ) {
            return Err(LeaveError::Unavailable);
        }
        if self.leave_operations.len() == MAX_LEAVE_OPERATIONS {
            return Err(LeaveError::HistoryFull);
        }
        let before = self.view.summary.clone();
        let changed = before.participation != Participation::Standalone || before.member_count != 1;
        if changed {
            let formation = SecretToken::generate().map_err(|_| DriverError::Entropy)?;
            let node = SecretToken::generate().map_err(|_| DriverError::Entropy)?;
            self.apply(Message::Local(Command::Leave {
                replacement_formation: formation
                    .expose()
                    .parse()
                    .map_err(|_| DriverError::Limit)?,
                replacement_node_id: node.expose().parse().map_err(|_| DriverError::Limit)?,
                replacement_cluster_name: before.cluster_name.clone(),
            }))?;
        }
        let receipt = LeaveReceipt {
            schema_version: 1,
            operation_id: request.operation_id.clone(),
            previous_formation_id: before.formation_id,
            previous_node_id: before.source_node_id,
            changed,
            current: self.view.summary.clone(),
        };
        self.leave_operations
            .insert(request.operation_id.clone(), (request, receipt.clone()));
        Ok(receipt)
    }

    fn deliver(&mut self, input: Input) -> Result<(), DriverError> {
        if input.generation != self.view.generation
            || (self.view.summary.participation == Participation::Ejected
                && !matches!(&input.message, Message::Local(Command::Leave { .. })))
        {
            self.view.stale_inputs = self.view.stale_inputs.saturating_add(1);
            self.published.send_replace(self.view.clone());
            Ok(())
        } else {
            self.apply(input.message)
        }
    }
    fn apply(&mut self, message: Message) -> Result<(), DriverError> {
        let mut pending = VecDeque::from([message]);
        let mut steps = 0;
        while let Some(message) = pending.pop_front() {
            steps += 1;
            if steps > MAX_INLINE_TRANSITIONS {
                return Err(DriverError::Limit);
            }
            let model = self
                .model
                .take()
                .expect("owner has membership outside update");
            let previous = (model.formation().clone(), model.local_id().clone());
            // Leave effects refer to the formation being abandoned, whereas
            // the transition's model already contains the replacement. Resolve
            // only its bounded notification candidates before handing off the
            // old model; never look these routes up in the replacement model.
            let leaving_routes: BTreeMap<_, _> =
                if matches!(&message, Message::Local(Command::Leave { .. })) {
                    model
                        .members()
                        .values()
                        .filter(|member| {
                            member.id != *model.local_id() && member.liveness == Liveness::Alive
                        })
                        .take(model.limits().indirect_probe_count().saturating_mul(2))
                        .filter_map(|member| {
                            self.sessions
                                .member_connection(&model, self.view.generation, &member.id)
                                .map(|(_, connection)| (member.id.clone(), connection))
                        })
                        .collect()
                } else {
                    BTreeMap::new()
                };
            let timer = match &message {
                Message::Timer(token) => Some(*token),
                _ => None,
            };
            let mut transition = orishu_membership::update(model, message);
            self.formation_metrics
                .record(timer, &transition.diagnostics);
            if transition.effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::Publish(orishu_membership::ChangeRecord::FormationLeft { .. })
                )
            }) {
                #[cfg(feature = "formation-fault-test")]
                let drop_departure = std::mem::take(&mut self.drop_next_departure);
                #[cfg(feature = "formation-fault-test")]
                let mut dropped_departures = 0_usize;
                transition.effects.retain(|effect| {
                    let Effect::Send {
                        destination: orishu_membership::Destination::Member(node),
                        message,
                    } = effect
                    else {
                        return true;
                    };
                    // The core emits only these sends for leave. Drop any
                    // unexpected send rather than granting it old authority.
                    let encoded = encode_departure(message.clone(), &previous.0, &previous.1);
                    #[cfg(feature = "formation-fault-test")]
                    if drop_departure && encoded.is_ok() {
                        dropped_departures += 1;
                        self.view.failed_sends = self.view.failed_sends.saturating_add(1);
                        return false;
                    }
                    let sent = encoded.ok().zip(leaving_routes.get(node)).is_some_and(
                        |(encoded, connection)| self.packet_io.submit(connection, encoded).is_ok(),
                    );
                    if !sent {
                        self.view.failed_sends = self.view.failed_sends.saturating_add(1);
                    }
                    false
                });
                #[cfg(feature = "formation-fault-test")]
                if drop_departure {
                    println!(
                        "FORMATION_TEST_DROPPED_DEPARTURE {} {dropped_departures}",
                        previous.1
                    );
                }
            }
            if previous
                != (
                    transition.model.formation().clone(),
                    transition.model.local_id().clone(),
                )
            {
                self.view.generation.0 = self
                    .view
                    .generation
                    .0
                    .checked_add(1)
                    .ok_or(DriverError::Limit)?;
                self.timers.clear();
                self.sends.abort_all();
                self.join_token = None;
                self.pending_catchup = None;
                self.catchup_started = None;
                self.catchup_attempts = 0;
                self.catchup_route_notified = false;
                self.catchup_failed = false;
                self.catchup_route = self
                    .outbound_join
                    .as_ref()
                    .and_then(|join| join.route.as_ref())
                    .filter(|route| route.formation == *transition.model.formation())
                    .map(|route| (route.node.clone(), route.fingerprint));
                self.outbound_join = None;
                self.admission_replays = Default::default();
                self.lock_operations.clear();
                pending.clear();
            }
            self.view.transitions = self.view.transitions.saturating_add(1);
            self.view.diagnostics = self
                .view
                .diagnostics
                .saturating_add(transition.diagnostics.len() as u64);
            self.view.foreign_gossip = self
                .view
                .foreign_gossip
                .saturating_add(transition.foreign_gossip.len() as u64);
            self.model = Some(transition.model);
            // Removal can arrive in ordinary gossip or reconciliation, not
            // only catch-up. Fence the entire effect batch before any response,
            // timer or follow-up work can run as the removed local identity.
            let local_id = self.model.as_ref().expect("installed model").local_id();
            let removed = transition.effects.iter().any(|effect| {
                matches!(effect,
                    Effect::Publish(orishu_membership::ChangeRecord::MemberRemoved { node, .. })
                        if node == local_id)
                    || matches!(
                        effect,
                        Effect::Publish(
                            orishu_membership::ChangeRecord::AdmissionBaselineApplied {
                                self_removed: true,
                                ..
                            }
                        )
                    )
            });
            if removed {
                for effect in &transition.effects {
                    if let Effect::Publish(
                        orishu_membership::ChangeRecord::AdmissionBaselineApplied {
                            snapshot, ..
                        },
                    ) = effect
                    {
                        self.baseline_result = Some((*snapshot, true, true));
                    }
                }
                self.eject()?;
                pending.clear();
                continue;
            }
            self.sessions.synchronize(
                self.model.as_ref().expect("installed model"),
                self.view.generation,
            );
            self.admission_baselines.sweep(
                self.model.as_ref().expect("installed model"),
                self.view.generation,
                Instant::now().into_std(),
            );
            for effect in transition.effects {
                match effect {
                    Effect::ArmTimer { token, delay } => {
                        if self.timers.len() == MAX_TIMERS && !self.timers.contains_key(&token) {
                            return Err(DriverError::Limit);
                        }
                        self.timers.insert(
                            token,
                            Instant::now()
                                .checked_add(Duration::from_millis(delay.0))
                                .ok_or(DriverError::Limit)?,
                        );
                    }
                    Effect::CancelTimer { token } => {
                        self.timers.remove(&token);
                    }
                    Effect::SelectPeers {
                        request,
                        purpose,
                        count,
                        exclude,
                    } => {
                        let model = self.model.as_ref().expect("installed model");
                        // O(members log members) sorting plus at most one bounded
                        // registry revalidation per reconciliation candidate.
                        // Outside simulation hot paths; independent random keys
                        // avoid preference for lexical node IDs.
                        let mut eligible = Vec::new();
                        for member in model.members().values().filter(|member| {
                            matches!(member.liveness, Liveness::Alive | Liveness::Suspected)
                                && &member.id != model.local_id()
                                && !exclude.contains(&member.id)
                        }) {
                            // Reconciliation needs an existing reliable route;
                            // an unsent pull must not occupy the round deadline.
                            // SWIM still selects disconnected members normally.
                            if matches!(purpose, orishu_membership::SelectionPurpose::AntiEntropy)
                                && self
                                    .sessions
                                    .member_connection(model, self.view.generation, &member.id)
                                    .is_none()
                            {
                                continue;
                            }
                            let random =
                                SecretToken::generate().map_err(|_| DriverError::Entropy)?;
                            eligible.push((random.expose().to_owned(), member.id.clone()));
                        }
                        eligible.sort_unstable();
                        pending.push_back(Message::Outcome(EffectOutcome::PeersSelected {
                            request,
                            peers: eligible.into_iter().take(count).map(|(_, id)| id).collect(),
                        }));
                    }
                    Effect::AllocateNodeId { request, session } => {
                        let outcome = match SecretToken::generate() {
                            Ok(value) => EffectOutcome::NodeIdAllocated {
                                request,
                                session,
                                node_id: NodeId::new(value.expose()).expect("random hex identity"),
                            },
                            Err(_) => EffectOutcome::NodeIdUnavailable { request, session },
                        };
                        pending.push_back(Message::Outcome(outcome));
                    }
                    Effect::Publish(change) => {
                        use orishu_membership::ChangeRecord;
                        match change {
                            ChangeRecord::MemberAdmitted { .. } => {
                                self.view.admissions.accepted =
                                    self.view.admissions.accepted.saturating_add(1);
                            }
                            ChangeRecord::AdmissionRejected { .. } => {
                                self.view.admissions.rejected =
                                    self.view.admissions.rejected.saturating_add(1);
                            }
                            ChangeRecord::FormationAdopted { .. } => {
                                self.view.summary.participation = Participation::CatchingUp;
                                self.catchup_started = Some(Instant::now());
                            }
                            ChangeRecord::AdmissionBaselineApplied {
                                snapshot,
                                self_removed,
                            } => {
                                self.baseline_result = Some((snapshot, true, self_removed));
                            }
                            ChangeRecord::AdmissionBaselineRejected { snapshot } => {
                                self.baseline_result = Some((snapshot, false, false));
                            }
                            ChangeRecord::FormationLeft { .. } => {
                                self.join_operations.end_lifecycle();
                                self.active_join_operation = None;
                                self.view.summary.participation = Participation::Standalone;
                                self.join_token = Some(
                                    SecretToken::generate().map_err(|_| DriverError::Entropy)?,
                                );
                            }
                            _ => {}
                        }
                    }
                    // A reliable session reply belongs only to the currently
                    // decoded request. Do not route it onto an arbitrary stream.
                    Effect::Send {
                        destination: orishu_membership::Destination::Session(session),
                        message,
                    } => {
                        if matches!(
                            &message.body,
                            orishu_membership::OutboundBody::JoinRequest { .. }
                        ) {
                            let model = self.model.as_ref().expect("installed model");
                            let join = self
                                .outbound_join
                                .as_ref()
                                .filter(|join| join.session == session)
                                .ok_or(DriverError::PeerAdapterUnavailable)?;
                            let encoded = crate::peer::wire::encode_join(
                                message,
                                join.target.clone(),
                                orishu_membership::SenderIdentity::Applicant(
                                    model.local_name().clone(),
                                ),
                                &join.token,
                                &join.attempt_id,
                            )
                            .map_err(|_| DriverError::Limit)?;
                            if let Some(connection) = self.sessions.introducer_connection(
                                model,
                                self.view.generation,
                                session,
                                &join.target,
                            ) && self.sends.len() < MAX_RELIABLE_SENDS
                            {
                                // Only the identified join owns this ancestry.
                                // Retries retain it; unrelated owner work never
                                // reads a last-client/global context slot.
                                #[cfg(feature = "otlp-tracing")]
                                let active = self.tracing.get().and_then(|queue| {
                                    queue.try_begin_with_parent(
                                        crate::trace_export::Operation::PeerExchange,
                                        join.parent,
                                    )
                                });
                                #[cfg(not(feature = "otlp-tracing"))]
                                let _ = join.parent;
                                #[cfg(feature = "otlp-tracing")]
                                let encoded = encoded.with_parent(
                                    active
                                        .as_ref()
                                        .map(crate::trace_export::ActiveSpan::context),
                                );
                                let generation = self.view.generation;
                                self.outbound_join.as_mut().expect("active join").emitted = true;
                                if let Some(id) = &self.active_join_operation {
                                    self.join_operations
                                        .advance(
                                            id,
                                            orishu::model::cluster::JoinOperationState::Admitting,
                                        )
                                        .map_err(|_| DriverError::Limit)?;
                                }
                                let exchange = self.exchanges.clone();
                                self.sends.spawn(async move {
                                    let result = exchange
                                        .request(
                                            &connection,
                                            &encoded.bytes,
                                            crate::peer::exchange::Phase::Membership,
                                        )
                                        .await;
                                    #[cfg(feature = "otlp-tracing")]
                                    if let Some(active) = active {
                                        active.finish(if result.is_ok() {
                                            crate::trace_export::Outcome::Completed
                                        } else {
                                            crate::trace_export::Outcome::Failed
                                        });
                                    }
                                    (generation, session, result)
                                });
                            } else {
                                self.view.failed_sends = self.view.failed_sends.saturating_add(1);
                            }
                            continue;
                        }
                        if let orishu_membership::OutboundBody::JoinAccepted { assigned, .. } =
                            &message.body
                        {
                            let (fingerprint, attempt) = self
                                .packet
                                .as_ref()
                                .and_then(|packet| packet.join_attempt.clone())
                                .ok_or(DriverError::PeerAdapterUnavailable)?;
                            self.admission_replays
                                .commit(fingerprint, attempt, assigned.clone(), message.seq)
                                .map_err(|_| DriverError::Limit)?;
                            self.sessions
                                .promote(
                                    self.model.as_ref().expect("installed model"),
                                    self.view.generation,
                                    session,
                                    assigned.clone(),
                                )
                                .map_err(|_| DriverError::PeerAdapterUnavailable)?;
                        }
                        let Some(packet) = self.packet.as_mut().filter(|packet| {
                            packet.session == session
                                && packet.generation == self.view.generation
                                && packet.transport == crate::peer::wire::Transport::Stream
                        }) else {
                            return Err(DriverError::PeerAdapterUnavailable);
                        };
                        if packet.response.is_some() {
                            return Err(DriverError::Limit);
                        }
                        #[cfg(feature = "otlp-tracing")]
                        if matches!(
                            message.body,
                            orishu_membership::OutboundBody::JoinRejected { .. }
                                | orishu_membership::OutboundBody::JoinRedirect { .. }
                        ) {
                            packet.admission_outcome = crate::trace_export::Outcome::Rejected;
                        }
                        let model = self.model.as_ref().expect("installed model");
                        let encoded = crate::peer::wire::encode(
                            message,
                            model.formation().clone(),
                            orishu_membership::SenderIdentity::Admitted(model.local_id().clone()),
                            None,
                        )
                        .map_err(|_| DriverError::Limit)?;
                        if encoded.transport != crate::peer::wire::Transport::Stream {
                            return Err(DriverError::Limit);
                        }
                        packet.response = Some(encoded);
                    }
                    Effect::VerifyCredential {
                        request,
                        session,
                        kind,
                    } => {
                        let packet = self
                            .packet
                            .as_mut()
                            .filter(|packet| {
                                packet.session == session
                                    && packet.generation == self.view.generation
                            })
                            .ok_or(DriverError::PeerAdapterUnavailable)?;
                        let presented = packet.token.take();
                        let orishu_membership::CredentialKind::JoinToken {
                            name,
                            cert_fingerprint,
                            blocked_networks,
                        } = kind;
                        let context = self.sessions.credential_context(
                            self.model.as_ref().expect("installed model"),
                            self.view.generation,
                            session,
                        );
                        let evidence = match context {
                            Ok((context, source))
                                if context.cert_fingerprint == cert_fingerprint
                                    && context.sender
                                        == orishu_membership::SenderIdentity::Applicant(name) =>
                            {
                                match self.join_token.as_ref().filter(|_| {
                                    matches!(
                                        self.view.summary.participation,
                                        Participation::Standalone | Participation::Joined
                                    )
                                }) {
                                    Some(expected) => {
                                        crate::peer::admission::verify(
                                            expected,
                                            presented.as_ref(),
                                            source,
                                            context.authenticated,
                                            &blocked_networks,
                                        )
                                        .evidence
                                    }
                                    None => orishu_membership::AdmissionEvidence {
                                        transport_authenticated: context.authenticated,
                                        token_valid: false,
                                        source_network_blocked:
                                            crate::peer::admission::check_source(
                                                source,
                                                &blocked_networks,
                                            )
                                            .blocked,
                                    },
                                }
                            }
                            _ => orishu_membership::AdmissionEvidence {
                                transport_authenticated: false,
                                token_valid: false,
                                source_network_blocked: true,
                            },
                        };
                        pending.push_back(Message::Outcome(EffectOutcome::CredentialVerified {
                            request,
                            session,
                            evidence,
                        }));
                    }
                    Effect::Send {
                        destination: orishu_membership::Destination::Member(node),
                        mut message,
                    } => {
                        let model = self.model.as_ref().expect("installed model");
                        let response = matches!(
                            &message.body,
                            orishu_membership::OutboundBody::PullReply { .. }
                        );
                        let route = if response {
                            None
                        } else {
                            self.sessions
                                .member_connection(model, self.view.generation, &node)
                        };
                        // Preparing a send is not transmission: a missing route
                        // must not consume any of its offered gossip allowance.
                        let unsent = if !response && route.is_none() {
                            std::mem::take(&mut message.gossip)
                        } else {
                            Vec::new()
                        };
                        let (encoded, trimmed) = crate::peer::wire::encode_member(
                            message,
                            model.formation().clone(),
                            orishu_membership::SenderIdentity::Admitted(model.local_id().clone()),
                        )
                        .map_err(|_| DriverError::Limit)?;
                        let deferred = if !unsent.is_empty() { unsent } else { trimmed };
                        if !deferred.is_empty() {
                            pending.push_back(Message::Outcome(EffectOutcome::GossipDeferred {
                                deltas: deferred,
                            }));
                        }
                        if response {
                            let packet = self
                                .packet
                                .as_mut()
                                .filter(|packet| {
                                    packet.member.as_ref() == Some(&node)
                                        && packet.generation == self.view.generation
                                        && packet.transport == crate::peer::wire::Transport::Stream
                                })
                                .ok_or(DriverError::Limit)?;
                            if packet.response.is_some() {
                                return Err(DriverError::Limit);
                            }
                            packet.response = Some(encoded);
                        } else if let Some((session, connection)) = route {
                            if encoded.transport == crate::peer::wire::Transport::Datagram {
                                if self.packet_io.submit(&connection, encoded).is_err() {
                                    self.view.failed_sends =
                                        self.view.failed_sends.saturating_add(1);
                                }
                            } else if self.sends.len() < MAX_RELIABLE_SENDS {
                                let generation = self.view.generation;
                                let exchange = self.exchanges.clone();
                                self.sends.spawn(async move {
                                    (
                                        generation,
                                        session,
                                        exchange
                                            .request(
                                                &connection,
                                                &encoded.bytes,
                                                crate::peer::exchange::Phase::Membership,
                                            )
                                            .await,
                                    )
                                });
                            } else {
                                self.view.failed_sends = self.view.failed_sends.saturating_add(1);
                            }
                        } else {
                            self.view.failed_sends = self.view.failed_sends.saturating_add(1);
                        }
                    }
                }
            }
            if pending.len() > MAX_INLINE_TRANSITIONS {
                return Err(DriverError::Limit);
            }
        }
        if self
            .model
            .as_ref()
            .expect("installed model")
            .join_attempt()
            .is_none()
        {
            let possibly_admitted = self.outbound_join.take().is_some_and(|join| join.emitted);
            if self.view.summary.participation == Participation::Joining {
                self.view.summary.participation = if possibly_admitted {
                    Participation::JoinUnresolved
                } else {
                    Participation::Standalone
                };
                self.sends.abort_all();
            }
        }
        self.publish()
    }

    fn eject(&mut self) -> Result<(), DriverError> {
        self.admission_replays = Default::default();
        self.catchup_failed = self.view.summary.participation == Participation::CatchingUp;
        self.view.summary.participation = Participation::Ejected;
        self.view.generation.0 = self
            .view
            .generation
            .0
            .checked_add(1)
            .ok_or(DriverError::Limit)?;
        self.join_token = None;
        self.pending_catchup = None;
        self.catchup_started = None;
        self.catchup_route = None;
        self.outbound_join = None;
        self.timers.clear();
        self.sends.abort_all();
        self.sessions.close_all();
        self.admission_baselines.sweep(
            self.model.as_ref().expect("installed model"),
            self.view.generation,
            Instant::now().into_std(),
        );
        if let Some(packet) = &mut self.packet {
            packet.response = None;
            packet.token = None;
        }
        Ok(())
    }

    fn local_admission_state_safe(&self) -> bool {
        use orishu_membership::model::{BlocklistAction, BlocklistKey};
        let model = self.model.as_ref().expect("installed model");
        model.members().contains_key(model.local_id())
            && ![
                BlocklistKey::Node(model.local_id().clone()),
                BlocklistKey::Name(model.local_name().clone()),
                BlocklistKey::Fingerprint(model.local().cert_fingerprint),
            ]
            .iter()
            .any(|key| model.is_blocked(key))
            && !model.tombstones().values().any(|entry| {
                !entry.cleared && entry.cert_fingerprint == model.local().cert_fingerprint
            })
            && crate::peer::admission::validate_networks(model.blocklist().values().filter_map(
                |entry| {
                    if entry.action == BlocklistAction::Block
                        && let BlocklistKey::Network(pattern) = &entry.key
                    {
                        Some(pattern)
                    } else {
                        None
                    }
                },
            ))
            .is_ok()
    }

    fn publish(&mut self) -> Result<(), DriverError> {
        if self.view.summary.participation == Participation::CatchingUp
            && self
                .catchup_started
                .is_some_and(|start| start.elapsed() >= Duration::from_secs(90))
        {
            self.catchup_failed = true;
        }
        self.view.summary = project_summary(
            self.model.as_ref().expect("installed model"),
            self.view.summary.participation,
        );
        self.view.summary.introducer_ready = introducer_ready(
            self.model.as_ref().expect("installed model"),
            self.view.summary.participation,
            self.join_token.is_some(),
        ) && self.local_admission_state_safe();
        if let Some(id) = &self.active_join_operation {
            use orishu::model::cluster::JoinOperationState as State;
            let next = match self.view.summary.participation {
                Participation::CatchingUp if self.catchup_failed => Some(State::CatchUpFailed {
                    node_id: self.view.summary.source_node_id.clone(),
                }),
                Participation::CatchingUp => Some(State::CatchingUp {
                    node_id: self.view.summary.source_node_id.clone(),
                }),
                Participation::Joined => Some(State::Joined {
                    node_id: self.view.summary.source_node_id.clone(),
                }),
                Participation::Ejected if self.catchup_failed => Some(State::CatchUpFailed {
                    node_id: self.view.summary.source_node_id.clone(),
                }),
                Participation::JoinUnresolved => Some(State::Unresolved),
                Participation::Standalone => Some(State::FailedBeforeAdmission),
                _ => None,
            };
            if let Some(next) = next {
                self.join_operations
                    .advance(id, next)
                    .map_err(|_| DriverError::Limit)?;
            }
        }
        let previous = self.published.send_replace(self.view.clone());
        if self.view.summary.participation == Participation::CatchingUp
            && (previous.generation != self.view.generation
                || previous.summary.participation != Participation::CatchingUp)
        {
            self.formation_adopted.notify_one();
        }
        Ok(())
    }
}

// Departures are the only effects permitted to use the abandoned identity.
// This is best-effort datagram submission, never a claim of peer receipt.
fn encode_departure(
    message: orishu_membership::OutboundMessage,
    formation: &orishu_membership::FormationId,
    sender: &NodeId,
) -> Result<crate::peer::wire::Encoded, DriverError> {
    if !matches!(&message.body,
        orishu_membership::OutboundBody::Announce {
            announcement: orishu_membership::Announcement::Leave, target, ..
        } if target == sender)
    {
        return Err(DriverError::Limit);
    }
    crate::peer::wire::encode(
        message,
        formation.clone(),
        orishu_membership::SenderIdentity::Admitted(sender.clone()),
        None,
    )
    .map_err(|_| DriverError::Limit)
}

// A fresh standalone formation owns its complete initial admission state.
// Adopted formations must not become ready until an explicit catch-up contract
// installs target credentials and advances participation to Joined.
fn introducer_ready(model: &Membership, participation: Participation, has_token: bool) -> bool {
    has_token
        && matches!(
            participation,
            Participation::Standalone | Participation::Joined
        )
        && model.policy().accepts_peers
        && model.local().accepts.peers
        && !model.local().endpoints.peers.is_empty()
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "otlp-tracing")]
    #[test]
    fn admission_trace_parent_requires_the_current_join_credential() {
        let expected = super::SecretToken::parse("a".repeat(64)).unwrap();
        let wrong = super::SecretToken::parse("b".repeat(64)).unwrap();
        for sampled in [false, true] {
            let parent = crate::trace_context::TraceParent::new([1; 16], [2; 8], sampled);
            assert_eq!(
                super::admission_trace_parent(Some(&expected), Some(&expected), parent),
                parent
            );
            for (current, presented) in [
                (Some(&expected), Some(&wrong)),
                (None, Some(&expected)),
                (Some(&expected), None),
                (None, None),
            ] {
                assert!(super::admission_trace_parent(current, presented, parent).is_none());
            }
            assert!(
                super::admission_trace_parent(Some(&expected), Some(&expected), None).is_none()
            );
        }
    }

    use super::*;
    use orishu_membership::{ClusterName, FormationId, testing};

    #[test]
    fn received_activity_counts_items_not_merges_and_saturates() {
        use orishu_membership::{DeltaBody, GossipDelta, PeerBody, PeerInput};
        let model = testing::model_with_members(1);
        let delta = GossipDelta {
            hops: 0,
            body: DeltaBody::MembershipUpdate(model.member(model.local_id()).unwrap().clone()),
        };
        let mut input = PeerInput {
            context: testing::peer_context(&model, model.local_id(), 1),
            body: PeerBody::Ping {
                probe: orishu_membership::ProbeId(1),
                incarnation: orishu_membership::Incarnation::INITIAL,
            },
        };
        input.context.gossip = vec![delta.clone(), delta.clone()];
        let mut counts = TrafficCounters::default();
        counts.record(&input);
        counts.record(&input);
        assert_eq!(
            counts,
            TrafficCounters {
                swim: 2,
                anti_entropy: 0,
                gossip_items: 4
            }
        );
        input.body = PeerBody::PullReply {
            round: 1,
            digest: orishu_membership::antientropy::MembershipTree::build(&model).digest(),
            deltas: vec![delta; 3],
            complete: false,
            cursor: None,
        };
        counts.record(&input);
        assert_eq!(
            counts,
            TrafficCounters {
                swim: 2,
                anti_entropy: 1,
                gossip_items: 9
            }
        );
        input.context.gossip.clear();
        input.body =
            PeerBody::JoinReply(orishu_membership::JoinReply::Redirect { candidates: vec![] });
        let before = counts;
        counts.record(&input);
        assert_eq!(counts, before, "join reply is not SWIM or anti-entropy");
        counts = TrafficCounters {
            swim: u64::MAX,
            anti_entropy: u64::MAX,
            gossip_items: u64::MAX,
        };
        input.context.gossip.push(GossipDelta {
            hops: 0,
            body: DeltaBody::MembershipUpdate(model.member(model.local_id()).unwrap().clone()),
        });
        input.body = PeerBody::PullRequest {
            round: 1,
            digest: orishu_membership::antientropy::MembershipTree::build(&model).digest(),
            buckets: vec![],
            cursor: None,
        };
        counts.record(&input);
        input.body = PeerBody::Ack {
            probe: orishu_membership::ProbeId(1),
            incarnation: orishu_membership::Incarnation::INITIAL,
        };
        counts.record(&input);
        assert_eq!(
            counts,
            TrafficCounters {
                swim: u64::MAX,
                anti_entropy: u64::MAX,
                gossip_items: u64::MAX
            }
        );
    }

    #[tokio::test]
    async fn reserved_control_disposes_payload_on_either_side_of_receiver_drop() {
        for close_first in [true, false] {
            let (sender, receiver) = mpsc::channel(1);
            let permit = sender.clone().try_reserve_owned().unwrap();
            let (reply, response) = oneshot::channel();
            let mut receiver = Some(receiver);
            if close_first {
                drop(receiver.take());
            }
            send_reserved_control(
                permit,
                Control::JoinStatus {
                    id: "disposed-completion".parse().unwrap(),
                    reply,
                },
            );
            drop(receiver);
            assert!(
                tokio::time::timeout(Duration::from_secs(1), response)
                    .await
                    .expect("payload disposed despite retained sender")
                    .is_err()
            );
            assert!(sender.is_closed());
        }
    }

    #[tokio::test]
    async fn identified_leave_retains_exact_receipt_across_identity_change() {
        let (handle, task) = spawn_standalone(testing::model_with_members(2));
        let before = handle.view().unwrap();
        handle
            .set_membership_lock(before.generation, before.summary.formation_id.clone(), true)
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        let request = LeaveRequest {
            schema_version: 1,
            operation_id: "leave-one".parse().unwrap(),
            formation_id: before.summary.formation_id.clone(),
        };
        // Losing the caller does not undo acceptance or lose its retry receipt.
        drop(handle.leave_operation(request.clone()).unwrap());
        let receipt = handle
            .leave_operation(request.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(receipt.changed);
        assert_eq!(receipt.previous_node_id, before.summary.source_node_id);
        assert_ne!(receipt.current.source_node_id, receipt.previous_node_id);
        assert_ne!(receipt.current.formation_id, receipt.previous_formation_id);
        assert_eq!(receipt.current.participation, Participation::Standalone);
        assert_eq!(receipt.current.member_count, 1);
        assert!(!receipt.current.membership_locked);
        assert_eq!(
            handle
                .leave_operation(request.clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap(),
            receipt
        );
        let conflict = LeaveRequest {
            formation_id: receipt.current.formation_id.clone(),
            ..request.clone()
        };
        assert_eq!(
            handle.leave_operation(conflict).unwrap().await.unwrap(),
            Err(LeaveError::Conflict)
        );
        let stale = LeaveRequest {
            operation_id: "stale-leave".parse().unwrap(),
            ..request
        };
        assert_eq!(
            handle.leave_operation(stale).unwrap().await.unwrap(),
            Err(LeaveError::StaleFormation)
        );
        let noop = LeaveRequest {
            schema_version: 1,
            operation_id: "noop".parse().unwrap(),
            formation_id: receipt.current.formation_id.clone(),
        };
        let noop = handle
            .leave_operation(noop)
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(!noop.changed);
        assert_eq!(noop.current, receipt.current);
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn leave_history_refuses_overflow_without_evicting_receipts() {
        let (handle, task) = spawn_standalone(testing::standalone("local"));
        let source = handle.view().unwrap().summary;
        let first = LeaveRequest {
            schema_version: 1,
            operation_id: "leave-0".parse().unwrap(),
            formation_id: source.formation_id,
        };
        let receipt = handle
            .leave_operation(first.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        for index in 1..MAX_LEAVE_OPERATIONS {
            handle
                .leave_operation(LeaveRequest {
                    operation_id: format!("leave-{index}").parse().unwrap(),
                    ..first.clone()
                })
                .unwrap()
                .await
                .unwrap()
                .unwrap();
        }
        let overflow = LeaveRequest {
            operation_id: "overflow".parse().unwrap(),
            ..first.clone()
        };
        assert_eq!(
            handle.leave_operation(overflow).unwrap().await.unwrap(),
            Err(LeaveError::HistoryFull)
        );
        assert_eq!(
            handle
                .leave_operation(first)
                .unwrap()
                .await
                .unwrap()
                .unwrap(),
            receipt
        );
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[test]
    fn departure_codec_keeps_old_identity_after_core_replacement() {
        use crate::peer::{codec, wire::Transport};
        let model = testing::model_with_members(2);
        let old_formation = model.formation().clone();
        let old_node = model.local_id().clone();
        let transition = orishu_membership::update(
            model,
            Message::Local(Command::Leave {
                replacement_formation: "replacement-formation".parse().unwrap(),
                replacement_node_id: "replacement-node".parse().unwrap(),
                replacement_cluster_name: "replacement-label".parse().unwrap(),
            }),
        );
        assert_ne!(transition.model.formation(), &old_formation);
        assert_ne!(transition.model.local_id(), &old_node);
        let mut departures = 0;
        for effect in transition.effects {
            if let Effect::Send { message, .. } = effect {
                let packet = encode_departure(message.clone(), &old_formation, &old_node).unwrap();
                assert_eq!(packet.transport, Transport::Datagram);
                let fields = codec::record_fields(&packet.bytes).unwrap();
                let field = |key| fields.iter().find(|(name, _)| *name == key).unwrap().1;
                assert_eq!(
                    codec::decode::<String>(field("formationId")).unwrap(),
                    old_formation.as_str()
                );
                assert_eq!(
                    codec::decode::<String>(field("senderId")).unwrap(),
                    old_node.as_str()
                );
                assert!(
                    encode_departure(
                        message,
                        transition.model.formation(),
                        transition.model.local_id()
                    )
                    .is_err()
                );
                departures += 1;
            }
        }
        assert_eq!(departures, 2);
        assert!(
            encode_departure(
                orishu_membership::OutboundMessage {
                    body: orishu_membership::OutboundBody::Ping {
                        probe: orishu_membership::ProbeId(1),
                        incarnation: orishu_membership::Incarnation(0),
                    },
                    seq: 1,
                    gossip: vec![],
                },
                &old_formation,
                &old_node
            )
            .is_err()
        );
    }

    fn lock_request(handle: &Handle, id: &str, locked: bool) -> LockRequest {
        LockRequest {
            schema_version: 1,
            operation_id: id.parse().unwrap(),
            formation_id: handle.view().unwrap().summary.formation_id,
            locked,
        }
    }

    async fn operate(handle: &Handle, request: LockRequest) -> Result<LockReceipt, LockError> {
        tokio::time::timeout(
            Duration::from_secs(2),
            handle.lock_operation(request).unwrap(),
        )
        .await
        .unwrap()
        .unwrap()
    }

    #[tokio::test]
    async fn dropped_join_preparation_records_failure_and_replays_without_new_work() {
        use orishu::model::cluster::{JoinMaterial, JoinOperationState, JoinRequest};
        let model = testing::standalone("source");
        let request = JoinRequest {
            schema_version: 1,
            operation_id: "cancelled-prepare".parse().unwrap(),
            formation_id: model.formation().clone(),
            material: JoinMaterial {
                schema_version: 1,
                formation_id: "target".parse().unwrap(),
                introducer_node_id: "introducer".parse().unwrap(),
                introducer_fingerprint: model.local().cert_fingerprint,
                peer_endpoints: vec!["127.0.0.1:6655".into()],
                introducer_ready: true,
                token: "a".repeat(64).try_into().unwrap(),
            },
        };
        let (handle, task) = spawn_standalone(model);
        let PreparedJoin::Start(job) = handle
            .prepare_join_with_parent(
                request.clone(),
                crate::trace_context::TraceParent::new([1; 16], [2; 8], true),
            )
            .unwrap()
            .await
            .unwrap()
            .unwrap()
        else {
            panic!("new operation");
        };
        assert!(
            job.parent.is_none(),
            "runtime-disabled owner retained context"
        );
        drop(job);
        let status = handle
            .join_status(request.operation_id.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.state, JoinOperationState::FailedBeforeAdmission);
        assert!(matches!(
            handle
                .prepare_join(request.clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap(),
            PreparedJoin::Replay(_)
        ));
        let mut cancelled_receiver = request;
        cancelled_receiver.operation_id = "receiver-gone".parse().unwrap();
        drop(handle.prepare_join(cancelled_receiver.clone()).unwrap());
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let status = handle
                    .join_status(cancelled_receiver.operation_id.clone())
                    .unwrap()
                    .await
                    .unwrap();
                if status.is_some_and(|s| s.state == JoinOperationState::FailedBeforeAdmission) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            handle.view().unwrap().summary.participation,
            Participation::Standalone
        );
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn stale_join_preparation_cancellation_preserves_new_operation() {
        use orishu::model::cluster::{JoinMaterial, JoinOperationState, JoinRequest};
        tokio::time::timeout(Duration::from_secs(3), async {
            let model = testing::standalone("source");
            let mut request = JoinRequest {
                schema_version: 1,
                operation_id: "old-prepare".parse().unwrap(),
                formation_id: model.formation().clone(),
                material: JoinMaterial {
                    schema_version: 1,
                    formation_id: "target".parse().unwrap(),
                    introducer_node_id: "introducer".parse().unwrap(),
                    introducer_fingerprint: model.local().cert_fingerprint,
                    peer_endpoints: vec!["127.0.0.1:6655".into()],
                    introducer_ready: true,
                    token: "a".repeat(64).try_into().unwrap(),
                },
            };
            let old_id = request.operation_id.clone();
            let (handle, task) = spawn_standalone(model);
            #[cfg(feature = "otlp-tracing")]
            let (_queue, _receiver) = {
                let pair = crate::trace_export::SpanQueue::new(1_000_000, 2, 2).unwrap();
                assert!(handle.install_trace_queue(pair.0.clone()));
                assert!(!handle.install_trace_queue(pair.0.clone()));
                pair
            };
            let old_parent = crate::trace_context::TraceParent::new([1; 16], [2; 8], true);
            let new_parent = crate::trace_context::TraceParent::new([3; 16], [4; 8], true);
            let PreparedJoin::Start(old) = handle
                .prepare_join_with_parent(request.clone(), old_parent)
                .unwrap()
                .await
                .unwrap()
                .unwrap()
            else {
                panic!("old preparation must start");
            };
            assert_eq!(
                old.parent,
                if cfg!(feature = "otlp-tracing") {
                    old_parent
                } else {
                    None
                }
            );
            assert!(matches!(
                handle
                    .prepare_join_with_parent(request.clone(), new_parent)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap(),
                PreparedJoin::Replay(_)
            ));
            assert_eq!(
                old.parent,
                if cfg!(feature = "otlp-tracing") {
                    old_parent
                } else {
                    None
                }
            );
            handle
                .try_submit(
                    Generation(0),
                    Message::Local(Command::Leave {
                        replacement_formation: "replacement".parse().unwrap(),
                        replacement_node_id: "replacement-node".parse().unwrap(),
                        replacement_cluster_name: "replacement-label".parse().unwrap(),
                    }),
                )
                .unwrap();
            let replacement = until(&handle, |view| view.generation == Generation(1)).await;
            request.operation_id = "new-prepare".parse().unwrap();
            request.formation_id = replacement.summary.formation_id.clone();
            let PreparedJoin::Start(new) = handle
                .prepare_join_with_parent(request.clone(), new_parent)
                .unwrap()
                .await
                .unwrap()
                .unwrap()
            else {
                panic!("new lifecycle preparation must start");
            };
            assert_eq!(
                new.parent,
                if cfg!(feature = "otlp-tracing") {
                    new_parent
                } else {
                    None
                }
            );
            // Uses the real reserved completion and its production Drop path.
            // The following status request is an ordered control-lane barrier.
            drop(old);
            assert_eq!(
                new.parent,
                if cfg!(feature = "otlp-tracing") {
                    new_parent
                } else {
                    None
                }
            );
            let status = handle
                .join_status(request.operation_id.clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.state, JoinOperationState::Connecting);
            let previous = handle.join_status(old_id).unwrap().await.unwrap().unwrap();
            assert_eq!(previous.state, JoinOperationState::FailedBeforeAdmission);
            let view = handle.view().unwrap();
            assert_eq!(view.generation, replacement.generation);
            assert_eq!(view.summary.formation_id, replacement.summary.formation_id);
            assert_eq!(
                view.summary.source_node_id,
                replacement.summary.source_node_id
            );
            drop(new);
            let status = handle
                .join_status(request.operation_id)
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.state, JoinOperationState::FailedBeforeAdmission);
            handle.shutdown().await.unwrap();
            assert_eq!(task.await.unwrap(), Ok(()));
        })
        .await
        .expect("stale preparation cancellation deadline");
    }

    #[tokio::test]
    async fn operation_replay_recovers_original_receipt_without_reapplying_old_intent() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        let lock = lock_request(&handle, "first", true);
        let first = operate(&handle, lock.clone()).await.unwrap();
        let unlocked = operate(&handle, lock_request(&handle, "second", false))
            .await
            .unwrap();
        assert_ne!(first.policy_version, unlocked.policy_version);
        assert_eq!(operate(&handle, lock.clone()).await.unwrap(), first);
        assert!(!handle.view().unwrap().summary.membership_locked);
        let conflict = LockRequest {
            locked: false,
            ..lock
        };
        assert_eq!(operate(&handle, conflict).await, Err(LockError::Conflict));
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn operation_history_never_evicts_into_reexecution_and_expires_with_formation() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        let first = lock_request(&handle, "first", false);
        let receipt = operate(&handle, first.clone()).await.unwrap();
        for index in 1..MAX_LOCK_OPERATIONS {
            operate(
                &handle,
                lock_request(&handle, &format!("op-{index}"), false),
            )
            .await
            .unwrap();
        }
        assert_eq!(
            operate(&handle, lock_request(&handle, "overflow", true)).await,
            Err(LockError::HistoryFull)
        );
        assert_eq!(operate(&handle, first.clone()).await.unwrap(), receipt);
        handle
            .try_submit(
                Generation(0),
                Message::Local(Command::Leave {
                    replacement_formation: FormationId::new("replacement").unwrap(),
                    replacement_node_id: NodeId::new("replacement-node").unwrap(),
                    replacement_cluster_name: ClusterName::new("replacement-label").unwrap(),
                }),
            )
            .unwrap();
        until(&handle, |view| view.generation == Generation(1)).await;
        assert_eq!(
            operate(&handle, first).await,
            Err(LockError::StaleFormation)
        );
        assert!(
            operate(&handle, lock_request(&handle, "first", true))
                .await
                .unwrap()
                .locked
        );
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    async fn until(handle: &Handle, predicate: impl Fn(&View) -> bool) -> View {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let view = handle.view().unwrap();
                if predicate(&view) {
                    return view;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("owner progress deadline")
    }

    #[tokio::test]
    async fn acknowledged_lock_reports_applied_state_and_rejects_stale_preconditions() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        let original = handle.view().unwrap();
        for locked in [true, true, false] {
            let result = handle
                .set_membership_lock(
                    original.generation,
                    original.summary.formation_id.clone(),
                    locked,
                )
                .unwrap();
            let accepted = tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(accepted.membership_locked, locked);
            assert_eq!(handle.view().unwrap().summary.membership_locked, locked);
        }
        for (generation, formation) in [
            (Generation(99), original.summary.formation_id.clone()),
            (
                original.generation,
                FormationId::new("wrong-formation").unwrap(),
            ),
        ] {
            let result = handle
                .set_membership_lock(generation, formation, true)
                .unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), result)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(LockError::StaleFormation)
            );
            assert!(!handle.view().unwrap().summary.membership_locked);
        }
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn queued_policy_command_is_fenced_after_preceding_leave() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        let original = handle.view().unwrap();
        handle
            .try_submit(
                original.generation,
                Message::Local(Command::Leave {
                    replacement_formation: FormationId::new("replacement").unwrap(),
                    replacement_node_id: NodeId::new("replacement-node").unwrap(),
                    replacement_cluster_name: ClusterName::new("replacement-label").unwrap(),
                }),
            )
            .unwrap();
        // Both commands are queued before yielding to the owner. The check must
        // happen at consumption, not against the HTTP-side snapshot above.
        let result = handle
            .set_membership_lock(original.generation, original.summary.formation_id, true)
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .unwrap()
                .unwrap(),
            Err(LockError::StaleFormation)
        );
        let current = handle.view().unwrap();
        assert_eq!(current.generation, Generation(1));
        assert!(!current.summary.membership_locked);
        let result = handle
            .set_membership_lock(current.generation, current.summary.formation_id, true)
            .unwrap();
        drop(result); // A disconnected requester cannot roll back accepted work.
        until(&handle, |view| view.summary.membership_locked).await;
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn acknowledged_commands_share_bounded_control_capacity() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        let view = handle.view().unwrap();
        let mut pending = Vec::new();
        for _ in 0..CONTROL_CAPACITY {
            pending.push(
                handle
                    .set_membership_lock(view.generation, view.summary.formation_id.clone(), true)
                    .unwrap(),
            );
        }
        assert!(matches!(
            handle.set_membership_lock(view.generation, view.summary.formation_id.clone(), false),
            Err(DriverError::Overloaded)
        ));
        for response in pending {
            assert!(
                tokio::time::timeout(Duration::from_secs(1), response)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap()
                    .membership_locked
            );
        }
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
        assert!(matches!(
            handle.set_membership_lock(view.generation, view.summary.formation_id, false),
            Err(DriverError::Closed)
        ));
    }

    #[tokio::test]
    async fn old_send_results_cannot_change_replacement_formation_or_counters() {
        tokio::time::timeout(Duration::from_secs(3), async {
            let (handle, task) = spawn_standalone(testing::model_with_members(0));
            until(&handle, |view| view.transitions >= 2).await;
            let old = handle.view().unwrap().generation;
            handle
                .try_submit(
                    old,
                    Message::Local(Command::Leave {
                        replacement_formation: "replacement".parse().unwrap(),
                        replacement_node_id: "replacement-node".parse().unwrap(),
                        replacement_cluster_name: "replacement-cluster".parse().unwrap(),
                    }),
                )
                .unwrap();
            let replacement = until(&handle, |view| view.generation != old).await;
            let session = orishu_membership::SessionId(123);
            // These represent IO results already completed before cancellation
            // won the lifecycle race. Feed the production consumption method;
            // do not claim this test executes the transport or its cancellation.
            for result in [
                (old, session, Ok(vec![0xff])),
                (
                    old,
                    session,
                    Err(crate::peer::exchange::ExchangeError::Owner),
                ),
            ] {
                let (processed, done) = oneshot::channel();
                handle
                    .control
                    .try_send(Control::ObserveSendCompletion { result, processed })
                    .unwrap();
                let current = done.await.unwrap();
                assert_eq!(current.generation, replacement.generation);
                assert_eq!(
                    current.summary.formation_id,
                    replacement.summary.formation_id
                );
                assert_eq!(
                    current.summary.source_node_id,
                    replacement.summary.source_node_id
                );
                assert_eq!(current.summary.participation, Participation::Standalone);
                assert_eq!(current.failed_sends, replacement.failed_sends);
                assert_eq!(current.completed_exchanges, replacement.completed_exchanges);
                assert_eq!(current.diagnostics, replacement.diagnostics);
            }
            // Positive control: a current-generation failed exchange remains
            // observable. Fencing must not become unconditional error loss.
            let (processed, done) = oneshot::channel();
            handle
                .control
                .try_send(Control::ObserveSendCompletion {
                    result: (
                        replacement.generation,
                        session,
                        Err(crate::peer::exchange::ExchangeError::Owner),
                    ),
                    processed,
                })
                .unwrap();
            let current = done.await.unwrap();
            assert_eq!(current.failed_sends, replacement.failed_sends + 1);
            handle.shutdown().await.unwrap();
            assert_eq!(task.await.unwrap(), Ok(()));
        })
        .await
        .expect("stale send-completion owner deadline");
    }

    #[tokio::test]
    async fn health_reads_cannot_mask_stalled_owner_shutdown() {
        use crate::health::OwnerHealth;
        tokio::time::timeout(Duration::from_secs(8), async {
            let (handle, task) = spawn_standalone(testing::model_with_members(0));
            assert_eq!(handle.health(Instant::now()), OwnerHealth::Starting);
            let initial = until(&handle, |view| view.owner_progress.is_some()).await;
            assert_eq!(handle.health(Instant::now()), OwnerHealth::Healthy);
            assert!(!initial.summary.introducer_ready);
            handle
                .try_submit(
                    initial.generation,
                    Message::Local(Command::SetMembershipLock(true)),
                )
                .unwrap();
            until(&handle, |view| view.summary.membership_locked).await;
            assert_eq!(handle.health(Instant::now()), OwnerHealth::Healthy);
            let (release, hold) = oneshot::channel();
            handle.hold_shutdown(hold).await;
            handle.shutdown().await.unwrap();
            assert_eq!(handle.health(Instant::now()), OwnerHealth::Stopping);
            let last = handle.view().unwrap().owner_progress;
            tokio::time::timeout(Duration::from_secs(6), async {
                loop {
                    let health = handle.health(Instant::now());
                    if health == OwnerHealth::Stalled {
                        break;
                    }
                    assert_eq!(health, OwnerHealth::Stopping);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("responsive readers must not hide stalled owner progress");
            assert_eq!(handle.view().unwrap().owner_progress, last);
            assert!(!handle.health(Instant::now()).formation_ready());
            release.send(()).unwrap();
            assert_eq!(task.await.unwrap(), Ok(()));
            assert_eq!(handle.health(Instant::now()), OwnerHealth::Closed);
        })
        .await
        .expect("owner supervision fixture deadline");
    }

    #[tokio::test]
    async fn leave_clears_real_owner_deadlines_and_fences_late_timer_input() {
        async fn timers(handle: &Handle) -> Vec<(orishu_membership::TimerToken, Instant)> {
            let (reply, receive) = oneshot::channel();
            handle
                .control
                .try_send(Control::TimerSnapshot(reply))
                .unwrap();
            receive.await.unwrap()
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            let (handle, task) =
                crate::formation_metrics::test_owner(testing::model_with_members(2));
            let old = handle.view().unwrap().generation;
            handle
                .try_submit(old, Message::Local(Command::StartProbeRound))
                .unwrap();
            let armed = timers(&handle).await;
            let (token, deadline) = armed
                .iter()
                .min_by_key(|(_, deadline)| *deadline)
                .copied()
                .expect("real probe round must arm an owner deadline");
            assert!(
                deadline > Instant::now(),
                "capture the pending timer before expiry"
            );
            let counters_before_leave = handle.counters();
            handle
                .try_submit(
                    old,
                    Message::Local(Command::Leave {
                        replacement_formation: "replacement".parse().unwrap(),
                        replacement_node_id: "replacement-node".parse().unwrap(),
                        replacement_cluster_name: "replacement-cluster".parse().unwrap(),
                    }),
                )
                .unwrap();
            // Same control lane: this snapshot observes the completed leave,
            // not a test-owned timer map or an injected cancellation result.
            assert!(timers(&handle).await.is_empty());
            let replacement = handle.view().unwrap();
            assert_ne!(replacement.generation, old);
            assert!(handle.counters().transitions >= counters_before_leave.transitions);
            handle.try_submit(old, Message::Timer(token)).unwrap();
            let current = until(&handle, |view| {
                view.stale_inputs == replacement.stale_inputs + 1
            })
            .await;
            assert_eq!(
                current.summary.formation_id,
                replacement.summary.formation_id
            );
            assert_eq!(
                current.summary.source_node_id,
                replacement.summary.source_node_id
            );
            assert_eq!(current.summary.member_count, 1);
            assert_eq!(current.summary.participation, Participation::Standalone);
            assert_eq!(current.diagnostics, replacement.diagnostics);
            assert_eq!(current.failed_sends, replacement.failed_sends);
            let counters = handle.counters();
            assert_eq!(counters.stale_inputs, current.stale_inputs);
            assert_eq!(counters.diagnostics, current.diagnostics);
            assert_eq!(counters.failed_sends, current.failed_sends);
            assert!(timers(&handle).await.is_empty());
            handle.shutdown().await.unwrap();
            assert_eq!(task.await.unwrap(), Ok(()));
            #[cfg(feature = "observability")]
            for event in crate::formation_metrics::Event::ALL {
                assert_eq!(
                    handle.formation_counters().unwrap().get(event),
                    0,
                    "leave cancellation and old-generation input must not become expiry"
                );
            }
            assert!(handle.counters().transitions >= counters.transitions);
            assert_eq!(handle.counters().stale_inputs, counters.stale_inputs);
        })
        .await
        .expect("pending-timer lifecycle deadline");
    }

    #[cfg(feature = "observability")]
    #[tokio::test]
    async fn owner_deadline_metrics_follow_real_timers_and_survive_leave() {
        use crate::formation_metrics::Event;
        use orishu_membership::{DurationMillis, Limits, LimitsSpec};
        tokio::time::timeout(Duration::from_secs(4), async {
            for peers in [1, 2] {
                let limits = Limits::try_from(LimitsSpec {
                    probe_timeout: DurationMillis(50),
                    indirect_probe_timeout: DurationMillis(50),
                    suspicion_timeout: DurationMillis(100),
                    anti_entropy_timeout: DurationMillis(100),
                    suspicion_multiplier: 1,
                    ..Default::default()
                })
                .unwrap();
                let (handle, task) =
                    crate::formation_metrics::test_owner(testing::model_with_limits(peers, limits));
                until(&handle, |view| view.owner_progress.is_some()).await;
                tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        let observed = handle.formation_counters().unwrap();
                        if observed.get(Event::SuspicionDeadline) > 0 {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("actual owner timers drive suspicion expiry");
                let before = handle.formation_counters().unwrap();
                for event in [
                    Event::DirectProbeDeadline,
                    Event::IndirectProbeDeadline,
                    Event::SuspicionDeadline,
                    Event::AntiEntropyDeadline,
                    Event::AntiEntropyAbandoned,
                ] {
                    // No eligible helper cancels indirect probing immediately;
                    // one other admitted peer leaves its real deadline armed.
                    let expected = match event {
                        Event::IndirectProbeDeadline => u64::from(peers == 2),
                        // These fixture members have no registered routes, so
                        // no pull is started. Real routed expiry is covered by
                        // the departed-reconciliation transport timing test.
                        Event::AntiEntropyDeadline | Event::AntiEntropyAbandoned => 0,
                        _ => 1,
                    };
                    assert_eq!(before.get(event), expected, "{event:?}, peers={peers}");
                }
                assert_eq!(before.get(Event::JoinRetryDue), 0);
                assert_eq!(before.get(Event::JoinAbandoned), 0);
                assert_eq!(before.get(Event::StaleTimerInput), 0);
                let model = handle.model_snapshot().await;
                assert_eq!(
                    model
                        .members()
                        .values()
                        .filter(|member| member.liveness == Liveness::Dead)
                        .count(),
                    1
                );
                let initial = handle.view().unwrap();
                handle
                    .try_submit(
                        initial.generation,
                        Message::Timer(TimerToken {
                            kind: orishu_membership::TimerKind::DirectProbe,
                            generation: u64::MAX,
                        }),
                    )
                    .unwrap();
                until(&handle, |view| view.diagnostics > initial.diagnostics).await;
                assert_eq!(
                    handle
                        .formation_counters()
                        .unwrap()
                        .get(Event::StaleTimerInput),
                    1
                );
                let before_leave = handle.formation_counters().unwrap();
                let receipt = handle
                    .leave_operation(LeaveRequest {
                        schema_version: 1,
                        operation_id: "deadline-leave".parse().unwrap(),
                        formation_id: initial.summary.formation_id,
                    })
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert!(receipt.changed);
                handle.shutdown().await.unwrap();
                assert_eq!(task.await.unwrap(), Ok(()));
                for event in Event::ALL {
                    assert_eq!(
                        handle.formation_counters().unwrap().get(event),
                        before_leave.get(event)
                    );
                }
            }
        })
        .await
        .expect("bounded owner deadline/leave/shutdown metrics fixture");
    }

    #[tokio::test]
    async fn real_owner_runs_periodic_work_and_fences_old_formation_inputs() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        until(&handle, |view| view.transitions >= 2).await;
        handle
            .try_submit(
                Generation(0),
                Message::Local(Command::Leave {
                    replacement_formation: FormationId::new("replacement").unwrap(),
                    replacement_node_id: NodeId::new("replacement-node").unwrap(),
                    replacement_cluster_name: ClusterName::new("replacement-label").unwrap(),
                }),
            )
            .unwrap();
        until(&handle, |view| view.generation == Generation(1)).await;
        handle
            .try_submit(
                Generation(0),
                Message::Local(Command::SetMembershipLock(true)),
            )
            .unwrap();
        let view = until(&handle, |view| view.stale_inputs == 1).await;
        assert!(!view.summary.membership_locked);
        handle
            .try_submit(
                Generation(1),
                Message::Local(Command::SetMembershipLock(true)),
            )
            .unwrap();
        until(&handle, |view| view.summary.membership_locked).await;
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
        assert!(matches!(handle.view(), Err(DriverError::Closed)));
    }

    #[tokio::test]
    async fn reserved_shutdown_is_not_blocked_by_a_full_data_mailbox() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        for _ in 0..CONTROL_CAPACITY {
            handle
                .try_submit(Generation(0), Message::Local(Command::StartProbeRound))
                .unwrap();
        }
        assert_eq!(
            handle.try_submit(Generation(0), Message::Local(Command::StartProbeRound)),
            Err(DriverError::Overloaded)
        );
        tokio::time::timeout(Duration::from_secs(1), handle.shutdown())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn reconciliation_does_not_wait_on_a_route_that_was_never_available() {
        let (handle, task) = spawn_standalone(testing::model_with_members(2));
        handle
            .try_submit(Generation(0), Message::Local(Command::StartProbeRound))
            .unwrap();
        handle
            .try_submit(
                Generation(0),
                Message::Local(Command::StartAntiEntropyRound),
            )
            .unwrap();
        let model = handle.model_snapshot().await;
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
        assert!(
            model.anti_entropy().is_none(),
            "no unsent reconciliation may occupy its deadline"
        );
        assert!(
            !model.probes().is_empty(),
            "disconnected members must still be failure-detected"
        );
    }

    #[tokio::test]
    async fn gossip_without_a_member_route_is_not_charged_as_transmitted() {
        let (handle, task) = spawn_standalone(testing::model_with_members(2));
        let initial = handle.view().unwrap();
        handle
            .set_membership_lock(initial.generation, initial.summary.formation_id, true)
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        for _ in 0..16 {
            handle
                .try_submit(Generation(0), Message::Local(Command::StartProbeRound))
                .unwrap();
        }
        let model = handle.model_snapshot().await;
        let view = handle.view().unwrap();
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
        assert!(model.membership_locked());
        assert!(view.failed_sends >= 16);
        assert_eq!(view.completed_exchanges, 0);
        assert!(
            model.gossip().iter().any(|(key, hops, _)| *key
                == orishu_membership::GossipKey::MembershipPolicy
                && hops == 0),
            "a route that never existed cannot consume the gossip allowance"
        );
    }

    #[tokio::test]
    async fn missing_member_routes_are_failed_sends_not_owner_failure_or_delivery() {
        let (handle, task) = spawn_standalone(testing::model_with_members(2));
        let view = until(&handle, |view| view.failed_sends >= 2).await;
        assert_eq!(view.completed_exchanges, 0);
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn peer_backlog_cannot_take_control_or_completion_capacity() {
        let model = testing::model_with_members(0);
        let peer = orishu_membership::PeerInput {
            context: testing::peer_context(&model, model.local_id(), 1),
            body: orishu_membership::PeerBody::Ping {
                probe: orishu_membership::ProbeId(1),
                incarnation: orishu_membership::Incarnation::INITIAL,
            },
        };
        let (handle, task) = spawn_standalone(model);
        for _ in 0..MAILBOX_CAPACITY {
            handle
                .try_submit(Generation(99), Message::Peer(peer.clone()))
                .unwrap();
        }
        assert_eq!(
            handle.try_submit(Generation(99), Message::Peer(peer)),
            Err(DriverError::Overloaded)
        );
        let pressure = handle.pressure();
        assert_eq!(
            pressure.peer,
            LanePressure {
                slots_in_use: MAILBOX_CAPACITY,
                capacity: MAILBOX_CAPACITY
            }
        );
        assert_eq!(
            pressure.control,
            LanePressure {
                slots_in_use: 0,
                capacity: CONTROL_CAPACITY
            }
        );
        assert_eq!(
            pressure.completion,
            LanePressure {
                slots_in_use: 0,
                capacity: MAILBOX_CAPACITY
            }
        );
        assert_eq!(
            pressure.shutdown,
            LanePressure {
                slots_in_use: 0,
                capacity: 1
            }
        );
        handle
            .try_submit(
                Generation(0),
                Message::Local(Command::SetMembershipLock(true)),
            )
            .unwrap();
        handle
            .try_submit(
                Generation(0),
                Message::Outcome(EffectOutcome::NodeIdUnavailable {
                    request: orishu_membership::AllocationId(99),
                    session: orishu_membership::SessionId(99),
                }),
            )
            .unwrap();
        until(&handle, |view| {
            view.summary.membership_locked
                && view.diagnostics > 0
                && view.stale_inputs == MAILBOX_CAPACITY as u64
        })
        .await;
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn reserved_verifications_complete_even_when_no_queue_capacity_remains() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        let mut pending = Vec::new();
        for index in 0..MAILBOX_CAPACITY {
            pending.push(
                handle
                    .reserve_verification(
                        Generation(0),
                        orishu_membership::VerificationId(index as u64),
                        orishu_membership::SessionId(index as u64),
                        true,
                    )
                    .unwrap(),
            );
        }
        assert_eq!(
            handle.pressure().completion,
            LanePressure {
                slots_in_use: MAILBOX_CAPACITY,
                capacity: MAILBOX_CAPACITY
            }
        );
        assert_eq!(handle.pressure().peer.slots_in_use, 0);
        assert!(matches!(
            handle.reserve_verification(
                Generation(0),
                orishu_membership::VerificationId(99),
                orishu_membership::SessionId(99),
                true
            ),
            Err(DriverError::Overloaded)
        ));
        // Both successful completion and cancellation consume their existing
        // reservation, with no best-effort try_send that could lose the outcome.
        pending
            .pop()
            .unwrap()
            .complete(orishu_membership::AdmissionEvidence {
                transport_authenticated: true,
                token_valid: true,
                source_network_blocked: false,
            })
            .unwrap();
        drop(pending);
        until(&handle, |view| view.diagnostics >= MAILBOX_CAPACITY as u64).await;
        handle.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn aborted_verifier_delivers_negative_evidence_with_original_correlation() {
        let (send, mut receive) = mpsc::channel(1);
        let completion = VerificationCompletion {
            permit: Some(send.try_reserve_owned().unwrap()),
            generation: Generation(7),
            request: orishu_membership::VerificationId(12),
            session: orishu_membership::SessionId(34),
            authenticated: true,
        };
        let task = tokio::spawn(async move {
            let retained = completion;
            std::future::pending::<()>().await;
            drop(retained);
        });
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let input = receive.recv().await.unwrap();
        assert_eq!(input.generation, Generation(7));
        assert_eq!(
            input.message,
            Message::Outcome(EffectOutcome::CredentialVerified {
                request: orishu_membership::VerificationId(12),
                session: orishu_membership::SessionId(34),
                evidence: orishu_membership::AdmissionEvidence {
                    transport_authenticated: true,
                    token_valid: false,
                    source_network_blocked: false
                },
            })
        );
        assert!(receive.recv().await.is_none());
    }
}
