//! Durable intent recording available to the run controller.
//!
//! Without a usable journal, Kagami can observe runs but cannot submit a
//! workload or send a run command: an unrecorded operation could not be
//! reconciled after a restart. The journal IO runs on the controller's job
//! thread, never on the window thread.
use super::intents::{Ledger, Source, Target};
use orishu::model::{run_command::*, run_load::*};

/// Run one journal operation on Unix. Elsewhere, refuse with the reason and
/// consume the listed arguments.
macro_rules! journal {
    ($self:ident, |$j:ident| $body:expr, ($($argument:expr),*)) => {{
        #[cfg(unix)]
        {
            $self.with(|$j| $body)
        }
        #[cfg(not(unix))]
        {
            $(let _ = $argument;)*
            $self.refuse()
        }
    }};
}

/// Text shown when the platform has no journal implementation.
#[cfg(not(unix))]
const UNSUPPORTED: &str = "Run recovery records are not implemented on this platform.";

/// Journal handle or the reason why submissions and commands are disabled.
#[derive(Clone, Debug)]
pub enum Recovery {
    #[cfg(unix)]
    Journal(std::sync::Arc<std::sync::Mutex<super::journal::Journal>>),
    /// Shown on every disabled submission and command action.
    Unavailable(String),
}
impl Default for Recovery {
    fn default() -> Self {
        Self::Unavailable("Run recovery records are not open. Start Kagami with --operator-token-file to enable them.".into())
    }
}
impl Recovery {
    /// Open the journal at `path`. A failure becomes an explanation, not an
    /// exit, so that observation stays available.
    #[cfg(unix)]
    pub fn open(path: &std::path::Path) -> Self {
        match super::journal::Journal::open(path) {
            Ok(journal) => Self::Journal(std::sync::Arc::new(std::sync::Mutex::new(journal))),
            Err(error) => Self::Unavailable(error.to_string()),
        }
    }
    /// Open the journal at its configured or default location.
    pub fn open_default() -> Self {
        #[cfg(unix)]
        {
            match super::journal::default_directory() {
                Ok(path) => Self::open(&path),
                Err(error) => Self::Unavailable(error.to_string()),
            }
        }
        #[cfg(not(unix))]
        {
            Self::Unavailable(UNSUPPORTED.into())
        }
    }

    /// The durable ledger and, if writes are no longer possible, the reason.
    pub(super) fn snapshot(&self) -> (Ledger, Option<String>) {
        let snapshot = journal!(
            self,
            |j| Ok((
                j.ledger().clone(),
                j.is_poisoned()
                    .then(|| super::journal::JournalError::Poisoned.to_string()),
            )),
            ()
        );
        snapshot.unwrap_or_else(|reason| (Ledger::default(), Some(reason)))
    }
    pub(super) fn begin_load(
        &self,
        target: Target,
        request: LoadRequest,
        source: Source,
        bundle: &[u8],
    ) -> Result<(), String> {
        journal!(
            self,
            |j| j.begin_load(target, request, source, bundle),
            (target, request, source, bundle)
        )
    }
    pub(super) fn record_load(&self, receipt: Option<LoadReceipt>) -> Result<(), String> {
        journal!(self, |j| j.record_load(receipt), (receipt))
    }
    pub(super) fn clear_load(&self) -> Result<(), String> {
        journal!(self, |j| j.clear_load(), ())
    }
    pub(super) fn begin_command(
        &self,
        target: Target,
        request: RunCommandRequest,
    ) -> Result<(), String> {
        journal!(
            self,
            |j| j.begin_command(target, request),
            (target, request)
        )
    }
    pub(super) fn record_command(&self, receipt: Option<RunCommandReceipt>) -> Result<(), String> {
        journal!(self, |j| j.record_command(receipt), (receipt))
    }
    /// Stored bytes of the recorded upload, verified against its workload root.
    pub(super) fn stored_bundle(
        &self,
        expected: orishu_workload::WorkloadDigest,
    ) -> Result<Vec<u8>, String> {
        let bytes = journal!(self, |j| j.bundle(), (expected))?;
        #[cfg(unix)]
        super::journal::verify_bundle(&bytes, expected).map_err(|e| e.to_string())?;
        Ok(bytes)
    }

    #[cfg(unix)]
    fn with<T>(
        &self,
        operation: impl FnOnce(&mut super::journal::Journal) -> Result<T, super::journal::JournalError>,
    ) -> Result<T, String> {
        match self {
            Self::Journal(journal) => {
                // A panic during a write leaves the state on disk unknown.
                let mut journal = journal
                    .lock()
                    .map_err(|_| super::journal::JournalError::Poisoned.to_string())?;
                operation(&mut journal).map_err(|e| e.to_string())
            }
            Self::Unavailable(reason) => Err(reason.clone()),
        }
    }
    #[cfg(not(unix))]
    fn refuse<T>(&self) -> Result<T, String> {
        let Self::Unavailable(reason) = self;
        Err(reason.clone())
    }
}
