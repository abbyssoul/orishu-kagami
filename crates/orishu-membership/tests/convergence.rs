//! Merge and anti-entropy: idempotence, convergence, and the cases that are
//! deliberately *not* resolved silently.
//!
//! The permutation tests here are property tests written without a property
//! framework: the input space that matters is "the same deltas in a different
//! order", and enumerating permutations of a small set covers it exactly
//! rather than sampling it.

use std::collections::BTreeMap;

use orishu_membership::{
    Address, Announcement, ChangeRecord, Command, DeltaBody, Destination, Diagnostic, Effect,
    ForeignDelta, GossipDelta, Incarnation, Liveness, Member, Message, NodeId, OpaquePayload,
    OutboundBody, PeerBody, PeerInput, ProbeId, RemovalMode, TimerKind, VersionTuple,
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
    relay(
        driver,
        sender,
        deltas
            .into_iter()
            .map(|body| GossipDelta { hops: 0, body })
            .collect(),
    );
}

/// Delivers `deltas` exactly as they left the node that piggybacked them,
/// hop counts included.
fn relay(driver: &mut Driver, sender: &str, deltas: Vec<GossipDelta>) {
    let mut context = testing::peer_context(driver.model(), &node(sender), 1);
    context.gossip = deltas;
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    }));
}

/// Delivers an `Announce` about `target` from `sender`.
fn announce(
    driver: &mut Driver,
    sender: &str,
    announcement: Announcement,
    target: &str,
    incarnation: Incarnation,
) {
    let context = testing::peer_context(driver.model(), &node(sender), 7);
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Announce {
            announcement,
            target: node(target),
            incarnation,
        },
    }));
}

/// Gossip `driver` piggybacked on the messages of its last transition.
fn piggybacked(driver: &Driver) -> Vec<GossipDelta> {
    driver
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { message, .. } => Some(message.gossip.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Makes `driver` answer a probe, which is how its queued gossip travels, and
/// returns what that outbound message carried.
fn next_hop(driver: &mut Driver) -> Vec<GossipDelta> {
    relay(driver, "node-0002", Vec::new());
    piggybacked(driver)
}

/// The delta about `id` among `deltas`.
fn delta_about<'a>(deltas: &'a [GossipDelta], id: &str) -> Option<&'a GossipDelta> {
    deltas
        .iter()
        .find(|delta| delta.body.entity() == format!("member:{id}"))
}

fn liveness_of(driver: &Driver, id: &str) -> (Liveness, Incarnation) {
    let member = driver.model().member(&node(id)).expect("member");
    (member.liveness, member.incarnation)
}

fn conflicts(driver: &Driver) -> usize {
    driver
        .diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic, Diagnostic::VersionConflict { .. }))
        .count()
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

// ── Liveness across a dissemination hop ──────────────────────────────────

/// Carries a liveness change learned from an `Announce` one hop further.
///
/// A about C is the first hop; B hears the resulting full record. The record's
/// descriptive version is unchanged — liveness is not a description — so B
/// only converges if it orders the two projections separately.
fn propagate(announcement: Announcement, incarnation: Incarnation) -> Driver {
    let mut a = Driver::new(testing::model_with_members(4));
    announce(&mut a, "node-0000", announcement, "node-0001", incarnation);
    let carried = next_hop(&mut a);
    let record = delta_about(&carried, "node-0001")
        .expect("the node that adopted the announcement must gossip the result")
        .clone();
    assert_eq!(
        record.body.version(),
        &testing::member("node-0001", 1).version,
        "adopting an announcement must not invent a descriptive version"
    );

    let mut b = Driver::new(testing::model_with_members(4));
    relay(&mut b, "node-0000", vec![record]);
    b
}

#[test]
fn a_suspicion_learned_by_announce_survives_the_next_gossip_hop() {
    let b = propagate(Announcement::Suspect, Incarnation::INITIAL);

    assert_eq!(
        liveness_of(&b, "node-0001"),
        (Liveness::Suspected, Incarnation::INITIAL),
        "an equal descriptive version is not a reason to discard a suspicion"
    );
    assert_eq!(conflicts(&b), 0);
    assert!(
        b.published().iter().any(|change| matches!(
            change,
            ChangeRecord::LivenessChanged {
                node: subject,
                to: Liveness::Suspected,
                ..
            } if subject == &node("node-0001")
        )),
        "the second hop is news to B and must be published once"
    );
}

