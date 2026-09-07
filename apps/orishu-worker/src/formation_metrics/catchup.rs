//! IO-owned attempt observations. No credential, baseline or operation identity
//! is retained; a guard follows the existing reserved owner completion.
use super::FormationMetrics;
use crate::peer::catchup::client::Error;

/// Finite catch-up stages, not membership or introduction authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatchupEvent {
    /// Owner selected a source and created one authorized transfer job.
    Started,
    /// All pages, content proof and credential were validated by the receiver.
    TransferValidated,
    /// Receiver rejected source/certificate/formation binding.
    TransferBindingRejected,
    /// Receiver rejected schema, correlation, content proof or completeness.
    TransferInvalid,
    /// Source returned a valid structured refusal.
    TransferRejected,
    /// Shared exchange failed, timed out or refused capacity.
    TransferUnavailable,
    /// Receiver's whole-transfer deadline elapsed, not a per-exchange timeout.
    TransferTimedOut,
    /// Job was dropped before a receiver outcome, including before execution.
    TransferCancelled,
    /// Owner installed a safe baseline and credential and entered Joined.
    OwnerAdopted,
    /// Current eligible completion did not lead to safe Joined state.
    OwnerNotAdopted,
    /// Owner refused completion due to lifecycle, source/session or deadline fencing.
    OwnerFenced,
    /// No terminal owner decision was recorded before the observation was dropped.
    OwnerAbandoned,
}
impl CatchupEvent {
    /// Static exposition order with no labels or untrusted data.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 12] = [
        Self::Started,
        Self::TransferValidated,
        Self::TransferBindingRejected,
        Self::TransferInvalid,
        Self::TransferRejected,
        Self::TransferUnavailable,
        Self::TransferTimedOut,
        Self::TransferCancelled,
        Self::OwnerAdopted,
        Self::OwnerNotAdopted,
        Self::OwnerFenced,
        Self::OwnerAbandoned,
    ];

    /// Suffix and short accounting description for an unlabelled counter.
    #[cfg(feature = "observability")]
    pub fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::Started => ("started_total", "Owner-authorized catch-up jobs."),
            Self::TransferValidated => (
                "transfer_validated_total",
                "Complete receiver-validated transfers; not owner adoption.",
            ),
            Self::TransferBindingRejected => (
                "transfer_binding_rejected_total",
                "Receiver source or formation binding failures.",
            ),
            Self::TransferInvalid => (
                "transfer_invalid_total",
                "Receiver schema, correlation or baseline validation failures.",
            ),
            Self::TransferRejected => (
                "transfer_rejected_total",
                "Structured source refusals; not admission refusals.",
            ),
            Self::TransferUnavailable => (
                "transfer_unavailable_total",
                "Exchange failures, deadlines or capacity refusals.",
            ),
            Self::TransferTimedOut => (
                "transfer_timed_out_total",
                "Whole-transfer receiver deadlines.",
            ),
            Self::TransferCancelled => (
                "transfer_cancelled_total",
                "Jobs dropped before a receiver outcome.",
            ),
            Self::OwnerAdopted => (
                "owner_adopted_total",
                "Safe baseline and credential adopted into Joined state.",
            ),
            Self::OwnerNotAdopted => (
                "owner_not_adopted_total",
                "Eligible completions not adopted; includes failed or cancelled transfers.",
            ),
            Self::OwnerFenced => (
                "owner_fenced_total",
                "Completions refused by lifecycle, deadline or session fences.",
            ),
            Self::OwnerAbandoned => (
                "owner_abandoned_total",
                "Observations dropped without a recorded terminal owner decision.",
            ),
        }
    }
}

pub(crate) enum CatchupDecision {
    Adopted,
    NotAdopted,
    Fenced,
}

/// A zero-sized no-op without the feature; no new allocation when enabled.
pub(crate) struct CatchupObservation {
    #[cfg(feature = "observability")]
    metrics: FormationMetrics,
    #[cfg(feature = "observability")]
    transfer_pending: bool,
    #[cfg(feature = "observability")]
    decision_pending: bool,
}
impl FormationMetrics {
    pub(crate) fn start_catchup(&self) -> CatchupObservation {
        self.catchup_event(CatchupEvent::Started);
        CatchupObservation {
            #[cfg(feature = "observability")]
            metrics: self.clone(),
            #[cfg(feature = "observability")]
            transfer_pending: true,
            #[cfg(feature = "observability")]
            decision_pending: true,
        }
    }

