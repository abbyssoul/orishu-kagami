//! One bounded queue consumer, independent of membership and process health.
use super::{BatchLimits, DeliveryError, HttpDelivery, SpanRecord};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

/// Aggregate delivery accounting, live or final. Counts are diagnostic, not receipts.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportStats {
    /// Spans reported accepted by a valid collector response.
    pub accepted: u64,
    /// Spans explicitly rejected in a valid partial-success response.
    pub rejected: u64,
    /// Spans in failed attempts; remote acceptance may be unknown.
    pub failed: u64,
    /// Records shed because encoding failed, without stalling later records.
    pub encoding_dropped: u64,
    /// Records abandoned by shutdown, including an interrupted attempt.
    pub shutdown_dropped: u64,
    /// Valid responses containing a collector warning, whose text is discarded.
    pub warnings: u64,
}

/// Constant-work live reads independent of exporter IO. Fields are independent
/// atomic observations, not a transactional receipt or progress/health signal.
#[derive(Clone, Default)]
pub struct DeliveryCounters(Arc<[AtomicU64; 6]>);

impl DeliveryCounters {
    /// Read bounded aggregate delivery counters without asking the exporter task.
    pub fn snapshot(&self) -> ExportStats {
        let [
            accepted,
            rejected,
            failed,
            encoding_dropped,
            shutdown_dropped,
            warnings,
        ] = std::array::from_fn(|i| self.0[i].load(Ordering::Relaxed));
        ExportStats {
            accepted,
            rejected,
            failed,
            encoding_dropped,
            shutdown_dropped,
            warnings,
        }
    }

    fn publish(&self, stats: &ExportStats) {
        for (slot, value) in self.0.iter().zip([
            stats.accepted,
            stats.rejected,
            stats.failed,
            stats.encoding_dropped,
            stats.shutdown_dropped,
            stats.warnings,
        ]) {
            slot.store(value, Ordering::Relaxed);
        }
    }
}

/// Owns one receiver, reusable record buffer and single-flight HTTP transport.
/// Constructing it does not spawn a task; the runtime owns its execution/join.
pub struct ExportLoop {
    receiver: mpsc::Receiver<SpanRecord>,
    delivery: HttpDelivery,
    limits: BatchLimits,
    flush_interval: Duration,
    shutdown_timeout: Duration,
    pending: Vec<SpanRecord>,
    in_flight: usize,
    stats: ExportStats,
    counters: DeliveryCounters,
}

impl ExportLoop {
    /// Validate timing and queue/batch relationships before starting the task.
    pub fn new(
        receiver: mpsc::Receiver<SpanRecord>,
        delivery: HttpDelivery,
        limits: BatchLimits,
        flush_interval: Duration,
        shutdown_timeout: Duration,
    ) -> Result<Self, DeliveryError> {
        if limits.spans > receiver.max_capacity()
            || !(Duration::from_millis(100)..=Duration::from_secs(10)).contains(&flush_interval)
            || !(Duration::from_millis(100)..=Duration::from_secs(10)).contains(&shutdown_timeout)
        {
            return Err(DeliveryError::Configuration);
        }
        let pending = Vec::with_capacity(limits.spans);
        Ok(Self {
            receiver,
            delivery,
            limits,
            flush_interval,
            shutdown_timeout,
            pending,
            in_flight: 0,
            stats: ExportStats::default(),
            counters: DeliveryCounters::default(),
        })
    }

    /// Retain a read-only diagnostic handle before moving this loop into its task.
    pub fn counters(&self) -> DeliveryCounters {
        self.counters.clone()
    }

    /// Run until producers finish or shutdown is signalled (including sender
    /// loss). Shutdown closes admission immediately, abandons any interrupted
    /// HTTP attempt without retry, and drains remaining records under one total
    /// deadline. Late active spans are shed by the closed queue. No detached task.
    pub async fn run(mut self, mut stop: oneshot::Receiver<()>) -> ExportStats {
        tokio::select! {
            biased;
            _ = &mut stop => {},
            _ = self.consume() => return self.stats,
        }
        let deadline = Instant::now() + self.shutdown_timeout;
        self.receiver.close();
        add(&mut self.stats.shutdown_dropped, self.in_flight);
        self.counters.publish(&self.stats);
        self.in_flight = 0;
        if tokio::time::timeout_at(deadline, self.consume())
            .await
            .is_err()
        {
            add(&mut self.stats.shutdown_dropped, self.in_flight);
            add(&mut self.stats.shutdown_dropped, self.pending.len());
            add(&mut self.stats.shutdown_dropped, self.receiver.len());
        }
        self.counters.publish(&self.stats);
        self.stats
    }

