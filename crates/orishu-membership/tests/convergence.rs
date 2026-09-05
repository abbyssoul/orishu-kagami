//! Merge and anti-entropy: idempotence, convergence, and the cases that are
//! deliberately *not* resolved silently.
//!
//! The permutation tests here are property tests written without a property
//! framework: the input space that matters is "the same deltas in a different
//! order", and enumerating permutations of a small set covers it exactly
//! rather than sampling it.

use std::collections::BTreeMap;

use orishu_membership::{
    Address, Command, DeltaBody, Destination, Diagnostic, ForeignDelta, GossipDelta, Incarnation,
    Liveness, Member, Message, NodeId, OpaquePayload, OutboundBody, PeerBody, PeerInput, ProbeId,
    RemovalMode, TimerKind, VersionTuple,
    antientropy::{MembershipTree, MerkleDigest},
    model::{AntiEntropyCursor, BlocklistAction, Membership},
    testing::{self, Driver},
};

fn node(id: &str) -> NodeId {
    NodeId::new(id).unwrap()
}

fn version(counter: u64, actor: &str) -> VersionTuple {
    VersionTuple {
        epoch: 0,
        counter,
        actor: node(actor),
    }
}

/// Delivers `deltas` as gossip piggybacked on a probe from `sender`.
fn gossip(driver: &mut Driver, sender: &str, deltas: Vec<DeltaBody>) {
    let mut context = testing::peer_context(driver.model(), &node(sender), 1);
    context.gossip = deltas
        .into_iter()
        .map(|body| GossipDelta { hops: 0, body })
        .collect();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));
}

/// Members, tombstones, and blocklist, rendered for comparison between models.
type MergedState = (BTreeMap<NodeId, Member>, Vec<String>, Vec<String>);

/// A projection of everything the merge path owns, for comparing two models.
fn state(model: &Membership) -> MergedState {
    (
        model.members().clone(),
        model
            .tombstones()
            .values()
            .map(|tombstone| {
                format!(
                    "{}:{}:{}",
                    tombstone.node_id, tombstone.version.counter, tombstone.cleared
                )
            })
            .collect(),
        model
            .blocklist()
            .values()
            .map(|entry| format!("{}:{:?}", entry.key, entry.action))
            .collect(),
    )
}

/// Every permutation of `items`, by Heap's algorithm.
fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    fn generate<T: Clone>(k: usize, items: &mut Vec<T>, out: &mut Vec<Vec<T>>) {
        if k == 1 {
            out.push(items.clone());
            return;
        }
        for index in 0..k {
            generate(k - 1, items, out);
            if k.is_multiple_of(2) {
                items.swap(index, k - 1);
            } else {
                items.swap(0, k - 1);
            }
        }
    }
    let mut working = items.to_vec();
    let mut out = Vec::new();
    let count = working.len();
    generate(count, &mut working, &mut out);
    out
}

// ── Idempotence and ordering ─────────────────────────────────────────────

#[test]
fn exact_replay_changes_nothing() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut updated = testing::member("node-0001", 5);
    updated.endpoints.peers = vec![Address("10.9.9.9:6655".into())];

    gossip(
        &mut driver,
        "node-0002",
        vec![DeltaBody::MembershipUpdate(updated.clone())],
    );
    let after_first = state(driver.model());

    for _ in 0..5 {
        gossip(
            &mut driver,
            "node-0002",
            vec![DeltaBody::MembershipUpdate(updated.clone())],
        );
        assert!(
            driver.diagnostics.is_empty(),
            "replay is a no-op, not an error: {:?}",
            driver.diagnostics
        );
    }
    assert_eq!(state(driver.model()), after_first);
}

