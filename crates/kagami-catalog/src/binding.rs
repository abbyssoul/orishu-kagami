//! The catalog's read-only projection into the shared variable environment.
//!
//! ADR 0018 makes every expression-capable template value a binding "by
//! virtue of being a template property" — there is no export list. This
//! module gives each such value its canonical identity, decides which
//! concise spellings resolve unambiguously, and defines the whole set in one
//! [`orishu_variables::VariablesSystem`] so catalog expressions are evaluated
//! by the same engine, with the same diagnostics, as experiment authoring.
//!
//! Canonical identities are:
//!
//! | binding | canonical name |
//! | --- | --- |
//! | property  | `catalog.template.component.property` |
//! | helper    | `catalog.template.helper` |
//! | parameter | `catalog.template.parameter` |
//!
//! A property additionally gets the concise spelling
//! `catalog.template.property`, which resolves only when exactly one binding
//! claims it. Ambiguity is reported, never broken arbitrarily.

use std::collections::BTreeMap;
use std::fmt;

use orishu_variables::{
    CompiledExpression, FQName, Limits as EvaluatorLimits, Namespace, VariableId, VariablesError,
    VariablesSystem,
};

use crate::name::{ComponentName, HelperName, ParameterName, PropertyName};
use crate::source::TemplateIdentity;
use crate::template::{PropertyValue, Template, Visibility};

/// Which kind of template definition a binding was projected from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingKind {
    /// A template parameter, projected at its default value.
    Parameter(ParameterName),
    /// A template-local helper definition.
    Helper(HelperName),
    /// An expression-capable property of a composed component.
    Property {
        /// The template-local component name.
        component: ComponentName,
        /// The property's name within that component.
        property: PropertyName,
    },
}

/// A binding's stable identity: which template declared it, and as what.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingIdentity {
    /// The declaring template.
    pub template: TemplateIdentity,
    /// What kind of definition it is.
    pub kind: BindingKind,
}

impl BindingIdentity {
    /// The fully qualified name this binding is always addressable by.
    pub fn canonical_name(&self) -> FQName {
        let scope = self.template_namespace();
        match &self.kind {
            BindingKind::Parameter(name) => scope.qualified(name.as_name()),
            BindingKind::Helper(name) => scope.qualified(name.as_name()),
            BindingKind::Property {
                component,
                property,
            } => Namespace::new(format!("{scope}.{component}")).qualified(property.as_name()),
        }
    }

    /// The concise spelling a property may also be addressed by, when it is
    /// unambiguous within its template.
    pub fn concise_name(&self) -> Option<FQName> {
        match &self.kind {
            BindingKind::Property { property, .. } => {
                Some(self.template_namespace().qualified(property.as_name()))
            }
            BindingKind::Parameter(_) | BindingKind::Helper(_) => None,
        }
    }

    fn template_namespace(&self) -> Namespace {
        Namespace::new(format!(
            "{}.{}",
            self.template.catalog, self.template.template
        ))
    }
}

impl fmt::Display for BindingIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.canonical_name().fmt(formatter)
    }
}

/// One projected binding.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// Stable identity.
    pub identity: BindingIdentity,
    /// Whether it resolves outside its declaring template.
    pub visibility: Visibility,
    /// The authored expression source, retained verbatim for display and for
    /// writing the file back.
    pub source: String,
    /// The expression actually published, in canonical SI. Identical to
    /// [`Self::source`] unless a scaling unit was declared, in which case the
    /// authored magnitude has been folded into its SI value.
    ///
    /// Anything that *copies* a binding — instantiation, and eventually
    /// workload capture — must use this one: copying the authored `1.5` of a
    /// value declared in tonnes would silently produce 1.5 kg.
    pub si_source: String,
}

/// A handle into a [`CatalogProjection`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingRef(usize);

/// What a reference in an authored expression resolves to.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    /// Exactly one binding, and it is visible from the referencing scope.
    Visible(BindingRef),
    /// Exactly one binding, but it is private to another template.
    Private {
        /// The binding that was found.
        binding: BindingRef,
        /// The template it is private to.
        owner: TemplateIdentity,
    },
    /// A concise spelling that more than one binding claims.
    Ambiguous {
        /// How many bindings claim the spelling.
        matches: usize,
    },
    /// No loaded catalog defines this name.
    Unknown,
}

