//! The pure transition: `update(model, message) -> Transition`.
//!
//! Nothing in this module opens a socket, sleeps, reads a clock, generates a
//! random value, or spawns a task. Everything it needs from the outside world
//! arrives as a [`Message`] and leaves as an [`Effect`].

use std::collections::{BTreeMap, BTreeSet};

use orishu_identity::{
    ClusterName, FormationId, Incarnation, MembershipTombstone, NodeId, RemovalMode, VersionTuple,
};

use crate::{
    admission,
    antientropy::MembershipTree,
    effect::{
        AllocationId, Announcement, ChangeRecord, Destination, Diagnostic, Effect, ForeignHandoff,
        IndirectResult, OutboundBody, OutboundMessage, ProbeId, RejectReason, SelectionId,
        SessionId, TimerKind, TimerToken, Transition, VerificationId,
    },
    gossip::{DeltaBody, GossipDelta},
    merge::{self, MergeOutcome},
    message::{
        AdmissionEvidence, Command, EffectOutcome, JoinReply, Message, PeerBody, PeerContext,
        PeerInput, SelectionPurpose, SenderIdentity,
    },
    model::{
        AdmissionStage, AntiEntropyRound, JoinAttempt, Liveness, Member, Membership,
        PendingAdmission, PendingProbe, ProbePhase, Suspicion,
    },
};

/// Folds one message into the model, returning the next model plus bounded
/// effects and diagnostics.
///
/// The model is taken by value and a new one returned, which is what makes a
/// transition a value a test can assert on. Internally the owned model is
/// mutated in place: observable purity means the result depends only on the
/// inputs, not that a thousand-member map must be copied to change one entry.
#[must_use]
pub fn update(model: Membership, message: Message) -> Transition {
    let mut ctx = Context::new(model);
    match message {
        Message::Peer(input) => handle_peer(&mut ctx, input),
        Message::Local(command) => handle_command(&mut ctx, command),
        Message::Timer(token) => handle_timer(&mut ctx, token),
        Message::Outcome(outcome) => handle_outcome(&mut ctx, outcome),
    }
    ctx.finish()
}

/// Accumulates a transition while it is being built.
struct Context {
    model: Membership,
    effects: Vec<Effect>,
    diagnostics: Vec<Diagnostic>,
    foreign: Vec<ForeignHandoff>,
}

impl Context {
    fn new(model: Membership) -> Self {
        Self {
            model,
            effects: Vec::new(),
            diagnostics: Vec::new(),
            foreign: Vec::new(),
        }
    }

    fn emit(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    fn note(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Applies the per-update budgets.
    ///
    /// Bounded inputs should keep every transition well under these caps; the
    /// caps exist so that a decoder bug or an unforeseen fan-out degrades into
    /// a reported truncation rather than an unbounded burst of sends, timers,
    /// and allocations.
    fn finish(mut self) -> Transition {
        let effect_cap = self.model.limits().max_effects_per_update();
        if self.effects.len() > effect_cap {
            let produced = self.effects.len();
            self.effects.truncate(effect_cap);
            self.diagnostics.push(Diagnostic::EffectBudgetExceeded {
                produced,
                cap: effect_cap,
            });
        }

        let diagnostic_cap = self.model.limits().max_diagnostics_per_update();
        if self.diagnostics.len() > diagnostic_cap {
            self.diagnostics.truncate(diagnostic_cap.saturating_sub(1));
            self.diagnostics.push(Diagnostic::DiagnosticBudgetExceeded {
                cap: diagnostic_cap,
            });
        }

        Transition {
            model: self.model,
            effects: self.effects,
            diagnostics: self.diagnostics,
            foreign_gossip: self.foreign,
        }
    }

    /// Builds an outbound message, stamping it with the next sequence number
    /// and a batch of piggybacked gossip.
    fn outbound(&mut self, body: OutboundBody) -> OutboundMessage {
        let seq = self.model.next_seq();
        let count = self.model.limits().max_gossip_per_message();
        let hops = self
            .model
            .limits()
            .effective_gossip_hops(self.model.scale_size());
        let gossip = self.model.gossip_mut().take(count, hops);
        OutboundMessage { body, seq, gossip }
    }

    /// Builds an outbound message with no piggybacked gossip, for
    /// pre-admission traffic that crosses a formation boundary.
    fn outbound_bare(&mut self, body: OutboundBody) -> OutboundMessage {
        let seq = self.model.next_seq();
        OutboundMessage {
            body,
            seq,
            gossip: Vec::new(),
        }
    }

    fn send_to(&mut self, target: NodeId, body: OutboundBody) {
        let message = self.outbound(body);
        self.emit(Effect::Send {
            destination: Destination::Member(target),
            message,
        });
    }

    fn reply_to(&mut self, session: SessionId, body: OutboundBody) {
        let message = self.outbound_bare(body);
        self.emit(Effect::Send {
            destination: Destination::Session(session),
            message,
        });
    }

    /// Mints the next timer token of `kind`.
    fn timer(&mut self, kind: TimerKind) -> TimerToken {
        TimerToken {
            kind,
            generation: self.model.next_correlation(),
        }
    }

    fn arm(&mut self, token: TimerToken, delay: crate::limits::DurationMillis) {
        self.emit(Effect::ArmTimer { token, delay });
    }

    fn cancel(&mut self, token: TimerToken) {
        self.emit(Effect::CancelTimer { token });
    }

    /// Asks the shell for peers, recording why so the answer can be routed.
    fn request_peers(&mut self, purpose: SelectionPurpose, count: usize) {
        let limit = self.model.limits().max_pending_probes();
        if self.model.selections().len() >= limit {
            self.note(Diagnostic::LimitExceeded {
                limit: "maxPendingProbes",
                value: self.model.selections().len() + 1,
                max: limit,
            });
            return;
        }
        let request = SelectionId(self.model.next_correlation());
        self.model.selections_mut().insert(request, purpose.clone());
        let exclude = vec![self.model.local_id().clone()];
        self.emit(Effect::SelectPeers {
            request,
            purpose,
            count,
            exclude,
        });
    }
}

// ── Peer input ───────────────────────────────────────────────────────────

fn handle_peer(ctx: &mut Context, input: PeerInput) {
    let PeerInput { context, body } = input;

    if !context.authenticated {
        ctx.note(Diagnostic::Unauthenticated);
        return;
    }

    // Pre-admission traffic is handled before the formation and sender-binding
    // checks, because by definition it has neither an assigned ID nor (for a
    // reply) our own formation.
    match body {
        PeerBody::JoinRequest {
            endpoints,
            accepts,
            capacity,
            capabilities,
        } => {
            handle_join_request(ctx, context, endpoints, accepts, capacity, capabilities);
            return;
        }
        PeerBody::JoinReply(reply) => {
            handle_join_reply(ctx, context, reply);
            return;
        }
        _ => {}
    }

    // Every post-admission input is re-checked here. A correct decoder rejects
    // these earlier, but correctness must not depend on every future adapter
    // remembering an undocumented precondition.
    if context.formation != *ctx.model.formation() {
        ctx.note(Diagnostic::FormationMismatch {
            expected: ctx.model.formation().clone(),
            received: context.formation,
        });
        return;
    }

    let SenderIdentity::Admitted(sender) = context.sender.clone() else {
        ctx.note(Diagnostic::Unexpected {
            what: "post-admission message from an unadmitted sender",
        });
        return;
    };

    let Some(member) = ctx.model.member(&sender) else {
        ctx.note(Diagnostic::UnknownSender { claimed: sender });
        return;
    };
    if member.cert_fingerprint != context.cert_fingerprint {
        ctx.note(Diagnostic::CertificateMismatch { node: sender });
        return;
    }

    absorb_gossip(ctx, Some(&sender), context.gossip.clone());

    let replayed = ctx.model.observe_seq(&sender, context.seq);
    if replayed && is_stream_carried(&body) {
        // Datagram probe traffic is correlated by probe ID and ordered by
        // incarnation, so a reordered `Ack` must still be honoured; a repeated
        // stream message is a genuine replay of a non-idempotent exchange.
        ctx.note(Diagnostic::ReplayedSequence {
            node: sender,
            seq: context.seq,
        });
        return;
    }

    match body {
        PeerBody::Ping { probe, incarnation } => {
            observe_alive(ctx, &sender, incarnation);
            let self_incarnation = ctx.model.incarnation();
            ctx.send_to(
                sender,
                OutboundBody::Ack {
                    probe,
                    incarnation: self_incarnation,
                },
            );
        }
        PeerBody::Ack { probe, incarnation } => handle_ack(ctx, &sender, probe, incarnation),
        PeerBody::PingReq { probe, target } => handle_ping_req(ctx, &sender, probe, target),
        PeerBody::PingReply {
            probe,
            target,
            result,
        } => handle_ping_reply(ctx, &sender, probe, &target, result),
        PeerBody::Announce {
            announcement,
            target,
            incarnation,
        } => {
            let outcome = merge::apply_announcement(
                &mut ctx.model,
                &sender,
                announcement,
                &target,
                incarnation,
            );
            absorb_outcome(ctx, outcome);
        }
        PeerBody::PullRequest {
            round,
            digest,
            buckets,
            cursor,
        } => handle_pull_request(ctx, &sender, round, &digest, &buckets, cursor.as_ref()),
        PeerBody::PullReply {
            round,
            digest: _,
            deltas,
            complete,
            cursor,
        } => handle_pull_reply(ctx, &sender, round, deltas, complete, cursor),
        PeerBody::JoinRequest { .. } | PeerBody::JoinReply(_) => unreachable!("handled above"),
    }
}

/// Whether a body travels on a QUIC stream, where an exact sequence replay is
/// a replay of a non-idempotent exchange rather than reordering.
fn is_stream_carried(body: &PeerBody) -> bool {
    matches!(
        body,
        PeerBody::JoinRequest { .. }
            | PeerBody::JoinReply(_)
            | PeerBody::PullRequest { .. }
            | PeerBody::PullReply { .. }
    )
}

/// Merges piggybacked gossip, whatever carried it.
fn absorb_gossip(ctx: &mut Context, from: Option<&NodeId>, deltas: Vec<GossipDelta>) {
    let limit = ctx.model.limits().max_gossip_per_inbound_message();
    if deltas.len() > limit {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "maxGossipPerInboundMessage",
            value: deltas.len(),
            max: limit,
        });
    }
    let max_hops = ctx
        .model
        .limits()
        .effective_gossip_hops(ctx.model.scale_size());

