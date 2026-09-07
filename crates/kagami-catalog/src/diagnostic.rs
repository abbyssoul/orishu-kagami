//! Structured reasons a catalog entry is invalid or unavailable.
//!
//! The two tiers are not interchangeable, and the split is the whole point of
//! ADR 0018's three load states:
//!
//! - [`InvalidReason`] is a defect *in the file*. It is the user's to fix and
//!   is the same on every machine: malformed YAML, a bad name, a cycle, a
//!   dimension mismatch, a non-finite result.
//! - [`UnavailableReason`] is a property of *this installation*. The file may
//!   be perfectly good; a plugin is missing, or a catalog it depends on is
//!   not loaded. Installing the plugin or copying the other catalog makes the
//!   entry available with no edit at all.
//!
//! Neither is ever repaired by substituting a fabricated value.

use std::fmt;

use orishu_variables::{ExprEvalError, ExprParsingError, SourceSpan};
use thiserror::Error;

use crate::name::{ComponentTypeId, NameError, PropertyName};
use crate::quantity::{Dimension, UnitError};
use crate::source::TemplateIdentity;

/// One problem found in a catalog document, located where it can be fixed.
#[derive(Clone, Debug, PartialEq, Error)]
#[error("{}", DiagnosticDisplay(self))]
pub struct Diagnostic {
    /// Dotted/bracketed path into the document, e.g.
    /// `spec.components[0].properties.mass`. `None` for a problem that
    /// precedes any field context, such as an oversized or non-UTF-8 file.
    pub field_path: Option<String>,
    /// Byte span within the authored expression source, when the problem
    /// belongs to expression text.
    pub span: Option<SourceSpan>,
    /// What is wrong.
    pub reason: InvalidReason,
}

impl Diagnostic {
    /// A diagnostic with no field context, for a whole-file failure.
    pub fn whole_file(reason: InvalidReason) -> Self {
        Self {
            field_path: None,
            span: None,
            reason,
        }
    }

    /// A diagnostic located at a document field path.
    pub fn at(field_path: impl Into<String>, reason: InvalidReason) -> Self {
        Self {
            field_path: Some(field_path.into()),
            span: None,
            reason,
        }
    }

    /// A diagnostic located at a byte span inside an expression at
    /// `field_path`.
    pub fn at_span(field_path: impl Into<String>, span: SourceSpan, reason: InvalidReason) -> Self {
        Self {
            field_path: Some(field_path.into()),
            span: Some(span),
            reason,
        }
    }
}

struct DiagnosticDisplay<'a>(&'a Diagnostic);

impl fmt::Display for DiagnosticDisplay<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.0.field_path, self.0.span) {
            (Some(path), Some(span)) => {
                write!(
                    formatter,
                    "{path}[{}..{}]: {}",
                    span.start, span.end, self.0.reason
                )
            }
            (Some(path), None) => write!(formatter, "{path}: {}", self.0.reason),
            (None, _) => write!(formatter, "{}", self.0.reason),
        }
    }
}

