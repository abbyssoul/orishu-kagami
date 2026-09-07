//! Optional process-lifetime accounting at the inbound peer handshake boundary.
//! No peer labels, payloads, clocks, exporter IO or membership decisions.

#[cfg(feature = "observability")]
use std::sync::{
    Arc, RwLock, Weak,
    atomic::{AtomicU64, Ordering},
};
#[cfg(feature = "observability")]
use tokio::sync::Semaphore;

#[cfg(feature = "observability")]
#[derive(Default)]
struct Counters {
    events: [AtomicU64; 10],
    pressure: RwLock<Pressure>,
}

#[cfg(feature = "observability")]
#[derive(Default)]
struct Pressure {
    tls: Weak<Semaphore>,
    tls_capacity: usize,
    connections: usize,
    connection_capacity: usize,
}

/// Current accepting adapter's budgets, not established sessions or members.
/// All fields are zero before adapter startup and after its lifetime ends.
#[cfg(feature = "observability")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IngressPressure {
    /// Actual semaphore permits occupied, including not-yet-polled TLS work.
    pub tls_slots_in_use: usize,
    /// Maximum TLS permits of the accepting adapter.
    pub tls_slots_capacity: usize,
    /// Last adapter-published JoinSet length, including uncollected completions.
    pub connection_slots_in_use: usize,
    /// Maximum connection tasks of the accepting adapter.
    pub connection_slots_capacity: usize,
}

/// Fixed inbound adapter outcomes. A completed handshake is not an admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngressEvent {
    /// A QUIC/TLS handshake completed at the server.
    TlsCompleted,
    /// QUIC returned a non-timeout error before TLS completed.
    TlsFailed,
    /// The outer TLS budget or QUIC transport timeout expired.
    TlsTimedOut,
    /// A pending TLS future was dropped before an outcome was observed.
    TlsCancelled,
    /// The initial application handshake was served and a session returned.
    HandshakeCompleted,
    /// The initial application exchange returned a non-timeout error.
    HandshakeFailed,
    /// The application handshake or nested exchange deadline expired.
    HandshakeTimedOut,
    /// A pending application handshake future was dropped.
    HandshakeCancelled,
    /// An incoming connection was refused at the TLS concurrency gate.
    TlsCapacityRefused,
    /// An incoming connection was refused at the connection-task gate.
    ConnectionCapacityRefused,
}

impl IngressEvent {
    /// Complete finite catalogue, in snapshot order. All values are counters.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 10] = [
        Self::TlsCompleted,
        Self::TlsFailed,
        Self::TlsTimedOut,
        Self::TlsCancelled,
        Self::HandshakeCompleted,
        Self::HandshakeFailed,
        Self::HandshakeTimedOut,
        Self::HandshakeCancelled,
        Self::TlsCapacityRefused,
        Self::ConnectionCapacityRefused,
    ];

    /// Static Prometheus suffix and bounded explanatory text.
    #[cfg(feature = "observability")]
    pub fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::TlsCompleted => ("tls_completed", "Inbound QUIC TLS handshakes completed."),
            Self::TlsFailed => ("tls_failed", "Inbound QUIC TLS non-timeout errors."),
            Self::TlsTimedOut => ("tls_timed_out", "Inbound QUIC TLS deadlines expired."),
            Self::TlsCancelled => ("tls_cancelled", "Pending inbound TLS futures dropped."),
            Self::HandshakeCompleted => (
                "handshake_completed",
                "Initial peer exchanges served; not admissions.",
            ),
            Self::HandshakeFailed => (
                "handshake_failed",
                "Initial peer exchanges with non-timeout errors.",
            ),
            Self::HandshakeTimedOut => (
                "handshake_timed_out",
                "Initial peer exchange deadlines expired.",
            ),
            Self::HandshakeCancelled => (
                "handshake_cancelled",
                "Pending initial peer exchange futures dropped.",
            ),
            Self::TlsCapacityRefused => (
                "tls_capacity_refused",
                "Incoming peers refused at TLS slot capacity.",
            ),
            Self::ConnectionCapacityRefused => (
                "connection_capacity_refused",
                "Incoming peers refused at connection task capacity.",
            ),
        }
    }
}

