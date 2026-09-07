//! Errors this crate is allowed to report.
//!
//! The boundary is deliberate. This crate reports *structural* failures: a
//! discriminator that is empty, over its byte bound, syntactically invalid, or
//! not the one the caller asked for. It never reports why a workload's domain
//! is inconsistent, why a catalog component does not resolve, or why a
//! membership projection is stale — those are structured errors owned by the
//! resource domain, and flattening them through here would lose them.
//!
//! Nothing in this module mentions a concrete `apiVersion` or `kind`. The
//! expected values in [`UnexpectedDiscriminator`] are supplied by the caller,
//! so the message reads correctly for a workload, a cluster projection, a
//! node, an object template, or a resource that does not exist yet.

use std::fmt;

use crate::discriminator::{ApiVersion, Kind};

/// Which of the two discriminator fields an error is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Discriminator {
    /// The `apiVersion` field.
    ApiVersion,
    /// The `kind` field.
    Kind,
}

impl Discriminator {
    /// The field's spelling on the wire.
    pub const fn field_name(self) -> &'static str {
        match self {
            Discriminator::ApiVersion => "apiVersion",
            Discriminator::Kind => "kind",
        }
    }

    /// The human name of what the field holds.
    const fn subject(self) -> &'static str {
        match self {
            Discriminator::ApiVersion => "resource API version",
            Discriminator::Kind => "resource kind",
        }
    }
}

impl fmt::Display for Discriminator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.field_name())
    }
}

/// Why a discriminator value could not be accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    /// The value was empty.
    Empty {
        /// The offending field.
        field: Discriminator,
    },
    /// The value exceeded the field's declared byte bound.
    ///
    /// Checked before the value is copied, so an oversized discriminator from
    /// a hostile document is refused rather than allocated.
    TooLong {
        /// The offending field.
        field: Discriminator,
        /// How many bytes arrived.
        found: usize,
        /// The declared bound.
        limit: usize,
    },
    /// The value contained a character the field's grammar does not allow.
    ///
    /// `found` is within the field's byte bound, because [`Self::TooLong`] is
    /// decided first.
    Syntax {
        /// The offending field.
        field: Discriminator,
        /// The rejected value, bounded by the field's limit.
        found: String,
    },
}

impl ResourceError {
    /// The field the error is about.
    pub fn field(&self) -> Discriminator {
        match self {
            ResourceError::Empty { field }
            | ResourceError::TooLong { field, .. }
            | ResourceError::Syntax { field, .. } => *field,
        }
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceError::Empty { field } => {
                write!(formatter, "{field} must not be empty")
            }
            ResourceError::TooLong {
                field,
                found,
                limit,
            } => write!(
                formatter,
                "{field} is {found} bytes, which exceeds the {limit}-byte limit"
            ),
            ResourceError::Syntax { field, found } => write!(
                formatter,
                "{field} {found:?} is not a valid {}",
                field.subject()
            ),
        }
    }
}

impl std::error::Error for ResourceError {}

/// A resource whose discriminator is well-formed but is not the resource the
/// caller expected.
///
/// Both the expected and the actual pair are reported, and both come from the
/// caller's own constants — this crate has no opinion about which resources
/// exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnexpectedDiscriminator {
    /// The `apiVersion` the caller required.
    pub expected_api_version: ApiVersion,
    /// The `kind` the caller required.
    pub expected_kind: Kind,
    /// The `apiVersion` the document actually carried.
    pub found_api_version: ApiVersion,
    /// The `kind` the document actually carried.
    pub found_kind: Kind,
}

impl UnexpectedDiscriminator {
    /// `true` when only the version differs, which is the case a caller may
    /// want to report as "a document from another format version" rather than
    /// as a wholly unrelated resource.
    pub fn is_version_mismatch(&self) -> bool {
        self.found_kind == self.expected_kind && self.found_api_version != self.expected_api_version
    }
}

impl fmt::Display for UnexpectedDiscriminator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "expected apiVersion={:?} kind={:?}, got apiVersion={:?} kind={:?}",
            self.expected_api_version.as_str(),
            self.expected_kind.as_str(),
            self.found_api_version.as_str(),
            self.found_kind.as_str()
        )
    }
}

impl std::error::Error for UnexpectedDiscriminator {}