#[test]
fn non_conflicting_updates_converge_under_every_delivery_order() {
    let mut tombstone = testing::tombstone("node-0003", RemovalMode::Graceful);
    tombstone.version = version(9, "node-0000");

    let deltas = vec![
        DeltaBody::MembershipUpdate({
            let mut member = testing::member("node-0001", 5);
            member.endpoints.peers = vec![Address("10.1.1.1:6655".into())];
            member
        }),
        DeltaBody::MembershipUpdate({
            let mut member = testing::member("node-0002", 7);
            member.capabilities.cpu_cores = 64;
            member
        }),
        DeltaBody::TombstoneUpdate(tombstone),
        DeltaBody::BlocklistUpdate(testing::blocklist_entry("worker-banned")),
    ];

    let mut expected: Option<MergedState> = None;
    for order in permutations(&deltas) {
        let mut driver = Driver::new(testing::model_with_members(4));
        for delta in order {
            gossip(&mut driver, "node-0000", vec![delta]);
        }
        let final_state = state(driver.model());
        match &expected {
            None => expected = Some(final_state),
            Some(first) => assert_eq!(
                &final_state, first,
                "delivery order must not change the converged state"
            ),
        }
    }
}

#[test]
fn duplicated_and_reordered_delivery_converges() {
    let deltas = vec![
        DeltaBody::MembershipUpdate(testing::member("node-0001", 2)),
        DeltaBody::MembershipUpdate(testing::member("node-0001", 5)),
        DeltaBody::MembershipUpdate(testing::member("node-0001", 3)),
    ];

    for order in permutations(&deltas) {
        let mut driver = Driver::new(testing::model_with_members(4));
        // Deliver each twice, to fold duplication in with reordering.
        for delta in order.iter().chain(order.iter()) {
            gossip(&mut driver, "node-0000", vec![delta.clone()]);
        }
        assert_eq!(
            driver
                .model()
                .member(&node("node-0001"))
                .unwrap()
                .version
                .counter,
            5,
            "the newest version must win regardless of arrival order"
        );
    }
}

#[test]
fn an_older_version_cannot_regress_state() {
    let mut driver = Driver::new(testing::model_with_members(4));
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(testing::member("node-0001", 9))],
    );
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(testing::member("node-0001", 2))],
    );

    assert_eq!(
        driver
            .model()
            .member(&node("node-0001"))
            .unwrap()
            .version
            .counter,
        9
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::StaleDelta { .. }]
    ));
}

#[test]
fn an_equal_version_with_a_different_payload_is_an_explicit_conflict() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut first = testing::member("node-0001", 4);
    first.capabilities.cpu_cores = 8;
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(first.clone())],
    );

    let mut second = first.clone();
    second.capabilities.cpu_cores = 64;
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(second)],
    );

    assert_eq!(
        driver
            .model()
            .member(&node("node-0001"))
            .unwrap()
            .capabilities
            .cpu_cores,
        8,
        "local state is kept; the conflict is reported rather than resolved by arrival order"
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::VersionConflict { .. }]
    ));
}

#[test]
fn a_conflicting_equal_version_is_reported_the_same_way_in_both_directions() {
    // Symmetry matters: if two nodes each hold one of the payloads, both must
    // report the conflict rather than one silently adopting the other.
    let mut low = testing::member("node-0001", 4);
    low.capabilities.cpu_cores = 8;
    let mut high = low.clone();
    high.capabilities.cpu_cores = 64;

    for (held, incoming) in [(low.clone(), high.clone()), (high, low)] {
        let mut driver = Driver::new(testing::model_with_members(4));
        testing::insert_member(driver.model_mut(), held.clone());
        gossip(
            &mut driver,
            "node-0000",
            vec![DeltaBody::MembershipUpdate(incoming)],
        );
        assert_eq!(driver.model().member(&node("node-0001")).unwrap(), &held);
        assert!(matches!(
            driver.diagnostics.as_slice(),
            [Diagnostic::VersionConflict { .. }]
        ));
    }
}

#[test]
fn gossip_cannot_rebind_a_members_certificate() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut impostor = testing::member("node-0001", 99);
    impostor.cert_fingerprint = testing::fingerprint(0xEE);
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(impostor)],
    );

    assert_eq!(
        driver
            .model()
            .member(&node("node-0001"))
            .unwrap()
            .cert_fingerprint,
        testing::member("node-0001", 1).cert_fingerprint
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::CertificateMismatch { .. }]
    ));
}

