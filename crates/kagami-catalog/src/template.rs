//! The validated object template: what a document means once its names,
//! units, expressions, and bounds have been parsed rather than merely
//! deserialized.
//!
//! [`Template::from_document`] is the one place structural validity is
//! decided. It is a pure function from a [`crate::document::TemplateDocument`]
//! to either a `Template` or the complete list of diagnostics, so a caller
//! can validate authored content — from a file, the UI, or an MCP tool —
//! without touching the filesystem or a schema registry.
//!
//! Structural validity deliberately stops short of anything installation
//! dependent. Whether the component types exist, whether a referenced binding
//! is visible, and whether the value dimension matches its property schema
//! are decided later, in [`mod@crate::resolve`], because their answers differ
//! between machines while a `Template` does not.

use std::collections::{BTreeMap, BTreeSet};

use orishu_variables::CompiledExpression;

use crate::diagnostic::{Diagnostic, InvalidReason};
use crate::document::{
    ComponentDocument, HelperDocument, MetadataDocument, ParameterDocument, PropertyValueDocument,
    QuantityDocument, SpecDocument, TemplateDocument, VisibilityDocument,
};
use crate::limits::Limits;
use crate::name::{ComponentName, ComponentTypeId, HelperName, ParameterName, PropertyName};
use crate::quantity::{self, Unit};
use crate::source::TemplateIdentity;

/// Whether a binding resolves outside the template that declares it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    /// Resolvable from any loaded catalog.
    #[default]
    Public,
    /// Resolvable only from within the declaring template.
    Private,
}

impl Visibility {
    fn from_document(value: Option<VisibilityDocument>, default: Visibility) -> Self {
        match value {
            Some(VisibilityDocument::Public) => Visibility::Public,
            Some(VisibilityDocument::Private) => Visibility::Private,
            None => default,
        }
    }

    fn to_document(self, default: Visibility) -> Option<VisibilityDocument> {
        if self == default {
            return None;
        }
        Some(match self {
            Visibility::Public => VisibilityDocument::Public,
            Visibility::Private => VisibilityDocument::Private,
        })
    }
}

/// A template's human-facing metadata, without its identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateMetadata {
    /// One-line human description.
    pub description: Option<String>,
    /// Searchable tags.
    pub labels: BTreeMap<String, String>,
    /// Free-form provenance.
    pub annotations: BTreeMap<String, String>,
}

/// An authored expression together with the unit its magnitude is written in.
///
/// The authored source is retained verbatim (ADR 0005) and is what the writer
/// emits again; [`Self::si_expression`] is the derived form published into the
/// shared variable environment, always in canonical SI.
#[derive(Clone, Debug, PartialEq)]
pub struct QuantityValue {
    expression: CompiledExpression,
    unit: Option<Unit>,
    si_expression: CompiledExpression,
    visibility: Visibility,
}

impl QuantityValue {
    /// The authored expression, exactly as written.
    pub fn expression(&self) -> &CompiledExpression {
        &self.expression
    }

    /// The authored unit, when one was declared.
    pub fn unit(&self) -> Option<Unit> {
        self.unit
    }

    /// The expression as published into the shared variable environment: the
    /// authored expression when it is already canonical, or its resolved
    /// canonical-SI magnitude when a scaling unit was declared.
    pub fn si_expression(&self) -> &CompiledExpression {
        &self.si_expression
    }

    /// Whether this value is visible outside its declaring template.
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }
}

/// One authored property value.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// A dimensioned physical value.
    Quantity(QuantityValue),
    /// A flag.
    Boolean(bool),
    /// Free-form text.
    Text(String),
}

impl PropertyValue {
    /// The value kind's name, for diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            PropertyValue::Quantity(_) => "quantity",
            PropertyValue::Boolean(_) => "boolean",
            PropertyValue::Text(_) => "text",
        }
    }
}

/// A template parameter: a dimensioned instantiation input with a default.
///
/// A default is mandatory so every structurally valid template can be
/// previewed and dimension-checked in the catalog browser without first
/// inventing instantiation inputs for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    /// The default value, used for preview and when an instantiation does
    /// not override the parameter.
    pub default: QuantityValue,
    /// One-line human description.
    pub description: Option<String>,
}

