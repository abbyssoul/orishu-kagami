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
//!
//! # Two phases, because a value may depend on a definition
//!
//! A batch can define a variable and use it in the same breath, and changing
//! one definition reprices every property that reads it. Neither works if a
//! property is priced the moment its command is applied, so the fold runs in
//! two phases:
//!
//! 1. **Structure.** Commands are applied, authored values are *collected*
//!    rather than resolved, and variable definitions reach their final shape.
//! 2. **Evaluation.** The candidate's definitions become one
//!    [`VariablesSystem`], every authored value this batch supplied is priced
//!    against it, and every *stored* value whose definitions changed is
//!    repriced.
//!
//! Only then is the finished candidate validated. A batch is therefore one
//! atomic edit even when it repriced a hundred properties, and a failure
//! anywhere leaves the caller's experiment untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kagami_catalog::{ComponentTypeId, SchemaRegistry};
use orishu_variables::{
    CompiledExpression, ExprEvalError, Name, VariableOptions, VariablesError, VariablesSystem,
    rewrite_symbols,
};

use crate::command::ExperimentCommand;
use crate::id::{Counters, ExperimentRevision, ObjectId};
use crate::limits::Limits;
use crate::model::{Experiment, ExperimentCheckpoint, ExperimentState};
use crate::object::{AuthoredValue, Object, PropertyValue};
use crate::setup::Setup;
use crate::validate::{
    ComponentPath, PropertyPath, Rejection, resolve_property, validate_governed,
    validate_restorable, validate_structure,
};
use crate::variable::{Variable, VariableId, VariableSpec};

/// How much evaluation one accepted batch cost.
///
/// Reported so "an edit evaluates the affected closure, not the document" is
/// something a caller can *observe* rather than a claim in a comment: the
/// counts stay flat as the unaffected part of the graph grows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvaluationWork {
    /// Variable definitions evaluated while deciding this batch.
    pub variables: usize,
    /// Property values priced or repriced by this batch.
    pub properties: usize,
}

