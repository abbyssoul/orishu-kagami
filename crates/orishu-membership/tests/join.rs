//! Joining another formation: validating a `JoinReply` and atomically
//! adopting the target formation.
//!
//! Everything an introducer sends is hostile input. The joiner has no way to
//! independently check most of it, so what it *can* check — its own identity,
//! its own certificate, the formation it asked for, and every bound — it must.

use orishu_membership::{
    ChangeRecord, ClusterName, Command, Destination, Diagnostic, FormationId, JoinReply, Liveness,
    Member, Message, NodeId, OutboundBody, PeerBody, PeerInput, ProtocolVersion, RejectReason,
    SessionId, TimerKind, WorkerName,
    model::BlocklistKey,
    testing::{self, Driver},
};

const SESSION: SessionId = SessionId(11);

fn target_formation() -> FormationId {
    FormationId::new("formation-target").unwrap()
}

/// A member record for this node as the introducer would have written it.
fn self_entry(driver: &Driver, assigned: &str) -> Member {
    let local = driver.model().local();
    let mut member = testing::member(assigned, 1);
    member.name = local.worker_name.clone();
    member.cert_fingerprint = local.cert_fingerprint;
    member.protocol = local.protocol;
    member
}

fn begin_join(driver: &mut Driver) {
    driver.apply(Message::Local(Command::BeginJoin {
        session: SESSION,
        target_formation: target_formation(),
    }));
}

/// Delivers a `JoinReply` on the join session, in the target formation.
fn reply(driver: &mut Driver, reply: JoinReply) {
    let mut context = testing::applicant_context(&target_formation(), "introducer", SESSION, 0x5E);
    context.sender =
        orishu_membership::SenderIdentity::Admitted(NodeId::new("node-introducer").unwrap());
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::JoinReply(reply),
    }));
}

fn accepted(driver: &Driver, assigned: &str, extra: Vec<Member>) -> JoinReply {
    let mut snapshot = vec![self_entry(driver, assigned)];
    snapshot.extend(extra);
    JoinReply::Accepted {
        formation: target_formation(),
        cluster_name: ClusterName::new("target-cluster").unwrap(),
        assigned: NodeId::new(assigned).unwrap(),
        snapshot,
        snapshot_bytes: 4_096,
    }
}

// ── Starting a join ──────────────────────────────────────────────────────

#[test]
fn beginning_a_join_sends_a_request_and_arms_a_retry() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);

    match driver.sent().as_slice() {
        [(Destination::Session(session), OutboundBody::JoinRequest { name, .. })] => {
            assert_eq!(*session, SESSION);
            assert_eq!(name, driver.model().local_name());
        }
        other => panic!("expected one JoinRequest, got {other:?}"),
    }
    assert!(
        driver
            .armed()
            .iter()
            .any(|token| token.kind == TimerKind::JoinRetry)
    );
    assert_eq!(
        driver.join_target(),
        Some(target_formation()),
        "the attempt records which formation it is joining"
    );
}

#[test]
fn a_join_request_carries_no_credential() {
    // The token is the shell's to hold. Anything in this message ends up in a
    // replay log, so a token here would be a token on disk.
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let request = driver
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            body @ OutboundBody::JoinRequest { .. } => Some(format!("{body:?}")),
            _ => None,
        })
        .expect("a join request");
    assert!(
        !request.to_lowercase().contains("token") && !request.to_lowercase().contains("secret"),
        "the join request must carry no credential: {request}"
    );
}

// ── Adoption ─────────────────────────────────────────────────────────────

