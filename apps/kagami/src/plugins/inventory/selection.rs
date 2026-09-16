//! Snapshot installed choices into an exact, retained, exportable closure.
use super::*;
use orishu_plugin::selected::{self, CompiledSelection, SelectedKernel, SelectionLimits};
use std::sync::Arc;

/// Exact selected artifacts retained independently of later inventory changes.
/// This is not an accepted experiment or workload. Keep this handle until the
/// owning draft/run releases its pins; it never marks a document dirty by itself.
#[derive(Debug)]
pub struct PreparedSelection {
    revision: u64,
    compiled: CompiledSelection,
    blobs: BTreeMap<ArtifactDigest, Arc<[u8]>>,
    _leases: Vec<ReleaseLease>,
}
impl PreparedSelection {
    /// Inventory revision that governed this explicit selection.
    pub fn inventory_revision(&self) -> u64 {
        self.revision
    }
    /// Shared compiler result, including independent declaration verification.
    pub fn compiled(&self) -> &CompiledSelection {
        &self.compiled
    }
    /// Exactly selected payloads/code and release-root evidence, never unrelated
    /// executables advertised by a selected vocabulary provider's package.
    pub fn blobs(&self) -> &BTreeMap<ArtifactDigest, Arc<[u8]>> {
        &self.blobs
    }
}

/// No guest execution occurs unless selection is ready. Headless and interactive
/// callers receive the same bounded dependency-choice or stale-revision outcome.
#[derive(Debug)]
pub enum PrepareSelectionOutcome {
    /// Complete immutable declaration/code closure with release leases.
    Ready(Box<PreparedSelection>),
    /// The original resolver outcome, with options rather than a guessed choice.
    Unresolved(resolution::ResolutionOutcome),
}

/// Short-lived inventory read guard held only between effect completion and
/// atomic document adoption. Never acquire this before JIT/guest execution.
#[derive(Debug)]
pub struct InventoryRevisionGuard {
    _lock: File,
}

impl PluginStore {
    /// Check availability's exact revision and prevent management mutations
    /// until the caller adopts/discards its completed authoring effect. Locking
    /// is nonblocking; callers must not retry implicitly or hold it during JIT.
    pub fn guard_revision(&self, expected: u64) -> Result<InventoryRevisionGuard, Error> {
        let lock = self.dir.lock("inventory.lock", true)?;
        self.index()?.expected(expected)?;
        Ok(InventoryRevisionGuard { _lock: lock })
    }
    /// Freeze explicit choices from one inventory revision into self-contained
    /// selected inputs. The nonblocking inventory lock covers resolution, reads,
    /// independent closure validation and lease acquisition; no JIT or guest runs
    /// under it. Later disable/removal cannot rewrite these bytes. Adoption of new
    /// intent must separately guard the document and current availability.
    pub fn prepare_selection(
        &self,
        request: &resolution::ResolutionRequest,
        overrides: &[(PluginId, bool)],
        instances: &[SelectedKernel],
        limits: SelectionLimits,
    ) -> Result<PrepareSelectionOutcome, Error> {
        let _lock = self.dir.lock("inventory.lock", false)?;
        let index = self.index()?;
        if index.revision != request.expected_inventory_revision {
            return Ok(PrepareSelectionOutcome::Unresolved(
                resolution::ResolutionOutcome::StaleRevision {
                    expected: request.expected_inventory_revision,
                    actual: index.revision,
                },
            ));
        }
        if instances.len() > limits.kernel_instances {
            return Err(selection_limit());
        }
        self.with_inventory(index, overrides, |index, releases, inventory| {
            let outcome = inventory.resolve(request)?;
            let resolution::ResolutionOutcome::Resolved { selection, .. } = outcome else {
                return Ok(PrepareSelectionOutcome::Unresolved(outcome));
            };
            let required: BTreeSet<_> = selection.contributions.iter().map(|c| c.release).collect();
            let releases: Vec<_> = releases
                .iter()
                .filter(|r| required.contains(&r.id()))
                .collect();
            if releases.len() > limits.releases
                || selection.contributions.len() > limits.contributions
            {
                return Err(selection_limit());
            }
            let mut wanted = BTreeMap::new();
            let mut total = 0u64;
            for c in &selection.contributions {
                let release = releases
                    .iter()
                    .find(|r| r.id() == c.release)
                    .expect("resolved release");
                let payload = &release.payloads()[&c.local_id];
                for digest in std::iter::once(payload.digest())
                    .chain(payload.payload().and_then(Payload::kernel))
                {
                    let a = release
                        .root()
                        .0
                        .spec
                        .artifacts
                        .iter()
                        .find(|a| a.digest == digest)
                        .expect("verified artifact");
                    if let Some(previous) = wanted.insert(digest, a.clone()) {
                        if previous != *a {
                            return Err(invalid("selected artifact descriptors disagree"));
                        }
                    } else {
                        total = total
                            .checked_add(a.size_bytes)
                            .filter(|n| *n <= limits.artifact_bytes)
                            .ok_or_else(selection_limit)?;
                        if wanted.len() > limits.artifacts {
                            return Err(selection_limit());
                        }
                    }
                }
            }
            let dir = self.dir.child("blobs".as_ref(), false)?;
            let mut blobs = BTreeMap::new();
            for (digest, a) in wanted {
                let size = usize::try_from(a.size_bytes).map_err(|_| selection_limit())?;
                blobs.insert(digest, dir.read(Path::new(&hex(digest)), size)?);
            }
            let borrowed = blobs.iter().map(|(d, b)| (*d, b.as_slice())).collect();
            let compiled = selected::compile(&selection, instances, &releases, &borrowed, limits)?;
            // compile independently charges evidence as well as payload/code.
            for (digest, bytes) in compiled.evidence() {
                blobs.entry(*digest).or_insert_with(|| bytes.clone());
            }
            let leases = self.dir.child("leases".as_ref(), false)?;
            let mut retained = Vec::new();
            for release in required {
                retained.push(ReleaseLease {
                    _lock: leases.lock(&hex(release), true)?,
                    release,
                });
            }
            Ok(PrepareSelectionOutcome::Ready(Box::new(
                PreparedSelection {
                    revision: index.revision,
                    compiled,
                    blobs: blobs.into_iter().map(|(d, b)| (d, Arc::from(b))).collect(),
                    _leases: retained,
                },
            )))
        })
    }
}

fn selection_limit() -> Error {
    Error::new(
        Code::LimitExceeded,
        "selected authoring closure budget exceeded",
    )
}
