//! SWIM: correlated probing, the liveness matrix, refutation, and the line
//! between failure detection and removal.

use orishu_membership::{
    Announcement, ChangeRecord, Command, Destination, Diagnostic, Incarnation, IndirectResult,
    Liveness, Message, NodeId, OutboundBody, PeerBody, PeerInput, ProbeId, RemovalMode, SessionId,
    TimerKind, TimerToken,
    testing::{self, Driver},
};

fn node(id: &str) -> NodeId {
    NodeId::new(id).unwrap()
}

fn liveness(driver: &Driver, id: &str) -> Liveness {
    driver.model().member(&node(id)).expect("member").liveness
}

fn incarnation(driver: &Driver, id: &str) -> Incarnation {
    driver
        .model()
        .member(&node(id))
        .expect("member")
        .incarnation
}

/// Starts a probe round and answers the peer selection with `target`.
fn probe(driver: &mut Driver, target: &str) -> ProbeId {
    driver.apply(Message::Local(Command::StartProbeRound));
    driver.supply_peers(&[target]);
    driver
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            OutboundBody::Ping { probe, .. } => Some(*probe),
            _ => None,
        })
        .expect("a probe round must send a Ping")
}

fn armed_of(driver: &Driver, kind: TimerKind) -> TimerToken {
    driver
        .armed()
        .into_iter()
        .find(|token| token.kind == kind)
        .unwrap_or_else(|| panic!("expected an armed {kind:?} timer"))
}

/// Delivers a peer message from `sender`.
fn from(driver: &mut Driver, sender: &str, body: PeerBody) {
    let context = testing::peer_context(driver.model(), &node(sender), next_seq(driver, sender));
    driver.apply(Message::Peer(PeerInput { context, body }));
}

/// Sequence numbers only need to advance; the exact values are irrelevant to
/// datagram correlation, which is the point being tested.
fn next_seq(driver: &Driver, sender: &str) -> u64 {
    // Derived from the model so repeated sends from one peer strictly increase.
    driver.model().members().len() as u64 * 100 + sender.len() as u64
}

// ── The direct probe ─────────────────────────────────────────────────────

#[test]
fn a_probe_round_pings_the_selected_peer_and_arms_a_timeout() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let probe_id = probe(&mut driver, "node-0001");

    match driver.sent().as_slice() {
        [(Destination::Member(target), OutboundBody::Ping { probe, .. })] => {
            assert_eq!(*target, node("node-0001"));
            assert_eq!(*probe, probe_id);
        }
        other => panic!("expected one Ping, got {other:?}"),
    }
    assert_eq!(
        armed_of(&driver, TimerKind::DirectProbe).kind,
        TimerKind::DirectProbe
    );
    assert_eq!(driver.model().probes().len(), 1);
}

#[test]
fn a_formation_of_one_has_nothing_to_probe() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    driver.apply(Message::Local(Command::StartProbeRound));
    assert!(driver.effects.is_empty());
}

#[test]
fn an_ack_clears_the_probe_and_cancels_its_timer() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let probe_id = probe(&mut driver, "node-0001");
    let timer = armed_of(&driver, TimerKind::DirectProbe);

    from(
        &mut driver,
        "node-0001",
        PeerBody::Ack {
            probe: probe_id,
            incarnation: Incarnation(3),
        },
    );

    assert!(driver.model().probes().is_empty());
    assert_eq!(driver.cancelled(), vec![timer]);
    assert_eq!(incarnation(&driver, "node-0001"), Incarnation(3));
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);
}

#[test]
fn a_duplicate_ack_is_ignored_rather_than_reprocessed() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let probe_id = probe(&mut driver, "node-0001");
    for _ in 0..2 {
        from(
            &mut driver,
            "node-0001",
            PeerBody::Ack {
                probe: probe_id,
                incarnation: Incarnation(3),
            },
        );
    }
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::UnknownProbe { probe }] if *probe == probe_id
    ));
}