/// Why a projection could not be built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProjectionError {
    /// The loaded catalogs publish more bindings than the declared bound.
    #[error("catalog set publishes {found} bindings, over the limit of {limit}")]
    TooManyBindings { found: usize, limit: usize },
}

/// A binding the shared evaluator would not accept, named so its template can
/// be isolated rather than the whole catalog set failing.
///
/// The reference a binding publishes is *generated* from its identity —
/// `catalog.template.component.property` — so it can be longer than any
/// expression anyone authored, and it is an expression in its own right: the
/// concise alias resolves by naming it. A template with a very long name can
/// therefore be structurally valid, parse cleanly, and still be beyond what
/// the evaluator will read. That is a diagnostic about one template, not a
/// reason to refuse every other catalog on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedBinding {
    /// The template the binding belongs to.
    pub template: TemplateIdentity,
    /// What was over its bound, phrased for [`crate::InvalidReason`].
    pub what: &'static str,
    /// The size found.
    pub found: usize,
    /// The size permitted.
    pub limit: usize,
}

/// Every binding the currently loaded catalogs publish, plus the one
/// variables system they are evaluated in.
pub struct CatalogProjection {
    bindings: Vec<Binding>,
    canonical: BTreeMap<String, BindingRef>,
    concise: BTreeMap<String, Vec<BindingRef>>,
    variables: VariablesSystem,
    variable_ids: Vec<VariableId>,
}

impl fmt::Debug for CatalogProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CatalogProjection")
            .field("bindings", &self.bindings.len())
            .finish_non_exhaustive()
    }
}

impl Default for CatalogProjection {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            canonical: BTreeMap::new(),
            concise: BTreeMap::new(),
            variables: VariablesSystem::with_limits(Self::evaluator_limits()),
            variable_ids: Vec::new(),
        }
    }
}

impl CatalogProjection {
    /// The shared-evaluator bounds a projection is built under.
    ///
    /// Named here rather than left implicit in a `VariablesSystem::default()`
    /// so the loader can price a template against the same numbers the
    /// projection will hold it to. Two places deciding this separately is how
    /// a catalog becomes loadable by one and impossible for the other.
    pub fn evaluator_limits() -> EvaluatorLimits {
        EvaluatorLimits::DEFAULT
    }

    /// The most bindings a projection can hold, whatever a caller's
    /// [`crate::Limits::max_bindings`] says.
    ///
    /// A binding costs two variables — its canonical name and its concise
    /// alias — so the evaluator's variable bound, not the catalog's, is the
    /// real ceiling.
    pub fn max_projectable_bindings() -> usize {
        Self::evaluator_limits().max_variables / 2
    }