/// A template-local helper definition, private unless declared otherwise.
#[derive(Clone, Debug, PartialEq)]
pub struct Helper {
    /// The helper's value.
    pub value: QuantityValue,
    /// One-line human description.
    pub description: Option<String>,
}

/// One component composed into a template.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentInstance {
    /// The plugin-qualified component type.
    pub type_id: ComponentTypeId,
    /// Authored property values, keyed by property name.
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

impl ComponentInstance {
    /// The template-local name this component's bindings are published
    /// under, which is its component type's own name.
    pub fn local_name(&self) -> &ComponentName {
        &self.type_id.name
    }
}

/// The reusable content of a template.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateSpec {
    /// Instantiation inputs.
    pub parameters: BTreeMap<ParameterName, Parameter>,
    /// Template-local intermediate definitions.
    pub helpers: BTreeMap<HelperName, Helper>,
    /// Composed components, in authored order.
    pub components: Vec<ComponentInstance>,
}

/// A structurally valid object template.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    /// The catalog/template identity.
    pub identity: TemplateIdentity,
    /// Human-facing metadata.
    pub metadata: TemplateMetadata,
    /// Reusable content.
    pub spec: TemplateSpec,
}

impl Template {
    /// Validate a decoded document into a template, or report every problem
    /// found. Pure: no filesystem, no schema registry, no ambient state.
    pub fn from_document(
        document: &TemplateDocument,
        limits: &Limits,
    ) -> Result<Self, Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();
        let identity = TemplateIdentity::new(
            document.metadata.catalog.clone(),
            document.metadata.name.clone(),
        );
        let metadata = validate_metadata(&document.metadata, limits, &mut diagnostics);
        let spec = validate_spec(&document.spec, limits, &mut diagnostics);

        if diagnostics.is_empty() {
            Ok(Self {
                identity,
                metadata,
                spec,
            })
        } else {
            Err(diagnostics)
        }
    }

    /// Rebuild the document form of this template.
    ///
    /// Lossless by construction: every authored field is retained by the
    /// validated types, so serializing the result is the canonical encoding
    /// a [`crate::source::ContentFingerprint`] is taken over and the bytes
    /// the writer emits.
    pub fn to_document(&self) -> TemplateDocument {
        TemplateDocument::new(
            MetadataDocument {
                catalog: self.identity.catalog.clone(),
                name: self.identity.template.clone(),
                description: self.metadata.description.clone(),
                labels: self.metadata.labels.clone(),
                annotations: self.metadata.annotations.clone(),
            },
            SpecDocument {
                parameters: self
                    .spec
                    .parameters
                    .iter()
                    .map(|(name, parameter)| {
                        (
                            name.clone(),
                            ParameterDocument {
                                default: parameter.default.expression.source().to_owned(),
                                unit: parameter.default.unit.map(|unit| unit.symbol().to_owned()),
                                description: parameter.description.clone(),
                            },
                        )
                    })
                    .collect(),
                helpers: self
                    .spec
                    .helpers
                    .iter()
                    .map(|(name, helper)| {
                        (
                            name.clone(),
                            HelperDocument {
                                expression: helper.value.expression.source().to_owned(),
                                unit: helper.value.unit.map(|unit| unit.symbol().to_owned()),
                                visibility: helper
                                    .value
                                    .visibility
                                    .to_document(Visibility::Private),
                                description: helper.description.clone(),
                            },
                        )
                    })
                    .collect(),
                components: self
                    .spec
                    .components
                    .iter()
                    .map(|component| ComponentDocument {
                        component_type: component.type_id.clone(),
                        properties: component
                            .properties
                            .iter()
                            .map(|(name, value)| (name.clone(), property_to_document(value)))
                            .collect(),
                    })
                    .collect(),
            },
        )
    }

    /// The canonical encoding of this template, as written to a file and
    /// fingerprinted.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_yaml::to_string(&self.to_document())
            .expect("template documents are always serializable")
            .into_bytes()
    }
}

