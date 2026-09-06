//! Deciding a candidate experiment, and saying precisely why not.
//!
//! Three rules shape this module.
//!
//! **The complete candidate is validated, not the individual command.** A
//! batch is applied to a candidate and the *result* is checked, so an
//! invariant that spans commands — a total count, a component's required
//! properties after several edits — cannot be satisfied at each step and
//! violated at the end.
//!
//! **A rejection is data, not a message.** [`Rejection`] carries a stable
//! machine-readable [`Rejection::code`], the affected entity or property path,
//! and a human explanation, because an automation client has to repair its
//! input and cannot parse prose to do it.
//!
//! **Structural validity is checked over everything; schema governance only
//! over what the batch touched.** The structural half asks what can be decided
//! from authored data alone — counts, bounds, referential integrity — and
//! needs no registry at all. The governance half asks whether the installed
//! schemas accept the content this batch created, attached or
//! changed. Content the batch did not touch is *not* revalidated against the
//! current registry: a plugin that is no longer installed makes its components
//! unusable, not the experiment invalid, and [`crate::capability`] is where
//! that is reported. Extending the governance check back over the whole
//! candidate would make uninstalling a plugin destroy the ability to edit
//! anything at all.
//!
//! # Where the resolved value comes from
//!
//! Resolving an [`AuthoredValue`] into a [`PropertyValue`] happens here and
//! only here. That is what makes "the stored magnitude is derived from the
//! stored source" an invariant of the model rather than a convention callers
//! are trusted to follow: a command carries what the user wrote, and the only
//! path from there into an experiment runs through this file.

use std::fmt;

use kagami_catalog::{
    ComponentTypeId, Dimension, PropertyKind, PropertyName, PropertySchema, SchemaRegistry,
};
use orishu_variables::{
    CompiledExpression, ExprEvalError, ExprParsingError, VariablesError, VariablesSystem,
};
use thiserror::Error;

use crate::capability::{CapabilityGap, gap_of};
use crate::id::ObjectId;
use crate::limits::Limits;
use crate::model::ExperimentState;
use crate::object::{AuthoredValue, PropertyValue};

/// Which component of which object a diagnostic is about.
///
/// Ordered by object and then component type, so a collection of paths — a
/// batch's touched components, a capability report — has one deterministic
/// order everywhere.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComponentPath {
    /// The object carrying, or asked to carry, the component.
    pub object: ObjectId,
    /// The plugin-qualified component type.
    pub component: ComponentTypeId,
}

impl fmt::Display for ComponentPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.object, self.component)
    }
}

/// Which property of which component of which object a diagnostic is about.
///
/// Rendered as `object-3/kagami.mass_sources/inertial_mass.mass`, so a
/// rejection names one addressable thing rather than describing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyPath {
    /// The component this property belongs to.
    pub component: ComponentPath,
    /// The property's name.
    pub property: PropertyName,
}

impl PropertyPath {
    /// Build a path from its parts.
    pub fn new(object: ObjectId, component: ComponentTypeId, property: PropertyName) -> Self {
        Self {
            component: ComponentPath { object, component },
            property,
        }
    }

    /// The object this property belongs to.
    pub fn object(&self) -> ObjectId {
        self.component.object
    }
}

impl fmt::Display for PropertyPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.component, self.property)
    }
}

