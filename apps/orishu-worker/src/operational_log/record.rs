//! Fixed vocabulary and stack-only JSON encoding. No arbitrary Display/Debug input.
use crate::trace_context::TraceParent;
use std::fmt::{self, Write};

/// Maximum complete JSON-line size, including its terminating newline.
pub const RECORD_BYTES: usize = 320;

/// Reviewed worker IO events, never derived from request paths or error text.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// Configured roles have completed initialization.
    Ready,
    /// Normal shutdown was requested.
    Stopping,
    /// Supervised worker shutdown completed.
    Stopped,
    /// A required runtime role failed.
    RuntimeFailed,
    /// The optional diagnostics server stopped unexpectedly.
    DiagnosticsFailed,
    /// The optional trace exporter task stopped unexpectedly.
    TraceExporterFailed,
    /// One fixed-name final trace accounting value, emitted best-effort.
    TraceAccounting,
    /// One sampled client invocation completed.
    ClientRequest,
    /// One sampled admission handler completed.
    Admission,
    /// One sampled outbound peer exchange completed.
    PeerExchange,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Self::Ready => "orishu.worker.ready",
            Self::Stopping => "orishu.worker.stopping",
            Self::Stopped => "orishu.worker.stopped",
            Self::RuntimeFailed => "orishu.worker.failed",
            Self::DiagnosticsFailed => "orishu.diagnostics.failed",
            Self::TraceExporterFailed => "orishu.trace_exporter.failed",
            Self::TraceAccounting => "orishu.trace.accounting",
            Self::ClientRequest => "orishu.client.request",
            Self::Admission => "orishu.admission",
            Self::PeerExchange => "orishu.peer.exchange",
        }
    }
}

/// Bounded diagnostic outcome; not an authoritative command receipt.
#[derive(Clone, Copy, Debug)]
pub enum Outcome {
    /// The observed adapter completed.
    Completed,
    /// The observed adapter refused the operation.
    Rejected,
    /// The observed adapter failed.
    Failed,
    /// The observed adapter was cancelled.
    Cancelled,
}

impl Outcome {
    fn name(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Complete log input. IDs must come from the actual local OTLP span, not a
/// subscriber-local handle or an unauthenticated incoming header.
#[derive(Clone, Copy)]
pub struct Record {
    event: Event,
    outcome: Outcome,
    unix_nanos: u64,
    context: Option<TraceParent>,
    counter: Option<(TraceCounter, u64)>,
}

/// Finite final trace accounting catalogue; never a caller-supplied label.
#[derive(Clone, Copy)]
pub enum TraceCounter {
    /// Locally sampled-out operations.
    SampledOut,
    /// Refused active spans.
    ActiveFull,
    /// Completed spans shed at queue capacity.
    QueueFull,
    /// Spans refused after queue closure.
    Closed,
    /// Invalid local identity/timestamp sources.
    InvalidSource,
    /// Completed spans queued.
    Enqueued,
    /// Spans accepted by the collector.
    Accepted,
    /// Spans rejected by the collector.
    Rejected,
    /// Spans in failed export attempts.
    Failed,
    /// Spans refused by the bounded encoder.
    EncodingDropped,
    /// Spans abandoned at exporter shutdown.
    ShutdownDropped,
    /// Collector warnings.
    Warnings,
}

impl TraceCounter {
    fn name(self) -> &'static str {
        match self {
            Self::SampledOut => "sampled_out",
            Self::ActiveFull => "active_full",
            Self::QueueFull => "queue_full",
            Self::Closed => "closed",
            Self::InvalidSource => "invalid_source",
            Self::Enqueued => "enqueued",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::EncodingDropped => "encoding_dropped",
            Self::ShutdownDropped => "shutdown_dropped",
            Self::Warnings => "warnings",
        }
    }
}

impl Record {
    /// Construct without clocks, allocation, dynamic fields or formatting.
    pub fn new(
        event: Event,
        outcome: Outcome,
        unix_nanos: u64,
        context: Option<TraceParent>,
    ) -> Self {
        Self {
            event,
            outcome,
            unix_nanos,
            context,
            counter: None,
        }
    }

    /// One finite final trace counter; cannot also carry a span identity.
    pub fn trace_counter(counter: TraceCounter, value: u64, unix_nanos: u64) -> Self {
        Self {
            event: Event::TraceAccounting,
            outcome: Outcome::Completed,
            unix_nanos,
            context: None,
            counter: Some((counter, value)),
        }
    }