#[test]
fn an_ack_from_the_wrong_node_cannot_clear_a_probe() {
    // Without correlation this is exactly the attack: any member could keep a
    // failing node alive by answering probes addressed to it.
    let mut driver = Driver::new(testing::model_with_members(4));
    let probe_id = probe(&mut driver, "node-0001");
    from(
        &mut driver,
        "node-0002",
        PeerBody::Ack {
            probe: probe_id,
            incarnation: Incarnation(9),
        },
    );
    assert_eq!(
        driver.model().probes().len(),
        1,
        "the probe stays in flight"
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::ProbeMismatch { .. }]
    ));
}

#[test]
fn an_ack_for_an_unknown_probe_is_reported_not_applied() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::Ack {
            probe: ProbeId(4242),
            incarnation: Incarnation(9),
        },
    );
    assert_eq!(incarnation(&driver, "node-0001"), Incarnation::INITIAL);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::UnknownProbe { .. }]
    ));
}

// ── Escalation to indirect probing ───────────────────────────────────────

#[test]
fn a_direct_timeout_escalates_to_indirect_probing() {
    let mut driver = Driver::new(testing::model_with_members(6));
    let probe_id = probe(&mut driver, "node-0001");
    let direct = armed_of(&driver, TimerKind::DirectProbe);

    driver.apply(Message::Timer(direct));
    // The indirect deadline is armed before the intermediaries are chosen, so
    // a shell that never answers cannot strand the probe.
    let indirect = armed_of(&driver, TimerKind::IndirectProbe);
    assert_ne!(indirect, direct);
    assert_eq!(driver.model().probes()[&probe_id].timer, indirect);

    driver.supply_peers(&["node-0002", "node-0003", "node-0004"]);
    let requests: Vec<_> = driver
        .sent()
        .into_iter()
        .filter_map(|(destination, body)| match (destination, body) {
            (Destination::Member(helper), OutboundBody::PingReq { probe, target }) => {
                Some((helper.clone(), *probe, target.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|(_, probe, target)| { *probe == probe_id && target == &node("node-0001") })
    );
}

#[test]
fn a_stale_direct_timeout_cannot_touch_a_newer_probe() {
    let mut driver = Driver::new(testing::model_with_members(6));
    let probe_id = probe(&mut driver, "node-0001");
    let direct = armed_of(&driver, TimerKind::DirectProbe);

    driver.apply(Message::Timer(direct));
    driver.supply_peers(&["node-0002"]);

    // The direct timer has been superseded by the indirect one. Replaying it
    // must not restart escalation.
    driver.apply(Message::Timer(direct));
    assert_eq!(
        driver.diagnostics,
        vec![Diagnostic::StaleTimer { token: direct }]
    );
    assert!(driver.effects.is_empty());
    assert!(matches!(
        driver.model().probes()[&probe_id].phase,
        orishu_membership::model::ProbePhase::Indirect { .. }
    ));
}

#[test]
fn one_indirect_success_keeps_the_target_alive() {
    let mut driver = Driver::new(testing::model_with_members(6));
    let probe_id = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&["node-0002", "node-0003", "node-0004"]);

    from(
        &mut driver,
        "node-0003",
        PeerBody::PingReply {
            probe: probe_id,
            target: node("node-0001"),
            result: IndirectResult::Ack(Incarnation(5)),
        },
    );

    assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);
    assert_eq!(incarnation(&driver, "node-0001"), Incarnation(5));
    assert!(driver.model().probes().is_empty());
    assert!(driver.model().suspicions().is_empty());
}

#[test]
fn suspicion_needs_every_intermediary_to_fail() {
    let mut driver = Driver::new(testing::model_with_members(6));
    let probe_id = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&["node-0002", "node-0003", "node-0004"]);

    for helper in ["node-0002", "node-0003"] {
        from(
            &mut driver,
            helper,
            PeerBody::PingReply {
                probe: probe_id,
                target: node("node-0001"),
                result: IndirectResult::Timeout,
            },
        );
        assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);
    }

    from(
        &mut driver,
        "node-0004",
        PeerBody::PingReply {
            probe: probe_id,
            target: node("node-0001"),
            result: IndirectResult::NoSuchPeer,
        },
    );
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Suspected);
    assert!(driver.model().suspicions().contains_key(&node("node-0001")));
}

