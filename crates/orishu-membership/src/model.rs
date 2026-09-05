//! The membership model: what one worker believes about its formation.
//!
//! Everything here is a value. There are no handles, sockets, clocks, or task
//! joins, so a model can be cloned, serialized by an adapter, diffed in a test,
//! or replayed from a message log.
//!
//! Maps are `BTreeMap`/`BTreeSet` rather than hash containers on purpose:
//! iteration order feeds gossip selection, anti-entropy digests, and effect
//! ordering, all of which must be identical on two nodes given identical state.
//! A `HashMap`'s randomized iteration order would make `update` reproducible
//! only within a single process run.

use std::collections::{BTreeMap, BTreeSet};

use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, MembershipTombstone, NodeId,
    ProtocolRange, ProtocolVersion, VersionTuple, WorkerName,
};
use serde::{Deserialize, Serialize};

use crate::{
    effect::{ProbeId, SelectionId, SessionId, TimerToken},
    gossip::GossipQueue,
    limits::{DurationMillis, Limits},
};

/// Why a value decoded from the wire cannot be adopted into the model.
///
/// Validation failures are structured rather than panics: every one of these
/// is reachable from a hostile or merely buggy peer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// A text or byte field exceeded its configured maximum length.
    #[error("field `{field}` is {len} bytes, exceeding the {max}-byte limit")]
    TooLong {
        /// Name of the offending field.
        field: &'static str,
        /// Length supplied.
        len: usize,
        /// Configured maximum.
        max: usize,
    },

    /// A collection exceeded its configured maximum item count.
    #[error("field `{field}` has {count} items, exceeding the limit of {max}")]
    TooMany {
        /// Name of the offending field.
        field: &'static str,
        /// Item count supplied.
        count: usize,
        /// Configured maximum.
        max: usize,
    },

    /// A field that must carry a value was empty.
    #[error("field `{field}` must not be empty")]
    Empty {
        /// Name of the offending field.
        field: &'static str,
    },

    /// A collection contained the same identity twice.
    #[error("field `{field}` contains duplicate entry `{key}`")]
    Duplicate {
        /// Name of the offending field.
        field: &'static str,
        /// The repeated key.
        key: String,
    },

    /// A record disagreed with itself or with the local node's own identity.
    #[error("field `{field}` is inconsistent: {reason}")]
    Inconsistent {
        /// Name of the offending field.
        field: &'static str,
        /// What made it inconsistent.
        reason: &'static str,
    },
}

/// Checks that free-form text fits its configured bound.
fn check_text(field: &'static str, value: &str, max: usize) -> Result<(), ValidationError> {
    if value.len() > max {
        return Err(ValidationError::TooLong {
            field,
            len: value.len(),
            max,
        });
    }
    Ok(())
}

/// Checks that a collection fits its configured bound.
fn check_count(field: &'static str, count: usize, max: usize) -> Result<(), ValidationError> {
    if count > max {
        return Err(ValidationError::TooMany { field, count, max });
    }
    Ok(())
}

/// An advertised network address, kept opaque.
///
/// The core neither parses nor resolves addresses; it stores what a peer
/// advertised so the IO shell can dial it. Bounding the length is the core's
/// whole responsibility here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(pub String);

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A blocklist network pattern (an IP or CIDR range), kept opaque.
///
/// Matching an address against a pattern needs address parsing, which is the
/// shell's job; the core replicates the pattern and consumes the shell's
/// verified match result. See [`crate::message::AdmissionEvidence`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NetworkPattern(pub String);

impl std::fmt::Display for NetworkPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Addresses a node advertises for inbound connections.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    /// Addresses accepting peer connections.
    pub peers: Vec<Address>,
    /// Addresses accepting client connections. Empty when the node does not
    /// accept clients.
    pub clients: Vec<Address>,
}

impl Endpoints {
    fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        check_count(
            "endpoints.peers",
            self.peers.len(),
            limits.max_addresses_per_node(),
        )?;
        check_count(
            "endpoints.clients",
            self.clients.len(),
            limits.max_addresses_per_node(),
        )?;
        for address in self.peers.iter().chain(&self.clients) {
            check_text("endpoints.address", &address.0, limits.max_address_len())?;
        }
        Ok(())
    }
}

