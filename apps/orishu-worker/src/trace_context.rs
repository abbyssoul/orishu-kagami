//! Fixed-size IO-owned trace context shared by client and peer adapters.
//!
//! Parsing is not authentication. Callers must validate their client authority
//! or peer/session binding before adopting a remote parent. This module has no
//! exporter dependency and never changes membership or operation identity.

/// Maximum input inspected by optional context processing, in bytes.
pub const MAX_TRACE_PARENT_BYTES: usize = 128;
/// Canonical version-00 output size, without HTTP or CBOR framing.
pub const TRACE_PARENT_BYTES: usize = 55;

/// Validated trace identity, parent span and advisory remote sampling flag.
/// Retains no baggage, vendor state, version extension or malformed input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceParent {
    trace: [u8; 16],
    span: [u8; 8],
    sampled: bool,
}

impl TraceParent {
    /// Construct from nonzero IDs. Sampling is metadata, not local permission
    /// to create/export a span or reserve any domain/telemetry capacity.
    pub fn new(trace: [u8; 16], span: [u8; 8], sampled: bool) -> Option<Self> {
        if trace == [0; 16] || span == [0; 8] {
            return None;
        }
        Some(Self {
            trace,
            span,
            sampled,
        })
    }

    /// Parse the protocol's bounded W3C subset without allocating. Missing or
    /// invalid context is discarded by the caller, not a domain rejection.
    /// Work and storage are constant, including oversized inputs.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if !(TRACE_PARENT_BYTES..=MAX_TRACE_PARENT_BYTES).contains(&bytes.len())
            || bytes[2] != b'-'
            || bytes[35] != b'-'
            || bytes[52] != b'-'
        {
            return None;
        }
        let version = decode_hex::<1>(&bytes[..2])?[0];
        if version == 255
            || (version == 0 && bytes.len() != TRACE_PARENT_BYTES)
            || (bytes.len() > TRACE_PARENT_BYTES && bytes[55] != b'-')
        {
            return None;
        }
        // Future extensions are opaque, but whitespace, control/non-ASCII
        // bytes and combined header values cannot masquerade as one parent.
        if bytes[55..]
            .iter()
            .any(|byte| !byte.is_ascii_graphic() || *byte == b',')
        {
            return None;
        }
        let trace = decode_hex::<16>(&bytes[3..35])?;
        let span = decode_hex::<8>(&bytes[36..52])?;
        let flags = decode_hex::<1>(&bytes[53..55])?[0];
        Self::new(trace, span, flags & 1 != 0)
    }

    /// Extract exactly one raw value. The caller performs case-insensitive
    /// header-name lookup and authorization. At most two values are inspected,
    /// so even a hostile iterator cannot cause unbounded duplicate scanning.
    pub fn from_values<'a>(mut values: impl Iterator<Item = &'a [u8]>) -> Option<Self> {
        let value = values.next()?;
        if values.next().is_some() {
            return None;
        }
        Self::parse(value)
    }

    /// Trace identity used by the OTLP record, not a subscriber-local handle.
    pub fn trace_id(self) -> [u8; 16] {
        self.trace
    }

    /// Span that the receiving adapter may use as its parent.
    pub fn span_id(self) -> [u8; 8] {
        self.span
    }

    /// Remote advice only; local sampling and budgets always take precedence.
    pub fn sampled(self) -> bool {
        self.sampled
    }

    /// Encode canonical version 00 into caller-owned fixed storage. Only the
    /// locally selected sampling bit should be present in an outbound value.
    pub fn encode(self) -> [u8; TRACE_PARENT_BYTES] {
        let mut output = [b'0'; TRACE_PARENT_BYTES];
        output[2] = b'-';
        output[35] = b'-';
        output[52] = b'-';
        encode_hex(&self.trace, &mut output[3..35]);
        encode_hex(&self.span, &mut output[36..52]);
        output[54] = if self.sampled { b'1' } else { b'0' };
        output
    }
}

