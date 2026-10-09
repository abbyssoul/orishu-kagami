//! Unix storage for the run-intent [`Ledger`] and the frozen workload bytes.
//!
//! One directory, by default `$XDG_STATE_HOME/kagami/runs`, holds:
//! - `intents.lock`: an exclusive advisory lock for the journal lifetime;
//! - `intents.json`: the ledger snapshot, replaced atomically;
//! - `bundles/<root>.okw`: the frozen bytes of the recorded upload.
//!
//! Every write is staged, flushed and renamed. Call these methods off the
//! window thread: a write waits for storage, and a bundle can be 128 MiB.
use super::intents::{DecodeError, IntentError, Ledger, MAX_LEDGER_BYTES, Source, Target};
use crate::files::{self, Directory};
use orishu::model::{run_command::*, run_load::*};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    path::{Path, PathBuf},
};

const LOCK: &str = "intents.lock";
const LEDGER: &str = "intents.json";
const BUNDLES: &str = "bundles";
/// Entries examined during cleanup. The journal itself needs only a few.
const MAX_ENTRIES: usize = 1024;

/// Why the journal cannot record or provide intent. Holds no path. The text is
/// suitable for a window notice or a disabled-action explanation.
#[derive(Debug)]
pub enum JournalError {
    /// Neither `KAGAMI_RUN_STATE_DIR`, `XDG_STATE_HOME` nor `HOME` is set.
    NoLocation,
    /// Another Kagami process holds the journal.
    InUse,
    /// The storage is full or over quota. Nothing new became visible.
    NoSpace,
    /// The storage is read-only or not writable by this user.
    ReadOnly,
    /// Another local storage failure.
    Io(StorageFailure),
    /// The recorded ledger cannot be used. It stays unchanged on disk.
    Damaged(DecodeError),
    /// The ledger refused the transition. Nothing was written.
    Refused(IntentError),
    /// An earlier ledger write failed after its result could be visible. The
    /// state on disk is unknown until the journal is opened again.
    Poisoned,
    /// The stored workload bytes are absent, damaged or for another workload.
    BundleUnavailable,
}
impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLocation => f.write_str(
                "No directory for run recovery records. Set XDG_STATE_HOME or KAGAMI_RUN_STATE_DIR.",
            ),
            Self::InUse => f.write_str(
                "Another Kagami instance records run submissions and commands. Use that instance, or close it and restart this one.",
            ),
            Self::NoSpace => f.write_str(
                "The storage for run recovery records is full. Free space, then try again.",
            ),
            Self::ReadOnly => f.write_str(
                "Run recovery records cannot be written: the storage is read-only or not writable by this user.",
            ),
            Self::Io(_) => f.write_str("Run recovery storage failed."),
            Self::Damaged(error) => write!(
                f,
                "{error} Kagami does not overwrite it. Move the file away only after you have reconciled its operations."
            ),
            Self::Refused(error) => error.fmt(f),
            Self::Poisoned => f.write_str(
                "A run recovery record write did not complete. Restart Kagami before you submit or control runs.",
            ),
            Self::BundleUnavailable => f.write_str(
                "The stored workload bytes are missing or damaged. Reconcile the submission; it cannot be sent again.",
            ),
        }
    }
}
impl std::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Damaged(error) => Some(error),
            Self::Refused(error) => Some(error),
            _ => None,
        }
    }
}
impl From<files::Error> for JournalError {
    fn from(error: files::Error) -> Self {
        use std::io::ErrorKind as K;
        let io = match &error {
            files::Error::Io(io) => Some(io),
            files::Error::Uncertain(inner) => match inner.as_ref() {
                files::Error::Io(io) => Some(io),
                _ => None,
            },
            files::Error::Busy => return Self::InUse,
            _ => None,
        };
        match io.map(std::io::Error::kind) {
            Some(K::StorageFull | K::QuotaExceeded) => Self::NoSpace,
            Some(K::ReadOnlyFilesystem | K::PermissionDenied) => Self::ReadOnly,
            _ => Self::Io(StorageFailure(error)),
        }
    }
}

/// Local storage failure without a recognized cause. Its source chain holds
/// the OS error, never a path.
#[derive(Debug)]
pub struct StorageFailure(files::Error);
impl std::fmt::Display for StorageFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for StorageFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// Explicit override, then the XDG state directory, then `~/.local/state`.
pub fn default_directory() -> Result<PathBuf, JournalError> {
    let var = |name| std::env::var_os(name).filter(|v| !v.is_empty());
    if let Some(path) = var("KAGAMI_RUN_STATE_DIR") {
        return Ok(PathBuf::from(path));
    }
    let state = match var("XDG_STATE_HOME") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(var("HOME").ok_or(JournalError::NoLocation)?).join(".local/state"),
    };
    Ok(state.join("kagami/runs"))
}