    async fn consume(&mut self) {
        loop {
            if self.pending.is_empty() {
                let Some(record) = self.receiver.recv().await else {
                    return;
                };
                self.pending.push(record);
            }
            // The interval starts with the first retained record, not each
            // arrival. A busy producer cannot postpone flushing indefinitely.
            let deadline = Instant::now() + self.flush_interval;
            while self.pending.len() < self.limits.spans {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep_until(deadline) => break,
                    record = self.receiver.recv() => match record {
                        Some(record) => self.pending.push(record),
                        None => break,
                    },
                }
            }
            self.flush().await;
        }
    }

    async fn flush(&mut self) {
        while !self.pending.is_empty() {
            let batch = match self.limits.encode(&self.pending) {
                Ok(batch) => batch,
                Err(_) => {
                    self.pending.remove(0);
                    add(&mut self.stats.encoding_dropped, 1);
                    self.counters.publish(&self.stats);
                    continue;
                }
            };
            let count = batch.consumed();
            // Remove before awaiting: cancellation must not replay a request
            // the collector may already have received.
            self.pending.drain(..count);
            self.in_flight = count;
            match self.delivery.deliver(batch).await {
                Ok(result) => {
                    add(&mut self.stats.accepted, count - result.rejected);
                    add(&mut self.stats.rejected, result.rejected);
                    add(&mut self.stats.warnings, usize::from(result.warning));
                }
                Err(_) => add(&mut self.stats.failed, count),
            }
            self.in_flight = 0;
            self.counters.publish(&self.stats);
        }
    }
}