    /// Project every expression-capable definition of `templates` into one
    /// variable environment.
    ///
    /// `templates` must already be structurally valid and free of duplicate
    /// identities; the loader guarantees both before calling this.
    ///
    /// Returns the projection together with any bindings the evaluator would
    /// not accept. Those are *skipped*, not projected under a different name:
    /// a reference to one then resolves as unknown, which is the truth, and
    /// its template is reported so the caller can isolate it.
    pub fn build<'a>(
        templates: impl IntoIterator<Item = &'a Template>,
        max_bindings: usize,
    ) -> Result<(Self, Vec<RejectedBinding>), ProjectionError> {
        let mut projection = Self::default();
        let mut rejected = Vec::new();
        for template in templates {
            projection.add_template(template, max_bindings, &mut rejected)?;
        }
        projection.define_concise_aliases(&mut rejected);
        Ok((projection, rejected))
    }

    fn add_template(
        &mut self,
        template: &Template,
        max_bindings: usize,
        rejected: &mut Vec<RejectedBinding>,
    ) -> Result<(), ProjectionError> {
        let identity = &template.identity;
        for (name, parameter) in &template.spec.parameters {
            self.add_binding(
                BindingIdentity {
                    template: identity.clone(),
                    kind: BindingKind::Parameter(name.clone()),
                },
                &parameter.default,
                max_bindings,
                rejected,
            )?;
        }
        for (name, helper) in &template.spec.helpers {
            self.add_binding(
                BindingIdentity {
                    template: identity.clone(),
                    kind: BindingKind::Helper(name.clone()),
                },
                &helper.value,
                max_bindings,
                rejected,
            )?;
        }
        for component in &template.spec.components {
            for (property, value) in &component.properties {
                let PropertyValue::Quantity(quantity) = value else {
                    continue;
                };
                self.add_binding(
                    BindingIdentity {
                        template: identity.clone(),
                        kind: BindingKind::Property {
                            component: component.local_name().clone(),
                            property: property.clone(),
                        },
                    },
                    quantity,
                    max_bindings,
                    rejected,
                )?;
            }
        }
        Ok(())
    }

    fn add_binding(
        &mut self,
        identity: BindingIdentity,
        value: &crate::template::QuantityValue,
        max_bindings: usize,
        rejected: &mut Vec<RejectedBinding>,
    ) -> Result<(), ProjectionError> {
        if self.bindings.len() >= max_bindings {
            return Err(ProjectionError::TooManyBindings {
                found: self.bindings.len() + 1,
                limit: max_bindings,
            });
        }
        let canonical = identity.canonical_name();
        // The reference this binding publishes is itself an expression — the
        // concise alias below resolves by naming it, and other templates write
        // it in their sources — so the evaluator has to be willing to read it.
        // It is generated from the identity rather than authored, so no bound
        // on what anyone typed constrains its length.
        let reference = canonical.to_string();
        let allowed = Self::evaluator_limits().max_expression_bytes;
        if reference.len() > allowed {
            rejected.push(RejectedBinding {
                template: identity.template.clone(),
                what: "binding reference",
                found: reference.len(),
                limit: allowed,
            });
            return Ok(());
        }

        let handle = match self.variables.define(
            canonical.namespace(),
            canonical.name().clone(),
            value.si_expression().clone(),
            Default::default(),
        ) {
            Ok(handle) => handle,
            // Canonical names are unique by construction — duplicate template
            // identities and duplicate component names are both rejected
            // before a projection is built — so this is the evaluator
            // declining the binding on some other ground. Report it against
            // its template rather than asserting a shape this module does not
            // own.
            Err(error) => {
                rejected.push(Self::declined(&identity, &error));
                return Ok(());
            }
        };

        let reference = BindingRef(self.bindings.len());
        if let Some(concise) = identity.concise_name() {
            self.concise
                .entry(concise.to_string())
                .or_default()
                .push(reference);
        }
        self.canonical.insert(canonical.to_string(), reference);
        self.bindings.push(Binding {
            visibility: value.visibility(),
            source: value.expression().source().to_owned(),
            si_source: value.si_expression().source().to_owned(),
            identity,
        });
        self.variable_ids.push(handle);
        Ok(())
    }

    /// Define each unambiguous concise spelling as an alias variable, so the
    /// shared evaluator resolves `planets.sun.mass` without the catalog
    /// rewriting authored source.
    fn define_concise_aliases(&mut self, rejected: &mut Vec<RejectedBinding>) {
        let limits = Self::evaluator_limits();
        let mut aliases = Vec::new();
        for (name, claimants) in &self.concise {
            let [only] = claimants.as_slice() else {
                continue;
            };
            if self.canonical.contains_key(name) {
                continue;
            }
            let Ok(alias) = FQName::parse(name) else {
                continue;
            };
            let identity = &self.bindings[only.0].identity;
            let target = identity.canonical_name().to_string();
            // Parsed under the projection's own bounds, not the defaults, so
            // the alias is held to exactly what the system it is defined in
            // will hold it to.
            match CompiledExpression::parse_bounded(&target, &limits) {
                Ok(expression) => aliases.push((alias, expression, identity.clone())),
                Err(error) => rejected.push(RejectedBinding {
                    template: identity.template.clone(),
                    what: "binding reference",
                    found: error
                        .limit_error()
                        .map_or(target.len(), |limit| limit.found as usize),
                    limit: limits.max_expression_bytes,
                }),
            }
        }
        for (alias, expression, identity) in aliases {
            if let Err(error) = self.variables.define(
                alias.namespace(),
                alias.name().clone(),
                expression,
                Default::default(),
            ) {
                rejected.push(Self::declined(&identity, &error));
            }
        }
    }

    /// Report the evaluator declining a binding, preserving the bound it named
    /// where it named one.
    fn declined(identity: &BindingIdentity, error: &VariablesError) -> RejectedBinding {
        let (found, limit) = match error {
            VariablesError::Limit(limit) => (limit.found as usize, limit.allowed as usize),
            _ => (0, 0),
        };
        RejectedBinding {
            template: identity.template.clone(),
            what: "projected binding",
            found,
            limit,
        }
    }

    /// Resolve a reference as written in an expression, from `scope`.
    pub fn resolve(&self, reference: &str, scope: &TemplateIdentity) -> Resolution {
        let found = match self.canonical.get(reference) {
            Some(reference) => *reference,
            None => match self.concise.get(reference).map(Vec::as_slice) {
                Some([only]) => *only,
                None | Some([]) => return Resolution::Unknown,
                Some(many) => {
                    return Resolution::Ambiguous {
                        matches: many.len(),
                    };
                }
            },
        };
        let binding = &self.bindings[found.0];
        if binding.visibility == Visibility::Private && &binding.identity.template != scope {
            return Resolution::Private {
                binding: found,
                owner: binding.identity.template.clone(),
            };
        }
        Resolution::Visible(found)
    }

    /// The binding behind a handle.
    pub fn binding(&self, reference: BindingRef) -> &Binding {
        &self.bindings[reference.0]
    }

    /// Every projected binding, in projection order.
    pub fn bindings(&self) -> impl Iterator<Item = (BindingRef, &Binding)> {
        self.bindings
            .iter()
            .enumerate()
            .map(|(index, binding)| (BindingRef(index), binding))
    }

    /// How many bindings the loaded catalogs publish.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` when no catalog publishes an expression-capable value.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Evaluate a binding to its canonical SI magnitude.
    pub fn value(&self, reference: BindingRef) -> Result<f64, VariablesError> {
        self.variables
            .value(self.variable_ids[reference.0])
            .map(|quantity| quantity.magnitude())
    }

    /// The variable environment the catalog publishes into, for a caller
    /// that needs to evaluate an ad-hoc expression against it.
    pub fn variables(&self) -> &VariablesSystem {
        &self.variables
    }
}

