//! Human-facing labels.
//!
//! Deliberately *not* an [`orishu_variables::Name`]. Catalog identifiers are
//! variable-name segments because every catalog property is published as a
//! binding at `catalog.template.component.property`, so a name that could not
//! be a segment could never be referenced. An object's name has no such role:
//! a researcher calls something "Earth core" or "Probe 2 (control)", two
//! objects may share a name, and renaming one must not disturb anything —
//! stable identity lives in [`crate::ObjectId`].
//!
//! What is enforced is only what keeps a label usable: it is trimmed,
//! non-empty, bounded, and free of control characters that would corrupt a
//! log line, a diagnostic, or a scene tree row.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Longest accepted display name, in bytes.
///
/// A structural ceiling, not a tunable policy: it exists so an MCP client
/// cannot make one label an unbounded allocation. [`crate::Limits`]
/// deliberately does not restate it — one bound in two places is one bound
/// that can disagree with itself.
pub const MAX_DISPLAY_NAME_BYTES: usize = 256;

/// Why a display name could not be accepted.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum NameError {
    /// The text was empty, or entirely whitespace.
    #[error("a name must not be empty")]
    Empty,
    /// The text exceeded [`MAX_DISPLAY_NAME_BYTES`].
    #[error("name is {found} bytes, over the limit of {limit}")]
    TooLong {
        /// The submitted length.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// The text contained a control character.
    #[error("a name must not contain control characters")]
    ControlCharacter,
}

/// A bounded, non-empty human label for an object or instrument.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DisplayName(String);

impl DisplayName {
    /// Parse a label, trimming surrounding whitespace.
    ///
    /// Trimming is normalisation at the boundary rather than a rejection:
    /// a user who typed a trailing space meant the name without it, and
    /// storing both spellings would make two identical-looking labels
    /// compare unequal.
    pub fn new(value: impl Into<String>) -> Result<Self, NameError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(NameError::Empty);
        }
        if trimmed.len() > MAX_DISPLAY_NAME_BYTES {
            return Err(NameError::TooLong {
                found: trimmed.len(),
                limit: MAX_DISPLAY_NAME_BYTES,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(NameError::ControlCharacter);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The label's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for DisplayName {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for DisplayName {
    type Error = NameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DisplayName> for String {
    fn from(value: DisplayName) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_labels_a_researcher_actually_types() {
        // Every one of these would be refused by a variable-name segment,
        // which is exactly why this is a different type.
        for label in [
            "Earth",
            "Earth core",
            "Probe 2 (control)",
            "α-particle",
            "1st",
        ] {
            assert_eq!(DisplayName::new(label).expect("valid").as_str(), label);
        }
    }

    #[test]
    fn trims_rather_than_refusing_surrounding_whitespace() {
        assert_eq!(
            DisplayName::new("  Earth \n").expect("valid").as_str(),
            "Earth"
        );
    }

    #[test]
    fn refuses_empty_oversized_and_control_bearing_labels() {
        assert_eq!(DisplayName::new(""), Err(NameError::Empty));
        assert_eq!(DisplayName::new("   "), Err(NameError::Empty));
        assert_eq!(
            DisplayName::new("a\u{7}b"),
            Err(NameError::ControlCharacter)
        );
        let long = "a".repeat(MAX_DISPLAY_NAME_BYTES + 1);
        assert_eq!(
            DisplayName::new(long),
            Err(NameError::TooLong {
                found: MAX_DISPLAY_NAME_BYTES + 1,
                limit: MAX_DISPLAY_NAME_BYTES,
            })
        );
    }

    #[test]
    fn two_objects_may_share_a_label() {
        // Names are labels, not identities: this must not be an error.
        assert_eq!(DisplayName::new("Earth"), DisplayName::new("Earth"));
    }

    #[test]
    fn serde_revalidates_at_the_document_boundary() {
        let name = DisplayName::new("Earth").expect("valid");
        let encoded = serde_json::to_string(&name).expect("encodes");
        assert_eq!(encoded, "\"Earth\"");
        assert_eq!(
            serde_json::from_str::<DisplayName>(&encoded).expect("decodes"),
            name
        );
        assert!(serde_json::from_str::<DisplayName>("\"\"").is_err());
    }
}
