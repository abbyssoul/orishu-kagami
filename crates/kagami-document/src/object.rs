//! An object is a named pose in space; everything physical is a component.
//!
//! ADR 0018 calls an object an *entity* and gives simulation plugins the
//! component and property schemas that may be attached to it. That is the
//! whole model: one authoring action creates an object with a placement and no
//! physics, and components are attached and detached afterwards. There is no
//! species enum, no `if electron`, and nothing here branches on a component's
//! name — a component becomes authorable the moment its schema is registered.
//!
//! # Two forms of a value, on purpose
//!
//! [`AuthoredValue`] is what a *command* carries: what the user typed.
//! [`PropertyValue`] is what the model *stores*: that same source, retained
//! verbatim as the author's intent (ADR 0005), together with the canonical SI
//! magnitude it resolved to and the dimension its schema declared.
//!
//! The resolved magnitude is derived, and it is derived *here* — the only way
//! a value enters an experiment is by being validated out of an
//! `AuthoredValue`, so no caller can assert a magnitude its source does not
//! produce. Persisting the resolved number is a cache, never the intent.
//!
//! # Who decides an object's motion
//!
//! Nothing here does, and no field records it. ADR 0020 makes the presence of
//! a compatible Dynamics component the *sole* opt-in to run-time integration:
//! an object carrying one is integrated, and an object without one keeps its
//! authored pose and velocity as kinematic initial state for the whole run.
//! A second, persisted motion-authority flag beside that would be a second
//! owner of the same decision, and the two could disagree. Editing an
//! object's initial pose or velocity is an ordinary Authoring-mode command
//! either way.
//!
//! # Not here
//!
//! Visibility. ADR 0012 lists it alongside camera and selection as
//! presentation state, so hiding an object is client-local and neither dirties
//! the document nor advances a revision.

use std::collections::BTreeMap;

use kagami_catalog::{ComponentTypeId, Dimension, PropertyName, SchemaVersion, Unit};
use serde::{Deserialize, Serialize};

use crate::geometry::{ObjectShape, Transform, Velocity};
use crate::name::DisplayName;

/// A value as the author wrote it, carried by a command.
///
/// The unit is named *beside* the expression rather than written inside it
/// (`{expression: "6.9634e5", unit: km}`), matching the catalog's authored
/// quantities. That is a deliberate, temporary limitation of the shared
/// expression grammar, not a preference: see [`PropertyValue::Quantity`].
#[derive(Clone, Debug, PartialEq)]
pub enum AuthoredValue {
    /// A dimensioned or dimensionless expression.
    Quantity {
        /// Expression source, exactly as authored.
        expression: String,
        /// The unit its magnitude is written in. `None` means the magnitude
        /// is already canonical SI in the dimension the schema declares.
        unit: Option<Unit>,
    },
    /// A flag.
    Boolean(bool),
    /// Free-form text.
    Text(String),
}

impl AuthoredValue {
    /// A quantity authored in canonical SI, with no unit annotation.
    pub fn si(expression: impl Into<String>) -> Self {
        Self::Quantity {
            expression: expression.into(),
            unit: None,
        }
    }

    /// A quantity authored in `unit`.
    pub fn in_unit(expression: impl Into<String>, unit: Unit) -> Self {
        Self::Quantity {
            expression: expression.into(),
            unit: Some(unit),
        }
    }

    /// A short name for this value's kind, for a diagnostic that has to
    /// report what was supplied against what a schema asked for.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Quantity { .. } => "quantity",
            Self::Boolean(_) => "boolean",
            Self::Text(_) => "text",
        }
    }
}

/// A stored property value: retained intent plus its resolved projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PropertyValue {
    /// A dimensioned physical value.
    ///
    /// # What the dimension check does and does not buy
    ///
    /// The dimension is *declared* by the property's schema and checked
    /// against the authored unit, not *inferred* through the expression's
    /// arithmetic. `mass / radius` annotated `kg` is therefore accepted here.
    /// Derived-dimension inference belongs to the shared variables subsystem,
    /// which owns the one dimension-aware value layer over the shared grammar;
    /// this crate will delegate to it rather than grow a second one. The
    /// catalog records the identical limitation for the identical reason.
    Quantity {
        /// Expression source, retained verbatim. This is the intent.
        source: String,
        /// The unit symbol the magnitude was authored in, kept so an editor
        /// can redisplay the value the way it was written. `None` means
        /// canonical SI. Presentation of intent, not a second magnitude.
        display_unit: Option<String>,
        /// Canonical SI magnitude, derived from `source` by this crate.
        si_value: f64,
        /// The dimension the property's schema declared.
        dimension: Dimension,
    },
    /// A flag.
    Boolean(bool),
    /// Free-form text.
    Text(String),
}

