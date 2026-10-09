//! Component-only provider adoption: declarations and leases, never JIT or fields.
use super::*;
use orishu_plugin::authoring_lock::{LockLimits, SelectionLock};
use std::sync::Arc;

/// Exact revalidated provider intent and component schemas from the same inventory
/// read. Keep this handle until document adoption/reference tracking finishes.
#[derive(Debug)]
pub struct PreparedAuthoringLock {
    revision: u64,
    lock: Arc<SelectionLock>,
    schemas: kagami_catalog::SchemaRegistry,
    _leases: Vec<ReleaseLease>,
}
impl PreparedAuthoringLock {
    /// Inventory revision used for independent revalidation.
    pub fn inventory_revision(&self) -> u64 {
        self.revision
    }
    /// Complete intent; no fields, configuration, state or executable blobs.
    pub fn lock(&self) -> &Arc<SelectionLock> {
        &self.lock
    }
    /// Only component declarations in this verified selected closure. These are
    /// a capability projection, not permission to mutate existing authored data.
    pub fn schemas(&self) -> &kagami_catalog::SchemaRegistry {
        &self.schemas
    }
}

/// Unavailability is a normal structured result, never fallback to default pins.
#[derive(Debug)]
pub enum PrepareAuthoringLockOutcome {
    /// Independently checked complete lock, schemas and exact release leases.
    Ready(Box<PreparedAuthoringLock>),
    /// Stale, disabled, missing or incompatible intent, with bounded options.
    Unresolved(resolution::ResolutionOutcome),
}

impl PluginStore {
    /// Revalidate explicit component-only intent and retain its exact releases.
    /// A shared, nonblocking management lock covers the snapshot, declarations
    /// and lease acquisition. Callers must additionally guard inventory revision
    /// and document incarnation/revision/mode at adoption; this is not acceptance.
    /// The bounded inventory loader verifies installed packages as for ordinary
    /// discovery, but no execution closure, JIT or guest state is created here.
    pub fn prepare_authoring_lock(
        &self,
        lock: Arc<SelectionLock>,
        expected_revision: u64,
        overrides: &[(PluginId, bool)],
        limits: LockLimits,
    ) -> Result<PrepareAuthoringLockOutcome, Error> {
        lock.validate(limits)?;
        if lock
            .selection()
            .roots
            .iter()
            .any(|c| KnownPoint::from_id(&c.extension_point) != Some(KnownPoint::Components))
        {
            return Err(invalid("component authoring lock has a non-component root"));
        }
        let _guard = self.dir.lock("inventory.lock", true)?;
        let index = self.index()?;
        if index.revision != expected_revision {
            return Ok(PrepareAuthoringLockOutcome::Unresolved(
                resolution::ResolutionOutcome::StaleRevision {
                    expected: expected_revision,
                    actual: index.revision,
                },
            ));
        }
        self.with_inventory(index, overrides, |_, releases, inventory| {
            let outcome = inventory.resolve_lock(&lock, expected_revision)?;
            let resolution::ResolutionOutcome::Resolved { .. } = outcome else {
                return Ok(PrepareAuthoringLockOutcome::Unresolved(outcome));
            };
            let mut schemas = kagami_catalog::SchemaRegistry::new();
            let mut required = BTreeSet::new();
            for reference in &lock.selection().contributions {
                required.insert(reference.release);
                if KnownPoint::from_id(&reference.extension_point) == Some(KnownPoint::Components) {
                    let release = releases
                        .iter()
                        .find(|r| r.id() == reference.release)
                        .expect("independently resolved release");
                    schemas.insert(
                        kagami_catalog::ComponentSchema::from_plugin(release, &reference.local_id)
                            .map_err(|_| invalid("component schema projection failed"))?,
                    );
                }
            }
            let leases = self.dir.child("leases".as_ref(), false)?;
            let mut retained = Vec::new();
            for release in required {
                retained.push(ReleaseLease {
                    _lock: leases.lock(&hex(release), true)?,
                    release,
                });
            }
            Ok(PrepareAuthoringLockOutcome::Ready(Box::new(
                PreparedAuthoringLock {
                    revision: expected_revision,
                    lock,
                    schemas,
                    _leases: retained,
                },
            )))
        })
    }
}