    for delta in deltas.into_iter().take(limit) {
        if delta.hops > max_hops.saturating_mul(2) {
            // A hop count far beyond retirement means the delta has been
            // circulating in a loop or was fabricated; either way it is not
            // news.
            ctx.note(Diagnostic::StaleDelta {
                entity: delta.body.entity(),
            });
            continue;
        }
        match delta.body {
            DeltaBody::Foreign(foreign) => ctx.foreign.push(ForeignHandoff {
                from: from.cloned(),
                delta: foreign,
            }),
            body => {
                let outcome = merge::merge_delta(&mut ctx.model, body);
                absorb_outcome(ctx, outcome);
            }
        }
    }
}

/// Turns a merge result into queued gossip, published changes, and
/// diagnostics.
fn absorb_outcome(ctx: &mut Context, outcome: MergeOutcome) {
    match outcome {
        MergeOutcome::Adopted { body, change } => {
            let limits = ctx.model.limits().clone();
            ctx.model.gossip_mut().enqueue(body, &limits);
            if let Some(change) = change {
                ctx.emit(Effect::Publish(change));
            }
        }
        MergeOutcome::Idempotent => {}
        MergeOutcome::Rejected(diagnostic) => ctx.note(diagnostic),
        MergeOutcome::SelfChallenged { incarnation } => refute(ctx, incarnation),
        MergeOutcome::SelfRemoved { tombstone } => {
            ctx.emit(Effect::Publish(ChangeRecord::MemberRemoved {
                node: tombstone.node_id,
                mode: tombstone.removal_mode,
            }));
        }
    }
}

/// Refutes a suspicion of this node by advancing its own incarnation.
///
/// Only the subject may raise its own incarnation, which is what makes the
/// refutation authoritative: no peer can manufacture one.
fn refute(ctx: &mut Context, challenged: Incarnation) {
    let current = ctx.model.incarnation().max(challenged);
    let Some(next) = current.checked_next() else {
        // Refusing to wrap is the correct failure: an incarnation of `0` would
        // rank below every stale `Suspect` still in flight, making the node
        // permanently unable to defend itself.
        ctx.note(Diagnostic::IncarnationExhausted);
        return;
    };

    ctx.model.set_incarnation(next);
    let local = ctx.model.local_id().clone();
    let updated = {
        let Some(member) = ctx.model.members_mut().get_mut(&local) else {
            return;
        };
        let from = member.liveness;
        member.liveness = Liveness::Alive;
        member.incarnation = next;
        if let Some(version) = member.version.checked_successor(local.clone()) {
            member.version = version;
        }
        (member.clone(), from)
    };

    let limits = ctx.model.limits().clone();
    ctx.model
        .gossip_mut()
        .enqueue(DeltaBody::MembershipUpdate(updated.0), &limits);
    ctx.emit(Effect::Publish(ChangeRecord::LivenessChanged {
        node: local.clone(),
        from: updated.1,
        to: Liveness::Alive,
        incarnation: next,
    }));

    let fan_out = ctx.model.limits().indirect_probe_count();
    ctx.request_peers(
        SelectionPurpose::Announce {
            announcement: Announcement::Alive,
            target: local,
            incarnation: next,
        },
        fan_out,
    );
}

