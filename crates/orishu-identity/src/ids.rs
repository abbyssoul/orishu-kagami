//! Opaque identities and human labels.
//!
//! The two families are deliberately separate Rust types with no conversion
//! between them. ADR 0013 requires that "labels never satisfy identity
//! checks"; the cheapest way to guarantee that is to make the mistake fail to
//! compile.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::IdentityError;

/// Maximum length of an opaque identity, in bytes.
///
/// Comfortably fits a UUID, a base64url-encoded 256-bit value, or a prefixed
/// variant of either, while keeping identities small enough to embed in
/// datagram-sized peer messages.
pub const MAX_IDENTITY_LEN: usize = 128;

/// Maximum length of an operator-supplied label, in bytes.
///
/// Matches the DNS subdomain limit used by the manifest `metadata.name`
/// convention.
pub const MAX_LABEL_LEN: usize = 253;

/// Validates an opaque identity: non-empty, bounded, printable non-whitespace
/// ASCII.
fn validate_identity(kind: &'static str, value: &str) -> Result<(), IdentityError> {
    if value.is_empty() {
        return Err(IdentityError::Empty { kind });
    }
    if value.len() > MAX_IDENTITY_LEN {
        return Err(IdentityError::TooLong {
            kind,
            len: value.len(),
            max: MAX_IDENTITY_LEN,
        });
    }
    match value.bytes().position(|b| !b.is_ascii_graphic()) {
        Some(offset) => Err(IdentityError::ForbiddenCharacter { kind, offset }),
        None => Ok(()),
    }
}

/// Validates an operator label: non-empty, bounded, no control characters, and
/// no surrounding whitespace (which would make two labels that look identical
/// compare unequal).
fn validate_label(kind: &'static str, value: &str) -> Result<(), IdentityError> {
    if value.is_empty() {
        return Err(IdentityError::Empty { kind });
    }
    if value.len() > MAX_LABEL_LEN {
        return Err(IdentityError::TooLong {
            kind,
            len: value.len(),
            max: MAX_LABEL_LEN,
        });
    }
    if let Some((offset, _)) = value.char_indices().find(|(_, c)| c.is_control()) {
        return Err(IdentityError::ForbiddenCharacter { kind, offset });
    }
    if value.trim() != value {
        return Err(IdentityError::UntrimmedLabel { kind });
    }
    Ok(())
}

