//! Optional fixed-size reliable-exchange accounting. No peer data is retained.
//! Accounting adds O(1) local work per stream IO completion and O(1) aggregate
//! work per terminal exchange/scrape, independent of peers or exchange history.
#[cfg(feature = "observability")]
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

/// Local role in one reliable request/reply exchange.
#[derive(Clone, Copy, Debug)]
pub enum ExchangeRole {
    /// Open a stream, send the request and receive its reply.
    Request,
    /// Read an accepted stream, invoke the owner and send its reply.
    Serve,
}

impl ExchangeRole {
    /// Fixed catalogue roles, never derived from packet fields.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 2] = [Self::Request, Self::Serve];
    /// Static metric-name fragment.
    #[cfg(feature = "observability")]
    pub fn name(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Serve => "serve",
        }
    }
}

/// Mutually exclusive terminal adapter outcomes, not domain decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeOutcome {
    /// The adapter returned `Ok`; remote domain acceptance is not implied.
    Completed,
    /// A non-timeout, non-capacity error, including local payload validation.
    Failed,
    /// The outer or nested reliable-exchange deadline expired.
    TimedOut,
    /// The adapter returned `ExchangeError::Overloaded`.
    CapacityRefused,
    /// The polled exchange future was dropped before returning an outcome.
    Cancelled,
}

impl ExchangeOutcome {
    /// Finite catalogue outcomes.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 5] = [
        Self::Completed,
        Self::Failed,
        Self::TimedOut,
        Self::CapacityRefused,
        Self::Cancelled,
    ];
    /// Static metric-name fragment.
    #[cfg(feature = "observability")]
    pub fn name(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::CapacityRefused => "capacity_refused",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Inclusive duration boundaries in microseconds, rendered in seconds.
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
struct RoleCounters {
    outcomes: [AtomicU64; 5],
    micros: AtomicU64,
    buckets: [AtomicU64; 8],
}

#[cfg(feature = "observability")]
#[derive(Default)]
struct Counters {
    roles: [RoleCounters; 2],
    sent: AtomicU64,
    received: AtomicU64,
}

/// One startup allocation, shared by the worker's reliable exchange pool.
/// Compiled-out collection is zero-sized; runtime-disabled collection allocates
/// no counters, reads no timing clock and uses the ordinary QUIC IO helpers.
#[derive(Clone, Default)]
pub struct ExchangeMetrics {
    #[cfg(feature = "observability")]
    counters: Option<Arc<Counters>>,
}

impl ExchangeMetrics {
    /// Allocate the fixed catalogue before any exchanges start.
    #[cfg(feature = "observability")]
    pub fn enabled() -> Self {
        Self {
            counters: Some(Arc::new(Counters::default())),
        }
    }

    /// Bounded relaxed reads without owner, connection or collector IO.
    #[cfg(feature = "observability")]
    pub fn snapshot(&self) -> Option<ExchangeSnapshot> {
        self.counters.as_ref().map(|c| ExchangeSnapshot {
            roles: std::array::from_fn(|i| RoleSnapshot {
                outcomes: std::array::from_fn(|j| c.roles[i].outcomes[j].load(Ordering::Relaxed)),
                duration_micros: c.roles[i].micros.load(Ordering::Relaxed),
                buckets: std::array::from_fn(|j| c.roles[i].buckets[j].load(Ordering::Relaxed)),
            }),
            bytes_sent: c.sent.load(Ordering::Relaxed),
            bytes_received: c.received.load(Ordering::Relaxed),
        })
    }

    pub(super) fn begin(&self, role: ExchangeRole) -> ExchangeObservation<'_> {
        ExchangeObservation {
            metrics: self,
            role,
            outcome: ExchangeOutcome::Cancelled,
            #[cfg(feature = "observability")]
            started: self.counters.as_ref().map(|_| Instant::now()),
            transfer: TransferCount {
                #[cfg(feature = "observability")]
                enabled: self.counters.is_some(),
                #[cfg(feature = "observability")]
                sent: 0,
                #[cfg(feature = "observability")]
                received: 0,
            },
        }
    }
}

