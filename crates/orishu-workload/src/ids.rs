//! The validated names a workload uses to refer to its own parts.
//!
//! Every one of these is a distinct Rust type over a `String`, for the reason
//! ADR 0013 gives for formation and node identity: a channel id and a component
//! instance id are both short ASCII strings, and confusing one for the other
//! must fail to compile rather than fail to validate.
//!
//! All of them are bounded at construction. A workload document is hostile
//! input, and these are the first values a reader materialises from it.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Maximum length of a machine-facing symbol, in bytes.
///
/// Sized to hold a reverse-DNS identifier with a version suffix — for example
/// `dev.orishu.electromagnetism.yee/v2` — with generous room to spare.
pub const MAX_SYMBOL_LEN: usize = 128;

/// Maximum length of a human-facing label, in bytes.
///
/// Matches the DNS subdomain limit `orishu-identity` uses for the same reason:
/// operator-visible names appear in the same places across the product.
pub const MAX_LABEL_LEN: usize = 253;

/// Maximum length of a media type, in bytes.
///
/// Comfortably above any registered type plus parameters.
pub const MAX_MEDIA_TYPE_LEN: usize = 255;

/// Why a workload name could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    /// The value is empty.
    #[error("{kind} must not be empty")]
    Empty {
        /// The type that was being constructed.
        kind: &'static str,
    },
    /// The value exceeds its declared bound.
    #[error("{kind} is {len} bytes, over the {max}-byte limit")]
    TooLong {
        /// The type that was being constructed.
        kind: &'static str,
        /// Length supplied.
        len: usize,
        /// Length permitted.
        max: usize,
    },
    /// The value contains a character the grammar forbids.
    #[error("{kind} contains a forbidden character at byte {offset}")]
    ForbiddenCharacter {
        /// The type that was being constructed.
        kind: &'static str,
        /// Byte offset of the first offending character.
        offset: usize,
    },
    /// A label has leading or trailing whitespace.
    #[error("{kind} must not have leading or trailing whitespace")]
    Untrimmed {
        /// The type that was being constructed.
        kind: &'static str,
    },
    /// A media type is not `type/subtype`.
    #[error("{kind} must be spelled `type/subtype`, got `{found}`")]
    NotAMediaType {
        /// The type that was being constructed.
        kind: &'static str,
        /// The value as supplied.
        found: String,
    },
}

/// A machine-facing symbol: non-empty, bounded, printable non-whitespace ASCII.
///
/// Deliberately not case-folded. Two symbols differing only in case are two
/// different symbols, because the canonical encoding commits to the exact bytes
/// and silently folding them would make one workload have two identities.
fn validate_symbol(kind: &'static str, value: &str) -> Result<(), NameError> {
    if value.is_empty() {
        return Err(NameError::Empty { kind });
    }
    if value.len() > MAX_SYMBOL_LEN {
        return Err(NameError::TooLong {
            kind,
            len: value.len(),
            max: MAX_SYMBOL_LEN,
        });
    }
    match value.bytes().position(|byte| !byte.is_ascii_graphic()) {
        Some(offset) => Err(NameError::ForbiddenCharacter { kind, offset }),
        None => Ok(()),
    }
}

/// A human-facing label: non-empty, bounded, no control characters, no
/// surrounding whitespace.
///
/// Whitespace is rejected rather than trimmed so that two labels which look
/// identical cannot encode differently.
fn validate_label(kind: &'static str, value: &str) -> Result<(), NameError> {
    if value.is_empty() {
        return Err(NameError::Empty { kind });
    }
    if value.len() > MAX_LABEL_LEN {
        return Err(NameError::TooLong {
            kind,
            len: value.len(),
            max: MAX_LABEL_LEN,
        });
    }
    if let Some((offset, _)) = value.char_indices().find(|(_, c)| c.is_control()) {
        return Err(NameError::ForbiddenCharacter { kind, offset });
    }
    if value.trim() != value {
        return Err(NameError::Untrimmed { kind });
    }
    Ok(())
}

