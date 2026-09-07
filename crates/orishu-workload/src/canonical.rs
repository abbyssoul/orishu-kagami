//! The deterministic encoding that gives a workload its identity.
//!
//! # Why this is not `serde`
//!
//! A workload's digest is taken over these bytes, so the encoding is a
//! contract, not an implementation detail. Deriving it would put that contract
//! at the mercy of a `#[serde(...)]` attribute: adding `skip_serializing_if` to
//! a field, reordering a struct, or renaming a variant would silently change
//! every workload's identity, and nothing in review would look like a wire
//! change.
//!
//! So the identity path is written out by hand over an explicit
//! [`CanonicalValue`] tree, and `serde` stays confined to the human authoring
//! path in [`crate::authoring`]. The two are allowed to disagree: JSON and YAML
//! are how a person writes a workload, and this is what the workload *is*.
//!
//! # The profile
//!
//! Deterministic CBOR, RFC 8949 §4.2. CBOR because
//! `docs/protocol-client.md` already makes it the canonical client payload
//! format, so this introduces no second codec to the product. The profile is
//! pinned as:
//!
//! - **Definite lengths only.** No indefinite-length arrays, maps, or strings,
//!   and no chunked strings — each has more than one spelling for one value.
//! - **Sorted map keys.** Keys are ordered bytewise by their *encoded* form
//!   (§4.2.1), and duplicates are refused.
//! - **Shortest-form integers.** A value that fits in a smaller argument uses
//!   it, so `1` has one encoding rather than five.
//! - **Floats are always 8 bytes.** The "no float shrinking" variant permitted
//!   by §4.2.2. Preferred-width floats would make the encoding depend on
//!   whether a value happens to be representable in binary16 or binary32, which
//!   is a needless second thing to get exactly right in every future
//!   implementation. Non-finite values cannot arrive here at all — see
//!   [`crate::value::FiniteF64`].
//! - **No tags, and no simple values beyond `true` and `false`.** `null` is
//!   absent by construction: an optional field that is absent is *omitted*,
//!   never encoded as null, so there is exactly one encoding of "not present".
//!
//! # Bounds
//!
//! Nesting depth is checked while encoding, against
//! [`Limits::max_nesting_depth`]. The typed model is not recursive, so a valid
//! manifest cannot reach it today; it is here so that adding a recursive field
//! later cannot turn encoding into unbounded stack use without someone raising
//! the bound first.

use crate::artifact::{ArtifactDescriptor, ArtifactRole, SchemaCompat};
use crate::digest::{ArtifactDigest, WorkloadDigest};
use crate::domain::{Discretization, DomainBounds, DomainSpec, Integration};
use crate::graph::{
    ComponentInstance, ComputeSpec, PlacementConstraint, Reduction, StateChannel, StepInvocation,
    StepPlan,
};
use crate::limits::Limits;
use crate::manifest::{
    WorkloadInputs, WorkloadManifest, WorkloadMeta, WorkloadRequirements, WorkloadSpec,
};
use crate::value::{FiniteF64, ScalarValue};

/// Why a value could not be canonically encoded or decoded.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalError {
    /// A map was built with the same key twice.
    #[error("the canonical map key `{key}` appears more than once")]
    DuplicateKey {
        /// The repeated key.
        key: String,
    },
    /// The value nests deeper than the reader accepts.
    #[error("the value nests deeper than the {limit}-level limit")]
    TooDeep {
        /// The limit that was reached.
        limit: usize,
    },
    /// The document contains more values than the reader accepts.
    #[error("the document contains more than {limit} values")]
    TooManyValues {
        /// The limit that was reached.
        limit: usize,
    },
    /// The input ended part-way through a value.
    #[error("the document is truncated at byte {at}")]
    Truncated {
        /// Where the input ran out.
        at: usize,
    },
    /// Bytes remained after one complete value.
    #[error("{extra} bytes follow the document; a canonical form is exactly one value")]
    TrailingBytes {
        /// How many bytes were left over.
        extra: usize,
    },
    /// The document uses a CBOR construct the deterministic profile forbids.
    ///
    /// Refused rather than normalised: accepting a second spelling of a value
    /// would mean two byte strings decode to one workload, and the digest over
    /// them would disagree about which one it identifies.
    #[error("non-canonical encoding at byte {at}: {reason}")]
    NotCanonical {
        /// Where the offending construct starts.
        at: usize,
        /// What was wrong with it.
        reason: &'static str,
    },
    /// A value was not the shape the model expects at that position.
    #[error("at {path}: expected {expected}, found {found}")]
    UnexpectedShape {
        /// Where in the document, as a dotted path.
        path: String,
        /// What the model needed.
        expected: &'static str,
        /// What was there instead.
        found: &'static str,
    },
    /// A required field is absent.
    #[error("at {path}: the required field `{field}` is missing")]
    MissingField {
        /// Where in the document, as a dotted path.
        path: String,
        /// The absent field.
        field: &'static str,
    },
    /// A field the schema does not define is present.
    ///
    /// Refused for the same reason the authoring path refuses one: an
    /// unrecognised key in an identity-bearing document is either a mistake or
    /// an attempt to carry something the digest does not cover.
    #[error("at {path}: the field `{field}` is not part of this schema")]
    UnknownField {
        /// Where in the document, as a dotted path.
        path: String,
        /// The unrecognised field.
        field: String,
    },
    /// A value decoded structurally but is not valid for its domain type.
    #[error("at {path}: {reason}")]
    InvalidValue {
        /// Where in the document, as a dotted path.
        path: String,
        /// Why the value was refused.
        reason: String,
    },
}

/// A value in the canonical data model.
///
/// Deliberately smaller than CBOR's: there is no null, no tag, no indefinite
/// form, and no float width other than 64 bits. A construct this enum cannot
/// express is a construct a workload cannot commit to.
#[derive(Clone, Debug, PartialEq)]
pub enum CanonicalValue {
    /// A boolean.
    Bool(bool),
    /// A non-negative integer.
    UInt(u64),
    /// A negative integer.
    NInt(i64),
    /// A finite 64-bit float.
    F64(FiniteF64),
    /// A byte string.
    Bytes(Vec<u8>),
    /// A UTF-8 text string.
    Text(String),
    /// An ordered sequence. Order is the author's and is preserved.
    Array(Vec<CanonicalValue>),
    /// A map whose key order is imposed, not authored.
    Map(CanonicalMap),
}

impl CanonicalValue {
    /// A text value from anything string-like.
    pub fn text(value: impl Into<String>) -> Self {
        CanonicalValue::Text(value.into())
    }

    /// An integer value, choosing the signed or unsigned representation.
    #[must_use]
    pub fn int(value: i64) -> Self {
        if value < 0 {
            CanonicalValue::NInt(value)
        } else {
            CanonicalValue::UInt(value.unsigned_abs())
        }
    }
}

