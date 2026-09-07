//! Sans-IO cluster membership for `orishu-worker`.
//!
//! One worker's view of who is in its cluster formation and whether they are
//! alive, expressed as a functional core: an immutable-by-contract
//! [`Membership`] model, a closed [`Message`] enum of everything that can
//! change it, and a pure [`update`] that folds one message into the next model
//! plus a bounded list of [`Effect`]s.
//!
//! ```
//! use orishu_membership::{Message, Command, update, testing};
//!
//! let model = testing::model_with_members(3);
//! // A probe round is a value, not a side effect: nothing has been sent.
//! let transition = update(model, Message::Local(Command::StartProbeRound));
//! assert!(matches!(
//!     transition.effects.as_slice(),
//!     [orishu_membership::Effect::SelectPeers { .. }]
//! ));
//! ```
//!
//! # What "sans-IO" buys, and what it costs
//!
//! The core never opens a socket, sleeps, reads a clock, generates a random
//! value, touches the filesystem, or hides work in an async task. Everything
//! it cannot compute — which peer to probe, what a fresh node ID should be,
//! whether a presented token matches — leaves as an effect and comes back as a
//! correlated [`EffectOutcome`]. An effect is a *request*: `update` never
//! treats one as having succeeded.
//!
//! The payoff is that every membership transition is a value. A recorded
//! message sequence replays to exactly the same state on a machine with no
//! network, which makes the SWIM state machine, the admission gates, and the
//! merge rules testable without a runtime, a fake socket, or a sleep.
//!
//! The cost is that the shell owns more: it must correlate outcomes back to
//! the requests that produced them, and it must not "helpfully" apply a
//! decision the core has not made. `Cargo.toml` documents the dependency
//! budget that keeps this honest, and `tests/dependencies.rs` enforces it.
//!
//! # What the core owns
//!
//! - **Admission** — every gate in the runtime design, decided atomically. See
//!   [`admission`].
//! - **Join adoption** — validating a `JoinReply` and atomically replacing this
//!   node's formation, importing nothing from the old one.
//! - **SWIM** — correlated direct probing, indirect probing, suspicion, death,
//!   and refutation. See [`update`].
//! - **Gossip merge** — one deterministic path for every carrier. See
//!   [`merge`](crate::gossip).
//! - **Anti-entropy** — canonical hashing, bounded resumable rounds. See
//!   [`antientropy`].
//!
//! # What the core deliberately does not own
//!
//! - Wire framing, CBOR, QUIC, mTLS, address resolution, timer wheels, and
//!   entropy. Those are the IO shell's, and this crate must not acquire a
//!   dependency on any of them.
//! - Non-membership gossip. `WorkloadUpdate`, checkpoint, result, and audit
//!   deltas are carried across untouched in
//!   [`Transition::foreign_gossip`] and never decoded here.
//! - The join token, private keys, and any other secret. The model is
//!   replayable, and a replayable secret is a leaked secret; credential checks
//!   happen behind [`Effect::VerifyCredential`].
//!
//! The versioned [`MembershipPolicy`] lock is replicated through gossip and
//! anti-entropy. [`AdmissionPolicy`] contains only node-local admission settings.
//! The worker shell must finish admission-state catch-up before introducing peers.
//!
//! # Decoder obligations
//!
//! The bounds in [`Limits`] are defence in depth. By the time a value reaches
//! `update`, its `Vec`s are allocated, so re-checking a length prevents
//! *adoption* of an oversized record but cannot un-allocate it. The following
//! remain mandatory in the future wire decoder:
//!
//! - Enforce the frame and datagram size limits before buffering.
//! - Bound every collection length *while* decoding — gossip arrays, snapshot
//!   arrays, bucket-hash arrays, address and accelerator lists — rather than
//!   decoding first and checking after.
//! - Reject a `formationId` mismatch and an unauthenticated session before
//!   decoding the payload at all.
//! - Report the encoded snapshot size as
//!   [`JoinReply::Accepted::snapshot_bytes`], which the core cannot measure
//!   itself.
//! - Keep the raw join token out of [`PeerBody::JoinRequest`]; answer
//!   [`Effect::VerifyCredential`] with a verdict instead.
//!
//! [`AdmissionPolicy`]: crate::model::AdmissionPolicy
//! [`JoinReply::Accepted::snapshot_bytes`]: crate::message::JoinReply

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod admission;
pub mod antientropy;
pub mod baseline;
pub mod effect;
pub mod gossip;
pub mod limits;
mod merge;
pub mod message;
pub mod model;
pub mod testing;
mod update;

pub use baseline::AdmissionBaseline;
pub use effect::{
    AllocationId, Announcement, ChangeRecord, CredentialKind, Destination, Diagnostic, Effect,
    ForeignHandoff, IndirectResult, OutboundBody, OutboundMessage, ProbeId, RejectReason,
    SelectionId, SessionId, TimerKind, TimerToken, Transition, VerificationId,
};
pub use gossip::{DeltaBody, ForeignDelta, GossipDelta, GossipKey, GossipQueue, OpaquePayload};
pub use limits::{DurationMillis, Limits, LimitsError, LimitsSpec};
pub use message::{
    AdmissionEvidence, Command, EffectOutcome, JoinReply, Message, PeerBody, PeerContext,
    PeerInput, SelectionPurpose, SenderIdentity,
};
pub use model::{
    Accepts, Address, AdmissionPolicy, Capabilities, EngineCapability, Liveness, LocalIdentity,
    Member, Membership, MembershipPolicy, NetworkPattern, NodeCapacity, ValidationError,
};
pub use update::update;

// Re-exported so an adapter can name the shared identity contract without
// taking its own dependency edge, and so there is visibly one definition of
// each of these types rather than a membership-private substitute.
pub use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, MembershipTombstone, NodeId,
    ProtocolRange, ProtocolVersion, RemovalMode, VersionTuple, WorkerName,
};