#[test]
fn a_repeated_failure_from_one_intermediary_does_not_count_twice() {
    // Otherwise a single chatty helper could push a healthy node into
    // suspicion on its own.
    let mut driver = Driver::new(testing::model_with_members(6));
    let probe_id = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&["node-0002", "node-0003", "node-0004"]);

    for _ in 0..5 {
        from(
            &mut driver,
            "node-0002",
            PeerBody::PingReply {
                probe: probe_id,
                target: node("node-0001"),
                result: IndirectResult::Timeout,
            },
        );
    }
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);
}

#[test]
fn a_reply_from_an_unasked_node_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(8));
    let probe_id = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&["node-0002", "node-0003", "node-0004"]);

    from(
        &mut driver,
        "node-0005",
        PeerBody::PingReply {
            probe: probe_id,
            target: node("node-0001"),
            result: IndirectResult::Ack(Incarnation(9)),
        },
    );
    assert_eq!(incarnation(&driver, "node-0001"), Incarnation::INITIAL);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::ProbeMismatch { .. }]
    ));
}

#[test]
fn a_reply_naming_a_different_target_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(6));
    let probe_id = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&["node-0002"]);

    from(
        &mut driver,
        "node-0002",
        PeerBody::PingReply {
            probe: probe_id,
            target: node("node-0003"),
            result: IndirectResult::Ack(Incarnation(9)),
        },
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::ProbeMismatch { .. }]
    ));
}

#[test]
fn overlapping_probes_of_one_target_stay_independent() {
    let mut driver = Driver::new(testing::model_with_members(6));
    let first = probe(&mut driver, "node-0001");
    let second = probe(&mut driver, "node-0001");
    assert_ne!(first, second);
    assert_eq!(driver.model().probes().len(), 2);

    from(
        &mut driver,
        "node-0001",
        PeerBody::Ack {
            probe: first,
            incarnation: Incarnation(2),
        },
    );
    assert_eq!(driver.model().probes().len(), 1);
    assert!(driver.model().probes().contains_key(&second));
}

#[test]
fn no_available_intermediary_suspects_immediately() {
    let mut driver = Driver::new(testing::model_with_members(2));
    let _ = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&[]);
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Suspected);
}

// ── Relaying a probe for someone else ────────────────────────────────────

#[test]
fn a_ping_req_is_relayed_and_its_ack_answered() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::PingReq {
            probe: ProbeId(77),
            target: node("node-0002"),
        },
    );

    let relayed = driver
        .sent()
        .into_iter()
        .find_map(|(destination, body)| match (destination, body) {
            (Destination::Member(target), OutboundBody::Ping { probe, .. })
                if target == &node("node-0002") =>
            {
                Some(*probe)
            }
            _ => None,
        })
        .expect("the intermediary must probe the target itself");
    assert_ne!(
        relayed,
        ProbeId(77),
        "the relay uses its own correlation ID"
    );

    from(
        &mut driver,
        "node-0002",
        PeerBody::Ack {
            probe: relayed,
            incarnation: Incarnation(4),
        },
    );
    let reply = driver
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            OutboundBody::PingReply { probe, result, .. } => Some((*probe, *result)),
            _ => None,
        })
        .expect("the requester must get a reply");
    assert_eq!(reply, (ProbeId(77), IndirectResult::Ack(Incarnation(4))));
}

#[test]
fn a_relay_for_an_unknown_target_answers_no_such_peer() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::PingReq {
            probe: ProbeId(77),
            target: node("node-absent"),
        },
    );
    assert!(driver.sent().iter().any(|(_, body)| matches!(
        body,
        OutboundBody::PingReply {
            result: IndirectResult::NoSuchPeer,
            ..
        }
    )));
}

#[test]
fn a_relay_that_times_out_answers_timeout() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::PingReq {
            probe: ProbeId(77),
            target: node("node-0002"),
        },
    );
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    assert!(driver.sent().iter().any(|(_, body)| matches!(
        body,
        OutboundBody::PingReply {
            result: IndirectResult::Timeout,
            ..
        }
    )));
    assert!(driver.model().probes().is_empty());
}

