//! Rebuilding an experiment from what a document persisted.
//!
//! The one way contents enter this crate other than by command. It exists
//! because a saved document carries things a command cannot: the identities
//! objects and definitions already had, and the counters that stop a re-created
//! object from inheriting a removed one's. Replaying a file as `CreateObject`
//! commands would mint fresh identities and silently rebind everything keyed to
//! the old ones.
//!
//! # What it will not accept
//!
//! Being an ingress is exactly why it is narrow:
//!
//! - **No resolved magnitudes.** A record carries the authored expression, not
//!   the number it resolved to last time. Every magnitude is derived here, so a
//!   file cannot inject an SI value its own source does not produce — the
//!   invariant [`crate::validate`] establishes for commands holds for documents
//!   too.
//! - **No identity beyond the counters.** An identity is accepted only if the
//!   counters say it was allocated, so a document cannot name an object the
//!   experiment's own history never minted.
//! - **No schema invention.** A component whose plugin is not installed keeps
//!   every authored field, and its quantities stay
//!   [`PropertyValue::Unresolved`] rather than being priced against a
//!   fabricated declaration.
//!
//! Structural bounds are checked; *capability* is not a load failure. A
//! document referencing a plugin this machine lacks loads, and
//! [`crate::capability`] reports why parts of it cannot be used.

use std::collections::BTreeMap;
use std::sync::Arc;

use kagami_catalog::{ComponentTypeId, PropertyName, SchemaRegistry, SchemaVersion};
use orishu_variables::{Name, Namespace};

use crate::geometry::{ObjectShape, Transform, Velocity};
use crate::id::Counters;
use crate::limits::Limits;
use crate::model::{Experiment, ExperimentState};
use crate::name::DisplayName;
use crate::object::{AuthoredValue, Object, ObjectComponent, PropertyValue};
use crate::setup::Setup;
use crate::validate::{PropertyPath, Rejection, resolve_property, validate_structure};
use crate::variable::{Variable, VariableId};

/// One component of one persisted object.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentRecord {
    /// The schema version the values were checked against when they were
    /// authored. Provenance: it is not used to decide anything here.
    pub schema_version: SchemaVersion,
    /// Authored property values, exactly as the document carried them.
    pub properties: BTreeMap<PropertyName, AuthoredValue>,
}

/// One persisted object, with the identity it had.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectRecord {
    /// The identity this object had when it was saved.
    pub id: u64,
    /// The human label.
    pub name: DisplayName,
    /// Where it is.
    pub transform: Transform,
    /// How it is moving.
    pub velocity: Velocity,
    /// Its extent.
    pub shape: Option<ObjectShape>,
    /// Its attached components.
    pub components: BTreeMap<ComponentTypeId, ComponentRecord>,
    /// The catalog template it was materialised from, if any.
    pub provenance: Option<kagami_catalog::TemplateProvenance>,
}

/// One persisted variable definition, with the identity it had.
#[derive(Clone, Debug, PartialEq)]
pub struct VariableRecord {
    /// The identity this definition had when it was saved.
    pub id: u64,
    /// Where it lives.
    pub namespace: Namespace,
    /// Its editable name.
    pub name: Name,
    /// Expression source, retained verbatim.
    pub expression: String,
    /// What the author says it is for.
    pub description: Option<String>,
}

/// Everything a document persisted about an experiment.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRecord {
    /// How many object identities the experiment had ever allocated.
    pub objects_minted: u64,
    /// How many variable identities it had ever allocated.
    pub variables_minted: u64,
    /// Its objects.
    pub objects: Vec<ObjectRecord>,
    /// Its variable definitions.
    pub variables: Vec<VariableRecord>,
    /// Its numerical setup and plugin composition.
    pub setup: Setup,
}