/// One optional, fixed-size allocation shared by inbound connection tasks.
/// Disabled builds use a zero-sized no-op; runtime-disabled builds allocate none.
#[derive(Clone, Default)]
pub struct IngressMetrics {
    #[cfg(feature = "observability")]
    counters: Option<Arc<Counters>>,
}

impl IngressMetrics {
    /// Allocate once before accepting peers, only when metrics are enabled.
    #[cfg(feature = "observability")]
    pub fn enabled() -> Self {
        Self {
            counters: Some(Arc::new(Counters::default())),
        }
    }

    /// Independent relaxed reads; not a transactional membership snapshot.
    /// Returns `None` when collection was not enabled at worker startup.
    #[cfg(feature = "observability")]
    pub fn snapshot(&self) -> Option<IngressSnapshot> {
        self.counters.as_ref().map(|counters| IngressSnapshot {
            values: std::array::from_fn(|i| counters.events[i].load(Ordering::Relaxed)),
            pressure: {
                // This lock protects three scalars and a weak pointer only.
                // No owner, network, timer or collection work occurs under it.
                let state = counters.pressure.read().expect("ingress pressure lock");
                state
                    .tls
                    .upgrade()
                    .map_or_else(IngressPressure::default, |tls| IngressPressure {
                        tls_slots_in_use: state.tls_capacity - tls.available_permits(),
                        tls_slots_capacity: state.tls_capacity,
                        connection_slots_in_use: state.connections,
                        connection_slots_capacity: state.connection_capacity,
                    })
            },
        })
    }

    /// Attach to the actual adapter budgets; disabled collection retains none.
    pub(super) fn serving(
        &self,
        tls: &std::sync::Arc<tokio::sync::Semaphore>,
        connection_capacity: usize,
    ) -> Serving {
        #[cfg(feature = "observability")]
        if let Some(counters) = &self.counters {
            let tls = Arc::downgrade(tls);
            *counters.pressure.write().expect("ingress pressure lock") = Pressure {
                tls_capacity: tls.upgrade().expect("live semaphore").available_permits(),
                tls: tls.clone(),
                connections: 0,
                connection_capacity,
            };
            return Serving {
                counters: Some(counters.clone()),
                tls,
            };
        }
        let _ = (tls, connection_capacity);
        Serving::default()
    }

    pub(super) fn record(&self, event: IngressEvent) {
        #[cfg(feature = "observability")]
        if let Some(counters) = &self.counters {
            let _ = counters.events[event as usize].fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |value| Some(value.saturating_add(1)),
            );
        }
        #[cfg(not(feature = "observability"))]
        let _ = event;
    }

    pub(super) fn tls(&self) -> PendingHandshake<'_> {
        PendingHandshake {
            metrics: self,
            cancelled: Some(IngressEvent::TlsCancelled),
        }
    }

    pub(super) fn handshake(&self) -> PendingHandshake<'_> {
        PendingHandshake {
            metrics: self,
            cancelled: Some(IngressEvent::HandshakeCancelled),
        }
    }
}

/// Fixed-size copy retained independently of the owner or connection lifetime.
#[cfg(feature = "observability")]
pub struct IngressSnapshot {
    values: [u64; 10],
    pressure: IngressPressure,
}

#[cfg(feature = "observability")]
impl IngressSnapshot {
    /// Read one aggregate counter, saturating at `u64::MAX` until process exit.
    pub fn get(&self, event: IngressEvent) -> u64 {
        self.values[event as usize]
    }
    /// Bounded adapter reading; independent of the cumulative event counters.
    pub fn pressure(&self) -> IngressPressure {
        self.pressure
    }
}

/// Adapter lifetime guard, including abort/unwind before normal cleanup.
#[derive(Default)]
pub(super) struct Serving {
    #[cfg(feature = "observability")]
    counters: Option<Arc<Counters>>,
    #[cfg(feature = "observability")]
    tls: Weak<Semaphore>,
}

impl Serving {
    pub(super) fn connections(&self, count: usize) {
        #[cfg(feature = "observability")]
        if let Some(counters) = &self.counters {
            let mut state = counters.pressure.write().expect("ingress pressure lock");
            if state.tls.ptr_eq(&self.tls) {
                assert!(count <= state.connection_capacity);
                state.connections = count;
            }
        }
        let _ = count;
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        #[cfg(feature = "observability")]
        if let Some(counters) = &self.counters {
            let mut state = counters.pressure.write().expect("ingress pressure lock");
            if state.tls.ptr_eq(&self.tls) {
                *state = Pressure::default();
            }
        }
    }
}

