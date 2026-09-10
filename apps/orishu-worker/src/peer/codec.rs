//! Allocation-bounded CBOR preflight for untrusted membership frames.
//!
//! Validate lengths, nesting, total work and duplicate keys before
//! serde constructs owned collections. This accepts the membership profile,
//! not arbitrary CBOR: tags, indefinite strings/arrays and floating point are
//! excluded. Bounded indefinite maps support serde's flattened record encoding.

use std::io::Cursor;

use serde::{Serialize, de::DeserializeOwned};

/// Hard limits for the formation wire profile, independent of peer claims.
pub const MAX_FRAME_BYTES: usize = 1_048_576;
const MAX_DEPTH: usize = 24;
const MAX_ITEMS: usize = 32_768;
const MAX_COLLECTION: usize = 4096;
const MAX_MAP_FIELDS: usize = 64;
const MAX_TEXT_BYTES: usize = 4096;

/// A bounded, non-payload-bearing rejection reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// Encoded or declared bytes exceed the frame cap.
    #[error("peer frame exceeds byte limit")]
    TooLarge,
    /// A collection, string, nesting level or work budget exceeds its cap.
    #[error("peer value exceeds structural limit")]
    Limit,
    /// Length prefixes or CBOR values end before their declared extent.
    #[error("truncated peer frame")]
    Truncated,
    /// Two fields use the same key in one map.
    #[error("duplicate peer field")]
    DuplicateKey,
    /// The wire profile excludes the encountered encoding.
    #[error("unsupported peer encoding")]
    Encoding,
    /// More than one value or undeclared bytes were supplied.
    #[error("trailing peer frame bytes")]
    Trailing,
    /// Well-formed CBOR does not satisfy the requested typed schema.
    #[error("invalid peer schema")]
    Schema,
}

/// Validate a length prefix before allocating a receive buffer.
pub fn frame_length(prefix: [u8; 4]) -> Result<usize, CodecError> {
    let length = u32::from_be_bytes(prefix) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(CodecError::TooLarge);
    }
    if length == 0 {
        return Err(CodecError::Truncated);
    }
    Ok(length)
}

/// Decode exactly one bounded CBOR value, rejecting duplicate fields before serde.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    validate(bytes)?;
    ciborium::from_reader(Cursor::new(bytes)).map_err(|_| CodecError::Schema)
}

/// Borrow a record's encoded fields without allocating its payload. This permits
/// session/envelope validation before typed payload decoding and measures the
/// actual snapshot bytes, including non-minimal (but valid) length encodings.
pub(crate) fn record_fields(bytes: &[u8]) -> Result<Vec<(&str, &[u8])>, CodecError> {
    validate(bytes)?;
    let mut scan = Scan {
        bytes,
        offset: 0,
        remaining: MAX_ITEMS,
    };
    let (major, count) = scan.header()?;
    if major != 5 {
        return Err(CodecError::Schema);
    }
    let mut fields = Vec::new();
    while if count == u64::MAX {
        scan.bytes.get(scan.offset) != Some(&0xff)
    } else {
        fields.len() < count as usize
    } {
        let (_, length) = scan.header()?;
        let key = std::str::from_utf8(scan.text(length)?).map_err(|_| CodecError::Encoding)?;
        let start = scan.offset;
        scan.value(1)?;
        fields.push((key, &bytes[start..scan.offset]));
    }
    Ok(fields)
}

/// Borrow one definite text value within an adapter-specific cap. Optional
/// metadata callers must first run ordinary structural/frame validation: this
/// returns `None` for non-text/oversized metadata, not a domain rejection.
pub(crate) fn bounded_text(bytes: &[u8], limit: usize) -> Option<&str> {
    let mut scan = Scan {
        bytes,
        offset: 0,
        remaining: MAX_ITEMS,
    };
    let (major, length) = scan.header().ok()?;
    if major != 3 || length > limit as u64 {
        return None;
    }
    let text = scan.text(length).ok()?;
    if scan.offset != bytes.len() {
        return None;
    }
    std::str::from_utf8(text).ok()
}

/// Encode a value with a hard output cap and check the same profile as receive.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let mut output = CappedWriter(Vec::new());
    ciborium::into_writer(value, &mut output).map_err(|_| CodecError::TooLarge)?;
    validate(&output.0)?;
    Ok(output.0)
}

