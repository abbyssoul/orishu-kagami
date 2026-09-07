//! Ownership-aware client socket recovery and cleanup. Never remove a regular
//! file, symlink, active listener or a socket substituted after registration.

use std::{
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

/// Startup refusal without socket contents or credential data.
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    /// Filesystem or connect check failed.
    #[error("Unix socket setup failed: {0}")]
    Io(#[from] std::io::Error),
    /// The path or its immediate parent is not safely owned.
    #[error("Unix socket requires an owned non-writable-by-others parent and an owned socket path")]
    Unsafe,
    /// A listener is active, or liveness could not be disproved within the bound.
    #[error("Unix socket is active or its liveness check timed out")]
    Active,
}

/// Cleanup lease acquired only after the caller successfully binds the path.
pub struct SocketLease {
    path: PathBuf,
    device: u64,
    inode: u64,
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn check_parent(path: &Path) -> Result<(), SocketError> {
    let metadata = std::fs::symlink_metadata(parent(path))?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(SocketError::Unsafe);
    }
    Ok(())
}

fn socket_metadata(path: &Path) -> Result<std::fs::Metadata, SocketError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
    {
        return Err(SocketError::Unsafe);
    }
    Ok(metadata)
}

/// Prepare one explicit listener path. Only an owned socket with a refused
/// connection is stale; every other result fails closed. Recovery is bounded
/// and rechecks the socket inode before removing its directory entry.
pub async fn prepare(path: &Path) -> Result<(), SocketError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(parent(path))?;
    check_parent(path)?;
    let initial = match socket_metadata(path) {
        Ok(metadata) => metadata,
        Err(SocketError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    match tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::UnixStream::connect(path),
    )
    .await
    {
        Err(_) | Ok(Ok(_)) => return Err(SocketError::Active),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {}
        Ok(Err(error)) => return Err(error.into()),
    }
    check_parent(path)?;
    let current = socket_metadata(path)?;
    if initial.dev() != current.dev() || initial.ino() != current.ino() {
        return Err(SocketError::Unsafe);
    }
    std::fs::remove_file(path)?;
    Ok(())
}

impl SocketLease {
    /// Register the inode immediately after a successful bind. The lease must
    /// outlive the listener, and its path must not be reused for another listener.
    pub fn bound(path: &Path) -> Result<Self, SocketError> {
        check_parent(path)?;
        let metadata = socket_metadata(path)?;
        Ok(Self {
            path: path.to_owned(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

impl Drop for SocketLease {
    fn drop(&mut self) {
        if check_parent(&self.path).is_ok()
            && let Ok(metadata) = socket_metadata(&self.path)
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixListener,
    };

    fn private_directory() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    #[tokio::test]
    async fn protects_active_listeners_and_recovers_only_stale_sockets() {
        let root = private_directory();
        let path = root.path().join("worker.sock");
        let listener = UnixListener::bind(&path).unwrap();
        assert!(matches!(prepare(&path).await, Err(SocketError::Active)));
        assert!(path.exists());
        drop(listener);
        prepare(&path).await.unwrap();
        assert!(!path.exists());
        let listener = UnixListener::bind(&path).unwrap();
        let lease = SocketLease::bound(&path).unwrap();
        drop(listener);
        drop(lease);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn regular_files_symlinks_and_replacements_are_not_deleted() {
        let root = private_directory();
        let path = root.path().join("worker.sock");
        std::fs::write(&path, b"user data").unwrap();
        assert!(matches!(prepare(&path).await, Err(SocketError::Unsafe)));
        assert_eq!(std::fs::read(&path).unwrap(), b"user data");
        let link = root.path().join("link.sock");
        symlink(&path, &link).unwrap();
        assert!(matches!(prepare(&link).await, Err(SocketError::Unsafe)));
        let original = root.path().join("original.sock");
        let listener = UnixListener::bind(&original).unwrap();
        let lease = SocketLease::bound(&original).unwrap();
        std::fs::rename(&original, root.path().join("moved.sock")).unwrap();
        std::fs::write(&original, b"replacement").unwrap();
        drop(listener);
        drop(lease);
        assert_eq!(std::fs::read(&original).unwrap(), b"replacement");
    }

    #[tokio::test]
    async fn unsafe_parent_is_rejected_without_removing_socket() {
        let root = private_directory();
        let path = root.path().join("worker.sock");
        let listener = UnixListener::bind(&path).unwrap();
        drop(listener);
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o770)).unwrap();
        assert!(matches!(prepare(&path).await, Err(SocketError::Unsafe)));
        assert!(path.exists());
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        prepare(&path).await.unwrap();
        assert!(!path.exists());
    }
}
