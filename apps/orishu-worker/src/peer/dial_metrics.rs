//! Optional fixed-size outbound dial accounting, separate from owner acceptance.
//! O(1) work per observed stage; no peer, certificate or endpoint data retained.
#[cfg(feature = "observability")]
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

/// Two nested transport boundaries, never domain admission outcomes.
#[derive(Clone, Copy, Debug)]
pub enum Stage {
    /// One capacity-admitted attempt, including configuration and all candidates.
    Attempt,
    /// One endpoint candidate's QUIC/TLS initiation and completion.
    Tls,
}

impl Stage {
    /// Fixed catalogue, independent of endpoints and formation identity.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 2] = [Self::Attempt, Self::Tls];
    /// Static Prometheus name fragment.
    #[cfg(feature = "observability")]
    pub fn name(self) -> &'static str {
        match self {
            Self::Attempt => "attempt",
            Self::Tls => "tls",
        }
    }
}

/// Exactly one observed terminal outcome per stage. Pending stages count none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Transport returned success; the owner has not yet validated its reply.
    Completed,
    /// Non-timeout error, including local configuration or candidate exhaustion.
    Failed,
    /// The stage's outer deadline, or QUIC's TLS transport timeout, expired.
    TimedOut,
    /// Future dropped before a terminal result was observed.
    Cancelled,
}

impl Outcome {
    /// Fixed terminal outcomes.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 4] = [
        Self::Completed,
        Self::Failed,
        Self::TimedOut,
        Self::Cancelled,
    ];
    /// Static Prometheus name fragment.
    #[cfg(feature = "observability")]
    pub fn name(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Inclusive microsecond boundaries, rendered in seconds; overflow is `+Inf`.
#[cfg(feature = "observability")]
pub const DURATION_BOUNDS: [(u64, &str); 7] = [
    (1_000, "0.001"),
    (5_000, "0.005"),
    (25_000, "0.025"),
    (100_000, "0.1"),
    (500_000, "0.5"),
    (1_000_000, "1"),
    (5_000_000, "5"),
];

#[cfg(feature = "observability")]
#[derive(Default)]
struct StageCounters {
    outcomes: [AtomicU64; 4],
    micros: AtomicU64,
    buckets: [AtomicU64; 8],
}

#[cfg(feature = "observability")]
#[derive(Default)]
struct Counters {
    stages: [StageCounters; 2],
    capacity_refused: AtomicU64,
}

/// One optional startup allocation owned by the process-wide dialer. Disabled
/// collection allocates nothing and reads no clock; compiled-out is zero-sized.
#[derive(Default)]
pub struct DialMetrics {
    #[cfg(feature = "observability")]
    counters: Option<Box<Counters>>,
}

impl DialMetrics {
    /// Enable before starting outbound work; no listener or exporter is created.
    #[cfg(feature = "observability")]
    pub fn enabled() -> Self {
        Self {
            counters: Some(Box::default()),
        }
    }

    /// Fixed relaxed reads, not a transactional snapshot or an owner request.
    #[cfg(feature = "observability")]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.counters.as_ref().map(|c| Snapshot {
            stages: std::array::from_fn(|i| StageSnapshot {
                outcomes: std::array::from_fn(|j| c.stages[i].outcomes[j].load(Ordering::Relaxed)),
                duration_micros: c.stages[i].micros.load(Ordering::Relaxed),
                buckets: std::array::from_fn(|j| c.stages[i].buckets[j].load(Ordering::Relaxed)),
            }),
            capacity_refused: c.capacity_refused.load(Ordering::Relaxed),
        })
    }

    pub(super) fn capacity_refused(&self) {
        #[cfg(feature = "observability")]
        if let Some(c) = &self.counters {
            add(&c.capacity_refused, 1);
        }
    }

    pub(super) fn begin(&self, stage: Stage) -> Observation<'_> {
        Observation {
            metrics: self,
            stage,
            outcome: Outcome::Cancelled,
            #[cfg(feature = "observability")]
            started: self.counters.as_ref().map(|_| Instant::now()),
        }
    }
}

/// Disjoint histogram samples and saturating process-lifetime outcome totals.
#[cfg(feature = "observability")]
pub struct StageSnapshot {
    outcomes: [u64; 4],
    /// Sum of terminal stage lifetimes, truncated to microseconds per sample.
    pub duration_micros: u64,
    /// Seven exclusive finite buckets plus overflow; render cumulatively.
    pub buckets: [u64; 8],
}
#[cfg(feature = "observability")]
impl StageSnapshot {
    /// One terminal outcome count, not a domain decision count.
    pub fn outcome(&self, outcome: Outcome) -> u64 {
        self.outcomes[outcome as usize]
    }
}