#[test]
fn a_death_learned_by_announce_survives_the_next_gossip_hop() {
    let b = propagate(Announcement::Dead, Incarnation::INITIAL);
    assert_eq!(
        liveness_of(&b, "node-0001"),
        (Liveness::Dead, Incarnation::INITIAL)
    );
    assert_eq!(conflicts(&b), 0);
}

#[test]
fn a_refutation_survives_the_next_gossip_hop() {
    // A holds a suspicion and hears the subject refute it at a newer
    // incarnation; B, which also suspects, must accept the refutation even
    // though the description did not move.
    let mut a = Driver::new(testing::model_with_members(4));
    testing::set_liveness(
        a.model_mut(),
        &node("node-0001"),
        Liveness::Suspected,
        Incarnation::INITIAL,
    );
    announce(
        &mut a,
        "node-0001",
        Announcement::Alive,
        "node-0001",
        Incarnation(1),
    );
    assert_eq!(
        liveness_of(&a, "node-0001"),
        (Liveness::Alive, Incarnation(1))
    );

    let carried = next_hop(&mut a);
    let record = delta_about(&carried, "node-0001")
        .expect("a refutation is news")
        .clone();

    let mut b = Driver::new(testing::model_with_members(4));
    testing::set_liveness(
        b.model_mut(),
        &node("node-0001"),
        Liveness::Suspected,
        Incarnation::INITIAL,
    );
    relay(&mut b, "node-0000", vec![record]);

    assert_eq!(
        liveness_of(&b, "node-0001"),
        (Liveness::Alive, Incarnation(1)),
        "a strictly newer incarnation refutes, whatever the descriptive version says"
    );
    assert_eq!(conflicts(&b), 0);
}

#[test]
fn a_suspicion_propagates_through_an_anti_entropy_exchange() {
    // The same record, carried by the other dissemination path. Anti-entropy
    // exists to repair what gossip missed, so it must apply the same merge.
    let mut responder = Driver::new(testing::model_with_members(4));
    announce(
        &mut responder,
        "node-0000",
        Announcement::Suspect,
        "node-0001",
        Incarnation::INITIAL,
    );

    let mut requester = Driver::new(testing::model_with_members(4));
    requester.apply(Message::Local(Command::StartAntiEntropyRound));
    requester.supply_peers(&["node-0002"]);
    let round = requester.model().anti_entropy().unwrap().round;
    let (digest, buckets, cursor) = requester
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            OutboundBody::PullRequest {
                digest,
                buckets,
                cursor,
                ..
            } => Some((digest.clone(), buckets.clone(), cursor.clone())),
            _ => None,
        })
        .expect("a round must send a PullRequest");

    let mut context = testing::peer_context(responder.model(), &node("node-0003"), 21);
    context.seq = 21;
    responder.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullRequest {
            round,
            digest,
            buckets,
            cursor,
        },
    }));
    let (deltas, complete) = responder
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            OutboundBody::PullReply {
                deltas, complete, ..
            } => Some((deltas.clone(), *complete)),
            _ => None,
        })
        .expect("a reply");
    assert!(
        delta_about(&deltas, "node-0001").is_some(),
        "the suspicion is exactly the divergence the round is repairing"
    );

    // Delivered with no piggybacked gossip, so only the anti-entropy payload
    // can be responsible for what the requester adopts.
    let mut context = testing::peer_context(requester.model(), &node("node-0002"), 22);
    context.seq = 22;
    requester.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::PullReply {
            round,
            digest: MembershipTree::build(requester.model()).digest(),
            deltas,
            complete,
            cursor: None,
        },
    }));

    assert_eq!(
        liveness_of(&requester, "node-0001"),
        (Liveness::Suspected, Incarnation::INITIAL)
    );
    assert_eq!(conflicts(&requester), 0);
}

// ── The two orderings, independently ─────────────────────────────────────

/// One row of the description/liveness matrix. The held record is
/// `node-0001` at version 4, 8 cores, `Alive(2)`.
struct Row {
    what: &'static str,
    incoming_version: u64,
    incoming_cores: u32,
    incoming_liveness: (Liveness, Incarnation),
    expected_version: u64,
    expected_cores: u32,
    expected_liveness: (Liveness, Incarnation),
    expected_conflicts: usize,
}