/// One role's process-lifetime snapshot; independent readings are not atomic
/// together. Histogram buckets are exclusive, not cumulative, until rendering.
#[cfg(feature = "observability")]
pub struct RoleSnapshot {
    outcomes: [u64; 5],
    /// Sum across all terminal outcomes, including early refusal/cancellation.
    pub duration_micros: u64,
    /// Seven finite buckets plus overflow, each containing disjoint samples.
    pub buckets: [u64; 8],
}

#[cfg(feature = "observability")]
impl RoleSnapshot {
    /// Read one terminal-outcome counter.
    pub fn outcome(&self, outcome: ExchangeOutcome) -> u64 {
        self.outcomes[outcome as usize]
    }
}

/// Process-lifetime stream IO, published at exchange termination, including
/// partial transfers. Bytes include frame prefixes, not QUIC/TLS wire overhead.
#[cfg(feature = "observability")]
pub struct ExchangeSnapshot {
    roles: [RoleSnapshot; 2],
    /// Bytes accepted by QUIC stream writes, not delivered/acknowledged bytes.
    pub bytes_sent: u64,
    /// Bytes actually consumed from QUIC streams, even on later codec failure.
    pub bytes_received: u64,
}

#[cfg(feature = "observability")]
impl ExchangeSnapshot {
    /// Read one of the two fixed role snapshots.
    pub fn role(&self, role: ExchangeRole) -> &RoleSnapshot {
        &self.roles[role as usize]
    }
}

/// Local per-exchange byte accumulation; no atomic update or allocation per IO
/// chunk. It is flushed once with the terminal outcome, even on cancellation.
#[derive(Default)]
pub(super) struct TransferCount {
    #[cfg(feature = "observability")]
    enabled: bool,
    #[cfg(feature = "observability")]
    sent: u64,
    #[cfg(feature = "observability")]
    received: u64,
}

impl TransferCount {
    pub(super) fn enabled(&self) -> bool {
        #[cfg(feature = "observability")]
        {
            self.enabled
        }
        #[cfg(not(feature = "observability"))]
        {
            false
        }
    }
    pub(super) fn sent(&mut self, bytes: usize) {
        #[cfg(feature = "observability")]
        {
            self.sent = self.sent.saturating_add(bytes as u64);
        }
        #[cfg(not(feature = "observability"))]
        let _ = bytes;
    }
    pub(super) fn received(&mut self, bytes: usize) {
        #[cfg(feature = "observability")]
        {
            self.received = self.received.saturating_add(bytes as u64);
        }
        #[cfg(not(feature = "observability"))]
        let _ = bytes;
    }
}

pub(super) struct ExchangeObservation<'a> {
    metrics: &'a ExchangeMetrics,
    role: ExchangeRole,
    outcome: ExchangeOutcome,
    #[cfg(feature = "observability")]
    started: Option<Instant>,
    pub(super) transfer: TransferCount,
}

impl ExchangeObservation<'_> {
    pub(super) fn finish<T>(mut self, result: &Result<T, super::exchange::ExchangeError>) {
        use super::{TransferError, exchange::ExchangeError};
        self.outcome = match result {
            Ok(_) => ExchangeOutcome::Completed,
            Err(ExchangeError::Overloaded) => ExchangeOutcome::CapacityRefused,
            Err(ExchangeError::Transfer(TransferError::Timeout)) => ExchangeOutcome::TimedOut,
            Err(_) => ExchangeOutcome::Failed,
        };
    }
}

#[cfg(feature = "observability")]
fn add(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(value))
    });
}

