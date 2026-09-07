//! Content digests: what names a blob, and what names a workload.
//!
//! Two distinct types, deliberately not one. An [`ArtifactDigest`] is taken
//! over the bytes of a blob; a [`WorkloadDigest`] is taken over the canonical
//! encoding of a root manifest. They are the same 32 bytes of SHA-256 and they
//! mean entirely different things, so there is no conversion in either
//! direction and a function that wants a root identity cannot be handed a blob
//! identity by accident.
//!
//! The algorithm is tagged in the value and in its textual form. SHA-256 is the
//! only one today; carrying the tag now means a second algorithm is an added
//! variant rather than a wire break, and it means a digest read from a document
//! says what it is instead of relying on the reader to assume.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// How a digest was computed.
///
/// Exhaustive today. A new variant is a compatible addition; changing what an
/// existing variant means is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DigestAlgorithm {
    /// SHA-256, the algorithm ADR 0010 selects for the first version.
    Sha256,
}

impl DigestAlgorithm {
    /// The tag as it appears before the colon in the textual form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            DigestAlgorithm::Sha256 => "sha256",
        }
    }

    /// The number of digest bytes this algorithm produces.
    #[must_use]
    pub const fn byte_len(self) -> usize {
        match self {
            DigestAlgorithm::Sha256 => 32,
        }
    }
}

impl fmt::Display for DigestAlgorithm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why a textual digest could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    /// The value has no `algorithm:value` separator.
    #[error("a digest must be spelled `<algorithm>:<hex>`, got `{found}`")]
    Malformed {
        /// The value as supplied.
        found: String,
    },
    /// The algorithm tag names something this build cannot compute.
    #[error("unsupported digest algorithm `{algorithm}`; this build supports `sha256`")]
    UnsupportedAlgorithm {
        /// The tag as supplied.
        algorithm: String,
    },
    /// The hex payload is the wrong length for its algorithm.
    #[error("`{algorithm}` digests are {expected} hex characters, got {found}")]
    WrongLength {
        /// The algorithm that was named.
        algorithm: DigestAlgorithm,
        /// Hex characters expected.
        expected: usize,
        /// Hex characters supplied.
        found: usize,
    },
    /// The payload is not lowercase hexadecimal.
    #[error("a digest payload must be lowercase hexadecimal; byte {offset} is not")]
    NotHex {
        /// Byte offset of the first offending character within the payload.
        offset: usize,
    },
}

/// The identity of one content-addressed blob.
///
/// Fields are private: the only ways to obtain one are to hash bytes or to
/// parse a well-formed textual digest, so a value of this type is always a
/// complete, correctly sized digest with a known algorithm.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactDigest {
    algorithm: DigestAlgorithm,
    // Sized for the longest supported algorithm. `algorithm.byte_len()` says
    // how much of it is meaningful.
    bytes: [u8; 32],
}

impl ArtifactDigest {
    /// Computes the SHA-256 digest of `content`.
    #[must_use]
    pub fn sha256_of(content: &[u8]) -> Self {
        Self {
            algorithm: DigestAlgorithm::Sha256,
            bytes: Sha256::digest(content).into(),
        }
    }

    /// The algorithm this digest was computed with.
    #[must_use]
    pub const fn algorithm(&self) -> DigestAlgorithm {
        self.algorithm
    }

    /// The digest bytes, exactly as long as the algorithm produces.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.algorithm.byte_len()]
    }

    /// Whether `content` hashes to this digest.
    ///
    /// This is the only sanctioned way to decide that some bytes are the
    /// artifact a descriptor names. Nothing about where the bytes came from
    /// participates.
    #[must_use]
    pub fn matches(&self, content: &[u8]) -> bool {
        match self.algorithm {
            DigestAlgorithm::Sha256 => Self::sha256_of(content) == *self,
        }
    }
}

impl fmt::Display for ArtifactDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.algorithm.as_str())?;
        formatter.write_str(":")?;
        for byte in self.as_bytes() {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ArtifactDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ArtifactDigest({self})")
    }
}

impl FromStr for ArtifactDigest {
    type Err = DigestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((tag, payload)) = value.split_once(':') else {
            return Err(DigestError::Malformed {
                found: value.to_owned(),
            });
        };
        let algorithm = match tag {
            "sha256" => DigestAlgorithm::Sha256,
            other => {
                return Err(DigestError::UnsupportedAlgorithm {
                    algorithm: other.to_owned(),
                });
            }
        };
        let expected = algorithm.byte_len() * 2;
        if payload.len() != expected {
            return Err(DigestError::WrongLength {
                algorithm,
                expected,
                found: payload.len(),
            });
        }

        let mut bytes = [0u8; 32];
        for (index, pair) in payload.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_value(pair[0]).ok_or(DigestError::NotHex { offset: index * 2 })?;
            let low = hex_value(pair[1]).ok_or(DigestError::NotHex {
                offset: index * 2 + 1,
            })?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self { algorithm, bytes })
    }
}