#[test]
fn reconnect_preserves_join_budget_and_fences_old_timer_and_reply() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let before = driver.model().join_attempt().unwrap().clone();
    let request = driver.sent()[0].1.clone();
    let replacement = SessionId(12);
    driver.apply(Message::Local(Command::RebindJoin {
        previous_session: SESSION,
        session: replacement,
        target_formation: target_formation(),
    }));
    let rebound = driver.model().join_attempt().unwrap().clone();
    assert_eq!(rebound.attempt, before.attempt + 1);
    assert_eq!(rebound.session, replacement);
    assert_eq!(rebound.target_formation, before.target_formation);
    assert_eq!(rebound.redirects, before.redirects);
    assert_ne!(rebound.timer, before.timer);
    assert_eq!(driver.cancelled(), vec![before.timer]);
    assert_eq!(driver.armed(), vec![rebound.timer]);
    assert_eq!(
        driver.sent(),
        vec![(&Destination::Session(replacement), &request)]
    );

    driver.apply(Message::Timer(before.timer));
    assert_eq!(driver.model().join_attempt(), Some(&rebound));
    assert!(driver.sent().is_empty());
    let ack = accepted(&driver, "assigned-once", vec![]);
    reply(&mut driver, ack.clone()); // old session
    assert_eq!(driver.model().join_attempt(), Some(&rebound));
    let mut context =
        testing::applicant_context(&target_formation(), "introducer", replacement, 0x5E);
    context.sender =
        orishu_membership::SenderIdentity::Admitted(NodeId::new("node-introducer").unwrap());
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::JoinReply(ack),
    }));
    assert_eq!(
        driver.model().local_id(),
        &NodeId::new("assigned-once").unwrap()
    );
    assert!(driver.model().join_attempt().is_none());
}

#[test]
fn stale_or_retargeted_reconnect_and_duplicate_begin_do_not_reset_a_join() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let before = driver.model().clone();
    let invalid = [
        Command::BeginJoin {
            session: SessionId(12),
            target_formation: target_formation(),
        },
        Command::RebindJoin {
            previous_session: SessionId(10),
            session: SessionId(12),
            target_formation: target_formation(),
        },
        Command::RebindJoin {
            previous_session: SESSION,
            session: SESSION,
            target_formation: target_formation(),
        },
        Command::RebindJoin {
            previous_session: SESSION,
            session: SessionId(12),
            target_formation: "other-formation".parse().unwrap(),
        },
    ];
    for command in invalid {
        driver.apply(Message::Local(command));
        assert_eq!(driver.model(), &before);
        assert!(driver.effects.is_empty());
        assert!(!driver.diagnostics.is_empty());
    }
}

#[test]
fn repeated_reconnects_exhaust_the_original_join_budget_without_resurrection() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let limit = driver.model().limits().max_join_attempts();
    let original = driver.model().formation().clone();
    for ordinal in 1..=limit {
        let previous = driver.model().join_attempt().unwrap().clone();
        driver.apply(Message::Local(Command::RebindJoin {
            previous_session: previous.session,
            session: SessionId(SESSION.0 + u64::from(ordinal)),
            target_formation: target_formation(),
        }));
        assert_eq!(driver.cancelled(), vec![previous.timer]);
        if ordinal < limit {
            assert_eq!(driver.model().join_attempt().unwrap().attempt, ordinal + 1);
            assert_eq!(driver.sent().len(), 1);
        } else {
            assert!(driver.model().join_attempt().is_none());
            assert!(driver.sent().is_empty());
            assert!(driver.armed().is_empty());
            assert!(
                driver
                    .diagnostics
                    .contains(&Diagnostic::JoinAbandoned { attempts: limit })
            );
        }
    }
    let before = driver.model().clone();
    driver.apply(Message::Local(Command::RebindJoin {
        previous_session: SessionId(SESSION.0 + u64::from(limit)),
        session: SessionId(999),
        target_formation: target_formation(),
    }));
    assert_eq!(driver.model(), &before);
    assert_eq!(driver.model().formation(), &original);
    assert!(driver.effects.is_empty());
}