/// How many bindings `template` would contribute to a projection.
///
/// Lets a loader decide *before* projecting whether admitting one more
/// template would breach [`crate::limits::Limits::max_bindings`], so the
/// entry that would have breached it is reported rather than the whole set
/// failing.
pub fn binding_count(template: &Template) -> usize {
    template.spec.parameters.len()
        + template.spec.helpers.len()
        + template
            .spec
            .components
            .iter()
            .map(|component| {
                component
                    .properties
                    .values()
                    .filter(|value| matches!(value, PropertyValue::Quantity(_)))
                    .count()
            })
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::TemplateDocument;
    use crate::limits::Limits;

    fn template(text: &str) -> Template {
        let document: TemplateDocument = serde_yaml::from_str(text).unwrap();
        Template::from_document(&document, &Limits::DEFAULT).unwrap()
    }

    fn identity(catalog: &str, name: &str) -> TemplateIdentity {
        TemplateIdentity::new(catalog.try_into().unwrap(), name.try_into().unwrap())
    }

    const SUN: &str = r#"
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: sun}
spec:
  parameters:
    scale: {default: "1"}
  helpers:
    solar_mass: {expression: "1.989e30", unit: kg}
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "planets.sun.solar_mass * planets.sun.scale"}
  - type: {plugin: kagami.geometry, name: sphere}
    properties:
      radius: {quantity: {expression: "6.9634e5", unit: km}}
