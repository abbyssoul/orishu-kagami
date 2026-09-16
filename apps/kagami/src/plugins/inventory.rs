use super::{Code, Error, Package, files::Directory, package::BoundedList};
use orishu_plugin::{
    resolution::{self, VerifiedRelease},
    *,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::Path,
};

const INDEX_BYTES: usize = 256 * 1024;

/// One immutable release registration. Enablement is logical-plugin-wide.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InstalledRelease {
    /// Logical namespace, independently checked against the root when loaded.
    pub plugin_id: PluginId,
    /// Content identity, never a mutable label.
    pub release: PluginReleaseId,
    /// Logical enablement (same value on every release of the plugin).
    pub enabled: bool,
    /// Exactly one installed release per logical plugin is the default.
    pub is_default: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Version {
    #[serde(rename = "orishu.plugin-inventory/v1")]
    V1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Index {
    api_version: Version,
    revision: u64,
    releases: BoundedList<InstalledRelease, 256>,
}
impl Default for Index {
    fn default() -> Self {
        Self {
            api_version: Version::V1,
            revision: 0,
            releases: BoundedList(Vec::new()),
        }
    }
}
impl Index {
    fn validate(&self) -> Result<(), Error> {
        let mut ids = BTreeSet::new();
        let mut logical = BTreeMap::new();
        for entry in &self.releases.0 {
            if !ids.insert(entry.release) {
                return Err(invalid("duplicate installed release"));
            }
            let group = logical
                .entry(&entry.plugin_id)
                .or_insert((entry.enabled, 0usize));
            if group.0 != entry.enabled {
                return Err(invalid("inconsistent logical enablement"));
            }
            group.1 += usize::from(entry.is_default);
        }
        if logical.values().any(|(_, defaults)| *defaults != 1) {
            return Err(invalid(
                "each installed plugin must have exactly one default",
            ));
        }
        Ok(())
    }
    fn expected(&self, expected: u64) -> Result<(), Error> {
        if expected != self.revision {
            return Err(Error {
                code: Code::StaleRevision,
                message: "refresh the plugin inventory and retry".into(),
                expected_revision: Some(expected),
                actual_revision: Some(self.revision),
            });
        }
        Ok(())
    }
}
fn invalid(message: &str) -> Error {
    Error::new(Code::InvalidSelection, message)
}
fn hex(id: impl std::fmt::Display) -> String {
    id.to_string()
        .strip_prefix("sha256:")
        .expect("typed SHA256")
        .into()
}

/// Serialized summary, safe to present without reading every kernel's bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryListing {
    /// Revision against which management commands and provider choices are made.
    pub revision: u64,
    /// Complete bounded list; no implicit truncation.
    pub releases: Vec<InstalledRelease>,
}

/// Exact selected authoring vocabulary from one verified inventory snapshot.
/// An unavailable or stale selection supplies no schemas; callers must display
/// the outcome and preserve authored pins, never substitute a default provider.
#[derive(Clone, Debug)]
pub struct AuthoringResolution {
    /// Provider decision, including exact bindings or actionable choices.
    pub outcome: resolution::ResolutionOutcome,
    /// Schemas for the component members of a fully resolved selection.
    pub schemas: kagami_catalog::SchemaRegistry,
}

/// Startup vocabulary only: no field model or integrator is selected or run.
#[derive(Clone, Debug)]
pub struct AvailableComponents {
    /// The immutable inventory revision used throughout discovery.
    pub revision: u64,
    /// Enabled exact component declarations with resolvable dependencies.
    pub schemas: kagami_catalog::SchemaRegistry,
    /// Enabled component pins requiring an explicit dependency choice or repair.
    /// `resolve_authoring` supplies detailed diagnostics when one is selected.
    pub unavailable: Vec<ContributionRef>,
}