#[test]
fn a_valid_acceptance_adopts_the_target_formation_atomically() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let previous_formation = driver.model().formation().clone();
    begin_join(&mut driver);

    let introducer = testing::member("node-introducer", 2);
    let payload = accepted(&driver, "node-assigned", vec![introducer.clone()]);
    reply(&mut driver, payload);

    assert_eq!(driver.model().formation(), &target_formation());
    assert_eq!(driver.model().cluster_name().as_str(), "target-cluster");
    assert_eq!(
        driver.model().local_id(),
        &NodeId::new("node-assigned").unwrap()
    );
    assert_eq!(driver.model().members().len(), 2);
    assert!(driver.model().member(&introducer.id).is_some());
    assert!(driver.model().join_attempt().is_none());

    assert!(driver.published().iter().any(|record| matches!(
        record,
        ChangeRecord::FormationAdopted { from, to, .. }
            if from == &previous_formation && to == &target_formation()
    )));
}

#[test]
fn adoption_imports_nothing_from_the_abandoned_formation() {
    let mut driver = Driver::new(testing::model_with_members(4));
    testing::insert_tombstone(
        driver.model_mut(),
        testing::tombstone("node-0001", orishu_membership::RemovalMode::Force),
    );
    testing::insert_blocklist(driver.model_mut(), testing::blocklist_entry("worker-old"));
    let stale_member = NodeId::new("node-0003").unwrap();
    begin_join(&mut driver);

    let payload = accepted(&driver, "node-assigned", vec![]);
    reply(&mut driver, payload);

    assert!(
        driver.model().members().len() == 1,
        "only the validated snapshot seeds the new formation"
    );
    assert!(driver.model().member(&stale_member).is_none());
    assert!(driver.model().tombstones().is_empty());
    assert!(driver.model().blocklist().is_empty());
    assert!(driver.model().gossip().is_empty());
    assert!(driver.model().probes().is_empty());
    assert_eq!(
        driver.model().incarnation(),
        orishu_membership::Incarnation::INITIAL
    );
}

#[test]
fn the_bootstrap_snapshot_is_ordinary_state_not_permanent_authority() {
    // Nothing about being the introducer is recorded; the snapshot's entries
    // are plain members that converge like any other.
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let introducer = testing::member("node-introducer", 2);
    let payload = accepted(&driver, "node-assigned", vec![introducer.clone()]);
    reply(&mut driver, payload);

    let adopted = driver.model().member(&introducer.id).unwrap();
    assert_eq!(adopted.liveness, Liveness::Alive);
    assert_eq!(adopted.version, introducer.version);
}

// ── Rejecting a bad acceptance ───────────────────────────────────────────

/// Asserts that a reply is refused and the joiner stays where it was.
fn assert_refused(driver: &mut Driver, bad: JoinReply) {
    let before = driver.model().formation().clone();
    let previous_id = driver.model().local_id().clone();
    reply(driver, bad);
    assert_eq!(
        driver.model().formation(),
        &before,
        "formation must not change"
    );
    assert_eq!(driver.model().local_id(), &previous_id);
    assert!(!driver.diagnostics.is_empty(), "refusal must be reported");
}

#[test]
fn an_acceptance_for_a_different_formation_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let mut bad = accepted(&driver, "node-assigned", vec![]);
    if let JoinReply::Accepted { formation, .. } = &mut bad {
        *formation = FormationId::new("formation-elsewhere").unwrap();
    }
    // The envelope still claims the target formation, so this tests the
    // payload check rather than the envelope guard.
    assert_refused(&mut driver, bad);
}

#[test]
fn an_acceptance_omitting_this_node_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let bad = JoinReply::Accepted {
        formation: target_formation(),
        cluster_name: ClusterName::new("target-cluster").unwrap(),
        assigned: NodeId::new("node-assigned").unwrap(),
        snapshot: vec![testing::member("node-introducer", 1)],
        snapshot_bytes: 512,
    };
    assert_refused(&mut driver, bad);
}

#[test]
fn an_acceptance_pinning_a_different_certificate_is_refused() {
    // Otherwise an introducer could admit one node and hand its identity to
    // another.
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let mut entry = self_entry(&driver, "node-assigned");
    entry.cert_fingerprint = testing::fingerprint(0xEE);
    let bad = JoinReply::Accepted {
        formation: target_formation(),
        cluster_name: ClusterName::new("target-cluster").unwrap(),
        assigned: NodeId::new("node-assigned").unwrap(),
        snapshot: vec![entry],
        snapshot_bytes: 512,
    };
    assert_refused(&mut driver, bad);
}

