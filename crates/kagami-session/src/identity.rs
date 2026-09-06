//! Who submitted a command, and which submission it was.
//!
//! Deliberately distinct types from [`kagami_catalog::CommandId`] and
//! [`kagami_catalog::ActorId`], despite the identical shape. A command
//! identity is what makes a resubmission idempotent, and it is only meaningful
//! against the authority that recorded it: replaying a catalog command's
//! identity through the document authority must not resolve to anything. Two
//! interchangeable types would let exactly that compile.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Longest accepted [`CommandId`] or [`ActorId`], in bytes.
pub const MAX_IDENTITY_BYTES: usize = 128;

/// Why a bounded caller-supplied identity was refused.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    /// The text was empty.
    #[error("identity must not be empty")]
    Empty,
    /// The text was longer than [`MAX_IDENTITY_BYTES`].
    #[error("identity is {found} bytes, over the limit of {limit}")]
    TooLong {
        /// The submitted length.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
}

macro_rules! bounded_identity {
    ($(#[$meta:meta])* $type_name:ident) => {
        $(#[$meta])*
        ///
        /// Serialised as its text and decoded through the same constructor, so
        /// an identity arriving in a message is bounded exactly as one built
        /// in process is.
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $type_name(String);

        impl $type_name {
            /// Validate and construct the identity.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdentityError::Empty);
                }
                if value.len() > MAX_IDENTITY_BYTES {
                    return Err(IdentityError::TooLong {
                        found: value.len(),
                        limit: MAX_IDENTITY_BYTES,
                    });
                }
                Ok(Self(value))
            }

            /// The identity's text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $type_name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $type_name {
            type Error = IdentityError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$type_name> for String {
            fn from(value: $type_name) -> Self {
                value.0
            }
        }
    };
}

bounded_identity!(
    /// Caller-chosen identity of one submitted command.
    ///
    /// Resending a submission that already succeeded replays its recorded
    /// outcome rather than applying the change twice, so an adapter that
    /// cannot tell a lost response from a lost request can retry safely.
    ///
    /// One identity names one *request*, not just one string. Replay compares
    /// the actor, guard, gesture and command as well, so reusing an identity
    /// for a different submission is refused
    /// ([`crate::SessionRejection::CommandIdentityConflict`]) rather than
    /// answered with the first one's outcome. Retention is bounded, so an
    /// identity old enough to have been forgotten is a new request.
    CommandId
);
bounded_identity!(
    /// Who submitted a command.
    ///
    /// Recorded on the resulting event so a change can be attributed — the UI,
    /// a named MCP client, an undo. Never used for authorization here; ADR
    /// 0006 keeps this session single-user and same-machine.
    ActorId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_must_be_present_and_bounded() {
        assert_eq!(CommandId::new(""), Err(IdentityError::Empty));
        assert_eq!(
            ActorId::new("a".repeat(MAX_IDENTITY_BYTES + 1)),
            Err(IdentityError::TooLong {
                found: MAX_IDENTITY_BYTES + 1,
                limit: MAX_IDENTITY_BYTES,
            })
        );
        assert_eq!(CommandId::new("cmd-1").expect("valid").as_str(), "cmd-1");
    }

    #[test]
    fn identities_display_as_their_text() {
        assert_eq!(ActorId::new("ui").expect("valid").to_string(), "ui");
    }

    #[test]
    fn serde_revalidates_an_identity_at_the_message_boundary() {
        let id = CommandId::new("ui-1").expect("valid");
        let encoded = serde_json::to_string(&id).expect("encodes");
        assert_eq!(encoded, "\"ui-1\"");
        assert_eq!(
            serde_json::from_str::<CommandId>(&encoded).expect("decodes"),
            id
        );

        // The bound belongs to the type, so a message cannot widen it.
        assert!(serde_json::from_str::<CommandId>("\"\"").is_err());
        let oversized = format!("\"{}\"", "a".repeat(MAX_IDENTITY_BYTES + 1));
        assert!(serde_json::from_str::<ActorId>(&oversized).is_err());
    }
}
