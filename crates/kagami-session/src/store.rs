//! Getting a document onto disk without ever costing the previous one.
//!
//! The only module in this crate that touches a filesystem, and it touches one
//! through a trait so the failure paths are testable. Everything above it
//! decides; this writes bytes.
//!
//! # Why a save is five steps and not one
//!
//! Writing over the primary file is a window in which a crash, a full disk or
//! a killed process leaves a half-written document and no previous one. So a
//! save never writes the primary:
//!
//! 1. write the complete bytes to a *sibling* temporary, created exclusively
//!    so an existing file or symlink is never followed;
//! 2. verify those bytes decode as a document — bytes that cannot be read back
//!    are not a save;
//! 3. flush the temporary to the device, because a rename that lands before
//!    its contents do is worse than no rename at all;
//! 4. if the *existing* primary independently decodes, move it to the backup —
//!    the backup is a copy of the last **verified** document, not of whatever
//!    bytes happened to be there;
//! 5. rename the temporary onto the primary, then flush the directory.
//!
//! Interrupted anywhere before step 5, the primary is untouched. Interrupted
//! at step 5, the platform's rename either happened or did not.
//!
//! # What "atomic" means here
//!
//! [`FileStore::rename`] is `std::fs::rename`, which on Unix replaces the
//! destination atomically within a filesystem. Both paths are siblings, so
//! they are on the same filesystem by construction. This module does not claim
//! the guarantee on platforms that do not provide it; it documents what it
//! relies on and confines the reliance to one call.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::document::{DocumentError, ExperimentDocument, decode_document};
use crate::persist::DocumentTarget;

/// Largest document this build will read into memory.
///
/// Checked before reading, not after: the point is to not allocate a
/// gigabyte because something else wrote a gigabyte.
pub const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// How many sibling temporary names a save will try.
///
/// A fixed `.tmp` could be a stale file, a directory, or a symlink someone
/// left behind, and recovering one safely means bounding all of that. Trying a
/// few uniquely-numbered candidates exclusively is simpler and needs no
/// such rules.
pub const MAX_TEMP_CANDIDATES: usize = 8;

/// The filesystem operations a durable save needs.
///
/// A trait so every step's failure is reachable in a test: creating the
/// temporary, writing it, flushing it, moving the backup, replacing the
/// primary, and flushing the directory each fail on real systems, and a
/// protocol that is only exercised on the happy path is a protocol nobody has
/// checked.
pub trait FileStore {
    /// Create `path` and write `bytes`, failing if it already exists.
    ///
    /// Exclusive creation is what stops a save following a symlink or
    /// clobbering a file someone else is using.
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Flush a file's contents to the device.
    fn sync_file(&self, path: &Path) -> io::Result<()>;

    /// Flush a directory entry, where the platform supports it.
    fn sync_dir(&self, path: &Path) -> io::Result<()>;

    /// Rename `from` onto `to`, replacing it.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Remove `path`, if it exists.
    fn remove(&self, path: &Path) -> io::Result<()>;

    /// Read `path`, refusing anything larger than [`MAX_DOCUMENT_BYTES`].
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Whether `path` names something.
    fn exists(&self, path: &Path) -> bool;
}

/// The real filesystem.
pub struct RealFileStore;

impl FileStore for RealFileStore {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create_new(path)?;
        file.write_all(bytes)?;
        file.flush()
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        std::fs::File::open(path)?.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        // Opening a directory and syncing it is how a rename is made durable
        // on Unix. Platforms that refuse to open one report an error the
        // caller can ignore, which is why this is a separate step.
        match std::fs::File::open(path) {
            Ok(directory) => directory.sync_all(),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        match std::fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let length = std::fs::metadata(path)?.len();
        if length > MAX_DOCUMENT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("document is {length} bytes, over the {MAX_DOCUMENT_BYTES}-byte limit"),
            ));
        }
        std::fs::read(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// Why a document could not be written.
#[derive(Debug, Error)]
pub enum SaveError {
    /// A filesystem operation failed, naming which step.
    #[error("could not {step} while saving: {source}")]
    Io {
        /// What was being attempted.
        step: &'static str,
        /// The underlying failure.
        #[source]
        source: io::Error,
    },
    /// The bytes about to be written could not be read back as a document.
    ///
    /// A save whose result cannot be loaded is not a save, and finding out
    /// before the primary is replaced is the whole point of writing to a
    /// temporary first.
    #[error("the encoded document does not decode: {source}")]
    NotReadable {
        /// Why it could not be read back.
        #[source]
        source: DocumentError,
    },
    /// Every sibling temporary name was taken.
    #[error("could not create a temporary file beside the document after {tried} attempts")]
    NoTemporary {
        /// How many candidates were tried.
        tried: usize,
    },
}

/// Which file a load actually came from.
///
/// Reported rather than hidden: reading anything but the primary means
/// something went wrong earlier, and the user is the one who can decide what
/// to do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadedFrom {
    /// The document itself.
    Primary,
    /// The backup, because the primary was missing or unreadable.
    Backup,
    /// A leftover temporary, because neither the primary nor the backup could
    /// be read. The last save was interrupted.
    Temporary,
}