/// Operational role flags a node advertises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accepts {
    /// Accepts direct connections from operator and user clients.
    pub clients: bool,
    /// Accepts join requests from new peers, that is, acts as an introducer.
    pub peers: bool,
    /// Accepts new workload partitions. `false` means the node is cordoned.
    pub work: bool,
}

impl Default for Accepts {
    fn default() -> Self {
        Self {
            clients: false,
            peers: true,
            work: true,
        }
    }
}

/// Connection limits a node advertises for itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCapacity {
    /// Maximum peer connections the node will accept.
    pub peers: u32,
    /// Maximum client connections the node will accept.
    pub clients: u32,
}

/// A workload runtime engine a node can execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapability {
    /// Engine identifier, for example `wasm-component`.
    pub engine: String,
    /// Opaque runtime-lifecycle compatibility identifier.
    pub runtime_lifecycle: String,
}

/// Hardware and runtime capability a node advertises.
///
/// Descriptive only: membership never interprets these values, it replicates
/// them so that workload placement can. They are still hostile input, so every
/// string and collection is bounded before adoption.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// CPU cores available to the node.
    pub cpu_cores: u32,
    /// Memory available to the node, in bytes.
    pub memory_bytes: u64,
    /// Machine architecture, for example `x86_64`.
    pub architecture: String,
    /// Storage backend identifier.
    pub storage_backend: String,
    /// Replication factor the node's storage provides.
    pub storage_replicas: Option<u32>,
    /// Accelerator descriptions.
    pub accelerators: Vec<String>,
    /// Runtime engines the node implements completely.
    pub engines: Vec<EngineCapability>,
}

impl Capabilities {
    fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        check_text(
            "capabilities.architecture",
            &self.architecture,
            limits.max_text_len(),
        )?;
        check_text(
            "capabilities.storageBackend",
            &self.storage_backend,
            limits.max_text_len(),
        )?;
        check_count(
            "capabilities.accelerators",
            self.accelerators.len(),
            limits.max_accelerators(),
        )?;
        for accelerator in &self.accelerators {
            check_text(
                "capabilities.accelerator",
                accelerator,
                limits.max_text_len(),
            )?;
        }
        check_count(
            "capabilities.engines",
            self.engines.len(),
            limits.max_engines(),
        )?;
        for engine in &self.engines {
            check_text("capabilities.engine", &engine.engine, limits.max_text_len())?;
            check_text(
                "capabilities.runtimeLifecycle",
                &engine.runtime_lifecycle,
                limits.max_text_len(),
            )?;
        }
        Ok(())
    }
}

/// SWIM liveness of a member, as currently believed locally.
///
/// Deliberately does *not* include a `Removed` state. Removal is recorded as a
/// [`MembershipTombstone`], because conflating the two would let a failure
/// detector's timeout produce the same outcome as an operator's decision, and
/// would let a later `Alive` announcement undo that decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Liveness {
    /// Responding to probes, directly or indirectly.
    Alive,
    /// Missed a direct and indirect probe; awaiting refutation.
    Suspected,
    /// Suspicion expired without refutation. Terminal within this formation.
    Dead,
}

impl std::fmt::Display for Liveness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Alive => "alive",
            Self::Suspected => "suspected",
            Self::Dead => "dead",
        })
    }
}

/// One admitted member, keyed in the model by its [`NodeId`] alone.
///
/// The label, the certificate fingerprint, and the assigned ID are separate
/// fields with separate types precisely so that no lookup can accidentally key
/// on a non-unique label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    /// Formation-assigned identity. The only key membership uses.
    pub id: NodeId,
    /// Operator label. Non-unique; diagnostic and display only.
    pub name: WorkerName,
    /// Certificate fingerprint pinned at admission.
    pub cert_fingerprint: CertFingerprint,
    /// Peer-protocol version this member speaks.
    pub protocol: ProtocolVersion,
    /// Locally believed liveness.
    pub liveness: Liveness,
    /// Incarnation associated with `liveness`.
    pub incarnation: Incarnation,
    /// Version of this record, for deterministic merge.
    pub version: VersionTuple,
    /// Advertised addresses.
    pub endpoints: Endpoints,
    /// Advertised role flags.
    pub accepts: Accepts,
    /// Advertised connection capacity.
    pub capacity: NodeCapacity,
    /// Advertised hardware and runtime capability.
    pub capabilities: Capabilities,
}

