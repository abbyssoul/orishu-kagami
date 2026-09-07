//! Atomic merge of a complete, externally validated admission-state baseline.
//! Transfer completeness, authenticated source binding and credentials belong
//! to the IO shell; this module owns domain validation and version ordering.

use crate::{
    Diagnostic, FormationId, Membership, MembershipPolicy, MembershipTombstone,
    gossip::DeltaBody,
    merge::{self, MergeOutcome},
    model::BlocklistEntry,
};
use serde::{Deserialize, Serialize};

/// Public admission facts from one complete source snapshot. Never includes a
/// token, transport session, clock or readiness assertion. Empty collections
/// mean a complete empty source view, not permission to erase local records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdmissionBaseline {
    /// Version 1 of this transfer-to-core value.
    pub schema_version: u8,
    /// Immutable formation checked again at the transition boundary.
    pub formation_id: FormationId,
    /// Shell correlation for acceptance/rejection; not authorization.
    pub snapshot: u64,
    /// None represents the initial unlocked source policy.
    pub policy: Option<MembershipPolicy>,
    /// Complete blocklist, including lifts, strictly ordered by key.
    pub blocklist: Vec<BlocklistEntry>,
    /// Complete removal barriers, including clears, strictly ordered by node ID.
    pub tombstones: Vec<MembershipTombstone>,
}

