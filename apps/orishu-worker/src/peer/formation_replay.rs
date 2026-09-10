//! Deterministic diagnostic replay, not a transport benchmark or acceptance gate.
//!
//! Arrange the post-admission chain snapshots through the real admission core.
//! TLS facts and scheduling are fixtures; session binding, peer bytes, gossip,
//! SWIM, timers and anti-entropy use production code. No wall-clock sleeps.
use super::AuthenticatedSession;
use crate::{
    driver::Generation,
    peer::{handshake, wire},
};
use orishu_membership::{
    AdmissionEvidence, Command, Destination, Effect, EffectOutcome, Liveness, Membership, Message,
    NodeId, OutboundBody, PeerBody, PeerInput, SelectionPurpose, SenderIdentity, SessionId,
    TimerToken, testing,
};
use std::collections::{BTreeMap, VecDeque};

struct Node {
    model: Option<Membership>,
    timers: BTreeMap<TimerToken, u64>,
    sessions: BTreeMap<usize, AuthenticatedSession>,
    dial_after: Option<NodeId>,
    reconciliation: crate::reconciliation::Reconciliation,
}

impl Node {
    fn model(&self) -> &Membership {
        self.model.as_ref().unwrap()
    }
}

#[derive(Default, Debug, PartialEq, Eq, serde::Serialize)]
struct Counts {
    transitions: usize,
    missing_routes: usize,
    handshake_failures: usize,
    wire_omissions: usize,
    sent: usize,
    rejected: usize,
    sent_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Experiment {
    Baseline,
    ConnectedProbes,
    QueuedReconciliation,
}

struct Replay {
    nodes: Vec<Node>,
    ids: BTreeMap<NodeId, usize>,
    pending: VecDeque<(usize, Message)>,
    random: u64,
    now: u64,
    epoch: tokio::time::Instant,
    counts: Counts,
    // Counterfactual only. It would hide real failure detection if deployed.
    experiment: Experiment,
}

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

impl Replay {
    fn chain(count: usize, seed: u64, experiment: Experiment) -> Self {
        assert!(matches!(count, 3 | 10 | 30));
        let mut entropy = seed;
        let mut nodes = Vec::<Node>::new();
        for role in 0..count {
            let id = format!("{:064x}", random(&mut entropy));
            let mut local = testing::local_identity(&id);
            local.cert_fingerprint = testing::fingerprint(role as u8);
            local.capabilities = Default::default();
            local.capabilities.cpu_cores = 20;
            local.capabilities.architecture = "x86_64".into();
            local.endpoints.peers[0].0 = format!("127.0.0.1:{}", 10000 + role);
            let base = testing::standalone(&id);
            let mut model = Membership::standalone(
                orishu_membership::FormationId::new(format!("{seed:064x}")).unwrap(),
                base.cluster_name().clone(),
                local.clone(),
                base.policy().clone(),
                base.limits().clone(),
            )
            .unwrap();
            if role > 0 {
                let mut issuer = testing::Driver::new(nodes[role - 1].model.take().unwrap());
                let session = SessionId(role as u64);
                let mut context = testing::applicant_context(
                    issuer.model().formation(),
                    local.worker_name.as_str(),
                    session,
                    role as u8,
                );
                context.cert_fingerprint = local.cert_fingerprint;
                issuer.apply(Message::Peer(PeerInput {
                    context,
                    body: PeerBody::JoinRequest {
                        endpoints: local.endpoints.clone(),
                        accepts: local.accepts,
                        capacity: local.capacity,
                        capabilities: local.capabilities.clone(),
                    },
                }));
                let request = issuer
                    .effects
                    .iter()
                    .find_map(|e| match e {
                        Effect::VerifyCredential { request, .. } => Some(*request),
                        _ => None,
                    })
                    .unwrap();
                issuer.apply(Message::Outcome(EffectOutcome::CredentialVerified {
                    request,
                    session,
                    evidence: AdmissionEvidence {
                        transport_authenticated: true,
                        token_valid: true,
                        source_network_blocked: false,
                    },
                }));
                let request = issuer
                    .effects
                    .iter()
                    .find_map(|e| match e {
                        Effect::AllocateNodeId { request, .. } => Some(*request),
                        _ => None,
                    })
                    .unwrap();
                issuer.apply(Message::Outcome(EffectOutcome::NodeIdAllocated {
                    request,
                    session,
                    node_id: local.node_id.clone(),
                }));
                let snapshot = issuer
                    .effects
                    .iter()
                    .find_map(|e| match e {
                        Effect::Send {
                            message:
                                orishu_membership::OutboundMessage {
                                    body: OutboundBody::JoinAccepted { snapshot, .. },
                                    ..
                                },
                            ..
                        } => Some(snapshot),
                        _ => None,
                    })
                    .unwrap();
                for member in snapshot {
                    testing::insert_member(&mut model, member.clone());
                }
                nodes[role - 1].model = Some(issuer.model().clone());
            }
            nodes.push(Node {
                model: Some(model),
                timers: BTreeMap::new(),
                sessions: BTreeMap::new(),
                dial_after: None,
                reconciliation: Default::default(),
            });
        }
        let ids = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.model().local_id().clone(), i))
            .collect();
        let mut replay = Self {
            nodes,
            ids,
            pending: VecDeque::new(),
            random: entropy,
            now: 0,
            epoch: tokio::time::Instant::now(),
            counts: Counts::default(),
            experiment,
        };
        // Catch-up has already established each issuer/joiner edge. Other
        // connections still require mutual knowledge and the real handshake.
        for role in 1..count {
            assert!(replay.connect(role - 1, role));
        }
        replay
    }

    fn binding(&self, receiver: usize, sender: usize) -> Option<AuthenticatedSession> {
        let model = self.nodes[receiver].model();
        let local = self.nodes[sender].model().local();
        let mut session = AuthenticatedSession {
            id: SessionId(sender as u64 + 100),
            generation: Generation(0),
            formation: model.formation().clone(),
            fingerprint: local.cert_fingerprint,
            sender: None,
            introducer_target: None,
        };
        let bytes = handshake::request(local, model.formation().clone(), false).unwrap();
        handshake::accept_request(&bytes, &mut session, model, Generation(0)).ok()?;
        Some(session)
    }

    fn connect(&mut self, a: usize, b: usize) -> bool {
        let (Some(at_a), Some(at_b)) = (self.binding(a, b), self.binding(b, a)) else {
            self.counts.handshake_failures += 1;
            return false;
        };
        self.nodes[a].sessions.insert(b, at_a);
        self.nodes[b].sessions.insert(a, at_b);
        true
    }

    fn connected(&self, a: usize, b: usize) -> bool {
        self.nodes[a]
            .sessions
            .get(&b)
            .is_some_and(|s| s.context(self.nodes[a].model(), Generation(0)).is_ok())
            && self.nodes[b]
                .sessions
                .get(&a)
                .is_some_and(|s| s.context(self.nodes[b].model(), Generation(0)).is_ok())
    }

    fn dial(&mut self, role: usize) {
        use std::ops::Bound::{Excluded, Unbounded};
        // Mirrors the ordinary member scan's canonical direction and four
        // attempts/tick. TLS completes instantly here; no capacity contention.
        for _ in 0..4 {
            let model = self.nodes[role].model();
            let mut after = None;
            let mut target = None;
            for (id, member) in model
                .members()
                .range((
                    self.nodes[role]
                        .dial_after
                        .as_ref()
                        .map_or(Unbounded, Excluded),
                    Unbounded,
                ))
                .take(64)
            {
                after = Some(id.clone());
                let peer = self.ids[id];
                if id > model.local_id()
                    && matches!(member.liveness, Liveness::Alive | Liveness::Suspected)
                    && !self.connected(role, peer)
                {
                    target = Some(peer);
                    break;
                }
            }
            self.nodes[role].dial_after = after;
            let Some(peer) = target else {
                break;
            };
            self.connect(role, peer);
        }
    }

    fn apply(&mut self, role: usize, message: Message) {
        self.pending.push_back((role, message));
        while let Some((role, message)) = self.pending.pop_front() {
            self.counts.transitions += 1;
            assert!(
                self.counts.transitions < 200_000,
                "bounded replay transition budget"
            );
            let transition =
                orishu_membership::update(self.nodes[role].model.take().unwrap(), message);
            self.nodes[role].model = Some(transition.model);
            // Production registry synchronization closes invalid bindings;
            // later refutation must not resurrect a previously closed session.
            let invalid: Vec<_> = self.nodes[role]
                .sessions
                .iter()
                .filter_map(|(peer, session)| {
                    session
                        .context(self.nodes[role].model(), Generation(0))
                        .is_err()
                        .then_some(*peer)
                })
                .collect();
            for peer in invalid {
                self.nodes[role].sessions.remove(&peer);
                self.nodes[peer].sessions.remove(&role);
            }
            for effect in transition.effects {
                match effect {
                    Effect::ArmTimer { token, delay } => {
                        self.nodes[role].timers.insert(token, self.now + delay.0);
                    }
                    Effect::CancelTimer { token } => {
                        self.nodes[role].timers.remove(&token);
                    }
                    Effect::SelectPeers {
                        request,
                        purpose,
                        count,
                        exclude,
                    } => {
                        let model = self.nodes[role].model();
                        let restricted = matches!(purpose, SelectionPurpose::AntiEntropy)
                            || (self.experiment == Experiment::ConnectedProbes
                                && matches!(purpose, SelectionPurpose::DirectProbe));
                        let mut candidates = Vec::new();
                        for member in model.members().values() {
                            let peer = self.ids[&member.id];
                            if peer != role
                                && matches!(member.liveness, Liveness::Alive | Liveness::Suspected)
                                && !exclude.contains(&member.id)
                                && (!restricted || self.connected(role, peer))
                            {
                                candidates.push((random(&mut self.random), member.id.clone()));
                            }
                        }
                        candidates.sort_unstable();
                        self.pending.push_back((
                            role,
                            Message::Outcome(EffectOutcome::PeersSelected {
                                request,
                                peers: candidates
                                    .into_iter()
                                    .take(count)
                                    .map(|(_, id)| id)
                                    .collect(),
                            }),
                        ));
                    }
                    Effect::Send {
                        destination: Destination::Member(id),
                        mut message,
                    } => {
                        let peer = self.ids[&id];
                        if !self.connected(role, peer) {
                            self.counts.missing_routes += 1;
                            if !message.gossip.is_empty() {
                                self.pending.push_back((
                                    role,
                                    Message::Outcome(EffectOutcome::GossipDeferred {
                                        deltas: std::mem::take(&mut message.gossip),
                                    }),
                                ));
                            }
                            continue;
                        }
                        let model = self.nodes[role].model();
                        let (packet, deferred) = wire::encode_member(
                            message,
                            model.formation().clone(),
                            SenderIdentity::Admitted(model.local_id().clone()),
                        )
                        .unwrap();
                        self.counts.wire_omissions += packet.deferred_gossip;
                        if !deferred.is_empty() {
                            self.pending.push_back((
                                role,
                                Message::Outcome(EffectOutcome::GossipDeferred {
                                    deltas: deferred,
                                }),
                            ));
                        }
                        let decoded = self.nodes[peer].sessions[&role].decode(
                            &packet.bytes,
                            self.nodes[peer].model(),
                            Generation(0),
                            packet.transport,
                        );
                        match decoded {
                            Ok(decoded) => {
                                self.counts.sent += 1;
                                self.counts.sent_bytes += packet.bytes.len();
                                self.pending.push_back((peer, Message::Peer(decoded.input)));
                            }
                            Err(_) => self.counts.rejected += 1,
                        }
                    }
                    Effect::Publish(_) => {}
                    unexpected => panic!("unexpected steady-state replay effect: {unexpected:?}"),
                }
            }
        }
    }

    fn settled(&self) -> bool {
        self.nodes.iter().all(|n| {
            n.model().members().len() == self.nodes.len()
                && n.model()
                    .members()
                    .values()
                    .all(|m| m.liveness == Liveness::Alive)
        })
    }

    fn run_until(&mut self, predicate: impl Fn(&Self) -> bool) -> Option<u64> {
        for now in (self.now..=60_000).step_by(50) {
            self.now = now;
            for role in 0..self.nodes.len() {
                let due: Vec<_> = self.nodes[role]
                    .timers
                    .iter()
                    .filter_map(|(t, d)| (*d <= now).then_some(*t))
                    .collect();
                for timer in due {
                    self.nodes[role].timers.remove(&timer);
                    self.apply(role, Message::Timer(timer));
                }
                if now % 1000 == 0 {
                    self.dial(role);
                    self.apply(role, Message::Local(Command::StartProbeRound));
                }
                let queued = !self.nodes[role].model().gossip().is_empty();
                let active = self.nodes[role].model().anti_entropy().is_some();
                let due = if self.experiment == Experiment::QueuedReconciliation {
                    now % 1000 == 0
                        && self.nodes[role].reconciliation.due(
                            self.epoch + std::time::Duration::from_millis(now),
                            queued,
                            active,
                        )
                } else {
                    now % 5000 == 0
                };
                if due {
                    self.apply(role, Message::Local(Command::StartAntiEntropyRound));
                }
            }
            if predicate(self) {
                self.now += 50;
                return Some(now);
            }
        }
        None
    }
}