impl Member {
    /// Checks every bounded field before this record is adopted into state.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when any string or collection exceeds its
    /// configured limit.
    pub fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        self.endpoints.validate(limits)?;
        self.capabilities.validate(limits)?;
        Ok(())
    }
}

/// What a blocklist entry matches on.
///
/// Identity keys are exact. `Network` is opaque to the core; see
/// [`NetworkPattern`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlocklistKey {
    /// A formation-assigned node ID.
    Node(NodeId),
    /// A worker label. Matches every worker using that label, which is why a
    /// name block is a blunt instrument.
    Name(WorkerName),
    /// A certificate fingerprint.
    Fingerprint(CertFingerprint),
    /// A host address or CIDR range, matched by the IO shell.
    Network(NetworkPattern),
}

impl std::fmt::Display for BlocklistKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(id) => write!(f, "node:{id}"),
            Self::Name(name) => write!(f, "name:{name}"),
            Self::Fingerprint(fingerprint) => write!(f, "cert:{fingerprint}"),
            Self::Network(pattern) => write!(f, "net:{pattern}"),
        }
    }
}

/// Whether a blocklist entry currently blocks or has been lifted.
///
/// Lifting is a tombstoned `Allow` rather than a deletion so that the removal
/// itself carries a version and converges; deleting the entry outright would
/// let a stale replica re-add it forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlocklistAction {
    /// The key is blocked.
    Block,
    /// A previous block was lifted.
    Allow,
}

/// One replicated blocklist entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlocklistEntry {
    /// What this entry matches.
    pub key: BlocklistKey,
    /// Whether the key is currently blocked.
    pub action: BlocklistAction,
    /// Version of this entry, for deterministic merge.
    pub version: VersionTuple,
    /// Operator identity that recorded the entry. Diagnostic only.
    pub added_by: String,
}

impl BlocklistEntry {
    /// Checks bounded fields before adoption.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when a text field exceeds its limit.
    pub fn validate(&self, limits: &Limits) -> Result<(), ValidationError> {
        check_text("blocklist.addedBy", &self.added_by, limits.max_text_len())?;
        if let BlocklistKey::Network(pattern) = &self.key {
            check_text("blocklist.network", &pattern.0, limits.max_text_len())?;
        }
        Ok(())
    }
}

/// The policy inputs an admission decision reads.
///
/// Held in the model so that admission is a pure function of `(model,
/// message)`. It deliberately holds no credential: the current join token
/// never enters the model, because the model is replayable and a replayable
/// secret is a leaked secret. Token comparison happens in the shell and
/// returns as [`crate::message::AdmissionEvidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionPolicy {
    /// Operator-set membership lock. While set, no node is admitted.
    pub membership_locked: bool,
    /// Whether this node acts as an introducer at all.
    pub accepts_peers: bool,
    /// Maximum members this formation will hold, including the local node.
    pub capacity: usize,
    /// Peer-protocol versions this node interoperates with.
    pub protocol_range: ProtocolRange,
}

impl Default for AdmissionPolicy {
    fn default() -> Self {
        Self {
            membership_locked: false,
            accepts_peers: true,
            capacity: 4096,
            protocol_range: ProtocolRange::default(),
        }
    }
}

/// Local identity and advertised description of this worker.
///
/// Both IDs are *inputs*: the core cannot generate them, because generating an
/// identity needs entropy and entropy would make replay non-deterministic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalIdentity {
    /// This node's formation-assigned ID within `formation`.
    pub node_id: NodeId,
    /// Operator label for this worker.
    pub worker_name: WorkerName,
    /// This node's certificate fingerprint.
    pub cert_fingerprint: CertFingerprint,
    /// Peer-protocol version this node speaks.
    pub protocol: ProtocolVersion,
    /// Addresses this node advertises.
    pub endpoints: Endpoints,
    /// Role flags this node advertises.
    pub accepts: Accepts,
    /// Connection capacity this node advertises.
    pub capacity: NodeCapacity,
    /// Hardware and runtime capability this node advertises.
    pub capabilities: Capabilities,
}

/// Which role a probe in flight is playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbePhase {
    /// A direct `Ping` we sent, awaiting the target's `Ack`.
    Direct,
    /// `PingReq`s we sent to intermediaries, awaiting any `PingReply`.
    Indirect {
        /// Intermediaries asked. A reply from anyone else is ignored.
        helpers: Vec<NodeId>,
        /// Intermediaries that have already answered, so a duplicate reply
        /// cannot be counted twice toward "everyone failed".
        answered: BTreeSet<NodeId>,
    },
    /// A `Ping` we sent on another member's behalf, awaiting the target's
    /// `Ack` so we can answer the requester's `PingReq`.
    Relay {
        /// Member that asked us to probe.
        requester: NodeId,
        /// The requester's own probe ID, echoed back in our `PingReply`.
        requester_probe: ProbeId,
    },
}