"#;

    #[test]
    fn every_expression_capable_value_is_projected_at_its_canonical_identity() {
        let (projection, _) = CatalogProjection::build([&template(SUN)], 1024).unwrap();
        let names: Vec<String> = projection
            .bindings()
            .map(|(_, binding)| binding.identity.canonical_name().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "planets.sun.scale",
                "planets.sun.solar_mass",
                "planets.sun.inertial_mass.mass",
                "planets.sun.sphere.radius",
            ]
        );
    }

    #[test]
    fn a_property_also_resolves_by_its_concise_spelling() {
        let (projection, _) = CatalogProjection::build([&template(SUN)], 1024).unwrap();
        let scope = identity("other", "thing");
        let Resolution::Visible(reference) = projection.resolve("planets.sun.mass", &scope) else {
            panic!("concise spelling should resolve");
        };
        assert_eq!(
            projection
                .binding(reference)
                .identity
                .canonical_name()
                .to_string(),
            "planets.sun.inertial_mass.mass"
        );
    }

    #[test]
    fn values_resolve_through_the_shared_evaluator_in_canonical_si() {
        let (projection, _) = CatalogProjection::build([&template(SUN)], 1024).unwrap();
        let scope = identity("planets", "sun");
        let Resolution::Visible(mass) = projection.resolve("planets.sun.mass", &scope) else {
            panic!("mass should resolve");
        };
        assert_eq!(projection.value(mass).unwrap(), 1.989e30);
        let Resolution::Visible(radius) = projection.resolve("planets.sun.radius", &scope) else {
            panic!("radius should resolve");
        };
        assert!((projection.value(radius).unwrap() - 6.9634e8).abs() < 1.0);
    }

    #[test]
    fn a_helper_is_private_outside_its_declaring_template() {
        let (projection, _) = CatalogProjection::build([&template(SUN)], 1024).unwrap();
        assert!(matches!(
            projection.resolve("planets.sun.solar_mass", &identity("planets", "sun")),
            Resolution::Visible(_)
        ));
        assert!(matches!(
            projection.resolve("planets.sun.solar_mass", &identity("other", "thing")),
            Resolution::Private { .. }
        ));
    }

    #[test]
    fn an_unknown_name_resolves_to_nothing_rather_than_a_guess() {
        let (projection, _) = CatalogProjection::build([&template(SUN)], 1024).unwrap();
        assert_eq!(
            projection.resolve("planets.sun.charge", &identity("planets", "sun")),
            Resolution::Unknown
        );
    }

    #[test]
    fn a_concise_spelling_two_components_claim_is_ambiguous() {
        let text = SUN.replace(
            "  - type: {plugin: kagami.geometry, name: sphere}\n    properties:\n      radius: {quantity: {expression: \"6.9634e5\", unit: km}}",
            "  - type: {plugin: kagami.mass_sources, name: gravitational_mass}\n    properties:\n      mass: {quantity: \"1.0\"}",
        );
        let (projection, _) = CatalogProjection::build([&template(&text)], 1024).unwrap();
        assert_eq!(
            projection.resolve("planets.sun.mass", &identity("planets", "sun")),
            Resolution::Ambiguous { matches: 2 }
        );
        // The canonical identities remain addressable and unambiguous.
        assert!(matches!(
            projection.resolve(
                "planets.sun.inertial_mass.mass",
                &identity("planets", "sun")
            ),
            Resolution::Visible(_)
        ));
    }

    #[test]
    fn a_concise_spelling_that_collides_with_a_helper_prefers_the_exact_name() {
        let text = SUN.replace("      mass: {quantity:", "      solar_mass: {quantity:");
        let (projection, _) = CatalogProjection::build([&template(&text)], 1024).unwrap();
        let Resolution::Visible(reference) =
            projection.resolve("planets.sun.solar_mass", &identity("planets", "sun"))
        else {
            panic!("the helper's canonical name should win");
        };
        assert!(matches!(
            projection.binding(reference).identity.kind,
            BindingKind::Helper(_)
        ));
    }

    #[test]
    fn bindings_from_two_catalogs_resolve_against_each_other() {
        let moon = r#"
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: moons, name: luna}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "planets.sun.mass / 2.7e7"}
"#;
        let (projection, _) =
            CatalogProjection::build([&template(SUN), &template(moon)], 1024).unwrap();
        let Resolution::Visible(mass) =
            projection.resolve("moons.luna.mass", &identity("moons", "luna"))
        else {
            panic!("cross-catalog reference should resolve");
        };
        assert!((projection.value(mass).unwrap() - 1.989e30 / 2.7e7).abs() < 1.0e18);
    }

    #[test]
    fn the_binding_count_is_bounded() {
        assert_eq!(
            CatalogProjection::build([&template(SUN)], 2).unwrap_err(),
            ProjectionError::TooManyBindings { found: 3, limit: 2 }
        );
    }
}
