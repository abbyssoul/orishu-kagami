//! Materialising a template into a self-contained object candidate.
//!
//! This is the pure half of ADR 0008's `InstantiateObjectTemplate`: given one
//! immutable [`CatalogSet`] snapshot, a template identity, and the caller's
//! parameter bindings, it produces the complete object state a document
//! authority would commit — or rejects the whole thing. Nothing here mutates
//! a document, mints an identity, or touches a file; the authority that owns
//! experiment intent does that with the value returned.
//!
//! The load-bearing property is **self-containment**. The candidate copies
//! the complete transitive closure of every definition its expressions need
//! into object-local identities and rewrites every reference to point at
//! those copies. An accepted object therefore resolves without the catalog
//! that produced it: renaming, editing, deleting, or losing the source
//! template cannot change it, and there is no tracking link, no propagation
//! index, and no compare/apply path back. The recorded
//! [`TemplateProvenance`] is historical evidence, not a pointer.

use std::collections::{BTreeMap, BTreeSet};

use orishu_variables::{
    CompiledExpression, ExprEvalError, Name, Namespace, VariableOptions, VariablesSystem,
};
use thiserror::Error;

use crate::binding::{BindingIdentity, BindingKind, BindingRef, Resolution};
use crate::entry::CatalogSet;
use crate::name::{ComponentTypeId, ParameterName, PropertyName};
use crate::quantity::Dimension;
use crate::schema::{PropertyKind, SchemaRegistry, SchemaVersion};
use crate::source::{ContentFingerprint, TemplateIdentity, TemplateProvenance};
use crate::template::{PropertyValue, Template};

pub use orishu_variables::rewrite_symbols;

/// What the caller asks for when instantiating a template.
#[derive(Clone, Debug, PartialEq)]
pub struct InstantiationRequest {
    /// Which template to instantiate.
    pub identity: TemplateIdentity,
    /// The content fingerprint the caller believes it is instantiating.
    /// A mismatch rejects the request rather than silently using newer
    /// content the caller never saw.
    pub expected_fingerprint: Option<ContentFingerprint>,
    /// Parameter overrides, as authored expressions. An override may itself
    /// reference a public catalog binding or a document variable supplied in
    /// [`Self::document_values`].
    pub bindings: BTreeMap<ParameterName, String>,
    /// The document-local namespace the copied definitions are placed in.
    /// The document authority supplies the new object's own scope.
    pub object_scope: Namespace,
    /// Already-resolved document variables an override or the template may
    /// reference, keyed by the name as written. They are captured as
    /// literals: instantiation materialises, it does not link.
    pub document_values: BTreeMap<String, f64>,
}

impl InstantiationRequest {
    /// A request with no parameter overrides and no document variables.
    pub fn new(identity: TemplateIdentity, object_scope: Namespace) -> Self {
        Self {
            identity,
            expected_fingerprint: None,
            bindings: BTreeMap::new(),
            object_scope,
            document_values: BTreeMap::new(),
        }
    }
}

/// One definition copied into the new object's local scope.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectDefinition {
    /// The expression source, rewritten to object-local names.
    pub source: String,
    /// Its canonical SI magnitude at the moment of instantiation.
    pub si_value: f64,
    /// Where the definition came from, for display. Historical only.
    pub origin: DefinitionOrigin,
}

/// What a copied definition was before it was copied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefinitionOrigin {
    /// A catalog binding, named by its canonical identity at capture time.
    Catalog(BindingIdentity),
    /// A document variable, captured as a literal.
    Document(String),
}

/// One property of a materialised component.
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectPropertyValue {
    /// A dimensioned value, retaining both the rewritten expression and the
    /// magnitude it resolved to.
    Quantity {
        /// Expression source, rewritten to object-local names.
        source: String,
        /// Canonical SI magnitude.
        si_value: f64,
        /// The dimension the property's schema declared.
        dimension: Dimension,
    },
    /// A flag.
    Boolean(bool),
    /// Free-form text.
    Text(String),
}

/// One component of a materialised object.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectComponent {
    /// The plugin-qualified component type.
    pub type_id: ComponentTypeId,
    /// The schema version the values were checked against.
    pub schema_version: SchemaVersion,
    /// Authored property values.
    pub properties: BTreeMap<PropertyName, ObjectPropertyValue>,
}

