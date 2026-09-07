//! Bounded datagram submission and optional pre-owner traffic accounting.
//! O(1) work/storage per event; no peer data, timing clock or exporter IO.

use super::{exchange::ExchangeError, wire};
#[cfg(feature = "observability")]
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Fixed local IO observations; none establishes remote delivery or admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// QUIC accepted a datagram into its local send path.
    DatagramSubmitted,
    /// Wrong delivery class, over-budget bytes or unavailable datagram capability.
    DatagramSubmitRefused,
    /// QUIC returned an error after local size/class/capability checks.
    DatagramSubmitFailed,
    /// One complete payload was returned by QUIC to the registered receive loop.
    DatagramReceived,
    /// Returned payload exceeded the membership datagram ceiling.
    DatagramOversized,
    /// Payload bytes in successful local datagram submissions.
    DatagramBytesSubmitted,
    /// Payload bytes returned by QUIC, including later rejected input.
    DatagramBytesReceived,
    /// Registered connection refused a stream before entering the shared pool.
    StreamCapacityRefused,
}

impl Event {
    /// Complete static catalogue; all entries are process-lifetime counters.
    #[cfg(feature = "observability")]
    pub const ALL: [Self; 8] = [
        Self::DatagramSubmitted,
        Self::DatagramSubmitRefused,
        Self::DatagramSubmitFailed,
        Self::DatagramReceived,
        Self::DatagramOversized,
        Self::DatagramBytesSubmitted,
        Self::DatagramBytesReceived,
        Self::StreamCapacityRefused,
    ];

    /// Fixed metric suffix and explanatory text; no packet-derived labels.
    #[cfg(feature = "observability")]
    pub fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::DatagramSubmitted => (
                "datagrams_submitted_total",
                "Datagrams queued locally, not delivered.",
            ),
            Self::DatagramSubmitRefused => (
                "datagrams_submit_refused_total",
                "Datagrams refused by local class, size or capability checks.",
            ),
            Self::DatagramSubmitFailed => (
                "datagrams_submit_failed_total",
                "Datagram submissions rejected by QUIC after local checks.",
            ),
            Self::DatagramReceived => (
                "datagrams_received_total",
                "Datagram payloads returned by QUIC before domain validation.",
            ),
            Self::DatagramOversized => (
                "datagrams_oversized_total",
                "Received datagrams over the membership byte ceiling.",
            ),
            Self::DatagramBytesSubmitted => (
                "datagram_bytes_submitted_total",
                "Payload bytes queued locally, not delivered.",
            ),
            Self::DatagramBytesReceived => (
                "datagram_bytes_received_total",
                "Consumed datagram payload bytes including rejected input.",
            ),
            Self::StreamCapacityRefused => (
                "stream_capacity_refused_total",
                "Registered streams refused at per-connection task capacity.",
            ),
        }
    }
}

/// Shared process IO adapter with one optional fixed counter allocation.
/// Disabled collection is allocation-free; omitted collection is zero-sized.
#[derive(Clone, Default)]
pub struct Traffic {
    #[cfg(feature = "observability")]
    counters: Option<Arc<[AtomicU64; 8]>>,
}

impl Traffic {
    /// Allocate only at startup when diagnostics and metrics are enabled.
    #[cfg(feature = "observability")]
    pub fn enabled() -> Self {
        Self {
            counters: Some(Arc::new(std::array::from_fn(|_| AtomicU64::new(0)))),
        }
    }

