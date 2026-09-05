//! Deterministic fixtures for tests, benchmarks, and downstream adapters.
//!
//! Everything here is generated from an index, never from a clock or a
//! random-number generator, so the same call always produces the same value.
//! That is what lets a benchmark compare two commits and a test compare two
//! models.
//!
//! This module is part of the public API on purpose: an adapter that drives
//! the core needs the same fixtures to test its own wiring, and Criterion
//! benchmarks cannot see `#[cfg(test)]` items.

use std::collections::BTreeMap;

use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, MembershipTombstone, NodeId,
    ProtocolVersion, RemovalMode, VersionTuple, WorkerName,
};

use crate::{
    effect::SessionId,
    gossip::{ForeignDelta, OpaquePayload},
    limits::Limits,
    message::{PeerContext, SenderIdentity},
    model::{
        Accepts, Address, AdmissionPolicy, BlocklistAction, BlocklistEntry, BlocklistKey,
        Capabilities, Endpoints, EngineCapability, Liveness, LocalIdentity, Member, Membership,
        NodeCapacity,
    },
};

/// The formation ID every fixture model uses.
#[must_use]
pub fn formation() -> FormationId {
    FormationId::new("formation-0000-test").unwrap()
}

/// A version tuple at `counter`, attributed to a fixed actor.
#[must_use]
pub fn version(counter: u64) -> VersionTuple {
    VersionTuple {
        epoch: 0,
        counter,
        actor: NodeId::new("node-actor").unwrap(),
    }
}

/// A fingerprint derived from `seed`, distinct for distinct seeds.
#[must_use]
pub fn fingerprint(seed: u8) -> CertFingerprint {
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = seed.wrapping_add(index as u8);
    }
    CertFingerprint::from_bytes(bytes)
}

/// Capabilities with enough populated fields to exercise canonical encoding.
#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities {
        cpu_cores: 8,
        memory_bytes: 16 * 1024 * 1024 * 1024,
        architecture: "x86_64".into(),
        storage_backend: "local".into(),
        storage_replicas: Some(3),
        accelerators: vec!["cuda".into()],
        engines: vec![EngineCapability {
            engine: "wasm-component".into(),
            runtime_lifecycle: "orishu.workload/v1".into(),
        }],
    }
}

/// This node's advertised description.
#[must_use]
pub fn local_identity(id: &str) -> LocalIdentity {
    LocalIdentity {
        node_id: NodeId::new(id).unwrap(),
        worker_name: WorkerName::new("worker-local").unwrap(),
        cert_fingerprint: fingerprint(0x10),
        protocol: ProtocolVersion::CURRENT,
        endpoints: Endpoints {
            peers: vec![Address("10.0.0.1:6655".into())],
            clients: vec![],
        },
        accepts: Accepts::default(),
        capacity: NodeCapacity {
            peers: 32_768,
            clients: 1_024,
        },
        capabilities: capabilities(),
    }
}

/// A member record for `id` at version `counter`.
#[must_use]
pub fn member(id: &str, counter: u64) -> Member {
    let node = NodeId::new(id).unwrap();
    Member {
        name: WorkerName::new(format!("worker-{id}")).unwrap(),
        cert_fingerprint: fingerprint(id.len() as u8),
        protocol: ProtocolVersion::CURRENT,
        liveness: Liveness::Alive,
        incarnation: Incarnation::INITIAL,
        version: VersionTuple {
            epoch: 0,
            counter,
            actor: node.clone(),
        },
        endpoints: Endpoints {
            peers: vec![Address(format!("10.0.0.{}:6655", id.len()))],
            clients: vec![],
        },
        accepts: Accepts::default(),
        capacity: NodeCapacity {
            peers: 32_768,
            clients: 1_024,
        },
        capabilities: capabilities(),
        id: node,
    }
}

/// A tombstone for `id`.
#[must_use]
pub fn tombstone(id: &str, mode: RemovalMode) -> MembershipTombstone {
    MembershipTombstone {
        node_id: NodeId::new(id).unwrap(),
        name: WorkerName::new(format!("worker-{id}")).unwrap(),
        cert_fingerprint: fingerprint(id.len() as u8),
        removal_mode: mode,
        version: version(1),
        cleared: false,
        reason: None,
    }
}

/// A blocklist entry blocking the label `name`.
#[must_use]
pub fn blocklist_entry(name: &str) -> BlocklistEntry {
    let key = BlocklistKey::Name(WorkerName::new(name).unwrap());
    BlocklistEntry {
        key,
        action: BlocklistAction::Block,
        version: version(1),
        added_by: "operator-1".into(),
    }
}