/// What one accepted batch did, beyond advancing the revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReport {
    /// The revision the candidate would become.
    pub revision: ExperimentRevision,
    /// Identities minted by this batch, in creation order.
    pub created_objects: Vec<ObjectId>,
    /// Identities removed by this batch, in removal order.
    pub removed_objects: Vec<ObjectId>,
    /// Variable definitions minted by this batch, in creation order.
    pub created_variables: Vec<VariableId>,
    /// What deciding this batch cost.
    pub work: EvaluationWork,
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

    /// The first variable this batch defined, if any.
    pub fn first_variable(&self) -> Option<VariableId> {
        self.created_variables.first().copied()
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

/// One authored value the batch supplied, waiting for the variable graph.
struct PendingValue {
    path: PropertyPath,
    value: AuthoredValue,
}

/// What phase 1 accumulated for phase 2 to act on.
#[derive(Default)]
struct Batch {
    created_objects: Vec<ObjectId>,
    removed_objects: Vec<ObjectId>,
    created_variables: Vec<VariableId>,
    touched: Vec<ComponentPath>,
    pending: Vec<PendingValue>,
    /// Qualified names whose meaning this batch changed. A rename contributes
    /// both the old and the new name: expressions that referred to either have
    /// to be repriced.
    changed_names: BTreeSet<String>,
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

    let mut state = (*experiment.state).clone();
    let mut counters = experiment.counters;
    let mut batch = Batch::default();

    for command in commands {
        apply(
            &mut state,
            &mut counters,
            &mut batch,
            command,
            schemas,
            limits,
        )?;
    }

    let work = evaluate(&mut state, &batch, schemas, limits)?;

    validate_structure(&state, limits)?;
    validate_governed(&state, &batch.touched, schemas)?;

    state.revision = experiment.revision().next();
    let report = CommitReport {
        revision: state.revision,
        created_objects: batch.created_objects,
        removed_objects: batch.removed_objects,
        created_variables: batch.created_variables,
        work,
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
/// Stored values are *not* repriced: they were priced by the definitions
/// captured alongside them, and both come back together.
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
        created_variables: Vec::new(),
        work: EvaluationWork::default(),
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

/// Phase 1: apply one command's structural effect.
fn apply(
    state: &mut ExperimentState,
    counters: &mut Counters,
    batch: &mut Batch,
    command: &ExperimentCommand,
    schemas: &SchemaRegistry,
    limits: &Limits,
) -> Result<(), Rejection> {
    match command {
        ExperimentCommand::CreateObject(spec) => {
            let id = counters.next_object();
            let mut components = BTreeMap::new();
            for (type_id, properties) in &spec.components {
                components.insert(type_id.clone(), empty_component(type_id, schemas)?);
                batch.touched.push(ComponentPath {
                    object: id,
                    component: type_id.clone(),
                });
                for (name, authored) in properties {
                    batch.pending.push(PendingValue {
                        path: PropertyPath::new(id, type_id.clone(), name.clone()),
                        value: authored.clone(),
                    });
                }
            }
            Arc::make_mut(&mut state.objects).insert(
                id,
                Object {
                    name: spec.name.clone(),
                    transform: spec.transform,
                    velocity: spec.velocity,
                    shape: spec.shape,
                    components,
                    provenance: spec.provenance.clone(),
                },
            );
            batch.created_objects.push(id);
        }
        ExperimentCommand::RemoveObject(object) => {
            let objects = Arc::make_mut(&mut state.objects);
            if objects.remove(object).is_none() {
                return Err(Rejection::UnknownObject { object: *object });
            }
            batch.removed_objects.push(*object);
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
            let fresh = empty_component(component, schemas)?;
            object_mut(state, *object)?
                .components
                .insert(component.clone(), fresh);
            batch.touched.push(ComponentPath {
                object: *object,
                component: component.clone(),
            });
            for (name, authored) in properties {
                batch.pending.push(PendingValue {
                    path: PropertyPath::new(*object, component.clone(), name.clone()),
                    value: authored.clone(),
                });
            }
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
            // The value is checked against the *installed* declaration in
            // phase 2, and the whole component again before adoption. Leaving
            // an older stamp would record that the values were accepted by a
            // declaration that did not see them.
            let version = schemas
                .get(component)
                .ok_or_else(|| Rejection::ComponentTypeNotInstalled {
                    component: component.clone(),
                })?
                .version;
            let target = object_mut(state, *object)?;
            let attached = target.components.get_mut(component).ok_or_else(|| {
                Rejection::ComponentNotAttached {
                    path: ComponentPath {
                        object: *object,
                        component: component.clone(),
                    },
                }
            })?;
            attached.schema_version = version;
            batch.touched.push(ComponentPath {
                object: *object,
                component: component.clone(),
            });
            batch.pending.push(PendingValue {
                path: PropertyPath::new(*object, component.clone(), property.clone()),
                value: value.clone(),
            });
        }
        ExperimentCommand::DefineVariable(spec) => {
            define_variable(state, counters, batch, spec, limits)?;
        }
        ExperimentCommand::SetVariableExpression {
            variable,
            expression,
        } => {
            check_expression_length(&qualified(state, *variable)?, expression, limits)?;
            let name = qualified(state, *variable)?;
            variable_mut(state, *variable)?.expression = expression.clone();
            batch.changed_names.insert(name);
        }
        ExperimentCommand::RenameVariable { variable, name } => {
            rename_variable(state, batch, *variable, name)?;
        }
        ExperimentCommand::RemoveVariable(variable) => {
            let name = qualified(state, *variable)?;
            Arc::make_mut(&mut state.variables).remove(variable);
            batch.changed_names.insert(name);
        }
        ExperimentCommand::SetVariableDescription {
            variable,
            description,
        } => {
            if let Some(text) = description
                && text.len() > limits.max_description_bytes
            {
                return Err(Rejection::DescriptionTooLong {
                    name: qualified(state, *variable)?,
                    found: text.len(),
                    limit: limits.max_description_bytes,
                });
            }
            variable_mut(state, *variable)?.description = description.clone();
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

fn define_variable(
    state: &mut ExperimentState,
    counters: &mut Counters,
    batch: &mut Batch,
    spec: &VariableSpec,
    limits: &Limits,
) -> Result<(), Rejection> {
    let qualified_name = spec.qualified_name();
    check_expression_length(&qualified_name, &spec.expression, limits)?;
    if let Some(text) = &spec.description
        && text.len() > limits.max_description_bytes
    {
        return Err(Rejection::DescriptionTooLong {
            name: qualified_name,
            found: text.len(),
            limit: limits.max_description_bytes,
        });
    }
    if state.variables.len() >= limits.max_variables {
        return Err(Rejection::TooManyVariables {
            found: state.variables.len() + 1,
            limit: limits.max_variables,
        });
    }
    if name_is_taken(state, &qualified_name, None) {
        return Err(Rejection::VariableNameTaken {
            name: qualified_name,
        });
    }

    let id = counters.next_variable();
    Arc::make_mut(&mut state.variables).insert(
        id,
        Variable {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            expression: spec.expression.clone(),
            description: spec.description.clone(),
        },
    );
    batch.created_variables.push(id);
    batch.changed_names.insert(qualified_name);
    Ok(())
}

/// Rename a definition and rewrite every expression that named the old one.
///
/// The rewrite is what makes a rename *refactoring* rather than breakage: the
/// identity never moves, and every source that referred to it by name is
/// updated in the same validated edit — or the whole batch is refused.
fn rename_variable(
    state: &mut ExperimentState,
    batch: &mut Batch,
    variable: VariableId,
    name: &Name,
) -> Result<(), Rejection> {
    let existing = state
        .variables
        .get(&variable)
        .ok_or(Rejection::UnknownVariable { variable })?;
    let old = existing.qualified_name();
    let new = existing.namespace.qualified(name).to_string();
    if old == new {
        return Ok(());
    }
    if name_is_taken(state, &new, Some(variable)) {
        return Err(Rejection::VariableNameTaken { name: new });
    }

    let renames = BTreeMap::from([(old.clone(), new.clone())]);
    let variables = Arc::make_mut(&mut state.variables);
    for (id, definition) in variables.iter_mut() {
        if *id == variable {
            definition.name = name.clone();
        } else {
            definition.expression = rewrite_symbols(&definition.expression, &renames);
        }
    }
    for object in Arc::make_mut(&mut state.objects).values_mut() {
        for component in object.components.values_mut() {
            for value in component.properties.values_mut() {
                if let PropertyValue::Quantity { source, .. } = value {
                    *source = rewrite_symbols(source, &renames);
                }
            }
        }
    }

    batch.changed_names.insert(old);
    batch.changed_names.insert(new);
    Ok(())
}

/// Phase 2: price everything this batch made or invalidated.
fn evaluate(
    state: &mut ExperimentState,
    batch: &Batch,
    schemas: &SchemaRegistry,
    limits: &Limits,
) -> Result<EvaluationWork, Rejection> {
    let affected = affected_names(state, &batch.changed_names, limits);
    let (variables, evaluated) = compile_variables(state, &affected, limits)?;
    let mut work = EvaluationWork {
        variables: evaluated,
        properties: 0,
    };

    // Everything this batch authored, priced against the finished graph.
    let mut resolved = Vec::with_capacity(batch.pending.len());
    for pending in &batch.pending {
        // A later command in the same batch may have removed what an earlier
        // one wrote to; there is nothing left to price.
        if !path_exists(state, &pending.path) {
            continue;
        }
        let value = resolve_authored(&pending.path, &pending.value, schemas, &variables, limits)?;
        resolved.push((pending.path.clone(), value));
        work.properties += 1;
    }

    // Everything already stored that this batch disturbed: a value whose
    // definitions moved under it, and any value in a component the batch
    // touched. The second is what makes an ordinary edit price a value that
    // was loaded without its plugin — see `PropertyValue::Unresolved`.
    let touched: BTreeSet<_> = batch.touched.iter().cloned().collect();
    if !affected.is_empty() || !touched.is_empty() {
        let authored: BTreeSet<_> = batch
            .pending
            .iter()
            .map(|pending| pending.path.clone())
            .collect();
        for (path, source, stored) in stored_values(state) {
            if authored.contains(&path) {
                continue;
            }
            let disturbed = touched.contains(&path.component)
                || source.is_some_and(|text| references_any(text, &affected));
            if !disturbed {
                continue;
            }
            let value = resolve_authored(&path, &stored, schemas, &variables, limits)?;
            resolved.push((path, value));
            work.properties += 1;
        }
    }

    for (path, value) in resolved {
        write_property(state, &path, value);
    }
    Ok(work)
}

/// Build the candidate's variable graph, and prove the affected part of it
/// resolves.
///
/// Every definition is *declared*, because any expression may reference any of
/// them. Only the affected ones are *evaluated*: a definition the batch did
/// not change, and none of whose inputs moved, resolved at the previous
/// revision and resolves identically now. That is what keeps the cost of an
/// edit proportional to what it disturbed rather than to the document.
///
/// Cycles, unknown names and dimension errors therefore surface while the
/// batch is being decided rather than when something later reads them, and a
/// batch that would leave the graph unevaluable is refused whole.
/// Every document variable, resolved to its canonical SI magnitude.
///
/// What a consumer outside this crate needs when it has to hand resolved
/// values to something that cannot read the graph itself — catalog
/// instantiation captures them as literals, because instantiation
/// materialises rather than links (ADR 0018).
///
/// # Errors
///
/// Returns the [`Rejection`] naming the first definition that does not
/// resolve.
pub fn resolve_variables(
    snapshot: &crate::model::ExperimentSnapshot,
    limits: &Limits,
) -> Result<BTreeMap<String, f64>, Rejection> {
    let state = snapshot.state();
    let system = compile_document_variables(state, limits)?;
    let mut values = BTreeMap::new();
    for definition in state.variables.values() {
        let name = definition.qualified_name();
        let handle = system
            .lookup(&orishu_variables::FQName::new(
                definition.namespace.clone(),
                definition.name.clone(),
            ))
            .expect("every definition was just compiled");
        let value = system
            .value(handle)
            .map_err(|error| variable_error(&name, error))?;
        values.insert(name, value.magnitude());
    }
    Ok(values)
}

/// Compile and prove *every* definition a document carried.
///
/// [`crate::hydrate`]'s counterpart to the incremental path below. Nothing in
/// a freshly decoded document has been proven here before, so "unchanged since
/// the last accepted revision" — the argument that lets an edit evaluate only
/// what it disturbed — does not apply to it.
pub(crate) fn compile_document_variables(
    state: &ExperimentState,
    limits: &Limits,
) -> Result<VariablesSystem, Rejection> {
    let all = state
        .variables
        .values()
        .map(Variable::qualified_name)
        .collect();
    compile_variables(state, &all, limits).map(|(system, _)| system)
}

fn compile_variables(
    state: &ExperimentState,
    affected: &BTreeSet<String>,
    limits: &Limits,
) -> Result<(VariablesSystem, usize), Rejection> {
    if state.variables.len() > limits.max_variables {
        return Err(Rejection::TooManyVariables {
            found: state.variables.len(),
            limit: limits.max_variables,
        });
    }

    let mut system = VariablesSystem::default();
    let mut handles = Vec::new();
    for definition in state.variables.values() {
        let name = definition.qualified_name();
        let compiled = CompiledExpression::parse(&definition.expression).map_err(|source| {
            Rejection::VariableInvalid {
                name: name.clone(),
                source: Box::new(source),
            }
        })?;
        let handle = system
            .define(
                &definition.namespace,
                definition.name.clone(),
                compiled,
                VariableOptions::default(),
            )
            .map_err(|error| variable_error(&name, error))?;
        if affected.contains(&name) {
            handles.push((name, handle));
        }
    }

    for (name, handle) in &handles {
        system
            .value(*handle)
            .map_err(|error| variable_error(name, error))?;
    }
    Ok((system, handles.len()))
}

/// The closure of definition names whose value this batch may have changed.
///
/// A definition that reads a changed name is itself changed, transitively.
/// The fixpoint is bounded by the number of definitions, because each pass
/// that changes nothing stops it.
fn affected_names(
    state: &ExperimentState,
    changed: &BTreeSet<String>,
    limits: &Limits,
) -> BTreeSet<String> {
    let mut affected = changed.clone();
    if affected.is_empty() {
        return affected;
    }
    for _ in 0..limits.max_variables {
        let mut grew = false;
        for definition in state.variables.values() {
            let name = definition.qualified_name();
            if affected.contains(&name) {
                continue;
            }
            if references_any(&definition.expression, &affected) {
                affected.insert(name);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    affected
}

/// `true` when `source` names any symbol in `names`.
fn references_any(source: &str, names: &BTreeSet<String>) -> bool {
    CompiledExpression::parse(source).is_ok_and(|compiled| {
        compiled
            .variables()
            .iter()
            .any(|symbol| names.contains(symbol))
    })
}

/// Every stored value, with its expression source and the authored value that
/// would reproduce it.
///
/// A stored value retains everything it was made from, so repricing it is
/// resolving that same intent again — there is no second form of it to keep in
/// step. The source is `None` for a value that has none (a flag, a label), so
/// a caller can skip the ones no definition can disturb.
fn stored_values(state: &ExperimentState) -> Vec<(PropertyPath, Option<&str>, AuthoredValue)> {
    let mut out = Vec::new();
    for (object, contents) in state.objects.iter() {
        for (type_id, component) in &contents.components {
            for (property, value) in &component.properties {
                out.push((
                    PropertyPath::new(*object, type_id.clone(), property.clone()),
                    value.source(),
                    value.authored(),
                ));
            }
        }
    }
    out
}

/// Resolve one authored value against the schema that governs its property.
fn resolve_authored(
    path: &PropertyPath,
    value: &AuthoredValue,
    schemas: &SchemaRegistry,
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<PropertyValue, Rejection> {
    let schema = schemas.get(&path.component.component).ok_or_else(|| {
        Rejection::ComponentTypeNotInstalled {
            component: path.component.component.clone(),
        }
    })?;
    let property_schema = schema
        .properties
        .get(&path.property)
        .ok_or_else(|| Rejection::PropertyNotDeclared { path: path.clone() })?;
    resolve_property(path.clone(), property_schema, value, variables, limits)
}

fn write_property(state: &mut ExperimentState, path: &PropertyPath, value: PropertyValue) {
    if let Some(object) = Arc::make_mut(&mut state.objects).get_mut(&path.component.object)
        && let Some(component) = object.components.get_mut(&path.component.component)
    {
        component.properties.insert(path.property.clone(), value);
    }
}

fn path_exists(state: &ExperimentState, path: &PropertyPath) -> bool {
    state
        .objects
        .get(&path.component.object)
        .is_some_and(|object| object.components.contains_key(&path.component.component))
}

/// A component with its installed schema version and no values yet.
fn empty_component(
    type_id: &ComponentTypeId,
    schemas: &SchemaRegistry,
) -> Result<crate::object::ObjectComponent, Rejection> {
    let schema = schemas
        .get(type_id)
        .ok_or_else(|| Rejection::ComponentTypeNotInstalled {
            component: type_id.clone(),
        })?;
    Ok(crate::object::ObjectComponent {
        schema_version: schema.version,
        properties: BTreeMap::new(),
    })
}

fn name_is_taken(
    state: &ExperimentState,
    qualified_name: &str,
    except: Option<VariableId>,
) -> bool {
    state.variables.iter().any(|(id, definition)| {
        Some(*id) != except && definition.qualified_name() == qualified_name
    })
}

fn check_expression_length(name: &str, expression: &str, limits: &Limits) -> Result<(), Rejection> {
    if expression.len() > limits.max_expression_bytes {
        return Err(Rejection::VariableExpressionTooLong {
            name: name.to_owned(),
            found: expression.len(),
            limit: limits.max_expression_bytes,
        });
    }
    Ok(())
}

fn qualified(state: &ExperimentState, variable: VariableId) -> Result<String, Rejection> {
    state
        .variables
        .get(&variable)
        .map(Variable::qualified_name)
        .ok_or(Rejection::UnknownVariable { variable })
}

fn variable_mut(
    state: &mut ExperimentState,
    variable: VariableId,
) -> Result<&mut Variable, Rejection> {
    Arc::make_mut(&mut state.variables)
        .get_mut(&variable)
        .ok_or(Rejection::UnknownVariable { variable })
}

/// Report an engine failure as the refusal it causes, naming the definition.
fn variable_error(name: &str, error: VariablesError) -> Rejection {
    match error {
        VariablesError::Parsing(source) => Rejection::VariableInvalid {
            name: name.to_owned(),
            source: Box::new(source),
        },
        VariablesError::Eval(source) => Rejection::VariableUnresolved {
            name: name.to_owned(),
            source,
        },
        VariablesError::ShadowsUnit { name: unit } => Rejection::VariableShadowsUnit {
            name: unit.to_string(),
        },
        VariablesError::DuplicateName { .. } => Rejection::VariableNameTaken {
            name: name.to_owned(),
        },
        VariablesError::InvalidName(_) | VariablesError::UnknownHandle => {
            Rejection::VariableUnresolved {
                name: name.to_owned(),
                source: ExprEvalError::UnknownVariable(name.to_owned()),
            }
        }
    }
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
