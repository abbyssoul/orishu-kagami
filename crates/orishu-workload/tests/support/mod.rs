//! A minimal CBOR reader, written for the tests only.
//!
//! # Why a second implementation exists
//!
//! An encoder asserted only against its own output agrees with itself about
//! anything. `crates/orishu-workload/src/canonical.rs` claims to emit
//! deterministic CBOR under RFC 8949 §4.2; this reads those bytes back with
//! code that shares nothing with it and checks the claim.
//!
//! It is deliberately strict rather than permissive. A general-purpose decoder
//! would accept an indefinite-length string or an over-long integer argument
//! and quietly normalise it, which is precisely the class of bug this is here
//! to catch — so every construct the profile forbids is an error here.
//!
//! This is a test fixture, not a general CBOR library. It handles exactly the
//! subset the profile permits.

use std::fmt;

/// A decoded CBOR value, in the subset the canonical profile permits.
#[derive(Clone, PartialEq)]
pub enum Cbor {
    Bool(bool),
    UInt(u64),
    NInt(i128),
    F64(f64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Cbor>),
    /// Entries in the order they appeared in the bytes, with each key's encoded
    /// form retained so the ordering rule can be checked as written.
    Map(Vec<(Cbor, Cbor)>),
}

impl fmt::Debug for Cbor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cbor::Bool(value) => write!(formatter, "{value}"),
            Cbor::UInt(value) => write!(formatter, "{value}"),
            Cbor::NInt(value) => write!(formatter, "{value}"),
            Cbor::F64(value) => write!(formatter, "{value}"),
            Cbor::Bytes(bytes) => write!(formatter, "<{} bytes>", bytes.len()),
            Cbor::Text(text) => write!(formatter, "{text:?}"),
            Cbor::Array(items) => formatter.debug_list().entries(items).finish(),
            Cbor::Map(entries) => formatter
                .debug_map()
                .entries(entries.iter().map(|(key, value)| (key, value)))
                .finish(),
        }
    }
}

impl Cbor {
    /// The text of a text value.
    ///
    /// # Panics
    ///
    /// Panics when the value is not text. Only used on map keys, which the
    /// profile makes text in every position the model uses.
    pub fn as_text(&self) -> &str {
        match self {
            Cbor::Text(text) => text,
            other => panic!("expected a text value, got {other:?}"),
        }
    }

    /// Calls `visit` with every map key anywhere in the value.
    pub fn walk_keys(&self, visit: &mut impl FnMut(&str)) {
        match self {
            Cbor::Array(items) => {
                for item in items {
                    item.walk_keys(visit);
                }
            }
            Cbor::Map(entries) => {
                for (key, value) in entries {
                    if let Cbor::Text(text) = key {
                        visit(text);
                    }
                    value.walk_keys(visit);
                }
            }
            _ => {}
        }
    }

    /// Asserts the properties the canonical profile promises, recursively.
    ///
    /// Length and shortest-form checks happen during decoding; what is left to
    /// check here is map key ordering and uniqueness, which is a property of a
    /// map as a whole rather than of any one head byte.
    pub fn assert_deterministic(&self, context: &str) {
        match self {
            Cbor::Array(items) => {
                for item in items {
                    item.assert_deterministic(context);
                }
            }
            Cbor::Map(entries) => {
                let encoded: Vec<Vec<u8>> =
                    entries.iter().map(|(key, _)| reencode_key(key)).collect();
                for window in encoded.windows(2) {
                    assert!(
                        window[0] < window[1],
                        "{context}: map keys must be strictly ascending by encoded bytes \
                         (RFC 8949 §4.2.1), but {:02x?} is not before {:02x?} in {self:?}",
                        window[0],
                        window[1],
                    );
                }
                for (_, value) in entries {
                    value.assert_deterministic(context);
                }
            }
            _ => {}
        }
    }
}

/// Re-encodes a decoded key so the ordering rule is checked against bytes
/// rather than against the decoded string.
fn reencode_key(key: &Cbor) -> Vec<u8> {
    let mut out = Vec::new();
    match key {
        Cbor::Text(text) => {
            write_head(3, text.len() as u64, &mut out);
            out.extend_from_slice(text.as_bytes());
        }
        Cbor::UInt(value) => write_head(0, *value, &mut out),
        other => panic!("a canonical map key must be text or an unsigned integer, got {other:?}"),
    }
    out
}