/// Records that `sender` is alive at `incarnation`.
fn observe_alive(ctx: &mut Context, sender: &NodeId, incarnation: Incarnation) {
    let outcome = merge::apply_announcement(
        &mut ctx.model,
        sender,
        Announcement::Alive,
        sender,
        incarnation,
    );
    match outcome {
        // "Nothing newer than what we hold" is the common case for a healthy
        // peer, not a problem worth a diagnostic.
        MergeOutcome::Rejected(Diagnostic::StaleDelta { .. }) => {}
        other => absorb_outcome(ctx, other),
    }
}

// ── SWIM probing ─────────────────────────────────────────────────────────

fn handle_ack(ctx: &mut Context, sender: &NodeId, probe: ProbeId, incarnation: Incarnation) {
    let Some(pending) = ctx.model.probes().get(&probe).cloned() else {
        ctx.note(Diagnostic::UnknownProbe { probe });
        return;
    };
    if &pending.target != sender {
        ctx.note(Diagnostic::ProbeMismatch {
            probe,
            reason: "acknowledgement came from a node other than the probe target",
        });
        return;
    }

    ctx.model.probes_mut().remove(&probe);
    ctx.cancel(pending.timer);
    observe_alive(ctx, sender, incarnation);

    if let ProbePhase::Relay {
        requester,
        requester_probe,
    } = pending.phase
    {
        ctx.send_to(
            requester,
            OutboundBody::PingReply {
                probe: requester_probe,
                target: sender.clone(),
                result: IndirectResult::Ack(incarnation),
            },
        );
    }
}

fn handle_ping_req(ctx: &mut Context, requester: &NodeId, probe: ProbeId, target: NodeId) {
    if &target == ctx.model.local_id() {
        let incarnation = ctx.model.incarnation();
        ctx.send_to(
            requester.clone(),
            OutboundBody::PingReply {
                probe,
                target,
                result: IndirectResult::Ack(incarnation),
            },
        );
        return;
    }

    if ctx.model.member(&target).is_none() || ctx.model.is_tombstoned(&target) {
        ctx.send_to(
            requester.clone(),
            OutboundBody::PingReply {
                probe,
                target,
                result: IndirectResult::NoSuchPeer,
            },
        );
        return;
    }

    let limit = ctx.model.limits().max_pending_probes();
    if ctx.model.probes().len() >= limit {
        // Refusing the relay is better than unbounded growth: the requester
        // asked several intermediaries and treats silence as one failure.
        ctx.note(Diagnostic::LimitExceeded {
            limit: "maxPendingProbes",
            value: ctx.model.probes().len() + 1,
            max: limit,
        });
        ctx.send_to(
            requester.clone(),
            OutboundBody::PingReply {
                probe,
                target,
                result: IndirectResult::NoSuchPeer,
            },
        );
        return;
    }

    let relay_probe = ProbeId(ctx.model.next_correlation());
    let timer = ctx.timer(TimerKind::DirectProbe);
    let target_incarnation = ctx
        .model
        .member(&target)
        .map_or(Incarnation::INITIAL, |member| member.incarnation);
    ctx.model.probes_mut().insert(
        relay_probe,
        PendingProbe {
            id: relay_probe,
            target: target.clone(),
            target_incarnation,
            phase: ProbePhase::Relay {
                requester: requester.clone(),
                requester_probe: probe,
            },
            timer,
        },
    );
    let incarnation = ctx.model.incarnation();
    ctx.send_to(
        target,
        OutboundBody::Ping {
            probe: relay_probe,
            incarnation,
        },
    );
    let delay = ctx.model.limits().probe_timeout();
    ctx.arm(timer, delay);
}

fn handle_ping_reply(
    ctx: &mut Context,
    sender: &NodeId,
    probe: ProbeId,
    target: &NodeId,
    result: IndirectResult,
) {
    let Some(pending) = ctx.model.probes().get(&probe).cloned() else {
        ctx.note(Diagnostic::UnknownProbe { probe });
        return;
    };
    if &pending.target != target {
        ctx.note(Diagnostic::ProbeMismatch {
            probe,
            reason: "reply names a different target than the probe",
        });
        return;
    }
    let ProbePhase::Indirect { helpers, answered } = pending.phase.clone() else {
        ctx.note(Diagnostic::ProbeMismatch {
            probe,
            reason: "reply arrived for a probe that is not in its indirect phase",
        });
        return;
    };
    if !helpers.contains(sender) {
        ctx.note(Diagnostic::ProbeMismatch {
            probe,
            reason: "reply came from a node that was not asked to probe",
        });
        return;
    }

    if let IndirectResult::Ack(incarnation) = result {
        ctx.model.probes_mut().remove(&probe);
        ctx.cancel(pending.timer);
        observe_alive(ctx, target, incarnation);
        return;
    }

    let mut answered = answered;
    // A duplicate reply from the same intermediary must not count twice, or
    // one chatty helper could push the probe to suspicion on its own.
    if !answered.insert(sender.clone()) {
        return;
    }
    let everyone_failed = answered.len() >= helpers.len();
    if let Some(entry) = ctx.model.probes_mut().get_mut(&probe) {
        entry.phase = ProbePhase::Indirect { helpers, answered };
    }

    if everyone_failed {
        ctx.model.probes_mut().remove(&probe);
        ctx.cancel(pending.timer);
        suspect(ctx, target.clone(), pending.target_incarnation);
    }
}

/// Moves a member into suspicion and arms its refutation deadline.
fn suspect(ctx: &mut Context, target: NodeId, incarnation: Incarnation) {
    let local = ctx.model.local_id().clone();
    let outcome = merge::apply_announcement(
        &mut ctx.model,
        &local,
        Announcement::Suspect,
        &target,
        incarnation,
    );
    let adopted = matches!(outcome, MergeOutcome::Adopted { .. });
    absorb_outcome(ctx, outcome);
    if !adopted {
        return;
    }

    let timer = ctx.timer(TimerKind::Suspicion);
    ctx.model
        .suspicions_mut()
        .insert(target.clone(), Suspicion { incarnation, timer });
    let delay = ctx.model.suspicion_timeout();
    ctx.arm(timer, delay);

    let fan_out = ctx.model.limits().indirect_probe_count();
    ctx.request_peers(
        SelectionPurpose::Announce {
            announcement: Announcement::Suspect,
            target,
            incarnation,
        },
        fan_out,
    );
}