impl LoadedFrom {
    /// `true` when this load should be reported to the user as a recovery.
    pub const fn is_recovery(&self) -> bool {
        !matches!(self, Self::Primary)
    }
}

impl fmt::Display for LoadedFrom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primary => formatter.write_str("the document"),
            Self::Backup => formatter.write_str("the backup"),
            Self::Temporary => formatter.write_str("an interrupted save"),
        }
    }
}

/// Why a document could not be read.
#[derive(Debug, Error)]
pub enum LoadError {
    /// Nothing readable was found at the target, its backup, or any leftover
    /// temporary.
    #[error("no readable document at {target}")]
    Unreadable {
        /// What was being opened.
        target: DocumentTarget,
    },
    /// The document was read but refused, and nothing could stand in for it.
    ///
    /// Kept distinct from [`Self::Unreadable`] so a document written by a
    /// newer build says so. "No readable document here" and "this is format
    /// version 3 and I read 2" are different problems for whoever is holding
    /// the file, and only the second one tells them what to do about it.
    #[error("cannot read {target}: {source}")]
    Refused {
        /// What was being opened.
        target: DocumentTarget,
        /// Why the document itself was refused.
        ///
        /// Boxed because a [`DocumentError`] can carry a whole model
        /// [`Rejection`](kagami_document::Rejection), and every `load` result —
        /// including the successful ones — would otherwise be sized for it.
        #[source]
        source: Box<DocumentError>,
    },
}

impl LoadError {
    /// A stable identifier for this reason.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreadable { .. } => "unreadable_document",
            Self::Refused { source, .. } => source.code(),
        }
    }
}

/// A document read back, and where it had to be found.
#[derive(Debug)]
pub struct Loaded {
    /// The decoded document.
    pub document: ExperimentDocument,
    /// Which file it came from.
    pub from: LoadedFrom,
}

/// The backup path for `target`.
fn backup_of(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".bak");
    PathBuf::from(name)
}

/// The `index`-th sibling temporary candidate for `target`.
fn temporary_of(target: &Path, index: usize) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(format!(".tmp{index}"));
    PathBuf::from(name)
}

/// Write `document` to `target` durably.
///
/// On success the primary holds the new document and the backup holds the
/// previous *verified* one. On any failure the primary is exactly as it was.
///
/// # Errors
///
/// Returns [`SaveError`] naming the step that failed.
pub fn save(
    store: &dyn FileStore,
    target: &DocumentTarget,
    document: &ExperimentDocument,
) -> Result<(), SaveError> {
    let bytes = serde_json::to_vec_pretty(document).map_err(|error| SaveError::Io {
        step: "encode the document",
        source: io::Error::other(error),
    })?;

    // Step 2, before anything is moved: bytes that cannot be read back are
    // not a document, whatever produced them. Verified through the same
    // version-checked entry point a load uses, so a save cannot produce
    // something only a laxer decoder would accept.
    decode_document(&bytes).map_err(|source| SaveError::NotReadable { source })?;

    let path = target.path();
    let (temporary, ()) = (0..MAX_TEMP_CANDIDATES)
        .map(|index| temporary_of(path, index))
        .find_map(|candidate| {
            match store.write_new(&candidate, &bytes) {
                Ok(()) => Some(Ok((candidate, ()))),
                // Taken by a stale file or a concurrent save; try the next.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(source) => Some(Err(SaveError::Io {
                    step: "create a temporary file",
                    source,
                })),
            }
        })
        .transpose()?
        .ok_or(SaveError::NoTemporary {
            tried: MAX_TEMP_CANDIDATES,
        })?;

    let outcome = finish_save(store, path, &temporary);
    if outcome.is_err() {
        // The primary was never touched, so the only thing to undo is the
        // temporary. A failure removing it is not worth reporting over the
        // failure that caused it.
        let _ = store.remove(&temporary);
    }
    outcome
}