    pub(super) fn encode(self) -> Result<Frame, fmt::Error> {
        let mut frame = Frame {
            bytes: [0; RECORD_BYTES],
            len: 0,
        };
        write!(
            frame,
            "{{\"version\":1,\"event\":\"{}\",\"outcome\":\"{}\",\"unix_nanos\":{}",
            self.event.name(),
            self.outcome.name(),
            self.unix_nanos
        )?;
        if let Some(context) = self.context {
            let encoded = context.encode();
            // The fixed context encoder produces only canonical ASCII hex.
            let trace = std::str::from_utf8(&encoded[3..35]).map_err(|_| fmt::Error)?;
            let span = std::str::from_utf8(&encoded[36..52]).map_err(|_| fmt::Error)?;
            write!(frame, ",\"trace_id\":\"{trace}\",\"span_id\":\"{span}\"")?;
        }
        if let Some((counter, value)) = self.counter {
            write!(
                frame,
                ",\"counter\":\"{}\",\"value\":{value}",
                counter.name()
            )?;
        }
        frame.write_str("}\n")?;
        Ok(frame)
    }
}

pub(super) struct Frame {
    bytes: [u8; RECORD_BYTES],
    len: usize,
}

impl Frame {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl Write for Frame {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_catalogue_fits_and_correlates_without_dynamic_fields() {
        let context = TraceParent::new([0xab; 16], [0xcd; 8], true).unwrap();
        for event in [
            Event::Ready,
            Event::Stopping,
            Event::Stopped,
            Event::RuntimeFailed,
            Event::DiagnosticsFailed,
            Event::TraceExporterFailed,
            Event::TraceAccounting,
            Event::ClientRequest,
            Event::Admission,
            Event::PeerExchange,
        ] {
            for outcome in [
                Outcome::Completed,
                Outcome::Rejected,
                Outcome::Failed,
                Outcome::Cancelled,
            ] {
                for parent in [None, Some(context)] {
                    let frame = Record::new(event, outcome, u64::MAX, parent)
                        .encode()
                        .unwrap();
                    assert!(frame.bytes().len() <= RECORD_BYTES);
                    assert_eq!(frame.bytes().last(), Some(&b'\n'));
                    assert_eq!(
                        frame.bytes().iter().filter(|byte| **byte == b'\n').count(),
                        1
                    );
                    let json: serde_json::Value = serde_json::from_slice(frame.bytes()).unwrap();
                    assert_eq!(json["event"], event.name());
                    assert_eq!(json["outcome"], outcome.name());
                    assert_eq!(json["unix_nanos"], u64::MAX);
                    assert_eq!(
                        json.as_object().unwrap().len(),
                        if parent.is_some() { 6 } else { 4 }
                    );
                    if parent.is_some() {
                        assert_eq!(json["trace_id"], "abababababababababababababababab");
                        assert_eq!(json["span_id"], "cdcdcdcdcdcdcdcd");
                    }
                }
            }
        }
    }

    #[test]
    fn encoder_refuses_overflow_without_partial_append() {
        let mut frame = Frame {
            bytes: [0; RECORD_BYTES],
            len: 0,
        };
        frame.write_str("ok").unwrap();
        assert!(frame.write_str(&"x".repeat(RECORD_BYTES)).is_err());
        assert_eq!(frame.bytes(), b"ok");
    }

    #[test]
    fn final_trace_counter_catalogue_fits_without_identity_or_dynamic_labels() {
        for counter in [
            TraceCounter::SampledOut,
            TraceCounter::ActiveFull,
            TraceCounter::QueueFull,
            TraceCounter::Closed,
            TraceCounter::InvalidSource,
            TraceCounter::Enqueued,
            TraceCounter::Accepted,
            TraceCounter::Rejected,
            TraceCounter::Failed,
            TraceCounter::EncodingDropped,
            TraceCounter::ShutdownDropped,
            TraceCounter::Warnings,
        ] {
            let frame = Record::trace_counter(counter, u64::MAX, u64::MAX)
                .encode()
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(frame.bytes()).unwrap();
            assert_eq!(json.as_object().unwrap().len(), 6);
            assert_eq!(json["counter"], counter.name());
            assert_eq!(json["value"], u64::MAX);
            assert!(json.get("trace_id").is_none());
            assert!(frame.bytes().len() <= RECORD_BYTES);
        }
    }
}