/// Explicitly selectable computation, not an automatic default or proof of
/// dependency availability. The shared resolver decides a complete selection.
#[derive(Clone, Debug, PartialEq)]
pub struct KernelChoice {
    /// Exact immutable provider-qualified contribution.
    pub contribution: ContributionRef,
    /// One platform execution contract.
    pub contract: ExecutionContractId,
    /// Human-readable provider/use label; never workload identity.
    pub label: String,
    /// Declared sample slots; the initial UI requests direct-quality samples.
    pub observable_slots: Vec<LocalContributionId>,
    /// Verified declarative configuration; UI text is not resolved physics.
    pub configuration: Vec<orishu_plugin::Property>,
}
/// One immutable startup projection of installed enabled computational choices.
pub struct AvailableModels {
    /// Compare with the component vocabulary projection before launching.
    pub revision: u64,
    /// Bounded choices; selecting one is an explicit user action.
    pub kernels: Vec<KernelChoice>,
}

/// Local durable authority. Mutations use a nonblocking process/cross-process
/// lock plus expected revision. The content cache is append-only in this MVP;
/// remove de-registers, never purges blobs or rewrites authored pins.
#[derive(Debug)]
pub struct PluginStore {
    dir: Directory,
}

mod selection;
pub use selection::{InventoryRevisionGuard, PrepareSelectionOutcome, PreparedSelection};

/// Keeps required release bytes available to an open document or accepted run.
/// OS-managed shared locks disappear on process exit; no stale PID lease cleanup.
#[derive(Debug)]
pub struct ReleaseLease {
    _lock: File,
    release: PluginReleaseId,
}
impl ReleaseLease {
    /// Exact retained release, even if later de-registered with acknowledgement.
    pub fn release(&self) -> PluginReleaseId {
        self.release
    }
}

/// Requested mutation. Installation packages enter through `install`, after
/// declaration verification; these operations never execute kernels.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "command",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InventoryCommand {
    /// Change the default; existing documents keep their exact pins.
    SetDefault {
        plugin_id: PluginId,
        release: PluginReleaseId,
    },
    /// Change future availability for all releases of one plugin.
    SetEnabled { plugin_id: PluginId, enabled: bool },
    /// De-register only; acknowledge leases explicitly without deleting data.
    Remove {
        plugin_id: PluginId,
        release: PluginReleaseId,
        ack_open_references: bool,
    },
}

