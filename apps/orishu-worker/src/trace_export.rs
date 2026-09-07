//! Bounded optional OTLP export in the worker IO shell.
//!
//! Records contain fixed-size IDs and finite operation/outcome vocabulary, never
//! request paths, credentials, arbitrary attributes or membership authority.
use opentelemetry_proto::tonic::{
    collector::trace::v1::ExportTraceServiceRequest,
    common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value},
    resource::v1::Resource,
    trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span::SpanKind, status::StatusCode},
};
use prost::Message;

mod queue;
pub use queue::{ActiveSpan, QueueError, QueueStats, SpanQueue};
mod http;
pub use http::{DeliveryError, DeliveryOutcome, HttpDelivery};
mod credentials;
pub use credentials::CollectorFiles;
mod worker;
pub use worker::{DeliveryCounters, ExportLoop, ExportStats};
mod requests;
pub use requests::TraceRequests;

/// Static instrumentation vocabulary; not derived from user input.
#[derive(Clone, Copy, Debug)]
pub enum Operation {
    /// One client service invocation, not transport delivery or domain acceptance.
    ClientRequest,
    /// One admission operation in the worker adapter.
    Admission,
    /// One identified peer exchange, not an aggregate gossip causality claim.
    PeerExchange,
}

/// Diagnostic outcome, independent of authoritative operation receipts.
#[derive(Clone, Copy, Debug)]
pub enum Outcome {
    /// The observed adapter operation completed.
    Completed,
    /// The adapter refused the operation.
    Rejected,
    /// The adapter failed the operation.
    Failed,
    /// The operation was cancelled before completion.
    Cancelled,
}

/// Fixed-size completed span suitable for a separately bounded export queue.
#[derive(Clone, Copy)]
pub struct SpanRecord {
    trace: [u8; 16],
    span: [u8; 8],
    parent: Option<[u8; 8]>,
    operation: Operation,
    start: u64,
    end: u64,
    outcome: Outcome,
}

/// Fixed, secret-free errors at the diagnostic encoding boundary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncodeError {
    /// IDs must be nonzero and timestamps ordered Unix nanoseconds.
    #[error("invalid trace identity or timestamp ordering")]
    InvalidRecord,
    /// Batch count or byte limits are outside the startup contract.
    #[error("invalid trace batch limits")]
    InvalidLimits,
    /// The first record cannot fit; the caller must shed it rather than stall.
    #[error("trace record exceeds the export byte budget")]
    RecordTooLarge,
    /// The maintained schema no longer agrees with the size calculation.
    #[error("trace protobuf size calculation mismatch")]
    Encoding,
}

impl SpanRecord {
    /// Validate IDs and timestamp ordering without allocation. Timestamps are
    /// supplied by the IO adapter; this function does not read a clock or RNG.
    pub fn new(
        trace: [u8; 16],
        span: [u8; 8],
        parent: Option<[u8; 8]>,
        operation: Operation,
        start: u64,
        end: u64,
        outcome: Outcome,
    ) -> Result<Self, EncodeError> {
        if trace == [0; 16]
            || span == [0; 8]
            || parent == Some([0; 8])
            || parent == Some(span)
            || end < start
        {
            return Err(EncodeError::InvalidRecord);
        }
        Ok(Self {
            trace,
            span,
            parent,
            operation,
            start,
            end,
            outcome,
        })
    }

    fn message(self) -> Span {
        let (name, kind) = match self.operation {
            Operation::ClientRequest => ("orishu.client.request", SpanKind::Server),
            Operation::Admission => ("orishu.admission", SpanKind::Internal),
            Operation::PeerExchange => ("orishu.peer.exchange", SpanKind::Client),
        };
        let outcome = match self.outcome {
            Outcome::Completed => "completed",
            Outcome::Rejected => "rejected",
            Outcome::Failed => "failed",
            Outcome::Cancelled => "cancelled",
        };
        Span {
            trace_id: self.trace.to_vec(),
            span_id: self.span.to_vec(),
            parent_span_id: self.parent.map_or_else(Vec::new, |id| id.to_vec()),
            flags: 1, // These records represent sampled spans only.
            name: name.into(),
            kind: kind.into(),
            start_time_unix_nano: self.start,
            end_time_unix_nano: self.end,
            attributes: vec![attribute("orishu.outcome", outcome)],
            status: Some(Status {
                code: if matches!(self.outcome, Outcome::Failed) {
                    StatusCode::Error.into()
                } else {
                    StatusCode::Unset.into()
                },
                message: String::new(),
            }),
            ..Default::default()
        }
    }
}