fn property_to_document(value: &PropertyValue) -> PropertyValueDocument {
    match value {
        PropertyValue::Quantity(quantity) => PropertyValueDocument::Quantity(QuantityDocument {
            expression: quantity.expression.source().to_owned(),
            unit: quantity.unit.map(|unit| unit.symbol().to_owned()),
            visibility: quantity.visibility.to_document(Visibility::Public),
        }),
        PropertyValue::Boolean(value) => PropertyValueDocument::Boolean(*value),
        PropertyValue::Text(value) => PropertyValueDocument::Text(value.clone()),
    }
}

fn validate_metadata(
    document: &MetadataDocument,
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> TemplateMetadata {
    check_limit(
        "metadata.labels",
        "label",
        document.labels.len(),
        limits.max_metadata_entries,
        diagnostics,
    );
    check_limit(
        "metadata.annotations",
        "annotation",
        document.annotations.len(),
        limits.max_metadata_entries,
        diagnostics,
    );
    if let Some(description) = &document.description {
        check_text("metadata.description", description, limits, diagnostics);
    }
    for (key, value) in document.labels.iter().chain(document.annotations.iter()) {
        check_text(&format!("metadata.{key}"), value, limits, diagnostics);
    }
    TemplateMetadata {
        description: document.description.clone(),
        labels: document.labels.clone(),
        annotations: document.annotations.clone(),
    }
}

fn validate_spec(
    document: &SpecDocument,
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> TemplateSpec {
    check_limit(
        "spec.parameters",
        "parameter",
        document.parameters.len(),
        limits.max_parameters_per_template,
        diagnostics,
    );
    check_limit(
        "spec.helpers",
        "helper",
        document.helpers.len(),
        limits.max_helpers_per_template,
        diagnostics,
    );
    check_limit(
        "spec.components",
        "component",
        document.components.len(),
        limits.max_components_per_template,
        diagnostics,
    );

    for name in document.parameters.keys() {
        if document
            .helpers
            .contains_key(&HelperName::new(name.as_str()).expect("validated name"))
        {
            diagnostics.push(Diagnostic::at(
                format!("spec.parameters.{name}"),
                InvalidReason::ConflictingLocalName {
                    name: name.to_string(),
                },
            ));
        }
    }

    let parameters = document
        .parameters
        .iter()
        .filter_map(|(name, parameter)| {
            let path = format!("spec.parameters.{name}");
            let default = validate_quantity(
                &path,
                &QuantityDocument {
                    expression: parameter.default.clone(),
                    unit: parameter.unit.clone(),
                    visibility: None,
                },
                Visibility::Private,
                limits,
                diagnostics,
            )?;
            Some((
                name.clone(),
                Parameter {
                    default,
                    description: parameter.description.clone(),
                },
            ))
        })
        .collect();

    let helpers = document
        .helpers
        .iter()
        .filter_map(|(name, helper)| {
            let path = format!("spec.helpers.{name}");
            let value = validate_quantity(
                &path,
                &QuantityDocument {
                    expression: helper.expression.clone(),
                    unit: helper.unit.clone(),
                    visibility: helper.visibility,
                },
                Visibility::Private,
                limits,
                diagnostics,
            )?;
            Some((
                name.clone(),
                Helper {
                    value,
                    description: helper.description.clone(),
                },
            ))
        })
        .collect();

    let mut seen_components: BTreeSet<&ComponentName> = BTreeSet::new();
    let components = document
        .components
        .iter()
        .enumerate()
        .filter_map(|(index, component)| {
            let path = format!("spec.components[{index}]");
            if !seen_components.insert(&component.component_type.name) {
                diagnostics.push(Diagnostic::at(
                    path.clone(),
                    InvalidReason::DuplicateComponent {
                        name: component.component_type.name.to_string(),
                    },
                ));
                return None;
            }
            Some(validate_component(&path, component, limits, diagnostics))
        })
        .collect();

    TemplateSpec {
        parameters,
        helpers,
        components,
    }
}

fn validate_component(
    path: &str,
    document: &ComponentDocument,
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> ComponentInstance {
    check_limit(
        path,
        "property",
        document.properties.len(),
        limits.max_properties_per_component,
        diagnostics,
    );
    let properties = document
        .properties
        .iter()
        .filter_map(|(name, value)| {
            let path = format!("{path}.properties.{name}");
            let value = match value {
                PropertyValueDocument::Quantity(quantity) => PropertyValue::Quantity(
                    validate_quantity(&path, quantity, Visibility::Public, limits, diagnostics)?,
                ),
                PropertyValueDocument::Boolean(value) => PropertyValue::Boolean(*value),
                PropertyValueDocument::Text(text) => {
                    check_text(&path, text, limits, diagnostics);
                    PropertyValue::Text(text.clone())
                }
            };
            Some((name.clone(), value))
        })
        .collect();
    ComponentInstance {
        type_id: document.component_type.clone(),
        properties,
    }
}

/// Parse one authored quantity: bound its source, compile its expression,
/// resolve its unit, and derive the canonical-SI form published as a binding.
fn validate_quantity(
    path: &str,
    document: &QuantityDocument,
    default_visibility: Visibility,
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<QuantityValue> {
    let unit = match document.unit.as_deref().map(quantity::lookup).transpose() {
        Ok(unit) => unit.copied(),
        Err(error) => {
            diagnostics.push(Diagnostic::at(path, InvalidReason::from(error)));
            None?
        }
    };
    let expression = compile(path, &document.expression, limits, diagnostics)?;

    let factor = unit.map_or(1.0, |unit| unit.si_factor());
    let si_expression = if factor == 1.0 {
        expression.clone()
    } else if !expression.is_const() {
        // A binding is published in canonical SI, so scaling the result of an
        // expression that reads another binding would rescale a magnitude
        // that is already canonical. Authoring in the canonical unit is
        // always possible, so this is refused rather than guessed at.
        diagnostics.push(Diagnostic::at(
            path,
            InvalidReason::NonCanonicalUnitInComputedExpression {
                source_text: document.expression.clone(),
                unit: unit.map_or_else(String::new, |unit| unit.symbol().to_owned()),
            },
        ));
        None?
    } else {
        scale_constant(path, &expression, factor, diagnostics)?
    };

    Some(QuantityValue {
        expression,
        unit,
        si_expression,
        visibility: Visibility::from_document(document.visibility, default_visibility),
    })
}

/// Fold a constant expression and its unit factor into the single canonical-SI
/// literal that is published as a binding.
fn scale_constant(
    path: &str,
    expression: &CompiledExpression,
    factor: f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CompiledExpression> {
    debug_assert!(expression.is_const(), "only constants are folded");
    let magnitude = match orishu_variables::VariablesSystem::default().eval(expression.source()) {
        Ok(magnitude) => magnitude,
        Err(orishu_variables::VariablesError::Eval(source)) => {
            diagnostics.push(Diagnostic::at(
                path,
                InvalidReason::ExpressionEval { source },
            ));
            None?
        }
        Err(_) => {
            diagnostics.push(Diagnostic::at(path, InvalidReason::NonFiniteValue));
            None?
        }
    };
    let si = magnitude * factor;
    if !si.is_finite() {
        diagnostics.push(Diagnostic::at(path, InvalidReason::NonFiniteValue));
        None?
    }
    // `{:?}` is f64's shortest round-tripping representation, so re-parsing
    // it is exact rather than a re-rounded approximation of the SI value.
    Some(
        CompiledExpression::parse(&format!("{si:?}"))
            .expect("a finite f64's debug form is a valid literal"),
    )
}

fn compile(
    path: &str,
    source: &str,
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CompiledExpression> {
    if source.len() > limits.max_expression_bytes {
        diagnostics.push(Diagnostic::at(
            path,
            InvalidReason::LimitExceeded {
                what: "expression byte",
                found: source.len(),
                limit: limits.max_expression_bytes,
            },
        ));
        return None;
    }
    let compiled = match CompiledExpression::parse(source) {
        Ok(compiled) => compiled,
        Err(source) => {
            let span = source.span;
            diagnostics.push(Diagnostic::at_span(
                path,
                span,
                InvalidReason::ExpressionSyntax { source },
            ));
            return None;
        }
    };
    let references = compiled.variables().len();
    if references > limits.max_expression_references {
        diagnostics.push(Diagnostic::at(
            path,
            InvalidReason::LimitExceeded {
                what: "expression reference",
                found: references,
                limit: limits.max_expression_references,
            },
        ));
        return None;
    }
    Some(compiled)
}

fn check_limit(
    path: &str,
    what: &'static str,
    found: usize,
    limit: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if found > limit {
        diagnostics.push(Diagnostic::at(
            path,
            InvalidReason::LimitExceeded { what, found, limit },
        ));
    }
}

fn check_text(path: &str, text: &str, limits: &Limits, diagnostics: &mut Vec<Diagnostic>) {
    if text.len() > limits.max_text_bytes {
        diagnostics.push(Diagnostic::at(
            path,
            InvalidReason::LimitExceeded {
                what: "text byte",
                found: text.len(),
                limit: limits.max_text_bytes,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata:
  catalog: planets
  name: sun
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: {expression: "1.989e30", unit: kg}}
"#;

    fn template(text: &str) -> Result<Template, Vec<Diagnostic>> {
        let document: TemplateDocument = serde_yaml::from_str(text).unwrap();
        Template::from_document(&document, &Limits::DEFAULT)
    }

    fn reasons(text: &str) -> Vec<InvalidReason> {
        template(text)
            .unwrap_err()
            .into_iter()
            .map(|diagnostic| diagnostic.reason)
            .collect()
    }

    fn quantity_of(template: &Template, component: usize, property: &str) -> QuantityValue {
        match &template.spec.components[component].properties[&PropertyName::new(property).unwrap()]
        {
            PropertyValue::Quantity(quantity) => quantity.clone(),
            other => panic!("expected a quantity, got {other:?}"),
        }
    }

    #[test]
    fn validates_a_minimal_template() {
        let template = template(MINIMAL).unwrap();
        assert_eq!(template.identity.to_string(), "planets/sun");
        assert_eq!(template.spec.components.len(), 1);
    }

    #[test]
    fn a_canonical_unit_leaves_the_published_expression_untouched() {
        let template = template(MINIMAL).unwrap();
        let mass = quantity_of(&template, 0, "mass");
        assert_eq!(mass.expression().source(), "1.989e30");
        assert_eq!(mass.si_expression().source(), "1.989e30");
        assert_eq!(mass.unit().unwrap().symbol(), "kg");
    }

    #[test]
    fn a_scaling_unit_publishes_the_canonical_si_magnitude() {
        let text = MINIMAL.replace(
            r#"mass: {quantity: {expression: "1.989e30", unit: kg}}"#,
            r#"mass: {quantity: {expression: "1.989e33", unit: g}}"#,
        );
        let template = template(&text).unwrap();
        let mass = quantity_of(&template, 0, "mass");
        // Authored source is retained; only the published form is scaled.
        assert_eq!(mass.expression().source(), "1.989e33");
        let si = orishu_variables::VariablesSystem::default()
            .eval(mass.si_expression().source())
            .unwrap();
        assert!((si - 1.989e30).abs() < 1.0e15, "{si}");
    }

    #[test]
    fn omitting_a_unit_publishes_the_expression_as_canonical_si() {
        let text = MINIMAL.replace(
            r#"mass: {quantity: {expression: "1.989e30", unit: kg}}"#,
            r#"mass: {quantity: "1.989e30"}"#,
        );
        let mass = quantity_of(&template(&text).unwrap(), 0, "mass");
        assert_eq!(mass.unit(), None);
        assert_eq!(mass.si_expression().source(), "1.989e30");
    }

    #[test]
    fn a_computed_expression_may_not_declare_a_scaling_unit() {
        let text = MINIMAL.replace(
            r#"mass: {quantity: {expression: "1.989e30", unit: kg}}"#,
            r#"mass: {quantity: {expression: "planets.sun.other / 2", unit: g}}"#,
        );
        assert!(matches!(
            reasons(&text).as_slice(),
            [InvalidReason::NonCanonicalUnitInComputedExpression { .. }]
        ));
    }

    #[test]
    fn a_computed_expression_may_declare_a_canonical_unit() {
        let text = MINIMAL.replace(
            r#"mass: {quantity: {expression: "1.989e30", unit: kg}}"#,
            r#"mass: {quantity: {expression: "planets.sun.other / 2", unit: kg}}"#,
        );
        let mass = quantity_of(&template(&text).unwrap(), 0, "mass");
        assert_eq!(mass.si_expression().source(), "planets.sun.other / 2");
    }

    #[test]
    fn an_unknown_unit_is_reported_rather_than_defaulted() {
        let text = MINIMAL.replace("unit: kg", "unit: furlong");
        assert!(matches!(
            reasons(&text).as_slice(),
            [InvalidReason::InvalidUnit { .. }]
        ));
    }

    #[test]
    fn a_malformed_expression_reports_its_source_span() {
        let text = MINIMAL.replace(r#"expression: "1.989e30""#, r#"expression: "1 +""#);
        let diagnostics = template(&text).unwrap_err();
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::ExpressionSyntax { .. }
        ));
        assert!(diagnostics[0].span.is_some());
    }

    #[test]
    fn a_component_composed_twice_could_not_have_distinct_bindings() {
        let text = MINIMAL.replace(
            "  components:\n",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    properties: {}\n",
        );
        assert!(matches!(
            reasons(&text).as_slice(),
            [InvalidReason::DuplicateComponent { .. }]
        ));
    }

    #[test]
    fn a_name_declared_as_both_parameter_and_helper_is_rejected() {
        let text = MINIMAL.replace(
            "spec:\n",
            "spec:\n  parameters:\n    x: {default: \"1\"}\n  helpers:\n    x: {expression: \"2\"}\n",
        );
        assert!(
            reasons(&text)
                .iter()
                .any(|reason| matches!(reason, InvalidReason::ConflictingLocalName { .. }))
        );
    }

    #[test]
    fn an_oversized_expression_fails_within_the_declared_bound() {
        let document: TemplateDocument = serde_yaml::from_str(MINIMAL).unwrap();
        let limits = Limits {
            max_expression_bytes: 4,
            ..Limits::DEFAULT
        };
        let diagnostics = Template::from_document(&document, &limits).unwrap_err();
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::LimitExceeded {
                what: "expression byte",
                ..
            }
        ));
    }

    #[test]
    fn too_many_components_fails_within_the_declared_bound() {
        let document: TemplateDocument = serde_yaml::from_str(MINIMAL).unwrap();
        let limits = Limits {
            max_components_per_template: 0,
            ..Limits::DEFAULT
        };
        let diagnostics = Template::from_document(&document, &limits).unwrap_err();
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::LimitExceeded {
                what: "component",
                ..
            }
        ));
    }

    #[test]
    fn every_problem_in_one_document_is_reported_at_once() {
        let text = MINIMAL.replace(
            r#"mass: {quantity: {expression: "1.989e30", unit: kg}}"#,
            "mass: {quantity: {expression: \"1 +\", unit: kg}}\n      \
             other: {quantity: {expression: \"1\", unit: furlong}}",
        );
        assert_eq!(reasons(&text).len(), 2);
    }

    #[test]
    fn helpers_are_private_and_properties_public_by_default() {
        let text = MINIMAL.replace(
            "spec:\n",
            "spec:\n  helpers:\n    solar_mass: {expression: \"1.989e30\", unit: kg}\n",
        );
        let template = template(&text).unwrap();
        assert_eq!(
            template.spec.helpers[&HelperName::new("solar_mass").unwrap()]
                .value
                .visibility(),
            Visibility::Private
        );
        assert_eq!(
            quantity_of(&template, 0, "mass").visibility(),
            Visibility::Public
        );
    }

    #[test]
    fn a_template_round_trips_through_its_canonical_document() {
        let template = template(MINIMAL).unwrap();
        let bytes = template.canonical_bytes();
        let document: TemplateDocument = serde_yaml::from_slice(&bytes).unwrap();
        assert_eq!(
            Template::from_document(&document, &Limits::DEFAULT).unwrap(),
            template
        );
    }

    #[test]
    fn canonical_bytes_ignore_authored_formatting() {
        let compact = template(MINIMAL).unwrap().canonical_bytes();
        let text = MINIMAL.replace(
            "      mass: {quantity: {expression: \"1.989e30\", unit: kg}}",
            "      mass:\n        quantity:\n          unit: kg\n          expression: \"1.989e30\"",
        );
        assert_eq!(template(&text).unwrap().canonical_bytes(), compact);
    }
}