#[test]
fn a_descriptive_update_does_not_silently_resolve_a_suspicion() {
    // The two orderings are separate: re-advertising an address must not clear
    // a suspicion, and a stale liveness must not revert an address change.
    let mut driver = Driver::new(testing::model_with_members(4));
    testing::set_liveness(
        driver.model_mut(),
        &node("node-0001"),
        Liveness::Suspected,
        Incarnation(4),
    );

    let mut redescribed = testing::member("node-0001", 9);
    redescribed.endpoints.peers = vec![Address("10.5.5.5:6655".into())];
    redescribed.liveness = Liveness::Alive;
    redescribed.incarnation = Incarnation(4);
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(redescribed)],
    );

    let member = driver.model().member(&node("node-0001")).unwrap();
    assert_eq!(member.endpoints.peers[0], Address("10.5.5.5:6655".into()));
    assert_eq!(
        member.liveness,
        Liveness::Suspected,
        "an equal incarnation cannot refute; only the subject can, at a newer one"
    );
}

#[test]
fn a_tombstone_fences_a_member_update_at_any_version() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::RemoveMember {
        node: node("node-0001"),
        mode: RemovalMode::Force,
        reason: None,
    }));
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(testing::member(
            "node-0001",
            u64::MAX,
        ))],
    );
    assert!(driver.model().member(&node("node-0001")).is_none());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::TombstoneFenced { .. }]
    ));
}

#[test]
fn a_blocklist_entry_converges_and_can_be_lifted() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut entry = testing::blocklist_entry("worker-banned");
    entry.version = version(1, "node-0000");
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::BlocklistUpdate(entry.clone())],
    );
    assert!(driver.model().is_blocked(&entry.key));

    let mut lifted = entry.clone();
    lifted.action = BlocklistAction::Allow;
    lifted.version = version(2, "node-0000");
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::BlocklistUpdate(lifted)],
    );
    assert!(!driver.model().is_blocked(&entry.key));
}

// ── The foreign-gossip boundary ──────────────────────────────────────────

#[test]
fn non_membership_gossip_is_handed_over_untouched() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let foreign = testing::foreign_delta();
    gossip(
        &mut driver,
        "node-0001",
        vec![
            DeltaBody::Foreign(foreign.clone()),
            DeltaBody::MembershipUpdate(testing::member("node-0002", 4)),
        ],
    );

    assert_eq!(driver.foreign.len(), 1);
    assert_eq!(driver.foreign[0].delta, foreign);
    assert_eq!(driver.foreign[0].from, Some(node("node-0001")));
    assert!(
        driver
            .model()
            .gossip()
            .iter()
            .all(|(_, _, body)| !matches!(body, DeltaBody::Foreign(_))),
        "membership must not queue another subsystem's data"
    );
    // The membership delta in the same batch is still merged.
    assert_eq!(
        driver
            .model()
            .member(&node("node-0002"))
            .unwrap()
            .version
            .counter,
        4
    );
}

#[test]
fn an_unknown_foreign_delta_type_is_relayed_rather_than_decoded() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let unknown = ForeignDelta {
        delta_type: "SomethingNotYetInvented".into(),
        key: "entity-1".into(),
        version: version(1, "node-0001"),
        payload: OpaquePayload(vec![0xFF; 16]),
    };
    gossip(
        &mut driver,
        "node-0001",
        vec![DeltaBody::Foreign(unknown.clone())],
    );
    assert_eq!(driver.foreign[0].delta, unknown);
    assert!(driver.diagnostics.is_empty());
}

// ── Bounds under hostile input ───────────────────────────────────────────

#[test]
fn an_oversized_gossip_batch_is_truncated_and_reported() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let limit = driver.model().limits().max_gossip_per_inbound_message();
    let deltas: Vec<DeltaBody> = (0..limit * 3)
        .map(|index| DeltaBody::MembershipUpdate(testing::member(&format!("node-f{index:04}"), 1)))
        .collect();
    gossip(&mut driver, "node-0000", deltas);

    assert!(driver.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::LimitExceeded {
            limit: "maxGossipPerInboundMessage",
            ..
        }
    )));
    // 4 peers + local + exactly `limit` newcomers.
    assert_eq!(driver.model().members().len(), 5 + limit);
}

#[test]
fn a_delta_circulating_far_past_retirement_is_dropped() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 1);
    context.gossip = vec![GossipDelta {
        hops: u32::MAX,
        body: DeltaBody::MembershipUpdate(testing::member("node-loop", 1)),
    }];
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));

    assert!(driver.model().member(&node("node-loop")).is_none());
    assert!(
        driver
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, Diagnostic::StaleDelta { .. }))
    );
}