/// A map whose entries are sorted and unique *by construction*.
///
/// The sort is not applied at encoding time, because a value that has not been
/// sorted yet is a value that could be encoded by some other path and produce
/// different bytes. Building one is the only way to get one, so an unsorted or
/// duplicated canonical map does not exist.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalMap {
    // Sorted by encoded key, per RFC 8949 §4.2.1.
    entries: Vec<(CanonicalValue, CanonicalValue)>,
}

impl CanonicalMap {
    /// Builds a map from `entries`, sorting by encoded key.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalError::DuplicateKey`] when two entries encode to the
    /// same key bytes. Duplicates are refused rather than last-wins, because
    /// last-wins would let two different documents share an identity.
    pub fn new(entries: Vec<(CanonicalValue, CanonicalValue)>) -> Result<Self, CanonicalError> {
        let mut keyed: Vec<(Vec<u8>, (CanonicalValue, CanonicalValue))> = entries
            .into_iter()
            .map(|entry| {
                let mut key_bytes = Vec::new();
                // A key is a text string or an integer in this model, neither
                // of which nests, so encoding it cannot exceed a depth bound.
                write_value(&entry.0, &mut key_bytes, 0, usize::MAX)
                    .expect("a canonical map key never nests");
                (key_bytes, entry)
            })
            .collect();
        keyed.sort_by(|left, right| left.0.cmp(&right.0));

        if let Some(window) = keyed.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(CanonicalError::DuplicateKey {
                key: describe_key(&window[0].1.0),
            });
        }

        Ok(Self {
            entries: keyed.into_iter().map(|(_, entry)| entry).collect(),
        })
    }

    /// Builds a map from string keys, dropping entries whose value is absent.
    ///
    /// Omission is how this model spells "not present": there is no null, so an
    /// absent optional field simply has no entry, and exactly one encoding
    /// exists for it.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalError::DuplicateKey`] when two keys collide.
    pub fn fields(
        entries: impl IntoIterator<Item = (&'static str, Option<CanonicalValue>)>,
    ) -> Result<Self, CanonicalError> {
        Self::new(
            entries
                .into_iter()
                .filter_map(|(key, value)| value.map(|value| (CanonicalValue::text(key), value)))
                .collect(),
        )
    }

    /// The entries, in canonical order.
    #[must_use]
    pub fn entries(&self) -> &[(CanonicalValue, CanonicalValue)] {
        &self.entries
    }
}

fn describe_key(key: &CanonicalValue) -> String {
    match key {
        CanonicalValue::Text(text) => text.clone(),
        other => format!("{other:?}"),
    }
}

/// A domain type that can be committed to.
///
/// Implemented by hand for every type in the model. That is more code than a
/// derive, and it is the point: a field only participates in workload identity
/// because someone wrote it down here.
pub trait ToCanonical {
    /// The canonical form of this value.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalError`] when a map within the value has duplicate
    /// keys.
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError>;
}

// ── encoding ────────────────────────────────────────────────────────────────

/// CBOR major types, named so the writer reads as the specification does.
const MAJOR_UINT: u8 = 0;
const MAJOR_NINT: u8 = 1;
const MAJOR_BYTES: u8 = 2;
const MAJOR_TEXT: u8 = 3;
const MAJOR_ARRAY: u8 = 4;
const MAJOR_MAP: u8 = 5;
const MAJOR_SIMPLE: u8 = 7;

/// Writes a major type and its argument in the shortest form that holds it.
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

fn write_value(
    value: &CanonicalValue,
    out: &mut Vec<u8>,
    depth: usize,
    max_depth: usize,
) -> Result<(), CanonicalError> {
    if depth > max_depth {
        return Err(CanonicalError::TooDeep { limit: max_depth });
    }
    match value {
        CanonicalValue::Bool(false) => out.push((MAJOR_SIMPLE << 5) | 20),
        CanonicalValue::Bool(true) => out.push((MAJOR_SIMPLE << 5) | 21),
        CanonicalValue::UInt(value) => write_head(MAJOR_UINT, *value, out),
        CanonicalValue::NInt(value) => {
            // CBOR encodes a negative integer as -1 - argument, so the
            // argument for -1 is 0. `unsigned_abs() - 1` is exact for every
            // negative i64 including i64::MIN, where the naive `-1 - value`
            // would overflow.
            debug_assert!(*value < 0, "NInt holds only negative values");
            write_head(MAJOR_NINT, value.unsigned_abs() - 1, out);
        }
        CanonicalValue::F64(value) => {
            // Always 8 bytes: the "no float shrinking" deterministic variant.
            out.push((MAJOR_SIMPLE << 5) | 27);
            out.extend_from_slice(&value.get().to_be_bytes());
        }
        CanonicalValue::Bytes(bytes) => {
            write_head(MAJOR_BYTES, bytes.len() as u64, out);
            out.extend_from_slice(bytes);
        }
        CanonicalValue::Text(text) => {
            write_head(MAJOR_TEXT, text.len() as u64, out);
            out.extend_from_slice(text.as_bytes());
        }
        CanonicalValue::Array(items) => {
            write_head(MAJOR_ARRAY, items.len() as u64, out);
            for item in items {
                write_value(item, out, depth + 1, max_depth)?;
            }
        }
        CanonicalValue::Map(map) => {
            write_head(MAJOR_MAP, map.entries.len() as u64, out);
            for (key, value) in &map.entries {
                write_value(key, out, depth + 1, max_depth)?;
                write_value(value, out, depth + 1, max_depth)?;
            }
        }
    }
    Ok(())
}

/// Encodes a canonical value to bytes under the given bounds.
///
/// # Errors
///
/// Returns [`CanonicalError::TooDeep`] when the value nests past
/// [`Limits::max_nesting_depth`].
pub fn encode(value: &CanonicalValue, limits: &Limits) -> Result<Vec<u8>, CanonicalError> {
    let mut out = Vec::new();
    write_value(value, &mut out, 0, limits.max_nesting_depth)?;
    Ok(out)
}

/// The canonical bytes of a workload manifest.
///
/// # Errors
///
/// Returns [`CanonicalError`] when the manifest nests too deeply or contains a
/// map with duplicate keys.
pub fn canonical_bytes(
    manifest: &WorkloadManifest,
    limits: &Limits,
) -> Result<Vec<u8>, CanonicalError> {
    encode(&manifest.to_canonical()?, limits)
}

/// The identity of a workload: the digest of its canonical bytes.
///
/// # Errors
///
/// Returns [`CanonicalError`] when the manifest cannot be canonically encoded.
pub fn workload_digest(
    manifest: &WorkloadManifest,
    limits: &Limits,
) -> Result<WorkloadDigest, CanonicalError> {
    Ok(WorkloadDigest::of_canonical_bytes(&canonical_bytes(
        manifest, limits,
    )?))
}

