//! Bounded worker-local operation bookkeeping. No retained secrets, network IO, clocks,
//! membership mutation or claim that a transport ACK completed adoption.
use orishu::model::cluster::{
    JoinOperation, JoinOperationState as State, JoinRecoveryReference, JoinRequest, OperationId,
    Participation, Summary,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CAPACITY: usize = 64;

struct Entry {
    request_hash: [u8; 32],
    operation: JoinOperation,
}

/// Process-lifetime records, retained across formation adoption. No eviction:
/// forgetting an outcome must never turn an old request into new work.
#[derive(Default)]
pub struct JoinOperations {
    entries: BTreeMap<OperationId, Entry>,
    active: Option<OperationId>,
}

/// Admission of an operator request into processing, not peer admission.
pub enum Reservation {
    Start(JoinOperation),
    Replay(JoinOperation),
}

/// Bounded, secret-free control failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OperationError {
    #[error("invalid join request")]
    Invalid,
    #[error("join operation ID already names a different request")]
    Conflict,
    #[error("join source formation is stale")]
    Stale,
    #[error("worker is not eligible for another join")]
    Busy,
    #[error("join operation history is full")]
    Full,
    #[error("unknown join operation")]
    Unknown,
    #[error("invalid join operation transition")]
    Transition,
}

impl JoinOperations {
    /// Explicit local lifecycle exit, not remote rollback. History is retained.
    pub fn end_lifecycle(&mut self) {
        if let Some(id) = self.active.take() {
            let entry = self.entries.get_mut(&id).expect("active record retained");
            entry.operation.state = match &entry.operation.state {
                State::Connecting => State::FailedBeforeAdmission,
                State::Admitting => State::Unresolved,
                State::CatchingUp { node_id } => State::CatchUpFailed {
                    node_id: node_id.clone(),
                },
                state => state.clone(),
            };
        }
    }
    /// Reserve at a serialized owner boundary. A replay precedes current-source
    /// checks so an accepted request remains recoverable after adoption.
    pub fn reserve(
        &mut self,
        request: &JoinRequest,
        source: &Summary,
    ) -> Result<Reservation, OperationError> {
        if request.schema_version != 1 || request.material.validate().is_err() {
            return Err(OperationError::Invalid);
        }
        let bytes = serde_json::to_vec(request).map_err(|_| OperationError::Invalid)?;
        let mut hash = Sha256::new();
        hash.update(b"orishu/join-operation-request/1\0");
        hash.update(bytes);
        let hash: [u8; 32] = hash.finalize().into();
        if let Some(entry) = self.entries.get(&request.operation_id) {
            return if entry.request_hash == hash {
                Ok(Reservation::Replay(entry.operation.clone()))
            } else {
                Err(OperationError::Conflict)
            };
        }
        if request.formation_id != source.formation_id {
            return Err(OperationError::Stale);
        }
        if self.active.is_some() || source.participation != Participation::Standalone {
            return Err(OperationError::Busy);
        }
        if request.formation_id == request.material.formation_id {
            return Err(OperationError::Invalid);
        }
        if self.entries.len() == CAPACITY {
            return Err(OperationError::Full);
        }
        let operation = JoinOperation {
            schema_version: 2,
            operation_id: request.operation_id.clone(),
            source_formation_id: source.formation_id.clone(),
            source_node_id: source.source_node_id.clone(),
            target_formation_id: request.material.formation_id.clone(),
            recovery_reference: None,
            state: State::Connecting,
        };
        self.active = Some(request.operation_id.clone());
        self.entries.insert(
            request.operation_id.clone(),
            Entry {
                request_hash: hash,
                operation: operation.clone(),
            },
        );
        Ok(Reservation::Start(operation))
    }

    /// Read a current local operation record, including historical source identity.
    pub fn get(&self, id: &OperationId) -> Option<&JoinOperation> {
        self.entries.get(id).map(|e| &e.operation)
    }