pub(super) struct PendingHandshake<'a> {
    metrics: &'a IngressMetrics,
    cancelled: Option<IngressEvent>,
}

impl PendingHandshake<'_> {
    pub(super) fn finish(mut self, event: IngressEvent) {
        self.cancelled = None;
        self.metrics.record(event);
    }
}

impl Drop for PendingHandshake<'_> {
    fn drop(&mut self) {
        if let Some(event) = self.cancelled {
            self.metrics.record(event);
        }
    }
}

#[cfg(all(test, feature = "observability"))]
mod tests {
    use super::*;

    #[test]
    fn pressure_tracks_actual_permits_and_guard_lifetime_without_stale_reset() {
        let metrics = IngressMetrics::enabled();
        assert_eq!(
            metrics.snapshot().unwrap().pressure(),
            IngressPressure::default()
        );
        let tls = Arc::new(Semaphore::new(16));
        let serving = metrics.serving(&tls, 64);
        let permit = tls.clone().try_acquire_owned().unwrap();
        serving.connections(3);
        assert_eq!(
            metrics.snapshot().unwrap().pressure(),
            IngressPressure {
                tls_slots_in_use: 1,
                tls_slots_capacity: 16,
                connection_slots_in_use: 3,
                connection_slots_capacity: 64,
            }
        );
        drop(permit);
        assert_eq!(metrics.snapshot().unwrap().pressure().tls_slots_in_use, 0);
        // A replaced adapter cannot reset or publish into its successor.
        let replacement = Arc::new(Semaphore::new(16));
        let next = metrics.serving(&replacement, 64);
        next.connections(2);
        serving.connections(4);
        drop(serving);
        assert_eq!(
            metrics
                .snapshot()
                .unwrap()
                .pressure()
                .connection_slots_in_use,
            2
        );
        drop(next);
        assert_eq!(
            metrics.snapshot().unwrap().pressure(),
            IngressPressure::default()
        );
    }

    #[test]
    fn disabled_collection_allocates_nothing_and_reports_unavailable() {
        let metrics = IngressMetrics::default();
        metrics.tls().finish(IngressEvent::TlsFailed);
        drop(metrics.handshake());
        assert!(metrics.counters.is_none());
        assert!(metrics.snapshot().is_none());
        let tls = Arc::new(Semaphore::new(16));
        let serving = metrics.serving(&tls, 64);
        serving.connections(1);
        assert_eq!(Arc::weak_count(&tls), 0);
        drop(serving);
        assert!(metrics.snapshot().is_none());
    }

    #[test]
    fn outcomes_are_exclusive_and_counters_saturate() {
        let metrics = IngressMetrics::enabled();
        metrics.tls().finish(IngressEvent::TlsCompleted);
        drop(metrics.handshake());
        let snapshot = metrics.snapshot().unwrap();
        for event in IngressEvent::ALL {
            assert_eq!(
                snapshot.get(event),
                u64::from(matches!(
                    event,
                    IngressEvent::TlsCompleted | IngressEvent::HandshakeCancelled
                ))
            );
        }
        metrics.counters.as_ref().unwrap().events[IngressEvent::TlsCompleted as usize]
            .store(u64::MAX, Ordering::Relaxed);
        metrics.tls().finish(IngressEvent::TlsCompleted);
        assert_eq!(
            metrics.snapshot().unwrap().get(IngressEvent::TlsCompleted),
            u64::MAX
        );
    }
}

#[cfg(all(test, not(feature = "observability")))]
#[test]
fn omitted_capability_has_no_counter_storage() {
    assert_eq!(std::mem::size_of::<IngressMetrics>(), 0);
    let metrics = IngressMetrics::default();
    metrics.tls().finish(IngressEvent::TlsCompleted);
    drop(metrics.handshake());
    assert_eq!(std::mem::size_of::<Serving>(), 0);
    let tls = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
    let serving = metrics.serving(&tls, 64);
    serving.connections(1);
    assert_eq!(std::sync::Arc::weak_count(&tls), 0);
}
