//! Secure per-worker identity and operator credential persistence.
//! Formation IDs and join tokens are ephemeral and are never restored here.

use serde::{Deserialize, Serialize};
use std::path::Path;
use subtle::ConstantTimeEq;

use crate::peer::tls::{PeerIdentity, TlsError};

const MAX_IDENTITY_BYTES: u64 = 65_536;

/// Fixed-length random bearer credential with redacted diagnostics.
pub struct SecretToken(String);

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}

impl SecretToken {
    /// Generate 256 bits using the TLS provider's operating-system entropy source.
    pub fn generate() -> Result<Self, CredentialError> {
        let mut random = [0; 32];
        rustls::crypto::aws_lc_rs::default_provider()
            .secure_random
            .fill(&mut random)
            .map_err(|_| CredentialError::Entropy)?;
        let mut token = String::with_capacity(64);
        use std::fmt::Write;
        for byte in random {
            write!(&mut token, "{byte:02x}").expect("string formatting");
        }
        Ok(Self(token))
    }

    /// Parse the persisted canonical hexadecimal form without echoing invalid input.
    pub fn parse(value: String) -> Result<Self, CredentialError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(CredentialError::Invalid);
        }
        Ok(Self(value))
    }

    /// Compare a bounded candidate without data-dependent comparison of valid tokens.
    pub fn matches(&self, candidate: &str) -> bool {
        candidate.len() == 64 && bool::from(self.0.as_bytes().ct_eq(candidate.as_bytes()))
    }

    /// Explicit secret access for authenticated transport or private-file persistence.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Errors intentionally omit secret bytes and parsed payloads.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// Filesystem failure, including another process holding the instance lock.
    #[error("worker credential filesystem error: {0}")]
    Io(#[from] std::io::Error),
    /// Unsafe permissions, links or ownership were detected.
    #[error(
        "worker credentials require an owned private directory and single-link private regular files"
    )]
    UnsafePath,
    /// Existing data is invalid; it must not silently be replaced.
    #[error("invalid or incomplete worker credential file")]
    Invalid,
    /// Cryptographic identity validation failed.
    #[error(transparent)]
    Tls(#[from] TlsError),
    /// The entropy provider failed.
    #[error("worker entropy source failed")]
    Entropy,
    /// No private-file implementation is provided for this platform yet.
    #[error("secure worker credential persistence is unsupported on this platform")]
    UnsupportedPlatform,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredIdentity {
    version: u32,
    certificate: Vec<u8>,
    private_key: Vec<u8>,
}

/// Loaded worker identity and operator token, retaining the exclusive instance lock.
pub struct WorkerCredentials {
    /// Stable certificate identity; independent of a process's ephemeral node ID.
    pub identity: PeerIdentity,
    /// Credential for this worker's privileged operator API, never peer admission.
    pub operator: SecretToken,
    _lock: std::fs::File,
}

impl WorkerCredentials {
    /// Load or initialize private credentials. Corrupt existing files are errors,
    /// not permission to regenerate identity. Only one worker may own a directory.
    #[cfg(unix)]
    pub fn load_or_create(path: &Path) -> Result<Self, CredentialError> {
        use rustix::fs::{FlockOperation, Mode, OFlags};
        use std::io::{Read, Write};
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
        let directory = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        let stat = rustix::fs::fstat(&directory).map_err(std::io::Error::from)?;
        if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o077 != 0 {
            return Err(CredentialError::UnsafePath);
        }
        let lock = open_private(&directory, "instance.lock", true)?;
        rustix::fs::flock(&lock, FlockOperation::NonBlockingLockExclusive)
            .map_err(std::io::Error::from)?;

        let identity = match open_private(&directory, "identity.json", false) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_IDENTITY_BYTES + 1).read_to_end(&mut bytes)?;
                if bytes.len() as u64 > MAX_IDENTITY_BYTES {
                    return Err(CredentialError::Invalid);
                }
                let stored: StoredIdentity =
                    serde_json::from_slice(&bytes).map_err(|_| CredentialError::Invalid)?;
                if stored.version != 1 {
                    return Err(CredentialError::Invalid);
                }
                PeerIdentity::from_der(stored.certificate, stored.private_key)?
            }
            Err(CredentialError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                let identity = PeerIdentity::generate()?;
                let stored = StoredIdentity {
                    version: 1,
                    certificate: identity.certificate().to_vec(),
                    private_key: identity.private_key().to_vec(),
                };
                let encoded = serde_json::to_vec(&stored).map_err(|_| CredentialError::Invalid)?;
                let mut file = create_private(&directory, "identity.json")?;
                file.write_all(&encoded)?;
                file.sync_all()?;
                identity
            }
            Err(error) => return Err(error),
        };
        let operator = match open_private(&directory, "operator.token", false) {
            Ok(file) => {
                let mut value = String::new();
                file.take(65).read_to_string(&mut value)?;
                SecretToken::parse(value)?
            }
            Err(CredentialError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                let token = SecretToken::generate()?;
                let mut file = create_private(&directory, "operator.token")?;
                file.write_all(token.expose().as_bytes())?;
                file.sync_all()?;
                token
            }
            Err(error) => return Err(error),
        };
        rustix::fs::fsync(&directory).map_err(std::io::Error::from)?;
        Ok(Self {
            identity,
            operator,
            _lock: lock,
        })
    }

    /// Unsupported platforms fail explicitly rather than storing credentials insecurely.
    #[cfg(not(unix))]
    pub fn load_or_create(_path: &Path) -> Result<Self, CredentialError> {
        Err(CredentialError::UnsupportedPlatform)
    }
}

