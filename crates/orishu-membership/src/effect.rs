//! Effects: work the core asks the IO shell to perform, and the vocabulary it
//! uses to report what happened.
//!
//! An effect is a *request*, never evidence that anything succeeded. Nothing
//! in [`crate::update`] treats an emitted effect as having taken place; the
//! model changes only when the corresponding
//! [`crate::message::EffectOutcome`] comes back in. That is what allows a
//! recorded message sequence to replay to exactly the same state on a machine
//! where no send, timer, or ID generator ever ran.

use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, NodeId, ProtocolVersion, RemovalMode,
    VersionTuple, WorkerName,
};
use serde::{Deserialize, Serialize};

use crate::{
    antientropy::MerkleDigest,
    gossip::GossipDelta,
    limits::DurationMillis,
    message::SelectionPurpose,
    model::{
        AntiEntropyCursor, BlocklistKey, Endpoints, Liveness, Member, NetworkPattern,
        ValidationError,
    },
};

/// Opaque handle for an authenticated transport session.
///
/// Minted by the IO shell. The core uses it to reply to a peer that has no
/// formation identity yet — a join applicant — without ever inventing a
/// `NodeId` for it. Sending to an admitted member uses
/// [`Destination::Member`] instead, so admitted and pre-admission traffic
/// cannot be confused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub u64);

/// Correlation ID for one probe, carried on the wire and echoed in replies.
///
/// Target ID alone is not enough to correlate a probe: datagrams may be
/// duplicated or reordered, and several probes of the same target can overlap
/// after a retry. The ID makes "which probe is this reply for?" answerable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProbeId(pub u64);

/// Correlation ID for a bounded peer-selection request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SelectionId(pub u64);

/// Correlation ID for a node-ID generation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AllocationId(pub u64);

/// Correlation ID for a credential-verification request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VerificationId(pub u64);

/// What a timer is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimerKind {
    /// A direct `Ping` awaiting its `Ack`.
    DirectProbe,
    /// `PingReq`s awaiting any `PingReply`.
    IndirectProbe,
    /// A suspicion awaiting refutation.
    Suspicion,
    /// A `PullReq` awaiting its `PullReply`.
    AntiEntropy,
    /// A join attempt awaiting its `JoinReply`, or backing off before a retry.
    JoinRetry,
}

/// Identifies one armed timer.
///
/// The `generation` is what makes a late expiry harmless. Whenever the core
/// re-arms a timer for the same logical thing — a probe moving from direct to
/// indirect, a join retrying — it mints a new generation and records only that
/// one as actionable. An expiry carrying an older generation is a message from
/// a superseded past and is discarded with a diagnostic rather than allowed to
/// suspect a member that has already answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerToken {
    /// What the timer is waiting for.
    pub kind: TimerKind,
    /// Monotonic generation from the model's correlation counter.
    pub generation: u64,
}