/// A complete, self-contained object ready to be committed as one document
/// revision.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectCandidate {
    /// Where it came from. Historical evidence, never a live link.
    pub provenance: TemplateProvenance,
    /// The namespace the definitions live in.
    pub scope: Namespace,
    /// Every definition the object's expressions need, copied local.
    pub definitions: BTreeMap<Name, ObjectDefinition>,
    /// The materialised components, in template order.
    pub components: Vec<ObjectComponent>,
}

impl ObjectCandidate {
    /// Re-evaluate this candidate's expressions using only its own copied
    /// definitions.
    ///
    /// This is what "self-contained" means operationally, and it is why the
    /// crate can prove the property in a test: the returned values are
    /// computed with no catalog in scope at all.
    pub fn resolve_standalone(&self) -> Result<BTreeMap<String, f64>, InstantiationError> {
        let mut system = VariablesSystem::default();
        for (name, definition) in &self.definitions {
            let compiled = CompiledExpression::parse(&definition.source).map_err(|_| {
                InstantiationError::UnresolvedReference {
                    reference: name.to_string(),
                }
            })?;
            system
                .define(
                    &self.scope,
                    name.clone(),
                    compiled,
                    VariableOptions::default(),
                )
                .map_err(|_| InstantiationError::UnresolvedReference {
                    reference: name.to_string(),
                })?;
        }
        let mut values = BTreeMap::new();
        for component in &self.components {
            for (property, value) in &component.properties {
                let ObjectPropertyValue::Quantity { source, .. } = value else {
                    continue;
                };
                let key = format!("{}.{property}", component.type_id.name);
                let resolved =
                    system
                        .eval(source)
                        .map_err(|_| InstantiationError::UnresolvedReference {
                            reference: source.clone(),
                        })?;
                values.insert(key, resolved.magnitude());
            }
        }
        Ok(values)
    }
}

/// Why an instantiation was refused. A refusal produces no partial object.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum InstantiationError {
    /// No loaded catalog defines this template.
    #[error("no loaded catalog defines template `{0}`")]
    UnknownTemplate(TemplateIdentity),
    /// The template is not in a state that can be instantiated.
    #[error("template `{identity}` is {state}, not available")]
    NotAvailable {
        identity: TemplateIdentity,
        state: &'static str,
    },
    /// The caller's expected fingerprint does not match the loaded content.
    #[error("template `{identity}` has changed since it was read")]
    StaleFingerprint {
        identity: TemplateIdentity,
        expected: ContentFingerprint,
        actual: Option<ContentFingerprint>,
    },
    /// The request binds a parameter the template does not declare.
    #[error("template `{identity}` has no parameter `{parameter}`")]
    UnknownParameter {
        identity: TemplateIdentity,
        parameter: ParameterName,
    },
    /// A parameter binding does not parse.
    #[error("binding for parameter `{parameter}` is not a valid expression: {message}")]
    InvalidBinding {
        parameter: ParameterName,
        message: String,
    },
    /// A reference resolves to nothing the instantiation may capture.
    #[error("`{reference}` is not a visible catalog binding or a supplied document variable")]
    UnresolvedReference { reference: String },
    /// The candidate's closure could not be evaluated.
    #[error("`{reference}` could not be evaluated: {source}")]
    Evaluation {
        reference: String,
        #[source]
        source: ExprEvalError,
    },
    /// A value resolved to NaN or an infinity.
    #[error("`{reference}` resolved to a value that is not finite")]
    NonFinite { reference: String },
    /// A component schema the template names is not installed. Only an
    /// available template reaches this, so it means the registry passed to
    /// instantiation differs from the one the set was resolved against.
    #[error("component type `{0}` is not installed")]
    UnknownComponentType(ComponentTypeId),
}

