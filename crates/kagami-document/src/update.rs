//! The transition: fold a command batch into a candidate experiment.
//!
//! This is the pure core the whole crate exists for. It reads its arguments,
//! returns a value, and touches nothing else — so a transition can be tested
//! as a `(experiment, commands) -> outcome` triple with no authority, no
//! adapter, and no IO standing up around it.
//!
//! # Validate a candidate, adopt on success
//!
//! A batch is applied to a *clone* of the experiment — a few refcount bumps —
//! and the finished candidate is validated in full. Nothing is adopted here at
//! all: the caller receives a [`Candidate`] and decides. A failure at any
//! point returns a [`Rejection`] and the input is untouched, which is how "a
//! batch that fails on its last command changes nothing" is structural rather
//! than a property each command has to preserve.
//!
//! Adoption being the caller's decision is also what lets the document server
//! guard a submission against an expected revision without a second, parallel
//! transition existing to be kept in sync.

use std::sync::Arc;

use kagami_catalog::{ComponentTypeId, SchemaRegistry};
use orishu_variables::VariablesSystem;

use crate::command::ExperimentCommand;
use crate::id::{ExperimentRevision, ObjectId};
use crate::limits::Limits;
use crate::model::{Experiment, ExperimentCheckpoint, ExperimentState};
use crate::object::{ComponentProperties, Object, ObjectComponent, ObjectSpec};
use crate::setup::Setup;
use crate::validate::{
    ComponentPath, PropertyPath, Rejection, resolve_property, validate_governed,
    validate_restorable, validate_structure,
};

/// What one accepted batch did, beyond advancing the revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReport {
    /// The revision the candidate would become.
    pub revision: ExperimentRevision,
    /// Identities minted by this batch, in creation order.
    pub created_objects: Vec<ObjectId>,
    /// Identities removed by this batch, in removal order.
    pub removed_objects: Vec<ObjectId>,
    /// The batch's user-facing name, from [`ExperimentCommand::batch_label`].
    pub label: String,
}

impl CommitReport {
    /// The first identity this batch created, if any.
    ///
    /// Exists so an editor that wants to select what it just made does not
    /// have to reach into the vector and reimplement "the one".
    pub fn first_created(&self) -> Option<ObjectId> {
        self.created_objects.first().copied()
    }
}

/// A validated experiment the caller may adopt, and what adopting it would do.
///
/// Deliberately not adopted on construction: the decision to advance the
/// authoritative revision belongs to whoever owns it.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    experiment: Experiment,
    report: CommitReport,
}

impl Candidate {
    /// The experiment as it would be after adoption.
    pub fn experiment(&self) -> &Experiment {
        &self.experiment
    }

    /// What adopting it would do.
    pub fn report(&self) -> &CommitReport {
        &self.report
    }

    /// Take both, consuming the candidate.
    pub fn adopt(self) -> (Experiment, CommitReport) {
        (self.experiment, self.report)
    }
}

/// Fold a command batch into a validated candidate.
///
/// # Errors
///
/// Returns the first [`Rejection`] encountered. `experiment` is never
/// modified, whether this succeeds or fails.
pub fn update(
    experiment: &Experiment,
    commands: &[ExperimentCommand],
    schemas: &SchemaRegistry,
    limits: &Limits,
) -> Result<Candidate, Rejection> {
    if commands.len() > limits.max_commands_per_batch {
        return Err(Rejection::BatchTooLarge {
            found: commands.len(),
            limit: limits.max_commands_per_batch,
        });
    }

    // K1 has no document variable graph, so every expression must be
    // self-contained; an empty system is what makes a symbol reference an
    // explicit `expression_unresolved` rather than a silent zero. K2 replaces
    // this with the document's own graph, and nothing else on this path
    // changes.
    let variables = VariablesSystem::default();

    let mut state = (*experiment.state).clone();
    let mut counters = experiment.counters;
    let mut created_objects = Vec::new();
    let mut removed_objects = Vec::new();
    let mut touched = Vec::new();

    for command in commands {
        apply(
            &mut state,
            &mut counters,
            &mut created_objects,
            &mut removed_objects,
            &mut touched,
            command,
            schemas,
            &variables,
            limits,
        )?;
    }

    validate_structure(&state, limits)?;
    validate_governed(&state, &touched, schemas)?;

    state.revision = experiment.revision().next();
    let report = CommitReport {
        revision: state.revision,
        created_objects,
        removed_objects,
        label: ExperimentCommand::batch_label(commands),
    };
    Ok(Candidate {
        experiment: Experiment {
            state: Arc::new(state),
            counters,
        },
        report,
    })
}