#[test]
fn an_acceptance_relabelling_this_node_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let mut entry = self_entry(&driver, "node-assigned");
    entry.name = WorkerName::new("someone-else").unwrap();
    let bad = JoinReply::Accepted {
        formation: target_formation(),
        cluster_name: ClusterName::new("target-cluster").unwrap(),
        assigned: NodeId::new("node-assigned").unwrap(),
        snapshot: vec![entry],
        snapshot_bytes: 512,
    };
    assert_refused(&mut driver, bad);
}

#[test]
fn a_snapshot_with_duplicate_identities_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let duplicate = testing::member("node-introducer", 1);
    let bad = accepted(&driver, "node-assigned", vec![duplicate.clone(), duplicate]);
    assert_refused(&mut driver, bad);
}

#[test]
fn an_oversized_snapshot_is_refused_by_item_count() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let max = driver.model().limits().max_snapshot_members();
    let extra: Vec<Member> = (0..=max)
        .map(|index| testing::member(&format!("node-x{index:05}"), 1))
        .collect();
    let bad = accepted(&driver, "node-assigned", extra);
    assert_refused(&mut driver, bad);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::LimitExceeded {
            limit: "maxSnapshotMembers",
            ..
        }]
    ));
}

#[test]
fn an_oversized_snapshot_is_refused_by_reported_bytes() {
    // The core cannot measure what it never saw encoded, so it enforces the
    // size the decoder reports.
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let max = driver.model().limits().max_snapshot_bytes();
    let bad = JoinReply::Accepted {
        formation: target_formation(),
        cluster_name: ClusterName::new("target-cluster").unwrap(),
        assigned: NodeId::new("node-assigned").unwrap(),
        snapshot: vec![self_entry(&driver, "node-assigned")],
        snapshot_bytes: max + 1,
    };
    assert_refused(&mut driver, bad);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::LimitExceeded {
            limit: "maxSnapshotBytes",
            ..
        }]
    ));
}

#[test]
fn a_snapshot_member_with_an_oversized_description_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let mut bloated = testing::member("node-introducer", 1);
    bloated.capabilities.accelerators = (0..1000).map(|i| format!("gpu-{i}")).collect();
    let bad = accepted(&driver, "node-assigned", vec![bloated]);
    assert_refused(&mut driver, bad);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::MalformedRecord { .. }]
    ));
}

#[test]
fn a_snapshot_member_speaking_an_incompatible_protocol_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let mut future = testing::member("node-introducer", 1);
    future.protocol = ProtocolVersion(99);
    let bad = accepted(&driver, "node-assigned", vec![future]);
    assert_refused(&mut driver, bad);
}

#[test]
fn a_reply_on_the_wrong_session_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let mut context =
        testing::applicant_context(&target_formation(), "introducer", SessionId(99), 0x5E);
    context.sender =
        orishu_membership::SenderIdentity::Admitted(NodeId::new("node-introducer").unwrap());
    let payload = accepted(&driver, "node-assigned", vec![]);
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::JoinReply(payload),
    }));
    assert_ne!(driver.model().formation(), &target_formation());
}

#[test]
fn a_reply_with_no_attempt_in_progress_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let payload = accepted(&driver, "node-assigned", vec![]);
    reply(&mut driver, payload);
    assert_ne!(driver.model().formation(), &target_formation());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::Unexpected { .. }]
    ));
}

// ── Refusal, redirect, and retry ─────────────────────────────────────────

