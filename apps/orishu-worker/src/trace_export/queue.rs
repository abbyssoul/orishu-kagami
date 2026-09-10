//! Non-blocking local sampling and bounded root/child span lifecycle.
use super::{Operation, Outcome, SpanRecord};
use crate::trace_context::TraceParent;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

/// Invalid sampling or memory budgets. Diagnostics never include input payloads.
#[derive(Debug, thiserror::Error)]
#[error("invalid trace sampling, active-span or queue budget")]
pub struct QueueError;

#[derive(Default)]
struct Counters {
    sampled_out: AtomicU64,
    active_full: AtomicU64,
    queue_full: AtomicU64,
    closed: AtomicU64,
    invalid_source: AtomicU64,
    enqueued: AtomicU64,
}

/// Aggregate diagnostic snapshot; enqueue success is not collector delivery.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct QueueStats {
    /// Operations excluded by local head sampling.
    pub sampled_out: u64,
    /// Sampled operations shed because active slots were full.
    pub active_full: u64,
    /// Completed spans shed because the export queue was full.
    pub queue_full: u64,
    /// Spans shed because the export consumer was closed.
    pub closed: u64,
    /// Entropy, identity or local timestamp acquisition failed.
    pub invalid_source: u64,
    /// Completed spans accepted by the queue, not exported spans.
    pub enqueued: u64,
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(1))
    });
}

/// Cloneable operation-side handle; no method waits for exporter capacity.
#[derive(Clone)]
pub struct SpanQueue {
    log: Option<crate::operational_log::Log>,
    sample_ppm: u32,
    // The provider owns TLS configuration vectors, but its random source is
    // static. Resolve it once instead of rebuilding those vectors per request.
    random: &'static dyn rustls::crypto::SecureRandom,
    active: Arc<Semaphore>,
    sender: mpsc::Sender<SpanRecord>,
    counters: Arc<Counters>,
}

impl SpanQueue {
    /// Attach explicit operational logging before cloning the queue into runtime
    /// adapters. Only actual sampled span completions produce operation records.
    pub fn with_log(mut self, log: crate::operational_log::Log) -> Self {
        self.log = Some(log);
        self
    }

    /// Create separately bounded active slots and completed-span queue. The
    /// receiver belongs to the eventual export task. No task is spawned here.
    pub fn new(
        sample_ppm: u32,
        active: usize,
        queued: usize,
    ) -> Result<(Self, mpsc::Receiver<SpanRecord>), QueueError> {
        if sample_ppm > 1_000_000 || !(1..=4096).contains(&active) || !(1..=4096).contains(&queued)
        {
            return Err(QueueError);
        }
        let (sender, receiver) = mpsc::channel(queued);
        Ok((
            Self {
                log: None,
                sample_ppm,
                random: rustls::crypto::aws_lc_rs::default_provider().secure_random,
                active: Arc::new(Semaphore::new(active)),
                sender,
                counters: Arc::new(Counters::default()),
            },
            receiver,
        ))
    }

    /// Sample a new local root using the worker's existing cryptographic entropy
    /// provider. Zero sampling performs no clock or entropy acquisition. A shed
    /// span is `None`, never an operation failure. No caller sampling flag exists.
    pub fn try_begin(&self, operation: Operation) -> Option<ActiveSpan> {
        self.try_begin_with_parent(operation, None)
    }

    /// Create a root or a child of context already authorized by the IO adapter.
    /// Parent flags never override local sampling, active slots or queue limits.
    /// No context is retained when this span is shed; periodic work must pass
    /// `None` rather than inheriting unrelated recent request context.
    pub fn try_begin_with_parent(
        &self,
        operation: Operation,
        parent: Option<TraceParent>,
    ) -> Option<ActiveSpan> {
        if self.sample_ppm == 0 {
            increment(&self.counters.sampled_out);
            return None;
        }
        if self.sender.is_closed() {
            increment(&self.counters.closed);
            return None;
        }
        let mut random = [0; 28];
        if self.random.fill(&mut random).is_err() {
            increment(&self.counters.invalid_source);
            return None;
        }
        self.begin_with_parent(operation, random, parent)
    }

    #[cfg(test)]
    fn begin(&self, operation: Operation, random: [u8; 28]) -> Option<ActiveSpan> {
        self.begin_with_parent(operation, random, None)
    }

