//! Local acquisition metadata, deliberately absent from release/workload identity.
//! Paths describe a past user-selected input, not a trusted fetch/update location.
use super::{Code, Error};
use serde::{Deserialize, Serialize};
use std::{
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

const MAX_PATH_BYTES: usize = 4096;

/// The input explicitly loaded by Kagami, never inferred from bundle contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OriginKind {
    LocalBundle,
    LocalSourceDirectory,
}

/// Bounded lossless absolute Unix path, with readable UTF-8 where possible.
/// Non-UTF-8 paths use lowercase hex; neither spelling is executable metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "encoding",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
enum OriginPath {
    Utf8(String),
    UnixBytesHex(String),
}

/// First known input for a release registration. Reinstallation cannot silently
/// rewrite it; removal followed by registration starts a new local record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WireOrigin")]
pub struct LocalOrigin {
    kind: OriginKind,
    path: OriginPath,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireOrigin {
    kind: OriginKind,
    path: OriginPath,
}
impl TryFrom<WireOrigin> for LocalOrigin {
    type Error = &'static str;
    fn try_from(value: WireOrigin) -> Result<Self, Self::Error> {
        let origin = Self {
            kind: value.kind,
            path: value.path,
        };
        origin.checked_path()?;
        Ok(origin)
    }
}
impl LocalOrigin {
    /// Record a bounded absolute spelling before input acquisition. This performs
    /// no canonicalization/fetch; parent symlinks retain the user's chosen spelling.
    pub(super) fn capture(path: &Path, kind: OriginKind) -> Result<Self, Error> {
        if path.as_os_str().as_bytes().len() > MAX_PATH_BYTES {
            return Err(Error::new(
                Code::LimitExceeded,
                "local origin path exceeds 4096 bytes",
            ));
        }
        let path = std::path::absolute(path)?;
        let raw = path.as_os_str().as_bytes();
        if raw.len() > MAX_PATH_BYTES || raw.contains(&0) {
            return Err(Error::new(
                Code::LimitExceeded,
                "invalid or oversized local origin path",
            ));
        }
        let path = match path.to_str() {
            Some(text) => OriginPath::Utf8(text.into()),
            None => {
                use std::fmt::Write;
                let mut encoded = String::with_capacity(raw.len() * 2);
                for byte in raw {
                    write!(&mut encoded, "{byte:02x}").expect("string write");
                }
                OriginPath::UnixBytesHex(encoded)
            }
        };
        Ok(Self { kind, path })
    }
    /// The recorded acquisition kind. Not permission to execute any code.
    pub fn kind(&self) -> OriginKind {
        self.kind
    }
    /// Decode historical metadata only; callers must not use it for automatic IO.
    pub fn path(&self) -> PathBuf {
        self.checked_path().expect("validated local origin")
    }
    fn checked_path(&self) -> Result<PathBuf, &'static str> {
        let bytes = match &self.path {
            OriginPath::Utf8(text) => {
                if text.len() > MAX_PATH_BYTES {
                    return Err("origin path exceeds 4096 bytes");
                }
                text.as_bytes().to_vec()
            }
            OriginPath::UnixBytesHex(text) => {
                if text.len() > MAX_PATH_BYTES * 2 || !text.len().is_multiple_of(2) {
                    return Err("invalid origin hex length");
                }
                let digit = |v| match v {
                    b'0'..=b'9' => Ok(v - b'0'),
                    b'a'..=b'f' => Ok(v - b'a' + 10),
                    _ => Err("invalid origin hex"),
                };
                let bytes = text
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|p| Ok(digit(p[0])? * 16 + digit(p[1])?))
                    .collect::<Result<Vec<_>, &'static str>>()?;
                if std::str::from_utf8(&bytes).is_ok() {
                    return Err("UTF-8 origin paths require utf8 encoding");
                }
                bytes
            }
        };
        if bytes.first() != Some(&b'/') || bytes.contains(&0) {
            return Err("origin path must be absolute and NUL-free");
        }
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_round_trip_losslessly_and_invalid_metadata_refuses() {
        for path in [
            PathBuf::from("/tmp/a\nfile"),
            PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/a\xff".to_vec())),
        ] {
            let origin = LocalOrigin::capture(&path, OriginKind::LocalBundle).unwrap();
            let json = serde_json::to_string(&origin).unwrap();
            assert!(!json.contains('\n'));
            let decoded: LocalOrigin = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.path(), path);
            assert_eq!(decoded, origin);
        }
        for (encoding, value) in [
            ("utf8", "relative"),
            ("utf8", "/tmp/\0bad"),
            ("unix-bytes-hex", "ff"),
            ("unix-bytes-hex", "2fGG"),
            ("unix-bytes-hex", "2f61"),
            ("unix-bytes-hex", "2fff0"),
        ] {
            let json = serde_json::json!({"kind":"local-bundle", "path":{"encoding":encoding,"value":value}});
            assert!(serde_json::from_value::<LocalOrigin>(json).is_err());
        }
        let json = serde_json::json!({"kind":"local-bundle", "path":{"encoding":"utf8","value":format!("/{}", "a".repeat(4096))}});
        assert!(serde_json::from_value::<LocalOrigin>(json).is_err());
    }
}