#[test]
fn a_relay_asking_about_this_node_is_answered_directly() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().local_id().clone();
    from(
        &mut driver,
        "node-0001",
        PeerBody::PingReq {
            probe: ProbeId(77),
            target: local.clone(),
        },
    );
    assert!(driver.sent().iter().any(|(_, body)| matches!(
        body,
        OutboundBody::PingReply { target, result: IndirectResult::Ack(_), .. } if target == &local
    )));
}

// ── Suspicion and death ──────────────────────────────────────────────────

#[test]
fn suspicion_expiry_declares_death_without_removing_the_member() {
    let mut driver = Driver::new(testing::model_with_members(2));
    let _ = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&[]);
    let suspicion = armed_of(&driver, TimerKind::Suspicion);

    driver.apply(Message::Timer(suspicion));
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Dead);
    assert!(
        driver.model().member(&node("node-0001")).is_some(),
        "a dead member is still a member; only removal takes it out"
    );
    assert!(
        !driver.model().is_tombstoned(&node("node-0001")),
        "the failure detector must never write a tombstone"
    );
}

#[test]
fn a_stale_suspicion_timer_cannot_kill_a_recovered_member() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let probe_id = probe(&mut driver, "node-0001");
    driver.apply(Message::Timer(armed_of(&driver, TimerKind::DirectProbe)));
    driver.supply_peers(&["node-0002"]);
    from(
        &mut driver,
        "node-0002",
        PeerBody::PingReply {
            probe: probe_id,
            target: node("node-0001"),
            result: IndirectResult::Timeout,
        },
    );
    let suspicion = armed_of(&driver, TimerKind::Suspicion);

    // The member refutes, at a newer incarnation.
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Alive,
            target: node("node-0001"),
            incarnation: Incarnation(1),
        },
    );
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);

    driver.apply(Message::Timer(suspicion));
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);
    assert_eq!(
        driver.diagnostics,
        vec![Diagnostic::StaleTimer { token: suspicion }]
    );
}

// ── The liveness matrix ──────────────────────────────────────────────────

#[test]
fn liveness_transitions_follow_the_override_table() {
    // (current state, current incarnation, announced state, announced
    // incarnation, expected state after)
    let cases = [
        (Liveness::Alive, 3, Announcement::Alive, 4, Liveness::Alive),
        (
            Liveness::Alive,
            3,
            Announcement::Suspect,
            3,
            Liveness::Suspected,
        ),
        (
            Liveness::Alive,
            3,
            Announcement::Suspect,
            4,
            Liveness::Suspected,
        ),
        (
            Liveness::Alive,
            3,
            Announcement::Suspect,
            2,
            Liveness::Alive,
        ),
        (Liveness::Alive, 3, Announcement::Dead, 3, Liveness::Dead),
        (Liveness::Alive, 3, Announcement::Dead, 2, Liveness::Alive),
        (
            Liveness::Suspected,
            3,
            Announcement::Alive,
            4,
            Liveness::Alive,
        ),
        (
            Liveness::Suspected,
            3,
            Announcement::Alive,
            3,
            Liveness::Suspected,
        ),
        (
            Liveness::Suspected,
            3,
            Announcement::Suspect,
            4,
            Liveness::Suspected,
        ),
        (
            Liveness::Suspected,
            3,
            Announcement::Dead,
            3,
            Liveness::Dead,
        ),
        // Death is terminal within one formation.
        (Liveness::Dead, 3, Announcement::Alive, 99, Liveness::Dead),
        (Liveness::Dead, 3, Announcement::Suspect, 99, Liveness::Dead),
    ];

    for (current, current_incarnation, announcement, announced, expected) in cases {
        let mut driver = Driver::new(testing::model_with_members(4));
        testing::set_liveness(
            driver.model_mut(),
            &node("node-0001"),
            current,
            Incarnation(current_incarnation),
        );
        from(
            &mut driver,
            "node-0002",
            PeerBody::Announce {
                announcement,
                target: node("node-0001"),
                incarnation: Incarnation(announced),
            },
        );
        assert_eq!(
            liveness(&driver, "node-0001"),
            expected,
            "{current:?}({current_incarnation}) + {announcement:?}({announced})"
        );
    }
}

// ── Refutation ───────────────────────────────────────────────────────────