fn add(counter: &mut u64, count: usize) {
    *counter = counter.saturating_add(count as u64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace_export::{Operation, Outcome, SpanQueue};
    use prost::Message;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn delivery_snapshot_preserves_each_counter_and_saturation() {
        let counters = DeliveryCounters::default();
        let reader = counters.clone();
        assert_eq!(reader.snapshot(), ExportStats::default());
        let mut stats = ExportStats {
            accepted: u64::MAX - 1,
            rejected: 2,
            failed: 3,
            encoding_dropped: 4,
            shutdown_dropped: 5,
            warnings: 6,
        };
        add(&mut stats.accepted, 2);
        counters.publish(&stats);
        assert_eq!(stats.accepted, u64::MAX);
        assert_eq!(reader.snapshot(), stats);
    }

    async fn request(stream: &mut tokio::net::TcpStream) -> usize {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            assert!(head.len() < 4096);
            head.push(stream.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
        let size: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert!(size <= 1024);
        let mut body = vec![0; size];
        stream.read_exact(&mut body).await.unwrap();
        let request = super::super::ExportTraceServiceRequest::decode(body.as_slice()).unwrap();
        request.resource_spans[0].scope_spans[0].spans.len()
    }

    fn emit(queue: &SpanQueue) {
        queue
            .try_begin(Operation::ClientRequest)
            .unwrap()
            .finish(Outcome::Completed);
    }

    async fn transport() -> (tokio::net::TcpListener, HttpDelivery) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let delivery =
            HttpDelivery::new(&endpoint, None, None, Duration::from_secs(2), 1024).unwrap();
        (listener, delivery)
    }

    #[tokio::test]
    async fn count_and_partial_interval_flush_reach_real_receiver() {
        tokio::time::timeout(Duration::from_secs(4), async {
            let (listener, delivery) = transport().await;
            let (seen, mut received) = mpsc::channel(2);
            let server = tokio::spawn(async move {
                for expected in [2, 1] {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    assert_eq!(request(&mut stream).await, expected);
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n").await.unwrap();
                    seen.send(expected).await.unwrap();
                }
            });
            let (queue, receiver) = SpanQueue::new(1_000_000, 1, 3).unwrap();
            let (_stop, signal) = oneshot::channel();
            let exporter = ExportLoop::new(receiver, delivery, BatchLimits::new(2, 1024).unwrap(),
                Duration::from_millis(100), Duration::from_millis(100)).unwrap();
            let counters = exporter.counters();
            emit(&queue); emit(&queue); emit(&queue);
            let task = tokio::spawn(exporter.run(signal));
            assert_eq!(received.recv().await, Some(2));
            // Keep the sender alive: the partial batch must flush by timer,
            // not because producer closure forced it out.
            assert_eq!(received.recv().await, Some(1));
            drop(queue);
            assert_eq!(task.await.unwrap(), ExportStats { accepted: 3, ..Default::default() });
            assert_eq!(counters.snapshot(), ExportStats { accepted: 3, ..Default::default() });
            server.await.unwrap();
        }).await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_closes_producers_and_caps_stalled_delivery_and_drain() {
        tokio::time::timeout(Duration::from_secs(4), async {
            let (listener, delivery) = transport().await;
            let (seen, ready) = oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                assert_eq!(request(&mut stream).await, 1);
                seen.send(()).unwrap();
                // Retain both requests without responding: the per-attempt
                // two-second timeout must not override 100 ms total shutdown.
                let (mut second, _) = listener.accept().await.unwrap();
                assert_eq!(request(&mut second).await, 1);
                std::future::pending::<()>().await;
                drop((stream, second));
            });
            let (queue, receiver) = SpanQueue::new(1_000_000, 2, 3).unwrap();
            let late = queue.try_begin(Operation::Admission).unwrap();
            let (stop, signal) = oneshot::channel();
            let exporter = ExportLoop::new(
                receiver,
                delivery,
                BatchLimits::new(1, 1024).unwrap(),
                Duration::from_millis(100),
                Duration::from_millis(100),
            )
            .unwrap();
            emit(&queue);
            let counters = exporter.counters();
            let task = tokio::spawn(exporter.run(signal));
            ready.await.unwrap();
            assert_eq!(counters.snapshot(), ExportStats::default());
            for _ in 0..4 {
                emit(&queue);
            }
            assert_eq!(queue.stats().queue_full, 1);
            let started = Instant::now();
            stop.send(()).unwrap();
            let stats = task.await.unwrap();
            assert_eq!(counters.snapshot(), stats);
            assert!(started.elapsed() < Duration::from_secs(1));
            assert_eq!(
                stats,
                ExportStats {
                    shutdown_dropped: 4,
                    ..Default::default()
                }
            );
            drop(late);
            assert!(queue.try_begin(Operation::ClientRequest).is_none());
            assert_eq!(queue.stats().closed, 2);
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn shutdown_drains_byte_split_batches_and_accounts_partial_rejections() {
        tokio::time::timeout(Duration::from_secs(4), async {
            let (listener, delivery) = transport().await;
            let server = tokio::spawn(async move {
                let mut received = 0;
                let mut batches = 0;
                while received < 64 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let count = request(&mut stream).await;
                    assert!((1..64).contains(&count));
                    received += count;
                    batches += 1;
                    let body = opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse {
                        partial_success: Some(opentelemetry_proto::tonic::collector::trace::v1::ExportTracePartialSuccess {
                            rejected_spans: 1,
                            error_message: "untrusted collector warning".into(),
                        }),
                    }.encode_to_vec();
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: {}\r\n\r\n", body.len());
                    stream.write_all(head.as_bytes()).await.unwrap();
                    stream.write_all(&body).await.unwrap();
                }
                assert_eq!(received, 64);
                batches
            });
            let (queue, receiver) = SpanQueue::new(1_000_000, 1, 64).unwrap();
            for _ in 0..64 { emit(&queue); }
            let (stop, signal) = oneshot::channel();
            // Losing the lifecycle owner must also stop the export loop.
            drop(stop);
            let exporter = ExportLoop::new(receiver, delivery, BatchLimits::new(64, 1024).unwrap(),
                Duration::from_secs(1), Duration::from_secs(2)).unwrap();
            let counters = exporter.counters();
            let stats = exporter.run(signal).await;
            assert_eq!(counters.snapshot(), stats);
            let batches = server.await.unwrap();
            assert_eq!(stats, ExportStats {
                accepted: 64 - batches, rejected: batches, warnings: batches, ..Default::default()
            });
            assert!(queue.try_begin(Operation::ClientRequest).is_none());
        }).await.unwrap();
    }

    #[tokio::test]
    async fn failed_delivery_does_not_retry_or_block_following_batches() {
        tokio::time::timeout(Duration::from_secs(4), async {
            let (listener, delivery) = transport().await;
            let server = tokio::spawn(async move {
                for status in [503, 200] {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    assert_eq!(request(&mut stream).await, 1);
                    stream.write_all(format!("HTTP/1.1 {status} Status\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n").as_bytes()).await.unwrap();
                }
            });
            let (queue, receiver) = SpanQueue::new(1_000_000, 1, 2).unwrap();
            emit(&queue); emit(&queue); drop(queue);
            let (_stop, signal) = oneshot::channel();
            let exporter = ExportLoop::new(receiver, delivery, BatchLimits::new(1, 1024).unwrap(),
                Duration::from_millis(100), Duration::from_millis(100)).unwrap();
            let counters = exporter.counters();
            assert_eq!(exporter.run(signal).await,
                ExportStats { accepted: 1, failed: 1, ..Default::default() });
            assert_eq!(counters.snapshot(), ExportStats { accepted: 1, failed: 1, ..Default::default() });
            server.await.unwrap();
        }).await.unwrap();
    }
}