#[test]
fn sparse_chain_repairs_news_without_suppressing_disconnected_probes() {
    let mut baseline = Replay::chain(10, 11, Experiment::Baseline);
    let baseline_time = baseline.run_until(Replay::settled).unwrap();
    let mut candidate = Replay::chain(10, 11, Experiment::QueuedReconciliation);
    let candidate_time = candidate.run_until(Replay::settled).unwrap();
    assert!(
        candidate_time < baseline_time,
        "queued news must not wait solely for idle reconciliation"
    );
    assert!(
        candidate.counts.missing_routes > 0,
        "ordinary disconnected SWIM candidates remain eligible"
    );
    assert!(
        candidate.counts.handshake_failures > 0,
        "unknown members never bypass handshake binding"
    );
    for (role, locked) in [(0, true), (9, false)] {
        candidate.apply(role, Message::Local(Command::SetMembershipLock(locked)));
        assert!(
            candidate
                .run_until(|r| r.settled()
                    && r.nodes
                        .iter()
                        .all(|n| n.model().membership_locked() == locked))
                .is_some()
        );
    }
}

#[test]
#[ignore = "bounded diagnostic replay; not a performance acceptance gate"]
fn chain_dissemination_diagnostic() {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut report = std::env::var_os("ORISHU_FORMATION_REPLAY_REPORT").map(|path| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        options.open(path).unwrap()
    });
    for size in [3, 10, 30] {
        for seed in [11, 29, 47] {
            for experiment in [
                Experiment::Baseline,
                Experiment::ConnectedProbes,
                Experiment::QueuedReconciliation,
            ] {
                let mut replay = Replay::chain(size, seed, experiment);
                let settled = replay.run_until(Replay::settled);
                let mut policy = Vec::new();
                if settled.is_some() {
                    for (role, locked) in [(0, true), (size - 1, false)] {
                        replay.apply(role, Message::Local(Command::SetMembershipLock(locked)));
                        let observed = replay.run_until(|r| {
                            r.settled()
                                && r.nodes
                                    .iter()
                                    .all(|n| n.model().membership_locked() == locked)
                        });
                        policy.push(observed);
                        if observed.is_none() {
                            break;
                        }
                    }
                }
                eprintln!(
                    "FORMATION_REPLAY size={size} seed={seed} experiment={experiment:?} settled_ms={settled:?} policy_ms={policy:?} counts={:?}",
                    replay.counts
                );
                if let Some(report) = report.as_mut() {
                    writeln!(
                        report,
                        "{}",
                        serde_json::json!({"schema_version":1, "size":size, "seed":seed,
                        "experiment":format!("{experiment:?}"), "settled_ms":settled,
                        "policy_ms":policy, "counts":replay.counts})
                    )
                    .unwrap();
                    report.flush().unwrap();
                }
            }
        }
    }
}
