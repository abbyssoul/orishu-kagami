//! Turning parsed documents into a resolved catalog set.
//!
//! This is the crate's decision core and is entirely pure: given the parsed
//! documents, the installed component schemas, and the bounds, it decides
//! which entries are available, which are unavailable here, and which are
//! invalid — with no filesystem, clock, or ambient state involved. The
//! loader in [`mod@crate::load`] is the thin shell that produces its input.
//!
//! Resolution happens in one pass over four questions, in this order,
//! because each depends on the answer to the last:
//!
//! 1. Does any other entry already own this template identity?
//! 2. Does every reference in every authored expression resolve to a
//!    *visible* binding?
//! 3. Does every expression evaluate to a finite value without a cycle?
//! 4. Are the component schemas installed, and do the authored values match
//!    them?
//!
//! Finally, unavailability is propagated: an entry that depends on an entry
//! which is itself unavailable is unavailable too, because its value cannot
//! be supplied until the other one can.

use std::collections::{BTreeMap, BTreeSet};

use orishu_variables::{ExprEvalError, VariablesError};

use crate::binding::{BindingKind, CatalogProjection, ProjectionError, Resolution, binding_count};
use crate::diagnostic::{Diagnostic, InvalidReason, UnavailableReason};
use crate::entry::{CatalogEntry, CatalogFileError, CatalogSet, LoadResult};
use crate::limits::Limits;
use crate::name::PropertyName;
use crate::schema::{PropertyKind, SchemaRegistry};
use crate::source::{ContentFingerprint, SourceLocation, TemplateIdentity};
use crate::template::{PropertyValue, QuantityValue, Template};

/// One document as produced by parsing, before anything installation
/// dependent has been considered.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedDocument {
    /// Where the document came from.
    pub source: SourceLocation,
    /// Its identity, when the metadata names validated.
    pub identity: Option<TemplateIdentity>,
    /// The validated template, or every structural problem found.
    pub outcome: Result<Template, Vec<Diagnostic>>,
}

impl ParsedDocument {
    /// A document that failed structural validation.
    pub fn invalid(
        source: SourceLocation,
        identity: Option<TemplateIdentity>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        debug_assert!(!diagnostics.is_empty(), "an invalid document has a reason");
        Self {
            source,
            identity,
            outcome: Err(diagnostics),
        }
    }

    /// A document that validated into `template`.
    pub fn valid(source: SourceLocation, template: Template) -> Self {
        Self {
            source,
            identity: Some(template.identity.clone()),
            outcome: Ok(template),
        }
    }
}

/// What an entry's state is while it is being decided.
#[derive(Debug, Default)]
struct Status {
    diagnostics: Vec<Diagnostic>,
    unavailable: Vec<UnavailableReason>,
    /// Templates this entry's expressions read from, excluding itself.
    dependencies: BTreeSet<TemplateIdentity>,
}