// ── decoding ────────────────────────────────────────────────────────────────
//
// The reverse direction is not a convenience. Without it there is no way to
// show that the encoding is *injective* — that two different manifests cannot
// produce one byte string — and a field silently dropped on the way out would
// give two workloads one digest. A round trip is the check that the encoding
// commits to everything the model holds.
//
// It is also what makes canonical CBOR an exchange format rather than only an
// identity one: a receiver holding canonical bytes can recover the manifest
// without being handed the author's JSON.
//
// Everything here treats its input as hostile. Lengths are checked against the
// remaining input before any allocation, the value count is bounded, and every
// construct outside the deterministic profile is refused rather than
// normalised — accepting a second spelling would mean two byte strings decode
// to one workload while their digests disagree about which is which.

/// Reads bytes into canonical values, enforcing the profile as it goes.
struct Decoder<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Counts down so a flat array claiming a billion elements cannot make the
    /// reader allocate; depth alone would not catch that.
    budget: usize,
    max_depth: usize,
}

impl Decoder<'_> {
    fn byte(&mut self) -> Result<u8, CanonicalError> {
        let byte = *self
            .bytes
            .get(self.at)
            .ok_or(CanonicalError::Truncated { at: self.at })?;
        self.at += 1;
        Ok(byte)
    }

    /// Borrows `len` bytes, checking against the input's real length first.
    fn take(&mut self, len: usize) -> Result<&[u8], CanonicalError> {
        let end = self
            .at
            .checked_add(len)
            .ok_or(CanonicalError::Truncated { at: self.at })?;
        if end > self.bytes.len() {
            return Err(CanonicalError::Truncated { at: self.at });
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    /// Reads a head argument, refusing indefinite lengths and any argument not
    /// written in its shortest form.
    fn argument(&mut self, additional: u8, at: usize) -> Result<u64, CanonicalError> {
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
                return Err(CanonicalError::NotCanonical {
                    at,
                    reason: "an indefinite length, which has more than one spelling",
                });
            }
            _ => {
                return Err(CanonicalError::NotCanonical {
                    at,
                    reason: "a reserved additional-information value",
                });
            }
        };
        if value < minimum {
            return Err(CanonicalError::NotCanonical {
                at,
                reason: "an integer argument not written in its shortest form",
            });
        }
        Ok(value)
    }

    /// Reads a collection length, refusing one larger than the input could
    /// possibly hold.
    ///
    /// The cheapest defence against a header claiming billions of elements:
    /// every element costs at least one byte, so a length beyond the remaining
    /// input is a lie regardless of what follows.
    fn count(&mut self, additional: u8, at: usize, per_item: usize) -> Result<usize, CanonicalError> {
        let claimed = self.argument(additional, at)?;
        let remaining = (self.bytes.len() - self.at) as u64;
        if claimed.saturating_mul(per_item as u64) > remaining {
            return Err(CanonicalError::Truncated { at });
        }
        let claimed = claimed as usize;
        if claimed > self.budget {
            return Err(CanonicalError::TooManyValues {
                limit: self.budget,
            });
        }
        Ok(claimed)
    }

    fn spend(&mut self) -> Result<(), CanonicalError> {
        self.budget = self
            .budget
            .checked_sub(1)
            .ok_or(CanonicalError::TooManyValues { limit: 0 })?;
        Ok(())
    }

    fn value(&mut self, depth: usize) -> Result<CanonicalValue, CanonicalError> {
        if depth > self.max_depth {
            return Err(CanonicalError::TooDeep {
                limit: self.max_depth,
            });
        }
        self.spend()?;

        let at = self.at;
        let head = self.byte()?;
        let major = head >> 5;
        let additional = head & 0x1f;

        match major {
            MAJOR_UINT => Ok(CanonicalValue::UInt(self.argument(additional, at)?)),
            MAJOR_NINT => {
                let argument = self.argument(additional, at)?;
                // -1 - argument. An argument past `i64::MAX` would name a value
                // this model cannot hold, so it is refused rather than wrapped.
                let magnitude = argument.checked_add(1).filter(|value| *value <= 1 << 63).ok_or(
                    CanonicalError::NotCanonical {
                        at,
                        reason: "a negative integer outside the range this model carries",
                    },
                )?;
                Ok(CanonicalValue::NInt((magnitude as i128).wrapping_neg() as i64))
            }
            MAJOR_BYTES => {
                let len = self.count(additional, at, 1)?;
                Ok(CanonicalValue::Bytes(self.take(len)?.to_vec()))
            }
            MAJOR_TEXT => {
                let len = self.count(additional, at, 1)?;
                let raw = self.take(len)?;
                String::from_utf8(raw.to_vec())
                    .map(CanonicalValue::Text)
                    .map_err(|_| CanonicalError::NotCanonical {
                        at,
                        reason: "a text string that is not valid UTF-8",
                    })
            }
            MAJOR_ARRAY => {
                let len = self.count(additional, at, 1)?;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.value(depth + 1)?);
                }
                Ok(CanonicalValue::Array(items))
            }
            MAJOR_MAP => {
                // Two values per entry, so a map header claiming more entries
                // than half the remaining bytes is already impossible.
                let len = self.count(additional, at, 2)?;
                let mut entries = Vec::with_capacity(len);
                let mut previous: Option<Vec<u8>> = None;
                for _ in 0..len {
                    let key_at = self.at;
                    let key = self.value(depth + 1)?;
                    // Ordering is checked as read rather than after building,
                    // so an out-of-order document is refused before the rest of
                    // it is decoded.
                    let encoded = encode_key(&key);
                    if let Some(previous) = &previous {
                        match encoded.as_slice().cmp(previous.as_slice()) {
                            std::cmp::Ordering::Greater => {}
                            std::cmp::Ordering::Equal => {
                                return Err(CanonicalError::DuplicateKey {
                                    key: describe_key(&key),
                                });
                            }
                            std::cmp::Ordering::Less => {
                                return Err(CanonicalError::NotCanonical {
                                    at: key_at,
                                    reason: "map keys out of bytewise ascending order",
                                });
                            }
                        }
                    }
                    previous = Some(encoded);
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                }
                // Already sorted and unique, so this constructor cannot fail;
                // going through it anyway keeps one definition of what a
                // canonical map is.
                Ok(CanonicalValue::Map(CanonicalMap { entries }))
            }
            MAJOR_SIMPLE => match additional {
                20 => Ok(CanonicalValue::Bool(false)),
                21 => Ok(CanonicalValue::Bool(true)),
                22 => Err(CanonicalError::NotCanonical {
                    at,
                    reason: "null, where the profile omits an absent field instead",
                }),
                25 | 26 => Err(CanonicalError::NotCanonical {
                    at,
                    reason: "a float narrower than 64 bits",
                }),
                27 => {
                    let raw = self.take(8)?;
                    let mut wide = [0u8; 8];
                    wide.copy_from_slice(raw);
                    let value = f64::from_be_bytes(wide);
                    if value.is_sign_negative() && value == 0.0 {
                        return Err(CanonicalError::NotCanonical {
                            at,
                            reason: "negative zero, which must be normalised to positive zero",
                        });
                    }
                    FiniteF64::new(value)
                        .map(CanonicalValue::F64)
                        .map_err(|_| CanonicalError::NotCanonical {
                            at,
                            reason: "a non-finite float",
                        })
                }
                _ => Err(CanonicalError::NotCanonical {
                    at,
                    reason: "a simple value other than true or false",
                }),
            },
            _ => Err(CanonicalError::NotCanonical {
                at,
                reason: "a tag, which the profile never emits",
            }),
        }
    }
}

