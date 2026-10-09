//! Nonblocking native adapter over the same inventory authority as the CLI.
//! Installed preferences are not experiment commands; refresh changes available
//! vocabulary, never authored pins or the scientific state of accepted runs.
use super::{
    Code, Error, InventoryCommand, InventoryListing, InventoryRevisionGuard, KernelChoice, Package,
};
use crate::scientific_effect::ScientificPlugins;
use orishu_plugin::{ContributionRef, PluginId, PluginReleaseId};
use std::{path::PathBuf, thread::JoinHandle};

#[derive(Clone, Debug, PartialEq)]
/// Explicit window intents; paths and choices are inputs, never physics pins.
pub enum Action {
    /// Open the panel and request a verified availability snapshot.
    Open,
    /// Hide the panel without cancelling an inventory write.
    Close,
    /// Explicitly read and adopt current availability.
    Refresh,
    /// Ask the native chooser for an install or named-plugin update bundle.
    PickBundle(Option<PluginId>),
    /// Install a local bundle against the displayed revision.
    Install {
        path: PathBuf,
        update: Option<PluginId>,
    },
    /// Submit one existing authority command.
    Mutate(InventoryCommand),
    /// Stage exact registration removal for explicit confirmation.
    Remove {
        plugin_id: PluginId,
        release: PluginReleaseId,
    },
    /// Confirm staged removal, optionally acknowledging known retained readers.
    ConfirmRemoval(bool),
    /// Discard the staged removal without submitting anything.
    CancelRemoval,
    /// Verify and summarize one exact installed release without executing it.
    Inspect(PluginReleaseId),
    /// Query this session's retained references; the document shell answers it.
    References(PluginReleaseId),
    /// Retry incomplete document lease reconciliation explicitly.
    RetryReferences,
    /// Nonblocking completion tick.
    Poll,
}
/// One coherent availability result, protected until the shell adopts it.
pub struct Refresh {
    pub(crate) plugins: ScientificPlugins,
    pub(crate) schemas: kagami_catalog::SchemaRegistry,
    pub(crate) kernels: Vec<KernelChoice>,
    pub(crate) unresolved_components: Vec<ContributionRef>,
    // Hold availability through the window's atomic vocabulary adoption.
    pub(crate) _guard: InventoryRevisionGuard,
}
struct Snapshot {
    listing: InventoryListing,
    refresh: Refresh,
    unavailable: usize,
}
enum Reply {
    Refreshed(Snapshot),
    Inspected(String),
    // Mutation acceptance is distinct from subsequent refresh success.
    Changed {
        revision: u64,
        snapshot: Result<Snapshot, Error>,
    },
}
/// One background operation and bounded inventory presentation. Dropping a
/// JoinHandle never joins on the window thread; the store owns atomic writes.
pub struct Controller {
    plugins: Option<ScientificPlugins>,
    pending: Option<JoinHandle<Result<Reply, Error>>>,
    changed_revision: Option<u64>,
    /// Whether its panel is visible (independent of job lifetime).
    pub open: bool,
    /// Last successfully verified snapshot, including mutation precondition.
    pub listing: Option<InventoryListing>,
    /// Accepted/pending/refused feedback, distinct from document diagnostics.
    pub notice: String,
    /// Declarative manifest summary only; never kernel bytes.
    pub inspection: Option<String>,
    /// Staged removal is not a command until explicitly confirmed.
    pub removal: Option<Removal>,
}
/// Exact displayed registration and inventory revision to be confirmed.
#[derive(Clone, Debug)]
pub struct Removal {
    pub plugin_id: PluginId,
    pub release: PluginReleaseId,
    revision: u64,
}
impl Controller {
    /// Bind startup's secure store without performing IO.
    pub fn new(plugins: Option<ScientificPlugins>) -> Self {
        Self {
            plugins,
            pending: None,
            changed_revision: None,
            open: false,
            listing: None,
            notice: String::new(),
            inspection: None,
            removal: None,
        }
    }
    /// Capacity remains occupied until the actual worker thread finishes.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub(crate) fn take_changed_revision(&mut self) -> Option<u64> {
        self.changed_revision.take()
    }
    /// Session preferences kept separate from persistent enablement.
    pub fn overrides(&self) -> &[(PluginId, bool)] {
        self.plugins
            .as_ref()
            .map_or(&[], |p| p.overrides.as_slice())
    }
    /// Start at most one operation; refuse stale revisions without retrying.
    pub fn act(&mut self, mut action: Action) {
        match action {
            Action::Close => {
                self.open = false;
                return;
            }
            Action::Open => self.open = true,
            Action::Poll => return,
            Action::CancelRemoval => {
                self.removal = None;
                return;
            }
            _ => (),
        }
        if self.is_pending() {
            return;
        }
        let Some(plugins) = self.plugins.clone() else {
            self.notice = "No supported local inventory is configured.".into();
            return;
        };
        let revision = self.listing.as_ref().map(|l| l.revision);
        if let Action::Remove { plugin_id, release } = action {
            if let Some(listing) = &self.listing
                && listing
                    .releases
                    .iter()
                    .any(|e| e.plugin_id == plugin_id && e.release == release)
            {
                self.removal = Some(Removal {
                    plugin_id,
                    release,
                    revision: listing.revision,
                });
            }
            return;
        }
        if let Action::ConfirmRemoval(ack_open_references) = action {
            let Some(removal) = &self.removal else {
                return;
            };
            if revision != Some(removal.revision) {
                self.notice =
                    "Inventory changed since removal was staged; inspect and confirm again.".into();
                self.removal = None;
                return;
            }
            action = Action::Mutate(InventoryCommand::Remove {
                plugin_id: removal.plugin_id.clone(),
                release: removal.release,
                ack_open_references,
            });
        }
        if matches!(action, Action::Install { .. } | Action::Mutate(_)) && revision.is_none() {
            self.notice = "Refresh the inventory before changing it.".into();
            return;
        }
        if matches!(
            action,
            Action::PickBundle(_) | Action::References(_) | Action::RetryReferences
        ) {
            return;
        } // shell owns chooser
        self.inspection = None;
        match std::thread::Builder::new()
            .name("kagami-plugins".into())
            .spawn(move || {
                let changed = match action {
                    Action::Open | Action::Refresh => {
                        return refresh(plugins).map(Reply::Refreshed);
                    }
                    Action::Inspect(release) => {
                        let package = plugins.store.inspect(release)?;
                        let root = &package.release().root().0;
                        let mut summary = format!(
                            "{}\n{}\nVersion label: {}\nContributions:\n",
                            root.metadata.plugin_id, release, root.metadata.version_label
                        );
                        // JSON escaping prevents local control characters from
                        // masquerading as labels; never interpret this as a URL.
                        summary.push_str(&format!(
                            "Local origin (historical only): {}\n",
                            serde_json::to_string(&package.origin()).expect("bounded metadata")
                        ));
                        for contribution in &root.spec.contributions {
                            use std::fmt::Write;
                            writeln!(
                                &mut summary,
                                "{} → {}",
                                contribution.local_id, contribution.extension_point
                            )
                            .expect("string write");
                        }
                        // Never return executable payloads to the UI.
                        return Ok(Reply::Inspected(summary));
                    }
                    Action::Install { path, update } => {
                        let package = Package::load_bundle(&path)?;
                        plugins.store.install(
                            revision.expect("checked revision"),
                            &package,
                            update.as_ref(),
                            None,
                        )?
                    }
                    Action::Mutate(command) => plugins
                        .store
                        .submit(revision.expect("checked revision"), command)?,
                    _ => unreachable!("local actions handled before spawning"),
                };
                Ok(Reply::Changed {
                    revision: changed,
                    snapshot: refresh(plugins),
                })
            }) {
            Ok(task) => {
                self.pending = Some(task);
                self.notice = "Plugin operation pending. Closing the panel does not cancel an inventory write.".into();
            }
            Err(_) => self.notice = "Cannot start plugin operation; nothing was submitted.".into(),
        }
    }
    /// Adopt feedback and return a coherent new vocabulary, never wait on IO.
    pub fn poll(&mut self) -> Option<Refresh> {
        if !self.pending.as_ref().is_some_and(|p| p.is_finished()) {
            return None;
        }
        let result = self
            .pending
            .take()
            .expect("finished")
            .join()
            .unwrap_or_else(|_| {
                Err(Error::new(
                    Code::IoFailure,
                    "plugin operation thread failed; refresh inventory before retrying",
                ))
            });
        if let Ok(Reply::Changed { revision, .. }) = &result {
            self.changed_revision = Some(*revision);
            self.removal = None;
        }
        let (snapshot, accepted) = match result {
            Ok(Reply::Inspected(text)) => {
                self.inspection = Some(text);
                self.notice = "Verified installed release; no guest executed.".into();
                return None;
            }
            Ok(Reply::Refreshed(snapshot)) => (snapshot, None),
            Ok(Reply::Changed {
                revision,
                snapshot: Ok(snapshot),
            }) => (snapshot, Some(revision)),
            Ok(Reply::Changed {
                revision,
                snapshot: Err(error),
            }) => {
                self.listing = None;
                self.notice = format!(
                    "Inventory change accepted at revision {revision}, but vocabulary refresh failed: {error}. Refresh explicitly; do not repeat the accepted mutation."
                );
                return None;
            }
            Err(error) => {
                self.notice = error.to_string();
                return None;
            }
        };
        self.notice = format!(
            "{}Inventory revision {}. {} component contributions require dependency selection or repair. Exact document pins and accepted runs are unchanged.",
            accepted.map_or(String::new(), |r| format!(
                "Change accepted at revision {r}. "
            )),
            snapshot.listing.revision,
            snapshot.unavailable
        );
        self.plugins = Some(snapshot.refresh.plugins.clone());
        self.listing = Some(snapshot.listing);
        Some(snapshot.refresh)
    }
}

