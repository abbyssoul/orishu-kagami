//! Descriptor-relative local IO. No path from a manifest is opened relative to
//! process cwd; every component is walked from an already-open directory without
//! following links. The user-selected root is configuration, not package content.

use super::{Code, Error};
use rustix::fs::{self, AtFlags, FlockOperation, Mode, OFlags};
use std::{
    ffi::OsStr,
    fs::File,
    io::{Read, Write},
    path::{Component, Path},
};

fn os(error: rustix::io::Errno) -> Error {
    std::io::Error::from(error).into()
}
fn invalid() -> Error {
    Error::new(
        Code::Malformed,
        "expected a regular contained file or directory",
    )
}
fn limit() -> Error {
    Error::new(Code::LimitExceeded, "local file byte budget exceeded")
}

fn nonempty_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
/// Read an explicitly selected regular file without following its final symlink.
/// The user-selected parent path is configuration, not untrusted archive content.
pub(crate) fn read_file(path: &Path, max: usize) -> Result<Vec<u8>, Error> {
    Directory::open(nonempty_parent(path))?
        .read(Path::new(path.file_name().ok_or_else(invalid)?), max)
}
/// Publish one complete new file; never overwrite an existing file or symlink.
/// A final directory-sync failure has an uncertain outcome: inspect before retry.
pub(crate) fn create_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(invalid)?;
    Directory::open(nonempty_parent(path))?.create(name, bytes)
}

#[derive(Debug)]
pub(super) struct Directory(File);
impl Directory {
    pub(super) fn open(path: &Path) -> Result<Self, Error> {
        let fd = fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(os)?;
        Ok(Self(File::from(fd)))
    }
    pub(super) fn child(&self, name: &OsStr, create: bool) -> Result<Self, Error> {
        single(name)?;
        if create {
            match fs::mkdirat(&self.0, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                Ok(()) => self.0.sync_all()?,
                Err(rustix::io::Errno::EXIST) => (),
                Err(e) => return Err(os(e)),
            }
        }
        let fd = fs::openat(
            &self.0,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(os)?;
        Ok(Self(File::from(fd)))
    }
    fn parent(&self, path: &Path) -> Result<(Self, std::ffi::OsString), Error> {
        let mut parts = path.components().peekable();
        let mut dir = Self(self.0.try_clone()?);
        while let Some(Component::Normal(name)) = parts.next() {
            single(name)?;
            if parts.peek().is_none() {
                return Ok((dir, name.to_owned()));
            }
            dir = dir.child(name, false)?;
        }
        Err(invalid())
    }
    pub(super) fn read(&self, path: &Path, max: usize) -> Result<Vec<u8>, Error> {
        self.read_optional(path, max)?
            .ok_or_else(|| Error::new(Code::IoFailure, "required local file is absent"))
    }
    pub(super) fn read_optional(&self, path: &Path, max: usize) -> Result<Option<Vec<u8>>, Error> {
        let (dir, name) = self.parent(path)?;
        let fd = match fs::openat(
            &dir.0,
            &name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(os(e)),
        };
        let file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(invalid());
        }
        if metadata.len() > max as u64 {
            return Err(limit());
        }
        // A concurrent writer may grow the file after metadata. Never read beyond
        // max + 1, and don't reserve from an untrusted on-disk length.
        let mut bytes = Vec::new();
        file.take((max as u64).checked_add(1).ok_or_else(limit)?)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(limit());
        }
        Ok(Some(bytes))
    }
    pub(super) fn lock(&self, name: &str, shared: bool) -> Result<File, Error> {
        single(name.as_ref())?;
        let fd = fs::openat(
            &self.0,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(os)?;
        let file = File::from(fd);
        if !file.metadata()?.is_file() {
            return Err(invalid());
        }
        let op = if shared {
            FlockOperation::NonBlockingLockShared
        } else {
            FlockOperation::NonBlockingLockExclusive
        };
        match fs::flock(&file, op) {
            Ok(()) => Ok(file),
            Err(rustix::io::Errno::WOULDBLOCK) => Err(Error::new(
                Code::Busy,
                "local plugin lock is held by another operation",
            )),
            Err(e) => Err(os(e)),
        }
    }
    /// Immutable cache write: verify any existing bytes rather than overwriting.
    /// Callers hold the inventory mutation lock and have verified the new bytes.
    pub(super) fn put(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        if let Some(existing) = self.read_optional(Path::new(name), bytes.len())? {
            if existing != bytes {
                return Err(Error::new(
                    Code::IntegrityMismatch,
                    "content-addressed cache entry differs",
                ));
            }
            return Ok(());
        }
        self.replace(name, bytes)
    }
    /// Flush staged contents, atomically publish on the same filesystem, flush
    /// parent. If the final flush fails the outcome is uncertain: the next reader
    /// must inspect the index, never blindly assume the command was rolled back.
    pub(super) fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        self.publish(name, bytes, true)
    }
    pub(super) fn create(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        self.publish(name, bytes, false)
    }
    fn publish(&self, name: &str, bytes: &[u8], replace: bool) -> Result<(), Error> {
        single(name.as_ref())?;
        let temp = format!(".stage-{}", uuid::Uuid::new_v4());
        let fd = fs::openat(
            &self.0,
            temp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(os)?;
        let mut file = File::from(fd);
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            if replace {
                fs::renameat(&self.0, temp.as_str(), &self.0, name).map_err(os)?;
            } else {
                // linkat publishes atomically without replacing an existing name,
                // even if another process creates it after our caller's check.
                fs::linkat(&self.0, temp.as_str(), &self.0, name, AtFlags::empty()).map_err(
                    |e| {
                        if e == rustix::io::Errno::EXIST {
                            Error::new(Code::InvalidSelection, "output file already exists")
                        } else {
                            os(e)
                        }
                    },
                )?;
                fs::unlinkat(&self.0, temp.as_str(), AtFlags::empty()).map_err(os)?;
            }
            self.0.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.0, temp.as_str(), AtFlags::empty());
        }
        result
    }
}
fn single(name: &OsStr) -> Result<(), Error> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(invalid());
    }
    Ok(())
}