/// Encodes a map key for the ordering comparison.
fn encode_key(key: &CanonicalValue) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(key, &mut out, 0, usize::MAX).expect("a canonical map key never nests");
    out
}

/// Decodes one canonical value from `bytes`.
///
/// Every construct outside the deterministic profile is an error: indefinite
/// lengths, non-shortest integer arguments, tags, narrowed or non-finite
/// floats, null, negative zero, and out-of-order or duplicated map keys. So is
/// anything after the first complete value.
///
/// # Errors
///
/// Returns [`CanonicalError`] for a truncated, over-large, over-deep, or
/// non-canonical document.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<CanonicalValue, CanonicalError> {
    let mut decoder = Decoder {
        bytes,
        at: 0,
        budget: limits.max_canonical_values,
        max_depth: limits.max_nesting_depth,
    };
    let value = decoder.value(0)?;
    if decoder.at != bytes.len() {
        return Err(CanonicalError::TrailingBytes {
            extra: bytes.len() - decoder.at,
        });
    }
    Ok(value)
}

/// Recovers a manifest from its canonical bytes.
///
/// The inverse of [`canonical_bytes`]: for any manifest this crate can encode,
/// decoding its bytes yields an equal manifest and therefore the same
/// [`workload_digest`]. That round trip is what makes the encoding safe to use
/// as identity — an encoder that dropped a field would give two workloads one
/// digest, and this is the check that would notice.
///
/// # Errors
///
/// Returns [`CanonicalError`] when the bytes are not canonical, or when they
/// decode structurally but do not describe a workload — a missing or
/// unrecognised field, a value of the wrong shape, or a name, digest, or
/// number the domain types refuse.
pub fn manifest_from_canonical_bytes(
    bytes: &[u8],
    limits: &Limits,
) -> Result<WorkloadManifest, CanonicalError> {
    WorkloadManifest::from_canonical(&decode(bytes, limits)?, &Path::root())
}

// ── reading a decoded value back into the model ─────────────────────────────

/// Where in a document a problem was found, for an error a person can act on.
///
/// Built as the reader descends. `$.spec.compute.components[1].artifact.digest`
/// is a great deal more useful than "invalid value", and a manifest is
/// something an author is trying to get right.
#[derive(Clone, Debug)]
pub(crate) struct Path(String);

impl Path {
    fn root() -> Self {
        Path("$".to_owned())
    }

    fn field(&self, name: &str) -> Self {
        Path(format!("{}.{name}", self.0))
    }

    fn index(&self, index: usize) -> Self {
        Path(format!("{}[{index}]", self.0))
    }

    fn key(&self, key: &str) -> Self {
        Path(format!("{}.{key}", self.0))
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A domain type that can be recovered from its canonical form.
///
/// The mirror of [`ToCanonical`], and written out by hand for the same reason:
/// each `impl` states exactly which fields exist, so a field can only be
/// dropped from a workload's identity by someone deleting it from both sides.
pub(crate) trait FromCanonical: Sized {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError>;
}

/// Names the shape of a value, for an error message.
fn shape_of(value: &CanonicalValue) -> &'static str {
    match value {
        CanonicalValue::Bool(_) => "a boolean",
        CanonicalValue::UInt(_) => "a non-negative integer",
        CanonicalValue::NInt(_) => "a negative integer",
        CanonicalValue::F64(_) => "a number",
        CanonicalValue::Bytes(_) => "a byte string",
        CanonicalValue::Text(_) => "a text string",
        CanonicalValue::Array(_) => "a list",
        CanonicalValue::Map(_) => "a map",
    }
}

fn wrong_shape(path: &Path, expected: &'static str, found: &CanonicalValue) -> CanonicalError {
    CanonicalError::UnexpectedShape {
        path: path.to_string(),
        expected,
        found: shape_of(found),
    }
}

/// Reads the fields of a canonical map, refusing any the schema does not name.
///
/// Unknown fields are refused for the same reason the authoring path refuses
/// them: in an identity-bearing document, a key nobody reads is either a
/// mistake or something being carried past the digest.
struct Fields<'v> {
    path: Path,
    entries: std::collections::BTreeMap<&'v str, &'v CanonicalValue>,
}

impl<'v> Fields<'v> {
    fn of(value: &'v CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let CanonicalValue::Map(map) = value else {
            return Err(wrong_shape(path, "a map", value));
        };
        let mut entries = std::collections::BTreeMap::new();
        for (key, value) in map.entries() {
            let CanonicalValue::Text(key) = key else {
                return Err(wrong_shape(path, "a text map key", key));
            };
            entries.insert(key.as_str(), value);
        }
        Ok(Self {
            path: path.clone(),
            entries,
        })
    }

    fn required<T: FromCanonical>(&mut self, field: &'static str) -> Result<T, CanonicalError> {
        let value = self
            .entries
            .remove(field)
            .ok_or_else(|| CanonicalError::MissingField {
                path: self.path.to_string(),
                field,
            })?;
        T::from_canonical(value, &self.path.field(field))
    }

    fn optional<T: FromCanonical>(
        &mut self,
        field: &'static str,
    ) -> Result<Option<T>, CanonicalError> {
        self.entries
            .remove(field)
            .map(|value| T::from_canonical(value, &self.path.field(field)))
            .transpose()
    }

    /// A list field. Absent and empty are the same, matching the encoder, which
    /// omits an empty collection rather than writing one.
    fn list<T: FromCanonical>(&mut self, field: &'static str) -> Result<Vec<T>, CanonicalError> {
        let Some(value) = self.entries.remove(field) else {
            return Ok(Vec::new());
        };
        let path = self.path.field(field);
        let CanonicalValue::Array(items) = value else {
            return Err(wrong_shape(&path, "a list", value));
        };
        items
            .iter()
            .enumerate()
            .map(|(index, item)| T::from_canonical(item, &path.index(index)))
            .collect()
    }

    /// A keyed map field, absent when empty.
    fn keyed<K, V>(
        &mut self,
        field: &'static str,
    ) -> Result<std::collections::BTreeMap<K, V>, CanonicalError>
    where
        K: Ord + std::str::FromStr,
        K::Err: std::fmt::Display,
        V: FromCanonical,
    {
        let Some(value) = self.entries.remove(field) else {
            return Ok(std::collections::BTreeMap::new());
        };
        let path = self.path.field(field);
        let CanonicalValue::Map(map) = value else {
            return Err(wrong_shape(&path, "a map", value));
        };
        let mut out = std::collections::BTreeMap::new();
        for (key, value) in map.entries() {
            let CanonicalValue::Text(key) = key else {
                return Err(wrong_shape(&path, "a text map key", key));
            };
            let entry = path.key(key);
            let key = key.parse::<K>().map_err(|error| CanonicalError::InvalidValue {
                path: entry.to_string(),
                reason: error.to_string(),
            })?;
            out.insert(key, V::from_canonical(value, &entry)?);
        }
        Ok(out)
    }

    /// Consumes the reader, refusing anything the schema did not claim.
    fn finish(self) -> Result<(), CanonicalError> {
        match self.entries.into_keys().next() {
            None => Ok(()),
            Some(unknown) => Err(CanonicalError::UnknownField {
                path: self.path.to_string(),
                field: unknown.to_owned(),
            }),
        }
    }
}

