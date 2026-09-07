//! Admission gates.
//!
//! Every gate in the runtime design's "member acceptance" section is evaluated
//! here, and the order matters. Gates that cost nothing — a formation
//! mismatch, a lock, a bounds violation — run before the core asks the shell
//! to do cryptographic work, so an unauthenticated flood is rejected without
//! ever reaching a token comparison.
//!
//! Splitting the evaluation in two is not a compromise of that ownership: the
//! core still makes the accept/reject decision atomically. What the shell
//! supplies is *evidence* it alone can establish — whether the transport
//! authenticated, whether the presented token matches the current one, whether
//! the source address falls in a blocked CIDR range. None of those can be
//! decided from a replayable model that deliberately holds no secrets and
//! parses no addresses.

use orishu_identity::{CertFingerprint, ProtocolVersion, WorkerName};

use crate::{
    effect::RejectReason,
    message::AdmissionEvidence,
    model::{Address, BlocklistKey, Liveness, Membership, NetworkPattern},
};

/// Gates that can be decided from the model alone, before any cryptography.
///
/// # Errors
///
/// Returns the first [`RejectReason`] that applies, in cheapest-first order.
pub(crate) fn evaluate_local_gates(
    model: &Membership,
    authenticated: bool,
    formation_matches: bool,
    protocol: ProtocolVersion,
    name: &WorkerName,
    fingerprint: CertFingerprint,
) -> Result<(), RejectReason> {
    if !authenticated {
        return Err(RejectReason::Unauthenticated);
    }
    if !formation_matches {
        return Err(RejectReason::FormationMismatch);
    }
    if !model.policy().protocol_range.accepts(protocol) {
        return Err(RejectReason::IncompatibleProtocol { offered: protocol });
    }
    if !model.policy().accepts_peers {
        return Err(RejectReason::NotAnIntroducer);
    }
    if model.membership_locked() {
        return Err(RejectReason::MembershipLocked);
    }

    // The blocklist fences by label and by certificate. It cannot fence by
    // assigned ID here, because an applicant has none yet — which is exactly
    // why a removed node is fenced by its *fingerprint* below rather than by
    // the ID it used to hold.
    for key in [
        BlocklistKey::Name(name.clone()),
        BlocklistKey::Fingerprint(fingerprint),
    ] {
        if model.is_blocked(&key) {
            return Err(RejectReason::Blocklisted { key });
        }
    }

    if model
        .tombstones()
        .values()
        .any(|tombstone| !tombstone.cleared && tombstone.cert_fingerprint == fingerprint)
    {
        return Err(RejectReason::Tombstoned);
    }

    if model.members().values().any(|member| {
        member.cert_fingerprint == fingerprint
            && matches!(
                member.liveness,
                crate::Liveness::Alive | crate::Liveness::Suspected
            )
    }) {
        return Err(RejectReason::AlreadyAdmitted);
    }

    if model.members().len() >= model.policy().capacity.min(model.limits().max_members()) {
        return Err(RejectReason::CapacityExhausted);
    }

    Ok(())
}

/// Gates that depend on facts only the shell can establish.
///
/// # Errors
///
/// Returns the [`RejectReason`] for the first failing fact.
pub(crate) fn evaluate_evidence(evidence: AdmissionEvidence) -> Result<(), RejectReason> {
    if !evidence.transport_authenticated {
        return Err(RejectReason::Unauthenticated);
    }
    if evidence.source_network_blocked {
        return Err(RejectReason::BlocklistedNetwork);
    }
    if !evidence.token_valid {
        return Err(RejectReason::InvalidToken);
    }
    Ok(())
}

/// Network patterns the shell should match an applicant's source address
/// against, bounded so one hostile blocklist cannot make verification
/// expensive.
pub(crate) fn blocked_networks(model: &Membership) -> Vec<NetworkPattern> {
    model
        .blocklist()
        .values()
        .filter(|entry| entry.action == crate::model::BlocklistAction::Block)
        .filter_map(|entry| match &entry.key {
            BlocklistKey::Network(pattern) => Some(pattern.clone()),
            _ => None,
        })
        .take(model.limits().max_blocklist_entries())
        .collect()
}