    fn catchup_event(&self, event: CatchupEvent) {
        #[cfg(feature = "observability")]
        if let Some(counters) = &self.counters {
            super::add(&counters[9 + event as usize]);
        }
        let _ = event;
    }
}
impl CatchupObservation {
    /// Called at the real receiver result, before handing it to the owner.
    pub(crate) fn fetched(&mut self, result: Result<(), Error>) {
        let event = match result {
            Ok(()) => CatchupEvent::TransferValidated,
            Err(Error::Binding) => CatchupEvent::TransferBindingRejected,
            Err(Error::Invalid) => CatchupEvent::TransferInvalid,
            Err(Error::Rejected(_)) => CatchupEvent::TransferRejected,
            Err(Error::Transport) => CatchupEvent::TransferUnavailable,
            Err(Error::Timeout) => CatchupEvent::TransferTimedOut,
        };
        self.finish_transfer(event);
    }

    fn finish_transfer(&mut self, event: CatchupEvent) {
        #[cfg(feature = "observability")]
        if self.transfer_pending {
            self.transfer_pending = false;
            self.metrics.catchup_event(event);
        }
        let _ = event;
    }

    /// Drop-before-execution and cancelled futures still use the normal owner
    /// completion; record cancellation before handing that completion over.
    pub(crate) fn cancel_if_pending(&mut self) {
        self.finish_transfer(CatchupEvent::TransferCancelled);
    }

    pub(crate) fn decide(&mut self, decision: CatchupDecision) {
        #[cfg(feature = "observability")]
        if self.decision_pending {
            self.decision_pending = false;
            self.metrics.catchup_event(match decision {
                CatchupDecision::Adopted => CatchupEvent::OwnerAdopted,
                CatchupDecision::NotAdopted => CatchupEvent::OwnerNotAdopted,
                CatchupDecision::Fenced => CatchupEvent::OwnerFenced,
            });
        }
        let _ = decision;
    }
}
impl Drop for CatchupObservation {
    fn drop(&mut self) {
        self.cancel_if_pending();
        #[cfg(feature = "observability")]
        if self.decision_pending {
            self.metrics.catchup_event(CatchupEvent::OwnerAbandoned);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_or_disabled_catchup_collection_has_no_storage() {
        let metrics = FormationMetrics::default();
        let mut job = metrics.start_catchup();
        job.fetched(Err(Error::Invalid));
        job.decide(CatchupDecision::NotAdopted);
        drop(job);
        #[cfg(feature = "observability")]
        assert!(metrics.snapshot().is_none());
        #[cfg(not(feature = "observability"))]
        assert_eq!(std::mem::size_of::<CatchupObservation>(), 0);
    }

    #[cfg(feature = "observability")]
    #[test]
    fn transfer_and_decision_complete_once_and_preserve_validated_abandonment() {
        use CatchupEvent as E;
        let metrics = FormationMetrics::enabled();
        {
            let mut job = metrics.start_catchup();
            job.fetched(Ok(()));
            // Late disposal is not a cancelled transfer or owner adoption.
        }
        {
            let mut job = metrics.start_catchup();
            job.cancel_if_pending();
            job.cancel_if_pending();
            job.decide(CatchupDecision::NotAdopted);
            job.decide(CatchupDecision::Adopted);
        }
        {
            let mut job = metrics.start_catchup();
            job.fetched(Err(Error::Timeout));
            job.decide(CatchupDecision::Fenced);
        }
        for event in E::ALL {
            assert_eq!(
                metrics.snapshot().unwrap().get_catchup(event),
                match event {
                    E::Started => 3,
                    E::TransferValidated
                    | E::TransferCancelled
                    | E::TransferTimedOut
                    | E::OwnerAbandoned
                    | E::OwnerNotAdopted
                    | E::OwnerFenced => 1,
                    _ => 0,
                },
                "{event:?}"
            );
        }
    }

    #[cfg(feature = "observability")]
    #[test]
    fn catchup_counters_saturate_without_changing_registry_or_deadline_counts() {
        use std::sync::atomic::Ordering;
        let metrics = FormationMetrics::enabled();
        for event in CatchupEvent::ALL {
            metrics.counters.as_ref().unwrap()[9 + event as usize]
                .store(u64::MAX, Ordering::Relaxed);
            metrics.catchup_event(event);
            assert_eq!(metrics.snapshot().unwrap().get_catchup(event), u64::MAX);
        }
        for event in super::super::Event::ALL {
            assert_eq!(metrics.snapshot().unwrap().get(event), 0);
        }
        assert_eq!(
            metrics.snapshot().unwrap().registry_pressure(),
            Default::default()
        );
    }
}