#[test]
fn a_node_that_learns_it_is_suspected_refutes_at_a_newer_incarnation() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().local_id().clone();
    assert_eq!(driver.model().incarnation(), Incarnation::INITIAL);

    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Suspect,
            target: local.clone(),
            incarnation: Incarnation::INITIAL,
        },
    );

    assert_eq!(driver.model().incarnation(), Incarnation(1));
    assert_eq!(
        driver.model().member(&local).unwrap().liveness,
        Liveness::Alive
    );
    assert!(
        driver.published().iter().any(|record| matches!(
            record,
            ChangeRecord::LivenessChanged { node, incarnation, .. }
                if node == &local && *incarnation == Incarnation(1)
        )),
        "refutation is a publishable change"
    );
    // Refutation is disseminated by queued gossip plus a bounded direct
    // announcement, not by messaging every member.
    assert_eq!(driver.model().gossip().len(), 1);
    assert!(driver.selection().is_some());
}

#[test]
fn refutation_outranks_the_suspicion_that_caused_it() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().local_id().clone();
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Suspect,
            target: local.clone(),
            incarnation: Incarnation(7),
        },
    );
    assert_eq!(driver.model().incarnation(), Incarnation(8));

    // Replaying the original suspicion is now stale and must not re-trigger.
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Suspect,
            target: local,
            incarnation: Incarnation(7),
        },
    );
    assert_eq!(driver.model().incarnation(), Incarnation(8));
}

#[test]
fn an_exhausted_incarnation_reports_rather_than_wrapping() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().local_id().clone();
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Suspect,
            target: local,
            incarnation: Incarnation(u64::MAX),
        },
    );
    assert_eq!(driver.diagnostics, vec![Diagnostic::IncarnationExhausted]);
    assert_eq!(driver.model().incarnation(), Incarnation::INITIAL);
}

#[test]
fn an_alive_announcement_about_this_node_from_a_peer_is_ignored() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().local_id().clone();
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Alive,
            target: local,
            incarnation: Incarnation(50),
        },
    );
    assert_eq!(driver.model().incarnation(), Incarnation::INITIAL);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::SelfAnnouncementIgnored { .. }]
    ));
}

// ── Leave, removal, and the boundary between them ────────────────────────

#[test]
fn a_self_leave_is_authoritative() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Leave,
            target: node("node-0001"),
            incarnation: Incarnation(2),
        },
    );
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Dead);
    assert!(
        !driver.model().is_tombstoned(&node("node-0001")),
        "leaving is not removal and writes no tombstone"
    );
    assert!(driver.published().iter().any(|record| matches!(
        record,
        ChangeRecord::MemberLeft { node } if node == &self::node("node-0001")
    )));
}

#[test]
fn a_leave_survives_a_replayed_alive_from_the_departing_node() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::Announce {
            announcement: Announcement::Leave,
            target: node("node-0001"),
            incarnation: Incarnation(2),
        },
    );
    from(
        &mut driver,
        "node-0002",
        PeerBody::Announce {
            announcement: Announcement::Alive,
            target: node("node-0001"),
            incarnation: Incarnation(2),
        },
    );
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Dead);
}

#[test]
fn a_leave_announced_about_someone_else_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0002",
        PeerBody::Announce {
            announcement: Announcement::Leave,
            target: node("node-0001"),
            incarnation: Incarnation(9),
        },
    );
    assert_eq!(liveness(&driver, "node-0001"), Liveness::Alive);
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::SpoofedLeave { .. }]
    ));
}

#[test]
fn operator_removal_tombstones_and_evicts() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::RemoveMember {
        node: node("node-0001"),
        mode: RemovalMode::Force,
        reason: Some("decommissioned".into()),
    }));

    assert!(driver.model().member(&node("node-0001")).is_none());
    assert!(driver.model().is_tombstoned(&node("node-0001")));
    assert!(driver.published().iter().any(|record| matches!(
        record,
        ChangeRecord::MemberRemoved {
            mode: RemovalMode::Force,
            ..
        }
    )));
}

