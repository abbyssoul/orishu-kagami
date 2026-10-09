//! Pure preparation shared by ordinary acceptance and scientific initialization.
//! Catalog snapshots are explicit inputs; preparation never mints or adopts IDs.
use std::sync::Arc;

use kagami_catalog::{
    CatalogSet, SchemaRegistry,
    materialize::{InstantiationRequest, ObjectCandidate, materialize},
};
use kagami_document::{
    AuthoredValue, ComponentProperties, Experiment, ExperimentCommand, Limits, ObjectSpec,
    VariableSpec, resolve_variables,
};
use orishu_variables::Namespace;

use crate::{InstantiationSpec, SessionRejection};

/// Refuse oversized caller bindings before cloning them into materialization.
///
/// # Errors
/// Returns a structured instantiation bound rejection; performs no catalog lookup.
pub fn check_request(spec: &InstantiationSpec, limits: &Limits) -> Result<(), SessionRejection> {
    if spec.bindings.len() > limits.max_variables {
        return Err(SessionRejection::InstantiationLimit {
            resource: "parameter bindings",
            found: spec.bindings.len(),
            limit: limits.max_variables,
        });
    }
    if let Some(source) = spec
        .bindings
        .values()
        .find(|source| source.len() > limits.max_expression_bytes)
    {
        return Err(SessionRejection::InstantiationLimit {
            resource: "binding expression bytes",
            found: source.len(),
            limit: limits.max_expression_bytes,
        });
    }
    Ok(())
}

/// Materialize one template into ordinary commands against one exact experiment
/// and immutable catalog snapshot. Object-local definitions use the source's real
/// allocation counters, including deleted/undone IDs. No experiment is produced:
/// callers must accept through normal authority validation against that revision.
///
/// A supplied fingerprint is checked by the catalog. A scientific adapter must
/// require one before asynchronous preparation if its UI promised exact content.
///
/// # Errors
/// Returns the catalog's structured availability/fingerprint/expression refusal,
/// document bounds, or a complete-provider-lock composition conflict.
pub fn prepare(
    experiment: &Experiment,
    catalog: &CatalogSet,
    schemas: &SchemaRegistry,
    spec: &InstantiationSpec,
    limits: &Limits,
) -> Result<Vec<ExperimentCommand>, SessionRejection> {
    check_request(spec, limits)?;
    let scope = Namespace::new(format!(
        "objects.object_{}",
        experiment.counters().objects_minted()
    ));
    let snapshot = experiment.snapshot();
    let request = InstantiationRequest {
        identity: spec.template.clone(),
        expected_fingerprint: spec.expected_fingerprint,
        bindings: spec.bindings.clone(),
        object_scope: scope.clone(),
        document_values: resolve_variables(&snapshot, limits)?,
    };
    let candidate = materialize(catalog, schemas, &request)
        .map_err(|e| SessionRejection::Instantiation(Box::new(e)))?;
    let count = candidate
        .definitions
        .len()
        .saturating_add(1)
        .saturating_add(usize::from(candidate.dependencies.is_some()));
    if count > limits.max_commands_per_batch {
        return Err(kagami_document::Rejection::BatchTooLarge {
            found: count,
            limit: limits.max_commands_per_batch,
        }
        .into());
    }
    let mut commands = commands(spec, &scope, &candidate);
    if let Some(incoming) = &candidate.dependencies {
        let lock = match snapshot.dependencies() {
            Some(existing) => {
                let merged = existing
                    .merge(incoming, limits.dependencies)
                    .map_err(kagami_document::dependencies::DependencyError::from)
                    .map_err(kagami_document::Rejection::from)?;
                if merged == **existing {
                    existing.clone()
                } else {
                    Arc::new(merged)
                }
            }
            None => incoming.clone(),
        };
        commands.push(ExperimentCommand::AdoptDependencies(lock));
    }
    Ok(commands)
}

fn commands(
    spec: &InstantiationSpec,
    scope: &Namespace,
    candidate: &ObjectCandidate,
) -> Vec<ExperimentCommand> {
    let mut commands = Vec::with_capacity(candidate.definitions.len() + 1);
    for (name, definition) in &candidate.definitions {
        commands.push(ExperimentCommand::DefineVariable(Box::new(
            VariableSpec::new(name.clone(), definition.source.clone()).in_namespace(scope.clone()),
        )));
    }
    let mut object = ObjectSpec::new(spec.name.clone())
        .with_transform(spec.transform)
        .with_velocity(spec.velocity)
        .from_template(candidate.provenance.clone());
    for component in &candidate.components {
        let mut properties = ComponentProperties::new();
        for (property, value) in &component.properties {
            let value = match value {
                kagami_catalog::ObjectPropertyValue::Quantity { source, .. } => {
                    AuthoredValue::si(source.clone())
                }
                kagami_catalog::ObjectPropertyValue::Boolean(v) => AuthoredValue::Boolean(*v),
                kagami_catalog::ObjectPropertyValue::Text(v) => AuthoredValue::Text(v.clone()),
            };
            properties.insert(property.clone(), value);
        }
        object = object.with_component(component.type_id.clone(), properties);
    }
    commands.push(ExperimentCommand::CreateObject(Box::new(object)));
    commands
}