/// A probe in flight.
///
/// The [`TimerToken`] recorded here is the *only* expiry that may act on this
/// probe. A timer armed for a superseded phase, or for a probe that has since
/// completed, is ignored: without that check a late timer from a previous
/// round could suspect a member that has already answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProbe {
    /// Correlation ID carried on the wire and echoed in replies.
    pub id: ProbeId,
    /// Member being probed.
    pub target: NodeId,
    /// The target's incarnation when the probe started, used when announcing
    /// suspicion so a refutation can outrank it.
    pub target_incarnation: Incarnation,
    /// Current phase of this probe.
    pub phase: ProbePhase,
    /// The one timer whose expiry is still actionable for this probe.
    pub timer: TimerToken,
}

/// A member currently under suspicion locally, with its deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspicion {
    /// Incarnation the suspicion was raised against.
    pub incarnation: Incarnation,
    /// The one timer whose expiry may declare this member dead.
    pub timer: TimerToken,
}

/// How far an admission decision has progressed while waiting for the shell.
///
/// Each stage names the request it is waiting for, so an outcome that arrives
/// for a superseded or already-answered request is discarded rather than
/// applied to whatever decision happens to occupy the session now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionStage {
    /// Local gates passed; awaiting the shell's credential verdict.
    AwaitingEvidence {
        /// Verification this decision is waiting on.
        request: crate::effect::VerificationId,
    },
    /// Evidence accepted; awaiting a freshly generated node ID.
    AwaitingNodeId {
        /// Allocation this decision is waiting on.
        request: crate::effect::AllocationId,
    },
}

/// An admission decision parked awaiting a correlated effect outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAdmission {
    /// Pre-admission session the applicant is connected on.
    pub session: SessionId,
    /// Applicant's label. Not identity.
    pub name: WorkerName,
    /// Applicant's certificate fingerprint.
    pub cert_fingerprint: CertFingerprint,
    /// Protocol version the applicant offered.
    pub protocol: ProtocolVersion,
    /// Description the applicant advertised, already bounds-checked.
    pub endpoints: Endpoints,
    /// Role flags the applicant advertised.
    pub accepts: Accepts,
    /// Connection capacity the applicant advertised.
    pub capacity: NodeCapacity,
    /// Capability the applicant advertised.
    pub capabilities: Capabilities,
    /// Progress of this decision.
    pub stage: AdmissionStage,
}

/// This node's own attempt to join another formation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinAttempt {
    /// Session the `JoinReq` was sent on.
    pub session: SessionId,
    /// Formation being joined. A `JoinReply` naming any other formation is
    /// rejected.
    pub target_formation: FormationId,
    /// Attempt number, one-based, driving the backoff.
    pub attempt: u32,
    /// Untrusted redirect hints collected so far, for the shell to try next.
    pub redirects: Vec<Address>,
    /// The one retry timer that may act on this attempt.
    pub timer: TimerToken,
}

/// Where a truncated anti-entropy reply should resume.
///
/// Ordering is canonical — ascending bucket, then ascending leaf key — so two
/// nodes resume at the same place regardless of how their maps are stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntiEntropyCursor {
    /// Bucket to resume in.
    pub bucket: u16,
    /// Resume strictly after this canonical leaf key.
    pub after_key: Vec<u8>,
}

/// An anti-entropy round in progress, initiated locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntiEntropyRound {
    /// Peer being reconciled with.
    pub peer: NodeId,
    /// Round identifier echoed by the peer, so a reply from an abandoned round
    /// cannot be adopted.
    pub round: u64,
    /// Where to resume, when the last reply was truncated.
    pub cursor: Option<AntiEntropyCursor>,
    /// Exchanges used so far, bounded by
    /// [`Limits::max_anti_entropy_rounds`].
    pub exchanges: u32,
    /// The one timer whose expiry may abandon this round.
    pub timer: TimerToken,
}

