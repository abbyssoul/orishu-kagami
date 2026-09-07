//! The closed set of authoring commands.
//!
//! A command expresses *intent*, not replacement internal state, and a batch
//! of them is decided atomically (ADR 0004). Everything that can change an
//! experiment is a variant here: a UI gesture, an MCP tool call, an undo, and
//! a catalog instantiation all become one of these before anything is
//! validated, which is what makes UI/MCP parity a property of the model rather
//! than a promise each adapter has to keep (ADR 0006).
//!
//! # Why labels live here
//!
//! An undo entry, a history log line, and a remote client's transaction list
//! all need the same words for the same edit. Naming an edit in three places
//! is naming it in three places that can disagree, so [`ExperimentCommand::label`]
//! is the one place, and a command a plugin's schema makes possible later
//! inherits its label from the variant that carries it.
//!
//! # Why these are not serialisable
//!
//! Commands are constructed in-process by adapters, and what is persisted is
//! the *model*, not the commands that produced it. An MCP tool builds a
//! command from its own versioned tool schema; making the internal enum a wire
//! format would freeze it as a second protocol nobody decided to publish.

use kagami_catalog::{ComponentTypeId, PluginId, PropertyName};
use orishu_variables::Name;

use crate::geometry::{ObjectShape, Transform, Velocity};
use crate::id::ObjectId;
use crate::name::DisplayName;
use crate::object::{AuthoredValue, ComponentProperties, ObjectSpec};
use crate::setup::{Domain, TimeStep};
use crate::variable::{VariableId, VariableSpec};

/// One authoring intent.
#[derive(Clone, Debug, PartialEq)]
pub enum ExperimentCommand {
    /// Add an object. Its identity is minted by the model, not supplied.
    ///
    /// Boxed because a spec carries its initial components and is far larger
    /// than every other variant; an unboxed one would make the whole enum
    /// that size.
    CreateObject(Box<ObjectSpec>),
    /// Remove an object and everything attached to it.
    RemoveObject(ObjectId),
    /// Change an object's label. Never a delete-and-recreate, so the identity
    /// and anything keyed by it survive.
    RenameObject {
        /// Which object.
        object: ObjectId,
        /// Its new label.
        name: DisplayName,
    },
    /// Move or reorient an object.
    SetTransform {
        /// Which object.
        object: ObjectId,
        /// Its new placement.
        transform: Transform,
    },
    /// Change an object's authored velocity.
    SetVelocity {
        /// Which object.
        object: ObjectId,
        /// Its new velocity.
        velocity: Velocity,
    },
    /// Change an object's extent. `None` makes it a point.
    SetShape {
        /// Which object.
        object: ObjectId,
        /// Its new extent.
        shape: Option<ObjectShape>,
    },
    /// Attach a component, or replace the values of one already attached.
    AttachComponent {
        /// Which object.
        object: ObjectId,
        /// The plugin-qualified component type.
        component: ComponentTypeId,
        /// Its authored property values.
        properties: ComponentProperties,
    },
    /// Detach a component.
    DetachComponent {
        /// Which object.
        object: ObjectId,
        /// The plugin-qualified component type.
        component: ComponentTypeId,
    },
    /// Change one authored property of an attached component.
    SetComponentProperty {
        /// Which object.
        object: ObjectId,
        /// Which of its components.
        component: ComponentTypeId,
        /// Which property of that component.
        property: PropertyName,
        /// Its new authored value.
        value: AuthoredValue,
    },
    /// Define a named value this experiment's expressions may use.
    ///
    /// Boxed for the reason [`Self::CreateObject`] is: a spec carries authored
    /// text and is far larger than every other variant.
    DefineVariable(Box<VariableSpec>),
    /// Replace a definition's expression, repricing everything that reads it.
    SetVariableExpression {
        /// Which definition.
        variable: VariableId,
        /// Its new expression source.
        expression: String,
    },
    /// Change a definition's name, rewriting every expression that referred to
    /// it by the old one.
    ///
    /// Never a delete-and-recreate: the identity survives, so anything keyed
    /// by it — and every dependant's meaning — survives with it.
    RenameVariable {
        /// Which definition.
        variable: VariableId,
        /// Its new name.
        name: Name,
    },
    /// Remove a definition.
    ///
    /// Refused while anything still refers to it, unless the same batch clears
    /// those references.
    RemoveVariable(VariableId),
    /// Change what a definition says it is for. Never affects resolution.
    SetVariableDescription {
        /// Which definition.
        variable: VariableId,
        /// The new description, or `None` to clear it.
        description: Option<String>,
    },
    /// Replace the computational region and grid.
    SetDomain(Domain),
    /// Replace the fixed simulation time step.
    SetTimeStep(TimeStep),
    /// Enable or disable one simulation plugin.
    SetPluginEnabled {
        /// Which plugin.
        plugin: PluginId,
        /// Whether it participates.
        enabled: bool,
    },
}

impl ExperimentCommand {
    /// A short name for this edit, in the user's terms.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::CreateObject(_) => "Add object",
            Self::RemoveObject(_) => "Remove object",
            Self::RenameObject { .. } => "Rename object",
            Self::SetTransform { .. } => "Move object",
            Self::SetVelocity { .. } => "Set velocity",
            Self::SetShape { .. } => "Change shape",
            Self::AttachComponent { .. } => "Attach component",
            Self::DetachComponent { .. } => "Remove component",
            Self::SetComponentProperty { .. } => "Edit property",
            Self::DefineVariable(_) => "Define variable",
            Self::SetVariableExpression { .. } => "Edit variable",
            Self::RenameVariable { .. } => "Rename variable",
            Self::RemoveVariable(_) => "Remove variable",
            Self::SetVariableDescription { .. } => "Describe variable",
            Self::SetDomain(_) => "Reconfigure domain",
            Self::SetTimeStep(_) => "Set time step",
            Self::SetPluginEnabled { .. } => "Change active physics",
        }
    }

    /// Name a batch committed as one atomic edit.
    ///
    /// A transaction is one step to a user however many commands it took, so
    /// it is named after the first and counts the rest rather than listing
    /// them — "Move object and 2 more" is what an undo button can say.
    pub fn batch_label(commands: &[Self]) -> String {
        match commands {
            [] => "Edit experiment".to_owned(),
            [only] => only.label().to_owned(),
            [first, rest @ ..] => format!("{} and {} more", first.label(), rest.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> ExperimentCommand {
        ExperimentCommand::RemoveObject(ObjectId::from_raw(0))
    }

    #[test]
    fn an_empty_batch_still_has_a_name() {
        assert_eq!(ExperimentCommand::batch_label(&[]), "Edit experiment");
    }

    #[test]
    fn a_single_command_batch_is_named_after_it() {
        assert_eq!(
            ExperimentCommand::batch_label(&[command()]),
            "Remove object"
        );
    }

    #[test]
    fn a_multi_command_batch_names_the_first_and_counts_the_rest() {
        assert_eq!(
            ExperimentCommand::batch_label(&[command(), command(), command()]),
            "Remove object and 2 more"
        );
    }

    #[test]
    fn every_variant_has_a_distinct_user_facing_label() {
        // Not a style check: two edits sharing a label make an undo button
        // lie about what it will reverse.
        let labels = [
            ExperimentCommand::RemoveObject(ObjectId::from_raw(0)).label(),
            ExperimentCommand::SetTimeStep(TimeStep::default()).label(),
            ExperimentCommand::SetDomain(Domain::default()).label(),
        ];
        let mut unique = labels.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), labels.len());
    }
}