/// Declares a suspected member dead after its deadline passed unrefuted.
fn declare_dead(ctx: &mut Context, target: NodeId, incarnation: Incarnation) {
    let local = ctx.model.local_id().clone();
    let outcome = merge::apply_announcement(
        &mut ctx.model,
        &local,
        Announcement::Dead,
        &target,
        incarnation,
    );
    let adopted = matches!(outcome, MergeOutcome::Adopted { .. });
    absorb_outcome(ctx, outcome);
    if !adopted {
        return;
    }

    // Declaring death does not remove the member and writes no tombstone.
    // Removal is an operator decision; a failure detector must not be able to
    // make one permanently.
    let fan_out = ctx.model.limits().indirect_probe_count();
    ctx.request_peers(
        SelectionPurpose::Announce {
            announcement: Announcement::Dead,
            target,
            incarnation,
        },
        fan_out,
    );
}

// ── Timers ───────────────────────────────────────────────────────────────

fn handle_timer(ctx: &mut Context, token: TimerToken) {
    match token.kind {
        TimerKind::DirectProbe | TimerKind::IndirectProbe => handle_probe_timer(ctx, token),
        TimerKind::Suspicion => handle_suspicion_timer(ctx, token),
        TimerKind::AntiEntropy => handle_anti_entropy_timer(ctx, token),
        TimerKind::JoinRetry => handle_join_timer(ctx, token),
    }
}

fn handle_probe_timer(ctx: &mut Context, token: TimerToken) {
    let Some(pending) = ctx
        .model
        .probes()
        .values()
        .find(|probe| probe.timer == token)
        .cloned()
    else {
        ctx.note(Diagnostic::StaleTimer { token });
        return;
    };

    match pending.phase {
        ProbePhase::Direct => {
            // Escalate. The new timer is armed now rather than after the peer
            // selection returns, so a shell that never answers cannot leave
            // the probe pending forever.
            let timer = ctx.timer(TimerKind::IndirectProbe);
            if let Some(entry) = ctx.model.probes_mut().get_mut(&pending.id) {
                entry.timer = timer;
                entry.phase = ProbePhase::Indirect {
                    helpers: Vec::new(),
                    answered: BTreeSet::new(),
                };
            }
            let delay = ctx.model.limits().indirect_probe_timeout();
            ctx.arm(timer, delay);
            let count = ctx.model.limits().indirect_probe_count();
            ctx.request_peers(SelectionPurpose::IndirectProbe { probe: pending.id }, count);
        }
        ProbePhase::Indirect { .. } => {
            ctx.model.probes_mut().remove(&pending.id);
            suspect(ctx, pending.target, pending.target_incarnation);
        }
        ProbePhase::Relay {
            requester,
            requester_probe,
        } => {
            ctx.model.probes_mut().remove(&pending.id);
            ctx.send_to(
                requester,
                OutboundBody::PingReply {
                    probe: requester_probe,
                    target: pending.target,
                    result: IndirectResult::Timeout,
                },
            );
        }
    }
}

fn handle_suspicion_timer(ctx: &mut Context, token: TimerToken) {
    let Some((target, suspicion)) = ctx
        .model
        .suspicions()
        .iter()
        .find(|(_, suspicion)| suspicion.timer == token)
        .map(|(target, suspicion)| (target.clone(), suspicion.clone()))
    else {
        ctx.note(Diagnostic::StaleTimer { token });
        return;
    };
    ctx.model.suspicions_mut().remove(&target);
    declare_dead(ctx, target, suspicion.incarnation);
}

fn handle_anti_entropy_timer(ctx: &mut Context, token: TimerToken) {
    let Some(round) = ctx
        .model
        .anti_entropy()
        .cloned()
        .filter(|r| r.timer == token)
    else {
        ctx.note(Diagnostic::StaleTimer { token });
        return;
    };
    ctx.model.set_anti_entropy(None);
    ctx.note(Diagnostic::AntiEntropyAbandoned {
        peer: round.peer,
        exchanges: round.exchanges,
    });
}

fn handle_join_timer(ctx: &mut Context, token: TimerToken) {
    let Some(attempt) = ctx
        .model
        .join_attempt()
        .cloned()
        .filter(|a| a.timer == token)
    else {
        ctx.note(Diagnostic::StaleTimer { token });
        return;
    };
    retry_join(ctx, attempt);
}

fn retry_join(ctx: &mut Context, attempt: JoinAttempt) {
    let next = attempt.attempt.saturating_add(1);
    if next > ctx.model.limits().max_join_attempts() {
        ctx.model.set_join_attempt(None);
        ctx.note(Diagnostic::JoinAbandoned {
            attempts: attempt.attempt,
        });
        return;
    }

    let timer = ctx.timer(TimerKind::JoinRetry);
    let body = join_request_body(&ctx.model);
    ctx.model.set_join_attempt(Some(JoinAttempt {
        attempt: next,
        timer,
        ..attempt.clone()
    }));
    ctx.reply_to(attempt.session, body);
    let delay = ctx.model.limits().join_backoff_for(next);
    ctx.arm(timer, delay);
}

fn join_request_body(model: &Membership) -> OutboundBody {
    let local = model.local();
    OutboundBody::JoinRequest {
        name: local.worker_name.clone(),
        cert_fingerprint: local.cert_fingerprint,
        protocol: local.protocol,
        endpoints: local.endpoints.clone(),
        accepts: local.accepts,
        capacity: local.capacity,
        capabilities: local.capabilities.clone(),
    }
}

// ── Admission, introducer side ───────────────────────────────────────────