#[cfg(feature = "observability")]
fn duration_bucket(micros: u64) -> usize {
    DURATION_BOUNDS
        .iter()
        .position(|(upper, _)| micros <= *upper)
        .unwrap_or(DURATION_BOUNDS.len())
}

impl Drop for ExchangeObservation<'_> {
    fn drop(&mut self) {
        #[cfg(feature = "observability")]
        if let (Some(c), Some(started)) = (&self.metrics.counters, self.started) {
            let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            let role = &c.roles[self.role as usize];
            add(&role.outcomes[self.outcome as usize], 1);
            add(&role.micros, micros);
            let bucket = duration_bucket(micros);
            add(&role.buckets[bucket], 1);
            add(&c.sent, self.transfer.sent);
            add(&c.received, self.transfer.received);
        }
        #[cfg(not(feature = "observability"))]
        let _ = (self.metrics, self.role);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_collection_does_not_allocate_or_read_a_clock() {
        let metrics = ExchangeMetrics::default();
        let observation = metrics.begin(ExchangeRole::Request);
        assert!(!observation.transfer.enabled());
        #[cfg(feature = "observability")]
        {
            assert!(observation.started.is_none());
            assert!(metrics.snapshot().is_none());
        }
        #[cfg(not(feature = "observability"))]
        assert_eq!(std::mem::size_of::<ExchangeMetrics>(), 0);
    }

    #[cfg(feature = "observability")]
    #[test]
    fn outcomes_partial_bytes_and_saturation_are_retained() {
        let metrics = ExchangeMetrics::enabled();
        let mut observation = metrics.begin(ExchangeRole::Request);
        observation.transfer.sent(7);
        observation.transfer.received(3);
        drop(observation);
        let snapshot = metrics.snapshot().unwrap();
        assert_eq!((snapshot.bytes_sent, snapshot.bytes_received), (7, 3));
        assert_eq!(
            snapshot
                .role(ExchangeRole::Request)
                .outcome(ExchangeOutcome::Cancelled),
            1
        );
        assert_eq!(
            snapshot
                .role(ExchangeRole::Request)
                .buckets
                .iter()
                .sum::<u64>(),
            1
        );
        let c = metrics.counters.as_ref().unwrap();
        c.sent.store(u64::MAX, Ordering::Relaxed);
        c.received.store(u64::MAX, Ordering::Relaxed);
        c.roles[0].micros.store(u64::MAX, Ordering::Relaxed);
        for bucket in &c.roles[0].buckets {
            bucket.store(u64::MAX, Ordering::Relaxed);
        }
        c.roles[0].outcomes[0].store(u64::MAX, Ordering::Relaxed);
        let mut observation = metrics.begin(ExchangeRole::Request);
        observation.transfer.sent(1);
        observation.transfer.received(1);
        observation.finish(&Ok(()));
        let snapshot = metrics.snapshot().unwrap();
        assert_eq!(snapshot.bytes_sent, u64::MAX);
        assert_eq!(snapshot.bytes_received, u64::MAX);
        assert_eq!(
            snapshot.role(ExchangeRole::Request).duration_micros,
            u64::MAX
        );
        assert_eq!(snapshot.role(ExchangeRole::Request).buckets, [u64::MAX; 8]);
        assert_eq!(
            snapshot
                .role(ExchangeRole::Request)
                .outcome(ExchangeOutcome::Completed),
            u64::MAX
        );
    }

    #[cfg(feature = "observability")]
    #[test]
    fn duration_boundaries_are_inclusive_and_overflow_is_finite() {
        assert_eq!(duration_bucket(0), 0);
        for (index, (upper, _)) in DURATION_BOUNDS.iter().enumerate() {
            assert_eq!(duration_bucket(upper - 1), index);
            assert_eq!(duration_bucket(*upper), index);
            assert_eq!(duration_bucket(upper + 1), index + 1);
        }
        assert_eq!(duration_bucket(u64::MAX), DURATION_BOUNDS.len());
    }
}