/// One worker's replicated view of its cluster formation.
///
/// This is the whole of the membership state. `update` takes it by value and
/// returns a new one, so a caller can keep the previous model for comparison
/// or discard it; internally `update` mutates the owned value rather than
/// cloning the member map, which is invisible from the outside and is what
/// keeps a transition proportional to what changed rather than to formation
/// size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    formation: FormationId,
    cluster_name: ClusterName,
    local: LocalIdentity,
    incarnation: Incarnation,
    next_seq: u64,
    next_correlation: u64,
    members: BTreeMap<NodeId, Member>,
    tombstones: BTreeMap<NodeId, MembershipTombstone>,
    blocklist: BTreeMap<BlocklistKey, BlocklistEntry>,
    last_seq: BTreeMap<NodeId, u64>,
    probes: BTreeMap<ProbeId, PendingProbe>,
    suspicions: BTreeMap<NodeId, Suspicion>,
    admissions: BTreeMap<SessionId, PendingAdmission>,
    selections: BTreeMap<SelectionId, crate::message::SelectionPurpose>,
    join_attempt: Option<JoinAttempt>,
    anti_entropy: Option<AntiEntropyRound>,
    gossip: GossipQueue,
    policy: AdmissionPolicy,
    limits: Limits,
}

impl Membership {
    /// Creates the one-node standalone formation a fresh worker starts in.
    ///
    /// A worker is never "not in a cluster": it is the sole member of its own
    /// formation, which is why [`Membership::formation`] is not an `Option`.
    /// Both `formation` and `local.node_id` come from the shell, since
    /// generating them requires entropy.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when the local node's own advertised
    /// description exceeds the configured limits — a misconfiguration worth
    /// failing at start-up rather than gossiping to peers.
    pub fn standalone(
        formation: FormationId,
        cluster_name: ClusterName,
        local: LocalIdentity,
        policy: AdmissionPolicy,
        limits: Limits,
    ) -> Result<Self, ValidationError> {
        local.endpoints.validate(&limits)?;
        local.capabilities.validate(&limits)?;

        let self_member = Member {
            id: local.node_id.clone(),
            name: local.worker_name.clone(),
            cert_fingerprint: local.cert_fingerprint,
            protocol: local.protocol,
            liveness: Liveness::Alive,
            incarnation: Incarnation::INITIAL,
            version: VersionTuple::initial(0, local.node_id.clone()),
            endpoints: local.endpoints.clone(),
            accepts: local.accepts,
            capacity: local.capacity,
            capabilities: local.capabilities.clone(),
        };

        Ok(Self {
            formation,
            cluster_name,
            members: BTreeMap::from([(local.node_id.clone(), self_member)]),
            local,
            incarnation: Incarnation::INITIAL,
            next_seq: 0,
            next_correlation: 0,
            tombstones: BTreeMap::new(),
            blocklist: BTreeMap::new(),
            last_seq: BTreeMap::new(),
            probes: BTreeMap::new(),
            suspicions: BTreeMap::new(),
            admissions: BTreeMap::new(),
            selections: BTreeMap::new(),
            join_attempt: None,
            anti_entropy: None,
            gossip: GossipQueue::default(),
            policy,
            limits,
        })
    }

    /// Replaces this node's formation wholesale after a successful join.
    ///
    /// Constructed field by field rather than by clearing the old model: ADR
    /// 0013 requires that a new formation inherit no membership, locks,
    /// tombstones, probe state, or versions from the old one, and a
    /// constructor makes that reviewable by reading it, whereas a sequence of
    /// `clear()` calls silently keeps whatever field the author forgot.
    ///
    /// `members` is the validated bootstrap snapshot, which seeds a local view
    /// and then converges normally; it confers no permanent authority on the
    /// introducer.
    pub(crate) fn adopt_formation(
        &self,
        formation: FormationId,
        cluster_name: ClusterName,
        assigned: NodeId,
        members: BTreeMap<NodeId, Member>,
    ) -> Self {
        let mut local = self.local.clone();
        local.node_id = assigned;
        Self {
            formation,
            cluster_name,
            local,
            incarnation: Incarnation::INITIAL,
            next_seq: 0,
            next_correlation: 0,
            members,
            tombstones: BTreeMap::new(),
            blocklist: BTreeMap::new(),
            last_seq: BTreeMap::new(),
            probes: BTreeMap::new(),
            suspicions: BTreeMap::new(),
            admissions: BTreeMap::new(),
            selections: BTreeMap::new(),
            join_attempt: None,
            anti_entropy: None,
            gossip: GossipQueue::default(),
            policy: self.policy.clone(),
            limits: self.limits.clone(),
        }
    }