fn refresh(mut plugins: ScientificPlugins) -> Result<Snapshot, Error> {
    let listing = plugins.store.list()?;
    let components = plugins.store.available_components(&plugins.overrides)?;
    let models = plugins.store.available_models(&plugins.overrides)?;
    if listing.revision != components.revision || listing.revision != models.revision {
        return Err(Error::new(
            Code::StaleRevision,
            "inventory changed during refresh; retry explicitly",
        ));
    }
    let guard = plugins.store.guard_revision(listing.revision)?;
    plugins.revision = listing.revision;
    let mut schemas = components.schemas;
    for schema in crate::model::bundled_schemas().schemas() {
        schemas.insert(schema.clone());
    }
    Ok(Snapshot {
        listing,
        unavailable: components.unavailable.len(),
        refresh: Refresh {
            plugins,
            schemas,
            kernels: models.kernels,
            unresolved_components: components.unavailable,
            _guard: guard,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_does_not_release_pending_capacity_or_hide_an_accepted_write() {
        let (release, resume) = std::sync::mpsc::channel();
        let mut manager = Controller::new(None);
        manager.open = true;
        manager.pending = Some(std::thread::spawn(move || {
            resume.recv().unwrap();
            Ok(Reply::Changed {
                revision: 7,
                snapshot: Err(Error::new(
                    Code::StaleRevision,
                    "refresh raced another writer",
                )),
            })
        }));
        manager.act(Action::Close);
        assert!(!manager.open && manager.is_pending());
        manager.act(Action::Refresh);
        assert!(manager.is_pending());
        assert!(manager.poll().is_none());
        release.send(()).unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while manager.is_pending() {
            assert!(std::time::Instant::now() < end);
            assert!(manager.poll().is_none());
            std::thread::yield_now();
        }
        assert!(manager.listing.is_none());
        assert!(manager.notice.contains("accepted at revision 7"));
        assert!(manager.notice.contains("do not repeat"));
    }
}
