//! SHA-256 certificate fingerprints.

use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::IdentityError;

/// Length of a SHA-256 digest in bytes.
const FINGERPRINT_LEN: usize = 32;

/// SHA-256 fingerprint of a peer's mTLS certificate.
///
/// Pinned on admission and re-verified on every reconnection, so it is part of
/// a member's identity rather than a mutable attribute. A fixed-size array
/// makes "wrong length" unrepresentable after construction, which matters
/// because the fingerprint is compared against hostile input on the admission
/// path.
///
/// # Wire representation
///
/// Serializes as a byte string in binary formats (CBOR major type 2, as
/// `docs/protocol-p2p.md` specifies) and as lowercase hex in human-readable
/// formats such as JSON. Deserialization accepts a byte string, a sequence of
/// integers, or a hex string, and rejects anything that is not exactly 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CertFingerprint([u8; FINGERPRINT_LEN]);

impl CertFingerprint {
    /// Wraps an already-sized digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; FINGERPRINT_LEN]) -> Self {
        Self(bytes)
    }

    /// Parses a digest of unknown length, as decoded from the wire.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError::FingerprintLength`] unless `bytes` is exactly
    /// 32 bytes long.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, IdentityError> {
        <[u8; FINGERPRINT_LEN]>::try_from(bytes)
            .map(Self)
            .map_err(|_| IdentityError::FingerprintLength { len: bytes.len() })
    }

    /// Borrows the raw digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; FINGERPRINT_LEN] {
        &self.0
    }

    /// Renders the digest as lowercase hex.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write as _;
        self.0.iter().fold(
            String::with_capacity(FINGERPRINT_LEN * 2),
            |mut out, byte| {
                // Writing into a String is infallible.
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
    }
}

impl std::fmt::Display for CertFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for CertFingerprint {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != FINGERPRINT_LEN * 2 {
            return Err(IdentityError::FingerprintLength {
                len: value.len().div_ceil(2),
            });
        }
        let mut bytes = [0u8; FINGERPRINT_LEN];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let pair = value
                .get(index * 2..index * 2 + 2)
                .ok_or(IdentityError::FingerprintEncoding)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| IdentityError::FingerprintEncoding)?;
        }
        Ok(Self(bytes))
    }
}

impl Serialize for CertFingerprint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_hex())
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for CertFingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FingerprintVisitor;

        impl<'de> de::Visitor<'de> for FingerprintVisitor {
            type Value = CertFingerprint;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a 32-byte SHA-256 certificate fingerprint")
            }

            fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                CertFingerprint::from_slice(value).map_err(E::custom)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut bytes = Vec::with_capacity(FINGERPRINT_LEN);
                // Bounded by construction: a longer sequence is rejected by
                // `from_slice` rather than allowed to grow the buffer.
                while let Some(byte) = seq.next_element::<u8>()? {
                    if bytes.len() == FINGERPRINT_LEN {
                        return Err(de::Error::custom(IdentityError::FingerprintLength {
                            len: FINGERPRINT_LEN + 1,
                        }));
                    }
                    bytes.push(byte);
                }
                CertFingerprint::from_slice(&bytes).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(FingerprintVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CertFingerprint {
        CertFingerprint::from_bytes([0xA1; FINGERPRINT_LEN])
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            CertFingerprint::from_slice(&[0u8; 16]),
            Err(IdentityError::FingerprintLength { len: 16 })
        );
    }

    #[test]
    fn hex_round_trips() {
        let fingerprint = sample();
        assert_eq!(fingerprint.to_hex(), "a1".repeat(FINGERPRINT_LEN));
        assert_eq!(
            fingerprint.to_hex().parse::<CertFingerprint>().unwrap(),
            fingerprint
        );
    }

    #[test]
    fn rejects_non_hex_text() {
        let text = "z".repeat(FINGERPRINT_LEN * 2);
        assert_eq!(
            text.parse::<CertFingerprint>(),
            Err(IdentityError::FingerprintEncoding)
        );
    }

    #[test]
    fn json_uses_hex_and_validates_on_read() {
        let json = serde_json::to_string(&sample()).unwrap();
        assert_eq!(json, format!("\"{}\"", "a1".repeat(FINGERPRINT_LEN)));
        assert_eq!(
            serde_json::from_str::<CertFingerprint>(&json).unwrap(),
            sample()
        );
        assert!(serde_json::from_str::<CertFingerprint>("\"a1a1\"").is_err());
    }

    #[test]
    fn json_also_accepts_a_byte_array() {
        let json = format!("{:?}", [0xA1u8; FINGERPRINT_LEN]);
        assert_eq!(
            serde_json::from_str::<CertFingerprint>(&json).unwrap(),
            sample()
        );
        assert!(serde_json::from_str::<CertFingerprint>("[1,2,3]").is_err());
    }
}