/// A semantic peer message the shell should encode and send.
///
/// These are protocol payloads, not frames: framing, CBOR encoding, QUIC
/// stream-versus-datagram choice, and retry are the shell's business.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundBody {
    /// Admission request from this node to an introducer.
    ///
    /// Carries no join token. The shell holds the credential and attaches it
    /// while encoding, so the secret never enters the replayable model.
    JoinRequest {
        /// This node's label, the only pre-admission identifier it has.
        name: WorkerName,
        /// This node's certificate fingerprint.
        cert_fingerprint: CertFingerprint,
        /// Protocol version offered.
        protocol: ProtocolVersion,
        /// Addresses advertised.
        endpoints: Endpoints,
        /// Role flags advertised.
        accepts: crate::model::Accepts,
        /// Connection capacity advertised.
        capacity: crate::model::NodeCapacity,
        /// Capability advertised.
        capabilities: crate::model::Capabilities,
    },

    /// Admission decision from this node, acting as introducer.
    JoinAccepted {
        /// Formation the applicant is joining.
        formation: FormationId,
        /// The formation's operator label.
        cluster_name: ClusterName,
        /// ID the formation has assigned to the applicant.
        assigned: NodeId,
        /// Bootstrap membership snapshot, already bounded.
        snapshot: Vec<Member>,
    },

    /// Admission refusal, with the gate that failed.
    JoinRejected {
        /// Which gate rejected the applicant.
        reason: RejectReason,
    },

    /// Admission redirect to other candidate introducers.
    ///
    /// Untrusted hints: an applicant may try them, and must not treat them as
    /// authority about the formation.
    JoinRedirect {
        /// Candidate introducer addresses.
        candidates: Vec<crate::model::Address>,
    },

    /// Direct probe.
    Ping {
        /// Correlation ID the target must echo.
        probe: ProbeId,
        /// Sender's current incarnation.
        incarnation: Incarnation,
    },

    /// Direct probe response.
    Ack {
        /// Correlation ID copied from the `Ping`.
        probe: ProbeId,
        /// Responder's current incarnation.
        incarnation: Incarnation,
    },

    /// Indirect probe request to an intermediary.
    PingReq {
        /// Requester's correlation ID, echoed in the reply.
        probe: ProbeId,
        /// Member the intermediary should probe.
        target: NodeId,
    },

    /// Indirect probe result from an intermediary.
    PingReply {
        /// Requester's correlation ID, copied from the `PingReq`.
        probe: ProbeId,
        /// Member that was probed.
        target: NodeId,
        /// Outcome of the intermediary's own probe.
        result: IndirectResult,
    },

    /// Protocol announcement about a member's liveness.
    Announce {
        /// What is being announced.
        announcement: Announcement,
        /// Member the announcement is about.
        target: NodeId,
        /// Incarnation the announcement is bound to.
        incarnation: Incarnation,
    },

    /// Anti-entropy digest exchange request.
    PullRequest {
        /// Round identifier, echoed in the reply.
        round: u64,
        /// This node's digest of its membership state.
        digest: MerkleDigest,
        /// Buckets whose contents are being requested. Empty means "compare
        /// digests and tell me what differs".
        buckets: Vec<u16>,
        /// Where to resume, when continuing a truncated exchange.
        cursor: Option<AntiEntropyCursor>,
    },

    /// Anti-entropy state transfer.
    PullReply {
        /// Round identifier copied from the request.
        round: u64,
        /// This node's own digest.
        digest: MerkleDigest,
        /// Entries that differ, bounded by
        /// [`crate::Limits::max_anti_entropy_entries`].
        deltas: Vec<GossipDelta>,
        /// Whether every divergent entry fitted in this reply.
        complete: bool,
        /// Where the requester should resume when `complete` is `false`.
        cursor: Option<AntiEntropyCursor>,
    },
}

/// What an intermediary observed when probing on another member's behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndirectResult {
    /// The target acknowledged, at this incarnation.
    Ack(Incarnation),
    /// The intermediary does not know the target.
    NoSuchPeer,
    /// The intermediary's own probe timed out.
    Timeout,
}

/// A SWIM announcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Announcement {
    /// The subject is suspected of having failed.
    Suspect,
    /// The subject is alive at the stated incarnation.
    Alive,
    /// The subject is declared failed.
    Dead,
    /// The subject is leaving voluntarily. Authoritative only when the
    /// announcing node is the subject.
    Leave,
}

/// One outbound message, complete with the envelope fields the core owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundMessage {
    /// Protocol payload.
    pub body: OutboundBody,
    /// This node's next per-sender sequence number.
    pub seq: u64,
    /// Deltas piggybacked on this message, already hop-incremented.
    pub gossip: Vec<GossipDelta>,
}

/// Where an outbound message goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// An admitted member, addressed by its assigned ID. The shell resolves
    /// the ID to a connection.
    Member(NodeId),
    /// A pre-admission peer, addressed by its opaque session handle.
    Session(SessionId),
}

/// Which credential the shell is being asked to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialKind {
    /// The join token presented in a `JoinReq`, compared against the
    /// formation's current token, plus the transport and network facts the
    /// core cannot establish itself.
    JoinToken {
        /// Applicant's label, for audit.
        name: WorkerName,
        /// Applicant's certificate fingerprint, for pinning and blocklist
        /// matching.
        cert_fingerprint: CertFingerprint,
        /// Network patterns the shell should match the applicant's source
        /// address against.
        blocked_networks: Vec<NetworkPattern>,
    },
}

