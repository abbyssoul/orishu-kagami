//! Independent off-window schema/catalog preparation with leases through adoption.
use super::*;
use crate::plugins::{InventoryRevisionGuard, PrepareAuthoringLockOutcome, PreparedAuthoringLock};
use kagami_document::{Experiment, ExperimentCommand, Limits};

pub(super) struct Ready {
    pub catalog: Arc<CatalogSet>,
    pub spec: kagami_session::InstantiationSpec,
    pub commands: Option<Vec<ExperimentCommand>>,
    pub schemas: kagami_catalog::SchemaRegistry,
    pub prepared: Box<PreparedAuthoringLock>,
    pub _availability: InventoryRevisionGuard,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    plugins: ScientificPlugins,
    source: Arc<CatalogSet>,
    experiment: Experiment,
    mut schemas: kagami_catalog::SchemaRegistry,
    spec: kagami_session::InstantiationSpec,
    lock: Arc<SelectionLock>,
    limits: Limits,
    revision: u64,
) -> Result<Ready, EffectError> {
    let prepared = match plugins
        .store
        .prepare_authoring_lock(
            lock.clone(),
            revision,
            &plugins.overrides,
            limits.dependencies,
        )
        .map_err(EffectError::Inventory)?
    {
        PrepareAuthoringLockOutcome::Ready(p) => p,
        PrepareAuthoringLockOutcome::Unresolved(r) => {
            return Err(EffectError::Resolution(Box::new(r)));
        }
    };
    for schema in prepared.schemas().schemas() {
        schemas.insert(schema.clone());
    }
    let catalog = Arc::new(source.revalidate(&schemas, &super::LIMITS));
    let commands = if experiment.snapshot().setup().scientific().is_some() {
        None
    } else {
        let mut commands =
            kagami_session::instantiation::prepare(&experiment, &catalog, &schemas, &spec, &limits)
                .map_err(|e| EffectError::Instantiation(Box::new(e)))?;
        if let Some(ExperimentCommand::AdoptDependencies(required)) = commands.last() {
            if required
                .merge(&lock, limits.dependencies)
                .map_err(kagami_document::dependencies::DependencyError::from)
                .map_err(kagami_document::Rejection::from)
                .map_err(EffectError::Authoring)?
                != *lock
            {
                return Err(EffectError::Selection);
            }
            commands.pop();
        }
        if let Some(previous) = experiment.snapshot().dependencies()
            && previous
                .merge(&lock, limits.dependencies)
                .map_err(kagami_document::dependencies::DependencyError::from)
                .map_err(kagami_document::Rejection::from)
                .map_err(EffectError::Authoring)?
                != *lock
        {
            return Err(EffectError::Selection);
        }
        commands.push(ExperimentCommand::AdoptDependencies(lock));
        Some(commands)
    };
    let availability = plugins
        .store
        .guard_revision(revision)
        .map_err(EffectError::Inventory)?;
    Ok(Ready {
        catalog,
        spec,
        commands,
        schemas,
        prepared,
        _availability: availability,
    })
}