#[test]
fn one_message_cannot_exceed_the_effect_budget() {
    let mut driver = Driver::new(testing::model_with_members(64));
    let limit = driver.model().limits().max_effects_per_update();
    for _ in 0..8 {
        gossip(
            &mut driver,
            "node-0000",
            vec![DeltaBody::MembershipUpdate(testing::member("node-0001", 4))],
        );
        assert!(
            driver.effects.len() <= limit,
            "one message produced {} effects, cap is {limit}",
            driver.effects.len()
        );
    }
}

// ── Anti-entropy rounds ──────────────────────────────────────────────────

#[test]
fn an_anti_entropy_round_sends_a_digest_and_arms_a_deadline() {
    let mut driver = Driver::new(testing::model_with_members(8));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);

    let round = driver
        .sent()
        .into_iter()
        .find_map(|(destination, body)| match (destination, body) {
            (Destination::Member(peer), OutboundBody::PullRequest { round, digest, .. })
                if peer == &node("node-0001") =>
            {
                assert!(digest.is_well_formed());
                Some(*round)
            }
            _ => None,
        })
        .expect("a round must send a PullRequest");

    assert_eq!(driver.model().anti_entropy().unwrap().round, round);
    assert!(
        driver
            .armed()
            .iter()
            .any(|token| token.kind == TimerKind::AntiEntropy)
    );
}

#[test]
fn a_second_round_is_refused_while_one_is_in_flight() {
    let mut driver = Driver::new(testing::model_with_members(8));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::Unexpected { .. }]
    ));
}

#[test]
fn a_pull_request_is_answered_with_the_divergent_entries() {
    let mut responder = Driver::new(testing::model_with_members(8));
    // The requester is missing one member entirely.
    let mut requester_model = testing::model_with_members(8);
    testing::remove_member(&mut requester_model, &node("node-0003"));
    let requester_digest = MembershipTree::build(&requester_model).digest();

    let mut context = testing::peer_context(responder.model(), &node("node-0001"), 1);
    context.seq = 42;
    responder.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullRequest {
            round: 5,
            digest: requester_digest,
            buckets: Vec::new(),
            cursor: None,
        },
    }));

    let deltas = responder
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            OutboundBody::PullReply {
                round,
                deltas,
                complete,
                ..
            } => {
                assert_eq!(*round, 5);
                assert!(complete);
                Some(deltas.clone())
            }
            _ => None,
        })
        .expect("a reply");
    assert!(
        deltas
            .iter()
            .any(|delta| delta.body.entity() == "member:node-0003")
    );
}

#[test]
fn a_pull_reply_merges_and_then_closes_the_round() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);
    let round = driver.model().anti_entropy().unwrap().round;
    let timer = driver.model().anti_entropy().unwrap().timer;

    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 77);
    context.seq = 77;
    let digest = MembershipTree::build(driver.model()).digest();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullReply {
            round,
            digest,
            deltas: vec![GossipDelta {
                hops: 0,
                body: DeltaBody::MembershipUpdate(testing::member("node-new", 1)),
            }],
            complete: true,
            cursor: None,
        },
    }));

    assert!(driver.model().member(&node("node-new")).is_some());
    assert!(driver.model().anti_entropy().is_none());
    assert!(driver.cancelled().contains(&timer));
}

#[test]
fn a_reply_for_another_round_is_refused() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);

    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 77);
    context.seq = 77;
    let digest = MembershipTree::build(driver.model()).digest();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullReply {
            round: 9_999,
            digest,
            deltas: vec![GossipDelta {
                hops: 0,
                body: DeltaBody::MembershipUpdate(testing::member("node-new", 1)),
            }],
            complete: true,
            cursor: None,
        },
    }));

    assert!(driver.model().member(&node("node-new")).is_none());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::UnknownCorrelation { .. }]
    ));
}