/// A media type: bounded, lowercase, exactly one `/`.
fn validate_media_type(kind: &'static str, value: &str) -> Result<(), NameError> {
    if value.is_empty() {
        return Err(NameError::Empty { kind });
    }
    if value.len() > MAX_MEDIA_TYPE_LEN {
        return Err(NameError::TooLong {
            kind,
            len: value.len(),
            max: MAX_MEDIA_TYPE_LEN,
        });
    }
    if let Some(offset) = value
        .bytes()
        .position(|byte| !byte.is_ascii_graphic() || byte.is_ascii_uppercase())
    {
        return Err(NameError::ForbiddenCharacter { kind, offset });
    }
    let mut parts = value.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(kind_part), Some(subtype), None) if !kind_part.is_empty() && !subtype.is_empty() => {
            Ok(())
        }
        _ => Err(NameError::NotAMediaType {
            kind,
            found: value.to_owned(),
        }),
    }
}

/// Validates an artifact role.
///
/// Lives here rather than in `artifact.rs` so every name in the model is
/// bounded by the same grammar; `ArtifactRole` itself is written out by hand
/// because it carries role-specific behaviour the macro does not generate.
pub(crate) fn validate_role(value: &str) -> Result<(), NameError> {
    validate_symbol("ArtifactRole", value)
}