impl PluginStore {
    /// Discover enabled field/integrator declarations without executing or
    /// selecting them. Independent/dormant contributions remain explicit choices;
    /// unresolved dependencies return structured refusals on Apply.
    pub fn available_models(
        &self,
        overrides: &[(PluginId, bool)],
    ) -> Result<AvailableModels, Error> {
        self.with_inventory(self.index()?, overrides, |index, releases, _| {
            let mut kernels = Vec::new();
            let mut parameter_bytes = 0usize;
            for (entry, release) in index.releases.0.iter().zip(releases) {
                if !overrides
                    .iter()
                    .find(|(p, _)| p == &entry.plugin_id)
                    .map_or(entry.enabled, |(_, enabled)| *enabled)
                {
                    continue;
                }
                for (local, payload) in release.payloads() {
                    let (contract, observable_slots, configuration) = match payload.payload() {
                        Some(Payload::FieldModels(m)) => (
                            ExecutionContractId::Field,
                            m.scientific.observables.as_slice(),
                            &m.scientific.configuration,
                        ),
                        Some(Payload::Integrators(m)) => (
                            ExecutionContractId::Dynamics,
                            &[][..],
                            &m.scientific.configuration,
                        ),
                        _ => continue,
                    };
                    if kernels.len() == 256 {
                        return Err(Error::new(
                            Code::LimitExceeded,
                            "startup model discovery exceeds 256 contributions",
                        ));
                    }
                    for property in configuration {
                        use orishu_plugin::PropertyType;
                        let text_bytes = match &property.schema {
                            PropertyType::Quantity {
                                default_expression, ..
                            } => default_expression.as_ref().map_or(0, String::len),
                            PropertyType::Text { default, .. } => {
                                default.as_ref().map_or(0, String::len)
                            }
                            PropertyType::Boolean { .. } => 0,
                        };
                        parameter_bytes = parameter_bytes
                            .checked_add(
                                std::mem::size_of::<orishu_plugin::Property>()
                                    + property.id.as_str().len()
                                    + text_bytes,
                            )
                            .filter(|n| *n <= 8 * 1024 * 1024)
                            .ok_or_else(|| {
                                Error::new(
                                    Code::LimitExceeded,
                                    "startup model parameter metadata exceeds 8 MiB",
                                )
                            })?;
                    }
                    kernels.push(KernelChoice {
                        contribution: release
                            .contribution_ref(local)
                            .expect("verified contribution"),
                        contract,
                        label: format!("{} / {} ({})", entry.plugin_id, local, entry.release),
                        observable_slots: observable_slots.to_vec(),
                        configuration: configuration.clone(),
                    });
                }
            }
            Ok(AvailableModels {
                revision: index.revision,
                kernels,
            })
        })
    }
    /// Open/create a user-selected private store root. Paths inside it are always
    /// descriptor-relative and never follow symlinks supplied by a package.
    pub fn open(path: &Path) -> Result<Self, Error> {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        let dir = Directory::open(path)?;
        let _lock = dir.lock("inventory.lock", false)?;
        dir.child("blobs".as_ref(), true)?;
        dir.child("releases".as_ref(), true)?;
        dir.child("leases".as_ref(), true)?;
        let store = Self { dir };
        store.index()?;
        Ok(store)
    }
    fn index(&self) -> Result<Index, Error> {
        let Some(bytes) = self
            .dir
            .read_optional(Path::new("index.json"), INDEX_BYTES)?
        else {
            return Ok(Index::default());
        };
        let index: Index = serde_json::from_slice(&bytes)
            .map_err(|_| Error::new(Code::Malformed, "invalid local plugin inventory index"))?;
        index.validate()?;
        Ok(index)
    }
    /// Read one immutable index revision. A writer's staging files are invisible.
    pub fn list(&self) -> Result<InventoryListing, Error> {
        let index = self.index()?;
        Ok(InventoryListing {
            revision: index.revision,
            releases: index.releases.0,
        })
    }
    fn publish(&self, mut index: Index) -> Result<u64, Error> {
        index.validate()?;
        if index.releases.0.len() > 256 {
            return Err(Error::new(
                Code::LimitExceeded,
                "installed release count exceeds 256",
            ));
        }
        index
            .releases
            .0
            .sort_by(|a, b| (&a.plugin_id, a.release).cmp(&(&b.plugin_id, b.release)));
        index.revision = index
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::new(Code::LimitExceeded, "inventory revision exhausted"))?;
        let bytes = serde_json::to_vec(&index)
            .map_err(|_| Error::new(Code::Malformed, "index serialization failed"))?;
        if bytes.len() > INDEX_BYTES {
            return Err(Error::new(
                Code::LimitExceeded,
                "inventory index byte budget exceeded",
            ));
        }
        self.dir.replace("index.json", &bytes)?;
        Ok(index.revision)
    }
    /// Install an exact package, or update a named logical plugin from it. First
    /// install enables and defaults; additional install preserves both. Update
    /// preserves enablement and explicitly changes the default, never a document.
    pub fn install(
        &self,
        expected_revision: u64,
        package: &Package,
        update: Option<&PluginId>,
        expect_release: Option<PluginReleaseId>,
    ) -> Result<u64, Error> {
        let id = package.release.id();
        let plugin = &package.release.root().0.metadata.plugin_id;
        if expect_release.is_some_and(|expected| expected != id) {
            return Err(Error::new(
                Code::IntegrityMismatch,
                "package does not match expected release",
            ));
        }
        if update.is_some_and(|expected| expected != plugin) {
            return Err(invalid("update package belongs to another logical plugin"));
        }
        let _lock = self.dir.lock("inventory.lock", false)?;
        let mut index = self.index()?;
        index.expected(expected_revision)?;
        let previous = index.releases.0.iter().find(|e| &e.plugin_id == plugin);
        if update.is_some() && previous.is_none() {
            return Err(invalid("update requires an installed logical plugin"));
        }
        let enabled = previous.is_none_or(|e| e.enabled);
        let default = previous.is_none() || update.is_some();
        let installed = index.releases.0.iter().any(|e| e.release == id);
        if index
            .releases
            .0
            .iter()
            .any(|e| e.release == id && &e.plugin_id != plugin)
        {
            return Err(Error::new(
                Code::IntegrityMismatch,
                "installed release has inconsistent logical identity",
            ));
        }
        if !installed && index.releases.0.len() >= 256 {
            return Err(Error::new(
                Code::LimitExceeded,
                "installed release count exceeds 256",
            ));
        }
        if default {
            for e in &mut index.releases.0 {
                if &e.plugin_id == plugin {
                    e.is_default = e.release == id;
                }
            }
        }
        if !installed {
            index.releases.0.push(InstalledRelease {
                plugin_id: plugin.clone(),
                release: id,
                enabled,
                is_default: default,
            });
        }
        index.validate()?;
        let blobs = self.dir.child("blobs".as_ref(), false)?;
        for (digest, bytes) in &package.blobs {
            blobs.put(&hex(digest), bytes)?;
        }
        let releases = self.dir.child("releases".as_ref(), false)?;
        releases.put(
            &hex(id),
            &package.release.root().canonical_bytes(&Limits::default())?,
        )?;
        // Every blob/root has been flushed before making this revision visible.
        self.publish(index)
    }
    /// Atomically apply a typed management command against an explicit revision.
    pub fn submit(&self, expected_revision: u64, command: InventoryCommand) -> Result<u64, Error> {
        let _lock = self.dir.lock("inventory.lock", false)?;
        let mut index = self.index()?;
        index.expected(expected_revision)?;
        let mut removal_guard = None;
        match command {
            InventoryCommand::SetDefault { plugin_id, release } => {
                if !index
                    .releases
                    .0
                    .iter()
                    .any(|e| e.plugin_id == plugin_id && e.release == release)
                {
                    return Err(invalid(
                        "default must name an installed release of that plugin",
                    ));
                }
                for e in &mut index.releases.0 {
                    if e.plugin_id == plugin_id {
                        e.is_default = e.release == release;
                    }
                }
            }
            InventoryCommand::SetEnabled { plugin_id, enabled } => {
                if !index.releases.0.iter().any(|e| e.plugin_id == plugin_id) {
                    return Err(invalid("plugin is not installed"));
                }
                for e in &mut index.releases.0 {
                    if e.plugin_id == plugin_id {
                        e.enabled = enabled;
                    }
                }
            }
            InventoryCommand::Remove {
                plugin_id,
                release,
                ack_open_references,
            } => {
                let entry = index
                    .releases
                    .0
                    .iter()
                    .find(|e| e.plugin_id == plugin_id && e.release == release)
                    .ok_or_else(|| invalid("release is not installed for that plugin"))?;
                if entry.is_default
                    && index
                        .releases
                        .0
                        .iter()
                        .any(|e| e.plugin_id == plugin_id && e.release != release)
                {
                    return Err(invalid(
                        "select another default before removing this release",
                    ));
                }
                let leases = self.dir.child("leases".as_ref(), false)?;
                match leases.lock(&hex(release), false) {
                    Ok(guard) => removal_guard = Some(guard),
                    Err(error) if error.code == Code::Busy && ack_open_references => (),
                    Err(error) if error.code == Code::Busy => {
                        return Err(Error::new(
                            Code::InUse,
                            "open references retain this release; explicit acknowledgement required",
                        ));
                    }
                    Err(error) => return Err(error),
                }
                index.releases.0.retain(|e| e.release != release);
            }
        }
        let result = self.publish(index);
        drop(removal_guard);
        result
    }
    /// Retain an installed release while a document/run uses it. Acquisition and
    /// de-registration serialize through the inventory lock. Disable is allowed;
    /// availability is separately validated when accepting new authored intent.
    pub fn lease(&self, release: PluginReleaseId) -> Result<ReleaseLease, Error> {
        let _lock = self.dir.lock("inventory.lock", false)?;
        if !self
            .index()?
            .releases
            .0
            .iter()
            .any(|e| e.release == release)
        {
            return Err(invalid("release is not installed"));
        }
        let lock = self
            .dir
            .child("leases".as_ref(), false)?
            .lock(&hex(release), true)?;
        Ok(ReleaseLease {
            _lock: lock,
            release,
        })
    }
    /// Load and independently reverify the exact immutable package. A retained
    /// lease can still read after acknowledged de-registration (no physical GC).
    pub fn retained_package(&self, lease: &ReleaseLease) -> Result<Package, Error> {
        self.package(lease.release)
    }
    /// Inspect an installed release, checking all stored bytes before returning.
    pub fn inspect(&self, release: PluginReleaseId) -> Result<Package, Error> {
        let lease = self.lease(release)?;
        self.retained_package(&lease)
    }
    fn package(&self, id: PluginReleaseId) -> Result<Package, Error> {
        let limits = Limits::default();
        let root_bytes = self
            .dir
            .child("releases".as_ref(), false)?
            .read(Path::new(&hex(id)), limits.max_manifest_bytes)?;
        let root = release_from_cbor(&root_bytes, &limits)?;
        if root.release_id(&limits)? != id {
            return Err(Error::new(
                Code::IntegrityMismatch,
                "stored release identity mismatch",
            ));
        }
        let dir = self.dir.child("blobs".as_ref(), false)?;
        let mut blobs = BTreeMap::new();
        for artifact in &root.0.spec.artifacts {
            blobs.insert(
                artifact.digest,
                dir.read(
                    Path::new(&hex(artifact.digest)),
                    artifact.size_bytes as usize,
                )?,
            );
        }
        let borrowed = blobs.iter().map(|(k, v)| (*k, v.as_slice())).collect();
        let release = VerifiedRelease::verify(root, &borrowed, &limits)?;
        Ok(Package { release, blobs })
    }
    /// Resolve against verified disk content and process-only enable overrides.
    /// Loading is explicitly bounded across releases; this cold operation is not
    /// intended to run per frame. An application may retain the immutable result.
    pub fn resolve(
        &self,
        request: &resolution::ResolutionRequest,
        overrides: &[(PluginId, bool)],
    ) -> Result<resolution::ResolutionOutcome, Error> {
        Ok(self.resolve_authoring(request, overrides)?.outcome)
    }

    /// Resolve and project selected component schemas from the same verified
    /// bytes. No second inventory read, provider-name fallback, guest execution,
    /// document edit or mutation of persistent enablement occurs.
    pub fn resolve_authoring(
        &self,
        request: &resolution::ResolutionRequest,
        overrides: &[(PluginId, bool)],
    ) -> Result<AuthoringResolution, Error> {
        let index = self.index()?;
        if index.revision != request.expected_inventory_revision {
            return Ok(AuthoringResolution {
                outcome: resolution::ResolutionOutcome::StaleRevision {
                    expected: request.expected_inventory_revision,
                    actual: index.revision,
                },
                schemas: kagami_catalog::SchemaRegistry::new(),
            });
        }
        self.with_inventory(index, overrides, |_, releases, inventory| {
            let outcome = inventory.resolve(request)?;
            let mut schemas = kagami_catalog::SchemaRegistry::new();
            if let resolution::ResolutionOutcome::Resolved { selection, .. } = &outcome {
                for release in releases {
                    for reference in selection
                        .contributions
                        .iter()
                        .filter(|c| c.release == release.id())
                    {
                        if KnownPoint::from_id(&reference.extension_point)
                            == Some(KnownPoint::Components)
                        {
                            schemas.insert(
                                kagami_catalog::ComponentSchema::from_plugin(
                                    release,
                                    &reference.local_id,
                                )
                                .map_err(|_| {
                                    invalid("selected component cannot be projected into authoring")
                                })?,
                            );
                        }
                    }
                }
            }
            Ok(AuthoringResolution { outcome, schemas })
        })
    }

    /// Discover component vocabulary independently, so one dormant contribution
    /// cannot hide unrelated usable components. Each immutable release is read
    /// once, not once per component. Existing non-default pins remain available;
    /// discovery never changes the default or selects executable physics.
    pub fn available_components(
        &self,
        overrides: &[(PluginId, bool)],
    ) -> Result<AvailableComponents, Error> {
        self.with_inventory(self.index()?, overrides, |index, releases, inventory| {
            const MAX_COMPONENTS: usize = 256;
            let count = releases
                .iter()
                .flat_map(|r| &r.root().0.spec.contributions)
                .filter(|c| KnownPoint::from_id(&c.extension_point) == Some(KnownPoint::Components))
                .count();
            if count > MAX_COMPONENTS {
                return Err(Error::new(
                    Code::LimitExceeded,
                    "startup component discovery exceeds 256 contributions",
                ));
            }
            let mut schemas = kagami_catalog::SchemaRegistry::new();
            let mut unavailable = Vec::new();
            for (entry, release) in index.releases.0.iter().zip(releases) {
                let enabled = overrides
                    .iter()
                    .find(|(p, _)| p == &entry.plugin_id)
                    .map_or(entry.enabled, |(_, enabled)| *enabled);
                if !enabled {
                    continue;
                }
                for contribution in &release.root().0.spec.contributions {
                    if KnownPoint::from_id(&contribution.extension_point)
                        != Some(KnownPoint::Components)
                    {
                        continue;
                    }
                    let pin = release
                        .contribution_ref(&contribution.local_id)
                        .ok_or_else(|| {
                            invalid("component reference missing from verified release")
                        })?;
                    let request = resolution::ResolutionRequest {
                        expected_inventory_revision: index.revision,
                        roots: vec![pin.clone()],
                        bindings: vec![],
                    };
                    if matches!(
                        inventory.resolve(&request)?,
                        resolution::ResolutionOutcome::Resolved { .. }
                    ) {
                        schemas.insert(
                            kagami_catalog::ComponentSchema::from_plugin(
                                release,
                                &contribution.local_id,
                            )
                            .map_err(|_| invalid("component schema projection failed"))?,
                        );
                    } else {
                        unavailable.push(pin);
                    }
                }
            }
            Ok(AvailableComponents {
                revision: index.revision,
                schemas,
                unavailable,
            })
        })
    }

    fn with_inventory<T>(
        &self,
        index: Index,
        overrides: &[(PluginId, bool)],
        action: impl FnOnce(&Index, &[VerifiedRelease], &resolution::Inventory<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut settings = BTreeMap::new();
        for (plugin, enabled) in overrides {
            if settings.insert(plugin, *enabled).is_some() {
                return Err(invalid(
                    "duplicate or conflicting process-only plugin override",
                ));
            }
            if !index.releases.0.iter().any(|e| &e.plugin_id == plugin) {
                return Err(invalid("process override names an uninstalled plugin"));
            }
        }
        let mut releases = Vec::new();
        let mut total = 0u64;
        for entry in &index.releases.0 {
            // Read root first to reject the aggregate before loading its artifacts.
            let root_bytes = self.dir.child("releases".as_ref(), false)?.read(
                Path::new(&hex(entry.release)),
                Limits::default().max_manifest_bytes,
            )?;
            let root = release_from_cbor(&root_bytes, &Limits::default())?;
            if root.release_id(&Limits::default())? != entry.release {
                return Err(Error::new(
                    Code::IntegrityMismatch,
                    "stored release identity mismatch",
                ));
            }
            for a in &root.0.spec.artifacts {
                total = total
                    .checked_add(a.size_bytes)
                    .filter(|n| *n <= Limits::default().max_declared_bytes)
                    .ok_or_else(|| {
                        Error::new(
                            Code::LimitExceeded,
                            "inventory resolution read budget exceeded",
                        )
                    })?;
            }
            let package = self.package(entry.release)?;
            if package.release.root().0.metadata.plugin_id != entry.plugin_id {
                return Err(Error::new(
                    Code::IntegrityMismatch,
                    "index plugin differs from stored release",
                ));
            }
            releases.push(package.release);
        }
        let entries: Vec<_> = index
            .releases
            .0
            .iter()
            .zip(&releases)
            .map(|(e, release)| resolution::InventoryEntry {
                release,
                enabled: settings.get(&e.plugin_id).copied().unwrap_or(e.enabled),
                is_default: e.is_default,
            })
            .collect();
        let inventory = resolution::Inventory::new(
            index.revision,
            &entries,
            resolution::ResolutionLimits::default(),
        )?;
        action(&index, &releases, &inventory)
    }
}
