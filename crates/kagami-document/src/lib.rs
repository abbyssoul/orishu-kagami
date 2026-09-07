//! Kagami's authoritative experiment model.
//!
//! This crate answers three questions and nothing else: what an experiment
//! *is*, what may change one, and whether a proposed change is valid. It is
//! the functional core of Kagami's authoring side — the imperative shell
//! around it (a window, an MCP transport, a file, a run) lives elsewhere, and
//! nothing here can reach it.
//!
//! ```text
//! apps/kagami        UI, MCP adapter, dialogs      <- imperative shell
//!      |  command envelope        ^ read projection
//! kagami-session     revisions, guards, events     <- the document server
//!      |  update(model, commands) ^ Rejection
//! kagami-document    this crate                    <- pure, sans-IO
//! ```
//!
//! # What this crate owns
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`id`] | stable identities, and the counters that mint them |
//! | [`geometry`] | placement value types whose invariants are their constructors |
//! | [`name`] | human labels, which are not identities |
//! | [`object`] | objects as entities composed from plugin-contributed components |
//! | [`setup`] | the numerical domain, time step, and plugin composition |
//! | [`variable`] | named values the experiment's expressions may use |
//! | [`command`] | the closed set of authoring intents |
//! | [`mod@update`] | the pure transition, and the candidate it produces |
//! | [`validate`] | the decision, and precisely why not |
//! | [`capability`] | what the installed schemas can govern, which the experiment is not |
//! | [`mod@hydrate`] | rebuilding an experiment from what a document persisted |
//! | [`history`] | undo and redo, as captured experiments |
//! | [`limits`] | declared bounds on what one batch may ask for |
//! | [`wire`] | the explicit, versioned shape an adapter converts through |
//!
//! It owns none of: revisions as a service, command envelopes, event logs,
//! persistence, transports, presentation state, or any part of a run.
//!
//! # The shape of an edit
//!
//! ```
//! # use kagami_catalog::{ComponentName, ComponentSchema, ComponentTypeId, Dimension,
//! #     PluginId, PropertyKind, PropertyName, PropertySchema, SchemaRegistry, SchemaVersion};
//! # use kagami_document::{AuthoredValue, DisplayName, Experiment, ExperimentCommand,
//! #     Limits, ObjectSpec, update};
//! # use std::collections::BTreeMap;
//! // Simulation plugins declare what may be attached; this crate only reads it.
//! let mass = ComponentTypeId::new(
//!     PluginId::new("kagami.mass_sources")?,
//!     ComponentName::new("inertial_mass")?,
//! );
//! let schemas = SchemaRegistry::new().with(
//!     ComponentSchema::new(mass.clone(), SchemaVersion(1)).with_property(
//!         PropertyName::new("mass")?,
//!         PropertySchema::required(PropertyKind::Quantity { dimension: Dimension::MASS }),
//!     ),
//! );
//!
//! // One authoring action creates an object; components are attached to it.
//! let mut properties = BTreeMap::new();
//! properties.insert(PropertyName::new("mass")?, AuthoredValue::si("1.989e30 / 2"));
//! let spec = ObjectSpec::new(DisplayName::new("Half a sun")?)
//!     .with_component(mass.clone(), properties);
//!
//! let experiment = Experiment::new();
//! let candidate = update(
//!     &experiment,
//!     &[ExperimentCommand::CreateObject(Box::new(spec))],
//!     &schemas,
//!     &Limits::DEFAULT,
//! )?;
//!
//! // Nothing was adopted: the caller decides, which is what lets the document
//! // server guard a submission without a second transition existing.
//! assert_eq!(experiment.snapshot().object_count(), 0);
//! let (experiment, report) = candidate.adopt();
//! assert_eq!(report.label, "Add object");
//!
//! // The authored source is the intent; the magnitude is derived from it.
//! let id = report.first_created().expect("one object");
//! let snapshot = experiment.snapshot();
//! let value = &snapshot.object(id).expect("created")
//!     .component(&mass).expect("attached")
//!     .properties[&PropertyName::new("mass")?];
//! assert_eq!(value.source(), Some("1.989e30 / 2"));
//! assert_eq!(value.si_value(), Some(9.945e29));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Units, and what is still deferred
//!
//! An authored quantity may write its unit *inside* the expression, and
//! `orishu-variables` derives the dimension from the arithmetic: `2.7 g` is a
//! mass, `2.7 m` assigned to a mass property is refused, and `1 kg + 1 m` is
//! not a value at all. A unit may still be named beside the expression for a
//! bare magnitude, which is how `mass: 5.972e24` means kilograms; declaring it
//! both ways is refused rather than applied twice.
//!
//! One limitation remains, and it is K2's to close: an expression with *no*
//! units of its own is read as canonical SI in whatever dimension the schema
//! declares, so a dimensionless ratio assigned to a mass is still accepted.
//! Until document variables exist there is nothing for such an expression to
//! reference, so it can only be a literal — a symbol reference is an explicit
//! `expression_unresolved` rejection, never a silent zero.

#![deny(missing_docs)]

pub mod capability;
pub mod command;
pub mod geometry;
pub mod history;
pub mod hydrate;
pub mod id;
pub mod limits;
pub mod model;
pub mod name;
pub mod object;
pub mod setup;
pub mod update;
pub mod validate;
pub mod variable;
pub mod wire;

pub use capability::{
    CapabilityDiagnostic, CapabilityGap, CapabilityReport, CapabilitySummary, Participation,
};
pub use command::ExperimentCommand;
pub use geometry::{GeometryError, ObjectShape, Rotation, Transform, Vector3, Velocity};
pub use history::{EditHistory, GestureId, Restoration};
pub use hydrate::{ComponentRecord, DocumentRecord, ObjectRecord, VariableRecord, hydrate};
pub use id::{Counters, ExperimentRevision, ObjectId};
pub use limits::Limits;
pub use model::{Experiment, ExperimentCheckpoint, ExperimentSnapshot};
pub use name::{DisplayName, MAX_DISPLAY_NAME_BYTES, NameError};
pub use object::{
    AuthoredValue, ComponentProperties, Object, ObjectComponent, ObjectSpec, PropertyValue,
};
pub use setup::{BoundaryCondition, Domain, PluginComposition, Setup, SetupError, TimeStep};
pub use update::{Candidate, CommitReport, EvaluationWork, resolve_variables, restore, update};
pub use validate::{ComponentPath, PropertyPath, Rejection};
pub use variable::{Variable, VariableId, VariableSpec};
pub use wire::{WIRE_VERSION, WireCommand, WireError, WireSnapshot};