/// A delta belonging to a subsystem other than membership.
#[must_use]
pub fn foreign_delta() -> ForeignDelta {
    ForeignDelta {
        delta_type: "WorkloadUpdate".into(),
        key: "workload-42".into(),
        version: version(3),
        payload: OpaquePayload(vec![0xDE, 0xAD, 0xBE, 0xEF]),
    }
}

/// A standalone formation of one, with default policy and limits.
#[must_use]
pub fn standalone(id: &str) -> Membership {
    Membership::standalone(
        formation(),
        ClusterName::new("test-cluster").unwrap(),
        local_identity(id),
        AdmissionPolicy::default(),
        Limits::default(),
    )
    .expect("fixture identity is within default limits")
}

/// A formation of `count` peers plus the local node `node-self`.
///
/// Peer IDs are zero-padded (`node-0000`, `node-0001`, ...) so that their
/// ordering is stable regardless of count.
#[must_use]
pub fn model_with_members(count: usize) -> Membership {
    model_with_limits(count, Limits::default())
}

/// As [`model_with_members`], with explicit limits.
#[must_use]
pub fn model_with_limits(count: usize, limits: Limits) -> Membership {
    let mut model = Membership::standalone(
        formation(),
        ClusterName::new("test-cluster").unwrap(),
        local_identity("node-self"),
        AdmissionPolicy {
            capacity: usize::MAX,
            ..AdmissionPolicy::default()
        },
        limits,
    )
    .expect("fixture identity is within the supplied limits");
    for index in 0..count {
        let peer = member(&format!("node-{index:04}"), 1);
        model.members_mut().insert(peer.id.clone(), peer);
    }
    model
}

/// A small model with one of every replicated entity, used to pin the
/// canonical anti-entropy encoding.
///
/// Changing this fixture changes the golden digest, which is the point: the
/// digest is a wire contract and must not drift silently.
#[must_use]
pub fn golden_model() -> Membership {
    let mut model = standalone("node-self");
    let peer = member("node-0001", 4);
    model.members_mut().insert(peer.id.clone(), peer);
    let removed = tombstone("node-0002", RemovalMode::Force);
    model
        .tombstones_mut()
        .insert(removed.node_id.clone(), removed);
    let entry = blocklist_entry("worker-banned");
    model.blocklist_mut().insert(entry.key.clone(), entry);
    model
}

/// An authenticated context for a message from an admitted member.
#[must_use]
pub fn peer_context(model: &Membership, sender: &NodeId, seq: u64) -> PeerContext {
    PeerContext {
        session: SessionId(1),
        authenticated: true,
        cert_fingerprint: model
            .member(sender)
            .map_or_else(|| fingerprint(0), |member| member.cert_fingerprint),
        sender: SenderIdentity::Admitted(sender.clone()),
        formation: model.formation().clone(),
        protocol: ProtocolVersion::CURRENT,
        seq,
        gossip: Vec::new(),
    }
}

/// An authenticated context for a pre-admission applicant.
#[must_use]
pub fn applicant_context(
    formation: &FormationId,
    name: &str,
    session: SessionId,
    seed: u8,
) -> PeerContext {
    PeerContext {
        session,
        authenticated: true,
        cert_fingerprint: fingerprint(seed),
        sender: SenderIdentity::Applicant(WorkerName::new(name).unwrap()),
        formation: formation.clone(),
        protocol: ProtocolVersion::CURRENT,
        seq: 1,
        gossip: Vec::new(),
    }
}

/// Members of `model` as a snapshot map, for constructing join replies.
#[must_use]
pub fn snapshot_of(model: &Membership) -> BTreeMap<NodeId, Member> {
    model.members().clone()
}

/// Arranges a member directly into `model`, bypassing admission.
///
/// Integration tests live outside the crate and cannot reach the crate-private
/// mutators, which is deliberate: arranging state is a test affordance, and
/// naming it here keeps it out of the production surface.
pub fn insert_member(model: &mut Membership, member: Member) {
    model.members_mut().insert(member.id.clone(), member);
}

/// Arranges a tombstone directly into `model`.
pub fn insert_tombstone(model: &mut Membership, tombstone: MembershipTombstone) {
    model
        .tombstones_mut()
        .insert(tombstone.node_id.clone(), tombstone);
}

/// Arranges a blocklist entry directly into `model`.
pub fn insert_blocklist(model: &mut Membership, entry: BlocklistEntry) {
    model.blocklist_mut().insert(entry.key.clone(), entry);
}

/// Removes a member directly, for arranging a diverged starting state.
pub fn remove_member(model: &mut Membership, id: &NodeId) {
    model.members_mut().remove(id);
}

