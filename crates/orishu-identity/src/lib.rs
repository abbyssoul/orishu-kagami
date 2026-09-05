//! Shared identity types for Orishu cluster formations and their members.
//!
//! [ADR 0013](../../../docs/adr/0013-cluster-formation-and-node-identity.md)
//! separates *identity* from *labels*:
//!
//! - A cluster is one ephemeral **formation** with a generated [`FormationId`].
//!   That ID scopes every post-admission peer message and durable provenance
//!   record.
//! - A formation assigns each admitted member a unique [`NodeId`]. Membership,
//!   ownership, administrative targeting, tombstones, and producing-node
//!   provenance use that ID.
//! - [`ClusterName`] and [`WorkerName`] are non-unique operator labels. They
//!   are deliberately *not* convertible to or comparable with an identity type,
//!   so a label can never satisfy an identity check by accident.
//!
//! The crate exists because two consumers need the same contract while having
//! incompatible dependency budgets: `orishu` carries an HTTP client (and with
//! it `reqwest`, `tokio`, and `chrono`), whereas the `orishu-membership`
//! functional core must build with no networking, async-runtime, clock,
//! filesystem, TLS, or RNG dependency at all. Defining the types once here and
//! re-exporting them from `orishu` keeps a single definition without forcing
//! the membership core to inherit the client's dependency tree.
//!
//! # Wire representation
//!
//! Every type serializes to the shape the peer and client protocols already
//! specify: identities and labels are text strings, [`CertFingerprint`] is a
//! 32-byte string (a hex string in human-readable formats such as JSON, a byte
//! string in CBOR), and [`VersionTuple`] is the protocol's `VersionTuple` map.
//! Deserialization is validating: a malformed identity fails to parse rather
//! than entering the domain as an unchecked `String`.

mod fingerprint;
mod ids;
mod tombstone;
mod version;

pub use fingerprint::CertFingerprint;
pub use ids::{ClusterName, FormationId, NodeId, ProtocolRange, ProtocolVersion, WorkerName};
pub use tombstone::{MembershipTombstone, RemovalMode};
pub use version::{Incarnation, VersionTuple};

/// Why a value could not be parsed into an identity, label, or fingerprint.
///
/// Identity parsing is a boundary check on hostile input, so every variant
/// names the offending field and the rule it broke rather than collapsing into
/// a display string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    /// The value was empty, and no identity or label may be empty.
    #[error("{kind} must not be empty")]
    Empty {
        /// Type of value being parsed, for example `NodeId`.
        kind: &'static str,
    },

    /// The value exceeded the maximum length for its kind.
    #[error("{kind} is {len} bytes, exceeding the {max}-byte maximum")]
    TooLong {
        /// Type of value being parsed.
        kind: &'static str,
        /// Length of the offending value, in bytes.
        len: usize,
        /// Maximum permitted length, in bytes.
        max: usize,
    },

    /// The value contained a character that is not permitted for its kind.
    ///
    /// Identities admit only printable, non-whitespace ASCII so that they can
    /// be embedded in canonical hash inputs, log lines, and URI path segments
    /// without escaping. Labels are more permissive but still reject control
    /// characters and surrounding whitespace.
    #[error("{kind} contains a forbidden character at byte {offset}")]
    ForbiddenCharacter {
        /// Type of value being parsed.
        kind: &'static str,
        /// Byte offset of the first offending character.
        offset: usize,
    },

    /// A label had leading or trailing whitespace, which would make two
    /// visually identical labels compare unequal.
    #[error("{kind} has leading or trailing whitespace")]
    UntrimmedLabel {
        /// Type of value being parsed.
        kind: &'static str,
    },

    /// A certificate fingerprint was not exactly 32 bytes of SHA-256 output.
    #[error("certificate fingerprint is {len} bytes, expected 32")]
    FingerprintLength {
        /// Length actually supplied, in bytes.
        len: usize,
    },

    /// A certificate fingerprint was given as text that is not valid hex.
    #[error("certificate fingerprint is not valid hexadecimal")]
    FingerprintEncoding,
}