/// Rebuild an experiment from a decoded document.
///
/// The result is a *new* experiment at [`crate::ExperimentRevision::INITIAL`]:
/// the revision a file carried is provenance about where its contents came
/// from, never permission to rewind a live session's monotonic revision. A
/// caller adopting this into a running authority establishes it as the clean
/// revision and clears history.
///
/// # Errors
///
/// Returns a [`Rejection`] for a document that is structurally invalid: an
/// identity the counters never allocated, a duplicate identity or variable
/// name, a bound exceeded, or an expression that does not resolve against the
/// definitions the same document carried.
pub fn hydrate(
    record: &DocumentRecord,
    schemas: &SchemaRegistry,
    limits: &Limits,
) -> Result<Experiment, Rejection> {
    let counters = Counters::restored(record.objects_minted, record.variables_minted);
    let variables = hydrate_variables(record, limits)?;
    let mut state = ExperimentState {
        revision: crate::ExperimentRevision::INITIAL,
        objects: Arc::new(BTreeMap::new()),
        variables: Arc::new(variables),
        setup: Arc::new(record.setup.clone()),
    };

    // The definitions the document carried are the environment its own
    // expressions were written against, so they are compiled before anything
    // reads them — and every one is evaluated, because none of them has been
    // proven here before.
    let system = crate::update::compile_document_variables(&state, limits)?;

    let mut objects = BTreeMap::new();
    for object in &record.objects {
        let id = counters
            .restored_object(object.id)
            .ok_or(Rejection::UnallocatedIdentity {
                kind: "object",
                identity: object.id,
            })?;
        if objects.contains_key(&id) {
            return Err(Rejection::DuplicateIdentity {
                kind: "object",
                identity: object.id,
            });
        }

        let mut components = BTreeMap::new();
        for (type_id, component) in &object.components {
            let mut properties = BTreeMap::new();
            for (property, authored) in &component.properties {
                let path = PropertyPath::new(id, type_id.clone(), property.clone());
                properties.insert(
                    property.clone(),
                    hydrate_value(&path, authored, schemas, &system, limits)?,
                );
            }
            components.insert(
                type_id.clone(),
                ObjectComponent {
                    schema_version: component.schema_version,
                    properties,
                },
            );
        }

        objects.insert(
            id,
            Object {
                name: object.name.clone(),
                transform: object.transform,
                velocity: object.velocity,
                shape: object.shape,
                components,
                provenance: object.provenance.clone(),
            },
        );
    }
    state.objects = Arc::new(objects);

    validate_structure(&state, limits)?;
    Ok(Experiment {
        state: Arc::new(state),
        counters,
    })
}

fn hydrate_variables(
    record: &DocumentRecord,
    limits: &Limits,
) -> Result<BTreeMap<VariableId, Variable>, Rejection> {
    if record.variables.len() > limits.max_variables {
        return Err(Rejection::TooManyVariables {
            found: record.variables.len(),
            limit: limits.max_variables,
        });
    }
    let counters = Counters::restored(record.objects_minted, record.variables_minted);
    let mut variables = BTreeMap::new();
    for definition in &record.variables {
        let id =
            counters
                .restored_variable(definition.id)
                .ok_or(Rejection::UnallocatedIdentity {
                    kind: "variable",
                    identity: definition.id,
                })?;
        if variables.contains_key(&id) {
            return Err(Rejection::DuplicateIdentity {
                kind: "variable",
                identity: definition.id,
            });
        }
        let value = Variable {
            namespace: definition.namespace.clone(),
            name: definition.name.clone(),
            expression: definition.expression.clone(),
            description: definition.description.clone(),
        };
        if value.expression.len() > limits.max_expression_bytes {
            return Err(Rejection::VariableExpressionTooLong {
                name: value.qualified_name(),
                found: value.expression.len(),
                limit: limits.max_expression_bytes,
            });
        }
        if let Some(text) = &value.description
            && text.len() > limits.max_description_bytes
        {
            return Err(Rejection::DescriptionTooLong {
                name: value.qualified_name(),
                found: text.len(),
                limit: limits.max_description_bytes,
            });
        }
        variables.insert(id, value);
    }
    Ok(variables)
}

/// Price one authored value, or retain it unpriced when nothing can.
///
/// A quantity whose component schema is absent has no dimension and no
/// magnitude *here*, and inventing either would be worse than admitting it.
/// A flag or a label needs no schema to mean what it says, so it is stored
/// directly.
fn hydrate_value(
    path: &PropertyPath,
    authored: &AuthoredValue,
    schemas: &SchemaRegistry,
    variables: &orishu_variables::VariablesSystem,
    limits: &Limits,
) -> Result<PropertyValue, Rejection> {
    let declared = schemas
        .get(&path.component.component)
        .and_then(|schema| schema.properties.get(&path.property));

    match declared {
        Some(schema) => resolve_property(path.clone(), schema, authored, variables, limits),
        None => Ok(match authored {
            AuthoredValue::Quantity { expression, unit } => PropertyValue::Unresolved {
                source: expression.clone(),
                display_unit: unit.map(|unit| unit.symbol().to_owned()),
            },
            AuthoredValue::Boolean(value) => PropertyValue::Boolean(*value),
            AuthoredValue::Text(value) => PropertyValue::Text(value.clone()),
        }),
    }
}