/// Work the IO shell should perform on the core's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Encode and send a peer message.
    Send {
        /// Where it goes.
        destination: Destination,
        /// What to send.
        message: OutboundMessage,
    },

    /// Arm a timer that will come back as
    /// [`crate::message::Message::Timer`].
    ArmTimer {
        /// Token identifying this arming. Only an expiry carrying this exact
        /// token is actionable.
        token: TimerToken,
        /// How long to wait.
        delay: DurationMillis,
    },

    /// Cancel a previously armed timer.
    ///
    /// Best effort. A cancellation that loses a race with the expiry is
    /// harmless, because the expiry's token is checked against the model
    /// anyway.
    CancelTimer {
        /// Token to cancel.
        token: TimerToken,
    },

    /// Choose up to `count` peers at random from the alive membership.
    ///
    /// Selection lives in the shell because it needs entropy. The core states
    /// the constraints and consumes the answer as
    /// [`crate::message::EffectOutcome::PeersSelected`].
    SelectPeers {
        /// Correlation ID for the answer.
        request: SelectionId,
        /// What the peers are for.
        purpose: SelectionPurpose,
        /// How many to choose.
        count: usize,
        /// Members that must not be chosen.
        exclude: Vec<NodeId>,
    },

    /// Generate a fresh, unique node ID for an applicant being admitted.
    ///
    /// The core must not do this itself: a deterministically generated ID
    /// would collide across nodes, and a randomly generated one would make
    /// replay non-deterministic.
    AllocateNodeId {
        /// Correlation ID for the answer.
        request: AllocationId,
        /// Session the applicant is on, so the answer rejoins the right
        /// pending decision.
        session: SessionId,
    },

    /// Verify a credential the core deliberately does not hold.
    VerifyCredential {
        /// Correlation ID for the answer.
        request: VerificationId,
        /// Session the applicant is on.
        session: SessionId,
        /// What to verify.
        kind: CredentialKind,
    },

    /// Publish a membership or admission change for audit, metrics, or an
    /// owning adapter.
    ///
    /// The core does not know or care whether anyone is listening; emitting
    /// the record is not performing the IO.
    Publish(ChangeRecord),
}

/// A membership or admission change worth recording outside the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeRecord {
    /// Complete public baseline merge outcome. One bounded publication replaces
    /// per-record events; normal reconciliation disseminates the merged state.
    AdmissionBaselineApplied {
        /// Shell-provided snapshot correlation.
        snapshot: u64,
        /// The merged state removes the local assigned identity. The shell must
        /// stop participating and must never install credentials or readiness.
        self_removed: bool,
    },
    /// No baseline changes were committed; diagnostics carry the domain reason.
    AdmissionBaselineRejected {
        /// Shell-provided snapshot correlation.
        snapshot: u64,
    },
    /// A new version of the formation lock was adopted.
    MembershipPolicyChanged {
        /// Locally observed lock state after adoption.
        locked: bool,
    },
    /// A node was admitted and inserted into membership.
    MemberAdmitted {
        /// Assigned ID.
        node: NodeId,
        /// Applicant's label.
        name: WorkerName,
    },
    /// A member's liveness changed.
    LivenessChanged {
        /// Member affected.
        node: NodeId,
        /// Previous liveness.
        from: Liveness,
        /// New liveness.
        to: Liveness,
        /// Incarnation the new state is bound to.
        incarnation: Incarnation,
    },
    /// A member was removed and tombstoned.
    MemberRemoved {
        /// Member removed.
        node: NodeId,
        /// How it was removed.
        mode: RemovalMode,
    },
    /// A member left voluntarily. No tombstone is written.
    MemberLeft {
        /// Member that left.
        node: NodeId,
    },
    /// An admission attempt was refused.
    AdmissionRejected {
        /// Applicant's label. Not identity.
        name: WorkerName,
        /// Gate that refused it.
        reason: RejectReason,
    },
    /// This node adopted another formation.
    FormationAdopted {
        /// Formation abandoned.
        from: FormationId,
        /// Formation adopted.
        to: FormationId,
        /// ID assigned by the new formation.
        assigned: NodeId,
    },
    /// This node left its formation voluntarily.
    FormationLeft {
        /// Formation left.
        formation: FormationId,
        /// ID held there, now dropped.
        previous: NodeId,
    },
}