#[test]
fn a_truncated_reply_continues_from_its_cursor() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);
    let round = driver.model().anti_entropy().unwrap().round;

    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 77);
    context.seq = 77;
    let digest = MembershipTree::build(driver.model()).digest();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullReply {
            round,
            digest,
            deltas: Vec::new(),
            complete: false,
            cursor: Some(AntiEntropyCursor {
                bucket: 3,
                after_key: vec![1, 2, 3],
            }),
        },
    }));

    let follow_up = driver.sent().into_iter().find_map(|(_, body)| match body {
        OutboundBody::PullRequest { cursor, round, .. } => Some((cursor.clone(), *round)),
        _ => None,
    });
    assert_eq!(
        follow_up,
        Some((
            Some(AntiEntropyCursor {
                bucket: 3,
                after_key: vec![1, 2, 3]
            }),
            round
        )),
        "a truncated reply must be continued, not treated as convergence"
    );
    assert_eq!(driver.model().anti_entropy().unwrap().exchanges, 1);
}

#[test]
fn a_peer_that_never_completes_is_abandoned() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);
    let max = driver.model().limits().max_anti_entropy_rounds();

    for exchange in 0..max + 1 {
        let Some(round) = driver.model().anti_entropy().map(|round| round.round) else {
            break;
        };
        // A `PullReply` is stream-carried, so each one needs a fresh sequence
        // number; a repeated one is a replay and is dropped by design.
        let mut context = testing::peer_context(driver.model(), &node("node-0001"), 77);
        context.seq = 100 + u64::from(exchange);
        let digest = MembershipTree::build(driver.model()).digest();
        driver.apply(Message::Peer(PeerInput {
            context,
            body: PeerBody::PullReply {
                round,
                digest,
                deltas: Vec::new(),
                complete: false,
                cursor: Some(AntiEntropyCursor {
                    bucket: 0,
                    after_key: Vec::new(),
                }),
            },
        }));
    }

    assert!(driver.model().anti_entropy().is_none());
    assert!(
        driver
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, Diagnostic::AntiEntropyAbandoned { .. }))
    );
}

#[test]
fn an_anti_entropy_timeout_abandons_the_round() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::StartAntiEntropyRound));
    driver.supply_peers(&["node-0001"]);
    let timer = driver.model().anti_entropy().unwrap().timer;

    driver.apply(Message::Timer(timer));
    assert!(driver.model().anti_entropy().is_none());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::AntiEntropyAbandoned { .. }]
    ));
}

#[test]
fn a_malformed_peer_digest_produces_no_reply() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 1);
    context.seq = 3;
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullRequest {
            round: 1,
            digest: MerkleDigest {
                depth: 4,
                root: orishu_membership::antientropy::Hash256::from_bytes([0; 32]),
                buckets: Vec::new(),
            },
            buckets: Vec::new(),
            cursor: None,
        },
    }));
    assert!(driver.sent().is_empty());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::MalformedRecord { .. }]
    ));
}

#[test]
fn out_of_range_bucket_requests_are_ignored_not_indexed() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut context = testing::peer_context(driver.model(), &node("node-0001"), 1);
    context.seq = 3;
    let digest = MembershipTree::build(driver.model()).digest();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullRequest {
            round: 1,
            digest,
            buckets: vec![0, 9_999, u16::MAX, 0],
            cursor: None,
        },
    }));
    assert!(
        driver
            .sent()
            .iter()
            .any(|(_, body)| matches!(body, OutboundBody::PullReply { complete: true, .. })),
        "an out-of-range bucket is skipped rather than panicking"
    );
}

#[test]
fn two_diverged_nodes_converge_through_one_exchange() {
    // The end-to-end property anti-entropy exists for: gossip may have missed
    // an entry entirely, and a full comparison still finds it.
    let mut left = testing::model_with_members(12);
    let mut right = testing::model_with_members(12);
    testing::remove_member(&mut left, &node("node-0005"));
    testing::remove_member(&mut right, &node("node-0009"));

    let left_tree = MembershipTree::build(&left);
    let right_tree = MembershipTree::build(&right);
    let divergent = left_tree
        .divergent_buckets(&right_tree.digest())
        .expect("comparable digests");
    assert!(!divergent.is_empty());

    let batch = right_tree.collect(&divergent, None, 1000);
    assert!(batch.complete);
    let mut driver = Driver::new(left);
    for delta in batch.deltas {
        if let DeltaBody::MembershipUpdate(_) = &delta.body {
            gossip(&mut driver, "node-0000", vec![delta.body]);
        }
    }
    assert!(driver.model().member(&node("node-0005")).is_some());
}