/// Declares a validated string new-type with `FromStr`, `Display`, and
/// validating serde support.
///
/// The inner `String` stays private: the only way to obtain a value is through
/// the validating constructor, so downstream code never has to re-check.
macro_rules! validated_string {
    (
        $(#[$meta:meta])*
        $name:ident, $validator:path
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parses `value`, rejecting anything that is not a well-formed
            #[doc = concat!("`", stringify!($name), "`.")]
            ///
            /// # Errors
            ///
            /// Returns [`IdentityError`] when the value is empty, exceeds the
            /// maximum length, or contains a forbidden character.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                $validator(stringify!($name), &value)?;
                Ok(Self(value))
            }

            /// Borrows the validated value as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdentityError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentityError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

validated_string!(
    /// Immutable identity of one cluster formation.
    ///
    /// A formation is one ephemeral instantiation of a cluster. Its ID is
    /// generated when the formation is created and is never reused: a new
    /// formation inherits no membership, locks, tombstones, or versions from
    /// an earlier one. Every post-admission peer message carries this ID as a
    /// strict isolation boundary, and durable artifacts record it as origin
    /// provenance independently of the reusable [`ClusterName`].
    FormationId,
    validate_identity
);

validated_string!(
    /// Formation-assigned identity of one cluster member.
    ///
    /// Generated by the admitting formation, opaque, and unique within that
    /// formation only. Leaving a formation drops the ID; admission to another
    /// formation assigns a fresh one. Never derive a `NodeId` from a
    /// [`WorkerName`] — several workers may legitimately share a name.
    NodeId,
    validate_identity
);

validated_string!(
    /// Non-unique operator label for a cluster formation.
    ///
    /// Useful for display and for admission/isolation intent during a
    /// handshake. It is not a security boundary and not identity: two
    /// unrelated formations may carry the same name.
    ClusterName,
    validate_label
);

validated_string!(
    /// Non-unique operator label for a worker process.
    ///
    /// Workers started from identical configuration share a name, so a name
    /// identifies a *join applicant* at most, never a member. Before admission
    /// the join flow uses this label together with the certificate
    /// fingerprint; after admission, use the assigned [`NodeId`].
    WorkerName,
    validate_label
);

/// Peer-protocol version carried in every `MessageEnvelope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(pub u32);

impl ProtocolVersion {
    /// The version described by `docs/protocol-p2p.md`.
    pub const CURRENT: Self = Self(1);
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Inclusive range of peer-protocol versions a node will interoperate with.
///
/// Kept as a value type rather than two loose integers so that an admission
/// gate cannot accidentally compare against only one bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolRange {
    min: ProtocolVersion,
    max: ProtocolVersion,
}

impl ProtocolRange {
    /// Builds a range, requiring `min <= max`.
    ///
    /// # Errors
    ///
    /// Returns the offending pair when `min` is greater than `max`.
    pub fn new(
        min: ProtocolVersion,
        max: ProtocolVersion,
    ) -> Result<Self, (ProtocolVersion, ProtocolVersion)> {
        if min > max {
            Err((min, max))
        } else {
            Ok(Self { min, max })
        }
    }

    /// A range accepting exactly one version.
    #[must_use]
    pub const fn exact(version: ProtocolVersion) -> Self {
        Self {
            min: version,
            max: version,
        }
    }

    /// Lowest accepted version.
    #[must_use]
    pub const fn min(&self) -> ProtocolVersion {
        self.min
    }

    /// Highest accepted version.
    #[must_use]
    pub const fn max(&self) -> ProtocolVersion {
        self.max
    }

    /// Whether `version` is interoperable with this node.
    #[must_use]
    pub fn accepts(&self, version: ProtocolVersion) -> bool {
        self.min <= version && version <= self.max
    }
}

impl Default for ProtocolRange {
    fn default() -> Self {
        Self::exact(ProtocolVersion::CURRENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rejects_empty() {
        assert_eq!(
            NodeId::new(""),
            Err(IdentityError::Empty { kind: "NodeId" })
        );
    }

    #[test]
    fn identity_rejects_whitespace_and_control_characters() {
        assert!(matches!(
            NodeId::new("node 1"),
            Err(IdentityError::ForbiddenCharacter { offset: 4, .. })
        ));
        assert!(matches!(
            FormationId::new("form\nation"),
            Err(IdentityError::ForbiddenCharacter { offset: 4, .. })
        ));
    }

    #[test]
    fn identity_rejects_oversized_values() {
        let long = "n".repeat(MAX_IDENTITY_LEN + 1);
        assert!(matches!(
            NodeId::new(long),
            Err(IdentityError::TooLong { max: 128, .. })
        ));
    }

    #[test]
    fn label_allows_spaces_but_not_padding() {
        assert!(WorkerName::new("west rack 3").is_ok());
        assert_eq!(
            WorkerName::new(" west"),
            Err(IdentityError::UntrimmedLabel { kind: "WorkerName" })
        );
    }

    #[test]
    fn labels_and_identities_are_distinct_types() {
        // The point of the split: a label cannot stand in for an identity.
        // `NodeId::new(name.as_str())` is possible but is an explicit,
        // greppable act rather than an implicit coercion.
        let name = WorkerName::new("worker-alpha").unwrap();
        let id = NodeId::new("node-abc-123").unwrap();
        assert_ne!(name.as_str(), id.as_str());
    }

    #[test]
    fn identities_round_trip_as_json_strings() {
        let id = NodeId::new("node-abc-123").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"node-abc-123\"");
        assert_eq!(serde_json::from_str::<NodeId>(&json).unwrap(), id);
    }

    #[test]
    fn deserialization_validates_rather_than_trusting_input() {
        let error = serde_json::from_str::<NodeId>("\"\"").unwrap_err();
        assert!(error.to_string().contains("must not be empty"));
    }

    #[test]
    fn protocol_range_bounds_are_inclusive_and_ordered() {
        let range = ProtocolRange::new(ProtocolVersion(1), ProtocolVersion(3)).unwrap();
        assert!(range.accepts(ProtocolVersion(1)));
        assert!(range.accepts(ProtocolVersion(3)));
        assert!(!range.accepts(ProtocolVersion(4)));
        assert!(ProtocolRange::new(ProtocolVersion(3), ProtocolVersion(1)).is_err());
    }
}