fn handle_join_request(
    ctx: &mut Context,
    context: PeerContext,
    endpoints: crate::model::Endpoints,
    accepts: crate::model::Accepts,
    capacity: crate::model::NodeCapacity,
    capabilities: crate::model::Capabilities,
) {
    let SenderIdentity::Applicant(name) = context.sender.clone() else {
        ctx.note(Diagnostic::Unexpected {
            what: "join request claiming an already-assigned node ID",
        });
        return;
    };

    let formation_matches = context.formation == *ctx.model.formation();
    if let Err(reason) = admission::evaluate_local_gates(
        &ctx.model,
        context.authenticated,
        formation_matches,
        context.protocol,
        &name,
        context.cert_fingerprint,
    ) {
        refuse_admission(ctx, context.session, name, reason);
        return;
    }

    // Bounds are checked before anything is stored, so an oversized
    // description cannot occupy a pending slot while the shell verifies a
    // token.
    let candidate = Member {
        id: ctx.model.local_id().clone(), // placeholder, never stored
        name: name.clone(),
        cert_fingerprint: context.cert_fingerprint,
        protocol: context.protocol,
        liveness: Liveness::Alive,
        incarnation: Incarnation::INITIAL,
        version: VersionTuple::initial(0, ctx.model.local_id().clone()),
        endpoints: endpoints.clone(),
        accepts,
        capacity,
        capabilities: capabilities.clone(),
    };
    if let Err(error) = candidate.validate(ctx.model.limits()) {
        refuse_admission(
            ctx,
            context.session,
            name,
            RejectReason::MalformedRequest {
                field: error.to_string(),
            },
        );
        return;
    }

    let limit = ctx.model.limits().max_pending_joins();
    if ctx.model.admissions().len() >= limit {
        refuse_admission(ctx, context.session, name, RejectReason::Overloaded);
        return;
    }

    let request = VerificationId(ctx.model.next_correlation());
    ctx.model.admissions_mut().insert(
        context.session,
        PendingAdmission {
            session: context.session,
            name: name.clone(),
            cert_fingerprint: context.cert_fingerprint,
            protocol: context.protocol,
            endpoints,
            accepts,
            capacity,
            capabilities,
            stage: AdmissionStage::AwaitingEvidence { request },
        },
    );

    let blocked_networks = admission::blocked_networks(&ctx.model);
    ctx.emit(Effect::VerifyCredential {
        request,
        session: context.session,
        kind: crate::effect::CredentialKind::JoinToken {
            name,
            cert_fingerprint: context.cert_fingerprint,
            blocked_networks,
        },
    });
}

/// Refuses an applicant, redirecting instead when the only problem is that
/// this node is full.
fn refuse_admission(
    ctx: &mut Context,
    session: SessionId,
    name: orishu_identity::WorkerName,
    reason: RejectReason,
) {
    ctx.model.admissions_mut().remove(&session);

    if reason == RejectReason::CapacityExhausted {
        let candidates = admission::redirect_candidates(&ctx.model);
        if !candidates.is_empty() {
            ctx.reply_to(session, OutboundBody::JoinRedirect { candidates });
            ctx.emit(Effect::Publish(ChangeRecord::AdmissionRejected {
                name,
                reason,
            }));
            return;
        }
    }

    ctx.reply_to(
        session,
        OutboundBody::JoinRejected {
            reason: reason.clone(),
        },
    );
    ctx.emit(Effect::Publish(ChangeRecord::AdmissionRejected {
        name,
        reason,
    }));
}

fn handle_credential_verified(
    ctx: &mut Context,
    request: VerificationId,
    session: SessionId,
    evidence: AdmissionEvidence,
) {
    let Some(pending) = ctx.model.admissions().get(&session).cloned() else {
        ctx.note(Diagnostic::UnknownCorrelation {
            kind: "credential verification",
        });
        return;
    };
    if pending.stage != (AdmissionStage::AwaitingEvidence { request }) {
        ctx.note(Diagnostic::UnknownCorrelation {
            kind: "superseded credential verification",
        });
        return;
    }

    if let Err(reason) = admission::evaluate_evidence(evidence) {
        refuse_admission(ctx, session, pending.name, reason);
        return;
    }

    // Re-run the local gates: the shell's verification is not instantaneous,
    // and membership may have locked or filled while it ran.
    if let Err(reason) = admission::evaluate_local_gates(
        &ctx.model,
        true,
        true,
        pending.protocol,
        &pending.name,
        pending.cert_fingerprint,
    ) {
        refuse_admission(ctx, session, pending.name, reason);
        return;
    }

    let allocation = AllocationId(ctx.model.next_correlation());
    if let Some(entry) = ctx.model.admissions_mut().get_mut(&session) {
        entry.stage = AdmissionStage::AwaitingNodeId {
            request: allocation,
        };
    }
    ctx.emit(Effect::AllocateNodeId {
        request: allocation,
        session,
    });
}

fn handle_node_id_allocated(
    ctx: &mut Context,
    request: AllocationId,
    session: SessionId,
    node_id: NodeId,
) {
    let Some(pending) = ctx.model.admissions().get(&session).cloned() else {
        ctx.note(Diagnostic::UnknownCorrelation {
            kind: "node ID allocation",
        });
        return;
    };
    if pending.stage != (AdmissionStage::AwaitingNodeId { request }) {
        ctx.note(Diagnostic::UnknownCorrelation {
            kind: "superseded node ID allocation",
        });
        return;
    }

    // Acceptance is only reported after the ID is known to be free and the
    // member is actually inserted. Replying first and inserting afterwards
    // would let a collision produce an admitted node that no one holds a
    // record for.
    if ctx.model.member(&node_id).is_some() || ctx.model.has_tombstone(&node_id) {
        refuse_admission(ctx, session, pending.name, RejectReason::IdentityCollision);
        return;
    }

    let version = VersionTuple::initial(0, ctx.model.local_id().clone());
    let member = Member {
        id: node_id.clone(),
        name: pending.name.clone(),
        cert_fingerprint: pending.cert_fingerprint,
        protocol: pending.protocol,
        liveness: Liveness::Alive,
        incarnation: Incarnation::INITIAL,
        version,
        endpoints: pending.endpoints.clone(),
        accepts: pending.accepts,
        capacity: pending.capacity,
        capabilities: pending.capabilities.clone(),
    };
    ctx.model
        .members_mut()
        .insert(node_id.clone(), member.clone());
    ctx.model.admissions_mut().remove(&session);

    let snapshot: Vec<Member> = ctx
        .model
        .members()
        .values()
        .take(ctx.model.limits().max_snapshot_members())
        .cloned()
        .collect();
    let formation = ctx.model.formation().clone();
    let cluster_name = ctx.model.cluster_name().clone();
    ctx.reply_to(
        session,
        OutboundBody::JoinAccepted {
            formation,
            cluster_name,
            assigned: node_id.clone(),
            snapshot,
        },
    );

    let limits = ctx.model.limits().clone();
    ctx.model
        .gossip_mut()
        .enqueue(DeltaBody::MembershipUpdate(member), &limits);
    ctx.emit(Effect::Publish(ChangeRecord::MemberAdmitted {
        node: node_id,
        name: pending.name,
    }));
}

