//! Admission: every gate, independently and in combination.
//!
//! No sockets, sleeps, clocks, entropy, or async runtime appear anywhere in
//! this file — the whole admission handshake is a sequence of values.

use orishu_membership::{
    Accepts, Address, AdmissionEvidence, AdmissionPolicy, Capabilities, ChangeRecord, Command,
    CredentialKind, Destination, Diagnostic, Effect, EffectOutcome, Liveness, Message,
    NodeCapacity, NodeId, OutboundBody, PeerBody, PeerInput, ProtocolRange, ProtocolVersion,
    RejectReason, RemovalMode, SessionId, WorkerName,
    message::PeerContext,
    model::{BlocklistAction, BlocklistEntry, BlocklistKey, Endpoints, NetworkPattern},
    testing::{self, Driver},
};

const SESSION: SessionId = SessionId(7);
const APPLICANT: &str = "worker-alpha";
const APPLICANT_SEED: u8 = 0xA1;

fn join_request() -> PeerBody {
    PeerBody::JoinRequest {
        endpoints: Endpoints {
            peers: vec![Address("192.168.1.10:6655".into())],
            clients: vec![],
        },
        accepts: Accepts::default(),
        capacity: NodeCapacity {
            peers: 32_768,
            clients: 1_024,
        },
        capabilities: testing::capabilities(),
    }
}

fn context(driver: &Driver) -> PeerContext {
    testing::applicant_context(
        driver.model().formation(),
        APPLICANT,
        SESSION,
        APPLICANT_SEED,
    )
}

/// Sends a `JoinReq` and returns the transition's outcome.
fn request_join(driver: &mut Driver) {
    let context = context(driver);
    driver.apply(Message::Peer(PeerInput {
        context,
        body: join_request(),
    }));
}

fn good_evidence() -> AdmissionEvidence {
    AdmissionEvidence {
        transport_authenticated: true,
        token_valid: true,
        source_network_blocked: false,
    }
}

/// Answers the pending `VerifyCredential` effect.
fn supply_evidence(driver: &mut Driver, evidence: AdmissionEvidence) {
    let request = driver
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::VerifyCredential { request, .. } => Some(*request),
            _ => None,
        })
        .expect("expected a credential verification request");
    driver.apply(Message::Outcome(EffectOutcome::CredentialVerified {
        request,
        session: SESSION,
        evidence,
    }));
}

/// Answers the pending `AllocateNodeId` effect with `assigned`.
fn supply_node_id(driver: &mut Driver, assigned: &str) {
    let request = driver
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::AllocateNodeId { request, .. } => Some(*request),
            _ => None,
        })
        .expect("expected a node ID allocation request");
    driver.apply(Message::Outcome(EffectOutcome::NodeIdAllocated {
        request,
        session: SESSION,
        node_id: NodeId::new(assigned).unwrap(),
    }));
}

fn rejection(driver: &Driver) -> Option<RejectReason> {
    driver.sent().into_iter().find_map(|(_, body)| match body {
        OutboundBody::JoinRejected { reason } => Some(reason.clone()),
        _ => None,
    })
}

// ── The happy path ───────────────────────────────────────────────────────

#[test]
fn a_complete_admission_verifies_then_allocates_then_inserts_then_replies() {
    let mut driver = Driver::new(testing::standalone("node-self"));

    request_join(&mut driver);
    // Nothing is admitted yet: the only effect is a request for evidence, and
    // that request carries no secret.
    assert_eq!(driver.model().members().len(), 1);
    match driver.effects.as_slice() {
        [
            Effect::VerifyCredential {
                session,
                kind: CredentialKind::JoinToken { name, .. },
                ..
            },
        ] => {
            assert_eq!(*session, SESSION);
            assert_eq!(name.as_str(), APPLICANT);
        }
        other => panic!("expected a single credential verification, got {other:?}"),
    }

    supply_evidence(&mut driver, good_evidence());
    // Valid evidence still admits nobody; an identity has to be generated
    // first, and generation can fail or collide.
    assert_eq!(driver.model().members().len(), 1);
    assert!(
        driver.sent().is_empty(),
        "no reply before an identity exists"
    );
    assert!(matches!(
        driver.effects.as_slice(),
        [Effect::AllocateNodeId { .. }]
    ));

    supply_node_id(&mut driver, "node-abc-123");
    let admitted = NodeId::new("node-abc-123").unwrap();
    let member = driver
        .model()
        .member(&admitted)
        .expect("the member must exist before acceptance is reported");
    assert_eq!(member.liveness, Liveness::Alive);
    assert_eq!(member.name.as_str(), APPLICANT);
    assert_eq!(
        member.cert_fingerprint,
        testing::fingerprint(APPLICANT_SEED)
    );

    let expected_formation = driver.model().formation().clone();
    match driver.sent().as_slice() {
        [
            (
                Destination::Session(session),
                OutboundBody::JoinAccepted {
                    formation,
                    assigned,
                    snapshot,
                    ..
                },
            ),
        ] => {
            assert_eq!(*session, SESSION);
            assert_eq!(*formation, expected_formation);
            assert_eq!(*assigned, admitted);
            assert!(
                snapshot.iter().any(|entry| entry.id == admitted),
                "the joiner must appear in its own bootstrap snapshot"
            );
        }
        other => panic!("expected exactly one JoinAccepted, got {other:?}"),
    }

    assert!(matches!(
        driver.published().as_slice(),
        [ChangeRecord::MemberAdmitted { node, .. }] if node == &admitted
    ));
}

