//! Explicit worker-local operator credential input. Never discovers credentials
//! from an endpoint or accepts raw secret values in CLI/environment options.

use orishu::client::Credentials;
use std::path::Path;

/// Errors intentionally omit both file contents and paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    #[cfg(unix)]
    Unreadable,
    #[cfg(unix)]
    Unsafe,
    #[cfg(unix)]
    Invalid,
    #[cfg(not(unix))]
    Unsupported,
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            #[cfg(unix)]
            Self::Unreadable => "cannot read operator token file",
            #[cfg(unix)]
            Self::Unsafe => "operator token must be a private, singly linked regular file owned by the current user",
            #[cfg(unix)]
            Self::Invalid => "operator token file must contain exactly 64 lowercase hexadecimal bytes",
            #[cfg(not(unix))]
            Self::Unsupported => "secure operator token file loading is unsupported on this platform",
        })
    }
}

/// Open without following the final symlink or blocking on a FIFO, then validate
/// the opened descriptor before reading at most 65 bytes. Accept precisely the
/// worker's persisted format; do not trim malformed input into a valid token.
#[cfg(unix)]
pub fn load(path: &Path) -> Result<Credentials, CredentialError> {
    let bytes = read_private(path, 65)?;
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(CredentialError::Invalid);
    }
    Ok(Credentials::Token(
        String::from_utf8(bytes).expect("validated ASCII"),
    ))
}

#[cfg(unix)]
fn read_private(path: &Path, limit: u64) -> Result<Vec<u8>, CredentialError> {
    use rustix::fs::{FileType, Mode, OFlags};
    use std::io::Read;

    let fd = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| CredentialError::Unreadable)?;
    let stat = rustix::fs::fstat(&fd).map_err(|_| CredentialError::Unreadable)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o077 != 0
        || stat.st_nlink != 1
    {
        return Err(CredentialError::Unsafe);
    }
    let file = std::fs::File::from(fd);
    let mut bytes = Vec::with_capacity(limit as usize);
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| CredentialError::Unreadable)?;
    Ok(bytes)
}

/// Load explicit JSON bootstrap material without leaking parser input in errors.
/// Uses the same descriptor ownership/link/permission checks as operator input.
#[cfg(unix)]
pub fn load_join_material(
    path: &Path,
) -> Result<orishu::model::cluster::JoinMaterial, &'static str> {
    let bytes = read_private(path, 16 * 1024 + 1)
        .map_err(|_| "join material requires a readable private singly linked regular file")?;
    if bytes.len() > 16 * 1024 {
        return Err("join material exceeds 16 KiB");
    }
    let material: orishu::model::cluster::JoinMaterial =
        serde_json::from_slice(&bytes).map_err(|_| "invalid versioned JSON join material")?;
    material.validate()?;
    Ok(material)
}

#[cfg(not(unix))]
pub fn load_join_material(
    _path: &Path,
) -> Result<orishu::model::cluster::JoinMaterial, &'static str> {
    Err("secure join material loading is unsupported on this platform")
}

#[cfg(not(unix))]
pub fn load(_path: &Path) -> Result<Credentials, CredentialError> {
    Err(CredentialError::Unsupported)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};

    fn private(path: &Path, contents: &[u8]) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(contents).unwrap();
    }

    #[test]
    fn join_material_is_private_bounded_and_keeps_target_binding() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("join.json");
        let valid = serde_json::json!({
            "schemaVersion": 1, "formationId": "target", "introducerNodeId": "introducer",
            "introducerFingerprint": "00".repeat(32), "peerEndpoints": ["127.0.0.1:6655"],
            "introducerReady": false, "token": "a".repeat(64)
        });
        private(&path, &serde_json::to_vec(&valid).unwrap());
        let material = load_join_material(&path).unwrap();
        assert_eq!(material.formation_id.as_str(), "target");
        assert_eq!(material.introducer_fingerprint.to_hex(), "00".repeat(32));
        assert!(!format!("{material:?}").contains(&"a".repeat(64)));
        for endpoint in [
            "0.0.0.0:6655",
            "127.0.0.1:0",
            "224.0.0.1:6655",
            "untrusted.example:6655",
        ] {
            let mut bad = valid.clone();
            bad["peerEndpoints"] = serde_json::json!([endpoint]);
            std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(load_join_material(&path).is_err());
        }
        std::fs::write(&path, vec![b' '; 16385]).unwrap();
        assert!(load_join_material(&path).is_err());
        std::fs::write(&path, serde_json::to_vec(&valid).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_join_material(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = directory.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(load_join_material(&link).is_err());
    }

    #[test]
    fn exact_worker_format_and_bounded_invalid_input() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("operator.token");
        private(&path, "a".repeat(64).as_bytes());
        assert!(matches!(load(&path), Ok(Credentials::Token(token)) if token == "a".repeat(64)));
        for invalid in [
            vec![],
            vec![b'a'; 63],
            vec![b'A'; 64],
            vec![0xff; 64],
            vec![b'a'; 65],
            vec![b'a'; 8192],
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(matches!(load(&path), Err(CredentialError::Invalid)));
        }
    }

    #[test]
    fn rejects_links_nonfiles_and_public_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("operator.token");
        private(&path, "a".repeat(64).as_bytes());
        let link = directory.path().join("symbolic");
        symlink(&path, &link).unwrap();
        assert!(load(&link).is_err());
        assert!(matches!(
            load(directory.path()),
            Err(CredentialError::Unsafe)
        ));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(load(&path), Err(CredentialError::Unsafe)));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::hard_link(&path, directory.path().join("hard")).unwrap();
        assert!(matches!(load(&path), Err(CredentialError::Unsafe)));
        let fifo = directory.path().join("fifo");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
            0,
        )
        .unwrap();
        assert!(matches!(load(&fifo), Err(CredentialError::Unsafe)));
    }
}