/// Reads a validated name, turning its constructor's refusal into a located
/// error.
fn parsed<T>(value: &CanonicalValue, path: &Path) -> Result<T, CanonicalError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let CanonicalValue::Text(text) = value else {
        return Err(wrong_shape(path, "a text string", value));
    };
    text.parse().map_err(|error: T::Err| CanonicalError::InvalidValue {
        path: path.to_string(),
        reason: error.to_string(),
    })
}

/// Declares `FromCanonical` for a type whose canonical form is its textual one.
macro_rules! from_canonical_text {
    ($($name:ty),+ $(,)?) => {
        $(impl FromCanonical for $name {
            fn from_canonical(
                value: &CanonicalValue,
                path: &Path,
            ) -> Result<Self, CanonicalError> {
                parsed(value, path)
            }
        })+
    };
}

from_canonical_text!(
    ArtifactDigest,
    ArtifactRole,
    crate::ids::ComponentInstanceId,
    crate::ids::ComponentRole,
    crate::ids::ConstraintName,
    crate::ids::Engine,
    crate::ids::GraphProfile,
    crate::ids::LifecycleId,
    crate::ids::MediaType,
    crate::ids::ModelId,
    crate::ids::ParameterName,
    crate::ids::PhaseId,
    crate::ids::PluginId,
    crate::ids::SchemaId,
    crate::ids::StateChannelId,
    crate::ids::StepInvocationId,
    crate::ids::WorkloadName,
    crate::ids::LabelValue,
    crate::manifest::ApiVersion,
    crate::manifest::Kind,
);

impl FromCanonical for u64 {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        match value {
            CanonicalValue::UInt(value) => Ok(*value),
            other => Err(wrong_shape(path, "a non-negative integer", other)),
        }
    }
}

impl FromCanonical for u32 {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        u64::from_canonical(value, path)?
            .try_into()
            .map_err(|_| CanonicalError::InvalidValue {
                path: path.to_string(),
                reason: "the value does not fit in 32 bits".to_owned(),
            })
    }
}

impl FromCanonical for u8 {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        u64::from_canonical(value, path)?
            .try_into()
            .map_err(|_| CanonicalError::InvalidValue {
                path: path.to_string(),
                reason: "the value does not fit in 8 bits".to_owned(),
            })
    }
}

impl FromCanonical for FiniteF64 {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        match value {
            // Only a float decodes as a real. An integer is not silently
            // widened: the encoder never writes one here, so accepting it
            // would let two byte strings mean one manifest.
            CanonicalValue::F64(value) => Ok(*value),
            other => Err(wrong_shape(path, "a number", other)),
        }
    }
}

impl FromCanonical for ScalarValue {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        match value {
            CanonicalValue::Bool(value) => Ok(ScalarValue::Bool(*value)),
            CanonicalValue::UInt(value) => i64::try_from(*value)
                .map(ScalarValue::Integer)
                .map_err(|_| CanonicalError::InvalidValue {
                    path: path.to_string(),
                    reason: "the integer does not fit in 64 signed bits".to_owned(),
                }),
            CanonicalValue::NInt(value) => Ok(ScalarValue::Integer(*value)),
            CanonicalValue::F64(value) => Ok(ScalarValue::Real(*value)),
            CanonicalValue::Text(value) => Ok(ScalarValue::Text(value.clone())),
            other => Err(wrong_shape(path, "a scalar", other)),
        }
    }
}

impl FromCanonical for SchemaCompat {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let compat = SchemaCompat {
            schema_id: fields.required("schemaId")?,
            version: fields.required("version")?,
        };
        fields.finish()?;
        Ok(compat)
    }
}

impl FromCanonical for ArtifactDescriptor {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let descriptor = ArtifactDescriptor {
            role: fields.required("role")?,
            digest: fields.required("digest")?,
            size_bytes: fields.required("sizeBytes")?,
            media_type: fields.required("mediaType")?,
            schema: fields.optional("schema")?,
        };
        fields.finish()?;
        Ok(descriptor)
    }
}

impl FromCanonical for ComponentInstance {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let instance = ComponentInstance {
            instance_id: fields.required("instanceId")?,
            artifact: fields.required("artifact")?,
            plugin_id: fields.required("pluginId")?,
            model_id: fields.required("modelId")?,
            schema_id: fields.required("schemaId")?,
            engine: fields.required("engine")?,
            lifecycle: fields.required("lifecycle")?,
            roles: fields.list("roles")?,
            state_ownership: fields.list("stateOwnership")?,
            config: fields.keyed("config")?,
            limits: fields.keyed("limits")?,
        };
        fields.finish()?;
        Ok(instance)
    }
}

impl FromCanonical for Reduction {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let CanonicalValue::Text(text) = value else {
            return Err(wrong_shape(path, "a text string", value));
        };
        match text.as_str() {
            "single" => Ok(Reduction::Single),
            "sum" => Ok(Reduction::Sum),
            "min" => Ok(Reduction::Min),
            "max" => Ok(Reduction::Max),
            other => Err(CanonicalError::InvalidValue {
                path: path.to_string(),
                reason: format!(
                    "`{other}` is not a reduction; expected single, sum, min, or max"
                ),
            }),
        }
    }
}

impl FromCanonical for StateChannel {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let channel = StateChannel {
            channel_id: fields.required("channelId")?,
            schema: fields.required("schema")?,
            shape: fields.list("shape")?,
            owner: fields.optional("owner")?,
            reduction: fields.required("reduction")?,
        };
        fields.finish()?;
        Ok(channel)
    }
}

impl FromCanonical for StepInvocation {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let invocation = StepInvocation {
            invocation_id: fields.required("invocationId")?,
            instance: fields.required("instance")?,
            phase_id: fields.required("phaseId")?,
            inputs: fields.list("inputs")?,
            outputs: fields.list("outputs")?,
            depends_on: fields.list("dependsOn")?,
        };
        fields.finish()?;
        Ok(invocation)
    }
}

