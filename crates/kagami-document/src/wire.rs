//! An explicit, versioned representation of what crosses the document
//! boundary.
//!
//! [`ExperimentCommand`] is an in-process enum, and the crate deliberately
//! keeps it that way: it is free to change shape as plugin-contributed
//! capabilities arrive, and freezing it as a wire format would publish a
//! protocol nobody decided to publish. But an in-process MCP adapter, and
//! later a headless host, still need *some* stable shape to convert through —
//! and "some serde derives happen to exist on the internal types" is not a
//! versioning policy. So this module owns that shape, named, versioned and
//! separate.
//!
//! # What this is not
//!
//! - **Not the experiment file.** Task K4 owns the saved document, its
//!   `apiVersion`, and its recovery protocol. A message here describes one
//!   *command* or one *read*, not a durable artefact.
//! - **Not a network protocol.** ADR 0012 keeps live collaborative editing out
//!   of the first product; publishing a transport, an authentication model, or
//!   a compatibility promise needs that decision reopened, not a serde derive.
//!
//! # Decoding cannot widen anything
//!
//! Every value here decodes through the same constructor a Rust caller must
//! use. Names, vectors, rotations, shapes, domains and time steps revalidate
//! inside `serde` (`try_from`), so a malformed one fails to deserialize at
//! all. The two checks `serde` cannot make on its own are explicit:
//!
//! - an object identity is resolved against a snapshot through
//!   [`ExperimentSnapshot::resolve_object`], because an identity is only
//!   meaningful inside one experiment; and
//! - a unit symbol is looked up in the shared unit table.
//!
//! # Which direction each type converts
//!
//! [`WireCommand`] is *inbound*: it converts both ways, because an adapter
//! submits commands. [`WireSnapshot`] is *outbound* only — it encodes what an
//! adapter may read. Decoding one yields a `WireSnapshot`, never an
//! experiment: contents enter this crate through commands, or (task K4)
//! through the document format, and a second ingress that could assert a
//! resolved magnitude its source does not produce would defeat the point of
//! [`crate::validate`].

use std::collections::BTreeMap;

use kagami_catalog::{ComponentTypeId, PluginId, PropertyName, SchemaVersion, UnitError, quantity};
use orishu_variables::{Name, Namespace};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::command::ExperimentCommand;
use crate::geometry::{ObjectShape, Transform, Velocity};
use crate::id::{ExperimentRevision, ObjectId};
use crate::model::ExperimentSnapshot;
use crate::name::DisplayName;
use crate::object::{AuthoredValue, ComponentProperties, ObjectSpec, PropertyValue};
use crate::setup::{Domain, Setup, TimeStep};
use crate::variable::{VariableId, VariableSpec};

/// The version of the representation in this module.
///
/// Bumped when an existing message's meaning changes, not when a variant is
/// added. Carried on [`WireSnapshot`] so a reader can refuse a message from a
/// future it does not understand rather than misreading it.
pub const WIRE_VERSION: u32 = 1;

/// Why a well-formed message could not be converted into a command.
///
/// Deliberately short: everything expressible as a malformed *value* is
/// already refused while deserializing, by the same constructors production
/// code uses. What is left is what depends on context the message cannot
/// carry.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum WireError {
    /// The message named an object this experiment does not have.
    ///
    /// Either it never existed, or it was removed between the read the caller
    /// composed against and this submission.
    #[error("no object {object} in this experiment")]
    UnknownObject {
        /// The raw identity that was supplied.
        object: u64,
    },
    /// The message named a variable definition this experiment does not have.
    #[error("no variable {variable} in this experiment")]
    UnknownVariable {
        /// The raw identity that was supplied.
        variable: u64,
    },
    /// The message annotated a quantity with a unit the product does not know.
    ///
    /// The shared unit table is the one answer to what a symbol means, so this
    /// reports its refusal rather than restating it.
    #[error(transparent)]
    UnknownUnit {
        /// Which symbol, from the unit table.
        #[from]
        source: UnitError,
    },
}