    /// Static relaxed reads, not an atomic membership or delivery snapshot.
    #[cfg(feature = "observability")]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.counters.as_ref().map(|c| Snapshot {
            values: std::array::from_fn(|i| c[i].load(Ordering::Relaxed)),
        })
    }

    /// Submit only a datagram-class packet within the existing local/negotiated
    /// limits. No reliable fallback, queue waiter or retry is introduced.
    pub fn submit(
        &self,
        connection: &quinn::Connection,
        packet: wire::Encoded,
    ) -> Result<(), ExchangeError> {
        if packet.transport != wire::Transport::Datagram
            || packet.bytes.len() > wire::MAX_DATAGRAM_BYTES
            || connection
                .max_datagram_size()
                .is_none_or(|maximum| packet.bytes.len() > maximum)
        {
            self.add(Event::DatagramSubmitRefused, 1);
            return Err(ExchangeError::Datagram);
        }
        let length = packet.bytes.len() as u64;
        if connection.send_datagram(packet.bytes.into()).is_err() {
            self.add(Event::DatagramSubmitFailed, 1);
            return Err(ExchangeError::Datagram);
        }
        self.add(Event::DatagramSubmitted, 1);
        self.add(Event::DatagramBytesSubmitted, length);
        Ok(())
    }

    /// Account for a payload actually read from QUIC and enforce the existing
    /// pre-owner size gate. False means discard without allocation/owner work.
    pub(crate) fn receive(&self, bytes: usize) -> bool {
        self.add(Event::DatagramReceived, 1);
        self.add(Event::DatagramBytesReceived, bytes as u64);
        let fits = bytes <= wire::MAX_DATAGRAM_BYTES;
        if !fits {
            self.add(Event::DatagramOversized, 1);
        }
        fits
    }

    /// Record the registered loop's pre-pool refusal, not QUIC stream-credit wait.
    pub(crate) fn stream_capacity_refused(&self) {
        self.add(Event::StreamCapacityRefused, 1);
    }

    fn add(&self, event: Event, amount: u64) {
        #[cfg(feature = "observability")]
        if let Some(c) = &self.counters {
            let _ = c[event as usize].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old.saturating_add(amount))
            });
        }
        #[cfg(not(feature = "observability"))]
        let _ = (event, amount);
    }
}

/// Fixed process-lifetime readings, retained across formation changes.
#[cfg(feature = "observability")]
pub struct Snapshot {
    values: [u64; 8],
}

#[cfg(feature = "observability")]
impl Snapshot {
    /// Read one saturating event/byte total; values reset on process restart.
    pub fn get(&self, event: Event) -> u64 {
        self.values[event as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_collection_keeps_size_gates_without_storage() {
        let traffic = Traffic::default();
        assert!(traffic.receive(wire::MAX_DATAGRAM_BYTES));
        assert!(!traffic.receive(wire::MAX_DATAGRAM_BYTES + 1));
        traffic.stream_capacity_refused();
        #[cfg(feature = "observability")]
        assert!(traffic.snapshot().is_none());
        #[cfg(not(feature = "observability"))]
        assert_eq!(std::mem::size_of::<Traffic>(), 0);
    }

    #[cfg(feature = "observability")]
    #[test]
    fn observed_size_boundaries_and_all_counters_saturate() {
        let traffic = Traffic::enabled();
        assert!(traffic.receive(wire::MAX_DATAGRAM_BYTES));
        assert!(!traffic.receive(wire::MAX_DATAGRAM_BYTES + 1));
        traffic.stream_capacity_refused();
        let snapshot = traffic.snapshot().unwrap();
        assert_eq!(snapshot.get(Event::DatagramReceived), 2);
        assert_eq!(
            snapshot.get(Event::DatagramBytesReceived),
            (2 * wire::MAX_DATAGRAM_BYTES + 1) as u64
        );
        assert_eq!(snapshot.get(Event::DatagramOversized), 1);
        assert_eq!(snapshot.get(Event::StreamCapacityRefused), 1);
        for event in Event::ALL {
            traffic.add(event, u64::MAX);
            traffic.add(event, 1);
            assert_eq!(traffic.snapshot().unwrap().get(event), u64::MAX);
        }
    }
}

#[cfg(all(test, feature = "observability"))]
#[path = "traffic_tests.rs"]
mod wire_tests;