/// Sets a member's liveness directly, for arranging a starting state.
///
/// # Panics
///
/// Panics when `id` is not a member.
pub fn set_liveness(
    model: &mut Membership,
    id: &NodeId,
    liveness: Liveness,
    incarnation: Incarnation,
) {
    let member = model
        .members_mut()
        .get_mut(id)
        .expect("cannot set liveness of a non-member");
    member.liveness = liveness;
    member.incarnation = incarnation;
}

/// Makes every member advertise `accepts.peers`, for redirect tests.
pub fn make_all_introducers(model: &mut Membership) {
    for member in model.members_mut().values_mut() {
        member.accepts.peers = true;
    }
}

/// Drives a model through a sequence of messages, keeping the most recent
/// transition's output to hand.
///
/// A test-only convenience: the production contract is
/// [`update`](crate::update) alone, and everything the driver exposes is a
/// projection of the [`Transition`](crate::Transition) that `update` already
/// returned.
#[derive(Debug)]
pub struct Driver {
    model: Option<Membership>,
    /// Effects from the most recent [`Driver::apply`].
    pub effects: Vec<crate::effect::Effect>,
    /// Diagnostics from the most recent [`Driver::apply`].
    pub diagnostics: Vec<crate::effect::Diagnostic>,
    /// Foreign gossip from the most recent [`Driver::apply`].
    pub foreign: Vec<crate::effect::ForeignHandoff>,
}

impl Driver {
    /// Starts driving `model`.
    #[must_use]
    pub fn new(model: Membership) -> Self {
        Self {
            model: Some(model),
            effects: Vec::new(),
            diagnostics: Vec::new(),
            foreign: Vec::new(),
        }
    }

    /// Folds one message in, replacing the recorded transition output.
    pub fn apply(&mut self, message: crate::message::Message) -> &mut Self {
        let transition = crate::update(
            self.model.take().expect("driver always holds a model"),
            message,
        );
        self.model = Some(transition.model);
        self.effects = transition.effects;
        self.diagnostics = transition.diagnostics;
        self.foreign = transition.foreign_gossip;
        self
    }

    /// The current model.
    #[must_use]
    pub fn model(&self) -> &Membership {
        self.model.as_ref().expect("driver always holds a model")
    }

    /// The current model, for arranging state a test needs.
    pub fn model_mut(&mut self) -> &mut Membership {
        self.model.as_mut().expect("driver always holds a model")
    }

    /// Messages the last transition asked to send.
    #[must_use]
    pub fn sent(&self) -> Vec<(&crate::effect::Destination, &crate::effect::OutboundBody)> {
        self.effects
            .iter()
            .filter_map(|effect| match effect {
                crate::effect::Effect::Send {
                    destination,
                    message,
                } => Some((destination, &message.body)),
                _ => None,
            })
            .collect()
    }

    /// Timers the last transition asked to arm.
    #[must_use]
    pub fn armed(&self) -> Vec<crate::effect::TimerToken> {
        self.effects
            .iter()
            .filter_map(|effect| match effect {
                crate::effect::Effect::ArmTimer { token, .. } => Some(*token),
                _ => None,
            })
            .collect()
    }

    /// Timers the last transition asked to cancel.
    #[must_use]
    pub fn cancelled(&self) -> Vec<crate::effect::TimerToken> {
        self.effects
            .iter()
            .filter_map(|effect| match effect {
                crate::effect::Effect::CancelTimer { token } => Some(*token),
                _ => None,
            })
            .collect()
    }

    /// Changes the last transition asked to publish.
    #[must_use]
    pub fn published(&self) -> Vec<&crate::effect::ChangeRecord> {
        self.effects
            .iter()
            .filter_map(|effect| match effect {
                crate::effect::Effect::Publish(record) => Some(record),
                _ => None,
            })
            .collect()
    }

    /// The first peer-selection request the last transition made.
    #[must_use]
    pub fn selection(&self) -> Option<crate::effect::SelectionId> {
        self.effects.iter().find_map(|effect| match effect {
            crate::effect::Effect::SelectPeers { request, .. } => Some(*request),
            _ => None,
        })
    }

    /// Answers the last transition's peer-selection request with `peers`.
    ///
    /// # Panics
    ///
    /// Panics when the last transition did not request a selection.
    pub fn supply_peers(&mut self, peers: &[&str]) -> &mut Self {
        let request = self
            .selection()
            .expect("expected the transition to request peers");
        self.apply(crate::message::Message::Outcome(
            crate::message::EffectOutcome::PeersSelected {
                request,
                peers: peers.iter().map(|id| NodeId::new(*id).unwrap()).collect(),
            },
        ))
    }
}
