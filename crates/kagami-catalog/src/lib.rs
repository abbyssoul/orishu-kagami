//! Kagami's editable object-template catalog.
//!
//! A catalog is a collection of human-readable files describing reusable
//! entity templates: a named composition of plugin-contributed components and
//! their authored properties. Instantiating one materialises a complete,
//! self-contained object in the current experiment. A template is data and
//! never selects executable physics — no template name, catalog name, or file
//! name reaches a solver.
//!
//! # What this crate owns
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`document`] | the hand-authored YAML format |
//! | [`mod@name`] | catalog identifier new-types |
//! | [`template`] | structural validation into typed template content |
//! | [`binding`] | the projection of template values into the shared variable environment |
//! | [`mod@resolve`] | the pure decision core: available, unavailable, or invalid |
//! | [`mod@load`] | the filesystem shell that feeds that core |
//! | [`mod@write`] | atomic, conflict-checked, sibling-preserving file replacement |
//! | [`mod@materialize`] | turning a template into a self-contained object candidate |
//! | [`authority`] | the one catalog authority UI and MCP adapters submit commands to |
//!
//! It owns none of: UI, MCP transport, a solver, Orishu runtime code, the
//! experiment document, or a template tracking link. The schema registry it
//! validates against is an owned value the caller supplies.
//!
//! # A minimal catalog
//!
//! ```yaml
//! apiVersion: kagami.catalog/v1
//! kind: ObjectTemplate
//! metadata:
//!   catalog: planets
//!   name: sun
//!   description: Sol
//! spec:
//!   parameters:
//!     scale: {default: "1", description: mass multiplier}
//!   helpers:
//!     solar_mass: {expression: "1.989e30", unit: kg}
//!   components:
//!   - type: {plugin: kagami.mass_sources, name: inertial_mass}
//!     properties:
//!       mass: {quantity: "planets.sun.solar_mass * planets.sun.scale"}
//! ```
//!
//! # Three load states
//!
//! Every entry is preserved whatever happens to it, because a catalog file is
//! a user's data and Kagami must never fail to start over it:
//!
//! - **Available** — usable now.
//! - **Unavailable** — a fact about *this installation*: a plugin is not
//!   installed, or a catalog it depends on is not loaded. Installing the
//!   plugin or copying the other catalog makes it available with no edit.
//! - **Invalid** — a defect *in the file*, identical on every machine.
//!
//! Nothing is ever repaired by substituting a fabricated scientific value.
//!
//! # Names are variable-name segments
//!
//! Every catalog, template, component, and property name is a validated
//! [`orishu_variables::Name`]. Every expression-capable property is published
//! as a binding at `catalog.template.component.property`, so a name that
//! could not be a variable segment could never be referenced. Parsing names
//! at the document boundary makes that unrepresentable rather than a failure
//! found later.
//!
//! # Units, and what is deferred
//!
//! Authored quantities declare a unit alongside their expression
//! (`{expression: "6.9634e5", unit: km}`) and are published as canonical SI.
//! The catalog checks that a declared unit measures the dimension the
//! property's schema asked for, that an unknown unit is an error rather than
//! a fabricated factor, and that a value resolves finitely.
//!
//! An expression may also carry its units directly — `2.7 g / cm^3` is a
//! density because `orishu-variables` derives it from the arithmetic. Saying
//! it both ways is refused
//! ([`InvalidReason::UnitDeclaredTwice`]) rather than scaled twice.
//!
//! Because bindings are published in canonical SI, an expression that
//! references another binding may only carry a canonical unit — otherwise the
//! declared factor would rescale a magnitude that is already canonical. That
//! is refused with
//! [`InvalidReason::NonCanonicalUnitInComputedExpression`] rather than
//! guessed at.

pub mod authority;
pub mod binding;
pub mod diagnostic;
pub mod document;
pub mod entry;
pub mod limits;
pub mod load;
pub mod materialize;
pub mod name;
pub mod resolve;
pub mod schema;
pub mod source;
pub mod template;
pub mod write;

pub use authority::{
    ActorId, CatalogAuthority, CatalogChange, CatalogCommand, CatalogCommandEnvelope, CatalogEvent,
    CatalogOutcome, CatalogQuery, CatalogRejection, CatalogRevision, CatalogView, CommandId,
    EntrySummary, ValidationReport,
};
pub use binding::{
    Binding, BindingIdentity, BindingKind, CatalogProjection, ProjectionError, RejectedBinding,
    Resolution,
};
pub use diagnostic::{Diagnostic, InvalidReason, UnavailableReason};
pub use document::{API_VERSION, KIND, TemplateDocument};
pub use entry::{CatalogEntry, CatalogFileError, CatalogSet, CatalogSummary, LoadResult};
pub use limits::Limits;
pub use load::{load_directory, parse_stream};
pub use materialize::{
    InstantiationError, InstantiationRequest, ObjectCandidate, ObjectComponent,
    ObjectPropertyValue, materialize,
};
pub use name::{
    CatalogName, ComponentName, ComponentTypeId, HelperName, NameError, ParameterName, PluginId,
    PropertyName, TemplateName,
};
/// The structural resource envelope a template document is an instance of.
///
/// Re-exported from `orishu-resource`, which owns it: Orishu's workload,
/// cluster, and node resources use the same `apiVersion`/`kind`/`metadata`/
/// `spec` shape, and maintaining a second definition of it here is what this
/// re-export replaced. Only the *shape* is shared — catalog metadata,
/// validation, canonical bytes, and write authority stay in this crate.
pub use orishu_resource::{ApiVersion, Kind, Resource, ResourceHeader, UnexpectedDiscriminator};
/// Physical dimensions, units, and the values evaluation produces.
///
/// Re-exported from `orishu-variables`, which owns them: dimensions are
/// derived by the shared expression engine, so the table that says what a
/// gram is has to be the same one the evaluator reads. A copy here would be a
/// second answer.
pub mod quantity {
    pub use orishu_variables::quantity::*;
}

pub use orishu_variables::{Dimension, Quantity, Unit, UnitError};
pub use resolve::{ParsedDocument, resolve};
pub use schema::{ComponentSchema, PropertyKind, PropertySchema, SchemaRegistry, SchemaVersion};
pub use source::{
    ContentFingerprint, DocumentOrdinal, SourceLocation, TemplateIdentity, TemplateProvenance,
};
pub use template::{Template, TemplateSpec, Visibility};
pub use write::{CatalogRoot, FileDigest, WriteError, WriteTarget};