/// Which admission gate refused an applicant.
///
/// One variant per gate in the runtime design, so a rejection says what to fix
/// rather than "denied". These are also the reasons carried on the wire, so
/// they hold no secrets and no internal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RejectReason {
    /// The transport was not authenticated.
    Unauthenticated,
    /// The applicant addressed a different formation.
    FormationMismatch,
    /// This node does not act as an introducer (`accepts.peers` is false).
    NotAnIntroducer,
    /// Membership is locked by an operator.
    MembershipLocked,
    /// The formation is at capacity.
    CapacityExhausted,
    /// The applicant's identity, label, or fingerprint is blocklisted.
    Blocklisted {
        /// Which blocklist key matched.
        key: BlocklistKey,
    },
    /// The applicant's source address matched a blocklisted network.
    BlocklistedNetwork,
    /// The applicant was removed from this formation and not reinstated.
    Tombstoned,
    /// The join token was missing, expired, or wrong.
    InvalidToken,
    /// The applicant's protocol version is outside this node's range.
    IncompatibleProtocol {
        /// Version offered.
        offered: ProtocolVersion,
    },
    /// The applicant's advertised description failed validation.
    MalformedRequest {
        /// Field that failed.
        field: String,
    },
    /// ID generation produced an ID already in use, so nothing was inserted.
    IdentityCollision,
    /// This certificate already owns an alive or suspected membership identity
    /// in the introducer's current view. Recover that admission; do not allocate
    /// a second live identity. Dead history alone does not forbid readmission.
    AlreadyAdmitted,
    /// Too many admission decisions are already in flight.
    Overloaded,
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthenticated => f.write_str("transport is not authenticated"),
            Self::FormationMismatch => f.write_str("request addressed another formation"),
            Self::NotAnIntroducer => f.write_str("node does not accept peer joins"),
            Self::MembershipLocked => f.write_str("membership is locked"),
            Self::CapacityExhausted => f.write_str("formation is at capacity"),
            Self::Blocklisted { key } => write!(f, "blocklisted by {key}"),
            Self::BlocklistedNetwork => f.write_str("source network is blocklisted"),
            Self::Tombstoned => f.write_str("identity was removed from this formation"),
            Self::InvalidToken => f.write_str("join token is not valid"),
            Self::IncompatibleProtocol { offered } => {
                write!(f, "protocol version {offered} is not supported")
            }
            Self::MalformedRequest { field } => write!(f, "malformed request field `{field}`"),
            Self::IdentityCollision => f.write_str("generated node ID already exists"),
            Self::AlreadyAdmitted => f.write_str("certificate already has a live admission"),
            Self::Overloaded => f.write_str("too many admissions in flight"),
        }
    }
}