/// Declares a validated string new-type.
///
/// The inner `String` stays private, so the validating constructor is the only
/// way in and no downstream code has to re-check. `Ord` is derived so these can
/// key a `BTreeMap`, which is how the model keeps authored map order from
/// reaching anything — the canonical encoder then imposes its own bytewise
/// ordering on top, so this ordering is a convenience rather than the contract.
macro_rules! validated_name {
    (
        $(#[$meta:meta])*
        $name:ident, $validator:path
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parses `value`, rejecting anything the grammar forbids.
            ///
            /// # Errors
            ///
            /// Returns [`NameError`] when the value is empty, exceeds its
            /// length bound, or contains a forbidden character.
            pub fn new(value: impl Into<String>) -> Result<Self, NameError> {
                let value = value.into();
                $validator(stringify!($name), &value)?;
                Ok(Self(value))
            }

            /// Borrows the validated value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = NameError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = NameError;
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

validated_name!(
    /// Names one component instance within one workload's graph.
    ///
    /// Scoped to the graph, not global: two workloads may both contain a
    /// `field` instance and they are unrelated.
    ComponentInstanceId,
    validate_symbol
);

validated_name!(
    /// Names one typed state or contribution channel within one graph.
    StateChannelId,
    validate_symbol
);

validated_name!(
    /// Names one node of the step plan.
    StepInvocationId,
    validate_symbol
);

validated_name!(
    /// Names a phase a component instance exports, as its schema declares it.
    ///
    /// The runtime uses this to select an admitted export; it is never an
    /// arbitrary function name a guest can be asked to run.
    PhaseId,
    validate_symbol
);

validated_name!(
    /// The stable identity of the simulation plugin a component came from.
    ///
    /// Authoring-time provenance that is pinned into the workload. It never
    /// selects executable code: the component's [`crate::ArtifactDescriptor`]
    /// does that, and installing a plugin of the same name elsewhere changes
    /// nothing.
    PluginId,
    validate_symbol
);

validated_name!(
    /// The stable identity of the computational model a component implements.
    ModelId,
    validate_symbol
);

validated_name!(
    /// The stable identity of a state or artifact schema.
    SchemaId,
    validate_symbol
);

validated_name!(
    /// What a component instance does in the graph, as its model declares it.
    ///
    /// For example `field-model`, `coupling`, `dynamics`, or `emitter`. An open
    /// vocabulary: ADR 0024 names those as initial roles rather than a closed
    /// list, so a plugin may declare another.
    ComponentRole,
    validate_symbol
);

validated_name!(
    /// The execution engine a component artifact targets, for example
    /// `wasm-component`.
    Engine,
    validate_symbol
);

validated_name!(
    /// The component lifecycle contract version, for example
    /// `orishu.component/v1`.
    LifecycleId,
    validate_symbol
);

validated_name!(
    /// The component-graph profile version, for example
    /// `orishu.workload-graph/v1`.
    ///
    /// Declares which graph and step-plan semantics the manifest was written
    /// against, so a worker can refuse a profile it does not implement instead
    /// of misreading one it does.
    GraphProfile,
    validate_symbol
);

validated_name!(
    /// The name of one frozen component-instance configuration parameter.
    ParameterName,
    validate_symbol
);

validated_name!(
    /// The name of one declared per-instance resource limit.
    LimitName,
    validate_symbol
);

validated_name!(
    /// The name of one placement-constraint key.
    ConstraintName,
    validate_symbol
);

validated_name!(
    /// A metadata label key.
    LabelKey,
    validate_symbol
);

validated_name!(
    /// A metadata label value.
    LabelValue,
    validate_label
);

validated_name!(
    /// The human-readable name of a workload.
    ///
    /// Useful for discovery and never an integrity identity: two unrelated
    /// workloads may share a name, and the [`crate::WorkloadDigest`] is what
    /// distinguishes them.
    WorkloadName,
    validate_label
);

validated_name!(
    /// An IANA-style media type describing an artifact's byte format.
    MediaType,
    validate_media_type
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_symbol_accepts_a_reverse_dns_identifier_with_a_version() {
        assert!(ModelId::new("dev.orishu.electromagnetism.yee/v2").is_ok());
    }

    #[test]
    fn a_symbol_rejects_whitespace_and_control_characters() {
        assert!(matches!(
            ComponentInstanceId::new("field model"),
            Err(NameError::ForbiddenCharacter { offset: 5, .. })
        ));
        assert!(matches!(
            StateChannelId::new("e\nfield"),
            Err(NameError::ForbiddenCharacter { offset: 1, .. })
        ));
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert_eq!(PhaseId::new(""), Err(NameError::Empty { kind: "PhaseId" }));
    }

    #[test]
    fn an_oversized_symbol_is_refused() {
        let long = "s".repeat(MAX_SYMBOL_LEN + 1);
        assert!(matches!(
            SchemaId::new(long),
            Err(NameError::TooLong { max: 128, .. })
        ));
    }

    #[test]
    fn a_label_allows_inner_spaces_but_not_padding() {
        assert!(WorkloadName::new("cavity resonance sweep").is_ok());
        assert_eq!(
            WorkloadName::new(" cavity"),
            Err(NameError::Untrimmed {
                kind: "WorkloadName"
            })
        );
    }

    #[test]
    fn a_media_type_needs_exactly_one_slash() {
        assert!(MediaType::new("application/wasm").is_ok());
        assert!(matches!(
            MediaType::new("application"),
            Err(NameError::NotAMediaType { .. })
        ));
        assert!(matches!(
            MediaType::new("application/vnd/orishu"),
            Err(NameError::NotAMediaType { .. })
        ));
    }

    #[test]
    fn a_media_type_is_lowercase_so_one_format_has_one_spelling() {
        assert!(matches!(
            MediaType::new("Application/WASM"),
            Err(NameError::ForbiddenCharacter { offset: 0, .. })
        ));
    }

    #[test]
    fn a_name_round_trips_through_serde_and_validates_on_the_way_in() {
        let id = ComponentInstanceId::new("field").expect("valid");
        let json = serde_json::to_string(&id).expect("it encodes");
        assert_eq!(json, "\"field\"");
        assert_eq!(
            serde_json::from_str::<ComponentInstanceId>(&json).expect("it decodes"),
            id
        );
        assert!(serde_json::from_str::<ComponentInstanceId>("\"a b\"").is_err());
    }
}