#[cfg(unix)]
fn checked_file(fd: rustix::fd::OwnedFd) -> Result<std::fs::File, CredentialError> {
    let stat = rustix::fs::fstat(&fd).map_err(std::io::Error::from)?;
    if stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o077 != 0
        || rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile
        || stat.st_nlink != 1
    {
        return Err(CredentialError::UnsafePath);
    }
    Ok(fd.into())
}

#[cfg(unix)]
fn open_private(
    directory: &rustix::fd::OwnedFd,
    name: &str,
    create: bool,
) -> Result<std::fs::File, CredentialError> {
    use rustix::fs::{Mode, OFlags};
    let flags = OFlags::RDONLY
        | OFlags::NOFOLLOW
        | OFlags::CLOEXEC
        | OFlags::NONBLOCK
        | if create {
            OFlags::CREATE
        } else {
            OFlags::empty()
        };
    let fd = rustix::fs::openat(directory, name, flags, Mode::from_raw_mode(0o600))
        .map_err(std::io::Error::from)?;
    checked_file(fd)
}

#[cfg(unix)]
fn create_private(
    directory: &rustix::fd::OwnedFd,
    name: &str,
) -> Result<std::fs::File, CredentialError> {
    use rustix::fs::{Mode, OFlags};
    let fd = rustix::fs::openat(
        directory,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(std::io::Error::from)?;
    checked_file(fd)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn identity_and_operator_survive_restart_with_exclusive_instance_ownership() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker");
        let first = WorkerCredentials::load_or_create(&path).unwrap();
        let fingerprint = first.identity.fingerprint();
        let token = first.operator.expose().to_owned();
        assert!(WorkerCredentials::load_or_create(&path).is_err());
        assert!(!format!("{:?}", first.operator).contains(&token));
        drop(first);
        let second = WorkerCredentials::load_or_create(&path).unwrap();
        assert_eq!(second.identity.fingerprint(), fingerprint);
        assert!(second.operator.matches(&token));
        assert!(!second.operator.matches(&"0".repeat(64)));
        assert_eq!(
            std::fs::metadata(path.join("identity.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn corrupt_existing_identity_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker");
        drop(WorkerCredentials::load_or_create(&path).unwrap());
        std::fs::write(path.join("identity.json"), b"partial write").unwrap();
        assert!(matches!(
            WorkerCredentials::load_or_create(&path),
            Err(CredentialError::Invalid)
        ));
        assert_eq!(
            std::fs::read(path.join("identity.json")).unwrap(),
            b"partial write"
        );
    }

    #[test]
    fn insecure_permissions_and_symlinks_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker");
        drop(WorkerCredentials::load_or_create(&path).unwrap());
        std::fs::set_permissions(
            path.join("operator.token"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(matches!(
            WorkerCredentials::load_or_create(&path),
            Err(CredentialError::UnsafePath)
        ));
        let alias = root.path().join("alias");
        symlink(&path, &alias).unwrap();
        assert!(WorkerCredentials::load_or_create(&alias).is_err());
    }
}