/// Materialise `request` against one immutable catalog snapshot.
///
/// Either returns a complete candidate whose every expression resolves using
/// only its own copied definitions, or rejects the request without producing
/// anything partial.
pub fn materialize(
    set: &CatalogSet,
    registry: &SchemaRegistry,
    request: &InstantiationRequest,
) -> Result<ObjectCandidate, InstantiationError> {
    let entry = set
        .get(&request.identity)
        .ok_or_else(|| InstantiationError::UnknownTemplate(request.identity.clone()))?;
    let crate::entry::LoadResult::Available { template } = &entry.result else {
        return Err(InstantiationError::NotAvailable {
            identity: request.identity.clone(),
            state: entry.result.state(),
        });
    };
    if let Some(expected) = request.expected_fingerprint
        && entry.fingerprint != Some(expected)
    {
        return Err(InstantiationError::StaleFingerprint {
            identity: request.identity.clone(),
            expected,
            actual: entry.fingerprint,
        });
    }

    for parameter in request.bindings.keys() {
        if !template.spec.parameters.contains_key(parameter) {
            return Err(InstantiationError::UnknownParameter {
                identity: request.identity.clone(),
                parameter: parameter.clone(),
            });
        }
    }

    let closure = Closure::collect(set, template, request)?;
    let definitions = closure.into_definitions(&request.object_scope)?;
    let components = materialize_components(template, registry, &definitions)?;

    let candidate = ObjectCandidate {
        provenance: entry
            .provenance()
            .expect("an available entry has an identity and a fingerprint"),
        scope: request.object_scope.clone(),
        definitions: definitions.local,
        components,
    };
    Ok(candidate)
}

/// The transitive set of definitions an instantiation must copy, keyed by the
/// name each was referenced under.
struct Closure<'a> {
    /// Catalog bindings reached, keyed by canonical name.
    catalog: BTreeMap<String, BindingRef>,
    /// Every spelling a reached binding was actually referenced by, mapped to
    /// its canonical name. A concise `planets.sun.mass` and the canonical
    /// `planets.sun.inertial_mass.mass` name one binding, and both spellings
    /// have to be rewritten onto the one object-local copy.
    spellings: BTreeMap<String, String>,
    /// Document variables reached, keyed by the name as written.
    document: BTreeMap<String, f64>,
    /// Parameter expressions, after applying the request's overrides.
    parameters: BTreeMap<String, String>,
    set: &'a CatalogSet,
}

impl<'a> Closure<'a> {
    fn collect(
        set: &'a CatalogSet,
        template: &Template,
        request: &InstantiationRequest,
    ) -> Result<Self, InstantiationError> {
        let mut closure = Self {
            catalog: BTreeMap::new(),
            spellings: BTreeMap::new(),
            document: BTreeMap::new(),
            parameters: BTreeMap::new(),
            set,
        };

        // A parameter binding is authored by the *instantiating* caller, not
        // by the template, so it is resolved from an outside scope: another
        // template's private helper is not reachable through it.
        let outside = TemplateIdentity::new(
            "instantiation".try_into().expect("a valid literal name"),
            "scope".try_into().expect("a valid literal name"),
        );
        for (name, parameter) in &template.spec.parameters {
            let source = match request.bindings.get(name) {
                Some(override_source) => {
                    CompiledExpression::parse(override_source).map_err(|error| {
                        InstantiationError::InvalidBinding {
                            parameter: name.clone(),
                            message: error.to_string(),
                        }
                    })?;
                    override_source.clone()
                }
                None => parameter.default.si_expression().source().to_owned(),
            };
            let scope = if request.bindings.contains_key(name) {
                &outside
            } else {
                &template.identity
            };
            closure.walk(&source, scope, request)?;
            closure.parameters.insert(
                BindingIdentity {
                    template: template.identity.clone(),
                    kind: BindingKind::Parameter(name.clone()),
                }
                .canonical_name()
                .to_string(),
                source,
            );
        }

        for component in &template.spec.components {
            for value in component.properties.values() {
                let PropertyValue::Quantity(quantity) = value else {
                    continue;
                };
                closure.walk(
                    quantity.si_expression().source(),
                    &template.identity,
                    request,
                )?;
            }
        }
        Ok(closure)
    }