impl WireError {
    /// A stable identifier for this reason.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownObject { .. } => "unknown_object",
            Self::UnknownVariable { .. } => "unknown_variable",
            Self::UnknownUnit { .. } => "unknown_unit",
        }
    }
}

/// A value as the author wrote it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WireValue {
    /// A dimensioned or dimensionless expression.
    Quantity {
        /// Expression source, exactly as authored.
        expression: String,
        /// The unit symbol its magnitude is written in. Absent means the
        /// magnitude is already canonical SI.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    /// A flag.
    Boolean {
        /// The flag.
        value: bool,
    },
    /// Free-form text.
    Text {
        /// The text.
        value: String,
    },
}

impl WireValue {
    /// Encode an authored value.
    pub fn of(value: &AuthoredValue) -> Self {
        match value {
            AuthoredValue::Quantity { expression, unit } => Self::Quantity {
                expression: expression.clone(),
                unit: unit.map(|unit| unit.symbol().to_owned()),
            },
            AuthoredValue::Boolean(value) => Self::Boolean { value: *value },
            AuthoredValue::Text(value) => Self::Text {
                value: value.clone(),
            },
        }
    }

    /// Decode into an authored value.
    ///
    /// The expression itself is *not* parsed here: whether it resolves, and
    /// whether its dimension matches the property's schema, is
    /// [`crate::validate`]'s decision, and duplicating it would create a
    /// second answer.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::UnknownUnit`] for a unit symbol the shared unit
    /// table does not define.
    pub fn into_authored(self) -> Result<AuthoredValue, WireError> {
        Ok(match self {
            Self::Quantity { expression, unit } => match unit {
                None => AuthoredValue::si(expression),
                Some(symbol) => AuthoredValue::in_unit(expression, *quantity::lookup(&symbol)?),
            },
            Self::Boolean { value } => AuthoredValue::Boolean(value),
            Self::Text { value } => AuthoredValue::Text(value),
        })
    }
}

/// One component and the values authored for it.
///
/// A list entry rather than a map keyed by component type, because a
/// plugin-qualified type is a structured identity and would have to be
/// flattened into a string to be a key — a second spelling of an identity that
/// already has one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireComponentValues {
    /// The plugin-qualified component type.
    pub component: ComponentTypeId,
    /// Its authored property values.
    pub properties: BTreeMap<PropertyName, WireValue>,
}

/// Everything needed to create an object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireObjectSpec {
    /// The human label.
    pub name: DisplayName,
    /// Where it starts. Absent is the origin.
    #[serde(default)]
    pub transform: Transform,
    /// How it starts moving. Absent is at rest.
    #[serde(default)]
    pub velocity: Velocity,
    /// Its extent. Absent is a point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ObjectShape>,
    /// Components to attach at creation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<WireComponentValues>,
}

impl WireObjectSpec {
    /// Encode a spec.
    pub fn of(spec: &ObjectSpec) -> Self {
        Self {
            name: spec.name.clone(),
            transform: spec.transform,
            velocity: spec.velocity,
            shape: spec.shape,
            components: spec
                .components
                .iter()
                .map(|(component, properties)| WireComponentValues {
                    component: component.clone(),
                    properties: properties
                        .iter()
                        .map(|(name, value)| (name.clone(), WireValue::of(value)))
                        .collect(),
                })
                .collect(),
        }
    }

    /// Decode into a spec.
    ///
    /// # Errors
    ///
    /// Returns [`WireError`] for a value the message could not name correctly.
    pub fn into_spec(self) -> Result<ObjectSpec, WireError> {
        let mut spec = ObjectSpec::new(self.name);
        spec.transform = self.transform;
        spec.velocity = self.velocity;
        spec.shape = self.shape;
        for entry in self.components {
            spec.components
                .insert(entry.component, into_properties(entry.properties)?);
        }
        Ok(spec)
    }
}