/// Lowercase hex only: accepting uppercase would let one digest have two
/// spellings, and the spelling is what a canonical encoding commits to.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl Serialize for ArtifactDigest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ArtifactDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <&str>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The identity of one logical workload.
///
/// Computed over the canonical encoding of the root manifest. Because every
/// dependency in that manifest is named by an [`ArtifactDigest`], this one
/// value commits to the entire closure.
///
/// A separate type from [`ArtifactDigest`] on purpose: a root identity and a
/// blob identity are never interchangeable, and there is no conversion that
/// would let one be mistaken for the other.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkloadDigest(ArtifactDigest);

impl WorkloadDigest {
    /// Computes the digest of already-canonical manifest bytes.
    ///
    /// Private to the crate because the input must be the output of
    /// [`crate::canonical::canonical_bytes`]. Hashing anything else — authored
    /// YAML, a re-serialized JSON document — would make whitespace and key
    /// order part of workload identity, which is exactly what the canonical
    /// codec exists to prevent.
    pub(crate) fn of_canonical_bytes(canonical: &[u8]) -> Self {
        Self(ArtifactDigest::sha256_of(canonical))
    }

    /// The algorithm this digest was computed with.
    #[must_use]
    pub const fn algorithm(&self) -> DigestAlgorithm {
        self.0.algorithm()
    }

    /// The digest bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Display for WorkloadDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for WorkloadDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "WorkloadDigest({self})")
    }
}

impl FromStr for WorkloadDigest {
    type Err = DigestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl Serialize for WorkloadDigest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WorkloadDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ArtifactDigest::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_round_trips_through_its_textual_form() {
        let digest = ArtifactDigest::sha256_of(b"initial conditions");
        let text = digest.to_string();
        assert!(text.starts_with("sha256:"));
        assert_eq!(text.parse::<ArtifactDigest>().expect("it re-reads"), digest);
    }

    #[test]
    fn the_textual_form_is_the_tagged_lowercase_hex_of_the_content() {
        // Pinned against an independently known SHA-256 so a change to the
        // hashing or hex path is visible here rather than only as a fixture
        // diff.
        assert_eq!(
            ArtifactDigest::sha256_of(b"").to_string(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_digest_verifies_only_the_content_it_was_taken_over() {
        let digest = ArtifactDigest::sha256_of(b"field state");
        assert!(digest.matches(b"field state"));
        assert!(!digest.matches(b"field statf"));
        assert!(!digest.matches(b""));
    }

    #[test]
    fn an_untagged_value_is_refused() {
        let error = "e3b0c442".parse::<ArtifactDigest>().unwrap_err();
        assert!(matches!(error, DigestError::Malformed { .. }));
    }

    #[test]
    fn an_unknown_algorithm_is_refused_rather_than_assumed_to_be_sha256() {
        let error = "sha512:abcd".parse::<ArtifactDigest>().unwrap_err();
        assert_eq!(
            error,
            DigestError::UnsupportedAlgorithm {
                algorithm: "sha512".to_owned()
            }
        );
    }

    #[test]
    fn a_truncated_payload_is_refused() {
        let error = "sha256:abcd".parse::<ArtifactDigest>().unwrap_err();
        assert!(matches!(
            error,
            DigestError::WrongLength {
                expected: 64,
                found: 4,
                ..
            }
        ));
    }

    #[test]
    fn uppercase_hex_is_refused_so_one_digest_has_one_spelling() {
        let upper = format!("sha256:{}", "AB".repeat(32));
        assert!(matches!(
            upper.parse::<ArtifactDigest>().unwrap_err(),
            DigestError::NotHex { offset: 0 }
        ));
    }

    #[test]
    fn a_workload_digest_and_an_artifact_digest_do_not_convert() {
        // Not an assertion about values: the point is that the only way to
        // build a `WorkloadDigest` from outside this module is to parse one,
        // so a blob digest cannot silently become a root identity. If a
        // `From<ArtifactDigest>` is ever added, this comment is the thing that
        // was traded away.
        let root = WorkloadDigest::of_canonical_bytes(b"canonical");
        let blob = ArtifactDigest::sha256_of(b"canonical");
        assert_eq!(root.as_bytes(), blob.as_bytes());
        assert_eq!(root.to_string(), blob.to_string());
    }

    #[test]
    fn digests_serialize_as_their_textual_form() {
        let digest = ArtifactDigest::sha256_of(b"mesh");
        let json = serde_json::to_string(&digest).expect("it encodes");
        assert_eq!(json, format!("\"{digest}\""));
        assert_eq!(
            serde_json::from_str::<ArtifactDigest>(&json).expect("it decodes"),
            digest
        );
    }
}
