//! One bounded off-window reconciliation over immutable document handles. The
//! document holds a coarse removal barrier until exact leases are adopted.
use super::{PluginStore, ReleaseLease};
use crate::document::Document;
use kagami_session::{PluginReferenceLimits, PluginReferenceReport};
use orishu_plugin::PluginReleaseId;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    thread::JoinHandle,
};

struct Completed {
    report: PluginReferenceReport,
    held: BTreeMap<PluginReleaseId, Arc<ReleaseLease>>,
    missing: BTreeSet<PluginReleaseId>,
}
/// Inventory-aware document lifetime adapter, independent of panel visibility.
pub struct References {
    store: Option<Arc<PluginStore>>,
    pending: Option<(u64, JoinHandle<Result<Completed, String>>)>,
    attempted: Option<u64>,
    settled: Option<u64>,
    held: BTreeMap<PluginReleaseId, Arc<ReleaseLease>>,
    /// Last complete known reference report (may be stale while reconciling).
    pub report: Option<PluginReferenceReport>,
    /// Referenced but not installed: preserved intent, not an acquisition failure.
    pub missing: BTreeSet<PluginReleaseId>,
    /// Bounded status; failures leave the removal barrier held until retry/close.
    pub notice: String,
}
impl References {
    /// Bind an already configured store. Document guard setup precedes Open.
    pub fn new(store: Option<Arc<PluginStore>>, document: &mut Document) -> Self {
        let notice = store
            .as_ref()
            .and_then(|s| document.configure_plugin_references(s.clone()).err())
            .map_or(String::new(), |e| e.to_string());
        Self {
            store,
            pending: None,
            attempted: None,
            settled: None,
            held: BTreeMap::new(),
            report: None,
            missing: BTreeSet::new(),
            notice,
        }
    }
    /// The pending snapshot retains its slot/leases until the actual task exits.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Only a complete current-generation reconciliation permits native removal.
    pub fn settled(&self, document: &Document) -> bool {
        self.store.is_some()
            && !self.is_pending()
            && self.settled == Some(document.reference_generation())
    }
    /// Retry explicitly after lock/IO/budget failure, never a polling retry loop.
    pub fn retry(&mut self, document: &mut Document) {
        if self.is_pending() {
            return;
        }
        if let Err(error) = document.mark_reference_update() {
            self.invalidate(error.to_string());
        }
    }
    pub(crate) fn invalidate(&mut self, notice: String) {
        self.settled = None;
        self.notice = notice;
    }
    /// Poll and start at most one bounded reconciliation for a changed generation.
    pub fn drive(&mut self, document: &mut Document) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, work)| work.is_finished())
        {
            let (generation, work) = self.pending.take().expect("finished");
            let result = work.join().unwrap_or_else(|_| {
                Err("Plugin reference worker failed; retry explicitly.".into())
            });
            if generation == document.reference_generation() {
                match result {
                    Ok(done) => {
                        // Install every precise lease before lifting the coarse guard.
                        self.held = done.held;
                        self.missing = done.missing;
                        self.report = Some(done.report);
                        match document.finish_reference_update(generation) {
                            Ok(true) => {
                                self.settled = Some(generation);
                                self.notice = format!(
                                    "Reference leases current: {} retained releases; {} unavailable releases preserved in document data.",
                                    self.held.len(),
                                    self.missing.len()
                                );
                            }
                            Ok(false) => {
                                self.notice =
                                    "Reference generation changed; reconciliation required.".into()
                            }
                            Err(error) => self.notice = error.to_string(),
                        }
                    }
                    Err(error) => self.notice = error,
                }
            }
        }
        if self.is_pending() || self.store.is_none() || document.reference_generation() == 0 {
            return;
        }
        let generation = document.reference_generation();
        if self.attempted == Some(generation) {
            return;
        }
        self.attempted = Some(generation);
        self.settled = None;
        let limits = PluginReferenceLimits::default();
        let snapshot = match document.reference_snapshot(limits) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.notice =
                    format!("Reference scan incomplete: {error}. Removal remains blocked.");
                return;
            }
        };
        let store = self.store.as_ref().expect("configured").clone();
        let previous = self.held.clone();
        let reconcile = move || {
            let report = snapshot
                .scan(limits)
                .map_err(|e| format!("Reference scan incomplete: {e}. Removal remains blocked."))?;
            let mut held = BTreeMap::new();
            let mut missing = BTreeSet::new();
            for release in report.releases.keys() {
                if let Some(lease) = previous.get(release) {
                    held.insert(*release, lease.clone());
                    continue;
                }
                match store.lease_if_installed(*release) {
                    Ok(Some(lease)) => {
                        held.insert(*release, Arc::new(lease));
                    }
                    Ok(None) => {
                        missing.insert(*release);
                    }
                    Err(error) => {
                        return Err(format!(
                            "Reference lease acquisition failed: {error}. Retry explicitly; removal remains blocked."
                        ));
                    }
                }
            }
            Ok(Completed {
                report,
                held,
                missing,
            })
        };
        match std::thread::Builder::new()
            .name("kagami-references".into())
            .spawn(reconcile)
        {
            Ok(work) => {
                self.pending = Some((generation, work));
                self.notice = "Reconciling document/history/request references; removal is temporarily blocked.".into();
            }
            Err(_) => self.notice =
                "Cannot start reference reconciliation; removal remains blocked. Retry explicitly."
                    .into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::InventoryCommand;
    use kagami_session::SessionCommand;

    fn removal(store: &PluginStore) -> super::super::Error {
        store
            .submit(
                0,
                InventoryCommand::Remove {
                    plugin_id: "org.test.absent".parse().unwrap(),
                    release: format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
                    ack_open_references: true,
                },
            )
            .unwrap_err()
    }
    fn finish(references: &mut References, document: &mut Document) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while references.is_pending() {
            assert!(std::time::Instant::now() < end);
            std::thread::yield_now();
            references.drive(document);
        }
    }
    #[test]
    fn stale_completion_cannot_unlock_a_later_receipt_only_generation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(PluginStore::open(dir.path()).unwrap());
        let mut document = Document::new(Default::default(), Default::default());
        let mut references = References::new(Some(store.clone()), &mut document);
        let generation = document.reference_generation();
        let before = document.snapshot().revision();
        let (send, receive) = std::sync::mpsc::channel();
        references.attempted = Some(generation);
        references.pending = Some((
            generation,
            std::thread::spawn(move || {
                receive.recv().unwrap();
                Ok(Completed {
                    report: PluginReferenceReport {
                        releases: BTreeMap::new(),
                        work: 1,
                    },
                    held: BTreeMap::new(),
                    missing: BTreeSet::new(),
                })
            }),
        ));
        assert!(document.submit(SessionCommand::BeginInteractiveEdit));
        assert_eq!(document.snapshot().revision(), before);
        assert_ne!(document.reference_generation(), generation);
        assert_eq!(removal(&store).code, super::super::Code::Busy);
        send.send(()).unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !references.pending.as_ref().unwrap().1.is_finished() {
            assert!(std::time::Instant::now() < end);
            std::thread::yield_now();
        }
        references.drive(&mut document);
        assert!(references.is_pending());
        assert_eq!(removal(&store).code, super::super::Code::Busy);
        finish(&mut references, &mut document);
        assert!(references.settled(&document));
        assert_eq!(removal(&store).code, super::super::Code::InvalidSelection);
    }
    #[test]
    fn acquisition_failure_keeps_the_guard_and_requires_explicit_retry() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(PluginStore::open(dir.path()).unwrap());
        let mut document = Document::new(Default::default(), Default::default());
        let mut references = References::new(Some(store.clone()), &mut document);
        let generation = document.reference_generation();
        references.attempted = Some(generation);
        references.pending = Some((
            generation,
            std::thread::spawn(|| Err("injected acquisition failure".into())),
        ));
        finish(&mut references, &mut document);
        assert!(!references.settled(&document));
        references.drive(&mut document);
        assert!(!references.is_pending(), "no implicit failure retry");
        assert_eq!(removal(&store).code, super::super::Code::Busy);
        references.retry(&mut document);
        references.drive(&mut document);
        finish(&mut references, &mut document);
        assert!(references.settled(&document));
    }

    #[test]
    fn removal_publication_refuses_authoring_before_any_document_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(PluginStore::open(dir.path()).unwrap());
        let mut document = Document::new(Default::default(), Default::default());
        let mut references = References::new(Some(store), &mut document);
        references.drive(&mut document);
        finish(&mut references, &mut document);
        let before = document.snapshot().clone();
        let generation = document.reference_generation();
        let gate = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("references.lock"))
            .unwrap();
        rustix::fs::flock(&gate, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
        assert!(!document.submit(SessionCommand::BeginInteractiveEdit));
        assert_eq!(document.reference_generation(), generation);
        assert_eq!(document.snapshot(), &before);
        drop(gate);
        assert!(document.submit(SessionCommand::BeginInteractiveEdit));
        assert_ne!(document.reference_generation(), generation);
        references.drive(&mut document);
        finish(&mut references, &mut document);
        assert!(references.settled(&document));
    }

    #[test]
    fn missing_registration_preserves_exact_pins_without_claiming_a_lease() {
        use kagami_catalog::{ComponentSchema, ComponentTypeId, SchemaRegistry, SchemaVersion};
        use kagami_document::{DisplayName, ExperimentCommand, ObjectSpec};
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(PluginStore::open(dir.path()).unwrap());
        let release = format!("sha256:{}", "01".repeat(32)).parse().unwrap();
        let kind = ComponentTypeId::exact(orishu_plugin::ContributionRef {
            release,
            extension_point: "orishu.model.components/v1".parse().unwrap(),
            local_id: "missing".parse().unwrap(),
        })
        .unwrap();
        let mut document = Document::new(
            SchemaRegistry::new().with(ComponentSchema::new(kind.clone(), SchemaVersion(1))),
            Default::default(),
        );
        let mut references = References::new(Some(store.clone()), &mut document);
        assert!(
            document.submit(SessionCommand::Edit(vec![ExperimentCommand::CreateObject(
                Box::new(
                    ObjectSpec::new(DisplayName::new("missing").unwrap())
                        .with_component(kind, Default::default())
                )
            )]))
        );
        let before = document.snapshot().clone();
        references.drive(&mut document);
        finish(&mut references, &mut document);
        assert!(references.settled(&document));
        assert_eq!(references.missing, BTreeSet::from([release]));
        assert!(references.held.is_empty());
        assert_eq!(document.snapshot(), &before);
        assert_eq!(removal(&store).code, super::super::Code::InvalidSelection);
    }
}