impl Status {
    fn is_invalid(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    fn is_unavailable(&self) -> bool {
        !self.unavailable.is_empty()
    }
}

/// Decide the state of every parsed document against the installed schemas.
///
/// Deterministic: the same documents, registry, and limits always produce the
/// same entries, in the same order, with the same diagnostics.
pub fn resolve(
    documents: Vec<ParsedDocument>,
    file_errors: Vec<CatalogFileError>,
    registry: &SchemaRegistry,
    limits: &Limits,
) -> CatalogSet {
    let mut templates: Vec<Option<Template>> = Vec::with_capacity(documents.len());
    let mut statuses: Vec<Status> = Vec::with_capacity(documents.len());
    let mut sources: Vec<SourceLocation> = Vec::with_capacity(documents.len());
    let mut identities: Vec<Option<TemplateIdentity>> = Vec::with_capacity(documents.len());
    let mut owner: BTreeMap<TemplateIdentity, SourceLocation> = BTreeMap::new();
    let mut projected_bindings = 0usize;
    // A binding costs two variables in the shared evaluator, so a caller may
    // declare a bound the evaluator cannot honour. Taking the smaller of the
    // two here means the excess is reported as the ordinary per-template
    // binding-count diagnostic — naming the bound that actually applied —
    // rather than surfacing later as a projection that could not be built.
    let max_bindings = limits
        .max_bindings
        .min(CatalogProjection::max_projectable_bindings());

    for document in documents {
        let mut status = Status::default();
        let template = match document.outcome {
            Err(diagnostics) => {
                status.diagnostics = diagnostics;
                None
            }
            Ok(template) => match owner.get(&template.identity) {
                Some(existing) => {
                    status.diagnostics.push(Diagnostic::at(
                        "metadata",
                        InvalidReason::DuplicateIdentity {
                            identity: template.identity.clone(),
                            existing: existing.to_string(),
                        },
                    ));
                    None
                }
                None => {
                    let count = binding_count(&template);
                    if projected_bindings + count > max_bindings {
                        status.diagnostics.push(Diagnostic::at(
                            "spec",
                            InvalidReason::LimitExceeded {
                                what: "catalog binding",
                                found: projected_bindings + count,
                                limit: max_bindings,
                            },
                        ));
                        None
                    } else {
                        projected_bindings += count;
                        owner.insert(template.identity.clone(), document.source.clone());
                        Some(template)
                    }
                }
            },
        };
        identities.push(document.identity);
        sources.push(document.source);
        templates.push(template);
        statuses.push(status);
    }

    let (projection, rejected) =
        match CatalogProjection::build(templates.iter().flatten(), max_bindings) {
            Ok(built) => built,
            // Counts are checked per template above, so this is unreachable
            // today; recovering rather than asserting keeps a future bound
            // added to the projection a diagnostic instead of an abort.
            Err(error) => {
                for (index, template) in templates.iter().enumerate() {
                    if template.is_some() {
                        statuses[index]
                            .diagnostics
                            .push(Diagnostic::at("spec", projection_reason(&error)));
                    }
                }
                templates.iter_mut().for_each(|slot| *slot = None);
                (CatalogProjection::default(), Vec::new())
            }
        };

    // Which document actually *supplied* each projected template.
    //
    // Built from the admitted templates rather than from every document's
    // claimed identity, because the two can differ: a document that failed to
    // parse still carries the identity it claimed, and a later document may be
    // the one that was projected under it. Attributing a rejection to the
    // claim rather than to the contribution files the diagnostic against a
    // template that published nothing — and leaves the one that did looking
    // valid. `owner` admits at most one template per identity, so this is
    // unambiguous.
    let projected: BTreeMap<TemplateIdentity, usize> = templates
        .iter()
        .enumerate()
        .filter_map(|(index, template)| {
            template
                .as_ref()
                .map(|template| (template.identity.clone(), index))
        })
        .collect();

    // A binding the evaluator would not accept invalidates the template that
    // published it, and only that template: the rest of the set is still
    // loadable, and a reference to the skipped binding resolves as unknown.
    for rejection in rejected {
        let Some(&index) = projected.get(&rejection.template) else {
            continue;
        };
        statuses[index].diagnostics.push(Diagnostic::at(
            "metadata",
            InvalidReason::LimitExceeded {
                what: rejection.what,
                found: rejection.found,
                limit: rejection.limit,
            },
        ));
    }

    for (index, template) in templates.iter().enumerate() {
        let Some(template) = template else { continue };
        let status = &mut statuses[index];
        check_references(template, &projection, status);
        if !status.is_invalid() && !status.is_unavailable() {
            check_values(template, &projection, status);
        }
        check_schemas(template, registry, status);
    }

    propagate_unavailability(&templates, &mut statuses);

    let entries = templates
        .into_iter()
        .zip(statuses)
        .zip(sources)
        .zip(identities)
        .map(|(((template, status), source), identity)| {
            build_entry(template, status, source, identity)
        })
        .collect();

    CatalogSet::new(entries, file_errors, projection)
}

/// Report a whole-set projection failure as a per-template reason.
fn projection_reason(error: &ProjectionError) -> InvalidReason {
    match error {
        ProjectionError::TooManyBindings { found, limit } => InvalidReason::LimitExceeded {
            what: "catalog binding",
            found: *found,
            limit: *limit,
        },
    }
}

fn build_entry(
    template: Option<Template>,
    status: Status,
    source: SourceLocation,
    identity: Option<TemplateIdentity>,
) -> CatalogEntry {
    let fingerprint = template
        .as_ref()
        .map(|template| ContentFingerprint::of(&template.canonical_bytes()));
    let result = match template {
        None => LoadResult::Invalid {
            diagnostics: status.diagnostics,
        },
        Some(template) => {
            if status.is_invalid() {
                LoadResult::Invalid {
                    diagnostics: status.diagnostics,
                }
            } else if status.is_unavailable() {
                LoadResult::Unavailable {
                    template,
                    reasons: status.unavailable,
                }
            } else {
                LoadResult::Available { template }
            }
        }
    };
    CatalogEntry {
        source,
        identity,
        // An entry whose structure was rejected has no canonical content to
        // fingerprint, so it can never be a write-conflict or instantiation
        // target.
        fingerprint: match &result {
            LoadResult::Invalid { .. } => None,
            _ => fingerprint,
        },
        result,
    }
}

/// One authored quantity, with the document path a diagnostic should point at
/// and the binding identity it is published under.
struct Quantity<'a> {
    path: String,
    kind: BindingKind,
    value: &'a QuantityValue,
}