// ── Admission, joiner side ───────────────────────────────────────────────

fn handle_join_reply(ctx: &mut Context, context: PeerContext, reply: JoinReply) {
    let Some(attempt) = ctx.model.join_attempt().cloned() else {
        ctx.note(Diagnostic::Unexpected {
            what: "join reply with no join attempt in progress",
        });
        return;
    };
    if context.session != attempt.session {
        ctx.note(Diagnostic::Unexpected {
            what: "join reply on a session other than the one the request went out on",
        });
        return;
    }
    if context.formation != attempt.target_formation {
        ctx.note(Diagnostic::FormationMismatch {
            expected: attempt.target_formation,
            received: context.formation,
        });
        return;
    }

    match reply {
        JoinReply::Rejected { reason } => {
            ctx.note(Diagnostic::JoinRejected { reason });
            retry_join(ctx, attempt);
        }
        JoinReply::Redirect { candidates } => {
            let max = ctx.model.limits().max_redirects();
            if candidates.len() > max {
                ctx.note(Diagnostic::LimitExceeded {
                    limit: "maxRedirects",
                    value: candidates.len(),
                    max,
                });
            }
            let bounded: Vec<_> = candidates.into_iter().take(max).collect();
            if let Some(current) = ctx.model.join_attempt_mut() {
                current.redirects = bounded;
            }
            let attempt = ctx.model.join_attempt().cloned().unwrap_or(attempt);
            retry_join(ctx, attempt);
        }
        JoinReply::Accepted {
            formation,
            cluster_name,
            assigned,
            snapshot,
            snapshot_bytes,
        } => adopt_formation(
            ctx,
            &attempt,
            context,
            formation,
            cluster_name,
            assigned,
            snapshot,
            snapshot_bytes,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn adopt_formation(
    ctx: &mut Context,
    attempt: &JoinAttempt,
    context: PeerContext,
    formation: FormationId,
    cluster_name: ClusterName,
    assigned: NodeId,
    snapshot: Vec<Member>,
    snapshot_bytes: usize,
) {
    let limits = ctx.model.limits().clone();

    if formation != attempt.target_formation {
        ctx.note(Diagnostic::JoinReplyInvalid {
            reason: "reply assigns a formation other than the one being joined",
        });
        return;
    }
    if snapshot_bytes > limits.max_snapshot_bytes() {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "maxSnapshotBytes",
            value: snapshot_bytes,
            max: limits.max_snapshot_bytes(),
        });
        return;
    }
    if snapshot.len() > limits.max_snapshot_members() {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "maxSnapshotMembers",
            value: snapshot.len(),
            max: limits.max_snapshot_members(),
        });
        return;
    }

    let mut members: BTreeMap<NodeId, Member> = BTreeMap::new();
    for member in snapshot {
        if let Err(error) = member.validate(&limits) {
            ctx.note(Diagnostic::MalformedRecord {
                entity: format!("snapshot:{}", member.id),
                error,
            });
            return;
        }
        if !limits_protocol_ok(&limits, ctx, &member) {
            return;
        }
        if members.insert(member.id.clone(), member).is_some() {
            ctx.note(Diagnostic::JoinReplyInvalid {
                reason: "snapshot contains duplicate node identities",
            });
            return;
        }
    }

    // The introducer must have inserted us before replying, and the entry it
    // inserted must be *us*: same assigned ID, same certificate, same label.
    // Without this, an introducer could admit one node and hand another node's
    // identity to the joiner.
    let Some(self_entry) = members.get(&assigned) else {
        ctx.note(Diagnostic::JoinReplyInvalid {
            reason: "snapshot omits the entry for the newly assigned identity",
        });
        return;
    };
    if self_entry.cert_fingerprint != ctx.model.local().cert_fingerprint {
        ctx.note(Diagnostic::JoinReplyInvalid {
            reason: "snapshot entry for this node pins a different certificate",
        });
        return;
    }
    if self_entry.name != ctx.model.local().worker_name {
        ctx.note(Diagnostic::JoinReplyInvalid {
            reason: "snapshot entry for this node carries a different label",
        });
        return;
    }
    if !ctx.model.policy().protocol_range.accepts(context.protocol) {
        ctx.note(Diagnostic::JoinReplyInvalid {
            reason: "introducer speaks an incompatible protocol version",
        });
        return;
    }

    let previous = ctx.model.formation().clone();
    ctx.model =
        ctx.model
            .adopt_formation(formation.clone(), cluster_name, assigned.clone(), members);
    ctx.emit(Effect::Publish(ChangeRecord::FormationAdopted {
        from: previous,
        to: formation,
        assigned,
    }));
}

fn limits_protocol_ok(_limits: &crate::limits::Limits, ctx: &mut Context, member: &Member) -> bool {
    if ctx.model.policy().protocol_range.accepts(member.protocol) {
        return true;
    }
    ctx.note(Diagnostic::JoinReplyInvalid {
        reason: "snapshot contains a member speaking an incompatible protocol version",
    });
    false
}

// ── Anti-entropy ─────────────────────────────────────────────────────────

fn handle_pull_request(
    ctx: &mut Context,
    sender: &NodeId,
    round: u64,
    digest: &crate::antientropy::MerkleDigest,
    buckets: &[u16],
    cursor: Option<&crate::model::AntiEntropyCursor>,
) {
    let tree = MembershipTree::build(&ctx.model);
    let bucket_count = ctx.model.limits().anti_entropy_buckets();

    let requested: Vec<u16> = if buckets.is_empty() {
        match tree.divergent_buckets(digest) {
            Ok(divergent) => divergent,
            Err(error) => {
                ctx.note(Diagnostic::MalformedRecord {
                    entity: format!("digest:{sender}"),
                    error: crate::model::ValidationError::Inconsistent {
                        field: "digest",
                        reason: match error {
                            crate::antientropy::DigestError::Malformed => "digest is malformed",
                            crate::antientropy::DigestError::DepthMismatch { .. } => {
                                "digest depth differs"
                            }
                        },
                    },
                });
                return;
            }
        }
    } else {
        let mut seen = BTreeSet::new();
        buckets
            .iter()
            .copied()
            .filter(|bucket| usize::from(*bucket) < bucket_count)
            .filter(|bucket| seen.insert(*bucket))
            .take(bucket_count)
            .collect()
    };

    let batch = tree.collect(
        &requested,
        cursor,
        ctx.model.limits().max_anti_entropy_entries(),
    );
    let reply = OutboundBody::PullReply {
        round,
        digest: tree.digest(),
        deltas: batch.deltas,
        complete: batch.complete,
        cursor: batch.cursor,
    };
    ctx.send_to(sender.clone(), reply);
}

