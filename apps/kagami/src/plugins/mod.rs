//! Kagami's local plugin-management authority and imperative package adapters.
//!
//! No guest is executed by these operations. Scientific admission remains a
//! separate runtime boundary. This module is intentionally app-local.

pub mod cli;

#[cfg(unix)]
pub(crate) mod files;
#[cfg(unix)]
mod inventory;
#[cfg(unix)]
mod package;
#[cfg(unix)]
pub use inventory::*;
#[cfg(unix)]
pub use package::*;

use serde::{Deserialize, Serialize};

/// Default local inventory shared by the CLI and native application.
#[cfg(unix)]
pub fn default_directory() -> Result<std::path::PathBuf, Error> {
    use std::path::PathBuf;
    if let Some(path) = std::env::var_os("KAGAMI_PLUGIN_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let root = if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(path)
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            Error::new(
                Code::IoFailure,
                "provide a plugin directory when no user data root is configured",
            )
        })?)
        .join(".local/share")
    };
    if !root.is_absolute() {
        return Err(Error::new(
            Code::InvalidSelection,
            "user data root must be absolute",
        ));
    }
    Ok(root.join("kagami/plugins"))
}

/// Explicit attachment suggestions, not defaults inserted by load/admission.
/// Missing defaults remain missing; the authority decides whether the proposed
/// composition is complete and valid, including expressions and constraints.
pub fn component_defaults(
    schema: &kagami_catalog::ComponentSchema,
) -> std::collections::BTreeMap<kagami_catalog::PropertyName, kagami_document::AuthoredValue> {
    use kagami_document::AuthoredValue;
    use orishu_plugin::PropertyType;
    schema
        .properties
        .iter()
        .filter_map(|(name, p)| {
            let value = match p.plugin_declaration()? {
                PropertyType::Quantity {
                    default_expression: Some(expression),
                    ..
                } => AuthoredValue::si(expression.clone()),
                PropertyType::Boolean {
                    default: Some(value),
                } => AuthoredValue::Boolean(*value),
                PropertyType::Text {
                    default: Some(value),
                    ..
                } => AuthoredValue::Text(value.clone()),
                _ => return None,
            };
            Some((name.clone(), value))
        })
        .collect()
}

/// Stable local-management failure category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Code {
    /// Bad source, index or command.
    Malformed,
    /// Explicit caller budget was exceeded.
    LimitExceeded,
    /// Stored or supplied bytes fail identity verification.
    IntegrityMismatch,
    /// Unsupported format, contract or platform.
    UnsupportedVersion,
    /// Selected release/plugin is not installed or does not match.
    InvalidSelection,
    /// Caller must refresh its inventory before retrying.
    StaleRevision,
    /// An open document/run retains this release.
    InUse,
    /// Another local writer holds the mutation lock; retry explicitly.
    Busy,
    /// Filesystem operation failed.
    IoFailure,
}

/// Bounded structured failure, without copying paths or underlying OS messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// Stable category, independent of display text.
    pub code: Code,
    /// Diagnostic explanation (not rejected content).
    pub message: String,
    /// Stale command's expected revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    /// Current revision, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_revision: Option<u64>,
}
impl Error {
    fn new(code: Code, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
            expected_revision: None,
            actual_revision: None,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::new(Code::IoFailure, "local filesystem operation failed")
    }
}
impl From<orishu_plugin::Error> for Error {
    fn from(error: orishu_plugin::Error) -> Self {
        use orishu_plugin::ErrorCode as P;
        Self::new(
            match error.code {
                P::Malformed => Code::Malformed,
                P::LimitExceeded => Code::LimitExceeded,
                P::IntegrityMismatch => Code::IntegrityMismatch,
                P::UnsupportedVersion => Code::UnsupportedVersion,
                P::InvalidSelection => Code::InvalidSelection,
            },
            &error.message,
        )
    }
}
