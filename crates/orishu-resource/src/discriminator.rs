//! The `apiVersion`/`kind` pair that says what a document is.
//!
//! Both are validated new-types rather than strings, for two reasons.
//!
//! The first is bounds. A discriminator arrives from an untrusted file or an
//! untrusted peer, and it is read *before* anything else about the document is
//! trusted — so it is the one field a hostile input is guaranteed to reach.
//! [`ApiVersion`] and [`Kind`] check length against a declared associated
//! constant on the borrowed `&str`, before any copy is made.
//!
//! The second is that parsing here makes the value valid everywhere
//! downstream. A [`Resource`](crate::Resource) keeps its discriminator fields
//! private and constructor-supplied, so a resource cannot exist carrying a
//! placeholder kind.
//!
//! This module names no concrete version or kind. `orishu.dev/v1`,
//! `kagami.catalog/v1`, `Workload`, and `ObjectTemplate` are the owning
//! domain's constants, and the owning domain decides which values it supports.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Discriminator, ResourceError, UnexpectedDiscriminator};

/// Validate a borrowed discriminator before it is copied.
///
/// `first` and `rest` describe the grammar; both are checked bytewise, which
/// is sound because a value containing any non-ASCII byte fails `rest`
/// anyway.
fn check(
    field: Discriminator,
    value: &str,
    limit: usize,
    first: fn(u8) -> bool,
    rest: fn(u8) -> bool,
) -> Result<(), ResourceError> {
    if value.is_empty() {
        return Err(ResourceError::Empty { field });
    }
    // Before `to_owned`: an oversized discriminator must not be allocated
    // merely to be rejected.
    if value.len() > limit {
        return Err(ResourceError::TooLong {
            field,
            found: value.len(),
            limit,
        });
    }
    let bytes = value.as_bytes();
    let well_formed = first(bytes[0]) && bytes[1..].iter().copied().all(rest);
    if !well_formed {
        return Err(ResourceError::Syntax {
            field,
            found: value.to_owned(),
        });
    }
    Ok(())
}

/// Generates one validated discriminator new-type.
///
/// The two share their whole surface — construction, bounds, serde, and
/// display — and differ only in grammar and limit, so writing them twice
/// would be two places for the bound to drift.
macro_rules! discriminator {
    (
        $(#[$meta:meta])*
        $name:ident, $field:expr, $limit:expr, $first:expr, $rest:expr, $expecting:literal
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// The largest value this field accepts, in bytes.
            pub const MAX_LEN: usize = $limit;

            /// Validate and construct the value.
            ///
            /// The bound is enforced against the *borrowed* view, so rejecting
            /// an oversized discriminator copies nothing: the
            /// `AsRef<str> + Into<String>` pair exists precisely so that
            /// `check` runs before `into`. A `&str` that passes is copied once
            /// on the success path; a `String` that passes is moved, not
            /// copied again.
            ///
            /// This ordering is a hostile-input property, not a micro
            /// optimisation — a discriminator is read before anything else
            /// about a document is trusted, so it is the field an oversized
            /// input reaches first. `tests/allocation.rs` asserts it.
            pub fn new<V: AsRef<str> + Into<String>>(value: V) -> Result<Self, ResourceError> {
                check($field, value.as_ref(), Self::MAX_LEN, $first, $rest)?;
                Ok(Self(value.into()))
            }

            /// Construct from a compile-time constant.
            ///
            /// # Panics
            ///
            /// If `value` is not valid. This is for a crate's own `const`
            /// discriminator, where an invalid value is a bug in that crate
            /// rather than untrusted input; use [`Self::new`] for anything
            /// that came from a file, a peer, or a client.
            pub fn from_static(value: &'static str) -> Self {
                match Self::new(value) {
                    Ok(valid) => valid,
                    Err(error) => panic!("`{value}` is not a valid discriminator: {error}"),
                }
            }

            /// The value's text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = ResourceError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ResourceError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = ResourceError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
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

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            /// Hand-written rather than `try_from = "String"` so the bound is
            /// checked on the borrowed `&str`, before a copy exists.
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct DiscriminatorVisitor;

                impl Visitor<'_> for DiscriminatorVisitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str($expecting)
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<$name, E> {
                        check($field, value, $name::MAX_LEN, $first, $rest)
                            .map_err(E::custom)?;
                        Ok($name(value.to_owned()))
                    }

                    fn visit_string<E: de::Error>(self, value: String) -> Result<$name, E> {
                        check($field, &value, $name::MAX_LEN, $first, $rest)
                            .map_err(E::custom)?;
                        Ok($name(value))
                    }
                }

                deserializer.deserialize_str(DiscriminatorVisitor)
            }
        }
    };
}