#[test]
fn an_admitted_member_is_queued_for_gossip() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    request_join(&mut driver);
    supply_evidence(&mut driver, good_evidence());
    supply_node_id(&mut driver, "node-abc-123");
    assert_eq!(driver.model().gossip().len(), 1);
}

// ── Gates, one at a time ─────────────────────────────────────────────────

#[test]
fn an_unauthenticated_session_is_refused_without_a_reply() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let mut context = context(&driver);
    context.authenticated = false;
    driver.apply(Message::Peer(PeerInput {
        context,
        body: join_request(),
    }));

    // An unauthenticated peer gets nothing at all: replying would make the
    // node a reflector for whoever spoofed the session.
    assert!(driver.sent().is_empty());
    assert_eq!(driver.diagnostics, vec![Diagnostic::Unauthenticated]);
}

#[test]
fn a_request_naming_another_formation_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let mut context = context(&driver);
    context.formation = orishu_membership::FormationId::new("formation-other").unwrap();
    driver.apply(Message::Peer(PeerInput {
        context,
        body: join_request(),
    }));
    assert_eq!(rejection(&driver), Some(RejectReason::FormationMismatch));
}

#[test]
fn an_incompatible_protocol_is_refused() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    driver.apply(Message::Local(Command::SetPolicy(AdmissionPolicy {
        protocol_range: ProtocolRange::exact(ProtocolVersion(1)),
        ..AdmissionPolicy::default()
    })));
    let mut context = context(&driver);
    context.protocol = ProtocolVersion(9);
    driver.apply(Message::Peer(PeerInput {
        context,
        body: join_request(),
    }));
    assert_eq!(
        rejection(&driver),
        Some(RejectReason::IncompatibleProtocol {
            offered: ProtocolVersion(9)
        })
    );
}

#[test]
fn a_node_that_is_not_an_introducer_refuses() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    driver.apply(Message::Local(Command::SetPolicy(AdmissionPolicy {
        accepts_peers: false,
        ..AdmissionPolicy::default()
    })));
    request_join(&mut driver);
    assert_eq!(rejection(&driver), Some(RejectReason::NotAnIntroducer));
}

#[test]
fn a_membership_lock_refuses() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    driver.apply(Message::Local(Command::SetPolicy(AdmissionPolicy {
        membership_locked: true,
        ..AdmissionPolicy::default()
    })));
    request_join(&mut driver);
    assert_eq!(rejection(&driver), Some(RejectReason::MembershipLocked));
}

#[test]
fn a_stale_token_refuses_after_verification() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    request_join(&mut driver);
    supply_evidence(
        &mut driver,
        AdmissionEvidence {
            token_valid: false,
            ..good_evidence()
        },
    );
    assert_eq!(rejection(&driver), Some(RejectReason::InvalidToken));
    assert_eq!(driver.model().members().len(), 1);
    assert!(driver.model().admissions().is_empty());
}

#[test]
fn a_blocklisted_source_network_refuses_after_verification() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let key = BlocklistKey::Network(NetworkPattern("10.0.0.0/24".into()));
    testing::insert_blocklist(
        driver.model_mut(),
        BlocklistEntry {
            key,
            action: BlocklistAction::Block,
            version: testing::version(1),
            added_by: "operator-1".into(),
        },
    );

    request_join(&mut driver);
    // The patterns are handed to the shell, which owns address parsing.
    let patterns = driver
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::VerifyCredential {
                kind:
                    CredentialKind::JoinToken {
                        blocked_networks, ..
                    },
                ..
            } => Some(blocked_networks.clone()),
            _ => None,
        })
        .expect("verification request");
    assert_eq!(patterns, vec![NetworkPattern("10.0.0.0/24".into())]);

    supply_evidence(
        &mut driver,
        AdmissionEvidence {
            source_network_blocked: true,
            ..good_evidence()
        },
    );
    assert_eq!(rejection(&driver), Some(RejectReason::BlocklistedNetwork));
}