impl PropertyValue {
    /// A short name for this value's kind, for diagnostics.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Quantity { .. } => "quantity",
            Self::Boolean(_) => "boolean",
            Self::Text(_) => "text",
        }
    }

    /// The canonical SI magnitude, for a quantity.
    pub fn si_value(&self) -> Option<f64> {
        match self {
            Self::Quantity { si_value, .. } => Some(*si_value),
            _ => None,
        }
    }

    /// The retained authored source, for a quantity.
    pub fn source(&self) -> Option<&str> {
        match self {
            Self::Quantity { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// One component attached to an object.
///
/// The component's type is the key it is stored under, so it is not repeated
/// here: an object carries at most one component of a given type, and a map
/// that could disagree with its own values would be a second source of truth.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectComponent {
    /// The schema version these values were checked against, recorded so a
    /// later reader can tell which declaration accepted them.
    pub schema_version: SchemaVersion,
    /// Authored property values, keyed by property name.
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

/// An object in the experiment.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// The human label. Not an identity: two objects may share one.
    pub name: DisplayName,
    /// Where it is.
    pub transform: Transform,
    /// How it is moving. Initial state a Dynamics component integrates from,
    /// and the velocity the object keeps for the whole run without one.
    pub velocity: Velocity,
    /// Its extent. `None` is a point.
    pub shape: Option<ObjectShape>,
    /// Attached components, keyed by their plugin-qualified type.
    pub components: BTreeMap<ComponentTypeId, ObjectComponent>,
}

impl Object {
    /// The values of `component`, if this object carries it.
    pub fn component(&self, component: &ComponentTypeId) -> Option<&ObjectComponent> {
        self.components.get(component)
    }
}

/// The properties of one component, as a command supplies them.
pub type ComponentProperties = BTreeMap<PropertyName, AuthoredValue>;

/// Everything needed to create an object, in one value.
///
/// A builder rather than a wide constructor so a creation command cannot be
/// assembled half-specified, and so adding an authored field later does not
/// break every existing call site.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectSpec {
    /// The human label.
    pub name: DisplayName,
    /// Where it starts.
    pub transform: Transform,
    /// How it starts moving.
    pub velocity: Velocity,
    /// Its extent; `None` is a point.
    pub shape: Option<ObjectShape>,
    /// Components to attach at creation, keyed by type.
    pub components: BTreeMap<ComponentTypeId, ComponentProperties>,
}

impl ObjectSpec {
    /// A named point at the origin, at rest, with no components — the state
    /// one authoring action creates.
    pub fn new(name: DisplayName) -> Self {
        Self {
            name,
            transform: Transform::IDENTITY,
            velocity: Velocity::ZERO,
            shape: None,
            components: BTreeMap::new(),
        }
    }

    /// Place it.
    #[must_use]
    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Give it an initial velocity.
    #[must_use]
    pub fn with_velocity(mut self, velocity: Velocity) -> Self {
        self.velocity = velocity;
        self
    }

    /// Give it an extent.
    #[must_use]
    pub fn with_shape(mut self, shape: ObjectShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Attach a component at creation, replacing any earlier entry for that
    /// type.
    #[must_use]
    pub fn with_component(
        mut self,
        component: ComponentTypeId,
        properties: ComponentProperties,
    ) -> Self {
        self.components.insert(component, properties);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> DisplayName {
        DisplayName::new(value).expect("valid label")
    }

    #[test]
    fn a_new_spec_is_a_point_at_rest_carrying_no_physics() {
        let spec = ObjectSpec::new(name("Earth"));
        assert_eq!(spec.transform, Transform::IDENTITY);
        assert_eq!(spec.velocity, Velocity::ZERO);
        assert_eq!(spec.shape, None);
        assert!(spec.components.is_empty());
    }

    #[test]
    fn an_authored_value_reports_its_kind_for_a_diagnostic() {
        assert_eq!(AuthoredValue::si("1").kind_label(), "quantity");
        assert_eq!(AuthoredValue::Boolean(true).kind_label(), "boolean");
        assert_eq!(AuthoredValue::Text("x".into()).kind_label(), "text");
    }

    #[test]
    fn a_stored_quantity_exposes_both_intent_and_projection() {
        let value = PropertyValue::Quantity {
            source: "1 / 3 + 0.1".to_owned(),
            display_unit: None,
            si_value: 0.4333333333333333,
            dimension: Dimension::DIMENSIONLESS,
        };
        assert_eq!(value.source(), Some("1 / 3 + 0.1"));
        assert_eq!(value.si_value(), Some(0.4333333333333333));
        assert_eq!(PropertyValue::Boolean(true).source(), None);
        assert_eq!(PropertyValue::Boolean(true).si_value(), None);
    }

    #[test]
    fn a_stored_property_round_trips_through_serde() {
        let value = PropertyValue::Quantity {
            source: "6.9634e5".to_owned(),
            display_unit: Some("km".to_owned()),
            si_value: 6.9634e8,
            dimension: Dimension::LENGTH,
        };
        let encoded = serde_json::to_string(&value).expect("encodes");
        assert_eq!(
            serde_json::from_str::<PropertyValue>(&encoded).expect("decodes"),
            value
        );
    }
}