#[test]
fn a_tombstone_cannot_be_undone_by_a_later_alive() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::RemoveMember {
        node: node("node-0001"),
        mode: RemovalMode::Force,
        reason: None,
    }));

    from(
        &mut driver,
        "node-0002",
        PeerBody::Announce {
            announcement: Announcement::Alive,
            target: node("node-0001"),
            incarnation: Incarnation(u64::MAX - 1),
        },
    );
    assert!(driver.model().member(&node("node-0001")).is_none());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::TombstoneFenced { .. }]
    ));
}

#[test]
fn clearing_a_tombstone_is_versioned_rather_than_deleted() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::RemoveMember {
        node: node("node-0001"),
        mode: RemovalMode::Graceful,
        reason: None,
    }));
    let removed_version = driver.model().tombstones()[&node("node-0001")]
        .version
        .clone();

    driver.apply(Message::Local(Command::ClearTombstone {
        node: node("node-0001"),
    }));
    let tombstone = &driver.model().tombstones()[&node("node-0001")];
    assert!(tombstone.cleared);
    assert!(
        tombstone.version > removed_version,
        "the clear must outrank the removal so it converges"
    );
    assert!(!driver.model().is_tombstoned(&node("node-0001")));
    assert_eq!(driver.model().gossip().len(), 1);
}

#[test]
fn this_node_cannot_remove_itself() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().local_id().clone();
    driver.apply(Message::Local(Command::RemoveMember {
        node: local.clone(),
        mode: RemovalMode::Force,
        reason: None,
    }));
    assert!(driver.model().member(&local).is_some());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::Unexpected { .. }]
    ));
}

#[test]
fn leaving_returns_to_a_standalone_formation_without_a_tombstone() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let previous_formation = driver.model().formation().clone();
    let previous_id = driver.model().local_id().clone();

    driver.apply(Message::Local(Command::Leave {
        replacement_formation: orishu_membership::FormationId::new("formation-solo").unwrap(),
        replacement_node_id: node("node-solo"),
        replacement_cluster_name: orishu_membership::ClusterName::new("solo").unwrap(),
    }));

    assert!(driver.sent().iter().any(|(_, body)| matches!(
        body,
        OutboundBody::Announce {
            announcement: Announcement::Leave,
            ..
        }
    )));
    assert_eq!(driver.model().members().len(), 1);
    assert_eq!(driver.model().local_id(), &node("node-solo"));
    assert_ne!(driver.model().formation(), &previous_formation);
    assert!(driver.model().tombstones().is_empty());
    assert!(driver.published().iter().any(|record| matches!(
        record,
        ChangeRecord::FormationLeft { previous, .. } if previous == &previous_id
    )));
}

// ── Sender binding ───────────────────────────────────────────────────────

#[test]
fn a_message_from_a_non_member_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 1);
    context.sender = orishu_membership::SenderIdentity::Admitted(node("node-stranger"));
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));
    assert!(driver.sent().is_empty());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::UnknownSender { .. }]
    ));
}

#[test]
fn a_member_presenting_the_wrong_certificate_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 1);
    context.cert_fingerprint = testing::fingerprint(0xEE);
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::CertificateMismatch { .. }]
    ));
}

#[test]
fn a_message_from_another_formation_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 1);
    context.formation = orishu_membership::FormationId::new("formation-elsewhere").unwrap();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::FormationMismatch { .. }]
    ));
}

#[test]
fn a_ping_is_answered_with_an_ack_carrying_the_same_correlation() {
    let mut driver = Driver::new(testing::model_with_members(4));
    from(
        &mut driver,
        "node-0001",
        PeerBody::Ping {
            probe: ProbeId(31),
            incarnation: Incarnation(2),
        },
    );
    assert!(driver.sent().iter().any(|(destination, body)| {
        matches!(
            (destination, body),
            (Destination::Member(target), OutboundBody::Ack { probe, .. })
                if target == &node("node-0001") && *probe == ProbeId(31)
        )
    }));
    assert_eq!(incarnation(&driver, "node-0001"), Incarnation(2));
}

#[test]
fn a_pre_admission_session_cannot_send_post_admission_traffic() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let context = testing::applicant_context(
        driver.model().formation(),
        "worker-alpha",
        SessionId(3),
        0xA1,
    );
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));
    assert!(driver.sent().is_empty());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::Unexpected { .. }]
    ));
}