    /// Bind immutable correlation in the serialized owner turn that starts IO.
    /// No status reader can observe that turn's emitted work without its reference.
    pub fn bind_recovery(
        &mut self,
        id: &OperationId,
        reference: JoinRecoveryReference,
    ) -> Result<(), OperationError> {
        let entry = self.entries.get_mut(id).ok_or(OperationError::Unknown)?;
        if let Some(existing) = &entry.operation.recovery_reference {
            return if existing == &reference {
                Ok(())
            } else {
                Err(OperationError::Conflict)
            };
        }
        if self.active.as_ref() != Some(id)
            || !matches!(entry.operation.state, State::Connecting | State::Admitting)
        {
            return Err(OperationError::Transition);
        }
        entry.operation.recovery_reference = Some(reference);
        Ok(())
    }

    /// Apply only an owner-verified stage change. Once admission may have been
    /// emitted, a clean pre-admission failure is no longer an allowed outcome.
    pub fn advance(&mut self, id: &OperationId, next: State) -> Result<(), OperationError> {
        let entry = self.entries.get_mut(id).ok_or(OperationError::Unknown)?;
        if entry.operation.state == next {
            return Ok(());
        }
        if self.active.as_ref() != Some(id) {
            return Err(OperationError::Transition);
        }
        let allowed = match (&entry.operation.state, &next) {
            (State::Connecting, State::Admitting | State::FailedBeforeAdmission) => true,
            (State::Admitting, State::CatchingUp { .. } | State::Unresolved) => true,
            (
                State::CatchingUp { node_id: old },
                State::Joined { node_id: new } | State::CatchUpFailed { node_id: new },
            ) => old == new,
            (State::CatchUpFailed { node_id: old }, State::CatchingUp { node_id: new }) => {
                old == new
            }
            _ => false,
        };
        if !allowed {
            return Err(OperationError::Transition);
        }
        if matches!(next, State::FailedBeforeAdmission | State::Joined { .. }) {
            self.active = None;
        }
        entry.operation.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu::model::cluster::JoinMaterial;

    fn fixture() -> (JoinRequest, Summary) {
        let model = orishu_membership::testing::standalone("source");
        let summary = crate::runtime::project_summary(&model, Participation::Standalone);
        let request = JoinRequest {
            schema_version: 1,
            operation_id: "attempt-1".parse().unwrap(),
            formation_id: summary.formation_id.clone(),
            material: JoinMaterial {
                schema_version: 1,
                formation_id: "target".parse().unwrap(),
                introducer_node_id: "introducer".parse().unwrap(),
                introducer_fingerprint: model.local().cert_fingerprint,
                peer_endpoints: vec!["127.0.0.1:6655".into()],
                introducer_ready: true,
                token: "a".repeat(64).try_into().unwrap(),
            },
        };
        (request, summary)
    }

    #[test]
    fn recovery_reference_is_immutable_retained_and_required_on_the_wire() {
        let (request, summary) = fixture();
        let mut operations = JoinOperations::default();
        operations.reserve(&request, &summary).unwrap();
        let id = &request.operation_id;
        let reference = JoinRecoveryReference {
            attempt_id: "peer-attempt".parse().unwrap(),
            applicant_fingerprint: request.material.introducer_fingerprint,
            introducer_node_id: request.material.introducer_node_id.clone(),
            introducer_fingerprint: request.material.introducer_fingerprint,
        };
        let initial = serde_json::to_value(operations.get(id).unwrap()).unwrap();
        assert_eq!(initial["schemaVersion"], 2);
        assert!(initial["recoveryReference"].is_null());
        assert!(serde_json::from_value::<JoinOperation>(initial.clone()).is_ok());
        let mut missing = initial.clone();
        missing.as_object_mut().unwrap().remove("recoveryReference");
        assert!(serde_json::from_value::<JoinOperation>(missing).is_err());
        let mut old = initial;
        old["schemaVersion"] = 1.into();
        assert!(serde_json::from_value::<JoinOperation>(old).is_err());

        operations.bind_recovery(id, reference.clone()).unwrap();
        operations.advance(id, State::Admitting).unwrap();
        operations.end_lifecycle();
        assert_eq!(operations.get(id).unwrap().state, State::Unresolved);
        assert_eq!(
            operations.get(id).unwrap().recovery_reference,
            Some(reference.clone())
        );
        operations.bind_recovery(id, reference.clone()).unwrap();
        let mut conflict = reference;
        conflict.attempt_id = "different-attempt".parse().unwrap();
        assert_eq!(
            operations.bind_recovery(id, conflict),
            Err(OperationError::Conflict)
        );
        let encoded = serde_json::to_string(operations.get(id).unwrap()).unwrap();
        assert!(!encoded.contains(request.material.token.expose()));
        assert_eq!(
            serde_json::from_str::<JoinOperation>(&encoded).unwrap(),
            *operations.get(id).unwrap()
        );
        assert!(matches!(
            operations.reserve(&request, &summary),
            Ok(Reservation::Replay(_))
        ));
    }

    #[test]
    fn exact_replay_survives_adoption_but_conflicting_secret_is_rejected() {
        let (request, mut summary) = fixture();
        let mut operations = JoinOperations::default();
        assert!(matches!(
            operations.reserve(&request, &summary),
            Ok(Reservation::Start(_))
        ));
        operations
            .advance(&request.operation_id, State::Admitting)
            .unwrap();
        let node_id = "assigned".parse().unwrap();
        operations
            .advance(&request.operation_id, State::CatchingUp { node_id })
            .unwrap();
        summary.formation_id = request.material.formation_id.clone();
        summary.participation = Participation::CatchingUp;
        assert!(matches!(
            operations.reserve(&request, &summary),
            Ok(Reservation::Replay(JoinOperation {
                state: State::CatchingUp { .. },
                ..
            }))
        ));
        let mut conflict = request.clone();
        conflict.material.token = "b".repeat(64).try_into().unwrap();
        assert!(matches!(
            operations.reserve(&conflict, &summary),
            Err(OperationError::Conflict)
        ));
        assert!(!format!("{:?}", operations.get(&request.operation_id)).contains(&"a".repeat(64)));
    }

    #[test]
    fn emitted_or_adopted_work_cannot_be_relabelled_clean_failure() {
        let (request, summary) = fixture();
        let mut operations = JoinOperations::default();
        operations.reserve(&request, &summary).unwrap();
        operations
            .advance(&request.operation_id, State::Admitting)
            .unwrap();
        assert_eq!(
            operations.advance(&request.operation_id, State::FailedBeforeAdmission),
            Err(OperationError::Transition)
        );
        operations
            .advance(&request.operation_id, State::Unresolved)
            .unwrap();
        let mut another = request.clone();
        another.operation_id = "another".parse().unwrap();
        assert!(matches!(
            operations.reserve(&another, &summary),
            Err(OperationError::Busy)
        ));
        assert_eq!(
            operations.advance(&request.operation_id, State::Connecting),
            Err(OperationError::Transition)
        );
    }

    #[test]
    fn catchup_preserves_assigned_identity_and_history_never_evicts() {
        let (mut request, summary) = fixture();
        let mut operations = JoinOperations::default();
        for index in 0..CAPACITY {
            request.operation_id = format!("attempt-{index}").parse().unwrap();
            operations.reserve(&request, &summary).unwrap();
            operations
                .advance(&request.operation_id, State::FailedBeforeAdmission)
                .unwrap();
        }
        request.operation_id = "over-capacity".parse().unwrap();
        assert!(matches!(
            operations.reserve(&request, &summary),
            Err(OperationError::Full)
        ));
        request.operation_id = "attempt-0".parse().unwrap();
        assert!(matches!(
            operations.reserve(&request, &summary),
            Ok(Reservation::Replay(_))
        ));

        let mut operations = JoinOperations::default();
        operations.reserve(&request, &summary).unwrap();
        operations
            .advance(&request.operation_id, State::Admitting)
            .unwrap();
        let assigned = "assigned".parse().unwrap();
        operations
            .advance(
                &request.operation_id,
                State::CatchingUp { node_id: assigned },
            )
            .unwrap();
        assert_eq!(
            operations.advance(
                &request.operation_id,
                State::Joined {
                    node_id: "other".parse().unwrap()
                }
            ),
            Err(OperationError::Transition)
        );
        assert_eq!(
            operations.advance(&request.operation_id, State::FailedBeforeAdmission),
            Err(OperationError::Transition)
        );
    }
}