/// Decode one four-byte-length-prefixed stream frame, with no trailing data.
pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T, CodecError> {
    let prefix = frame.get(..4).ok_or(CodecError::Truncated)?;
    let length = frame_length(prefix.try_into().map_err(|_| CodecError::Truncated)?)?;
    if frame.len() < 4 + length {
        return Err(CodecError::Truncated);
    }
    if frame.len() != 4 + length {
        return Err(CodecError::Trailing);
    }
    decode(&frame[4..])
}

/// Encode a bounded stream frame. Datagrams use [`encode`] without a prefix.
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let payload = encode(value)?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

struct CappedWriter(Vec<u8>);

impl std::io::Write for CappedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_FRAME_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::ErrorKind::OutOfMemory.into());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn validate(bytes: &[u8]) -> Result<(), CodecError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(CodecError::TooLarge);
    }
    let mut parser = Scan {
        bytes,
        offset: 0,
        remaining: MAX_ITEMS,
    };
    parser.value(0)?;
    if parser.offset != bytes.len() {
        return Err(CodecError::Trailing);
    }
    Ok(())
}

struct Scan<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: usize,
}

impl<'a> Scan<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        let end = self.offset.checked_add(length).ok_or(CodecError::Limit)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(CodecError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn header(&mut self) -> Result<(u8, u64), CodecError> {
        self.remaining = self.remaining.checked_sub(1).ok_or(CodecError::Limit)?;
        let initial = self.take(1)?[0];
        let argument = match initial & 31 {
            small @ 0..=23 => u64::from(small),
            size @ 24..=27 => {
                let mut value = 0_u64;
                for byte in self.take(1 << (size - 24))? {
                    value = (value << 8) | u64::from(*byte);
                }
                value
            }
            31 if initial == 0xbf => u64::MAX,
            _ => return Err(CodecError::Encoding),
        };
        // Only direct encodings of false, true and null are admitted simple values.
        if initial >> 5 == 7 && !matches!(initial, 0xf4..=0xf6) {
            return Err(CodecError::Encoding);
        }
        Ok((initial >> 5, argument))
    }

    fn text(&mut self, length: u64) -> Result<&'a [u8], CodecError> {
        if length > MAX_TEXT_BYTES as u64 {
            return Err(CodecError::Limit);
        }
        let text = self.take(length as usize)?;
        std::str::from_utf8(text).map_err(|_| CodecError::Encoding)?;
        Ok(text)
    }

    fn value(&mut self, depth: usize) -> Result<(), CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::Limit);
        }
        let (major, argument) = self.header()?;
        match major {
            0 | 1 | 7 => {}
            2 => {
                let length = usize::try_from(argument).map_err(|_| CodecError::Limit)?;
                if length > MAX_FRAME_BYTES {
                    return Err(CodecError::TooLarge);
                }
                self.take(length)?;
            }
            3 => {
                self.text(argument)?;
            }
            4 => {
                if argument > MAX_COLLECTION as u64 {
                    return Err(CodecError::Limit);
                }
                for _ in 0..argument {
                    self.value(depth + 1)?;
                }
            }
            5 => {
                let indefinite = argument == u64::MAX;
                if !indefinite && argument > MAX_MAP_FIELDS as u64 {
                    return Err(CodecError::Limit);
                }
                // Keys borrow the bounded frame; no attacker-sized allocations.
                let mut keys = [None; MAX_MAP_FIELDS];
                let mut index = 0;
                loop {
                    if indefinite && self.bytes.get(self.offset) == Some(&0xff) {
                        self.take(1)?;
                        break;
                    }
                    if !indefinite && index == argument as usize {
                        break;
                    }
                    if index == MAX_MAP_FIELDS {
                        return Err(CodecError::Limit);
                    }
                    let (kind, length) = self.header()?;
                    if kind != 3 {
                        return Err(CodecError::Encoding);
                    }
                    let key = self.text(length)?;
                    if keys[..index].contains(&Some(key)) {
                        return Err(CodecError::DuplicateKey);
                    }
                    keys[index] = Some(key);
                    // These names have fixed collection meanings in profile 2.
                    // Check before serde reserves an attacker-declared capacity.
                    let bound = match key {
                        b"gossip" => 10,
                        b"deltas" => 1000,
                        b"nodeHashes" | b"buckets" => 256,
                        b"accelerators" => 32,
                        b"engines" => 16,
                        b"peers" | b"clients" | b"redirectTo" => 8,
                        _ => MAX_COLLECTION,
                    };
                    if self
                        .bytes
                        .get(self.offset)
                        .is_some_and(|byte| byte >> 5 == 4)
                    {
                        let saved = (self.offset, self.remaining);
                        let (_, count) = self.header()?;
                        if count > bound as u64 {
                            return Err(CodecError::Limit);
                        }
                        (self.offset, self.remaining) = saved;
                    }
                    self.value(depth + 1)?;
                    index += 1;
                }
            }
            _ => return Err(CodecError::Encoding),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Payload {
        locked: bool,
    }

    #[test]
    fn output_cap_accepts_exact_limit_and_refuses_growth_before_copying() {
        use std::io::Write;
        struct Bytes<'a>(&'a [u8]);
        impl Serialize for Bytes<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_bytes(self.0)
            }
        }
        // A byte string this large has a five-byte CBOR header. Exercise the
        // same serializer used for client responses and membership packets.
        let payload = vec![0x5a; MAX_FRAME_BYTES - 5];
        let encoded = encode(&Bytes(&payload)).unwrap();
        assert_eq!(encoded.len(), MAX_FRAME_BYTES);
        assert_eq!(validate(&encoded), Ok(()));
        assert_eq!(
            frame_length((encoded.len() as u32).to_be_bytes()),
            Ok(MAX_FRAME_BYTES)
        );
        let oversized = vec![0x5a; MAX_FRAME_BYTES - 4];
        assert_eq!(encode(&Bytes(&oversized)), Err(CodecError::TooLarge));
        let mut writer = CappedWriter(encoded);
        let capacity = writer.0.capacity();
        assert_eq!(
            writer.write(&[0]).unwrap_err().kind(),
            std::io::ErrorKind::OutOfMemory
        );
        assert_eq!(writer.0.len(), MAX_FRAME_BYTES);
        assert_eq!(writer.0.capacity(), capacity);
        assert_eq!(writer.0.last(), Some(&0x5a));
        assert_eq!(writer.write(&[]).unwrap(), 0);
    }

    #[test]
    fn golden_frame_and_every_truncation() {
        let value = Payload { locked: true };
        let frame = encode_frame(&value).unwrap();
        assert_eq!(frame, b"\0\0\0\x09\xa1\x66locked\xf5");
        assert_eq!(decode_frame::<Payload>(&frame), Ok(value));
        for end in 0..frame.len() {
            assert!(decode_frame::<Payload>(&frame[..end]).is_err());
        }
        let mut extra = frame.clone();
        extra.push(0);
        assert_eq!(decode_frame::<Payload>(&extra), Err(CodecError::Trailing));
    }

    #[test]
    fn rejects_hostile_lengths_duplicates_and_unsupported_values() {
        assert_eq!(
            frame_length(u32::MAX.to_be_bytes()),
            Err(CodecError::TooLarge)
        );
        for bytes in [
            &b"\xa2\x66locked\xf5\x66locked\xf4"[..],
            &b"\x9a\xff\xff\xff\xff"[..],
            &b"\x7a\xff\xff\xff\xff"[..],
            &b"\x9f\xff"[..],
            &b"\xc0\x00"[..],
            &b"\xfa\x00\x00\x00\x00"[..],
        ] {
            assert!(decode::<Payload>(bytes).is_err());
        }
        assert_eq!(
            validate(b"\xa2\x61x\x00\x78\x01x\x01"),
            Err(CodecError::DuplicateKey)
        );
        let mut nested = vec![0x81; MAX_DEPTH + 1];
        nested.push(0);
        assert_eq!(validate(&nested), Err(CodecError::Limit));
    }

    #[test]
    fn unknown_fields_are_schema_errors_and_payloads_are_never_in_errors() {
        assert_eq!(
            decode::<Payload>(b"\xa1\x66secret\xf5"),
            Err(CodecError::Schema)
        );
        assert_eq!(CodecError::Schema.to_string(), "invalid peer schema");
    }

    #[test]
    fn flattened_records_remain_bounded_and_duplicate_checked() {
        #[derive(Serialize, Deserialize)]
        struct Flattened {
            hops: u32,
            #[serde(flatten)]
            payload: Payload,
        }
        let value = Flattened {
            hops: 1,
            payload: Payload { locked: true },
        };
        let bytes = encode(&value).unwrap();
        assert!(decode::<Flattened>(&bytes).unwrap().payload.locked);
        assert_eq!(
            validate(b"\xbf\x61x\x00\x61x\x01\xff"),
            Err(CodecError::DuplicateKey)
        );
        assert_eq!(validate(b"\xbf\x61x\x00"), Err(CodecError::Truncated));
    }
}
