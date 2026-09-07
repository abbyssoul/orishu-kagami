//! Messages: every fact that can change the model.
//!
//! One closed enum, four sources — an authenticated peer, a local operator
//! command, a timer, and the outcome of an effect the core previously
//! requested. From the model's point of view they are uniform: each is a value
//! describing what happened, and none of them can smuggle in a socket, a
//! clock, or a random number.

use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, NodeId, ProtocolVersion, RemovalMode,
    WorkerName,
};

use crate::{
    antientropy::MerkleDigest,
    effect::{
        AllocationId, Announcement, IndirectResult, ProbeId, RejectReason, SelectionId, SessionId,
        TimerToken, VerificationId,
    },
    gossip::GossipDelta,
    model::{
        Accepts, Address, AdmissionPolicy, AntiEntropyCursor, BlocklistEntry, Capabilities,
        Endpoints, Member, NodeCapacity,
    },
};

/// A fact for the core to fold into the model.
///
/// `Peer` is much larger than the timer and outcome variants, because it
/// carries a decoded payload. Boxing it would add a heap allocation to every
/// inbound message on the hot path to save a stack move on the rare ones, and
/// would make the common construction `Message::Peer(PeerInput { .. })` read
/// worse at every call site. The size difference is deliberate.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Something arrived over an authenticated peer connection.
    Peer(PeerInput),
    /// The local operator or supervising process asked for something.
    Local(Command),
    /// A timer the core armed has expired.
    Timer(TimerToken),
    /// An effect the core requested has produced a result.
    Outcome(EffectOutcome),
}

/// Who a peer message claims to be from.
///
/// Two variants, not one nullable ID: pre-admission traffic legitimately has
/// no formation identity, and giving it a placeholder `NodeId` would let a
/// label reach code that expects an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SenderIdentity {
    /// An admitted member, identified by its assigned ID.
    Admitted(NodeId),
    /// A node that has not been admitted, identified only by its label and
    /// certificate.
    Applicant(WorkerName),
}

/// Authenticated facts about an inbound peer message, established by the shell.
///
/// The core re-checks the formation and sender binding on every
/// post-admission input rather than assuming the decoder did. A decoder
/// *should* reject a mismatch early — and `docs/protocol-p2p.md` requires it —
/// but correctness here must not depend on every future adapter remembering an
/// undocumented precondition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerContext {
    /// Opaque handle for the transport session this arrived on.
    pub session: SessionId,
    /// Whether the mTLS handshake completed successfully.
    ///
    /// Always `true` from a correct QUIC adapter; carried explicitly so that
    /// the authentication gate is a value the core checks rather than an
    /// assumption it makes.
    pub authenticated: bool,
    /// SHA-256 fingerprint of the certificate presented on this session.
    pub cert_fingerprint: CertFingerprint,
    /// Identity claimed in the envelope's `senderId`.
    pub sender: SenderIdentity,
    /// Formation claimed in the envelope's `formationId`.
    pub formation: FormationId,
    /// Protocol version claimed in the envelope's `proto`.
    pub protocol: ProtocolVersion,
    /// Per-sender sequence number from the envelope.
    pub seq: u64,
    /// Deltas piggybacked on the envelope.
    ///
    /// Every carrier delivers them here, so merging has exactly one entry
    /// point regardless of whether they rode on a probe, a probe reply, or a
    /// future workload message.
    pub gossip: Vec<GossipDelta>,
}

/// One inbound peer message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInput {
    /// Authenticated context.
    pub context: PeerContext,
    /// Protocol payload.
    pub body: PeerBody,
}

