//! Optional owner-side deadline and registry observations, never core authority.
//! No identity, timer generation, diagnostic payload or timing clock is retained.
use orishu_membership::{Diagnostic, TimerToken};
mod catchup;
pub use catchup::CatchupEvent;
pub(crate) use catchup::{CatchupDecision, CatchupObservation};
#[cfg(feature = "observability")]
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Finite semantic events. Deadline consumption is distinct from peer death.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A current direct/relay probe timer was consumed by the core.
    DirectProbeDeadline,
    /// A current indirect probe timer was consumed by the core.
    IndirectProbeDeadline,
    /// A current suspicion timer was consumed; inspect actual liveness separately.
    SuspicionDeadline,
    /// A current reconciliation timer was consumed by the core.
    AntiEntropyDeadline,
    /// A current join retry/backoff timer was consumed; not an admission refusal.
    JoinRetryDue,
    /// Core rejected a timer token as stale; excludes owner generation refusal.
    StaleTimerInput,
    /// Core emitted `JoinAbandoned`, whether on timer or another retry trigger.
    JoinAbandoned,
    /// Core emitted `AntiEntropyAbandoned`, on expiry or continuation exhaustion.
    AntiEntropyAbandoned,
}
impl Event {
    /// Fixed catalogue order; each entry is an unlabelled saturating counter.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 8] = [
        Self::DirectProbeDeadline,
        Self::IndirectProbeDeadline,
        Self::SuspicionDeadline,
        Self::AntiEntropyDeadline,
        Self::JoinRetryDue,
        Self::StaleTimerInput,
        Self::JoinAbandoned,
        Self::AntiEntropyAbandoned,
    ];

    /// Static metric suffix and help; no fields of an untrusted diagnostic.
    #[cfg(feature = "observability")]
    pub fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::DirectProbeDeadline => (
                "direct_probe_deadlines_total",
                "Current direct or relay probe timers consumed.",
            ),
            Self::IndirectProbeDeadline => (
                "indirect_probe_deadlines_total",
                "Current indirect probe timers consumed.",
            ),
            Self::SuspicionDeadline => (
                "suspicion_deadlines_total",
                "Current suspicion timers consumed; not peer-death authority.",
            ),
            Self::AntiEntropyDeadline => (
                "anti_entropy_deadlines_total",
                "Current reconciliation timers consumed.",
            ),
            Self::JoinRetryDue => (
                "join_retry_due_total",
                "Current join retry timers consumed; not admission refusals.",
            ),
            Self::StaleTimerInput => (
                "stale_timer_inputs_total",
                "Timer inputs rejected by the core as stale.",
            ),
            Self::JoinAbandoned => (
                "join_abandoned_total",
                "Core join retry budgets exhausted; acceptance may remain unknown.",
            ),
            Self::AntiEntropyAbandoned => (
                "anti_entropy_abandoned_total",
                "Core reconciliation rounds abandoned on expiry or round limit.",
            ),
        }
    }
}