fn handle_pull_reply(
    ctx: &mut Context,
    sender: &NodeId,
    round: u64,
    deltas: Vec<GossipDelta>,
    complete: bool,
    cursor: Option<crate::model::AntiEntropyCursor>,
) {
    let Some(current) = ctx.model.anti_entropy().cloned() else {
        ctx.note(Diagnostic::Unexpected {
            what: "anti-entropy reply with no round in progress",
        });
        return;
    };
    if current.round != round || &current.peer != sender {
        ctx.note(Diagnostic::UnknownCorrelation {
            kind: "anti-entropy round",
        });
        return;
    }

    let max = ctx.model.limits().max_anti_entropy_entries();
    if deltas.len() > max {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "maxAntiEntropyEntries",
            value: deltas.len(),
            max,
        });
    }
    for delta in deltas.into_iter().take(max) {
        match delta.body {
            DeltaBody::Foreign(foreign) => ctx.foreign.push(ForeignHandoff {
                from: Some(sender.clone()),
                delta: foreign,
            }),
            body => {
                let outcome = merge::merge_delta(&mut ctx.model, body);
                absorb_outcome(ctx, outcome);
            }
        }
    }

    ctx.cancel(current.timer);

    // A truncated reply is not convergence. Continuing is bounded so a peer
    // that always answers `complete: false` cannot hold the round open.
    if complete || cursor.is_none() {
        ctx.model.set_anti_entropy(None);
        return;
    }
    let exchanges = current.exchanges.saturating_add(1);
    if exchanges >= ctx.model.limits().max_anti_entropy_rounds() {
        ctx.model.set_anti_entropy(None);
        ctx.note(Diagnostic::AntiEntropyAbandoned {
            peer: sender.clone(),
            exchanges,
        });
        return;
    }

    let timer = ctx.timer(TimerKind::AntiEntropy);
    ctx.model.set_anti_entropy(Some(AntiEntropyRound {
        peer: sender.clone(),
        round,
        cursor: cursor.clone(),
        exchanges,
        timer,
    }));
    let digest = MembershipTree::build(&ctx.model).digest();
    ctx.send_to(
        sender.clone(),
        OutboundBody::PullRequest {
            round,
            digest,
            buckets: Vec::new(),
            cursor,
        },
    );
    let delay = ctx.model.limits().anti_entropy_timeout();
    ctx.arm(timer, delay);
}

// ── Local commands ───────────────────────────────────────────────────────

fn handle_command(ctx: &mut Context, command: Command) {
    match command {
        Command::BeginJoin {
            session,
            target_formation,
        } => {
            let timer = ctx.timer(TimerKind::JoinRetry);
            ctx.model.set_join_attempt(Some(JoinAttempt {
                session,
                target_formation,
                attempt: 1,
                redirects: Vec::new(),
                timer,
            }));
            let body = join_request_body(&ctx.model);
            ctx.reply_to(session, body);
            let delay = ctx.model.limits().join_backoff_for(1);
            ctx.arm(timer, delay);
        }

        Command::Leave {
            replacement_formation,
            replacement_node_id,
            replacement_cluster_name,
        } => handle_leave(
            ctx,
            replacement_formation,
            replacement_node_id,
            replacement_cluster_name,
        ),

        Command::RemoveMember { node, mode, reason } => handle_remove(ctx, node, mode, reason),

        Command::ClearTombstone { node } => handle_clear_tombstone(ctx, node),

        Command::SetPolicy(policy) => ctx.model.set_policy(policy),

        Command::UpdateBlocklist(entry) => {
            let outcome = merge::merge_delta(&mut ctx.model, DeltaBody::BlocklistUpdate(entry));
            absorb_outcome(ctx, outcome);
        }

        Command::StartProbeRound => {
            if ctx.model.scale_size() <= 1 {
                // A formation of one has nothing to probe; asking the shell
                // for a peer it cannot supply would just burn a correlation.
                return;
            }
            ctx.request_peers(SelectionPurpose::DirectProbe, 1);
        }

        Command::StartAntiEntropyRound => {
            if ctx.model.anti_entropy().is_some() {
                ctx.note(Diagnostic::Unexpected {
                    what: "anti-entropy round requested while one is already in progress",
                });
                return;
            }
            if ctx.model.scale_size() <= 1 {
                return;
            }
            ctx.request_peers(SelectionPurpose::AntiEntropy, 1);
        }
    }
}

fn handle_leave(
    ctx: &mut Context,
    formation: FormationId,
    node_id: NodeId,
    cluster_name: ClusterName,
) {
    let local = ctx.model.local_id().clone();
    let incarnation = ctx.model.incarnation();
    let previous = ctx.model.formation().clone();

    // A departing node cannot rely on gossip it will no longer participate in,
    // so it tells a bounded set of peers directly and lets SWIM cover the rest.
    let notify: Vec<NodeId> = ctx
        .model
        .members()
        .values()
        .filter(|member| member.id != local && member.liveness == Liveness::Alive)
        .map(|member| member.id.clone())
        .take(ctx.model.limits().indirect_probe_count().saturating_mul(2))
        .collect();
    for peer in notify {
        ctx.send_to(
            peer,
            OutboundBody::Announce {
                announcement: Announcement::Leave,
                target: local.clone(),
                incarnation,
            },
        );
    }

    let policy = ctx.model.policy().clone();
    let limits = ctx.model.limits().clone();
    let mut local_identity = ctx.model.local().clone();
    local_identity.node_id = node_id;

    // Leaving writes no tombstone: this node chose to stop participating, and
    // the formation has not barred it.
    ctx.model = Membership::standalone(formation, cluster_name, local_identity, policy, limits)
        .expect("local description was already validated");
    ctx.emit(Effect::Publish(ChangeRecord::FormationLeft {
        formation: previous,
        previous: local,
    }));
}

fn handle_remove(ctx: &mut Context, node: NodeId, mode: RemovalMode, reason: Option<String>) {
    if &node == ctx.model.local_id() {
        ctx.note(Diagnostic::Unexpected {
            what: "operator asked this node to remove itself; use Leave",
        });
        return;
    }
    let Some(member) = ctx.model.member(&node).cloned() else {
        ctx.note(Diagnostic::UnknownSender { claimed: node });
        return;
    };
    let Some(version) = merge::local_successor(&mut ctx.model, &member.version) else {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "versionCounter",
            value: 0,
            max: 0,
        });
        return;
    };

    let tombstone = MembershipTombstone {
        node_id: node,
        name: member.name,
        cert_fingerprint: member.cert_fingerprint,
        removal_mode: mode,
        version,
        cleared: false,
        reason,
    };
    let outcome = merge::merge_delta(&mut ctx.model, DeltaBody::TombstoneUpdate(tombstone));
    absorb_outcome(ctx, outcome);
}