    fn begin_with_parent(
        &self,
        operation: Operation,
        random: [u8; 28],
        parent: Option<TraceParent>,
    ) -> Option<ActiveSpan> {
        let draw = u32::from_be_bytes(random[..4].try_into().unwrap());
        if !selected(draw, self.sample_ppm) {
            increment(&self.counters.sampled_out);
            return None;
        }
        let Ok(permit) = Arc::clone(&self.active).try_acquire_owned() else {
            increment(&self.counters.active_full);
            return None;
        };
        let Some(start) = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|time| u64::try_from(time.as_nanos()).ok())
        else {
            increment(&self.counters.invalid_source);
            return None;
        };
        let record = SpanRecord::new(
            parent.map_or_else(|| random[4..20].try_into().unwrap(), TraceParent::trace_id),
            random[20..].try_into().unwrap(),
            parent.map(TraceParent::span_id),
            operation,
            start,
            start,
            Outcome::Cancelled,
        );
        let Ok(record) = record else {
            increment(&self.counters.invalid_source);
            return None;
        };
        Some(ActiveSpan {
            record: Some(record),
            started: Instant::now(),
            queue: self.clone(),
            _permit: permit,
        })
    }

    /// Read independent saturating counters in constant work; not an atomic
    /// cross-counter transaction or a metric-based authority.
    pub fn stats(&self) -> QueueStats {
        let c = &self.counters;
        QueueStats {
            sampled_out: c.sampled_out.load(Ordering::Relaxed),
            active_full: c.active_full.load(Ordering::Relaxed),
            queue_full: c.queue_full.load(Ordering::Relaxed),
            closed: c.closed.load(Ordering::Relaxed),
            invalid_source: c.invalid_source.load(Ordering::Relaxed),
            enqueued: c.enqueued.load(Ordering::Relaxed),
        }
    }
}

// Round down to a 32-bit probability bucket; never exceed configured head rate.
fn selected(draw: u32, ppm: u32) -> bool {
    u64::from(draw) < (u64::from(ppm) * (1_u64 << 32)) / 1_000_000
}

/// An active slot plus fixed-size record. Dropping it publishes cancellation
/// best-effort and always releases capacity, even if the receiver is gone.
pub struct ActiveSpan {
    record: Option<SpanRecord>,
    started: Instant,
    queue: SpanQueue,
    _permit: OwnedSemaphorePermit,
}

impl ActiveSpan {
    /// The sampled local span's context for an explicitly causal child/exchange.
    /// The private record remains live until this handle is consumed or dropped.
    /// Output flags describe local sampling, never copied remote flags.
    pub fn context(&self) -> TraceParent {
        let record = self.record.as_ref().expect("active span owns a record");
        TraceParent::new(record.trace, record.span, true).expect("validated active span IDs")
    }

    /// Finish once with the adapter's actual outcome. Publication cannot wait.
    pub fn finish(mut self, outcome: Outcome) {
        if let Some(record) = &mut self.record {
            record.outcome = outcome;
        }
        self.publish();
    }

    fn publish(&mut self) {
        let Some(mut record) = self.record.take() else {
            return;
        };
        // Use elapsed monotonic time so wall-clock adjustment cannot reverse a span.
        let elapsed = self.started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        record.end = record.start.saturating_add(elapsed);
        if let Some(log) = &self.queue.log {
            let _ = log.try_record(record.log_record());
        }
        match self.queue.sender.try_send(record) {
            Ok(()) => increment(&self.queue.counters.enqueued),
            Err(mpsc::error::TrySendError::Full(_)) => increment(&self.queue.counters.queue_full),
            Err(mpsc::error::TrySendError::Closed(_)) => increment(&self.queue.counters.closed),
        }
    }
}