fn write_head(major: u8, argument: u64, out: &mut Vec<u8>) {
    let major = major << 5;
    match argument {
        0..=23 => out.push(major | argument as u8),
        24..=0xFF => {
            out.push(major | 24);
            out.push(argument as u8);
        }
        0x100..=0xFFFF => {
            out.push(major | 25);
            out.extend_from_slice(&(argument as u16).to_be_bytes());
        }
        0x1_0000..=0xFFFF_FFFF => {
            out.push(major | 26);
            out.extend_from_slice(&(argument as u32).to_be_bytes());
        }
        _ => {
            out.push(major | 27);
            out.extend_from_slice(&argument.to_be_bytes());
        }
    }
}

/// Why some bytes are not canonical CBOR.
#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Decodes one canonical value, returning it and how many bytes it used.
///
/// # Errors
///
/// Returns [`DecodeError`] for truncated input and for every construct the
/// deterministic profile forbids: indefinite lengths, non-shortest integer
/// arguments, tags, floats narrower than 64 bits, and simple values other than
/// `true` and `false`.
pub fn decode(bytes: &[u8]) -> Result<(Cbor, usize), DecodeError> {
    let mut cursor = Cursor { bytes, at: 0 };
    let value = cursor.value()?;
    Ok((value, cursor.at))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, DecodeError> {
        let byte = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| DecodeError(format!("truncated at byte {}", self.at)))?;
        self.at += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&[u8], DecodeError> {
        let end = self
            .at
            .checked_add(len)
            .ok_or_else(|| DecodeError(format!("length {len} at byte {} overflows", self.at)))?;
        if end > self.bytes.len() {
            return Err(DecodeError(format!(
                "wanted {len} bytes at {} but only {} remain",
                self.at,
                self.bytes.len() - self.at
            )));
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    /// Reads a head's argument, refusing indefinite lengths and any argument
    /// not written in its shortest form.
    fn argument(&mut self, additional: u8, position: usize) -> Result<u64, DecodeError> {
        let (value, minimum) = match additional {
            0..=23 => return Ok(u64::from(additional)),
            24 => (u64::from(self.byte()?), 24),
            25 => {
                let raw = self.take(2)?;
                (u64::from(u16::from_be_bytes([raw[0], raw[1]])), 0x100)
            }
            26 => {
                let raw = self.take(4)?;
                (
                    u64::from(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])),
                    0x1_0000,
                )
            }
            27 => {
                let raw = self.take(8)?;
                let mut wide = [0u8; 8];
                wide.copy_from_slice(raw);
                (u64::from_be_bytes(wide), 0x1_0000_0000)
            }
            31 => {
                return Err(DecodeError(format!(
                    "indefinite length at byte {position}; the deterministic profile forbids it"
                )));
            }
            other => {
                return Err(DecodeError(format!(
                    "reserved additional information {other} at byte {position}"
                )));
            }
        };
        if value < minimum {
            return Err(DecodeError(format!(
                "the argument {value} at byte {position} is not in its shortest form; \
                 the deterministic profile requires it"
            )));
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Cbor, DecodeError> {
        let position = self.at;
        let head = self.byte()?;
        let major = head >> 5;
        let additional = head & 0x1f;

        match major {
            0 => Ok(Cbor::UInt(self.argument(additional, position)?)),
            1 => {
                let argument = self.argument(additional, position)?;
                Ok(Cbor::NInt(-1 - i128::from(argument)))
            }
            2 => {
                let len = self.argument(additional, position)? as usize;
                Ok(Cbor::Bytes(self.take(len)?.to_vec()))
            }
            3 => {
                let len = self.argument(additional, position)? as usize;
                let raw = self.take(len)?;
                String::from_utf8(raw.to_vec())
                    .map(Cbor::Text)
                    .map_err(|error| DecodeError(format!("invalid UTF-8 at {position}: {error}")))
            }
            4 => {
                let len = self.argument(additional, position)? as usize;
                let mut items = Vec::with_capacity(len.min(1024));
                for _ in 0..len {
                    items.push(self.value()?);
                }
                Ok(Cbor::Array(items))
            }
            5 => {
                let len = self.argument(additional, position)? as usize;
                let mut entries = Vec::with_capacity(len.min(1024));
                for _ in 0..len {
                    let key = self.value()?;
                    let value = self.value()?;
                    entries.push((key, value));
                }
                Ok(Cbor::Map(entries))
            }
            6 => Err(DecodeError(format!(
                "a tag at byte {position}; the canonical profile emits none"
            ))),
            7 => match additional {
                20 => Ok(Cbor::Bool(false)),
                21 => Ok(Cbor::Bool(true)),
                22 => Err(DecodeError(format!(
                    "null at byte {position}; the canonical profile omits absent fields instead"
                ))),
                25 | 26 => Err(DecodeError(format!(
                    "a narrowed float at byte {position}; the profile always emits 64 bits"
                ))),
                27 => {
                    let raw = self.take(8)?;
                    let mut wide = [0u8; 8];
                    wide.copy_from_slice(raw);
                    let value = f64::from_be_bytes(wide);
                    if !value.is_finite() {
                        return Err(DecodeError(format!(
                            "a non-finite float at byte {position}"
                        )));
                    }
                    if value == 0.0 && value.is_sign_negative() {
                        return Err(DecodeError(format!(
                            "negative zero at byte {position}; it must be normalised"
                        )));
                    }
                    Ok(Cbor::F64(value))
                }
                other => Err(DecodeError(format!(
                    "simple value {other} at byte {position}; only true and false are permitted"
                ))),
            },
            _ => unreachable!("a 3-bit major type is 0..=7"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reader_refuses_an_indefinite_length_array() {
        // 0x9f is an indefinite-length array head.
        let error = decode(&[0x9f, 0x01, 0xff]).unwrap_err();
        assert!(error.0.contains("indefinite"), "{error}");
    }

    #[test]
    fn the_reader_refuses_a_non_shortest_integer() {
        // 1 written with a one-byte argument instead of inline.
        let error = decode(&[0x18, 0x01]).unwrap_err();
        assert!(error.0.contains("shortest form"), "{error}");
    }

    #[test]
    fn the_reader_refuses_a_narrowed_float() {
        // 0xf9 is binary16.
        let error = decode(&[0xf9, 0x3c, 0x00]).unwrap_err();
        assert!(error.0.contains("narrowed float"), "{error}");
    }

    #[test]
    fn the_reader_refuses_null_and_tags() {
        assert!(decode(&[0xf6]).unwrap_err().0.contains("null"));
        assert!(decode(&[0xc0, 0x00]).unwrap_err().0.contains("tag"));
    }

    #[test]
    fn the_reader_refuses_negative_zero() {
        let mut bytes = vec![0xfb];
        bytes.extend_from_slice(&(-0.0f64).to_be_bytes());
        assert!(decode(&bytes).unwrap_err().0.contains("negative zero"));
    }

    #[test]
    fn the_reader_reads_what_the_profile_emits() {
        assert_eq!(decode(&[0x00]).expect("decodes").0, Cbor::UInt(0));
        assert_eq!(decode(&[0x20]).expect("decodes").0, Cbor::NInt(-1));
        assert_eq!(
            decode(&[0x61, 0x61]).expect("decodes").0,
            Cbor::Text("a".to_owned())
        );
        assert_eq!(decode(&[0xf5]).expect("decodes").0, Cbor::Bool(true));
    }

    #[test]
    fn an_unsorted_map_fails_the_determinism_assertion() {
        let unsorted = Cbor::Map(vec![
            (Cbor::Text("b".to_owned()), Cbor::UInt(1)),
            (Cbor::Text("a".to_owned()), Cbor::UInt(2)),
        ]);
        let panicked = std::panic::catch_unwind(|| unsorted.assert_deterministic("test")).is_err();
        assert!(panicked, "an unsorted map must not pass the assertion");
    }
}