impl FromCanonical for StepPlan {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let plan = StepPlan {
            profile: fields.required("profile")?,
            invocations: fields.list("invocations")?,
        };
        fields.finish()?;
        Ok(plan)
    }
}

impl FromCanonical for PlacementConstraint {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let constraint = PlacementConstraint {
            constraint: fields.required("constraint")?,
            instances: fields.list("instances")?,
            parameters: fields.keyed("parameters")?,
        };
        fields.finish()?;
        Ok(constraint)
    }
}

impl FromCanonical for ComputeSpec {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let compute = ComputeSpec {
            workload_graph_profile: fields.required("workloadGraphProfile")?,
            components: fields.list("components")?,
            channels: fields.list("channels")?,
            step_plan: fields.required("stepPlan")?,
            placement_constraints: fields.list("placementConstraints")?,
        };
        fields.finish()?;
        Ok(compute)
    }
}

impl FromCanonical for DomainBounds {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let shape: String = {
            let value = fields
                .entries
                .remove("shape")
                .ok_or_else(|| CanonicalError::MissingField {
                    path: path.to_string(),
                    field: "shape",
                })?;
            match value {
                CanonicalValue::Text(text) => text.clone(),
                other => return Err(wrong_shape(&path.field("shape"), "a text string", other)),
            }
        };
        let bounds = match shape.as_str() {
            "cube" => DomainBounds::Cube {
                side_metres: fields.required("sideMetres")?,
            },
            "box" => DomainBounds::Box {
                side_metres: fields.list("sideMetres")?,
            },
            other => {
                return Err(CanonicalError::InvalidValue {
                    path: path.field("shape").to_string(),
                    reason: format!("`{other}` is not a domain shape; expected cube or box"),
                });
            }
        };
        fields.finish()?;
        Ok(bounds)
    }
}

impl FromCanonical for Integration {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let integration = Integration {
            scheme: fields.required("scheme")?,
            parameters: fields.keyed("parameters")?,
        };
        fields.finish()?;
        Ok(integration)
    }
}

impl FromCanonical for Discretization {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let discretization = Discretization {
            space_metres: fields.required("spaceMetres")?,
            time_seconds: fields.required("timeSeconds")?,
            integration: fields.optional("integration")?,
        };
        fields.finish()?;
        Ok(discretization)
    }
}

impl FromCanonical for DomainSpec {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let domain = DomainSpec {
            dimensions: fields.required("dimensions")?,
            bounds: fields.required("bounds")?,
            discretization: fields.required("discretization")?,
        };
        fields.finish()?;
        Ok(domain)
    }
}

impl FromCanonical for WorkloadInputs {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let inputs = WorkloadInputs {
            geometry: fields.optional("geometry")?,
            initial_conditions: fields.list("initialConditions")?,
            additional: fields.list("additional")?,
        };
        fields.finish()?;
        Ok(inputs)
    }
}

impl FromCanonical for WorkloadRequirements {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let requirements = WorkloadRequirements {
            hardware: fields.keyed("hardware")?,
            execution_profile: fields.keyed("executionProfile")?,
        };
        fields.finish()?;
        Ok(requirements)
    }
}

impl FromCanonical for WorkloadSpec {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let spec = WorkloadSpec {
            compute: fields.required("compute")?,
            domain: fields.required("domain")?,
            inputs: fields.required("inputs")?,
            requirements: fields.required("requirements")?,
        };
        fields.finish()?;
        Ok(spec)
    }
}

impl FromCanonical for WorkloadMeta {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let meta = WorkloadMeta {
            name: fields.required("name")?,
            labels: fields.keyed("labels")?,
        };
        fields.finish()?;
        Ok(meta)
    }
}

impl FromCanonical for WorkloadManifest {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut fields = Fields::of(value, path)?;
        let manifest = WorkloadManifest::new(
            fields.required("apiVersion")?,
            fields.required("kind")?,
            fields.required("metadata")?,
            fields.required("spec")?,
        );
        // No `status` arm: the manifest's status type is uninhabited, so a
        // document carrying one is refused by `finish` as an unknown field,
        // which is the same answer the authoring path gives.
        fields.finish()?;
        Ok(manifest)
    }
}

// ── the model's canonical forms ─────────────────────────────────────────────
//
// Each `impl` below is the authoritative statement of which fields a workload
// commits to. A field absent from one of these does not participate in
// identity, whatever the Rust struct or the serde derive says.

/// Shorthand for a text-valued field that is always present.
fn some_text(value: impl Into<String>) -> Option<CanonicalValue> {
    Some(CanonicalValue::text(value))
}

/// Canonicalises a sequence, propagating the first failure.
fn array<T: ToCanonical>(items: &[T]) -> Result<CanonicalValue, CanonicalError> {
    items
        .iter()
        .map(ToCanonical::to_canonical)
        .collect::<Result<Vec<_>, _>>()
        .map(CanonicalValue::Array)
}

/// Canonicalises a sequence, omitting it entirely when empty.
///
/// An empty collection and an absent one encode identically, which matches the
/// authoring types: both spell "there are none", and giving them two encodings
/// would give one workload two identities.
fn array_or_omitted<T: ToCanonical>(items: &[T]) -> Result<Option<CanonicalValue>, CanonicalError> {
    if items.is_empty() {
        Ok(None)
    } else {
        array(items).map(Some)
    }
}

/// Canonicalises a keyed map, omitting it entirely when empty.
fn map_or_omitted<K: AsRef<str>, V: ToCanonical>(
    entries: &std::collections::BTreeMap<K, V>,
) -> Result<Option<CanonicalValue>, CanonicalError> {
    if entries.is_empty() {
        return Ok(None);
    }
    let pairs = entries
        .iter()
        .map(|(key, value)| Ok((CanonicalValue::text(key.as_ref()), value.to_canonical()?)))
        .collect::<Result<Vec<_>, CanonicalError>>()?;
    CanonicalMap::new(pairs).map(CanonicalValue::Map).map(Some)
}

impl ToCanonical for ScalarValue {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        Ok(match self {
            ScalarValue::Bool(value) => CanonicalValue::Bool(*value),
            ScalarValue::Integer(value) => CanonicalValue::int(*value),
            ScalarValue::Real(value) => CanonicalValue::F64(*value),
            ScalarValue::Text(value) => CanonicalValue::text(value.clone()),
        })
    }
}

impl ToCanonical for u64 {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        Ok(CanonicalValue::UInt(*self))
    }
}

impl ToCanonical for u32 {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        Ok(CanonicalValue::UInt(u64::from(*self)))
    }
}

impl ToCanonical for FiniteF64 {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        Ok(CanonicalValue::F64(*self))
    }
}

impl ToCanonical for ArtifactDigest {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        // The tagged textual form, not raw bytes: the algorithm tag is part of
        // what a digest means, and dropping it from the identity would make a
        // future SHA-512 workload collide with a SHA-256 one over the same
        // 32-byte prefix.
        Ok(CanonicalValue::text(self.to_string()))
    }
}

