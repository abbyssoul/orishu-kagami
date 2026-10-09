//! Bounded, non-mutating window compilation. This never initializes a kernel or
//! submits a run. Publication/submission consume an explicitly frozen workload.
use crate::{
    document::{AuthoringGuard, Document},
    export::{ExportError, ExportReport, PreparedExport},
    plugins::InventoryRevisionGuard,
    scientific_effect::ScientificPlugins,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

const INPUT_BYTES: usize = 128 * 1024 * 1024;
pub(crate) const BUNDLE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Name(String),
    Formation(String),
    Prepare,
    Cancel,
    Export,
    ExportTo(PathBuf),
    Submit,
    Poll,
}

/// A workload already compiled from one revision. Immutable bytes, not a view of
/// the mutable document or inventory. No provider re-resolution on export/upload.
pub struct FrozenWorkload {
    pub report: ExportReport,
    /// Local source incarnation distinguishes replacement drafts at the same revision.
    /// It is lineage for this window, never part of the workload's canonical bytes.
    pub source_context: uuid::Uuid,
    pub source_revision: u64,
    pub(crate) bytes: Arc<Vec<u8>>,
}
struct Ready {
    guard: AuthoringGuard,
    frozen: Arc<FrozenWorkload>,
}
enum Completed {
    Prepared {
        product: PreparedExport,
        _availability: InventoryRevisionGuard,
    },
    Exported,
}
struct Pending {
    guard: Option<AuthoringGuard>,
    revision: u64,
    cancel: Arc<AtomicBool>,
    work: Option<JoinHandle<Result<Completed, ExportError>>>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// One non-queuing compile/export lane. A cancelled job retains capacity until
/// actual exit. The revision guard covers adoption; the inventory lock is held
/// only at completion, never while compiling or uploading.
pub struct Preparation {
    plugins: Option<ScientificPlugins>,
    pending: Option<Pending>,
    ready: Option<Ready>,
    pub name: String,
    pub formation: String,
    pub notice: String,
}
impl Preparation {
    pub(crate) fn refresh_plugins(&mut self, plugins: ScientificPlugins) {
        self.cancel();
        self.ready = None;
        self.plugins = Some(plugins);
    }
    pub fn new(plugins: Option<ScientificPlugins>) -> Self {
        Self {
            plugins,
            pending: None,
            ready: None,
            name: "experiment".into(),
            formation: String::new(),
            notice: String::new(),
        }
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn configured(&self) -> bool {
        self.plugins.is_some()
    }
    pub fn can_cancel(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| p.guard.is_some())
    }
    pub fn ready(&self, document: &Document) -> Option<&Arc<FrozenWorkload>> {
        self.ready
            .as_ref()
            .filter(|r| document.accepts_effect(r.guard))
            .map(|r| &r.frozen)
    }
    pub fn edit_name(&mut self, value: String) {
        if value.len() <= 128 && !self.is_pending() {
            self.name = value;
            self.ready = None;
        }
    }
    pub fn edit_formation(&mut self, value: String) {
        if value.len() <= 128 {
            self.formation = value;
        }
    }
    pub fn cancel(&self) {
        if let Some(p) = &self.pending
            && p.guard.is_some()
        {
            p.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn prepare(&mut self, document: &Document) -> Result<(), ExportError> {
        if self.is_pending() {
            return Err(refusal("busy", "A workload job still owns capacity."));
        }
        let guard = document
            .authoring_guard()
            .ok_or_else(|| refusal("mode", "Return to Authoring first."))?;
        let plugins = self
            .plugins
            .clone()
            .ok_or_else(|| refusal("inventory", "No startup plugin inventory."))?;
        let name = self
            .name
            .parse()
            .map_err(|_| refusal("name", "Enter a valid workload name."))?;
        let setup = document.snapshot().setup().scientific().ok_or_else(|| {
            refusal(
                "setup",
                "Configure and capture scientific physics before preparation.",
            )
        })?;
        kagami_document::scientific::ScientificRetention::new(INPUT_BYTES)
            .include(setup)
            .map_err(|_| {
                refusal(
                    "limit",
                    "Captured input exceeds the 128 MiB preparation budget.",
                )
            })?;
        let snapshot = document.snapshot().clone();
        let revision = snapshot.revision().get();
        let limits = *document.limits();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        self.ready = None;
        let work = std::thread::Builder::new()
            .name("kagami-workload-compile".into())
            .spawn(move || {
                if flag.load(Ordering::Relaxed) {
                    return Err(refusal("cancelled", "Preparation cancelled."));
                }
                let mut policy = orishu_plugin::workload::ProfileLimits::default();
                policy.selection.artifact_bytes = 64 * 1024 * 1024;
                policy.selection.metadata_bytes = 8 * 1024 * 1024;
                policy.input_bytes = INPUT_BYTES as u64;
                policy.workload.max_aggregate_declared_bytes = BUNDLE_BYTES as u64;
                let archive = orishu_plugin::archive::ArchiveLimits {
                    max_bytes: BUNDLE_BYTES,
                    total_blob_bytes: BUNDLE_BYTES as u64,
                    ..Default::default()
                };
                let product = crate::export::compile_snapshot(
                    &snapshot, &plugins, name, policy, archive, &limits,
                )?;
                if flag.load(Ordering::Relaxed) {
                    return Err(refusal("cancelled", "Preparation cancelled."));
                }
                let availability =
                    plugins
                        .store
                        .guard_revision(plugins.revision)
                        .map_err(|_| {
                            refusal(
                                "inventory_changed",
                                "Inventory changed or is busy; prepare again explicitly.",
                            )
                        })?;
                Ok(Completed::Prepared {
                    product,
                    _availability: availability,
                })
            })
            .map_err(|_| refusal("executor", "Cannot start workload preparation."))?;
        self.pending = Some(Pending {
            guard: Some(guard),
            revision,
            cancel,
            work: Some(work),
        });
        self.notice =
            "Compiling captured state and exact selected kernels; no initialization or upload."
                .into();
        Ok(())
    }
    /// Export the explicitly frozen revision to a new file. Subsequent draft
    /// edits do not rewrite or cancel an already requested immutable export.
    pub fn export(&mut self, document: &Document, path: PathBuf) -> Result<(), ExportError> {
        if self.is_pending() {
            return Err(refusal("busy", "A workload job still owns capacity."));
        }
        let frozen = self
            .ready(document)
            .cloned()
            .ok_or_else(|| refusal("stale", "Prepare the current draft first."))?;
        let work = std::thread::Builder::new()
            .name("kagami-workload-export".into())
            .spawn(move || {
                crate::files::create_file(&path, &frozen.bytes)
                    .map_err(|e| refusal("publish", crate::plugins::Error::from(e)))?;
                Ok(Completed::Exported)
            })
            .map_err(|_| refusal("executor", "Cannot start workload export."))?;
        self.pending = Some(Pending {
            guard: None,
            revision: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            work: Some(work),
        });
        self.notice =
            "Publishing the frozen workload to a new file; existing files are never overwritten."
                .into();
        Ok(())
    }
    pub fn poll(&mut self, document: &Document) {
        if self
            .ready
            .as_ref()
            .is_some_and(|r| !document.accepts_effect(r.guard))
        {
            self.ready = None;
        }
        let Some(p) = &self.pending else {
            return;
        };
        if p.guard.is_some_and(|g| !document.accepts_effect(g)) {
            p.cancel.store(true, Ordering::Relaxed);
        }
        if !p.work.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        // Move the handle out without prematurely treating cancellation as exit.
        let mut p = self.pending.take().expect("finished");
        let result = p
            .work
            .take()
            .expect("finished handle")
            .join()
            .unwrap_or_else(|_| Err(refusal("executor", "Workload job failed.")));
        if p.guard.is_some() && p.cancel.load(Ordering::Relaxed) {
            self.notice = "Preparation discarded: cancelled or authoring context changed. Prepare again explicitly.".into();
            return;
        }
        match result {
            Ok(Completed::Prepared {
                product,
                _availability,
            }) => {
                self.ready = Some(Ready {
                    guard: p.guard.expect("compile guard"),
                    frozen: Arc::new(FrozenWorkload {
                        report: product.report,
                        source_context: p.guard.expect("compile guard").context_id(),
                        source_revision: p.revision,
                        bytes: Arc::new(product.bytes),
                    }),
                });
                self.notice = "Workload frozen. Export or submit explicitly; edits invalidate the prepared draft.".into();
            }
            Ok(Completed::Exported) => {
                self.notice = "Frozen workload exported. The experiment is unchanged.".into()
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
}
fn refusal(code: &str, message: impl std::fmt::Display) -> ExportError {
    ExportError::new("window", code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn cancelled_or_stale_work_keeps_its_slot_until_actual_exit() {
        for edit in [false, true] {
            let mut document = Document::new(Default::default(), Default::default());
            let mut preparation = Preparation::new(None);
            let (release, resume) = std::sync::mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            preparation.pending = Some(Pending {
                guard: document.authoring_guard(),
                revision: 0,
                cancel: cancel.clone(),
                work: Some(std::thread::spawn(move || {
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    Err(refusal("test", "Completed worker refusal."))
                })),
            });
            if edit {
                assert!(
                    document.edit(vec![kagami_document::ExperimentCommand::CreateObject(
                        Box::new(kagami_document::ObjectSpec::new(
                            kagami_document::DisplayName::new("changed").unwrap()
                        ))
                    )])
                );
            } else {
                preparation.cancel();
            }
            preparation.poll(&document);
            assert!(cancel.load(Ordering::Relaxed));
            assert!(preparation.is_pending());
            assert_eq!(preparation.prepare(&document).unwrap_err().code, "busy");
            release.send(()).unwrap();
            let end = Instant::now() + Duration::from_secs(5);
            while preparation.is_pending() {
                preparation.poll(&document);
                assert!(Instant::now() < end);
                std::thread::yield_now();
            }
            assert!(preparation.ready(&document).is_none());
            assert!(preparation.notice.contains("discarded"));
        }
    }
}