#[test]
fn description_and_liveness_are_ordered_independently() {
    let rows = [
        Row {
            what: "equal version, same description, winning liveness",
            incoming_version: 4,
            incoming_cores: 8,
            incoming_liveness: (Liveness::Suspected, Incarnation(2)),
            expected_version: 4,
            expected_cores: 8,
            expected_liveness: (Liveness::Suspected, Incarnation(2)),
            expected_conflicts: 0,
        },
        Row {
            what: "equal version, same description, stale liveness",
            incoming_version: 4,
            incoming_cores: 8,
            incoming_liveness: (Liveness::Alive, Incarnation(1)),
            expected_version: 4,
            expected_cores: 8,
            expected_liveness: (Liveness::Alive, Incarnation(2)),
            expected_conflicts: 0,
        },
        Row {
            what: "equal version, conflicting description, winning liveness",
            incoming_version: 4,
            incoming_cores: 64,
            incoming_liveness: (Liveness::Suspected, Incarnation(2)),
            expected_version: 4,
            expected_cores: 8,
            expected_liveness: (Liveness::Suspected, Incarnation(2)),
            expected_conflicts: 1,
        },
        Row {
            what: "equal version, conflicting description, stale liveness",
            incoming_version: 4,
            incoming_cores: 64,
            incoming_liveness: (Liveness::Alive, Incarnation(1)),
            expected_version: 4,
            expected_cores: 8,
            expected_liveness: (Liveness::Alive, Incarnation(2)),
            expected_conflicts: 1,
        },
        Row {
            what: "newer description, non-winning liveness",
            incoming_version: 9,
            incoming_cores: 64,
            incoming_liveness: (Liveness::Alive, Incarnation(1)),
            expected_version: 9,
            expected_cores: 64,
            expected_liveness: (Liveness::Alive, Incarnation(2)),
            expected_conflicts: 0,
        },
        Row {
            what: "older description, winning liveness",
            incoming_version: 2,
            incoming_cores: 64,
            incoming_liveness: (Liveness::Suspected, Incarnation(2)),
            expected_version: 4,
            expected_cores: 8,
            expected_liveness: (Liveness::Suspected, Incarnation(2)),
            expected_conflicts: 0,
        },
    ];

    for row in rows {
        let mut driver = Driver::new(testing::model_with_members(4));
        let mut current = testing::member("node-0001", 4);
        current.capabilities.cpu_cores = 8;
        current.liveness = Liveness::Alive;
        current.incarnation = Incarnation(2);
        testing::insert_member(driver.model_mut(), current);

        let mut incoming = testing::member("node-0001", row.incoming_version);
        incoming.capabilities.cpu_cores = row.incoming_cores;
        incoming.liveness = row.incoming_liveness.0;
        incoming.incarnation = row.incoming_liveness.1;
        gossip(
            &mut driver,
            "node-0000",
            vec![DeltaBody::MembershipUpdate(incoming)],
        );

        let held = driver.model().member(&node("node-0001")).unwrap().clone();
        assert_eq!(held.version.counter, row.expected_version, "{}", row.what);
        assert_eq!(
            held.capabilities.cpu_cores, row.expected_cores,
            "{}",
            row.what
        );
        assert_eq!(
            (held.liveness, held.incarnation),
            row.expected_liveness,
            "{}",
            row.what
        );
        assert_eq!(conflicts(&driver), row.expected_conflicts, "{}", row.what);

        // Whatever travels onward is what this node holds, never the record it
        // was handed: a partially adopted delta no node holds must not spread.
        let carried = piggybacked(&driver);
        let changed = held.version.counter != 4
            || held.capabilities.cpu_cores != 8
            || (held.liveness, held.incarnation) != (Liveness::Alive, Incarnation(2));
        match delta_about(&carried, "node-0001") {
            Some(delta) => {
                assert!(
                    changed,
                    "nothing changed, so nothing should travel: {}",
                    row.what
                );
                assert_eq!(
                    delta.body,
                    DeltaBody::MembershipUpdate(held),
                    "{}",
                    row.what
                );
            }
            None => assert!(
                !changed,
                "an adopted change must be disseminated: {}",
                row.what
            ),
        }
    }
}

/// Hop count of the queued delta about `id`.
fn queued_hops(driver: &Driver, id: &str) -> Option<u32> {
    driver
        .model()
        .gossip()
        .iter()
        .find(|(_, _, body)| body.entity() == format!("member:{id}"))
        .map(|(_, hops, _)| hops)
}