/// Why a proposed edit was not adopted.
///
/// Every variant names what was affected. A caller that receives one has
/// enough to point at the offending field without guessing.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum Rejection {
    /// The batch held more commands than the limits allow.
    #[error("batch of {found} commands exceeds the limit of {limit}")]
    BatchTooLarge {
        /// The submitted count.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// The candidate held more objects than the limits allow.
    #[error("experiment would hold {found} objects, over the limit of {limit}")]
    TooManyObjects {
        /// The resulting count.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// A command named an object this experiment does not have.
    #[error("no object {object} in this experiment")]
    UnknownObject {
        /// The identity that was named.
        object: ObjectId,
    },
    /// A command edited or detached a component the object does not carry.
    #[error("{path} is not attached")]
    ComponentNotAttached {
        /// The component that was named.
        path: ComponentPath,
    },
    /// No installed simulation plugin contributes this component type.
    ///
    /// A fact about *this installation*, not about the command: installing the
    /// plugin makes the same command succeed. The component's stored values,
    /// if it already had some, are untouched — see [`crate::capability`].
    #[error("component `{component}` is not contributed by any installed simulation plugin")]
    ComponentTypeNotInstalled {
        /// The component type that was named.
        component: ComponentTypeId,
    },
    /// The batch touched a component the installed schema for its type
    /// refuses.
    ///
    /// The plugin *is* installed and disagrees with what is stored, so this is
    /// not repaired by installing anything. Until an explicit migration
    /// command exists, the component is editable only by detaching it: no
    /// substitute schema and no default value is used to make it fit.
    #[error("{path} is not governed by the installed schema: {gap}")]
    ComponentIncompatible {
        /// The component.
        path: ComponentPath,
        /// How the stored values and the installed declaration disagree.
        ///
        /// Boxed to keep `Rejection` small: it is the `Err` half of every
        /// fallible function here, so its size is paid on the success path
        /// too.
        gap: Box<CapabilityGap>,
    },
    /// An object carried more components than the limits allow.
    #[error("object {object} would carry {found} components, over the limit of {limit}")]
    TooManyComponents {
        /// The object.
        object: ObjectId,
        /// The resulting count.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// A component authored more properties than the limits allow.
    #[error("{path} would author {found} properties, over the limit of {limit}")]
    TooManyProperties {
        /// The component.
        path: ComponentPath,
        /// The resulting count.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// The component's schema declares no such property.
    #[error("{path} is not declared by the component's schema")]
    PropertyNotDeclared {
        /// The property that was named.
        path: PropertyPath,
    },
    /// A property the component's schema requires was not authored.
    #[error("{path} is required by the component's schema but was not authored")]
    RequiredPropertyMissing {
        /// The missing property.
        path: PropertyPath,
    },
    /// The authored value was of a different kind than the schema declares.
    #[error("{path} is declared as a {expected}, but a {found} was supplied")]
    PropertyKindMismatch {
        /// The property.
        path: PropertyPath,
        /// The kind its schema declares.
        expected: &'static str,
        /// The kind that was supplied.
        found: &'static str,
    },
    /// The authored unit does not measure the dimension the schema declares.
    #[error("{path} is declared in {expected}, but unit `{unit}` measures {found}")]
    UnitDimensionMismatch {
        /// The property.
        path: PropertyPath,
        /// The unit symbol that was authored. Borrowed from the unit table,
        /// which is static, so this costs a pointer rather than an
        /// allocation on a path that is already an error.
        unit: &'static str,
        /// The dimension the schema declares.
        expected: Dimension,
        /// The dimension the unit measures.
        found: Dimension,
    },
    /// The authored expression source exceeded the limits.
    #[error("expression for {path} is {found} bytes, over the limit of {limit}")]
    ExpressionTooLong {
        /// The property.
        path: PropertyPath,
        /// The submitted length.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// The authored expression referenced more symbols than the limits allow.
    #[error("expression for {path} makes {found} symbol references, over the limit of {limit}")]
    TooManyExpressionReferences {
        /// The property.
        path: PropertyPath,
        /// The submitted count.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// The authored expression did not parse.
    #[error("expression for {path} could not be parsed: {source}")]
    ExpressionInvalid {
        /// The property.
        path: PropertyPath,
        /// The parse failure, carrying the byte span it occurred at.
        ///
        /// Boxed to keep `Rejection` small: it is the `Err` half of every
        /// fallible function here, so its size is paid on the success path
        /// too. See `a_rejection_stays_small_enough_to_return_by_value`.
        #[source]
        source: Box<ExprParsingError>,
    },
    /// The authored expression parsed but could not be evaluated.
    #[error("expression for {path} could not be evaluated: {source}")]
    ExpressionUnresolved {
        /// The property.
        path: PropertyPath,
        /// The evaluation failure.
        #[source]
        source: ExprEvalError,
    },
    /// Converting the magnitude to canonical SI produced a non-finite value.
    ///
    /// Distinct from an evaluation failure: the expression resolved, but
    /// applying the authored unit's factor overflowed.
    #[error("expression for {path} does not resolve to a finite value in canonical SI")]
    NonFiniteValue {
        /// The property.
        path: PropertyPath,
    },
    /// A text property exceeded the limits.
    #[error("text for {path} is {found} bytes, over the limit of {limit}")]
    TextTooLong {
        /// The property.
        path: PropertyPath,
        /// The submitted length.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// More simulation plugins were enabled than the limits allow.
    #[error("{found} simulation plugins would be enabled, over the limit of {limit}")]
    TooManyEnabledPlugins {
        /// The resulting count.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
}

impl Rejection {
    /// A stable identifier for this reason.
    ///
    /// Machine-readable and independent of wording, so an adapter can branch
    /// on it and a translated message cannot change its meaning.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::BatchTooLarge { .. } => "batch_too_large",
            Self::TooManyObjects { .. } => "too_many_objects",
            Self::UnknownObject { .. } => "unknown_object",
            Self::ComponentNotAttached { .. } => "component_not_attached",
            Self::ComponentTypeNotInstalled { .. } => "component_type_not_installed",
            Self::ComponentIncompatible { .. } => "component_incompatible",
            Self::TooManyComponents { .. } => "too_many_components",
            Self::TooManyProperties { .. } => "too_many_properties",
            Self::PropertyNotDeclared { .. } => "property_not_declared",
            Self::RequiredPropertyMissing { .. } => "required_property_missing",
            Self::PropertyKindMismatch { .. } => "property_kind_mismatch",
            Self::UnitDimensionMismatch { .. } => "unit_dimension_mismatch",
            Self::ExpressionTooLong { .. } => "expression_too_long",
            Self::TooManyExpressionReferences { .. } => "too_many_expression_references",
            Self::ExpressionInvalid { .. } => "expression_invalid",
            Self::ExpressionUnresolved { .. } => "expression_unresolved",
            Self::NonFiniteValue { .. } => "non_finite_value",
            Self::TextTooLong { .. } => "text_too_long",
            Self::TooManyEnabledPlugins { .. } => "too_many_enabled_plugins",
        }
    }
}

/// Resolve one authored value against the property schema that governs it.
///
/// The single ingress from "what the user wrote" to "what the model stores".
pub(crate) fn resolve_property(
    path: PropertyPath,
    schema: &PropertySchema,
    authored: &AuthoredValue,
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<PropertyValue, Rejection> {
    match (&schema.kind, authored) {
        (PropertyKind::Quantity { dimension }, AuthoredValue::Quantity { expression, unit }) => {
            resolve_quantity(path, *dimension, expression, *unit, variables, limits)
        }
        (PropertyKind::Boolean, AuthoredValue::Boolean(value)) => {
            Ok(PropertyValue::Boolean(*value))
        }
        (PropertyKind::Text, AuthoredValue::Text(value)) => {
            if value.len() > limits.max_text_bytes {
                return Err(Rejection::TextTooLong {
                    path,
                    found: value.len(),
                    limit: limits.max_text_bytes,
                });
            }
            Ok(PropertyValue::Text(value.clone()))
        }
        (expected, found) => Err(Rejection::PropertyKindMismatch {
            path,
            expected: expected.label(),
            found: found.kind_label(),
        }),
    }
}

fn resolve_quantity(
    path: PropertyPath,
    dimension: Dimension,
    expression: &str,
    unit: Option<kagami_catalog::Unit>,
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<PropertyValue, Rejection> {
    if expression.len() > limits.max_expression_bytes {
        return Err(Rejection::ExpressionTooLong {
            path,
            found: expression.len(),
            limit: limits.max_expression_bytes,
        });
    }

    // Parsed here as well as inside `eval` so the reference count can be
    // bounded *before* anything is evaluated. The double parse costs a pass
    // over at most `max_expression_bytes`, which is cheaper than admitting an
    // unbounded symbol graph and discovering it during evaluation.
    let compiled = match CompiledExpression::parse(expression) {
        Ok(compiled) => compiled,
        Err(source) => {
            return Err(Rejection::ExpressionInvalid {
                path,
                source: Box::new(source),
            });
        }
    };
    let references = compiled.variables().len();
    if references > limits.max_expression_references {
        return Err(Rejection::TooManyExpressionReferences {
            path,
            found: references,
            limit: limits.max_expression_references,
        });
    }

    let (factor, display_unit) = match unit {
        Some(unit) if unit.dimension() != dimension => {
            return Err(Rejection::UnitDimensionMismatch {
                path,
                unit: unit.symbol(),
                expected: dimension,
                found: unit.dimension(),
            });
        }
        Some(unit) => (unit.si_factor(), Some(unit.symbol().to_owned())),
        None => (1.0, None),
    };

    let magnitude = match variables.eval(expression) {
        Ok(value) => value,
        Err(VariablesError::Parsing(source)) => {
            return Err(Rejection::ExpressionInvalid {
                path,
                source: Box::new(source),
            });
        }
        Err(VariablesError::Eval(source)) => {
            return Err(Rejection::ExpressionUnresolved { path, source });
        }
        // The remaining variants report defining or looking up a variable,
        // which evaluating an ad-hoc expression never does.
        Err(_) => {
            return Err(Rejection::ExpressionUnresolved {
                path,
                source: ExprEvalError::UnknownVariable(expression.to_owned()),
            });
        }
    };

    let si_value = magnitude * factor;
    if !si_value.is_finite() {
        return Err(Rejection::NonFiniteValue { path });
    }

    Ok(PropertyValue::Quantity {
        source: expression.to_owned(),
        display_unit,
        si_value,
        dimension,
    })
}

/// Check every invariant decidable from authored data alone.
///
/// Runs against the finished candidate, so a batch cannot satisfy a bound at
/// each step and violate it in aggregate. Takes no [`SchemaRegistry`] on
/// purpose: everything here holds for a document whatever is installed, which
/// is exactly what makes it safe to check on content the batch did not touch.
pub(crate) fn validate_structure(
    state: &ExperimentState,
    limits: &Limits,
) -> Result<(), Rejection> {
    if state.objects.len() > limits.max_objects {
        return Err(Rejection::TooManyObjects {
            found: state.objects.len(),
            limit: limits.max_objects,
        });
    }
    if state.setup.plugins.len() > limits.max_enabled_plugins {
        return Err(Rejection::TooManyEnabledPlugins {
            found: state.setup.plugins.len(),
            limit: limits.max_enabled_plugins,
        });
    }

    for (id, object) in state.objects.iter() {
        if object.components.len() > limits.max_components_per_object {
            return Err(Rejection::TooManyComponents {
                object: *id,
                found: object.components.len(),
                limit: limits.max_components_per_object,
            });
        }
        for (type_id, component) in &object.components {
            if component.properties.len() > limits.max_properties_per_component {
                return Err(Rejection::TooManyProperties {
                    path: ComponentPath {
                        object: *id,
                        component: type_id.clone(),
                    },
                    found: component.properties.len(),
                    limit: limits.max_properties_per_component,
                });
            }
        }
    }

    Ok(())
}

/// Check that the installed schemas govern everything this batch touched.
///
/// `touched` names the components the batch created, attached, or changed a
/// property of. A path whose object or component the same batch went on to
/// remove is skipped: the batch left nothing there to govern.
///
/// This is where a required property missing *after* several commands is
/// caught, which is why it runs over the finished candidate rather than at
/// each command.
pub(crate) fn validate_governed(
    state: &ExperimentState,
    touched: &[ComponentPath],
    schemas: &SchemaRegistry,
) -> Result<(), Rejection> {
    for path in touched {
        let Some(object) = state.objects.get(&path.object) else {
            continue;
        };
        let Some(component) = object.components.get(&path.component) else {
            continue;
        };
        if schemas.get(&path.component).is_none() {
            return Err(Rejection::ComponentTypeNotInstalled {
                component: path.component.clone(),
            });
        }
        if let Some(gap) = gap_of(&path.component, component, schemas) {
            return Err(rejection_for(path.clone(), gap));
        }
    }

    Ok(())
}

/// Check that captured contents may be reinstated as the current experiment.
///
/// Deliberately weaker than [`validate_governed`] in one direction and
/// stronger in another. A component whose plugin is merely *absent* restores
/// fine: those contents are no different from a document loaded on a machine
/// without that plugin, and refusing would mean uninstalling a plugin
/// mid-session silently disables undo. A component an *installed* schema
/// refuses does not restore: making it current would assert values that
/// declaration rejects, and no substitute schema exists to reinterpret them.
pub(crate) fn validate_restorable(
    state: &ExperimentState,
    schemas: &SchemaRegistry,
) -> Result<(), Rejection> {
    for (id, object) in state.objects.iter() {
        for (type_id, component) in &object.components {
            if let Some(gap) = gap_of(type_id, component, schemas)
                && !gap.is_absent()
            {
                return Err(rejection_for(
                    ComponentPath {
                        object: *id,
                        component: type_id.clone(),
                    },
                    gap,
                ));
            }
        }
    }

    Ok(())
}

/// Report a capability gap as the refusal it causes.
///
/// The two gaps that already have an addressable property-level rejection keep
/// it, so an adapter branching on `required_property_missing` sees the same
/// code whether the property was never authored or the schema started
/// requiring it.
fn rejection_for(path: ComponentPath, gap: CapabilityGap) -> Rejection {
    match gap {
        CapabilityGap::SchemaNotInstalled => Rejection::ComponentTypeNotInstalled {
            component: path.component,
        },
        CapabilityGap::RequiredPropertyMissing { property } => Rejection::RequiredPropertyMissing {
            path: PropertyPath {
                component: path,
                property,
            },
        },
        CapabilityGap::PropertyNotDeclared { property } => Rejection::PropertyNotDeclared {
            path: PropertyPath {
                component: path,
                property,
            },
        },
        gap => Rejection::ComponentIncompatible {
            path,
            gap: Box::new(gap),
        },
    }
}

#[cfg(test)]
mod tests {
    use kagami_catalog::{ComponentName, PluginId};

    use super::*;

    fn path() -> PropertyPath {
        PropertyPath::new(
            ObjectId::from_raw(0),
            ComponentTypeId::new(
                PluginId::new("kagami.mass_sources").expect("valid"),
                ComponentName::new("inertial_mass").expect("valid"),
            ),
            PropertyName::new("mass").expect("valid"),
        )
    }

    fn mass_schema() -> PropertySchema {
        PropertySchema::required(PropertyKind::Quantity {
            dimension: Dimension::MASS,
        })
    }

    fn resolve(
        schema: &PropertySchema,
        authored: &AuthoredValue,
    ) -> Result<PropertyValue, Rejection> {
        resolve_property(
            path(),
            schema,
            authored,
            &VariablesSystem::default(),
            &Limits::DEFAULT,
        )
    }

    #[test]
    fn a_rejection_stays_small_enough_to_return_by_value() {
        // A `Rejection` is the `Err` half of every fallible function in this
        // crate, so its size is paid on the *success* path too — every
        // `Result` is as large as its largest variant. Adding a wide payload
        // to a variant is fine; adding it unboxed is not.
        assert!(
            size_of::<Rejection>() <= 128,
            "Rejection grew to {} bytes; box the new payload",
            size_of::<Rejection>()
        );
    }

    #[test]
    fn a_property_path_renders_one_addressable_thing() {
        assert_eq!(
            path().to_string(),
            "object-0/kagami.mass_sources/inertial_mass.mass"
        );
    }

    #[test]
    fn arithmetic_resolves_to_canonical_si() {
        let value = resolve(&mass_schema(), &AuthoredValue::si("1 / 3 + 0.1")).expect("resolves");
        assert_eq!(value.source(), Some("1 / 3 + 0.1"));
        assert!((value.si_value().expect("quantity") - 0.433_333_333_333_333_3).abs() < 1e-15);
    }

    #[test]
    fn an_authored_unit_scales_the_magnitude_and_is_retained_for_display() {
        let unit = *kagami_catalog::quantity::lookup("g").expect("known unit");
        let value =
            resolve(&mass_schema(), &AuthoredValue::in_unit("2.7", unit)).expect("resolves");
        assert_eq!(value.si_value(), Some(2.7e-3));
        match value {
            PropertyValue::Quantity { display_unit, .. } => {
                assert_eq!(display_unit.as_deref(), Some("g"));
            }
            other => panic!("expected a quantity, got {other:?}"),
        }
    }

    #[test]
    fn a_unit_measuring_the_wrong_dimension_is_refused() {
        let unit = *kagami_catalog::quantity::lookup("m").expect("known unit");
        let rejection = resolve(&mass_schema(), &AuthoredValue::in_unit("1", unit))
            .expect_err("a length is not a mass");
        assert_eq!(rejection.code(), "unit_dimension_mismatch");
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_refused_with_both_kinds_named() {
        let rejection = resolve(&mass_schema(), &AuthoredValue::Boolean(true))
            .expect_err("a boolean is not a mass");
        assert_eq!(
            rejection,
            Rejection::PropertyKindMismatch {
                path: path(),
                expected: "quantity",
                found: "boolean",
            }
        );
    }

    #[test]
    fn a_syntactically_invalid_expression_reports_its_span() {
        let rejection =
            resolve(&mass_schema(), &AuthoredValue::si("1 +")).expect_err("incomplete expression");
        match rejection {
            Rejection::ExpressionInvalid { source, .. } => {
                assert!(source.span.end >= source.span.start);
            }
            other => panic!("expected a parse failure, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_symbol_is_unresolved_rather_than_defaulted() {
        // K1 has no document variable graph, so every symbol is unknown here.
        // The point is that it is *reported*, never silently taken as zero.
        let rejection = resolve(&mass_schema(), &AuthoredValue::si("mass_of_sun"))
            .expect_err("no such variable");
        assert_eq!(rejection.code(), "expression_unresolved");
    }

    #[test]
    fn division_by_zero_and_overflow_never_become_a_stored_value() {
        assert_eq!(
            resolve(&mass_schema(), &AuthoredValue::si("1 / 0"))
                .expect_err("division by zero")
                .code(),
            "expression_unresolved"
        );
        assert_eq!(
            resolve(&mass_schema(), &AuthoredValue::si("1e308 * 1e308"))
                .expect_err("overflow")
                .code(),
            "expression_unresolved"
        );
    }

    #[test]
    fn an_oversized_expression_is_refused_before_it_is_parsed() {
        let limits = Limits {
            max_expression_bytes: 8,
            ..Limits::DEFAULT
        };
        let rejection = resolve_property(
            path(),
            &mass_schema(),
            &AuthoredValue::si("1 + 1 + 1 + 1 + 1"),
            &VariablesSystem::default(),
            &limits,
        )
        .expect_err("over the byte limit");
        assert_eq!(rejection.code(), "expression_too_long");
    }

    #[test]
    fn an_over_referencing_expression_is_refused_before_it_is_evaluated() {
        let limits = Limits {
            max_expression_references: 1,
            ..Limits::DEFAULT
        };
        let rejection = resolve_property(
            path(),
            &mass_schema(),
            &AuthoredValue::si("a + b + c"),
            &VariablesSystem::default(),
            &limits,
        )
        .expect_err("over the reference limit");
        assert_eq!(rejection.code(), "too_many_expression_references");
    }

    #[test]
    fn an_oversized_text_property_is_refused() {
        let schema = PropertySchema::optional(PropertyKind::Text);
        let limits = Limits {
            max_text_bytes: 4,
            ..Limits::DEFAULT
        };
        let rejection = resolve_property(
            path(),
            &schema,
            &AuthoredValue::Text("far too long".to_owned()),
            &VariablesSystem::default(),
            &limits,
        )
        .expect_err("over the text limit");
        assert_eq!(rejection.code(), "text_too_long");
    }
}