fn finish_save(store: &dyn FileStore, path: &Path, temporary: &Path) -> Result<(), SaveError> {
    store.sync_file(temporary).map_err(|source| SaveError::Io {
        step: "flush the temporary file",
        source,
    })?;

    // Step 4: retain a backup of the previous document unless it was damaged.
    // Truncated or garbled bytes are not worth keeping as a recovery target —
    // they would replace a backup that might still be good. But a document
    // this build merely *declines*, such as one written by a newer version, is
    // intact and must survive being replaced; dropping it here is the same
    // data loss the load path refuses to walk into.
    if store.exists(path) && worth_preserving(store, path) {
        store
            .rename(path, &backup_of(path))
            .map_err(|source| SaveError::Io {
                step: "move the previous document to the backup",
                source,
            })?;
    }

    store
        .rename(temporary, path)
        .map_err(|source| SaveError::Io {
            step: "replace the document",
            source,
        })?;

    if let Some(directory) = path.parent() {
        store.sync_dir(directory).map_err(|source| SaveError::Io {
            step: "flush the directory",
            source,
        })?;
    }
    Ok(())
}

/// Read the document at `target`, falling back to what survived.
///
/// Tries the primary, then the backup, then any leftover temporary, decoding
/// each independently. The result says which one answered so a caller can warn
/// rather than silently present recovered content as the document.
///
/// # Errors
///
/// Returns [`LoadError::Refused`] when the primary is intact but this build
/// will not interpret it, and [`LoadError::Unreadable`] when nothing readable
/// was found anywhere.
///
/// # Recovery is for damage only
///
/// A primary this build *declines* — a newer format version, or another format
/// entirely — stops the load right there, before any backup is considered.
/// Opening an older backup instead would look like a successful recovery, and
/// the next save would then replace the intact newer document with it. See
/// [`DocumentError::is_damage`].
pub fn load(store: &dyn FileStore, target: &DocumentTarget) -> Result<Loaded, LoadError> {
    let path = target.path();
    // Why the primary itself failed, kept for the failure message. A usable
    // backup still wins over explaining damage — recovering is better than
    // explaining — but if nothing else answers, this is the reason worth
    // reporting.
    let mut damage = None;

    match try_decode(store, path) {
        Some(Ok(document)) => {
            return Ok(Loaded {
                document,
                from: LoadedFrom::Primary,
            });
        }
        Some(Err(error)) if !error.is_damage() => {
            // Intact, and not ours to work around. Refusing here is what stops
            // a later save from overwriting it with something older.
            return Err(LoadError::Refused {
                target: target.clone(),
                source: Box::new(error),
            });
        }
        Some(Err(error)) => damage = Some(error),
        None => {}
    }

    // The primary was damaged or absent, so recovery is what is left: the
    // backup first, then any temporary an interrupted save left behind.
    if let Some(Ok(document)) = try_decode(store, &backup_of(path)) {
        return Ok(Loaded {
            document,
            from: LoadedFrom::Backup,
        });
    }
    for index in 0..MAX_TEMP_CANDIDATES {
        if let Some(Ok(document)) = try_decode(store, &temporary_of(path, index)) {
            return Ok(Loaded {
                document,
                from: LoadedFrom::Temporary,
            });
        }
    }

    Err(match damage {
        Some(source) => LoadError::Refused {
            target: target.clone(),
            source: Box::new(source),
        },
        None => LoadError::Unreadable {
            target: target.clone(),
        },
    })
}

/// Read and decode one candidate.
///
/// `None` means the candidate produced no bytes — it is missing, or too large,
/// or unreadable. `Some(Err(_))` means it produced bytes this build will not
/// interpret, and says why. The two are distinguished because a *missing*
/// primary and a primary written by a newer build call for different messages.
///
/// Goes through [`decode_document`], so the format identifier and the version
/// are checked before the body is interpreted.
fn try_decode(
    store: &dyn FileStore,
    path: &Path,
) -> Option<Result<ExperimentDocument, DocumentError>> {
    let bytes = store.read(path).ok()?;
    Some(decode_document(&bytes))
}

/// Whether the document at `path` should survive being replaced.
///
/// The question the save protocol asks, and it is not "does this build read
/// it". A document from a newer build is intact and irreplaceable by anything
/// here, so it is preserved; only damage is dropped.
fn worth_preserving(store: &dyn FileStore, path: &Path) -> bool {
    match try_decode(store, path) {
        Some(Ok(_)) => true,
        Some(Err(error)) => !error.is_damage(),
        // No bytes at all: nothing to preserve.
        None => false,
    }
}