fn alphanumeric(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

fn letter(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn api_version_body(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/')
}

discriminator!(
    /// A resource format version, such as a `group/version` pair.
    ///
    /// The grammar is deliberately wider than any one domain needs: it must
    /// accept a version this crate has never heard of, because rejecting an
    /// unknown *future* version is the owning domain's decision and must be
    /// reported as a version mismatch rather than as a syntax error.
    ///
    /// Accepts up to [`Self::MAX_LEN`] ASCII bytes that start alphanumeric and
    /// continue with alphanumerics, `.`, `-`, `_`, or `/`.
    ApiVersion,
    Discriminator::ApiVersion,
    253,
    alphanumeric,
    api_version_body,
    "a resource apiVersion string"
);

discriminator!(
    /// A resource kind, such as the name of one resource type in a group.
    ///
    /// Accepts up to [`Self::MAX_LEN`] ASCII alphanumeric bytes starting with
    /// a letter — the shape of a type name, which is what a kind is.
    Kind,
    Discriminator::Kind,
    63,
    letter,
    alphanumeric,
    "a resource kind string"
);

/// The `apiVersion`/`kind` pair on its own, decoded before the rest of a
/// document is trusted.
///
/// Unknown fields are accepted here, and only here. That is the whole point:
/// a document from a future format version must be refused *for its version*,
/// not for whichever field of its unrecognisable body happens to be read
/// first. Decoding a header therefore tells a caller what a document claims to
/// be, without committing to being able to decode it.
///
/// ```
/// use orishu_resource::{ApiVersion, Kind, ResourceHeader};
///
/// let header: ResourceHeader = serde_json::from_str(
///     r#"{"apiVersion": "example.dev/v2", "kind": "Widget", "spec": {"anything": [1, 2]}}"#,
/// )?;
///
/// let mismatch = header
///     .expect(&ApiVersion::from_static("example.dev/v1"), &Kind::from_static("Widget"))
///     .unwrap_err();
/// assert!(mismatch.is_version_mismatch());
/// assert_eq!(
///     mismatch.to_string(),
///     r#"expected apiVersion="example.dev/v1" kind="Widget", got apiVersion="example.dev/v2" kind="Widget""#
/// );
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHeader {
    #[serde(rename = "apiVersion")]
    api_version: ApiVersion,
    kind: Kind,
}

impl ResourceHeader {
    /// Build a header from an already-validated pair.
    pub fn new(api_version: ApiVersion, kind: Kind) -> Self {
        Self { api_version, kind }
    }

    /// The declared format version.
    pub fn api_version(&self) -> &ApiVersion {
        &self.api_version
    }

    /// The declared resource kind.
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Check this header against the discriminator the caller supports.
    ///
    /// The expected values come from the caller, so the resulting message
    /// names the caller's resource rather than one this crate hard-coded.
    pub fn expect(
        &self,
        api_version: &ApiVersion,
        kind: &Kind,
    ) -> Result<(), UnexpectedDiscriminator> {
        if &self.api_version == api_version && &self.kind == kind {
            return Ok(());
        }
        Err(UnexpectedDiscriminator {
            expected_api_version: api_version.clone(),
            expected_kind: kind.clone(),
            found_api_version: self.api_version.clone(),
            found_kind: self.kind.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_version_pair_and_a_bare_version_are_both_accepted() {
        assert_eq!(
            ApiVersion::new("orishu.dev/v1").unwrap().as_str(),
            "orishu.dev/v1"
        );
        assert_eq!(
            ApiVersion::new("kagami.catalog/v1").unwrap().as_str(),
            "kagami.catalog/v1"
        );
        assert_eq!(
            ApiVersion::new("orishu.workload-graph/v1")
                .unwrap()
                .as_str(),
            "orishu.workload-graph/v1"
        );
        assert!(ApiVersion::new("v1").is_ok());
    }

    #[test]
    fn an_unknown_future_version_is_syntactically_fine() {
        // Refusing it is the owning domain's decision, reported as a version
        // mismatch. A syntax error here would report the wrong thing.
        assert!(ApiVersion::new("kagami.catalog/v99").is_ok());
        assert!(Kind::new("SomethingNobodyHasWrittenYet").is_ok());
    }

    #[test]
    fn an_empty_discriminator_is_refused() {
        assert_eq!(
            ApiVersion::new(""),
            Err(ResourceError::Empty {
                field: Discriminator::ApiVersion
            })
        );
        assert_eq!(
            Kind::new(""),
            Err(ResourceError::Empty {
                field: Discriminator::Kind
            })
        );
    }

    #[test]
    fn an_oversized_discriminator_is_refused_by_its_declared_bound() {
        let long = "a".repeat(ApiVersion::MAX_LEN + 1);
        assert_eq!(
            ApiVersion::new(long),
            Err(ResourceError::TooLong {
                field: Discriminator::ApiVersion,
                found: ApiVersion::MAX_LEN + 1,
                limit: ApiVersion::MAX_LEN,
            })
        );
        assert!(ApiVersion::new("a".repeat(ApiVersion::MAX_LEN)).is_ok());

        let long = "A".repeat(Kind::MAX_LEN + 1);
        assert!(matches!(
            Kind::new(long),
            Err(ResourceError::TooLong { .. })
        ));
        assert!(Kind::new("A".repeat(Kind::MAX_LEN)).is_ok());
    }

    #[test]
    fn a_discriminator_that_is_not_a_name_is_refused() {
        assert!(matches!(
            ApiVersion::new("@@@"),
            Err(ResourceError::Syntax { .. })
        ));
        assert!(matches!(
            ApiVersion::new("/leading-slash"),
            Err(ResourceError::Syntax { .. })
        ));
        // A kind is a type name: no separators, no leading digit.
        assert!(matches!(
            Kind::new("Object-Template"),
            Err(ResourceError::Syntax { .. })
        ));
        assert!(matches!(
            Kind::new("1Workload"),
            Err(ResourceError::Syntax { .. })
        ));
        assert!(matches!(
            Kind::new("wörkload"),
            Err(ResourceError::Syntax { .. })
        ));
    }

    #[test]
    fn the_rejected_value_is_reported_but_stays_bounded() {
        let Err(ResourceError::Syntax { found, .. }) = ApiVersion::new("nope!") else {
            panic!("expected a syntax error");
        };
        assert_eq!(found, "nope!");
        assert!(found.len() <= ApiVersion::MAX_LEN);
    }

    #[test]
    fn an_oversized_discriminator_is_refused_when_deserialized_too() {
        let long = "a".repeat(ApiVersion::MAX_LEN + 1);
        let error = serde_json::from_str::<ApiVersion>(&format!("\"{long}\"")).unwrap_err();
        assert!(error.to_string().contains("exceeds"), "{error}");
    }

    #[test]
    fn a_header_ignores_a_body_it_cannot_decode() {
        let header: ResourceHeader =
            serde_yaml::from_str("apiVersion: example.dev/v2\nkind: Nonsense\nwat: [1, 2]\n")
                .unwrap();
        assert_eq!(header.api_version().as_str(), "example.dev/v2");
        assert_eq!(header.kind().as_str(), "Nonsense");
    }

    #[test]
    fn expect_reports_both_pairs_without_naming_a_resource_this_crate_knows() {
        let header = ResourceHeader::new(
            ApiVersion::from_static("example.dev/v1"),
            Kind::from_static("Gadget"),
        );
        let error = header
            .expect(
                &ApiVersion::from_static("example.dev/v1"),
                &Kind::from_static("Widget"),
            )
            .unwrap_err();
        assert!(!error.is_version_mismatch());
        assert_eq!(
            error.to_string(),
            r#"expected apiVersion="example.dev/v1" kind="Widget", got apiVersion="example.dev/v1" kind="Gadget""#
        );
    }

    #[test]
    fn a_matching_header_is_accepted() {
        let header = ResourceHeader::new(
            ApiVersion::from_static("example.dev/v1"),
            Kind::from_static("Widget"),
        );
        assert!(
            header
                .expect(
                    &ApiVersion::from_static("example.dev/v1"),
                    &Kind::from_static("Widget")
                )
                .is_ok()
        );
    }

    #[test]
    #[should_panic(expected = "is not a valid discriminator")]
    fn from_static_refuses_an_invalid_constant() {
        let _ = Kind::from_static("not a kind");
    }
}