fn quantities(template: &Template) -> Vec<Quantity<'_>> {
    let mut out = Vec::new();
    for (name, parameter) in &template.spec.parameters {
        out.push(Quantity {
            path: format!("spec.parameters.{name}"),
            kind: BindingKind::Parameter(name.clone()),
            value: &parameter.default,
        });
    }
    for (name, helper) in &template.spec.helpers {
        out.push(Quantity {
            path: format!("spec.helpers.{name}"),
            kind: BindingKind::Helper(name.clone()),
            value: &helper.value,
        });
    }
    for (index, component) in template.spec.components.iter().enumerate() {
        for (property, value) in &component.properties {
            let PropertyValue::Quantity(quantity) = value else {
                continue;
            };
            out.push(Quantity {
                path: format!("spec.components[{index}].properties.{property}"),
                kind: BindingKind::Property {
                    component: component.local_name().clone(),
                    property: property.clone(),
                },
                value: quantity,
            });
        }
    }
    out
}

/// Question 2: does every reference resolve to a binding this template may
/// see? A missing or private target is an installation fact (unavailable); an
/// ambiguous concise spelling is an authoring defect the user must qualify.
fn check_references(template: &Template, projection: &CatalogProjection, status: &mut Status) {
    for quantity in quantities(template) {
        for reference in quantity.value.expression().variables() {
            match projection.resolve(&reference, &template.identity) {
                Resolution::Visible(target) => {
                    let owner = &projection.binding(target).identity.template;
                    if owner != &template.identity {
                        status.dependencies.insert(owner.clone());
                    }
                }
                Resolution::Private { owner, .. } => {
                    status
                        .unavailable
                        .push(UnavailableReason::PrivateDependency { reference, owner });
                }
                Resolution::Ambiguous { matches } => {
                    status.diagnostics.push(Diagnostic::at(
                        &quantity.path,
                        InvalidReason::AmbiguousReference { reference, matches },
                    ));
                }
                Resolution::Unknown => {
                    status
                        .unavailable
                        .push(UnavailableReason::MissingDependency { reference });
                }
            }
        }
    }
}