#[test]
fn a_rejection_backs_off_and_retries() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let previous_timer = driver.model().join_attempt().unwrap().timer;
    reply(
        &mut driver,
        JoinReply::Rejected {
            reason: RejectReason::MembershipLocked,
        },
    );

    assert!(driver.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::JoinRejected {
            reason: RejectReason::MembershipLocked
        }
    )));
    assert!(
        driver
            .sent()
            .iter()
            .any(|(_, body)| matches!(body, OutboundBody::JoinRequest { .. }))
    );
    assert_eq!(driver.join_attempt_number(), Some(2));
    assert_eq!(driver.cancelled(), vec![previous_timer]);
    let current = driver.model().join_attempt().unwrap().clone();
    driver.apply(Message::Timer(previous_timer));
    assert_eq!(driver.model().join_attempt(), Some(&current));
    assert!(driver.sent().is_empty());
}

#[test]
fn redirect_hints_are_bounded_and_recorded() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let max = driver.model().limits().max_redirects();
    let candidates = (0..max + 5)
        .map(|index| orishu_membership::Address(format!("10.0.0.{index}:6655")))
        .collect();
    reply(&mut driver, JoinReply::Redirect { candidates });

    let recorded = driver.model().join_attempt().unwrap().redirects.len();
    assert_eq!(recorded, max, "untrusted hints are truncated, not trusted");
    assert!(driver.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::LimitExceeded {
            limit: "maxRedirects",
            ..
        }
    )));
}

#[test]
fn a_join_gives_up_after_its_attempt_budget() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let max = driver.model().limits().max_join_attempts();

    for _ in 0..max {
        let timer = driver
            .armed()
            .into_iter()
            .find(|token| token.kind == TimerKind::JoinRetry);
        match timer {
            Some(token) => {
                driver.apply(Message::Timer(token));
            }
            None => break,
        }
    }

    assert!(driver.model().join_attempt().is_none());
    assert!(
        driver
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, Diagnostic::JoinAbandoned { .. }))
    );
}

#[test]
fn a_stale_join_timer_does_not_retry() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    begin_join(&mut driver);
    let first = driver
        .armed()
        .into_iter()
        .find(|token| token.kind == TimerKind::JoinRetry)
        .unwrap();
    driver.apply(Message::Timer(first));

    driver.apply(Message::Timer(first));
    assert_eq!(
        driver.diagnostics,
        vec![Diagnostic::StaleTimer { token: first }]
    );
    assert!(driver.sent().is_empty());
}

// ── Being removed by the formation ───────────────────────────────────────

#[test]
fn a_tombstone_naming_this_node_removes_it_from_its_own_view() {
    let mut driver = Driver::new(testing::model_with_members(3));
    let local = driver.model().local_id().clone();
    let mut tombstone = testing::tombstone("node-0001", orishu_membership::RemovalMode::Force);
    tombstone.node_id = local.clone();

    let context = {
        let mut context =
            testing::peer_context(driver.model(), &NodeId::new("node-0001").unwrap(), 5);
        context.gossip = vec![orishu_membership::GossipDelta {
            hops: 0,
            body: orishu_membership::DeltaBody::TombstoneUpdate(tombstone),
        }];
        context
    };
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: orishu_membership::ProbeId(1),
            incarnation: orishu_membership::Incarnation(1),
        },
    }));

    assert!(driver.model().member(&local).is_none());
    assert!(driver.model().is_tombstoned(&local));
    assert!(driver.published().iter().any(|record| matches!(
        record,
        ChangeRecord::MemberRemoved { node, .. } if node == &local
    )));
}

// ── Small accessors used above ───────────────────────────────────────────

trait JoinInspection {
    fn join_target(&self) -> Option<FormationId>;
    fn join_attempt_number(&self) -> Option<u32>;
}

impl JoinInspection for Driver {
    fn join_target(&self) -> Option<FormationId> {
        self.model()
            .join_attempt()
            .map(|attempt| attempt.target_formation.clone())
    }

    fn join_attempt_number(&self) -> Option<u32> {
        self.model().join_attempt().map(|attempt| attempt.attempt)
    }
}

#[test]
fn blocklist_keys_are_kept_distinct_by_kind() {
    // A guard against label and identity blocks colliding in one key space.
    let name = BlocklistKey::Name(WorkerName::new("shared").unwrap());
    let node = BlocklistKey::Node(NodeId::new("shared").unwrap());
    assert_ne!(name, node);
}