fn handle_clear_tombstone(ctx: &mut Context, node: NodeId) {
    let Some(current) = ctx.model.tombstones().get(&node).cloned() else {
        ctx.note(Diagnostic::Unexpected {
            what: "no tombstone to clear for that identity",
        });
        return;
    };
    if current.cleared {
        return;
    }
    let Some(version) = merge::local_successor(&mut ctx.model, &current.version) else {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "versionCounter",
            value: 0,
            max: 0,
        });
        return;
    };

    // Clearing is a versioned update, not a deletion, so it converges: a peer
    // still holding the uncleared tombstone adopts the newer version instead
    // of re-fencing the node.
    let cleared = MembershipTombstone {
        cleared: true,
        version,
        ..current
    };
    ctx.model.tombstones_mut().insert(node, cleared.clone());
    let limits = ctx.model.limits().clone();
    ctx.model
        .gossip_mut()
        .enqueue(DeltaBody::TombstoneUpdate(cleared), &limits);
}

// ── Effect outcomes ──────────────────────────────────────────────────────

fn handle_outcome(ctx: &mut Context, outcome: EffectOutcome) {
    match outcome {
        EffectOutcome::PeersSelected { request, peers } => {
            handle_peers_selected(ctx, request, peers)
        }
        EffectOutcome::NodeIdAllocated {
            request,
            session,
            node_id,
        } => handle_node_id_allocated(ctx, request, session, node_id),
        EffectOutcome::NodeIdUnavailable { request, session } => {
            let Some(pending) = ctx.model.admissions().get(&session).cloned() else {
                ctx.note(Diagnostic::UnknownCorrelation {
                    kind: "node ID allocation",
                });
                return;
            };
            if pending.stage != (AdmissionStage::AwaitingNodeId { request }) {
                ctx.note(Diagnostic::UnknownCorrelation {
                    kind: "superseded node ID allocation",
                });
                return;
            }
            refuse_admission(ctx, session, pending.name, RejectReason::Overloaded);
        }
        EffectOutcome::CredentialVerified {
            request,
            session,
            evidence,
        } => handle_credential_verified(ctx, request, session, evidence),
    }
}

fn handle_peers_selected(ctx: &mut Context, request: SelectionId, peers: Vec<NodeId>) {
    let Some(purpose) = ctx.model.selections_mut().remove(&request) else {
        ctx.note(Diagnostic::UnknownCorrelation {
            kind: "peer selection",
        });
        return;
    };

    match purpose {
        SelectionPurpose::DirectProbe => {
            let Some(target) = peers.into_iter().find(|id| ctx.model.member(id).is_some()) else {
                return;
            };
            start_direct_probe(ctx, target);
        }

        SelectionPurpose::IndirectProbe { probe } => {
            let Some(pending) = ctx.model.probes().get(&probe).cloned() else {
                // The probe completed while the shell was choosing peers; the
                // target has already answered, so there is nothing to ask.
                return;
            };
            let helpers: Vec<NodeId> = peers
                .into_iter()
                .filter(|id| id != &pending.target && ctx.model.member(id).is_some())
                .take(ctx.model.limits().indirect_probe_count())
                .collect();
            if helpers.is_empty() {
                // No intermediary available: fall back to the direct evidence
                // we already have rather than waiting for a timeout.
                ctx.model.probes_mut().remove(&probe);
                ctx.cancel(pending.timer);
                suspect(ctx, pending.target, pending.target_incarnation);
                return;
            }
            if let Some(entry) = ctx.model.probes_mut().get_mut(&probe) {
                entry.phase = ProbePhase::Indirect {
                    helpers: helpers.clone(),
                    answered: BTreeSet::new(),
                };
            }
            for helper in helpers {
                ctx.send_to(
                    helper,
                    OutboundBody::PingReq {
                        probe,
                        target: pending.target.clone(),
                    },
                );
            }
        }

        SelectionPurpose::AntiEntropy => {
            let Some(peer) = peers.into_iter().find(|id| ctx.model.member(id).is_some()) else {
                return;
            };
            let round = ctx.model.next_correlation();
            let timer = ctx.timer(TimerKind::AntiEntropy);
            ctx.model.set_anti_entropy(Some(AntiEntropyRound {
                peer: peer.clone(),
                round,
                cursor: None,
                exchanges: 0,
                timer,
            }));
            let digest = MembershipTree::build(&ctx.model).digest();
            ctx.send_to(
                peer,
                OutboundBody::PullRequest {
                    round,
                    digest,
                    buckets: Vec::new(),
                    cursor: None,
                },
            );
            let delay = ctx.model.limits().anti_entropy_timeout();
            ctx.arm(timer, delay);
        }

        SelectionPurpose::Announce {
            announcement,
            target,
            incarnation,
        } => {
            let fan_out = ctx.model.limits().indirect_probe_count();
            for peer in peers.into_iter().take(fan_out) {
                if ctx.model.member(&peer).is_none() {
                    continue;
                }
                ctx.send_to(
                    peer,
                    OutboundBody::Announce {
                        announcement,
                        target: target.clone(),
                        incarnation,
                    },
                );
            }
        }
    }
}

fn start_direct_probe(ctx: &mut Context, target: NodeId) {
    let limit = ctx.model.limits().max_pending_probes();
    if ctx.model.probes().len() >= limit {
        ctx.note(Diagnostic::LimitExceeded {
            limit: "maxPendingProbes",
            value: ctx.model.probes().len() + 1,
            max: limit,
        });
        return;
    }

    let probe = ProbeId(ctx.model.next_correlation());
    let timer = ctx.timer(TimerKind::DirectProbe);
    let target_incarnation = ctx
        .model
        .member(&target)
        .map_or(Incarnation::INITIAL, |member| member.incarnation);
    ctx.model.probes_mut().insert(
        probe,
        PendingProbe {
            id: probe,
            target: target.clone(),
            target_incarnation,
            phase: ProbePhase::Direct,
            timer,
        },
    );
    let incarnation = ctx.model.incarnation();
    ctx.send_to(target, OutboundBody::Ping { probe, incarnation });
    let delay = ctx.model.limits().probe_timeout();
    ctx.arm(timer, delay);
}