/// An inbound protocol payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerBody {
    /// An applicant asks to join this formation.
    ///
    /// The join token is absent by design: the shell holds it, compares it,
    /// and reports the verdict as [`AdmissionEvidence`]. A token in this
    /// struct would end up in every replay log and test fixture.
    JoinRequest {
        /// Addresses the applicant advertises.
        endpoints: Endpoints,
        /// Role flags the applicant advertises.
        accepts: Accepts,
        /// Connection capacity the applicant advertises.
        capacity: NodeCapacity,
        /// Capability the applicant advertises.
        capabilities: Capabilities,
    },

    /// An introducer answers this node's join attempt.
    JoinReply(JoinReply),

    /// A direct probe.
    Ping {
        /// Sender's correlation ID, to be echoed.
        probe: ProbeId,
        /// Sender's incarnation.
        incarnation: Incarnation,
    },

    /// A direct probe response.
    Ack {
        /// Correlation ID echoed from our `Ping`.
        probe: ProbeId,
        /// Responder's incarnation.
        incarnation: Incarnation,
    },

    /// A request to probe a third node on the sender's behalf.
    PingReq {
        /// Requester's correlation ID, to be echoed.
        probe: ProbeId,
        /// Node to probe.
        target: NodeId,
    },

    /// The result of an indirect probe we requested.
    PingReply {
        /// Correlation ID echoed from our `PingReq`.
        probe: ProbeId,
        /// Node that was probed.
        target: NodeId,
        /// What the intermediary observed.
        result: IndirectResult,
    },

    /// A SWIM announcement.
    Announce {
        /// What is announced.
        announcement: Announcement,
        /// Node it is about.
        target: NodeId,
        /// Incarnation it is bound to.
        incarnation: Incarnation,
    },

    /// A peer asks to reconcile state.
    PullRequest {
        /// Round identifier to echo.
        round: u64,
        /// The peer's digest.
        digest: MerkleDigest,
        /// Buckets the peer wants; empty means "you decide from the digests".
        buckets: Vec<u16>,
        /// Where to resume a truncated exchange.
        cursor: Option<AntiEntropyCursor>,
    },

    /// A peer answers our reconciliation request.
    PullReply {
        /// Round identifier echoed from our request.
        round: u64,
        /// The peer's digest.
        digest: MerkleDigest,
        /// Divergent entries.
        deltas: Vec<GossipDelta>,
        /// Whether every divergent entry fitted.
        complete: bool,
        /// Where to resume when it did not.
        cursor: Option<AntiEntropyCursor>,
    },
}

/// An introducer's answer to a join attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinReply {
    /// Admitted. Everything here is hostile input until validated.
    Accepted {
        /// Formation the applicant has joined.
        formation: FormationId,
        /// The formation's operator label.
        cluster_name: ClusterName,
        /// ID the formation assigned.
        assigned: NodeId,
        /// Bootstrap membership snapshot.
        snapshot: Vec<Member>,
        /// Encoded size of the snapshot in bytes, as measured by the decoder.
        ///
        /// The core cannot re-measure what it never saw encoded, so the
        /// decoder reports it and the core enforces
        /// [`crate::Limits::max_snapshot_bytes`] against the reported value.
        /// The decoder must also refuse to *allocate* beyond that bound; this
        /// check catches a decoder that did not.
        snapshot_bytes: usize,
    },
    /// Refused.
    Rejected {
        /// The introducer's stated reason. Untrusted.
        reason: RejectReason,
    },
    /// Redirected to other candidate introducers.
    Redirect {
        /// Untrusted candidate addresses.
        candidates: Vec<Address>,
    },
}