/// Other introducers worth suggesting when this node cannot admit.
///
/// Redirects are hints, not authority: they name alive members that advertise
/// `accepts.peers`, in deterministic ID order, bounded by
/// [`crate::Limits::max_redirects`]. An applicant may try them; nothing about
/// the list is a claim that they will succeed.
pub(crate) fn redirect_candidates(model: &Membership) -> Vec<Address> {
    model
        .members()
        .values()
        .filter(|member| member.id != *model.local_id())
        .filter(|member| member.liveness == Liveness::Alive && member.accepts.peers)
        .flat_map(|member| member.endpoints.peers.iter().cloned())
        .take(model.limits().max_redirects())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{AdmissionPolicy, BlocklistAction},
        testing,
    };
    use orishu_identity::{ProtocolRange, RemovalMode};

    fn applicant() -> (WorkerName, CertFingerprint) {
        (
            WorkerName::new("worker-alpha").unwrap(),
            CertFingerprint::from_bytes([0xA1; 32]),
        )
    }

    fn gates(model: &Membership) -> Result<(), RejectReason> {
        let (name, fingerprint) = applicant();
        evaluate_local_gates(
            model,
            true,
            true,
            ProtocolVersion::CURRENT,
            &name,
            fingerprint,
        )
    }

    #[test]
    fn a_healthy_introducer_admits() {
        assert_eq!(gates(&testing::standalone("node-self")), Ok(()));
    }

    #[test]
    fn unauthenticated_transport_is_refused_first() {
        // Even with every other gate also failing, authentication is reported:
        // the cheapest check must run first.
        let mut model = testing::standalone("node-self");
        model.set_policy(AdmissionPolicy {
            accepts_peers: false,
            capacity: 1,
            ..AdmissionPolicy::default()
        });
        let (name, fingerprint) = applicant();
        assert_eq!(
            evaluate_local_gates(
                &model,
                false,
                false,
                ProtocolVersion(99),
                &name,
                fingerprint
            ),
            Err(RejectReason::Unauthenticated)
        );
    }

    #[test]
    fn a_foreign_formation_is_refused() {
        let (name, fingerprint) = applicant();
        assert_eq!(
            evaluate_local_gates(
                &testing::standalone("node-self"),
                true,
                false,
                ProtocolVersion::CURRENT,
                &name,
                fingerprint
            ),
            Err(RejectReason::FormationMismatch)
        );
    }

    #[test]
    fn an_incompatible_protocol_is_refused() {
        let mut model = testing::standalone("node-self");
        model.set_policy(AdmissionPolicy {
            protocol_range: ProtocolRange::exact(ProtocolVersion(2)),
            ..AdmissionPolicy::default()
        });
        let (name, fingerprint) = applicant();
        assert_eq!(
            evaluate_local_gates(&model, true, true, ProtocolVersion(1), &name, fingerprint),
            Err(RejectReason::IncompatibleProtocol {
                offered: ProtocolVersion(1)
            })
        );
    }

    #[test]
    fn a_non_introducer_is_refused() {
        let mut model = testing::standalone("node-self");
        model.set_policy(AdmissionPolicy {
            accepts_peers: false,
            ..AdmissionPolicy::default()
        });
        assert_eq!(gates(&model), Err(RejectReason::NotAnIntroducer));
    }

    #[test]
    fn a_membership_lock_is_refused() {
        let mut model = testing::standalone("node-self");
        model = crate::update(
            model,
            crate::Message::Local(crate::Command::SetMembershipLock(true)),
        )
        .model;
        assert_eq!(gates(&model), Err(RejectReason::MembershipLocked));
    }

    #[test]
    fn a_blocklisted_label_is_refused() {
        let mut model = testing::standalone("node-self");
        let key = BlocklistKey::Name(WorkerName::new("worker-alpha").unwrap());
        model.blocklist_mut().insert(
            key.clone(),
            crate::model::BlocklistEntry {
                key: key.clone(),
                action: BlocklistAction::Block,
                version: testing::version(1),
                added_by: "operator".into(),
            },
        );
        assert_eq!(gates(&model), Err(RejectReason::Blocklisted { key }));
    }

    #[test]
    fn a_lifted_blocklist_entry_no_longer_refuses() {
        let mut model = testing::standalone("node-self");
        let key = BlocklistKey::Name(WorkerName::new("worker-alpha").unwrap());
        model.blocklist_mut().insert(
            key.clone(),
            crate::model::BlocklistEntry {
                key,
                action: BlocklistAction::Allow,
                version: testing::version(2),
                added_by: "operator".into(),
            },
        );
        assert_eq!(gates(&model), Ok(()));
    }

    #[test]
    fn a_blocklisted_fingerprint_is_refused() {
        let mut model = testing::standalone("node-self");
        let key = BlocklistKey::Fingerprint(CertFingerprint::from_bytes([0xA1; 32]));
        model.blocklist_mut().insert(
            key.clone(),
            crate::model::BlocklistEntry {
                key: key.clone(),
                action: BlocklistAction::Block,
                version: testing::version(1),
                added_by: "operator".into(),
            },
        );
        assert_eq!(gates(&model), Err(RejectReason::Blocklisted { key }));
    }

    #[test]
    fn a_removed_identity_is_fenced_by_its_certificate() {
        // The applicant has no assigned ID to fence on, so the tombstone's
        // pinned fingerprint is what stops a removed node walking back in
        // under a fresh label.
        let mut model = testing::standalone("node-self");
        let mut tombstone = testing::tombstone("node-removed", RemovalMode::Force);
        tombstone.cert_fingerprint = CertFingerprint::from_bytes([0xA1; 32]);
        model
            .tombstones_mut()
            .insert(tombstone.node_id.clone(), tombstone);
        assert_eq!(gates(&model), Err(RejectReason::Tombstoned));
    }

    #[test]
    fn a_cleared_tombstone_stops_fencing() {
        let mut model = testing::standalone("node-self");
        let mut tombstone = testing::tombstone("node-removed", RemovalMode::Force);
        tombstone.cert_fingerprint = CertFingerprint::from_bytes([0xA1; 32]);
        tombstone.cleared = true;
        model
            .tombstones_mut()
            .insert(tombstone.node_id.clone(), tombstone);
        assert_eq!(gates(&model), Ok(()));
    }

    #[test]
    fn capacity_is_refused_last_among_local_gates() {
        let mut model = testing::model_with_members(3);
        model.set_policy(AdmissionPolicy {
            capacity: 4,
            ..AdmissionPolicy::default()
        });
        assert_eq!(gates(&model), Err(RejectReason::CapacityExhausted));
    }

    #[test]
    fn a_lock_outranks_capacity() {
        let mut model = testing::model_with_members(3);
        model.set_policy(AdmissionPolicy {
            capacity: 4,
            ..AdmissionPolicy::default()
        });
        model = crate::update(
            model,
            crate::Message::Local(crate::Command::SetMembershipLock(true)),
        )
        .model;
        assert_eq!(gates(&model), Err(RejectReason::MembershipLocked));
    }

    #[test]
    fn evidence_gates_cover_each_shell_fact() {
        assert_eq!(
            evaluate_evidence(AdmissionEvidence {
                transport_authenticated: false,
                token_valid: true,
                source_network_blocked: false,
            }),
            Err(RejectReason::Unauthenticated)
        );
        assert_eq!(
            evaluate_evidence(AdmissionEvidence {
                transport_authenticated: true,
                token_valid: true,
                source_network_blocked: true,
            }),
            Err(RejectReason::BlocklistedNetwork)
        );
        assert_eq!(
            evaluate_evidence(AdmissionEvidence {
                transport_authenticated: true,
                token_valid: false,
                source_network_blocked: false,
            }),
            Err(RejectReason::InvalidToken)
        );
        assert_eq!(
            evaluate_evidence(AdmissionEvidence {
                transport_authenticated: true,
                token_valid: true,
                source_network_blocked: false,
            }),
            Ok(())
        );
    }

    #[test]
    fn redirects_name_only_alive_introducers_and_stay_bounded() {
        let mut model = testing::model_with_members(20);
        for (index, member) in model.members_mut().values_mut().enumerate() {
            member.accepts.peers = true;
            if index % 2 == 0 {
                member.liveness = Liveness::Dead;
            }
        }
        let candidates = redirect_candidates(&model);
        assert!(candidates.len() <= model.limits().max_redirects());
        assert!(!candidates.is_empty());
    }

    #[test]
    fn redirects_exclude_this_node() {
        let model = testing::standalone("node-self");
        assert!(redirect_candidates(&model).is_empty());
    }
}