/// Exclusive owner of the durable run intents of this Kagami instance.
#[derive(Debug)]
pub struct Journal {
    dir: Directory,
    bundles: Directory,
    _lock: File,
    ledger: Ledger,
    poisoned: bool,
}
impl Journal {
    /// Open or create the journal, and remove staging files and bundles that no
    /// record needs. A damaged or newer ledger is refused and kept unchanged.
    ///
    /// Submissions and run commands need this journal, so Kagami without
    /// writable storage can observe runs but not change them. A future mode
    /// for read-only media would add an explicit volatile journal here, with a
    /// clear warning that a restart loses recovery; the run controller would
    /// then accept that mode in place of this error.
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|e| JournalError::from(files::Error::Io(e)))?;
        let dir = Directory::open(path)?;
        let lock = dir.lock(LOCK, false)?;
        let ledger = match dir.read_optional(Path::new(LEDGER), MAX_LEDGER_BYTES) {
            Ok(None) => Ledger::default(),
            Ok(Some(bytes)) => Ledger::decode(&bytes).map_err(JournalError::Damaged)?,
            Err(files::Error::LimitExceeded) => {
                return Err(JournalError::Damaged(DecodeError::Oversized));
            }
            Err(files::Error::NotContained) => {
                return Err(JournalError::Damaged(DecodeError::Malformed));
            }
            Err(error) => return Err(error.into()),
        };
        let bundles = dir.child(OsStr::new(BUNDLES), true)?;
        // The lock is held, so no other writer can own these files.
        dir.remove_staging(MAX_ENTRIES)?;
        bundles.remove_staging(MAX_ENTRIES)?;
        let needed = needed_bundle(&ledger);
        for name in bundles.names(MAX_ENTRIES)? {
            if Some(&name) != needed.as_ref() {
                bundles.remove(&name)?;
            }
        }
        Ok(Self {
            dir,
            bundles,
            _lock: lock,
            ledger,
            poisoned: false,
        })
    }

    /// The durable state. After [`JournalError::Poisoned`], this is the last
    /// state known to be durable, not necessarily the state on disk.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Store the frozen bytes, then record the upload. The caller sends nothing
    /// unless this succeeds. The caller supplies bytes of `request`'s workload.
    pub fn begin_load(
        &mut self,
        target: Target,
        request: LoadRequest,
        source: Source,
        bundle: &[u8],
    ) -> Result<(), JournalError> {
        self.usable()?;
        let next = self
            .ledger
            .begin_load(target, request, source)
            .map_err(JournalError::Refused)?;
        let name = needed_bundle(&next).expect("a new upload is unresolved");
        // No record references this name, so a leftover from an earlier upload
        // of the same workload may be replaced.
        self.bundles
            .replace(name.to_str().expect("ASCII bundle name"), bundle)?;
        self.commit(next)
    }

    /// Record a submit or lookup reply. A final outcome releases the bytes.
    pub fn record_load(&mut self, receipt: Option<LoadReceipt>) -> Result<(), JournalError> {
        self.usable()?;
        let next = self
            .ledger
            .record_load(receipt)
            .map_err(JournalError::Refused)?;
        self.commit(next)?;
        self.release_bundle();
        Ok(())
    }

    /// Forget a final submission. Worker state does not change.
    pub fn clear_load(&mut self) -> Result<(), JournalError> {
        self.usable()?;
        let next = self.ledger.clear_load().map_err(JournalError::Refused)?;
        self.commit(next)?;
        self.release_bundle();
        Ok(())
    }

    /// Record a run command. The caller sends nothing unless this succeeds.
    pub fn begin_command(
        &mut self,
        target: Target,
        request: RunCommandRequest,
    ) -> Result<(), JournalError> {
        self.usable()?;
        let next = self
            .ledger
            .begin_command(target, request)
            .map_err(JournalError::Refused)?;
        self.commit(next)
    }

    /// Record a command or lookup reply.
    pub fn record_command(
        &mut self,
        receipt: Option<RunCommandReceipt>,
    ) -> Result<(), JournalError> {
        self.usable()?;
        let next = self
            .ledger
            .record_command(receipt)
            .map_err(JournalError::Refused)?;
        self.commit(next)
    }

    /// The exact stored bytes of the unresolved upload, for an explicit resend.
    /// Check them with [`verify_bundle`] before sending.
    pub fn bundle(&self) -> Result<Vec<u8>, JournalError> {
        let name = needed_bundle(&self.ledger).ok_or(JournalError::BundleUnavailable)?;
        self.bundles
            .read(Path::new(&name), crate::workload_preparation::BUNDLE_BYTES)
            .map_err(|_| JournalError::BundleUnavailable)
    }

    fn usable(&self) -> Result<(), JournalError> {
        if self.poisoned {
            return Err(JournalError::Poisoned);
        }
        Ok(())
    }

    /// Write-ahead: adopt `next` only when its bytes are durable. A failure
    /// before the new snapshot became visible leaves the old one authoritative;
    /// a later failure makes the state on disk unknown.
    fn commit(&mut self, next: Ledger) -> Result<(), JournalError> {
        let bytes = next.encode().map_err(JournalError::Refused)?;
        match self.dir.replace(LEDGER, &bytes) {
            Ok(()) => {
                self.ledger = next;
                Ok(())
            }
            Err(error) => {
                if matches!(error, files::Error::Uncertain(_)) {
                    self.poisoned = true;
                }
                Err(error.into())
            }
        }
    }

    /// Best effort: a leftover file is removed at the next open.
    fn release_bundle(&self) {
        let needed = needed_bundle(&self.ledger);
        if let Ok(names) = self.bundles.names(MAX_ENTRIES) {
            for name in names {
                if Some(&name) != needed.as_ref() {
                    let _ = self.bundles.remove(&name);
                }
            }
        }
    }
}

/// Only an unresolved upload can be sent again, so only it keeps its bytes.
fn needed_bundle(ledger: &Ledger) -> Option<OsString> {
    let intent = ledger.load().filter(|intent| !intent.is_final())?;
    Some(
        format!(
            "{}.okw",
            intent.request().workload_id().to_string().replace(':', "-")
        )
        .into(),
    )
}

/// Full bounded closure verification against the requested workload root,
/// the same check that headless submission applies to a file.
pub fn verify_bundle(
    bytes: &[u8],
    expected: orishu_workload::WorkloadDigest,
) -> Result<(), JournalError> {
    let bundle =
        orishu_plugin::workload::bundle::read(bytes, Default::default(), Default::default())
            .map_err(|_| JournalError::BundleUnavailable)?;
    if bundle.verified().root() != expected {
        return Err(JournalError::BundleUnavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