    /// Add every binding `source` references, and everything they reference
    /// in turn, resolving each from the scope that authored it.
    fn walk(
        &mut self,
        source: &str,
        scope: &TemplateIdentity,
        request: &InstantiationRequest,
    ) -> Result<(), InstantiationError> {
        let compiled = CompiledExpression::parse(source).map_err(|error| {
            InstantiationError::UnresolvedReference {
                reference: error.to_string(),
            }
        })?;
        let mut pending: Vec<(String, TemplateIdentity)> = compiled
            .variables()
            .into_iter()
            .map(|reference| (reference, scope.clone()))
            .collect();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        while let Some((reference, scope)) = pending.pop() {
            if !seen.insert(reference.clone()) {
                continue;
            }
            match self.set.projection().resolve(&reference, &scope) {
                Resolution::Visible(binding) => {
                    let projected = self.set.projection().binding(binding);
                    let canonical = projected.identity.canonical_name().to_string();
                    self.spellings.insert(reference, canonical.clone());
                    if self.catalog.insert(canonical.clone(), binding).is_some() {
                        continue;
                    }
                    // A parameter's own expression is supplied by the caller,
                    // so its default's references are walked above, not here.
                    if matches!(projected.identity.kind, BindingKind::Parameter(_))
                        && self.parameters.contains_key(&canonical)
                    {
                        continue;
                    }
                    let owner = projected.identity.template.clone();
                    let inner =
                        CompiledExpression::parse(&projected.si_source).map_err(|error| {
                            InstantiationError::UnresolvedReference {
                                reference: error.to_string(),
                            }
                        })?;
                    for next in inner.variables() {
                        pending.push((next, owner.clone()));
                    }
                }
                Resolution::Private { .. } | Resolution::Ambiguous { .. } | Resolution::Unknown => {
                    match request.document_values.get(&reference) {
                        Some(value) => {
                            self.document.insert(reference, *value);
                        }
                        None => {
                            return Err(InstantiationError::UnresolvedReference { reference });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Assign each captured definition a deterministic object-local name,
    /// rewrite every expression onto those names, and evaluate the result.
    fn into_definitions(self, scope: &Namespace) -> Result<Definitions, InstantiationError> {
        let mut names = LocalNames::default();
        let mut renames: BTreeMap<String, String> = BTreeMap::new();
        for (canonical, binding) in &self.catalog {
            let identity = &self.set.projection().binding(*binding).identity;
            let local = names.assign(&local_stem(identity));
            renames.insert(canonical.clone(), qualified(scope, &local));
        }
        for reference in self.document.keys() {
            let local = names.assign(&reference.replace('.', "__"));
            renames.insert(reference.clone(), qualified(scope, &local));
        }
        for (spelling, canonical) in &self.spellings {
            let local = renames[canonical].clone();
            renames.insert(spelling.clone(), local);
        }

        let mut system = VariablesSystem::default();
        let mut local = BTreeMap::new();
        for (canonical, binding) in &self.catalog {
            let projected = self.set.projection().binding(*binding);
            // The *published* form, never the authored one: copying the
            // authored `1.5` of a value declared in tonnes would silently
            // produce 1.5 kg in the new object.
            let published = self
                .parameters
                .get(canonical)
                .map(String::as_str)
                .unwrap_or(projected.si_source.as_str());
            let source = rewrite_symbols(published, &renames);
            let name = trailing_name(&renames[canonical]);
            define(&mut system, scope, &name, &source)?;
            local.insert(
                name,
                ObjectDefinition {
                    source,
                    si_value: 0.0,
                    origin: DefinitionOrigin::Catalog(projected.identity.clone()),
                },
            );
        }
        for (reference, value) in &self.document {
            if !value.is_finite() {
                return Err(InstantiationError::NonFinite {
                    reference: reference.clone(),
                });
            }
            let source = format!("{value:?}");
            let name = trailing_name(&renames[reference]);
            define(&mut system, scope, &name, &source)?;
            local.insert(
                name,
                ObjectDefinition {
                    source,
                    si_value: *value,
                    origin: DefinitionOrigin::Document(reference.clone()),
                },
            );
        }

        for (name, definition) in &mut local {
            let handle = system
                .lookup_in(scope, name)
                .expect("every definition was just defined");
            // A `Quantity` is finite by construction, so a non-finite result
            // is reported by the evaluator rather than re-checked here.
            let value = system
                .value(handle)
                .map_err(|error| evaluation_error(name.as_str(), error))?;
            definition.si_value = value.magnitude();
        }

        Ok(Definitions {
            local,
            renames,
            system,
        })
    }
}

/// The object-local definitions, the rename map that produced them, and the
/// system they evaluate in.
struct Definitions {
    local: BTreeMap<Name, ObjectDefinition>,
    renames: BTreeMap<String, String>,
    system: VariablesSystem,
}

impl Definitions {
    fn resolve(&self, source: &str) -> Result<(String, f64), InstantiationError> {
        let rewritten = rewrite_symbols(source, &self.renames);
        let value = self
            .system
            .eval(&rewritten)
            .map_err(|error| evaluation_error(&rewritten, error))?;
        Ok((rewritten, value.magnitude()))
    }
}

fn materialize_components(
    template: &Template,
    registry: &SchemaRegistry,
    definitions: &Definitions,
) -> Result<Vec<ObjectComponent>, InstantiationError> {
    template
        .spec
        .components
        .iter()
        .map(|component| {
            let schema = registry.get(&component.type_id).ok_or_else(|| {
                InstantiationError::UnknownComponentType(component.type_id.clone())
            })?;
            let properties = component
                .properties
                .iter()
                .map(|(name, value)| {
                    let materialized = match value {
                        PropertyValue::Quantity(quantity) => {
                            let (source, si_value) =
                                definitions.resolve(quantity.si_expression().source())?;
                            let dimension = match schema.properties.get(name).map(|p| &p.kind) {
                                Some(PropertyKind::Quantity { dimension }) => *dimension,
                                // An available entry has already been checked
                                // against this registry; anything else here
                                // means the caller swapped registries.
                                _ => Dimension::DIMENSIONLESS,
                            };
                            ObjectPropertyValue::Quantity {
                                source,
                                si_value,
                                dimension,
                            }
                        }
                        PropertyValue::Boolean(value) => ObjectPropertyValue::Boolean(*value),
                        PropertyValue::Text(text) => ObjectPropertyValue::Text(text.clone()),
                    };
                    Ok((name.clone(), materialized))
                })
                .collect::<Result<BTreeMap<_, _>, InstantiationError>>()?;
            Ok(ObjectComponent {
                type_id: component.type_id.clone(),
                schema_version: schema.version,
                properties,
            })
        })
        .collect()
}

fn define(
    system: &mut VariablesSystem,
    scope: &Namespace,
    name: &Name,
    source: &str,
) -> Result<(), InstantiationError> {
    let compiled =
        CompiledExpression::parse(source).map_err(|_| InstantiationError::UnresolvedReference {
            reference: source.to_owned(),
        })?;
    system
        .define(scope, name.clone(), compiled, VariableOptions::default())
        .map_err(|_| InstantiationError::UnresolvedReference {
            reference: name.to_string(),
        })?;
    Ok(())
}

fn evaluation_error(
    reference: &str,
    error: orishu_variables::VariablesError,
) -> InstantiationError {
    match error {
        orishu_variables::VariablesError::Eval(ExprEvalError::NonFinite) => {
            InstantiationError::NonFinite {
                reference: reference.to_owned(),
            }
        }
        orishu_variables::VariablesError::Eval(source) => InstantiationError::Evaluation {
            reference: reference.to_owned(),
            source,
        },
        _ => InstantiationError::UnresolvedReference {
            reference: reference.to_owned(),
        },
    }
}

/// Deterministic object-local stem for a captured catalog binding.
fn local_stem(identity: &BindingIdentity) -> String {
    let prefix = format!(
        "{}__{}",
        identity.template.catalog, identity.template.template
    );
    match &identity.kind {
        BindingKind::Parameter(name) => format!("{prefix}__{name}"),
        BindingKind::Helper(name) => format!("{prefix}__{name}"),
        BindingKind::Property {
            component,
            property,
        } => format!("{prefix}__{component}__{property}"),
    }
}

/// Assigns unique local names, disambiguating a collision deterministically
/// rather than silently overwriting a definition.
#[derive(Default)]
struct LocalNames {
    taken: BTreeSet<String>,
}

impl LocalNames {
    fn assign(&mut self, stem: &str) -> Name {
        let mut candidate = stem.to_owned();
        let mut suffix = 2u32;
        while !self.taken.insert(candidate.clone()) {
            candidate = format!("{stem}_{suffix}");
            suffix += 1;
        }
        Name::new(candidate).expect("local stems are built from validated name segments")
    }
}

fn qualified(scope: &Namespace, name: &Name) -> String {
    scope.qualified(name).to_string()
}

fn trailing_name(qualified: &str) -> Name {
    orishu_variables::FQName::parse(qualified)
        .expect("a qualified local name is well formed")
        .name()
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::Limits;
    use crate::load::parse_stream;
    use crate::name::{ComponentName, PluginId};
    use crate::resolve::resolve;
    use crate::schema::{ComponentSchema, PropertySchema};
    use std::path::Path;

    fn registry() -> SchemaRegistry {
        SchemaRegistry::new()
            .with(
                ComponentSchema::new(
                    ComponentTypeId::new(
                        PluginId::new("kagami.mass_sources").unwrap(),
                        ComponentName::new("inertial_mass").unwrap(),
                    ),
                    SchemaVersion(3),
                )
                .with_property(
                    PropertyName::new("mass").unwrap(),
                    PropertySchema::required(PropertyKind::Quantity {
                        dimension: Dimension::MASS,
                    }),
                )
                .with_property(
                    PropertyName::new("follows").unwrap(),
                    PropertySchema::optional(PropertyKind::Boolean),
                ),
            )
            .with(
                ComponentSchema::new(
                    ComponentTypeId::new(
                        PluginId::new("kagami.geometry").unwrap(),
                        ComponentName::new("sphere").unwrap(),
                    ),
                    SchemaVersion(1),
                )
                .with_property(
                    PropertyName::new("radius").unwrap(),
                    PropertySchema::required(PropertyKind::Quantity {
                        dimension: Dimension::LENGTH,
                    }),
                ),
            )
    }

    const CATALOG: &str = r#"apiVersion: kagami.catalog/v1
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
      follows: {boolean: true}
  - type: {plugin: kagami.geometry, name: sphere}
    properties:
      radius: {quantity: {expression: "6.9634e5", unit: km}}
---
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: twin}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "planets.sun.mass / 2"}
"#;

    fn catalog() -> CatalogSet {
        let stream = parse_stream(Path::new("planets.yaml"), CATALOG, &Limits::DEFAULT);
        resolve(stream.documents, Vec::new(), &registry(), &Limits::DEFAULT)
    }

    fn identity(name: &str) -> TemplateIdentity {
        TemplateIdentity::new("planets".try_into().unwrap(), name.try_into().unwrap())
    }

    fn request(name: &str) -> InstantiationRequest {
        InstantiationRequest::new(identity(name), Namespace::new("objects.obj_1"))
    }

    fn mass_of(candidate: &ObjectCandidate) -> f64 {
        let ObjectPropertyValue::Quantity { si_value, .. } =
            &candidate.components[0].properties[&PropertyName::new("mass").unwrap()]
        else {
            panic!("mass should be a quantity");
        };
        *si_value
    }

    #[test]
    fn materializing_resolves_every_property_to_canonical_si() {
        let candidate = materialize(&catalog(), &registry(), &request("sun")).unwrap();
        assert_eq!(mass_of(&candidate), 1.989e30);
        let ObjectPropertyValue::Quantity {
            si_value,
            dimension,
            ..
        } = &candidate.components[1].properties[&PropertyName::new("radius").unwrap()]
        else {
            panic!("radius should be a quantity");
        };
        assert!((si_value - 6.9634e8).abs() < 1.0);
        assert_eq!(*dimension, Dimension::LENGTH);
    }

    #[test]
    fn non_expression_properties_are_carried_across_unchanged() {
        let candidate = materialize(&catalog(), &registry(), &request("sun")).unwrap();
        assert_eq!(
            candidate.components[0].properties[&PropertyName::new("follows").unwrap()],
            ObjectPropertyValue::Boolean(true)
        );
    }

    #[test]
    fn the_schema_version_the_values_were_checked_against_is_recorded() {
        let candidate = materialize(&catalog(), &registry(), &request("sun")).unwrap();
        assert_eq!(candidate.components[0].schema_version, SchemaVersion(3));
        assert_eq!(candidate.components[1].schema_version, SchemaVersion(1));
    }

    #[test]
    fn provenance_records_the_source_but_is_not_a_link() {
        let set = catalog();
        let candidate = materialize(&set, &registry(), &request("sun")).unwrap();
        assert_eq!(candidate.provenance.identity, identity("sun"));
        assert_eq!(candidate.provenance.source.to_string(), "planets.yaml#1");
        assert_eq!(
            Some(candidate.provenance.fingerprint),
            set.get(&identity("sun")).unwrap().fingerprint
        );
    }

    #[test]
    fn every_definition_the_object_needs_is_copied_into_its_own_scope() {
        let candidate = materialize(&catalog(), &registry(), &request("sun")).unwrap();
        let names: Vec<String> = candidate
            .definitions
            .keys()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            names,
            vec!["planets__sun__scale", "planets__sun__solar_mass"]
        );
        for definition in candidate.definitions.values() {
            assert!(matches!(definition.origin, DefinitionOrigin::Catalog(_)));
        }
    }

    #[test]
    fn expressions_are_rewritten_onto_the_object_local_names() {
        let candidate = materialize(&catalog(), &registry(), &request("sun")).unwrap();
        let ObjectPropertyValue::Quantity { source, .. } =
            &candidate.components[0].properties[&PropertyName::new("mass").unwrap()]
        else {
            panic!("mass should be a quantity");
        };
        assert_eq!(
            source,
            "objects.obj_1.planets__sun__solar_mass * objects.obj_1.planets__sun__scale"
        );
    }

    #[test]
    fn a_candidate_resolves_with_no_catalog_in_scope() {
        let candidate = materialize(&catalog(), &registry(), &request("sun")).unwrap();
        let standalone = candidate.resolve_standalone().unwrap();
        assert_eq!(standalone["inertial_mass.mass"], 1.989e30);
    }

    #[test]
    fn a_cross_template_dependency_is_captured_transitively() {
        let candidate = materialize(&catalog(), &registry(), &request("twin")).unwrap();
        // `twin` reads `planets.sun.mass`, which itself reads the sun's
        // private helper and parameter: all three are copied.
        let names: Vec<String> = candidate
            .definitions
            .keys()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            names,
            vec![
                "planets__sun__inertial_mass__mass",
                "planets__sun__scale",
                "planets__sun__solar_mass",
            ]
        );
        assert_eq!(mass_of(&candidate), 1.989e30 / 2.0);
        assert_eq!(
            candidate.resolve_standalone().unwrap()["inertial_mass.mass"],
            1.989e30 / 2.0
        );
    }

    #[test]
    fn a_parameter_override_changes_the_materialised_value() {
        let mut request = request("sun");
        request
            .bindings
            .insert(ParameterName::new("scale").unwrap(), "0.5".to_owned());
        let candidate = materialize(&catalog(), &registry(), &request).unwrap();
        assert_eq!(mass_of(&candidate), 1.989e30 * 0.5);
    }

    #[test]
    fn a_binding_may_reference_a_supplied_document_variable() {
        let mut request = request("sun");
        request.bindings.insert(
            ParameterName::new("scale").unwrap(),
            "doc.factor * 2".to_owned(),
        );
        request
            .document_values
            .insert("doc.factor".to_owned(), 0.25);
        let candidate = materialize(&catalog(), &registry(), &request).unwrap();
        assert_eq!(mass_of(&candidate), 1.989e30 * 0.5);
        // The document variable is captured as a literal, not linked.
        let captured = candidate
            .definitions
            .values()
            .find(|definition| {
                definition.origin == DefinitionOrigin::Document("doc.factor".to_owned())
            })
            .expect("the document variable is captured");
        assert_eq!(captured.source, "0.25");
        assert_eq!(
            candidate.resolve_standalone().unwrap()["inertial_mass.mass"],
            1.989e30 * 0.5
        );
    }

    #[test]
    fn an_unresolvable_binding_rejects_the_whole_instantiation() {
        let mut request = request("sun");
        request.bindings.insert(
            ParameterName::new("scale").unwrap(),
            "doc.missing".to_owned(),
        );
        assert_eq!(
            materialize(&catalog(), &registry(), &request).unwrap_err(),
            InstantiationError::UnresolvedReference {
                reference: "doc.missing".to_owned()
            }
        );
    }

    #[test]
    fn a_binding_may_not_reach_another_templates_private_helper() {
        let mut request = request("twin");
        request.bindings.clear();
        // `twin` declares no parameters, so bind through `sun` instead.
        let mut request = InstantiationRequest::new(identity("sun"), Namespace::new("objects.o"));
        request.bindings.insert(
            ParameterName::new("scale").unwrap(),
            "planets.sun.solar_mass".to_owned(),
        );
        assert_eq!(
            materialize(&catalog(), &registry(), &request).unwrap_err(),
            InstantiationError::UnresolvedReference {
                reference: "planets.sun.solar_mass".to_owned()
            }
        );
    }

    #[test]
    fn binding_a_parameter_the_template_does_not_declare_is_rejected() {
        let mut request = request("sun");
        request
            .bindings
            .insert(ParameterName::new("nonsense").unwrap(), "1".to_owned());
        assert!(matches!(
            materialize(&catalog(), &registry(), &request).unwrap_err(),
            InstantiationError::UnknownParameter { .. }
        ));
    }

    #[test]
    fn a_malformed_binding_expression_is_rejected() {
        let mut request = request("sun");
        request
            .bindings
            .insert(ParameterName::new("scale").unwrap(), "1 +".to_owned());
        assert!(matches!(
            materialize(&catalog(), &registry(), &request).unwrap_err(),
            InstantiationError::InvalidBinding { .. }
        ));
    }

    #[test]
    fn a_stale_fingerprint_rejects_the_instantiation() {
        let mut request = request("sun");
        request.expected_fingerprint = Some(ContentFingerprint::of(b"something else"));
        assert!(matches!(
            materialize(&catalog(), &registry(), &request).unwrap_err(),
            InstantiationError::StaleFingerprint { .. }
        ));
    }

    #[test]
    fn an_unavailable_template_cannot_be_instantiated() {
        let stream = parse_stream(Path::new("planets.yaml"), CATALOG, &Limits::DEFAULT);
        let set = resolve(
            stream.documents,
            Vec::new(),
            &SchemaRegistry::new(),
            &Limits::DEFAULT,
        );
        assert_eq!(
            materialize(&set, &registry(), &request("sun")).unwrap_err(),
            InstantiationError::NotAvailable {
                identity: identity("sun"),
                state: "unavailable",
            }
        );
    }

    #[test]
    fn an_unknown_template_is_rejected() {
        assert_eq!(
            materialize(&catalog(), &registry(), &request("pluto")).unwrap_err(),
            InstantiationError::UnknownTemplate(identity("pluto"))
        );
    }

    #[test]
    fn a_later_catalog_edit_cannot_change_an_existing_candidate() {
        let before = materialize(&catalog(), &registry(), &request("sun")).unwrap();

        let edited = CATALOG.replace("1.989e30", "1.0");
        let stream = parse_stream(Path::new("planets.yaml"), &edited, &Limits::DEFAULT);
        let after_set = resolve(stream.documents, Vec::new(), &registry(), &Limits::DEFAULT);
        let after = materialize(&after_set, &registry(), &request("sun")).unwrap();

        assert_eq!(mass_of(&before), 1.989e30);
        assert_eq!(mass_of(&after), 1.0);
        // The already-materialised object is untouched by the edit and still
        // resolves on its own.
        assert_eq!(
            before.resolve_standalone().unwrap()["inertial_mass.mass"],
            1.989e30
        );
    }

    #[test]
    fn materialization_is_deterministic() {
        let set = catalog();
        assert_eq!(
            materialize(&set, &registry(), &request("twin")).unwrap(),
            materialize(&set, &registry(), &request("twin")).unwrap()
        );
    }
}