/// Stage against a clone and return it only if every record is accepted or
/// superseded by a newer local record. Complexity is one bounded model copy
/// plus O(records * log(held records)); this is a cold bootstrap path.
pub(crate) fn merge(
    model: &Membership,
    baseline: AdmissionBaseline,
) -> Result<Membership, Diagnostic> {
    if baseline.schema_version != 1 {
        return Err(Diagnostic::Unexpected {
            what: "unsupported admission baseline schema",
        });
    }
    if &baseline.formation_id != model.formation() {
        return Err(Diagnostic::FormationMismatch {
            expected: model.formation().clone(),
            received: baseline.formation_id,
        });
    }
    for (limit, value, max) in [
        (
            "maxBlocklistEntries",
            baseline.blocklist.len(),
            model.limits().max_blocklist_entries(),
        ),
        (
            "maxTombstones",
            baseline.tombstones.len(),
            model.limits().max_tombstones(),
        ),
    ] {
        if value > max {
            return Err(Diagnostic::LimitExceeded { limit, value, max });
        }
    }
    if baseline
        .blocklist
        .windows(2)
        .any(|pair| pair[0].key >= pair[1].key)
        || baseline
            .tombstones
            .windows(2)
            .any(|pair| pair[0].node_id >= pair[1].node_id)
    {
        return Err(Diagnostic::Unexpected {
            what: "unordered or duplicate admission baseline key",
        });
    }
    let mut staged = model.clone();
    let limits = model.limits().clone();
    let deltas = baseline
        .policy
        .into_iter()
        .map(DeltaBody::MembershipPolicyUpdate)
        .chain(
            baseline
                .blocklist
                .into_iter()
                .map(DeltaBody::BlocklistUpdate),
        )
        .chain(
            baseline
                .tombstones
                .into_iter()
                .map(DeltaBody::TombstoneUpdate),
        );
    for delta in deltas {
        match merge::merge_delta(&mut staged, delta) {
            MergeOutcome::Adopted {
                body,
                diagnostic: None,
                ..
            } => {
                staged.gossip_mut().enqueue(body, &limits);
            }
            MergeOutcome::Idempotent | MergeOutcome::Rejected(Diagnostic::StaleDelta { .. }) => {}
            MergeOutcome::SelfRemoved { tombstone } => {
                staged
                    .gossip_mut()
                    .enqueue(DeltaBody::TombstoneUpdate(tombstone), &limits);
            }
            MergeOutcome::Rejected(reason)
            | MergeOutcome::Adopted {
                diagnostic: Some(reason),
                ..
            } => return Err(reason),
            MergeOutcome::SelfChallenged { .. } => {
                return Err(Diagnostic::Unexpected {
                    what: "liveness challenge in admission-only baseline",
                });
            }
        }
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ChangeRecord, Command, Effect, Limits, LimitsSpec, Message, RemovalMode, testing, update,
    };

    fn input(model: &Membership) -> AdmissionBaseline {
        AdmissionBaseline {
            schema_version: 1,
            formation_id: model.formation().clone(),
            snapshot: 7,
            policy: Some(MembershipPolicy {
                locked: true,
                version: testing::version(1),
            }),
            blocklist: vec![testing::blocklist_entry("blocked")],
            tombstones: vec![],
        }
    }

    fn apply(model: Membership, baseline: AdmissionBaseline) -> crate::Transition {
        update(
            model,
            Message::Local(Command::InstallAdmissionBaseline(baseline)),
        )
    }

    #[test]
    fn complete_baseline_merges_and_replay_preserves_model_and_local_policy() {
        let model = testing::standalone("self");
        let local_policy = model.policy().clone();
        let baseline = input(&model);
        let transition = apply(model, baseline.clone());
        assert_eq!(
            transition.effects,
            vec![Effect::Publish(ChangeRecord::AdmissionBaselineApplied {
                snapshot: 7,
                self_removed: false
            })]
        );
        assert!(transition.model.membership_locked());
        assert_eq!(transition.model.blocklist().len(), 1);
        assert_eq!(transition.model.policy(), &local_policy);
        let replay = apply(transition.model.clone(), baseline);
        assert_eq!(replay.model, transition.model);
        assert!(replay.diagnostics.is_empty());
    }

    #[test]
    fn late_invalid_record_rolls_back_policy_blocklist_gossip_and_removal() {
        let model = testing::model_with_members(2);
        let mut baseline = input(&model);
        let mut invalid = testing::tombstone("node-0001", RemovalMode::Force);
        invalid.reason = Some("x".repeat(model.limits().max_text_len() + 1));
        baseline.tombstones = vec![testing::tombstone("node-0000", RemovalMode::Force), invalid];
        let transition = apply(model.clone(), baseline);
        assert_eq!(transition.model, model);
        assert_eq!(
            transition.effects,
            vec![Effect::Publish(ChangeRecord::AdmissionBaselineRejected {
                snapshot: 7
            })]
        );
        assert!(matches!(
            transition.diagnostics.as_slice(),
            [Diagnostic::MalformedRecord { .. }]
        ));
    }

    #[test]
    fn newer_local_restrictions_survive_stale_and_empty_source_views() {
        let model = testing::standalone("self");
        let mut newer = input(&model);
        newer.policy.as_mut().unwrap().version = testing::version(5);
        newer.blocklist[0].version = testing::version(5);
        let current = apply(model, newer).model;
        let mut older = input(&current);
        older.policy.as_mut().unwrap().locked = false;
        older.blocklist[0].action = crate::model::BlocklistAction::Allow;
        assert_eq!(apply(current.clone(), older).model, current);
        let empty = AdmissionBaseline {
            policy: None,
            blocklist: vec![],
            ..input(&current)
        };
        assert_eq!(apply(current.clone(), empty).model, current);
    }

    #[test]
    fn conflict_wrong_identity_duplicates_and_union_capacity_are_atomic() {
        let limits = Limits::try_from(LimitsSpec {
            max_blocklist_entries: 1,
            ..Default::default()
        })
        .unwrap();
        let initial = testing::model_with_limits(0, limits);
        let current = apply(initial.clone(), input(&initial)).model;
        let mut conflict = input(&current);
        conflict.blocklist[0].action = crate::model::BlocklistAction::Allow;
        let mut wrong = input(&current);
        wrong.formation_id = "other".parse().unwrap();
        let mut full = input(&current);
        full.blocklist = vec![testing::blocklist_entry("another")];
        let mut duplicate = input(&current);
        duplicate.blocklist.push(duplicate.blocklist[0].clone());
        for rejected in [conflict, wrong, full, duplicate] {
            let result = apply(current.clone(), rejected);
            assert_eq!(result.model, current);
            assert!(matches!(
                result.effects.as_slice(),
                [Effect::Publish(
                    ChangeRecord::AdmissionBaselineRejected { .. }
                )]
            ));
        }
    }

    #[test]
    fn self_removal_is_explicit_and_never_readiness() {
        let model = testing::standalone("self");
        let mut baseline = input(&model);
        baseline
            .tombstones
            .push(testing::tombstone("self", RemovalMode::Force));
        let result = apply(model, baseline);
        assert!(!result.model.members().contains_key(result.model.local_id()));
        assert!(matches!(
            result.effects.as_slice(),
            [Effect::Publish(ChangeRecord::AdmissionBaselineApplied {
                self_removed: true,
                ..
            })]
        ));
    }
}