    // ── Identity ─────────────────────────────────────────────────────────

    /// The formation this node currently belongs to. Never absent: a fresh
    /// worker is the sole member of its own standalone formation.
    #[must_use]
    pub fn formation(&self) -> &FormationId {
        &self.formation
    }

    /// The formation's operator label. Not identity, not a security boundary.
    #[must_use]
    pub fn cluster_name(&self) -> &ClusterName {
        &self.cluster_name
    }

    /// This node's formation-assigned ID.
    #[must_use]
    pub fn local_id(&self) -> &NodeId {
        &self.local.node_id
    }

    /// This node's operator label.
    #[must_use]
    pub fn local_name(&self) -> &WorkerName {
        &self.local.worker_name
    }

    /// This node's own advertised description.
    #[must_use]
    pub fn local(&self) -> &LocalIdentity {
        &self.local
    }

    /// This node's current SWIM incarnation.
    #[must_use]
    pub fn incarnation(&self) -> Incarnation {
        self.incarnation
    }

    // ── Membership view ──────────────────────────────────────────────────

    /// Admitted members, keyed by assigned ID, in ascending key order.
    #[must_use]
    pub fn members(&self) -> &BTreeMap<NodeId, Member> {
        &self.members
    }

    /// Looks up one member.
    #[must_use]
    pub fn member(&self, id: &NodeId) -> Option<&Member> {
        self.members.get(id)
    }

    /// Number of members currently believed alive.
    ///
    /// `O(n)` in formation size. Use it for metrics and operator views, not on
    /// the message hot path — see [`Membership::scale_size`].
    #[must_use]
    pub fn alive_count(&self) -> usize {
        self.members
            .values()
            .filter(|member| member.liveness == Liveness::Alive)
            .count()
    }

    /// Formation size used to scale the protocol's `log2(N)` parameters.
    ///
    /// Total membership, not the alive subset. The protocol defines `N` as the
    /// alive count, but counting that is `O(n)` and both consumers — the
    /// suspicion timeout and the gossip hop limit — scale as `ceil(log2(N))`,
    /// so substituting the total changes the result by at most a few units.
    /// Every one of those units errs conservatively: a longer suspicion
    /// timeout (fewer false positives) and a wider dissemination window (more
    /// reliable spread), never the reverse.
    ///
    /// Measured on a 10,000-member formation, this was the difference between
    /// a per-message cost of ~138 µs and ~1 µs; see `benches/membership.rs`.
    #[must_use]
    pub fn scale_size(&self) -> usize {
        self.members.len()
    }

    /// Membership tombstones, keyed by the removed node's assigned ID.
    #[must_use]
    pub fn tombstones(&self) -> &BTreeMap<NodeId, MembershipTombstone> {
        &self.tombstones
    }

    /// Replicated blocklist entries.
    #[must_use]
    pub fn blocklist(&self) -> &BTreeMap<BlocklistKey, BlocklistEntry> {
        &self.blocklist
    }

    /// The admission policy in force.
    #[must_use]
    pub fn policy(&self) -> &AdmissionPolicy {
        &self.policy
    }

    /// The validated limits in force.
    #[must_use]
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Probes currently in flight, keyed by correlation ID.
    #[must_use]
    pub fn probes(&self) -> &BTreeMap<ProbeId, PendingProbe> {
        &self.probes
    }

    /// Members currently under local suspicion.
    #[must_use]
    pub fn suspicions(&self) -> &BTreeMap<NodeId, Suspicion> {
        &self.suspicions
    }

    /// Admission decisions parked awaiting an effect outcome.
    #[must_use]
    pub fn admissions(&self) -> &BTreeMap<SessionId, PendingAdmission> {
        &self.admissions
    }

    /// Peer-selection requests awaiting an answer from the shell.
    #[must_use]
    pub fn selections(&self) -> &BTreeMap<SelectionId, crate::message::SelectionPurpose> {
        &self.selections
    }

    /// This node's join attempt, when one is in progress.
    #[must_use]
    pub fn join_attempt(&self) -> Option<&JoinAttempt> {
        self.join_attempt.as_ref()
    }