fn decode_hex<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    let nibble = |byte| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    };
    if bytes.len() != N * 2 {
        return None;
    }
    let mut output = [0; N];
    for (value, pair) in output.iter_mut().zip(bytes.chunks_exact(2)) {
        *value = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(output)
}

fn encode_hex(input: &[u8], output: &mut [u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (byte, pair) in input.iter().zip(output.chunks_exact_mut(2)) {
        pair[0] = HEX[usize::from(byte >> 4)];
        pair[1] = HEX[usize::from(byte & 15)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: &[u8; 55] = b"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn canonical_bytes_and_nonzero_construction() {
        let parent = TraceParent::parse(PARENT).unwrap();
        assert_eq!(parent.encode(), *PARENT);
        assert!(parent.sampled());
        assert_eq!(std::mem::size_of::<TraceParent>(), 25);
        assert!(TraceParent::new([0; 16], [1; 8], true).is_none());
        assert!(TraceParent::new([1; 16], [0; 8], true).is_none());
        for flags in 0..=255_u8 {
            let mut input = *PARENT;
            encode_hex(&[flags], &mut input[53..]);
            let parsed = TraceParent::parse(&input).unwrap();
            assert_eq!(parsed.sampled(), flags & 1 != 0);
            assert_eq!(parsed.encode()[53], b'0');
            assert_eq!(
                parsed.encode()[54],
                if flags & 1 == 1 { b'1' } else { b'0' }
            );
        }
    }

    #[test]
    fn malformed_lengths_ids_hex_and_separators_are_discarded() {
        for length in 0..55 {
            assert!(TraceParent::parse(&PARENT[..length]).is_none());
        }
        for index in 0..55 {
            let mut input = *PARENT;
            input[index] = b'G';
            assert!(TraceParent::parse(&input).is_none(), "byte {index}");
        }
        for range in [3..35, 36..52] {
            let mut input = *PARENT;
            input[range].fill(b'0');
            assert!(TraceParent::parse(&input).is_none());
        }
        let mut upper = *PARENT;
        upper[4] = b'B';
        assert!(TraceParent::parse(&upper).is_none());
        assert!(TraceParent::parse(&[b'0'; 129]).is_none());
    }

    #[test]
    fn future_versions_are_bounded_and_never_forwarded() {
        for version in 0..=255_u8 {
            let mut input = [b'x'; 128];
            input[..55].copy_from_slice(PARENT);
            encode_hex(&[version], &mut input[..2]);
            input[55] = b'-';
            assert_eq!(TraceParent::parse(&input[..55]).is_some(), version != 255);
            assert_eq!(
                TraceParent::parse(&input).is_some(),
                (1..255).contains(&version)
            );
            if let Some(parent) = TraceParent::parse(&input) {
                assert_eq!(parent.encode(), *PARENT);
            }
        }
        let mut future = PARENT.to_vec();
        future[1] = b'1';
        future.extend_from_slice(b"-extension");
        let mut maximum = future.clone();
        maximum.resize(128, b'x');
        assert!(TraceParent::parse(&maximum).is_some());
        maximum.push(b'x');
        assert!(TraceParent::parse(&maximum).is_none());
        for byte in [b',', b' ', b'\n', b'\r', b'\t', 0, 127, 255] {
            *future.last_mut().unwrap() = byte;
            assert!(TraceParent::parse(&future).is_none());
        }
    }

    #[test]
    fn duplicate_values_are_not_joined_or_scanned_without_bound() {
        assert!(TraceParent::from_values(std::iter::empty()).is_none());
        assert!(TraceParent::from_values([PARENT.as_slice()].into_iter()).is_some());
        assert!(TraceParent::from_values(std::iter::repeat(PARENT.as_slice())).is_none());
        let joined = [PARENT.as_slice(), b",", PARENT.as_slice()].concat();
        assert!(TraceParent::from_values([joined.as_slice()].into_iter()).is_none());
        let mut future = joined;
        future[1] = b'1';
        future[55] = b'-';
        future[60] = b',';
        assert!(TraceParent::parse(&future).is_none());
    }
}