/// Question 3: does every published value actually evaluate? Cycles, division
/// by zero, and non-finite results are defects in the file, not properties of
/// this installation.
fn check_values(template: &Template, projection: &CatalogProjection, status: &mut Status) {
    for quantity in quantities(template) {
        let identity = crate::binding::BindingIdentity {
            template: template.identity.clone(),
            kind: quantity.kind,
        };
        let canonical = identity.canonical_name().to_string();
        let Resolution::Visible(target) = projection.resolve(&canonical, &template.identity) else {
            // A binding of this template is not in the projection, which since
            // the evaluator gained bounds is a real state rather than an
            // impossible one: the reference it publishes can be past what the
            // evaluator reads, and such a binding is skipped. Reported here as
            // well as where the projection rejected it, so the entry is
            // classified Invalid even if that report were ever misrouted —
            // this used to be an assertion, and an assertion is exactly what
            // stops being true when a new bound is added underneath it.
            status.diagnostics.push(Diagnostic::at(
                &quantity.path,
                InvalidReason::LimitExceeded {
                    what: "binding reference",
                    found: canonical.len(),
                    limit: CatalogProjection::evaluator_limits().max_expression_bytes,
                },
            ));
            continue;
        };
        match projection.value(target) {
            Ok(value) if value.is_finite() => {}
            Ok(_) => status.diagnostics.push(Diagnostic::at(
                &quantity.path,
                InvalidReason::NonFiniteValue,
            )),
            Err(VariablesError::Eval(ExprEvalError::NonFinite)) => status.diagnostics.push(
                Diagnostic::at(&quantity.path, InvalidReason::NonFiniteValue),
            ),
            Err(VariablesError::Eval(source)) => status.diagnostics.push(Diagnostic::at(
                &quantity.path,
                InvalidReason::ExpressionEval { source },
            )),
            Err(other) => status.diagnostics.push(Diagnostic::at(
                &quantity.path,
                InvalidReason::SchemaMismatch {
                    message: other.to_string(),
                },
            )),
        }
    }
}

/// Question 4: are the component schemas installed, and does each authored
/// value match the one it declares?
///
/// The split matters: an *absent* schema leaves the entry unavailable, since
/// installing the plugin fixes it without an edit. A *present* schema the
/// value contradicts is an authoring defect only the file can fix.
fn check_schemas(template: &Template, registry: &SchemaRegistry, status: &mut Status) {
    for (index, component) in template.spec.components.iter().enumerate() {
        let Some(schema) = registry.get(&component.type_id) else {
            status
                .unavailable
                .push(UnavailableReason::UnknownComponentType {
                    component: component.type_id.clone(),
                });
            continue;
        };
        for (property, value) in &component.properties {
            let path = format!("spec.components[{index}].properties.{property}");
            let Some(declared) = schema.properties.get(property) else {
                status.unavailable.push(UnavailableReason::UnknownProperty {
                    component: component.type_id.clone(),
                    property: property.clone(),
                });
                continue;
            };
            check_property(&path, property, value, &declared.kind, status);
        }
        for required in schema.required_properties() {
            if !component.properties.contains_key(required) {
                status.diagnostics.push(Diagnostic::at(
                    format!("spec.components[{index}]"),
                    InvalidReason::MissingRequiredProperty {
                        component: component.type_id.clone(),
                        property: required.clone(),
                    },
                ));
            }
        }
    }
}

fn check_property(
    path: &str,
    property: &PropertyName,
    value: &PropertyValue,
    declared: &PropertyKind,
    status: &mut Status,
) {
    match (value, declared) {
        (PropertyValue::Quantity(quantity), PropertyKind::Quantity { dimension }) => {
            // With no unit declared the value is read in the property's own
            // canonical unit, so it cannot mismatch. A declared unit must
            // measure the dimension the schema asked for.
            if let Some(unit) = quantity.unit()
                && unit.dimension() != *dimension
            {
                status.diagnostics.push(Diagnostic::at(
                    path,
                    InvalidReason::DimensionMismatch {
                        property: property.clone(),
                        expected: *dimension,
                        found: unit.dimension(),
                    },
                ));
            }
        }
        (PropertyValue::Boolean(_), PropertyKind::Boolean)
        | (PropertyValue::Text(_), PropertyKind::Text) => {}
        (value, declared) => status.diagnostics.push(Diagnostic::at(
            path,
            InvalidReason::PropertyKindMismatch {
                property: property.clone(),
                expected: declared.label(),
                found: value.label(),
            },
        )),
    }
}