impl ToCanonical for ArtifactRole {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        Ok(CanonicalValue::text(self.as_str()))
    }
}

impl ToCanonical for SchemaCompat {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("schemaId", some_text(self.schema_id.as_str())),
            ("version", Some(CanonicalValue::UInt(self.version.into()))),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for ArtifactDescriptor {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("role", Some(self.role.to_canonical()?)),
            ("digest", Some(self.digest.to_canonical()?)),
            ("sizeBytes", Some(CanonicalValue::UInt(self.size_bytes))),
            ("mediaType", some_text(self.media_type.as_str())),
            (
                "schema",
                self.schema
                    .as_ref()
                    .map(ToCanonical::to_canonical)
                    .transpose()?,
            ),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for ComponentInstance {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("instanceId", some_text(self.instance_id.as_str())),
            ("artifact", Some(self.artifact.to_canonical()?)),
            ("pluginId", some_text(self.plugin_id.as_str())),
            ("modelId", some_text(self.model_id.as_str())),
            ("schemaId", some_text(self.schema_id.as_str())),
            ("engine", some_text(self.engine.as_str())),
            ("lifecycle", some_text(self.lifecycle.as_str())),
            ("roles", names_or_omitted(&self.roles)),
            ("stateOwnership", names_or_omitted(&self.state_ownership)),
            ("config", map_or_omitted(&self.config)?),
            ("limits", map_or_omitted(&self.limits)?),
        ])
        .map(CanonicalValue::Map)
    }
}

/// Canonicalises a list of validated names, omitting it when empty.
fn names_or_omitted<T: AsRef<str>>(names: &[T]) -> Option<CanonicalValue> {
    if names.is_empty() {
        None
    } else {
        Some(CanonicalValue::Array(
            names
                .iter()
                .map(|name| CanonicalValue::text(name.as_ref()))
                .collect(),
        ))
    }
}

impl ToCanonical for Reduction {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        Ok(CanonicalValue::text(match self {
            Reduction::Single => "single",
            Reduction::Sum => "sum",
            Reduction::Min => "min",
            Reduction::Max => "max",
        }))
    }
}