    /// The anti-entropy round in progress, when there is one.
    #[must_use]
    pub fn anti_entropy(&self) -> Option<&AntiEntropyRound> {
        self.anti_entropy.as_ref()
    }

    /// Deltas queued for piggybacking.
    #[must_use]
    pub fn gossip(&self) -> &GossipQueue {
        &self.gossip
    }

    /// Whether `id` is currently fenced out of this formation.
    ///
    /// A fenced ID cannot be readmitted and cannot receive any liveness
    /// update, however new its incarnation. An explicitly cleared tombstone
    /// stops fencing while remaining on record, so this is not simply "a
    /// tombstone exists" — see [`Membership::has_tombstone`].
    #[must_use]
    pub fn is_tombstoned(&self, id: &NodeId) -> bool {
        self.tombstones
            .get(id)
            .is_some_and(|tombstone| !tombstone.cleared)
    }

    /// Whether a tombstone exists for `id`, cleared or not.
    ///
    /// Identity reuse checks want this rather than
    /// [`Membership::is_tombstoned`]: a cleared tombstone still records that
    /// the ID was once in use, and handing it out again would make the audit
    /// trail ambiguous.
    #[must_use]
    pub fn has_tombstone(&self, id: &NodeId) -> bool {
        self.tombstones.contains_key(id)
    }

    /// Whether `key` is currently blocked.
    #[must_use]
    pub fn is_blocked(&self, key: &BlocklistKey) -> bool {
        self.blocklist
            .get(key)
            .is_some_and(|entry| entry.action == BlocklistAction::Block)
    }

    /// The suspicion timeout appropriate to the current formation size.
    #[must_use]
    pub fn suspicion_timeout(&self) -> DurationMillis {
        self.limits.effective_suspicion_timeout(self.scale_size())
    }

    // ── Mutation, used only by `update` and its helpers ───────────────────

    pub(crate) fn members_mut(&mut self) -> &mut BTreeMap<NodeId, Member> {
        &mut self.members
    }

    pub(crate) fn tombstones_mut(&mut self) -> &mut BTreeMap<NodeId, MembershipTombstone> {
        &mut self.tombstones
    }

    pub(crate) fn blocklist_mut(&mut self) -> &mut BTreeMap<BlocklistKey, BlocklistEntry> {
        &mut self.blocklist
    }

    pub(crate) fn probes_mut(&mut self) -> &mut BTreeMap<ProbeId, PendingProbe> {
        &mut self.probes
    }

    pub(crate) fn suspicions_mut(&mut self) -> &mut BTreeMap<NodeId, Suspicion> {
        &mut self.suspicions
    }

    pub(crate) fn admissions_mut(&mut self) -> &mut BTreeMap<SessionId, PendingAdmission> {
        &mut self.admissions
    }

    pub(crate) fn selections_mut(
        &mut self,
    ) -> &mut BTreeMap<SelectionId, crate::message::SelectionPurpose> {
        &mut self.selections
    }

    pub(crate) fn gossip_mut(&mut self) -> &mut GossipQueue {
        &mut self.gossip
    }

    pub(crate) fn set_join_attempt(&mut self, attempt: Option<JoinAttempt>) {
        self.join_attempt = attempt;
    }

    pub(crate) fn join_attempt_mut(&mut self) -> Option<&mut JoinAttempt> {
        self.join_attempt.as_mut()
    }

    pub(crate) fn set_anti_entropy(&mut self, round: Option<AntiEntropyRound>) {
        self.anti_entropy = round;
    }

    /// Sets this node's incarnation, used only when refuting suspicion.
    pub(crate) fn set_incarnation(&mut self, incarnation: Incarnation) {
        self.incarnation = incarnation;
    }

    /// Replaces the admission policy, for operator lock/unlock and capacity
    /// changes.
    pub(crate) fn set_policy(&mut self, policy: AdmissionPolicy) {
        self.policy = policy;
    }