/// An entry whose expressions read a template that is itself unavailable
/// cannot supply a value either. Iterated to a fixed point so a chain of
/// dependencies propagates, bounded by the number of entries.
fn propagate_unavailability(templates: &[Option<Template>], statuses: &mut [Status]) {
    let index_of: BTreeMap<&TemplateIdentity, usize> = templates
        .iter()
        .enumerate()
        .filter_map(|(index, template)| Some((&template.as_ref()?.identity, index)))
        .collect();

    for _ in 0..statuses.len() {
        let mut newly = Vec::new();
        for (index, status) in statuses.iter().enumerate() {
            if templates[index].is_none() || status.is_invalid() {
                continue;
            }
            for dependency in &status.dependencies {
                let Some(other) = index_of.get(dependency) else {
                    continue;
                };
                let blocked = statuses[*other].is_invalid() || statuses[*other].is_unavailable();
                let already = status.unavailable.iter().any(|reason| {
                    matches!(
                        reason,
                        UnavailableReason::UnavailableDependency { reference }
                            if reference == &dependency.to_string()
                    )
                });
                if blocked && !already {
                    newly.push((
                        index,
                        UnavailableReason::UnavailableDependency {
                            reference: dependency.to_string(),
                        },
                    ));
                }
            }
        }
        if newly.is_empty() {
            return;
        }
        for (index, reason) in newly {
            statuses[index].unavailable.push(reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::TemplateDocument;
    use crate::name::{CatalogName, ComponentName, ComponentTypeId, PluginId, TemplateName};
    use crate::quantity::Dimension;
    use crate::schema::{ComponentSchema, PropertySchema, SchemaVersion};
    use crate::source::DocumentOrdinal;
    use orishu_variables::Limits as EvaluatorLimits;

    fn type_id(plugin: &str, name: &str) -> ComponentTypeId {
        ComponentTypeId::new(
            PluginId::new(plugin).unwrap(),
            ComponentName::new(name).unwrap(),
        )
    }

    fn mass_registry() -> SchemaRegistry {
        SchemaRegistry::new().with(
            ComponentSchema::new(
                type_id("kagami.mass_sources", "inertial_mass"),
                SchemaVersion(1),
            )
            .with_property(
                PropertyName::new("mass").unwrap(),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            )
            .with_property(
                PropertyName::new("label").unwrap(),
                PropertySchema::optional(PropertyKind::Text),
            ),
        )
    }

    fn parsed(index: usize, text: &str) -> ParsedDocument {
        let source = SourceLocation::new("catalog.yaml", DocumentOrdinal::from_index(index));
        let document: TemplateDocument = serde_yaml::from_str(text).unwrap();
        let identity = Some(TemplateIdentity::new(
            document.metadata.catalog.clone(),
            document.metadata.name.clone(),
        ));
        match Template::from_document(&document, &Limits::DEFAULT) {
            Ok(template) => ParsedDocument::valid(source, template),
            Err(diagnostics) => ParsedDocument::invalid(source, identity, diagnostics),
        }
    }

    fn resolved(texts: &[&str], registry: &SchemaRegistry) -> CatalogSet {
        let documents = texts
            .iter()
            .enumerate()
            .map(|(index, text)| parsed(index, text))
            .collect();
        resolve(documents, Vec::new(), registry, &Limits::DEFAULT)
    }

    fn template_text(name: &str, body: &str) -> String {
        format!(
            "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata: {{catalog: planets, \
             name: {name}}}\nspec:\n{body}"
        )
    }

    fn sun() -> String {
        template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: {expression: \"1.989e30\", unit: kg}}\n",
        )
    }

    #[test]
    fn a_template_whose_schema_is_installed_is_available() {
        let set = resolved(&[&sun()], &mass_registry());
        assert_eq!(set.summary().available, 1);
        assert!(set.entries()[0].fingerprint.is_some());
    }

    #[test]
    fn a_template_whose_plugin_is_missing_is_preserved_as_unavailable() {
        let set = resolved(&[&sun()], &SchemaRegistry::new());
        let LoadResult::Unavailable { reasons, .. } = &set.entries()[0].result else {
            panic!("expected unavailable, got {:?}", set.entries()[0].result);
        };
        assert!(matches!(
            reasons.as_slice(),
            [UnavailableReason::UnknownComponentType { .. }]
        ));
        // Preserved: the template is still inspectable and editable.
        assert!(set.entries()[0].result.template().is_some());
    }

    #[test]
    fn two_valid_entries_load_while_a_third_is_isolated_as_invalid() {
        let broken = template_text(
            "broken",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: {expression: \"1.0\", unit: m}}\n",
        );
        let earth = template_text(
            "earth",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"5.97e24\"}\n",
        );
        let set = resolved(&[&sun(), &broken, &earth], &mass_registry());
        assert_eq!(set.summary().available, 2);
        assert_eq!(set.summary().invalid, 1);
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::DimensionMismatch { .. }
        ));
    }

    #[test]
    fn a_duplicate_identity_is_rejected_and_the_first_claimant_kept() {
        let set = resolved(&[&sun(), &sun()], &mass_registry());
        assert_eq!(set.summary().available, 1);
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!("expected the second claim to be invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::DuplicateIdentity { .. }
        ));
        assert!(set.entries()[1].fingerprint.is_none());
    }

    #[test]
    fn a_missing_required_property_is_a_defect_in_the_file() {
        let text = template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties: {}\n",
        );
        let set = resolved(&[&text], &mass_registry());
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::MissingRequiredProperty { .. }
        ));
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_a_defect_in_the_file() {
        let text = template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {boolean: true}\n",
        );
        let set = resolved(&[&text], &mass_registry());
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::PropertyKindMismatch { .. }
        ));
    }

    #[test]
    fn a_property_the_schema_does_not_declare_leaves_the_entry_unavailable() {
        let text = template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"1.0\"}\n      spin: {quantity: \"1.0\"}\n",
        );
        let set = resolved(&[&text], &mass_registry());
        let LoadResult::Unavailable { reasons, .. } = &set.entries()[0].result else {
            panic!("expected unavailable");
        };
        assert!(matches!(
            reasons.as_slice(),
            [UnavailableReason::UnknownProperty { .. }]
        ));
    }

    #[test]
    fn a_reference_to_an_undefined_binding_leaves_the_entry_unavailable() {
        let text = template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.nowhere.mass\"}\n",
        );
        let set = resolved(&[&text], &mass_registry());
        let LoadResult::Unavailable { reasons, .. } = &set.entries()[0].result else {
            panic!("expected unavailable");
        };
        assert!(matches!(
            reasons.as_slice(),
            [UnavailableReason::MissingDependency { .. }]
        ));
    }

    #[test]
    fn a_reference_to_another_templates_private_helper_leaves_the_entry_unavailable() {
        let owner = template_text(
            "sun",
            "  helpers:\n    solar_mass: {expression: \"1.989e30\", unit: kg}\n  components:\n  \
             - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    properties:\n      \
             mass: {quantity: \"planets.sun.solar_mass\"}\n",
        );
        let borrower = template_text(
            "twin",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.sun.solar_mass\"}\n",
        );
        let set = resolved(&[&owner, &borrower], &mass_registry());
        assert!(set.entries()[0].result.is_available());
        let LoadResult::Unavailable { reasons, .. } = &set.entries()[1].result else {
            panic!("expected unavailable");
        };
        assert!(matches!(
            reasons.as_slice(),
            [UnavailableReason::PrivateDependency { .. }]
        ));
    }

    #[test]
    fn a_public_helper_is_visible_to_another_catalog() {
        let owner = template_text(
            "sun",
            "  helpers:\n    solar_mass: {expression: \"1.989e30\", unit: kg, visibility: public}\n  \
             components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.sun.solar_mass\"}\n",
        );
        let borrower = template_text(
            "twin",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.sun.solar_mass / 2\"}\n",
        );
        let set = resolved(&[&owner, &borrower], &mass_registry());
        assert_eq!(set.summary().available, 2);
    }

    #[test]
    fn a_cycle_between_two_templates_is_a_defect_in_the_file() {
        let one = template_text(
            "a",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.b.mass\"}\n",
        );
        let two = template_text(
            "b",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.a.mass\"}\n",
        );
        let set = resolved(&[&one, &two], &mass_registry());
        assert_eq!(set.summary().invalid, 2);
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::ExpressionEval {
                source: ExprEvalError::Cycle
            }
        ));
    }

    #[test]
    fn a_non_finite_result_is_a_defect_in_the_file() {
        let text = template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"1.0 / 0\"}\n",
        );
        let set = resolved(&[&text], &mass_registry());
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::ExpressionEval {
                source: ExprEvalError::DivisionByZero
            }
        ));
    }

    #[test]
    fn an_ambiguous_concise_reference_is_a_defect_the_author_must_qualify() {
        let owner = template_text(
            "sun",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"1.0\"}\n  - type: {plugin: kagami.mass_sources, \
             name: gravitational_mass}\n    properties:\n      mass: {quantity: \"1.0\"}\n",
        );
        let borrower = template_text(
            "twin",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.sun.mass\"}\n",
        );
        let set = resolved(&[&owner, &borrower], &mass_registry());
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::AmbiguousReference { matches: 2, .. }
        ));
    }

    #[test]
    fn depending_on_an_unavailable_template_makes_an_entry_unavailable() {
        let owner = template_text(
            "sun",
            r#"  components:
  - type: {plugin: kagami.unknown, name: mystery}
    properties:
      mass: {quantity: "1.989e30"}
"#,
        );
        let borrower = template_text(
            "twin",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"planets.sun.mass / 2\"}\n",
        );
        let set = resolved(&[&owner, &borrower], &mass_registry());
        let LoadResult::Unavailable { reasons, .. } = &set.entries()[1].result else {
            panic!("expected the dependent entry to be unavailable");
        };
        assert!(
            reasons
                .iter()
                .any(|reason| matches!(reason, UnavailableReason::UnavailableDependency { .. }))
        );
    }

    #[test]
    fn the_projected_binding_total_is_bounded() {
        let limits = Limits {
            max_bindings: 1,
            ..Limits::DEFAULT
        };
        let earth = template_text(
            "earth",
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"5.97e24\"}\n",
        );
        let set = resolve(
            vec![parsed(0, &sun()), parsed(1, &earth)],
            Vec::new(),
            &mass_registry(),
            &limits,
        );
        assert_eq!(set.summary().available, 1);
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!("expected the entry that breached the bound to be invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::LimitExceeded {
                what: "catalog binding",
                ..
            }
        ));
    }

    #[test]
    fn a_template_whose_generated_reference_is_too_long_is_isolated_not_fatal() {
        // The reference a binding publishes is generated from the identity,
        // so `catalog.template.component.property` can be past what the
        // evaluator reads even though every *authored* expression here is one
        // character. Nothing in parsing bounds a template name, so this
        // document is structurally valid and must come back as a diagnostic.
        let long = "a".repeat(EvaluatorLimits::DEFAULT.max_expression_bytes);
        let sprawling = template_text(
            &long,
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"1\"}\n",
        );
        let set = resolve(
            vec![parsed(0, &sun()), parsed(1, &sprawling)],
            Vec::new(),
            &mass_registry(),
            &Limits::DEFAULT,
        );

        // The offending entry, and only it.
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!("expected the over-long identity to be invalid, not to abort the load");
        };
        assert!(
            diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.reason,
                InvalidReason::LimitExceeded {
                    what: "binding reference",
                    ..
                }
            )),
            "{diagnostics:?}"
        );
        assert_eq!(set.summary().available, 1);
        assert!(matches!(
            set.entries()[0].result,
            LoadResult::Available { .. }
        ));
    }

    #[test]
    fn a_rejection_lands_on_the_document_that_supplied_the_template() {
        // Two documents claim one identity: the first fails to parse, so the
        // second is the one actually projected. A document that failed to
        // parse still carries the identity it claimed, so attributing the
        // rejection by identity alone files it against the wrong entry —
        // leaving the entry that really published the binding classified
        // Available with a binding the projection had skipped.
        let long = "a".repeat(EvaluatorLimits::DEFAULT.max_expression_bytes);
        let unparsable = template_text(
            &long,
            "  parameters:\n    p: {default: '1 +'}\n  components:\n  - type: {plugin: \
             kagami.mass_sources, name: inertial_mass}\n    properties:\n      mass: {quantity: \
             \"1\"}\n",
        );
        let sound = template_text(
            &long,
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"1\"}\n",
        );
        let first = parsed(0, &unparsable);
        let second = parsed(1, &sound);
        assert!(first.outcome.is_err(), "the first document must not parse");
        assert!(second.outcome.is_ok(), "the second document must parse");
        assert_eq!(first.identity, second.identity);

        let set = resolve(
            vec![first, second],
            Vec::new(),
            &mass_registry(),
            &Limits::DEFAULT,
        );

        // The projected document is the one that must be refused.
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!(
                "expected the projected entry to be invalid, got {:?}",
                set.entries()[1].result
            );
        };
        assert!(
            diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.reason,
                InvalidReason::LimitExceeded {
                    what: "binding reference",
                    ..
                }
            )),
            "{diagnostics:?}"
        );
        assert_eq!(set.summary().available, 0);

        // And it must land there rather than on the document that merely
        // claimed the identity: that one is invalid for its own reason, and
        // reporting a binding it never published against it would send a
        // reader to the wrong file.
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("the unparsable document is invalid for its own reason");
        };
        assert!(
            !diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.reason,
                InvalidReason::LimitExceeded {
                    what: "binding reference",
                    ..
                }
            )),
            "the rejection was filed against the document that published nothing: {diagnostics:?}"
        );
    }

    #[test]
    fn a_skipped_binding_is_not_published_under_any_spelling() {
        // Skipping rather than projecting is what keeps the story honest: a
        // reference to the binding resolves as unknown, instead of resolving
        // to a name the evaluator would then fail to look up.
        let long = "a".repeat(EvaluatorLimits::DEFAULT.max_expression_bytes);
        let sprawling = template_text(
            &long,
            "  components:\n  - type: {plugin: kagami.mass_sources, name: inertial_mass}\n    \
             properties:\n      mass: {quantity: \"1\"}\n",
        );
        let set = resolve(
            vec![parsed(0, &sprawling)],
            Vec::new(),
            &mass_registry(),
            &Limits::DEFAULT,
        );
        let scope = TemplateIdentity::new(
            CatalogName::new("test").unwrap(),
            TemplateName::new(&long).unwrap(),
        );
        assert_eq!(
            set.projection()
                .resolve(&format!("test.{long}.inertial_mass.mass"), &scope),
            Resolution::Unknown
        );
        assert_eq!(set.projection().bindings().count(), 0);
    }

    #[test]
    fn a_binding_bound_wider_than_the_evaluator_can_hold_is_reported_not_projected() {
        // A caller may declare `max_bindings` the shared evaluator cannot
        // honour — it spends two variables per binding. The excess has to come
        // back as the ordinary per-template diagnostic rather than as a
        // projection that could not be built.
        let limits = Limits {
            max_bindings: usize::MAX,
            ..Limits::DEFAULT
        };
        assert!(CatalogProjection::max_projectable_bindings() < limits.max_bindings);
        let set = resolve(
            vec![parsed(0, &sun())],
            Vec::new(),
            &mass_registry(),
            &limits,
        );
        // Well under the effective ceiling, so it still loads: clamping the
        // bound must not refuse ordinary catalogs.
        assert_eq!(set.summary().available, 1);
    }

    #[test]
    fn resolution_is_deterministic_for_the_same_inputs() {
        let first = resolved(&[&sun()], &mass_registry());
        let second = resolved(&[&sun()], &mass_registry());
        assert_eq!(first.entries(), second.entries());
    }
}