impl ToCanonical for StateChannel {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("channelId", some_text(self.channel_id.as_str())),
            ("schema", Some(self.schema.to_canonical()?)),
            ("shape", array_or_omitted(&self.shape)?),
            (
                "owner",
                self.owner
                    .as_ref()
                    .map(|id| CanonicalValue::text(id.as_str())),
            ),
            ("reduction", Some(self.reduction.to_canonical()?)),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for StepInvocation {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("invocationId", some_text(self.invocation_id.as_str())),
            ("instance", some_text(self.instance.as_str())),
            ("phaseId", some_text(self.phase_id.as_str())),
            ("inputs", names_or_omitted(&self.inputs)),
            ("outputs", names_or_omitted(&self.outputs)),
            ("dependsOn", names_or_omitted(&self.depends_on)),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for StepPlan {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("profile", some_text(self.profile.as_str())),
            ("invocations", Some(array(&self.invocations)?)),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for PlacementConstraint {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("constraint", some_text(self.constraint.as_str())),
            ("instances", names_or_omitted(&self.instances)),
            ("parameters", map_or_omitted(&self.parameters)?),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for ComputeSpec {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            (
                "workloadGraphProfile",
                some_text(self.workload_graph_profile.as_str()),
            ),
            ("components", Some(array(&self.components)?)),
            ("channels", array_or_omitted(&self.channels)?),
            ("stepPlan", Some(self.step_plan.to_canonical()?)),
            (
                "placementConstraints",
                array_or_omitted(&self.placement_constraints)?,
            ),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for DomainBounds {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        match self {
            DomainBounds::Cube { side_metres } => CanonicalMap::fields([
                ("shape", some_text("cube")),
                ("sideMetres", Some(CanonicalValue::F64(*side_metres))),
            ]),
            DomainBounds::Box { side_metres } => CanonicalMap::fields([
                ("shape", some_text("box")),
                (
                    "sideMetres",
                    Some(CanonicalValue::Array(
                        side_metres
                            .iter()
                            .copied()
                            .map(CanonicalValue::F64)
                            .collect(),
                    )),
                ),
            ]),
        }
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for Integration {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("scheme", some_text(self.scheme.as_str())),
            ("parameters", map_or_omitted(&self.parameters)?),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for Discretization {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("spaceMetres", Some(CanonicalValue::F64(self.space_metres))),
            ("timeSeconds", Some(CanonicalValue::F64(self.time_seconds))),
            (
                "integration",
                self.integration
                    .as_ref()
                    .map(ToCanonical::to_canonical)
                    .transpose()?,
            ),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for DomainSpec {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            (
                "dimensions",
                Some(CanonicalValue::UInt(u64::from(self.dimensions))),
            ),
            ("bounds", Some(self.bounds.to_canonical()?)),
            ("discretization", Some(self.discretization.to_canonical()?)),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for WorkloadInputs {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            (
                "geometry",
                self.geometry
                    .as_ref()
                    .map(ToCanonical::to_canonical)
                    .transpose()?,
            ),
            (
                "initialConditions",
                array_or_omitted(&self.initial_conditions)?,
            ),
            ("additional", array_or_omitted(&self.additional)?),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for WorkloadRequirements {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("hardware", map_or_omitted(&self.hardware)?),
            ("executionProfile", map_or_omitted(&self.execution_profile)?),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for WorkloadSpec {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("compute", Some(self.compute.to_canonical()?)),
            ("domain", Some(self.domain.to_canonical()?)),
            ("inputs", Some(self.inputs.to_canonical()?)),
            ("requirements", Some(self.requirements.to_canonical()?)),
        ])
        .map(CanonicalValue::Map)
    }
}

impl ToCanonical for WorkloadMeta {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        let labels = if self.labels.is_empty() {
            None
        } else {
            Some(CanonicalValue::Map(CanonicalMap::new(
                self.labels
                    .iter()
                    .map(|(key, value)| {
                        (
                            CanonicalValue::text(key.as_str()),
                            CanonicalValue::text(value.as_str()),
                        )
                    })
                    .collect(),
            )?))
        };
        CanonicalMap::fields([("name", some_text(self.name.as_str())), ("labels", labels)])
            .map(CanonicalValue::Map)
    }
}

impl ToCanonical for WorkloadManifest {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        // The discriminator is inside the identity on purpose: a manifest read
        // under a different schema version is a different workload, even if
        // every other byte matches.
        CanonicalMap::fields([
            ("apiVersion", some_text(self.api_version().as_str())),
            ("kind", some_text(self.kind().as_str())),
            ("metadata", Some(self.metadata.to_canonical()?)),
            ("spec", Some(self.spec.to_canonical()?)),
        ])
        .map(CanonicalValue::Map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(value: &CanonicalValue) -> Vec<u8> {
        encode(value, &Limits::DEFAULT).expect("it encodes")
    }

    fn hex(value: &CanonicalValue) -> String {
        bytes(value).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn integers_use_the_shortest_form_that_holds_them() {
        // RFC 8949 §4.2.1. Pinned against hand-computed bytes so a regression
        // in `write_head` shows up here rather than as a fixture diff.
        assert_eq!(hex(&CanonicalValue::UInt(0)), "00");
        assert_eq!(hex(&CanonicalValue::UInt(23)), "17");
        assert_eq!(hex(&CanonicalValue::UInt(24)), "1818");
        assert_eq!(hex(&CanonicalValue::UInt(255)), "18ff");
        assert_eq!(hex(&CanonicalValue::UInt(256)), "190100");
        assert_eq!(hex(&CanonicalValue::UInt(65_535)), "19ffff");
        assert_eq!(hex(&CanonicalValue::UInt(65_536)), "1a00010000");
        assert_eq!(
            hex(&CanonicalValue::UInt(4_294_967_296)),
            "1b0000000100000000"
        );
    }

    #[test]
    fn negative_integers_encode_as_minus_one_minus_the_argument() {
        assert_eq!(hex(&CanonicalValue::int(-1)), "20");
        assert_eq!(hex(&CanonicalValue::int(-24)), "37");
        assert_eq!(hex(&CanonicalValue::int(-25)), "3818");
        assert_eq!(hex(&CanonicalValue::int(-256)), "38ff");
    }

    #[test]
    fn the_most_negative_integer_does_not_overflow() {
        // The naive `-1 - value` panics here in debug and wraps in release.
        assert_eq!(hex(&CanonicalValue::int(i64::MIN)), "3b7fffffffffffffff");
    }

    #[test]
    fn zero_is_unsigned_rather_than_negative() {
        assert_eq!(hex(&CanonicalValue::int(0)), "00");
    }

    #[test]
    fn floats_are_always_eight_bytes() {
        // 1.0 is exactly representable in binary16; a preferred-width encoder
        // would emit `f93c00`. This profile does not shrink.
        let one = CanonicalValue::F64(FiniteF64::new(1.0).expect("finite"));
        assert_eq!(hex(&one), "fb3ff0000000000000");
        let zero = CanonicalValue::F64(FiniteF64::ZERO);
        assert_eq!(hex(&zero), "fb0000000000000000");
    }

    #[test]
    fn negative_zero_encodes_as_positive_zero() {
        let negative = CanonicalValue::F64(FiniteF64::new(-0.0).expect("finite"));
        assert_eq!(hex(&negative), "fb0000000000000000");
    }

    #[test]
    fn map_keys_are_sorted_bytewise_on_their_encoded_form() {
        // Per §4.2.1 the comparison is over encoded keys, so the length head
        // participates: "a" is 61 61, "z" is 61 7a, and "aa" is 62 61 61. The
        // one-byte keys therefore both sort before the two-byte one, which is
        // *not* what plain lexicographic ordering of the strings would give.
        let map = CanonicalMap::new(vec![
            (CanonicalValue::text("z"), CanonicalValue::UInt(1)),
            (CanonicalValue::text("aa"), CanonicalValue::UInt(2)),
            (CanonicalValue::text("a"), CanonicalValue::UInt(3)),
        ])
        .expect("distinct keys");
        let keys: Vec<&str> = map
            .entries()
            .iter()
            .map(|(key, _)| match key {
                CanonicalValue::Text(text) => text.as_str(),
                other => panic!("unexpected key {other:?}"),
            })
            .collect();
        assert_eq!(keys, ["a", "z", "aa"]);
    }

    #[test]
    fn authored_map_order_does_not_reach_the_encoding() {
        let one = CanonicalMap::new(vec![
            (CanonicalValue::text("beta"), CanonicalValue::UInt(2)),
            (CanonicalValue::text("alpha"), CanonicalValue::UInt(1)),
        ])
        .expect("distinct keys");
        let other = CanonicalMap::new(vec![
            (CanonicalValue::text("alpha"), CanonicalValue::UInt(1)),
            (CanonicalValue::text("beta"), CanonicalValue::UInt(2)),
        ])
        .expect("distinct keys");
        assert_eq!(
            bytes(&CanonicalValue::Map(one)),
            bytes(&CanonicalValue::Map(other))
        );
    }

    #[test]
    fn a_duplicate_map_key_is_refused_rather_than_resolved() {
        // Last-wins would let two different documents share one identity.
        let error = CanonicalMap::new(vec![
            (CanonicalValue::text("dt"), CanonicalValue::UInt(1)),
            (CanonicalValue::text("dt"), CanonicalValue::UInt(2)),
        ])
        .unwrap_err();
        assert_eq!(
            error,
            CanonicalError::DuplicateKey {
                key: "dt".to_owned()
            }
        );
    }

    #[test]
    fn an_absent_field_is_omitted_rather_than_encoded_as_null() {
        let present = CanonicalMap::fields([("a", Some(CanonicalValue::UInt(1))), ("b", None)])
            .expect("distinct keys");
        let only_a =
            CanonicalMap::fields([("a", Some(CanonicalValue::UInt(1)))]).expect("distinct keys");
        assert_eq!(
            bytes(&CanonicalValue::Map(present)),
            bytes(&CanonicalValue::Map(only_a))
        );
    }

    #[test]
    fn array_order_is_preserved_because_it_is_the_authors() {
        let ascending =
            CanonicalValue::Array(vec![CanonicalValue::UInt(1), CanonicalValue::UInt(2)]);
        let descending =
            CanonicalValue::Array(vec![CanonicalValue::UInt(2), CanonicalValue::UInt(1)]);
        assert_ne!(bytes(&ascending), bytes(&descending));
    }

    #[test]
    fn text_and_bytes_carry_a_definite_length_head() {
        assert_eq!(hex(&CanonicalValue::text("a")), "6161");
        assert_eq!(hex(&CanonicalValue::Bytes(vec![0xde, 0xad])), "42dead");
        assert_eq!(hex(&CanonicalValue::text("")), "60");
    }

    #[test]
    fn booleans_are_the_only_simple_values() {
        assert_eq!(hex(&CanonicalValue::Bool(false)), "f4");
        assert_eq!(hex(&CanonicalValue::Bool(true)), "f5");
    }

    #[test]
    fn nesting_past_the_limit_is_refused() {
        let mut value = CanonicalValue::UInt(0);
        for _ in 0..8 {
            value = CanonicalValue::Array(vec![value]);
        }
        let tight = Limits {
            max_nesting_depth: 4,
            ..Limits::DEFAULT
        };
        assert_eq!(
            encode(&value, &tight),
            Err(CanonicalError::TooDeep { limit: 4 })
        );
        assert!(encode(&value, &Limits::DEFAULT).is_ok());
    }
}