    /// Next outbound sequence number. Saturating rather than wrapping: a
    /// wrapped sequence would look like a replay to every peer.
    pub(crate) fn next_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        seq
    }

    /// Next correlation number.
    ///
    /// The counter is the core's substitute for randomness: probe IDs, timer
    /// tokens, and effect request IDs all come from it, so the same message
    /// sequence always produces the same identifiers and a recorded session
    /// replays exactly.
    pub(crate) fn next_correlation(&mut self) -> u64 {
        let value = self.next_correlation;
        self.next_correlation = self.next_correlation.saturating_add(1);
        value
    }

    /// Records an inbound sequence number, reporting whether it is a replay of
    /// one already seen from that sender.
    pub(crate) fn observe_seq(&mut self, sender: &NodeId, seq: u64) -> bool {
        match self.last_seq.get_mut(sender) {
            Some(last) if seq <= *last => true,
            Some(last) => {
                *last = seq;
                false
            }
            None => {
                // Bounded by member count: a sender that is not a member never
                // reaches here, because the sender-binding check runs first.
                self.last_seq.insert(sender.clone(), seq);
                false
            }
        }
    }

    /// Drops per-sender bookkeeping for a member that is no longer present.
    pub(crate) fn forget_sender(&mut self, sender: &NodeId) {
        self.last_seq.remove(sender);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn a_fresh_worker_is_the_sole_member_of_its_own_formation() {
        let model = testing::standalone("node-self");
        assert_eq!(model.members().len(), 1);
        assert_eq!(model.alive_count(), 1);
        assert_eq!(
            model.member(model.local_id()).unwrap().liveness,
            Liveness::Alive
        );
        assert!(model.tombstones().is_empty());
    }

    #[test]
    fn adopting_a_formation_imports_no_state_from_the_old_one() {
        let mut model = testing::standalone("node-self");
        let stale = testing::member("node-old", 1);
        model.members_mut().insert(stale.id.clone(), stale.clone());
        model.tombstones_mut().insert(
            stale.id.clone(),
            testing::tombstone("node-old", orishu_identity::RemovalMode::Force),
        );
        model.blocklist_mut().insert(
            BlocklistKey::Name(WorkerName::new("banned").unwrap()),
            testing::blocklist_entry("banned"),
        );
        model.set_policy(AdmissionPolicy {
            membership_locked: true,
            ..AdmissionPolicy::default()
        });

        let target = FormationId::new("formation-target").unwrap();
        let assigned = NodeId::new("node-assigned").unwrap();
        let introducer = testing::member("node-introducer", 1);
        let snapshot = BTreeMap::from([(introducer.id.clone(), introducer)]);
        let adopted = model.adopt_formation(
            target.clone(),
            ClusterName::new("target").unwrap(),
            assigned.clone(),
            snapshot,
        );

        assert_eq!(adopted.formation(), &target);
        assert_eq!(adopted.local_id(), &assigned);
        assert!(
            adopted.tombstones().is_empty(),
            "tombstones must not carry over"
        );
        assert!(
            adopted.blocklist().is_empty(),
            "blocklist must not carry over"
        );
        assert!(!adopted.members().contains_key(&stale.id));
        assert_eq!(adopted.incarnation(), Incarnation::INITIAL);
        // Local configuration is not formation state, so it does carry over.
        assert!(adopted.policy().membership_locked);
        assert_eq!(adopted.local_name(), model.local_name());
    }

    #[test]
    fn sequence_tracking_reports_replays() {
        let mut model = testing::standalone("node-self");
        let sender = NodeId::new("node-peer").unwrap();
        assert!(!model.observe_seq(&sender, 5));
        assert!(model.observe_seq(&sender, 5), "same sequence is a replay");
        assert!(model.observe_seq(&sender, 4), "older sequence is a replay");
        assert!(!model.observe_seq(&sender, 6));
    }

    #[test]
    fn correlation_counter_is_deterministic() {
        let mut first = testing::standalone("node-self");
        let mut second = testing::standalone("node-self");
        let from_first: Vec<u64> = (0..4).map(|_| first.next_correlation()).collect();
        let from_second: Vec<u64> = (0..4).map(|_| second.next_correlation()).collect();
        assert_eq!(from_first, from_second);
        assert_eq!(from_first, vec![0, 1, 2, 3]);
    }

    #[test]
    fn oversized_advertised_state_is_rejected_at_construction() {
        let limits = Limits::default();
        let mut local = testing::local_identity("node-self");
        local.endpoints.peers = (0..limits.max_addresses_per_node() + 1)
            .map(|i| Address(format!("10.0.0.{i}:6655")))
            .collect();
        let error = Membership::standalone(
            FormationId::new("formation-1").unwrap(),
            ClusterName::new("test").unwrap(),
            local,
            AdmissionPolicy::default(),
            limits,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ValidationError::TooMany {
                field: "endpoints.peers",
                ..
            }
        ));
    }
}