impl Drop for ActiveSpan {
    fn drop(&mut self) {
        self.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace_export::BatchLimits;

    #[test]
    #[ignore = "manual allocation profile; compare with and without ORISHU_PROFILE_REBUILD_PROVIDER"]
    fn profile_sampling_provider_allocations() {
        // Test-only reproduction of the prior setup, against the same current
        // dependencies and production sampling path. No worker runtime switch.
        let rebuild = std::env::var_os("ORISHU_PROFILE_REBUILD_PROVIDER").is_some();
        let (mut queue, _receiver) = SpanQueue::new(1, 1, 1).unwrap();
        for _ in 0..10_000 {
            if rebuild {
                queue.random = rustls::crypto::aws_lc_rs::default_provider().secure_random;
            }
            drop(queue.try_begin(Operation::ClientRequest));
        }
        assert_eq!(queue.stats().invalid_source, 0);
    }

    #[test]
    fn retained_random_source_preserves_failure_and_fast_path_semantics() {
        #[derive(Debug)]
        struct FailingRandom(std::sync::atomic::AtomicUsize);
        impl rustls::crypto::SecureRandom for FailingRandom {
            fn fill(&self, bytes: &mut [u8]) -> Result<(), rustls::crypto::GetRandomFailed> {
                assert_eq!(bytes.len(), 28);
                self.0.fetch_add(1, Ordering::Relaxed);
                Err(rustls::crypto::GetRandomFailed)
            }
        }
        static RANDOM: FailingRandom = FailingRandom(std::sync::atomic::AtomicUsize::new(0));
        let (mut queue, receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        assert!(std::ptr::eq(queue.random, queue.clone().random));
        queue.random = &RANDOM;
        assert!(queue.clone().try_begin(Operation::ClientRequest).is_none());
        assert_eq!(queue.stats().invalid_source, 1);
        assert_eq!(queue.active.available_permits(), 1);
        queue.sample_ppm = 0;
        assert!(queue.try_begin(Operation::ClientRequest).is_none());
        assert_eq!(queue.stats().sampled_out, 1);
        queue.sample_ppm = 1_000_000;
        drop(receiver);
        assert!(queue.try_begin(Operation::ClientRequest).is_none());
        assert_eq!(queue.stats().closed, 1);
        assert_eq!(RANDOM.0.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn sampling_boundaries_and_zero_policy() {
        assert!(!selected(0, 0));
        assert!(selected(u32::MAX, 1_000_000));
        let threshold = ((1_u64 << 32) * 1000 / 1_000_000) as u32;
        assert!(selected(threshold - 1, 1000));
        assert!(!selected(threshold, 1000));
        let (queue, mut receiver) = SpanQueue::new(0, 1, 1).unwrap();
        assert!(queue.try_begin(Operation::ClientRequest).is_none());
        assert!(receiver.try_recv().is_err());
        assert_eq!(queue.stats().sampled_out, 1);
        for (sample, active, queued) in [
            (1_000_001, 1, 1),
            (0, 0, 1),
            (0, 4097, 1),
            (0, 1, 0),
            (0, 1, 4097),
        ] {
            assert!(SpanQueue::new(sample, active, queued).is_err());
        }
    }

    #[test]
    fn active_queue_overflow_and_closed_consumer_release_capacity() {
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        let first = queue.try_begin(Operation::ClientRequest).unwrap();
        assert!(queue.try_begin(Operation::ClientRequest).is_none());
        first.finish(Outcome::Completed);
        // A full completed queue does not leak the separate active slot.
        drop(queue.try_begin(Operation::Admission).unwrap());
        let first = receiver.try_recv().unwrap();
        assert!(matches!(first.outcome, Outcome::Completed));
        assert!(first.end >= first.start);
        assert_eq!(queue.stats().active_full, 1);
        assert_eq!(queue.stats().queue_full, 1);
        let guard = queue.try_begin(Operation::PeerExchange).unwrap();
        drop(receiver);
        drop(guard);
        assert_eq!(queue.active.available_permits(), 1);
        assert!(queue.try_begin(Operation::ClientRequest).is_none());
        assert_eq!(queue.stats().closed, 2);
        assert_eq!(queue.stats().enqueued, 1);
    }

    #[tokio::test]
    async fn aborted_operation_enqueues_one_cancelled_record_that_encodes() {
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        let guard = queue.try_begin(Operation::ClientRequest).unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _guard = guard;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let span = receiver.try_recv().unwrap();
        assert!(matches!(span.outcome, Outcome::Cancelled));
        assert!(receiver.try_recv().is_err());
        assert_eq!(queue.active.available_permits(), 1);
        assert_eq!(
            BatchLimits::new(1, 1024)
                .unwrap()
                .encode(&[span])
                .unwrap()
                .consumed(),
            1
        );
    }

    #[test]
    fn invalid_identity_releases_reservation_and_counters_saturate() {
        let (queue, _receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        assert!(queue.begin(Operation::ClientRequest, [0; 28]).is_none());
        assert_eq!(queue.active.available_permits(), 1);
        assert_eq!(queue.stats().invalid_source, 1);
        queue
            .counters
            .invalid_source
            .store(u64::MAX, Ordering::Relaxed);
        assert!(queue.begin(Operation::ClientRequest, [0; 28]).is_none());
        assert_eq!(queue.stats().invalid_source, u64::MAX);
    }

    #[test]
    fn child_context_and_actual_otlp_bytes_preserve_parent_identity() {
        use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
        use prost::Message;

        let (queue, mut receiver) = SpanQueue::new(1_000_000, 2, 2).unwrap();
        let root = queue.try_begin(Operation::ClientRequest).unwrap();
        // Exercise the same fixed wire-value representation future adapters use.
        let parent = TraceParent::parse(&root.context().encode()).unwrap();
        let child = queue
            .try_begin_with_parent(Operation::PeerExchange, Some(parent))
            .unwrap();
        let context = child.context();
        assert_eq!(context.trace_id(), parent.trace_id());
        assert_ne!(context.span_id(), parent.span_id());
        assert!(context.sampled());
        child.finish(Outcome::Completed);
        root.finish(Outcome::Completed);
        let batch = BatchLimits::new(2, 1024)
            .unwrap()
            .encode(&[receiver.try_recv().unwrap(), receiver.try_recv().unwrap()])
            .unwrap();
        let decoded = ExportTraceServiceRequest::decode(batch.body()).unwrap();
        let spans = &decoded.resource_spans[0].scope_spans[0].spans;
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].trace_id, spans[1].trace_id);
        assert_eq!(spans[0].parent_span_id, spans[1].span_id);
        assert_eq!(spans[0].span_id, context.span_id());
        assert_eq!(spans[0].flags, 1);
        assert!(spans[1].parent_span_id.is_empty());
    }

    #[test]
    fn remote_flags_never_override_local_sampling_or_reserve_capacity() {
        let (mut queue, mut receiver) = SpanQueue::new(1000, 1, 1).unwrap();
        for sampled in [false, true] {
            let parent = TraceParent::new([7; 16], [8; 8], sampled).unwrap();
            assert!(
                queue
                    .begin_with_parent(Operation::PeerExchange, [255; 28], Some(parent))
                    .is_none()
            );
            let mut random = [0; 28];
            random[20..].fill(9);
            let active = queue
                .begin_with_parent(Operation::PeerExchange, random, Some(parent))
                .unwrap();
            assert_eq!(
                active.context(),
                TraceParent::new([7; 16], [9; 8], true).unwrap()
            );
            drop(active);
            let record = receiver.try_recv().unwrap();
            assert_eq!(record.parent, Some([8; 8]));
            assert!(matches!(record.outcome, Outcome::Cancelled));
        }
        assert_eq!(queue.stats().sampled_out, 2);
        #[derive(Debug)]
        struct NoEntropy;
        impl rustls::crypto::SecureRandom for NoEntropy {
            fn fill(&self, _: &mut [u8]) -> Result<(), rustls::crypto::GetRandomFailed> {
                panic!("zero sampling must not acquire entropy")
            }
        }
        queue.sample_ppm = 0;
        queue.random = &NoEntropy;
        assert!(
            queue
                .try_begin_with_parent(
                    Operation::PeerExchange,
                    TraceParent::new([7; 16], [8; 8], true),
                )
                .is_none()
        );
        assert_eq!(queue.active.available_permits(), 1);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn parent_collision_and_child_pressure_release_slots_without_root_contamination() {
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        let parent = TraceParent::new([7; 16], [8; 8], true).unwrap();
        let mut random = [0; 28];
        random[20..].fill(8);
        assert!(
            queue
                .begin_with_parent(Operation::PeerExchange, random, Some(parent))
                .is_none()
        );
        assert_eq!(queue.stats().invalid_source, 1);
        assert_eq!(queue.active.available_permits(), 1);
        let active = queue
            .try_begin_with_parent(Operation::PeerExchange, Some(parent))
            .unwrap();
        assert!(
            queue
                .try_begin_with_parent(Operation::PeerExchange, Some(parent))
                .is_none()
        );
        active.finish(Outcome::Completed);
        drop(
            queue
                .try_begin_with_parent(Operation::PeerExchange, Some(parent))
                .unwrap(),
        );
        assert_eq!(queue.stats().active_full, 1);
        assert_eq!(queue.stats().queue_full, 1);
        assert_eq!(queue.active.available_permits(), 1);
        receiver.try_recv().unwrap();
        drop(queue.try_begin(Operation::Admission).unwrap());
        let unrelated = receiver.try_recv().unwrap();
        assert!(unrelated.parent.is_none());
        assert_ne!(unrelated.trace, parent.trace_id());
        drop(receiver);
        assert!(
            queue
                .try_begin_with_parent(Operation::PeerExchange, Some(parent))
                .is_none()
        );
        assert_eq!(queue.stats().closed, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cloned_producers_share_exact_active_and_queue_budgets() {
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 4, 2).unwrap();
        let barrier = Arc::new(tokio::sync::Barrier::new(16));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..16 {
            let queue = queue.clone();
            let barrier = Arc::clone(&barrier);
            tasks.spawn(async move {
                let active = queue.try_begin(Operation::ClientRequest);
                // All sixteen try before any accepted guard may release a slot.
                barrier.wait().await;
                if let Some(active) = active {
                    active.finish(Outcome::Completed);
                }
            });
        }
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while let Some(result) = tasks.join_next().await {
                result.unwrap();
            }
        })
        .await
        .expect("bounded concurrent trace producers");
        assert_eq!(
            queue.stats(),
            QueueStats {
                active_full: 12,
                queue_full: 2,
                enqueued: 2,
                ..Default::default()
            }
        );
        assert_eq!(queue.active.available_permits(), 4);
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err());
        queue
            .try_begin(Operation::Admission)
            .unwrap()
            .finish(Outcome::Rejected);
        assert!(matches!(
            receiver.try_recv().unwrap().outcome,
            Outcome::Rejected
        ));
    }
}