fn attribute(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.into())),
        }),
        ..Default::default()
    }
}

/// Validated count and encoded-request bounds, independent of queue capacity.
pub struct BatchLimits {
    spans: usize,
    bytes: usize,
}

/// One bounded request and the number of input records it consumes.
pub struct EncodedBatch {
    body: Vec<u8>,
    consumed: usize,
}

impl EncodedBatch {
    /// Binary OTLP `ExportTraceServiceRequest` body, without HTTP framing.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    /// Prefix removed from the export queue after this batch is handled.
    pub fn consumed(&self) -> usize {
        self.consumed
    }
}

impl BatchLimits {
    /// Enforce the same absolute bounds as staged startup configuration.
    pub fn new(spans: usize, bytes: usize) -> Result<Self, EncodeError> {
        if !(1..=256).contains(&spans) || !(1024..=4_194_304).contains(&bytes) {
            return Err(EncodeError::InvalidLimits);
        }
        Ok(Self { spans, bytes })
    }

    /// Encode the largest permitted prefix in O(selected spans) work and bounded
    /// space. Empty input returns an empty body and consumes nothing. The caller
    /// retains the tail for another batch; an oversized first record is explicit.
    ///
    /// Generated message allocation is bounded by the fixed record vocabulary
    /// and count cap. The output is sized before allocation, then encoded into a
    /// fixed slice: the serializer cannot grow it past the configured byte cap.
    pub fn encode(&self, records: &[SpanRecord]) -> Result<EncodedBatch, EncodeError> {
        if records.is_empty() {
            return Ok(EncodedBatch {
                body: Vec::new(),
                consumed: 0,
            });
        }
        let mut resource = ResourceSpans {
            resource: Some(Resource {
                attributes: vec![attribute("service.name", "orishu-worker")],
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut scope = ScopeSpans {
            scope: Some(InstrumentationScope {
                name: "orishu.worker".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let resource_base = resource.encoded_len();
        let mut scope_size = scope.encoded_len();
        let mut total = 0;
        for record in records.iter().take(self.spans) {
            let span = record.message();
            let next_scope = scope_size + delimited(span.encoded_len());
            let next_total = delimited(resource_base + delimited(next_scope));
            if next_total > self.bytes {
                break;
            }
            scope_size = next_scope;
            total = next_total;
            scope.spans.push(span);
        }
        let consumed = scope.spans.len();
        if consumed == 0 {
            return Err(EncodeError::RecordTooLarge);
        }
        resource.scope_spans.push(scope);
        let request = ExportTraceServiceRequest {
            resource_spans: vec![resource],
        };
        if request.encoded_len() != total {
            return Err(EncodeError::Encoding);
        }
        let mut body = vec![0; total];
        let mut remaining = body.as_mut_slice();
        request
            .encode(&mut remaining)
            .map_err(|_| EncodeError::Encoding)?;
        if !remaining.is_empty() {
            return Err(EncodeError::Encoding);
        }
        Ok(EncodedBatch { body, consumed })
    }
}

// Span, ScopeSpans and ResourceSpans wrapper fields all have one-byte tags.
// Length prefixes must be recomputed across protobuf varint boundaries.
fn delimited(size: usize) -> usize {
    1 + prost::length_delimiter_len(size) + size
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(index: u8) -> SpanRecord {
        SpanRecord::new(
            [1; 16],
            [index; 8],
            Some([255; 8]),
            Operation::ClientRequest,
            123,
            456,
            Outcome::Completed,
        )
        .unwrap()
    }

    #[test]
    fn identities_and_time_are_validated_before_encoding() {
        for (trace, span, parent, start, end) in [
            ([0; 16], [1; 8], None, 0, 1),
            ([1; 16], [0; 8], None, 0, 1),
            ([1; 16], [1; 8], Some([0; 8]), 0, 1),
            ([1; 16], [1; 8], Some([1; 8]), 0, 1),
            ([1; 16], [1; 8], None, 2, 1),
        ] {
            assert!(matches!(
                SpanRecord::new(
                    trace,
                    span,
                    parent,
                    Operation::Admission,
                    start,
                    end,
                    Outcome::Rejected
                ),
                Err(EncodeError::InvalidRecord)
            ));
        }
    }

    #[test]
    fn exact_byte_cap_splits_without_losing_or_reordering_records() {
        let records: Vec<_> = (1..=100).map(record).collect();
        let whole = BatchLimits::new(100, 4_194_304)
            .unwrap()
            .encode(&records)
            .unwrap();
        let exact = BatchLimits::new(100, whole.body().len())
            .unwrap()
            .encode(&records)
            .unwrap();
        assert_eq!(exact.body(), whole.body());
        assert_eq!(exact.consumed(), 100);
        let split = BatchLimits::new(100, whole.body().len() - 1)
            .unwrap()
            .encode(&records)
            .unwrap();
        assert_eq!(split.consumed(), 99);
        let limits = BatchLimits::new(17, 1024).unwrap();
        let mut offset = 0;
        while offset < records.len() {
            let batch = limits.encode(&records[offset..]).unwrap();
            assert!(batch.body().len() <= 1024);
            assert!((1..=17).contains(&batch.consumed()));
            let request = ExportTraceServiceRequest::decode(batch.body()).unwrap();
            let spans = &request.resource_spans[0].scope_spans[0].spans;
            assert_eq!(spans.len(), batch.consumed());
            for (i, span) in spans.iter().enumerate() {
                assert_eq!(span.span_id, vec![(offset + i + 1) as u8; 8]);
                assert_eq!(span.trace_id, vec![1; 16]);
                assert_eq!(span.parent_span_id, vec![255; 8]);
                assert_eq!(
                    (span.start_time_unix_nano, span.end_time_unix_nano),
                    (123, 456)
                );
                assert!(
                    span.events.is_empty() && span.links.is_empty() && span.trace_state.is_empty()
                );
            }
            offset += batch.consumed();
        }
    }

    #[test]
    fn maximum_batch_crosses_nested_varint_boundaries() {
        let records: Vec<_> = (1_u64..=256)
            .map(|id| {
                SpanRecord::new(
                    [1; 16],
                    id.to_be_bytes(),
                    None,
                    Operation::PeerExchange,
                    1,
                    2,
                    Outcome::Failed,
                )
                .unwrap()
            })
            .collect();
        let batch = BatchLimits::new(256, 4_194_304)
            .unwrap()
            .encode(&records)
            .unwrap();
        assert_eq!(batch.consumed(), 256);
        assert!(batch.body().len() > 16_383);
        let decoded = ExportTraceServiceRequest::decode(batch.body()).unwrap();
        assert_eq!(decoded.encoded_len(), batch.body().len());
        let spans = &decoded.resource_spans[0].scope_spans[0].spans;
        assert_eq!(spans.len(), 256);
        assert!(spans.iter().all(|span| span.parent_span_id.is_empty()
            && span.status.as_ref().unwrap().code == i32::from(StatusCode::Error)));
        let too_small = BatchLimits { spans: 1, bytes: 1 };
        assert!(matches!(
            too_small.encode(&records),
            Err(EncodeError::RecordTooLarge)
        ));
    }

    #[test]
    fn count_limit_empty_input_and_invalid_limits() {
        let limits = BatchLimits::new(1, 1024).unwrap();
        assert_eq!(
            limits.encode(&[record(1), record(2)]).unwrap().consumed(),
            1
        );
        let empty = limits.encode(&[]).unwrap();
        assert!(empty.body().is_empty());
        assert_eq!(empty.consumed(), 0);
        for (count, bytes) in [(0, 1024), (257, 1024), (1, 1023), (1, 4_194_305)] {
            assert!(matches!(
                BatchLimits::new(count, bytes),
                Err(EncodeError::InvalidLimits)
            ));
        }
    }
}