/// Something the local operator or supervising process asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Start joining another formation over an already-established session.
    ///
    /// Adopting the target formation abandons this node's current
    /// formation-scoped state entirely.
    BeginJoin {
        /// Session to the prospective introducer.
        session: SessionId,
        /// Formation being joined, as learned from the handshake.
        target_formation: FormationId,
    },

    /// Rebind one pending join after the shell authenticates a replacement
    /// connection to the same introducer/formation. This consumes the next
    /// retry, preserving the attempt budget and fencing the old timer/replies.
    /// It cannot restart an abandoned attempt or change its target formation.
    RebindJoin {
        /// The session still held by the pending attempt, not a stale job's guess.
        previous_session: SessionId,
        /// Fresh authenticated session; the shell must not reuse a retired handle.
        session: SessionId,
        /// Expected target, independently checked by the replacement handshake.
        target_formation: FormationId,
    },

    /// Leave the current formation voluntarily.
    ///
    /// Announces `Leave`, drops the assigned ID, and returns to a standalone
    /// formation of one. It writes no tombstone: leaving is this node's own
    /// decision, not the formation barring it.
    Leave {
        /// ID for the standalone formation this node falls back to.
        replacement_formation: FormationId,
        /// ID this node will hold in that formation.
        replacement_node_id: NodeId,
        /// Label for the standalone formation.
        replacement_cluster_name: ClusterName,
    },

    /// Remove a member by operator action, writing a tombstone.
    RemoveMember {
        /// Member to remove.
        node: NodeId,
        /// How it is being removed.
        mode: RemovalMode,
        /// Optional operator-supplied reason.
        reason: Option<String>,
    },

    /// Clear a membership tombstone, permitting readmission.
    ///
    /// The only way a tombstone is ever lifted.
    ClearTombstone {
        /// Node whose tombstone is cleared.
        node: NodeId,
    },

    /// Change the replicated membership lock using a new local version.
    SetMembershipLock(bool),

    /// Replace node-local admission configuration; never changes the cluster lock.
    SetPolicy(AdmissionPolicy),

    /// Atomically merge a complete, authenticated admission-state baseline.
    /// The shell proves completeness and fences lifecycle before submitting;
    /// acceptance changes no local admission role or credential authority.
    InstallAdmissionBaseline(crate::baseline::AdmissionBaseline),

    /// Add or lift a blocklist entry.
    UpdateBlocklist(BlocklistEntry),

    /// Begin one SWIM probe round.
    ///
    /// The shell drives the period; the core owns what a round does.
    StartProbeRound,

    /// Begin one anti-entropy round.
    StartAntiEntropyRound,
}

/// Why the core asked for peers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionPurpose {
    /// One peer to probe directly.
    DirectProbe,
    /// Intermediaries for an indirect probe.
    IndirectProbe {
        /// Probe the intermediaries will serve.
        probe: ProbeId,
    },
    /// One peer to reconcile with.
    AntiEntropy,
    /// A bounded subset to receive an urgent announcement.
    ///
    /// Announcing to *every* member would make one message's fan-out
    /// proportional to formation size. A bounded subset plus the gossip queue
    /// reaches everyone in `O(log n)` rounds without that spike.
    Announce {
        /// What to announce.
        announcement: Announcement,
        /// Node it is about.
        target: NodeId,
        /// Incarnation it is bound to.
        incarnation: Incarnation,
    },
}

/// Facts about an applicant that only the shell can establish.
///
/// Cryptographic token comparison, transport authentication, and CIDR matching
/// all need capabilities the core deliberately lacks. It states what it needs
/// verified, and consumes the verdict — the atomic accept/reject decision
/// still belongs to the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionEvidence {
    /// Whether the mTLS handshake authenticated the applicant.
    pub transport_authenticated: bool,
    /// Whether the presented join token matched the formation's current one.
    pub token_valid: bool,
    /// Whether the applicant's source address matched a blocklisted network.
    pub source_network_blocked: bool,
}

/// The result of an effect the core previously requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectOutcome {
    /// Peers chosen for a [`Effect::SelectPeers`](crate::Effect::SelectPeers)
    /// request.
    PeersSelected {
        /// Correlation ID from the request.
        request: SelectionId,
        /// Chosen peers. May be shorter than requested, or empty.
        peers: Vec<NodeId>,
    },

    /// A fresh node ID was generated for an applicant.
    NodeIdAllocated {
        /// Correlation ID from the request.
        request: AllocationId,
        /// Session the applicant is on.
        session: SessionId,
        /// The generated ID. Still checked for collision before use.
        node_id: NodeId,
    },

    /// No node ID could be generated.
    NodeIdUnavailable {
        /// Correlation ID from the request.
        request: AllocationId,
        /// Session the applicant is on.
        session: SessionId,
    },

    /// A credential verification completed.
    CredentialVerified {
        /// Correlation ID from the request.
        request: VerificationId,
        /// Session the applicant is on.
        session: SessionId,
        /// What the shell established.
        evidence: AdmissionEvidence,
    },
}