/// One authoring intent, as a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "command",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WireCommand {
    /// Add an object.
    CreateObject {
        /// What to create. Its identity is minted by the model, never
        /// supplied.
        spec: Box<WireObjectSpec>,
    },
    /// Remove an object and everything attached to it.
    RemoveObject {
        /// Which object.
        object: u64,
    },
    /// Change an object's label.
    RenameObject {
        /// Which object.
        object: u64,
        /// Its new label.
        name: DisplayName,
    },
    /// Move or reorient an object.
    SetTransform {
        /// Which object.
        object: u64,
        /// Its new placement.
        transform: Transform,
    },
    /// Change an object's authored velocity.
    SetVelocity {
        /// Which object.
        object: u64,
        /// Its new velocity.
        velocity: Velocity,
    },
    /// Change an object's extent. Absent makes it a point.
    SetShape {
        /// Which object.
        object: u64,
        /// Its new extent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shape: Option<ObjectShape>,
    },
    /// Attach a component, or replace the values of one already attached.
    AttachComponent {
        /// Which object.
        object: u64,
        /// The component and its authored values.
        #[serde(flatten)]
        values: WireComponentValues,
    },
    /// Detach a component.
    DetachComponent {
        /// Which object.
        object: u64,
        /// The plugin-qualified component type.
        component: ComponentTypeId,
    },
    /// Change one authored property of an attached component.
    SetComponentProperty {
        /// Which object.
        object: u64,
        /// Which of its components.
        component: ComponentTypeId,
        /// Which property of that component.
        property: PropertyName,
        /// Its new authored value.
        value: WireValue,
    },
    /// Define a named value this experiment's expressions may use.
    DefineVariable {
        /// Where the definition lives. Absent is the root namespace.
        #[serde(default, skip_serializing_if = "str::is_empty")]
        namespace: String,
        /// The editable name.
        name: Name,
        /// Expression source, exactly as authored.
        expression: String,
        /// What the author says it is for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Replace a definition's expression.
    SetVariableExpression {
        /// Which definition.
        variable: u64,
        /// Its new expression source.
        expression: String,
    },
    /// Change a definition's name, rewriting the expressions that used it.
    RenameVariable {
        /// Which definition.
        variable: u64,
        /// Its new name.
        name: Name,
    },
    /// Remove a definition.
    RemoveVariable {
        /// Which definition.
        variable: u64,
    },
    /// Change what a definition says it is for.
    SetVariableDescription {
        /// Which definition.
        variable: u64,
        /// The new description, or absent to clear it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Replace the computational region and grid.
    SetDomain {
        /// The new domain.
        domain: Domain,
    },
    /// Replace the fixed simulation time step.
    SetTimeStep {
        /// The new step.
        time_step: TimeStep,
    },
    /// Enable or disable one simulation plugin.
    SetPluginEnabled {
        /// Which plugin.
        plugin: PluginId,
        /// Whether it participates.
        enabled: bool,
    },
}

impl WireCommand {
    /// Encode a command.
    pub fn of(command: &ExperimentCommand) -> Self {
        match command {
            ExperimentCommand::CreateObject(spec) => Self::CreateObject {
                spec: Box::new(WireObjectSpec::of(spec)),
            },
            ExperimentCommand::RemoveObject(object) => Self::RemoveObject {
                object: object.get(),
            },
            ExperimentCommand::RenameObject { object, name } => Self::RenameObject {
                object: object.get(),
                name: name.clone(),
            },
            ExperimentCommand::SetTransform { object, transform } => Self::SetTransform {
                object: object.get(),
                transform: *transform,
            },
            ExperimentCommand::SetVelocity { object, velocity } => Self::SetVelocity {
                object: object.get(),
                velocity: *velocity,
            },
            ExperimentCommand::SetShape { object, shape } => Self::SetShape {
                object: object.get(),
                shape: *shape,
            },
            ExperimentCommand::AttachComponent {
                object,
                component,
                properties,
            } => Self::AttachComponent {
                object: object.get(),
                values: WireComponentValues {
                    component: component.clone(),
                    properties: properties
                        .iter()
                        .map(|(name, value)| (name.clone(), WireValue::of(value)))
                        .collect(),
                },
            },
            ExperimentCommand::DetachComponent { object, component } => Self::DetachComponent {
                object: object.get(),
                component: component.clone(),
            },
            ExperimentCommand::SetComponentProperty {
                object,
                component,
                property,
                value,
            } => Self::SetComponentProperty {
                object: object.get(),
                component: component.clone(),
                property: property.clone(),
                value: WireValue::of(value),
            },
            ExperimentCommand::DefineVariable(spec) => Self::DefineVariable {
                namespace: spec.namespace.as_str().to_owned(),
                name: spec.name.clone(),
                expression: spec.expression.clone(),
                description: spec.description.clone(),
            },
            ExperimentCommand::SetVariableExpression {
                variable,
                expression,
            } => Self::SetVariableExpression {
                variable: variable.get(),
                expression: expression.clone(),
            },
            ExperimentCommand::RenameVariable { variable, name } => Self::RenameVariable {
                variable: variable.get(),
                name: name.clone(),
            },
            ExperimentCommand::RemoveVariable(variable) => Self::RemoveVariable {
                variable: variable.get(),
            },
            ExperimentCommand::SetVariableDescription {
                variable,
                description,
            } => Self::SetVariableDescription {
                variable: variable.get(),
                description: description.clone(),
            },
            ExperimentCommand::SetDomain(domain) => Self::SetDomain { domain: *domain },
            ExperimentCommand::SetTimeStep(time_step) => Self::SetTimeStep {
                time_step: *time_step,
            },
            ExperimentCommand::SetPluginEnabled { plugin, enabled } => Self::SetPluginEnabled {
                plugin: plugin.clone(),
                enabled: *enabled,
            },
        }
    }

    /// Decode into a command, resolving identities against `snapshot`.
    ///
    /// The snapshot is what makes a raw integer an [`ObjectId`]: an identity
    /// is meaningful within one experiment's history, so it is resolved
    /// against a real read rather than constructed from the message. That an
    /// object existed at `snapshot`'s revision is not a promise it still
    /// exists when the command is decided — the authority checks again.
    ///
    /// # Errors
    ///
    /// Returns [`WireError`] for an identity that names no object in
    /// `snapshot`, or a unit the product does not know.
    pub fn into_command(
        self,
        snapshot: &ExperimentSnapshot,
    ) -> Result<ExperimentCommand, WireError> {
        Ok(match self {
            Self::CreateObject { spec } => {
                ExperimentCommand::CreateObject(Box::new(spec.into_spec()?))
            }
            Self::RemoveObject { object } => {
                ExperimentCommand::RemoveObject(resolve(snapshot, object)?)
            }
            Self::RenameObject { object, name } => ExperimentCommand::RenameObject {
                object: resolve(snapshot, object)?,
                name,
            },
            Self::SetTransform { object, transform } => ExperimentCommand::SetTransform {
                object: resolve(snapshot, object)?,
                transform,
            },
            Self::SetVelocity { object, velocity } => ExperimentCommand::SetVelocity {
                object: resolve(snapshot, object)?,
                velocity,
            },
            Self::SetShape { object, shape } => ExperimentCommand::SetShape {
                object: resolve(snapshot, object)?,
                shape,
            },
            Self::AttachComponent { object, values } => ExperimentCommand::AttachComponent {
                object: resolve(snapshot, object)?,
                component: values.component,
                properties: into_properties(values.properties)?,
            },
            Self::DetachComponent { object, component } => ExperimentCommand::DetachComponent {
                object: resolve(snapshot, object)?,
                component,
            },
            Self::SetComponentProperty {
                object,
                component,
                property,
                value,
            } => ExperimentCommand::SetComponentProperty {
                object: resolve(snapshot, object)?,
                component,
                property,
                value: value.into_authored()?,
            },
            Self::DefineVariable {
                namespace,
                name,
                expression,
                description,
            } => {
                let mut spec =
                    VariableSpec::new(name, expression).in_namespace(Namespace::new(namespace));
                spec.description = description;
                ExperimentCommand::DefineVariable(Box::new(spec))
            }
            Self::SetVariableExpression {
                variable,
                expression,
            } => ExperimentCommand::SetVariableExpression {
                variable: resolve_variable(snapshot, variable)?,
                expression,
            },
            Self::RenameVariable { variable, name } => ExperimentCommand::RenameVariable {
                variable: resolve_variable(snapshot, variable)?,
                name,
            },
            Self::RemoveVariable { variable } => {
                ExperimentCommand::RemoveVariable(resolve_variable(snapshot, variable)?)
            }
            Self::SetVariableDescription {
                variable,
                description,
            } => ExperimentCommand::SetVariableDescription {
                variable: resolve_variable(snapshot, variable)?,
                description,
            },
            Self::SetDomain { domain } => ExperimentCommand::SetDomain(domain),
            Self::SetTimeStep { time_step } => ExperimentCommand::SetTimeStep(time_step),
            Self::SetPluginEnabled { plugin, enabled } => {
                ExperimentCommand::SetPluginEnabled { plugin, enabled }
            }
        })
    }
}

/// One component of one object, as a reader sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireComponent {
    /// The plugin-qualified component type.
    pub component: ComponentTypeId,
    /// The schema version these values were checked against.
    pub schema_version: SchemaVersion,
    /// The stored values: retained authored source and its resolved
    /// projection.
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

/// One object, as a reader sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireObject {
    /// Its identity within this experiment.
    pub id: u64,
    /// The human label.
    pub name: DisplayName,
    /// Where it is.
    pub transform: Transform,
    /// How it is moving.
    pub velocity: Velocity,
    /// Its extent. Absent is a point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ObjectShape>,
    /// Its attached components, in component-type order.
    pub components: Vec<WireComponent>,
}