#[test]
fn a_blocklisted_identity_refuses_before_verification() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let key = BlocklistKey::Name(WorkerName::new(APPLICANT).unwrap());
    testing::insert_blocklist(
        driver.model_mut(),
        BlocklistEntry {
            key: key.clone(),
            action: BlocklistAction::Block,
            version: testing::version(1),
            added_by: "operator-1".into(),
        },
    );
    request_join(&mut driver);
    assert_eq!(rejection(&driver), Some(RejectReason::Blocklisted { key }));
    assert!(
        !driver
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::VerifyCredential { .. })),
        "a blocklisted applicant must not cost a token comparison"
    );
}

#[test]
fn a_tombstoned_identity_refuses() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let mut tombstone = testing::tombstone("node-was-removed", RemovalMode::Force);
    tombstone.cert_fingerprint = testing::fingerprint(APPLICANT_SEED);
    testing::insert_tombstone(driver.model_mut(), tombstone);
    request_join(&mut driver);
    assert_eq!(rejection(&driver), Some(RejectReason::Tombstoned));
}

#[test]
fn an_oversized_description_refuses() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let context = context(&driver);
    let body = PeerBody::JoinRequest {
        endpoints: Endpoints {
            peers: (0..64)
                .map(|i| Address(format!("10.0.0.{i}:6655")))
                .collect(),
            clients: vec![],
        },
        accepts: Accepts::default(),
        capacity: NodeCapacity::default(),
        capabilities: Capabilities::default(),
    };
    driver.apply(Message::Peer(PeerInput { context, body }));
    assert!(matches!(
        rejection(&driver),
        Some(RejectReason::MalformedRequest { .. })
    ));
}

// ── Capacity and redirects ───────────────────────────────────────────────

#[test]
fn a_full_formation_with_no_alternative_refuses() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    driver.apply(Message::Local(Command::SetPolicy(AdmissionPolicy {
        capacity: 1,
        ..AdmissionPolicy::default()
    })));
    request_join(&mut driver);
    assert_eq!(rejection(&driver), Some(RejectReason::CapacityExhausted));
}

#[test]
fn a_full_formation_redirects_to_bounded_alternatives() {
    let mut driver = Driver::new(testing::model_with_members(30));
    testing::make_all_introducers(driver.model_mut());
    driver.apply(Message::Local(Command::SetPolicy(AdmissionPolicy {
        capacity: 2,
        ..AdmissionPolicy::default()
    })));

    request_join(&mut driver);
    let candidates = driver
        .sent()
        .into_iter()
        .find_map(|(_, body)| match body {
            OutboundBody::JoinRedirect { candidates } => Some(candidates.clone()),
            _ => None,
        })
        .expect("expected a redirect");
    assert!(!candidates.is_empty());
    assert!(
        candidates.len() <= driver.model().limits().max_redirects(),
        "redirect hints must stay bounded"
    );
}

#[test]
fn admission_that_becomes_impossible_during_verification_is_refused() {
    // The shell's token comparison is not instantaneous. If the operator locks
    // membership while it runs, the applicant must not slip through on a
    // decision made before the lock.
    let mut driver = Driver::new(testing::standalone("node-self"));
    request_join(&mut driver);
    let request = driver
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::VerifyCredential { request, .. } => Some(*request),
            _ => None,
        })
        .expect("verification request");

    driver.apply(Message::Local(Command::SetPolicy(AdmissionPolicy {
        membership_locked: true,
        ..AdmissionPolicy::default()
    })));
    driver.apply(Message::Outcome(EffectOutcome::CredentialVerified {
        request,
        session: SESSION,
        evidence: good_evidence(),
    }));

    assert_eq!(rejection(&driver), Some(RejectReason::MembershipLocked));
    assert_eq!(driver.model().members().len(), 1);
}

// ── Identity generation ──────────────────────────────────────────────────

#[test]
fn a_colliding_generated_identity_admits_nobody() {
    let mut driver = Driver::new(testing::model_with_members(4));
    request_join(&mut driver);
    supply_evidence(&mut driver, good_evidence());

    let before = driver.model().members().len();
    supply_node_id(&mut driver, "node-0002");
    assert_eq!(rejection(&driver), Some(RejectReason::IdentityCollision));
    assert_eq!(
        driver.model().members().len(),
        before,
        "a collision must not overwrite the existing member"
    );
    assert!(driver.model().admissions().is_empty());
}