/// Build a candidate that restores previously captured contents.
///
/// The restored experiment is a *new, later* revision, and the identity
/// counters are `experiment`'s — not the checkpoint's — so undoing a creation
/// frees no identifier.
///
/// The contents are re-checked because the installation may have changed since
/// they were captured, but by [`validate_restorable`](crate::validate)'s
/// weaker rule: contents holding a component whose plugin was uninstalled
/// mid-session still restore, and are reported unavailable exactly as a
/// document loaded on that machine would be. Contents an *installed* schema
/// refuses do not restore.
///
/// # Errors
///
/// Returns a [`Rejection`] if the captured contents cannot be reinstated. The
/// caller's experiment and history are untouched.
pub fn restore(
    experiment: &Experiment,
    checkpoint: &ExperimentCheckpoint,
    schemas: &SchemaRegistry,
    limits: &Limits,
) -> Result<Candidate, Rejection> {
    let mut state = (*checkpoint.0).clone();
    validate_structure(&state, limits)?;
    validate_restorable(&state, schemas)?;

    state.revision = experiment.revision().next();
    let report = CommitReport {
        revision: state.revision,
        created_objects: Vec::new(),
        removed_objects: Vec::new(),
        label: "Restore experiment".to_owned(),
    };
    Ok(Candidate {
        experiment: Experiment {
            state: Arc::new(state),
            counters: experiment.counters,
        },
        report,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "every argument is a distinct input or output of one transition; \
              bundling them into a context struct would hide which of them the \
              function may write"
)]
fn apply(
    state: &mut ExperimentState,
    counters: &mut crate::id::Counters,
    created: &mut Vec<ObjectId>,
    removed: &mut Vec<ObjectId>,
    touched: &mut Vec<ComponentPath>,
    command: &ExperimentCommand,
    schemas: &SchemaRegistry,
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<(), Rejection> {
    match command {
        ExperimentCommand::CreateObject(spec) => {
            let id = counters.next_object();
            let object = build_object(id, spec, schemas, variables, limits)?;
            touched.extend(spec.components.keys().map(|component| ComponentPath {
                object: id,
                component: component.clone(),
            }));
            Arc::make_mut(&mut state.objects).insert(id, object);
            created.push(id);
        }
        ExperimentCommand::RemoveObject(object) => {
            let objects = Arc::make_mut(&mut state.objects);
            if objects.remove(object).is_none() {
                return Err(Rejection::UnknownObject { object: *object });
            }
            removed.push(*object);
        }
        ExperimentCommand::RenameObject { object, name } => {
            object_mut(state, *object)?.name = name.clone();
        }
        ExperimentCommand::SetTransform { object, transform } => {
            object_mut(state, *object)?.transform = *transform;
        }
        ExperimentCommand::SetVelocity { object, velocity } => {
            object_mut(state, *object)?.velocity = *velocity;
        }
        ExperimentCommand::SetShape { object, shape } => {
            object_mut(state, *object)?.shape = *shape;
        }
        ExperimentCommand::AttachComponent {
            object,
            component,
            properties,
        } => {
            let resolved =
                resolve_component(*object, component, properties, schemas, variables, limits)?;
            object_mut(state, *object)?
                .components
                .insert(component.clone(), resolved);
            touched.push(ComponentPath {
                object: *object,
                component: component.clone(),
            });
        }
        ExperimentCommand::DetachComponent { object, component } => {
            let target = object_mut(state, *object)?;
            if target.components.remove(component).is_none() {
                return Err(Rejection::ComponentNotAttached {
                    path: ComponentPath {
                        object: *object,
                        component: component.clone(),
                    },
                });
            }
        }
        ExperimentCommand::SetComponentProperty {
            object,
            component,
            property,
            value,
        } => {
            let path = PropertyPath::new(*object, component.clone(), property.clone());
            let schema =
                schemas
                    .get(component)
                    .ok_or_else(|| Rejection::ComponentTypeNotInstalled {
                        component: component.clone(),
                    })?;
            let property_schema =
                schema
                    .properties
                    .get(property)
                    .ok_or_else(|| Rejection::PropertyNotDeclared {
                        path: PropertyPath::new(*object, component.clone(), property.clone()),
                    })?;
            let resolved = resolve_property(path, property_schema, value, variables, limits)?;
            let version = schema.version;
            let target = object_mut(state, *object)?;
            let attached = target.components.get_mut(component).ok_or_else(|| {
                Rejection::ComponentNotAttached {
                    path: ComponentPath {
                        object: *object,
                        component: component.clone(),
                    },
                }
            })?;
            attached.properties.insert(property.clone(), resolved);
            // The value was checked against the *installed* declaration, and
            // the whole component is checked against it again before this
            // batch is adopted. Leaving the older stamp would record that the
            // values were accepted by a declaration that did not see them.
            attached.schema_version = version;
            touched.push(ComponentPath {
                object: *object,
                component: component.clone(),
            });
        }
        ExperimentCommand::SetDomain(domain) => {
            setup_mut(state).domain = *domain;
        }
        ExperimentCommand::SetTimeStep(time_step) => {
            setup_mut(state).time_step = *time_step;
        }
        ExperimentCommand::SetPluginEnabled { plugin, enabled } => {
            setup_mut(state)
                .plugins
                .set_enabled(plugin.clone(), *enabled);
        }
    }
    Ok(())
}

/// Mutable access to one object, or the rejection naming what was missing.
fn object_mut(state: &mut ExperimentState, object: ObjectId) -> Result<&mut Object, Rejection> {
    Arc::make_mut(&mut state.objects)
        .get_mut(&object)
        .ok_or(Rejection::UnknownObject { object })
}

/// Mutable access to the setup, cloning it only when something writes.
fn setup_mut(state: &mut ExperimentState) -> &mut Setup {
    Arc::make_mut(&mut state.setup)
}

fn build_object(
    id: ObjectId,
    spec: &ObjectSpec,
    schemas: &SchemaRegistry,
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<Object, Rejection> {
    let mut components = std::collections::BTreeMap::new();
    for (type_id, properties) in &spec.components {
        let resolved = resolve_component(id, type_id, properties, schemas, variables, limits)?;
        components.insert(type_id.clone(), resolved);
    }
    Ok(Object {
        name: spec.name.clone(),
        transform: spec.transform,
        velocity: spec.velocity,
        shape: spec.shape,
        components,
    })
}

fn resolve_component(
    object: ObjectId,
    type_id: &ComponentTypeId,
    properties: &ComponentProperties,
    schemas: &SchemaRegistry,
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<ObjectComponent, Rejection> {
    let schema = schemas
        .get(type_id)
        .ok_or_else(|| Rejection::ComponentTypeNotInstalled {
            component: type_id.clone(),
        })?;

    let mut resolved = std::collections::BTreeMap::new();
    for (name, authored) in properties {
        let path = PropertyPath::new(object, type_id.clone(), name.clone());
        let property_schema = schema
            .properties
            .get(name)
            .ok_or_else(|| Rejection::PropertyNotDeclared { path: path.clone() })?;
        resolved.insert(
            name.clone(),
            resolve_property(path, property_schema, authored, variables, limits)?,
        );
    }

    Ok(ObjectComponent {
        schema_version: schema.version,
        properties: resolved,
    })
}