#[test]
fn an_exact_replay_does_not_restart_dissemination() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut updated = testing::member("node-0001", 5);
    updated.liveness = Liveness::Suspected;
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(updated.clone())],
    );
    assert_eq!(
        queued_hops(&driver, "node-0001"),
        Some(1),
        "the answering Ack carried the adopted record once"
    );

    // Replayed on an `Announce`, which the core answers with nothing, so only
    // the merge can touch the queue.
    let mut context = testing::peer_context(driver.model(), &node("node-0002"), 3);
    context.gossip = vec![GossipDelta {
        hops: 0,
        body: DeltaBody::MembershipUpdate(updated),
    }];
    driver.apply(Message::Peer(PeerInput {
        context,
        body: PeerBody::Announce {
            announcement: Announcement::Alive,
            target: node("node-0002"),
            incarnation: Incarnation(1),
        },
    }));

    assert!(driver.diagnostics.is_empty(), "{:?}", driver.diagnostics);
    assert!(driver.published().iter().all(|change| !matches!(
        change,
        ChangeRecord::LivenessChanged { node: subject, .. } if subject == &node("node-0001")
    )));
    assert_eq!(
        queued_hops(&driver, "node-0001"),
        Some(1),
        "a replay must not restart a delta already on its way to retirement"
    );
}

#[test]
fn description_and_liveness_news_converge_under_every_delivery_order() {
    // The two projections arrive on different deltas; neither disputes the
    // other, so every interleaving must end in the same place.
    let mut described = testing::member("node-0001", 9);
    described.endpoints.peers = vec![Address("10.7.7.7:6655".into())];

    let mut suspected = testing::member("node-0001", 1);
    suspected.liveness = Liveness::Suspected;
    suspected.incarnation = Incarnation(1);

    let deltas = vec![
        DeltaBody::MembershipUpdate(described),
        DeltaBody::MembershipUpdate(suspected),
    ];

    for order in permutations(&deltas) {
        let mut driver = Driver::new(testing::model_with_members(4));
        for delta in order.iter().chain(order.iter()) {
            gossip(&mut driver, "node-0000", vec![delta.clone()]);
        }
        let held = driver.model().member(&node("node-0001")).unwrap();
        assert_eq!(held.version.counter, 9);
        assert_eq!(held.endpoints.peers[0], Address("10.7.7.7:6655".into()));
        assert_eq!(
            (held.liveness, held.incarnation),
            (Liveness::Suspected, Incarnation(1)),
            "a suspicion must survive a re-description, in either order"
        );
        assert_eq!(conflicts(&driver), 0);
    }
}

// ── Safety fences, unchanged by independent ordering ─────────────────────

#[test]
fn a_mismatched_certificate_cannot_smuggle_in_a_liveness_update() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let mut impostor = testing::member("node-0001", 1);
    impostor.cert_fingerprint = testing::fingerprint(0xEE);
    impostor.liveness = Liveness::Dead;
    impostor.incarnation = Incarnation(9);
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(impostor)],
    );

    assert_eq!(
        liveness_of(&driver, "node-0001"),
        (Liveness::Alive, Incarnation::INITIAL),
        "a rebinding attempt is refused whole; its liveness is not salvaged"
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::CertificateMismatch { .. }]
    ));
    assert!(delta_about(&piggybacked(&driver), "node-0001").is_none());
}

#[test]
fn a_tombstone_outranks_any_liveness_claim() {
    let mut driver = Driver::new(testing::model_with_members(4));
    driver.apply(Message::Local(Command::RemoveMember {
        node: node("node-0001"),
        mode: RemovalMode::Force,
        reason: None,
    }));

    let mut resurrected = testing::member("node-0001", u64::MAX);
    resurrected.liveness = Liveness::Alive;
    resurrected.incarnation = Incarnation(u64::MAX);
    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(resurrected)],
    );

    assert!(driver.model().member(&node("node-0001")).is_none());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::TombstoneFenced { .. }]
    ));
}

#[test]
fn a_record_about_this_node_is_refuted_rather_than_adopted() {
    let mut driver = Driver::new(testing::model_with_members(4));
    let local = driver.model().member(&node("node-self")).unwrap().clone();
    let mut spoof = local.clone();
    spoof.liveness = Liveness::Suspected;
    spoof.incarnation = driver.model().incarnation();
    spoof.endpoints.peers = vec![Address("10.6.6.6:6655".into())];
    spoof.version = version(u64::MAX, "node-0000");

    gossip(
        &mut driver,
        "node-0000",
        vec![DeltaBody::MembershipUpdate(spoof)],
    );

    let held = driver.model().member(&node("node-self")).unwrap();
    assert_eq!(held.liveness, Liveness::Alive);
    assert!(
        held.incarnation > local.incarnation,
        "only this node may speak for itself, and it answers by refuting"
    );
    assert_eq!(
        held.endpoints, local.endpoints,
        "a peer cannot re-describe this node's own identity"
    );
    assert_eq!(conflicts(&driver), 0);
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