/// A structured account of input the core refused to act on.
///
/// Diagnostics are how "we ignored that" stays observable. Every hostile-input
/// path produces one rather than panicking, logging, or silently dropping, so
/// a test can assert on exactly why a message had no effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// A peer message arrived on a transport that did not authenticate.
    ///
    /// Reported rather than trusted: the core checks the fact even though a
    /// correct QUIC adapter cannot deliver an unauthenticated message.
    Unauthenticated,
    /// A message named a formation other than this node's.
    FormationMismatch {
        /// Formation this node belongs to.
        expected: FormationId,
        /// Formation the message named.
        received: FormationId,
    },
    /// A post-admission message came from a node that is not a member.
    UnknownSender {
        /// Identity the sender claimed.
        claimed: NodeId,
    },
    /// A sender's certificate did not match the fingerprint pinned for its ID.
    CertificateMismatch {
        /// Member whose fingerprint was expected.
        node: NodeId,
    },
    /// A stream message repeated a sequence number already processed.
    ReplayedSequence {
        /// Sender.
        node: NodeId,
        /// Sequence number repeated.
        seq: u64,
    },
    /// A timer expiry carried a superseded token.
    StaleTimer {
        /// Token that expired.
        token: TimerToken,
    },
    /// A probe reply referenced a probe that is not in flight.
    UnknownProbe {
        /// Probe referenced.
        probe: ProbeId,
    },
    /// A probe reply came from the wrong node, or for the wrong target.
    ProbeMismatch {
        /// Probe referenced.
        probe: ProbeId,
        /// Why the reply did not fit.
        reason: &'static str,
    },
    /// An effect outcome referenced a request that is no longer pending.
    UnknownCorrelation {
        /// Which kind of request.
        kind: &'static str,
    },
    /// A `Leave` claimed to be about a node other than its sender.
    SpoofedLeave {
        /// Node that sent it.
        sender: NodeId,
        /// Node it claimed to be about.
        subject: NodeId,
    },
    /// An announcement about this node was ignored because only this node may
    /// speak for itself.
    SelfAnnouncementIgnored {
        /// What was announced.
        announcement: Announcement,
    },
    /// A liveness update was refused because the subject is tombstoned.
    TombstoneFenced {
        /// Node the update was about.
        node: NodeId,
    },
    /// Two payloads claimed the same version. Local state is kept.
    VersionConflict {
        /// Entity affected.
        entity: String,
        /// The disputed version.
        version: VersionTuple,
    },
    /// An update was older than what is already held.
    StaleDelta {
        /// Entity affected.
        entity: String,
    },
    /// A record failed bounds or consistency validation.
    MalformedRecord {
        /// Entity affected.
        entity: String,
        /// Why it was refused.
        error: ValidationError,
    },
    /// A configured limit would have been exceeded.
    LimitExceeded {
        /// Which limit.
        limit: &'static str,
        /// Value that would have been reached.
        value: usize,
        /// Configured maximum.
        max: usize,
    },
    /// This node cannot refute suspicion because its incarnation is exhausted.
    IncarnationExhausted,
    /// A join attempt was refused by an introducer.
    JoinRejected {
        /// Reason the introducer gave. Untrusted.
        reason: RejectReason,
    },
    /// A `JoinReply` failed validation before adoption.
    JoinReplyInvalid {
        /// Why it was refused.
        reason: &'static str,
    },
    /// A join attempt exhausted its retries.
    JoinAbandoned {
        /// Attempts made.
        attempts: u32,
    },
    /// An anti-entropy round was abandoned after too many exchanges.
    AntiEntropyAbandoned {
        /// Peer involved.
        peer: NodeId,
        /// Exchanges used.
        exchanges: u32,
    },
    /// A message arrived that this node has no state for.
    Unexpected {
        /// What arrived.
        what: &'static str,
    },
    /// One update produced more effects than the budget allows; the excess was
    /// dropped.
    EffectBudgetExceeded {
        /// Effects produced.
        produced: usize,
        /// Configured cap.
        cap: usize,
    },
    /// One update produced more diagnostics than the budget allows.
    DiagnosticBudgetExceeded {
        /// Configured cap.
        cap: usize,
    },
}

/// The result of folding one message into the model.
///
/// Returned by value, so a caller may keep the previous model, diff the two,
/// or discard the transition entirely — nothing has happened to the outside
/// world yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// The next model.
    pub model: crate::model::Membership,
    /// Work for the IO shell, in the order it should be performed.
    pub effects: Vec<Effect>,
    /// Structured reasons for anything the core refused to act on.
    pub diagnostics: Vec<Diagnostic>,
    /// Gossip that is not membership-owned, handed back untouched.
    ///
    /// The explicit boundary the task requires: the core neither decodes nor
    /// stores `WorkloadUpdate`, checkpoint, result, or audit deltas. It
    /// carries them across and lets their owner decide, which is why
    /// [`crate::gossip::ForeignDelta`] holds opaque bytes rather than a parsed
    /// payload.
    pub foreign_gossip: Vec<ForeignHandoff>,
}

/// One non-membership delta handed to its owning subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignHandoff {
    /// Member that carried it, when it arrived over an admitted connection.
    pub from: Option<NodeId>,
    /// The delta, unparsed.
    pub delta: crate::gossip::ForeignDelta,
}