/// An experiment at one revision, as a reader sees it.
///
/// Outbound only. See the module documentation for why decoding one produces a
/// `WireSnapshot` rather than an experiment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSnapshot {
    /// The representation version these contents are written in.
    pub version: u32,
    /// The revision they are.
    pub revision: ExperimentRevision,
    /// Every object, in identity order.
    pub objects: Vec<WireObject>,
    /// The numerical setup and plugin composition.
    pub setup: Setup,
}

impl WireSnapshot {
    /// Encode a read projection.
    pub fn of(snapshot: &ExperimentSnapshot) -> Self {
        Self {
            version: WIRE_VERSION,
            revision: snapshot.revision(),
            objects: snapshot
                .objects()
                .iter()
                .map(|(id, object)| WireObject {
                    id: id.get(),
                    name: object.name.clone(),
                    transform: object.transform,
                    velocity: object.velocity,
                    shape: object.shape,
                    components: object
                        .components
                        .iter()
                        .map(|(component, values)| WireComponent {
                            component: component.clone(),
                            schema_version: values.schema_version,
                            properties: values.properties.clone(),
                        })
                        .collect(),
                })
                .collect(),
            setup: snapshot.setup().clone(),
        }
    }
}

fn resolve_variable(snapshot: &ExperimentSnapshot, variable: u64) -> Result<VariableId, WireError> {
    snapshot
        .resolve_variable(variable)
        .ok_or(WireError::UnknownVariable { variable })
}

fn resolve(snapshot: &ExperimentSnapshot, object: u64) -> Result<ObjectId, WireError> {
    snapshot
        .resolve_object(object)
        .ok_or(WireError::UnknownObject { object })
}

fn into_properties(
    properties: BTreeMap<PropertyName, WireValue>,
) -> Result<ComponentProperties, WireError> {
    properties
        .into_iter()
        .map(|(name, value)| Ok((name, value.into_authored()?)))
        .collect()
}