/// Local readings retained across formation changes; restart resets them.
#[cfg(feature = "observability")]
pub struct Snapshot {
    stages: [StageSnapshot; 2],
    /// Refusals before any stage starts, excluded from duration histograms.
    pub capacity_refused: u64,
}
#[cfg(feature = "observability")]
impl Snapshot {
    /// Read one of the two fixed nested stages.
    pub fn stage(&self, stage: Stage) -> &StageSnapshot {
        &self.stages[stage as usize]
    }
}

pub(super) struct Observation<'a> {
    metrics: &'a DialMetrics,
    stage: Stage,
    outcome: Outcome,
    #[cfg(feature = "observability")]
    started: Option<Instant>,
}
impl Observation<'_> {
    pub(super) fn finish(mut self, outcome: Outcome) {
        self.outcome = outcome;
    }
}
#[cfg(feature = "observability")]
fn add(counter: &AtomicU64, amount: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(amount))
    });
}
#[cfg(feature = "observability")]
fn bucket(micros: u64) -> usize {
    DURATION_BOUNDS
        .iter()
        .position(|(upper, _)| micros <= *upper)
        .unwrap_or(DURATION_BOUNDS.len())
}
impl Drop for Observation<'_> {
    fn drop(&mut self) {
        #[cfg(feature = "observability")]
        if let (Some(c), Some(started)) = (&self.metrics.counters, self.started) {
            let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            let stage = &c.stages[self.stage as usize];
            add(&stage.outcomes[self.outcome as usize], 1);
            add(&stage.micros, micros);
            add(&stage.buckets[bucket(micros)], 1);
        }
        #[cfg(not(feature = "observability"))]
        let _ = (self.metrics, self.stage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_collection_has_no_allocation_or_timing_clock() {
        let metrics = DialMetrics::default();
        let observation = metrics.begin(Stage::Attempt);
        metrics.capacity_refused();
        #[cfg(feature = "observability")]
        {
            assert!(observation.started.is_none());
            assert!(metrics.snapshot().is_none());
        }
        #[cfg(not(feature = "observability"))]
        assert_eq!(std::mem::size_of::<DialMetrics>(), 0);
        drop(observation);
    }

    #[cfg(feature = "observability")]
    #[test]
    fn exclusive_outcomes_histograms_and_saturation() {
        let metrics = DialMetrics::enabled();
        metrics.capacity_refused();
        for stage in Stage::ALL {
            for outcome in Outcome::ALL {
                metrics.begin(stage).finish(outcome);
            }
            drop(metrics.begin(stage));
            let snapshot = metrics.snapshot().unwrap();
            assert_eq!(snapshot.capacity_refused, 1);
            let s = snapshot.stage(stage);
            for outcome in Outcome::ALL {
                assert_eq!(
                    s.outcome(outcome),
                    if outcome == Outcome::Cancelled { 2 } else { 1 }
                );
            }
            assert_eq!(s.buckets.iter().sum::<u64>(), 5);
        }
        let c = metrics.counters.as_ref().unwrap();
        c.capacity_refused.store(u64::MAX, Ordering::Relaxed);
        metrics.capacity_refused();
        for stage in Stage::ALL {
            let s = &c.stages[stage as usize];
            s.micros.store(u64::MAX, Ordering::Relaxed);
            for value in s.buckets.iter().chain(&s.outcomes) {
                value.store(u64::MAX, Ordering::Relaxed);
            }
            for outcome in Outcome::ALL {
                metrics.begin(stage).finish(outcome);
            }
            let snapshot = metrics.snapshot().unwrap();
            assert_eq!(snapshot.capacity_refused, u64::MAX);
            let s = snapshot.stage(stage);
            assert_eq!(s.duration_micros, u64::MAX);
            assert_eq!(s.buckets, [u64::MAX; 8]);
            for outcome in Outcome::ALL {
                assert_eq!(s.outcome(outcome), u64::MAX);
            }
        }
    }

    #[cfg(feature = "observability")]
    #[test]
    fn inclusive_duration_boundaries() {
        assert_eq!(bucket(0), 0);
        for (i, (upper, _)) in DURATION_BOUNDS.iter().enumerate() {
            assert_eq!(bucket(upper - 1), i);
            assert_eq!(bucket(*upper), i);
            assert_eq!(bucket(upper + 1), i + 1);
        }
        assert_eq!(bucket(u64::MAX), 7);
    }
}