/// One optional fixed startup allocation shared with read-only process handles.
/// Disabled collection allocates nothing; compiled-out collection is zero-sized.
#[derive(Clone, Default)]
pub struct FormationMetrics {
    #[cfg(feature = "observability")]
    // Eight deadline counters, one registry reading, twelve catch-up counters.
    counters: Option<Arc<[AtomicU64; 21]>>,
}
impl FormationMetrics {
    /// Enable before owner startup; does not enable any listener or exporter.
    #[cfg(feature = "observability")]
    pub fn enabled() -> Self {
        Self {
            counters: Some(Arc::new(std::array::from_fn(|_| AtomicU64::new(0)))),
        }
    }
    /// Bounded independent atomic reads, including after owner shutdown.
    #[cfg(feature = "observability")]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.counters.as_ref().map(|c| Snapshot {
            values: std::array::from_fn(|i| c[i].load(Ordering::Relaxed)),
            registry: c[8].load(Ordering::Relaxed),
            catchup: std::array::from_fn(|i| c[9 + i].load(Ordering::Relaxed)),
        })
    }

    /// Publish only trusted registry counts, never admission authority. One
    /// atomic packs four bounded octets; this is not a wire/persisted format.
    #[cfg(feature = "observability")]
    pub(crate) fn registry(
        &self,
        used: usize,
        capacity: usize,
        provisional: usize,
        provisional_capacity: usize,
    ) {
        if let Some(c) = &self.counters {
            assert!(used <= capacity && capacity <= 255);
            assert!(provisional <= used && provisional <= provisional_capacity);
            assert!(provisional_capacity <= capacity);
            let value = used as u64
                | ((capacity as u64) << 8)
                | ((provisional as u64) << 16)
                | ((provisional_capacity as u64) << 24);
            c[8].store(value, Ordering::Relaxed);
        }
    }

    /// Observe only an actual completed core transition. Input must be the timer
    /// passed to that update, if any, with its bounded returned diagnostics.
    /// O(diagnostics), already bounded by the core, and O(1) retained space.
    pub(crate) fn record(&self, timer: Option<TimerToken>, diagnostics: &[Diagnostic]) {
        #[cfg(feature = "observability")]
        if let Some(c) = &self.counters {
            let mut stale = false;
            for diagnostic in diagnostics {
                match diagnostic {
                    Diagnostic::StaleTimer { token } => {
                        stale |= Some(*token) == timer;
                        add(&c[Event::StaleTimerInput as usize]);
                    }
                    Diagnostic::JoinAbandoned { .. } => add(&c[Event::JoinAbandoned as usize]),
                    Diagnostic::AntiEntropyAbandoned { .. } => {
                        add(&c[Event::AntiEntropyAbandoned as usize])
                    }
                    _ => {}
                }
            }
            if let Some(timer) = timer.filter(|_| !stale) {
                use orishu_membership::TimerKind;
                let event = match timer.kind {
                    TimerKind::DirectProbe => Event::DirectProbeDeadline,
                    TimerKind::IndirectProbe => Event::IndirectProbeDeadline,
                    TimerKind::Suspicion => Event::SuspicionDeadline,
                    TimerKind::AntiEntropy => Event::AntiEntropyDeadline,
                    TimerKind::JoinRetry => Event::JoinRetryDue,
                };
                add(&c[event as usize]);
            }
        }
        #[cfg(not(feature = "observability"))]
        let _ = (timer, diagnostics);
    }
}

#[cfg(feature = "observability")]
fn add(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}

/// Process-lifetime saturating counts; independent readings are not a transaction.
#[cfg(feature = "observability")]
pub struct Snapshot {
    values: [u64; 8],
    registry: u64,
    catchup: [u64; 12],
}
#[cfg(feature = "observability")]
impl Snapshot {
    /// Read one event, without exposing any input or diagnostic payload.
    pub fn get(&self, event: Event) -> u64 {
        self.values[event as usize]
    }
    /// Catch-up transfer and owner-decision stages are independent counters.
    pub fn get_catchup(&self, event: CatchupEvent) -> u64 {
        self.catchup[event as usize]
    }
    /// Retained registry slots, not open connections or admitted members.
    pub fn registry_pressure(&self) -> RegistryPressure {
        RegistryPressure {
            slots_in_use: self.registry & 255,
            slots_capacity: (self.registry >> 8) & 255,
            provisional_slots_in_use: (self.registry >> 16) & 255,
            provisional_slots_capacity: (self.registry >> 24) & 255,
        }
    }
}

/// One coherent registry reading; separate from all other metrics and health.
#[cfg(feature = "observability")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RegistryPressure {
    /// Retained entries, including closed/expired entries awaiting pruning.
    pub slots_in_use: u64,
    /// Registry bound while its owner exists; zero before startup/after drop.
    pub slots_capacity: u64,
    /// Retained provisional entries: incoming applicants or outgoing introducers.
    pub provisional_slots_in_use: u64,
    /// Provisional subset limit while the registry exists; otherwise zero.
    pub provisional_slots_capacity: u64,
}

