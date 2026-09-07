//! Formation-lifetime accepted assignments. No secrets, snapshots, eviction or
//! domain authority: the owner revalidates current membership before replay.

use super::wire::JoinAttempt;
use orishu_membership::{CertFingerprint, NodeId};
use std::collections::BTreeMap;

const CAPACITY: usize = 1024;

#[derive(Clone)]
pub(crate) struct Accepted {
    digest: [u8; 32],
    pub assigned: NodeId,
    pub seq: u64,
}

#[derive(Default)]
pub(crate) struct Ledger(BTreeMap<(CertFingerprint, NodeId), Accepted>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    Conflict,
    Full,
}

impl Ledger {
    /// Read retained evidence without admitting, checking a payload or consuming capacity.
    pub fn assigned(&self, fingerprint: CertFingerprint, attempt: &NodeId) -> Option<&NodeId> {
        self.0
            .get(&(fingerprint, attempt.clone()))
            .map(|record| &record.assigned)
    }
    /// Inspect before admission in the same serialized owner turn. Exact
    /// accepted retries remain recoverable even when the ledger is full.
    pub fn lookup(
        &self,
        fingerprint: CertFingerprint,
        attempt: &JoinAttempt,
    ) -> Result<Option<Accepted>, Error> {
        if let Some(record) = self.0.get(&(fingerprint, attempt.id.clone())) {
            return if record.digest == attempt.digest {
                Ok(Some(record.clone()))
            } else {
                Err(Error::Conflict)
            };
        }
        if self.0.len() == CAPACITY {
            Err(Error::Full)
        } else {
            Ok(None)
        }
    }

    /// Record insertion before sending its ACK. Lookup and admission must not
    /// yield to another owner turn; no post-insertion allocation race is allowed.
    pub fn commit(
        &mut self,
        fingerprint: CertFingerprint,
        attempt: JoinAttempt,
        assigned: NodeId,
        seq: u64,
    ) -> Result<(), Error> {
        if let Some(record) = self.lookup(fingerprint, &attempt)? {
            return if record.assigned == assigned && record.seq == seq {
                Ok(())
            } else {
                Err(Error::Conflict)
            };
        }
        self.0.insert(
            (fingerprint, attempt.id),
            Accepted {
                digest: attempt.digest,
                assigned,
                seq,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::testing;

    #[test]
    fn accepted_retries_survive_capacity_without_eviction_or_reexecution() {
        let mut ledger = Ledger::default();
        let fingerprint = testing::fingerprint(1);
        for index in 0..CAPACITY {
            let attempt = JoinAttempt {
                id: format!("attempt-{index}").parse().unwrap(),
                digest: [1; 32],
            };
            ledger
                .commit(
                    fingerprint,
                    attempt,
                    format!("node-{index}").parse().unwrap(),
                    index as u64,
                )
                .unwrap();
        }
        let attempt = JoinAttempt {
            id: "attempt-0".parse().unwrap(),
            digest: [1; 32],
        };
        let record = ledger.lookup(fingerprint, &attempt).unwrap().unwrap();
        assert_eq!(
            ledger.assigned(fingerprint, &attempt.id),
            Some(&record.assigned)
        );
        assert!(
            ledger
                .assigned(testing::fingerprint(2), &attempt.id)
                .is_none()
        );
        assert!(
            ledger
                .assigned(fingerprint, &"missing".parse().unwrap())
                .is_none()
        );
        assert_eq!(record.assigned, "node-0".parse().unwrap());
        ledger
            .commit(fingerprint, attempt.clone(), record.assigned, record.seq)
            .unwrap();
        assert!(matches!(
            ledger.lookup(
                fingerprint,
                &JoinAttempt {
                    digest: [2; 32],
                    ..attempt.clone()
                }
            ),
            Err(Error::Conflict)
        ));
        assert!(matches!(
            ledger.lookup(testing::fingerprint(2), &attempt),
            Err(Error::Full)
        ));
        assert!(matches!(
            ledger.lookup(
                fingerprint,
                &JoinAttempt {
                    id: "new-attempt".parse().unwrap(),
                    ..attempt
                }
            ),
            Err(Error::Full)
        ));
        assert_eq!(ledger.0.len(), CAPACITY);
    }
}