#[test]
fn a_generated_identity_matching_a_tombstone_admits_nobody() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    let tombstone = testing::tombstone("node-recycled", RemovalMode::Graceful);
    testing::insert_tombstone(driver.model_mut(), tombstone);
    request_join(&mut driver);
    supply_evidence(&mut driver, good_evidence());
    supply_node_id(&mut driver, "node-recycled");
    assert_eq!(rejection(&driver), Some(RejectReason::IdentityCollision));
}

#[test]
fn an_unavailable_identity_reports_overload() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    request_join(&mut driver);
    supply_evidence(&mut driver, good_evidence());
    let request = driver
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::AllocateNodeId { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    driver.apply(Message::Outcome(EffectOutcome::NodeIdUnavailable {
        request,
        session: SESSION,
    }));
    assert_eq!(rejection(&driver), Some(RejectReason::Overloaded));
}

#[test]
fn a_superseded_allocation_outcome_is_ignored() {
    let mut driver = Driver::new(testing::standalone("node-self"));
    request_join(&mut driver);
    supply_evidence(&mut driver, good_evidence());
    supply_node_id(&mut driver, "node-abc-123");

    // A second, late answer for the same session must not admit a second node.
    driver.apply(Message::Outcome(EffectOutcome::NodeIdAllocated {
        request: orishu_membership::AllocationId(999),
        session: SESSION,
        node_id: NodeId::new("node-late").unwrap(),
    }));
    assert!(
        driver
            .model()
            .member(&NodeId::new("node-late").unwrap())
            .is_none()
    );
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::UnknownCorrelation { .. }]
    ));
}

#[test]
fn too_many_pending_admissions_are_shed() {
    let spec = orishu_membership::LimitsSpec {
        max_pending_joins: 2,
        ..orishu_membership::LimitsSpec::default()
    };
    let limits = orishu_membership::Limits::try_from(spec).unwrap();
    let mut driver = Driver::new(testing::model_with_limits(0, limits));

    for index in 0..3u64 {
        let context = testing::applicant_context(
            driver.model().formation(),
            APPLICANT,
            SessionId(index),
            APPLICANT_SEED,
        );
        driver.apply(Message::Peer(PeerInput {
            context,
            body: join_request(),
        }));
    }
    assert_eq!(rejection(&driver), Some(RejectReason::Overloaded));
    assert_eq!(driver.model().admissions().len(), 2);
}

// ── Identity is not a label ──────────────────────────────────────────────

#[test]
fn a_request_claiming_an_assigned_identity_is_refused() {
    // Pre-admission traffic must identify itself by label and certificate. A
    // sender that arrives claiming an assigned `NodeId` is either confused or
    // trying to skip admission entirely.
    let mut driver = Driver::new(testing::standalone("node-self"));
    let mut context = context(&driver);
    context.sender =
        orishu_membership::SenderIdentity::Admitted(NodeId::new("node-fabricated").unwrap());
    driver.apply(Message::Peer(PeerInput {
        context,
        body: join_request(),
    }));
    assert!(driver.sent().is_empty());
    assert!(matches!(
        driver.diagnostics.as_slice(),
        [Diagnostic::Unexpected { .. }]
    ));
}

#[test]
fn two_applicants_sharing_a_label_get_distinct_identities() {
    // Names are non-unique by design, so admitting two workers called the same
    // thing must produce two members.
    let mut driver = Driver::new(testing::standalone("node-self"));
    for (session, seed, assigned) in [
        (SessionId(1), 0xA1, "node-one"),
        (SessionId(2), 0xB2, "node-two"),
    ] {
        let context =
            testing::applicant_context(driver.model().formation(), APPLICANT, session, seed);
        driver.apply(Message::Peer(PeerInput {
            context,
            body: join_request(),
        }));
        let request = driver
            .effects
            .iter()
            .find_map(|effect| match effect {
                Effect::VerifyCredential { request, .. } => Some(*request),
                _ => None,
            })
            .unwrap();
        driver.apply(Message::Outcome(EffectOutcome::CredentialVerified {
            request,
            session,
            evidence: good_evidence(),
        }));
        let request = driver
            .effects
            .iter()
            .find_map(|effect| match effect {
                Effect::AllocateNodeId { request, .. } => Some(*request),
                _ => None,
            })
            .unwrap();
        driver.apply(Message::Outcome(EffectOutcome::NodeIdAllocated {
            request,
            session,
            node_id: NodeId::new(assigned).unwrap(),
        }));
    }

    assert_eq!(driver.model().members().len(), 3);
    let names: Vec<_> = driver
        .model()
        .members()
        .values()
        .filter(|member| member.name.as_str() == APPLICANT)
        .collect();
    assert_eq!(names.len(), 2, "a shared label must not merge two members");
}