#[cfg(test)]
pub(crate) fn test_owner(
    model: orishu_membership::Membership,
) -> (
    crate::driver::Handle,
    tokio::task::JoinHandle<Result<(), crate::driver::DriverError>>,
) {
    #[cfg(feature = "observability")]
    let metrics = FormationMetrics::enabled();
    #[cfg(not(feature = "observability"))]
    let metrics = FormationMetrics::default();
    crate::driver::spawn_standalone_observed(
        model,
        None,
        Default::default(),
        Default::default(),
        metrics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "observability")]
    use orishu_membership::TimerKind;

    #[test]
    fn disabled_collection_retains_no_storage() {
        let metrics = FormationMetrics::default();
        metrics.record(None, &[Diagnostic::JoinAbandoned { attempts: 1 }]);
        #[cfg(feature = "observability")]
        {
            metrics.registry(16, 64, 16, 16);
            assert!(metrics.snapshot().is_none());
        }
        #[cfg(not(feature = "observability"))]
        assert_eq!(std::mem::size_of::<FormationMetrics>(), 0);
    }

    #[cfg(feature = "observability")]
    #[test]
    fn registry_snapshot_is_coherent_and_does_not_reset_events() {
        let metrics = FormationMetrics::enabled();
        assert_eq!(
            metrics.snapshot().unwrap().registry_pressure(),
            Default::default()
        );
        metrics.record(None, &[Diagnostic::JoinAbandoned { attempts: 1 }]);
        let writer = metrics.clone();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                for _ in 0..10_000 {
                    writer.registry(16, 64, 16, 16);
                    writer.registry(64, 64, 0, 16);
                    writer.registry(0, 0, 0, 0);
                }
            });
            for _ in 0..10_000 {
                let reading = metrics.snapshot().unwrap().registry_pressure();
                assert!(matches!(
                    (
                        reading.slots_in_use,
                        reading.slots_capacity,
                        reading.provisional_slots_in_use,
                        reading.provisional_slots_capacity
                    ),
                    (0, 0, 0, 0) | (16, 64, 16, 16) | (64, 64, 0, 16)
                ));
            }
        });
        assert_eq!(
            metrics.snapshot().unwrap().registry_pressure(),
            Default::default()
        );
        assert_eq!(metrics.snapshot().unwrap().get(Event::JoinAbandoned), 1);
    }

    #[cfg(feature = "observability")]
    #[test]
    fn real_core_rejects_unowned_timers_without_counting_deadlines() {
        let metrics = FormationMetrics::enabled();
        for kind in [
            TimerKind::DirectProbe,
            TimerKind::IndirectProbe,
            TimerKind::Suspicion,
            TimerKind::AntiEntropy,
            TimerKind::JoinRetry,
        ] {
            let token = TimerToken {
                kind,
                generation: u64::MAX,
            };
            let transition = orishu_membership::update(
                orishu_membership::testing::standalone("stale"),
                orishu_membership::Message::Timer(token),
            );
            assert!(
                matches!(transition.diagnostics.as_slice(), [Diagnostic::StaleTimer { token: actual }] if *actual == token)
            );
            metrics.record(Some(token), &transition.diagnostics);
        }
        for event in Event::ALL {
            assert_eq!(
                metrics.snapshot().unwrap().get(event),
                if event == Event::StaleTimerInput {
                    5
                } else {
                    0
                }
            );
        }
    }

    #[cfg(feature = "observability")]
    #[test]
    fn real_core_abandonment_without_timer_is_not_deadline_expiry() {
        use orishu_membership::{
            Command, Limits, LimitsSpec, Message, PeerBody, PeerInput, SessionId,
            antientropy::MembershipTree,
            model::AntiEntropyCursor,
            testing::{self, Driver},
        };
        let metrics = FormationMetrics::enabled();
        let mut driver = Driver::new(testing::model_with_members(2));
        driver.apply(Message::Local(Command::StartAntiEntropyRound));
        driver.supply_peers(&["node-0001"]);
        let maximum = driver.model().limits().max_anti_entropy_rounds();
        for exchange in 0..maximum {
            let round = driver.model().anti_entropy().unwrap().round;
            let context = testing::peer_context(
                driver.model(),
                &"node-0001".parse().unwrap(),
                100 + u64::from(exchange),
            );
            let digest = MembershipTree::build(driver.model()).digest();
            driver.apply(Message::Peer(PeerInput {
                context,
                body: PeerBody::PullReply {
                    round,
                    digest,
                    deltas: vec![],
                    complete: false,
                    cursor: Some(AntiEntropyCursor {
                        bucket: 0,
                        after_key: vec![],
                    }),
                },
            }));
            metrics.record(None, &driver.diagnostics);
        }
        assert!(driver.model().anti_entropy().is_none());
        assert_eq!(
            metrics.snapshot().unwrap().get(Event::AntiEntropyAbandoned),
            1
        );

        let mut joiner = Driver::new(testing::model_with_limits(
            0,
            Limits::try_from(LimitsSpec {
                max_join_attempts: 1,
                ..Default::default()
            })
            .unwrap(),
        ));
        let formation = "join-target".parse().unwrap();
        joiner.apply(Message::Local(Command::BeginJoin {
            session: SessionId(1),
            target_formation: formation,
        }));
        let attempt = joiner.model().join_attempt().unwrap().clone();
        joiner.apply(Message::Local(Command::RebindJoin {
            previous_session: SessionId(1),
            session: SessionId(2),
            target_formation: attempt.target_formation,
        }));
        assert!(joiner.model().join_attempt().is_none());
        metrics.record(None, &joiner.diagnostics);
        for event in Event::ALL {
            assert_eq!(
                metrics.snapshot().unwrap().get(event),
                u64::from(matches!(
                    event,
                    Event::JoinAbandoned | Event::AntiEntropyAbandoned
                ))
            );
        }
    }

    #[cfg(feature = "observability")]
    #[test]
    fn timer_classification_stale_exclusion_and_saturation() {
        let metrics = FormationMetrics::enabled();
        for (kind, event) in [
            (TimerKind::DirectProbe, Event::DirectProbeDeadline),
            (TimerKind::IndirectProbe, Event::IndirectProbeDeadline),
            (TimerKind::Suspicion, Event::SuspicionDeadline),
            (TimerKind::AntiEntropy, Event::AntiEntropyDeadline),
            (TimerKind::JoinRetry, Event::JoinRetryDue),
        ] {
            let timer = TimerToken {
                kind,
                generation: 1,
            };
            metrics.record(Some(timer), &[]);
            metrics.record(Some(timer), &[Diagnostic::StaleTimer { token: timer }]);
            assert_eq!(metrics.snapshot().unwrap().get(event), 1);
        }
        metrics.record(
            None,
            &[
                Diagnostic::JoinAbandoned { attempts: 3 },
                Diagnostic::AntiEntropyAbandoned {
                    peer: "private-peer".parse().unwrap(),
                    exchanges: 4,
                },
            ],
        );
        for event in Event::ALL {
            assert_eq!(
                metrics.snapshot().unwrap().get(event),
                if event == Event::StaleTimerInput {
                    5
                } else {
                    1
                }
            );
            metrics.counters.as_ref().unwrap()[event as usize].store(u64::MAX, Ordering::Relaxed);
            add(&metrics.counters.as_ref().unwrap()[event as usize]);
            assert_eq!(metrics.snapshot().unwrap().get(event), u64::MAX);
        }
    }
}