/// Why a catalog entry, or the file containing it, is not structurally valid.
///
/// Deliberately independent of which component schemas happen to be
/// installed — see [`UnavailableReason`] for that tier.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum InvalidReason {
    /// The file is larger than the declared bound.
    #[error("file is {actual_bytes} bytes, over the {max_bytes}-byte catalog limit")]
    FileTooLarge { max_bytes: u64, actual_bytes: u64 },
    /// The file's bytes are not UTF-8.
    #[error("file is not valid UTF-8")]
    NotUtf8,
    /// The file could not be read.
    #[error("could not read catalog file: {message}")]
    Io { message: String },
    /// The directory entry is a symbolic link, so the bytes behind it are not
    /// this catalog's data to own, edit, or replace.
    #[error("catalog entry is a symbolic link, which the catalog does not follow")]
    SymbolicLink,
    /// The YAML stream does not parse.
    #[error("malformed YAML: {message}")]
    MalformedYaml { message: String },
    /// The document does not carry the expected `apiVersion`.
    #[error("unsupported apiVersion `{found}`, expected `{expected}`")]
    UnsupportedApiVersion { found: String, expected: String },
    /// The document does not carry the expected `kind`.
    #[error("unsupported kind `{found}`, expected `{expected}`")]
    UnsupportedKind { found: String, expected: String },
    /// The document parsed as YAML but does not match the template format.
    #[error("does not match the object-template format: {message}")]
    SchemaMismatch { message: String },
    /// An identifier in the document is not a valid catalog name.
    #[error("invalid name: {source}")]
    InvalidName {
        #[source]
        source: NameError,
    },
    /// A `unit:` field names a unit the catalog does not know.
    #[error(transparent)]
    InvalidUnit {
        #[from]
        source: UnitError,
    },
    /// A declared count exceeded its bound.
    #[error("{what} count is {found}, over the limit of {limit}")]
    LimitExceeded {
        what: &'static str,
        found: usize,
        limit: usize,
    },
    /// Two components in one template share a local name, so their
    /// properties could not have distinct binding identities.
    #[error("component `{name}` is composed more than once in this template")]
    DuplicateComponent { name: String },
    /// Two entries claim the same catalog/template identity.
    #[error("template identity `{identity}` is already defined at {existing}")]
    DuplicateIdentity {
        identity: TemplateIdentity,
        existing: String,
    },
    /// An authored expression does not parse.
    #[error("invalid expression: {source}")]
    ExpressionSyntax {
        #[source]
        source: ExprParsingError,
    },
    /// An authored expression could not be evaluated.
    #[error("expression could not be evaluated: {source}")]
    ExpressionEval {
        #[source]
        source: ExprEvalError,
    },
    /// A concise reference such as `planets.sun.mass` matches more than one
    /// binding, so it cannot be resolved without qualification.
    #[error("reference `{reference}` is ambiguous; it matches {matches} bindings")]
    AmbiguousReference { reference: String, matches: usize },
    /// A quantity's declared unit does not measure the dimension its
    /// property schema declares.
    #[error("property `{property}` expects {expected} but the value is declared in {found}")]
    DimensionMismatch {
        property: PropertyName,
        expected: Dimension,
        found: Dimension,
    },
    /// A quantity declared its unit both inside the expression and in the
    /// `unit:` field.
    ///
    /// The shared grammar carries units, so `2.7 g` is a complete quantity.
    /// Declaring `unit:` as well would scale the magnitude a second time, and
    /// the two can disagree; saying it once is always possible.
    #[error(
        "expression `{source_text}` already resolves to {found}, so it must not also declare \
         `unit: {unit}`"
    )]
    UnitDeclaredTwice {
        /// The authored expression.
        source_text: String,
        /// The unit that was also declared.
        unit: String,
        /// The dimension the expression itself derived.
        found: Dimension,
    },
    /// A computed expression was annotated with a non-canonical unit.
    ///
    /// Catalog bindings are published in canonical SI, so scaling the result
    /// of an expression that reads another binding would silently rescale a
    /// value that is already in SI. Authoring the expression in canonical
    /// units is always possible and never ambiguous.
    #[error(
        "expression `{source_text}` references other bindings, so it must be declared in the \
         canonical unit for its dimension, not `{unit}`"
    )]
    NonCanonicalUnitInComputedExpression { source_text: String, unit: String },
    /// The authored value kind does not match the property's schema.
    #[error("property `{property}` is a {expected} but the value is a {found}")]
    PropertyKindMismatch {
        property: PropertyName,
        expected: &'static str,
        found: &'static str,
    },
    /// A component's schema is installed but the template omits a property
    /// that schema requires.
    #[error("component `{component}` requires property `{property}`")]
    MissingRequiredProperty {
        component: ComponentTypeId,
        property: PropertyName,
    },
    /// A value resolved to NaN or an infinity.
    #[error("value is not finite")]
    NonFiniteValue,
    /// A template declares the same local name as both a parameter and a
    /// helper, so a reference to it could not name one definition.
    #[error("`{name}` is declared as both a parameter and a helper")]
    ConflictingLocalName { name: String },
}

/// Why a structurally valid entry cannot be used in *this* installation.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum UnavailableReason {
    /// No installed plugin contributes this component type.
    #[error("component type `{component}` is not provided by any installed plugin")]
    UnknownComponentType { component: ComponentTypeId },
    /// The component schema is installed but does not declare this property.
    #[error("component `{component}` does not declare property `{property}`")]
    UnknownProperty {
        component: ComponentTypeId,
        property: PropertyName,
    },
    /// An expression references a binding no loaded catalog publishes.
    #[error("expression references `{reference}`, which no loaded catalog defines")]
    MissingDependency { reference: String },
    /// An expression references a binding that exists but is private to
    /// another scope.
    #[error("expression references `{reference}`, which is private to `{owner}`")]
    PrivateDependency {
        reference: String,
        owner: TemplateIdentity,
    },
    /// An expression depends, directly or transitively, on an entry that is
    /// itself unavailable.
    #[error("expression depends on `{reference}`, which is itself unavailable")]
    UnavailableDependency { reference: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::{ComponentName, PluginId};

    #[test]
    fn diagnostic_without_a_path_displays_only_its_reason() {
        let diagnostic = Diagnostic::whole_file(InvalidReason::NotUtf8);
        assert_eq!(diagnostic.to_string(), "file is not valid UTF-8");
    }

    #[test]
    fn diagnostic_with_a_path_is_prefixed_by_where_to_fix_it() {
        let diagnostic = Diagnostic::at(
            "spec.components[0].properties.mass",
            InvalidReason::NonFiniteValue,
        );
        assert_eq!(
            diagnostic.to_string(),
            "spec.components[0].properties.mass: value is not finite"
        );
    }

    #[test]
    fn diagnostic_with_a_span_reports_the_offending_byte_range() {
        let diagnostic = Diagnostic::at_span(
            "spec.helpers.solar_mass",
            SourceSpan { start: 4, end: 9 },
            InvalidReason::NonFiniteValue,
        );
        assert_eq!(
            diagnostic.to_string(),
            "spec.helpers.solar_mass[4..9]: value is not finite"
        );
    }

    #[test]
    fn dimension_mismatch_names_both_dimensions_in_base_units() {
        let reason = InvalidReason::DimensionMismatch {
            property: PropertyName::new("mass").unwrap(),
            expected: Dimension::MASS,
            found: Dimension::LENGTH,
        };
        assert_eq!(
            reason.to_string(),
            "property `mass` expects kg but the value is declared in m"
        );
    }

    #[test]
    fn unavailable_reason_names_the_missing_plugin_component() {
        let reason = UnavailableReason::UnknownComponentType {
            component: ComponentTypeId::new(
                PluginId::new("kagami.mass_sources").unwrap(),
                ComponentName::new("inertial_mass").unwrap(),
            ),
        };
        assert_eq!(
            reason.to_string(),
            "component type `kagami.mass_sources/inertial_mass` is not provided by any installed \
             plugin"
        );
    }
}
