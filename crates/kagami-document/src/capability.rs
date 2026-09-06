//! What the installed schemas can govern, which is not what the experiment
//! *is*.
//!
//! An experiment is durable authored content. Whether that content can be used
//! is a fact about the installation reading it, and the two must not be
//! confused: a document saved with a plugin installed is still a perfectly good
//! document after the plugin is removed, and treating it as corrupt would make
//! uninstalling a plugin destroy work.
//!
//! So this module answers a *separate* question from [`crate::validate`].
//! Validation decides whether a proposed edit may be adopted. A
//! [`CapabilityReport`] is a projection over already-adopted content, saying
//! which components this installation can currently interpret and precisely
//! why not for the rest. The two tiers are the ones `kagami-catalog` already
//! established for templates (ADR 0018), for the same reason.
//!
//! # What follows from a gap
//!
//! - **Reading and unrelated editing stay possible.** A component with a gap is
//!   preserved verbatim. Nothing substitutes a schema or invents a default
//!   value for it, and no edit elsewhere in the experiment revalidates it
//!   under guessed rules.
//! - **Editing the gap itself is refused.** Creating, attaching, or changing a
//!   property of content a schema cannot govern would be authoring against
//!   rules nobody has. Detaching it, and removing the object carrying it, stay
//!   possible: neither needs the schema to be correct, and an author who cannot
//!   interpret content must still be able to remove it.
//! - **Workload compilation is refused entirely.** Executing an experiment
//!   whose content this installation cannot interpret would be guessing about
//!   physics. [`CapabilityReport::is_complete`] is that gate.
//!
//! # Why a schema *version* change is not itself a gap
//!
//! Stored values are checked against the installed declaration directly —
//! every property, kind, dimension and requirement — rather than against the
//! version number they were originally accepted under. A component whose
//! values still satisfy the installed schema is usable however the version
//! moved, and one whose values do not reports the specific mismatch. Deciding
//! that some version *steps* are compatible regardless of shape needs the
//! simulation-plugin compatibility contract (X-PLUGIN), and inventing a policy
//! here would be a second answer to that question. The stored
//! [`ObjectComponent::schema_version`] remains recorded provenance either way.

use std::fmt;

use kagami_catalog::{ComponentTypeId, Dimension, PropertyKind, PropertyName, SchemaRegistry};
use thiserror::Error;

use crate::id::ObjectId;
use crate::model::ExperimentSnapshot;
use crate::object::{Object, ObjectComponent, PropertyValue};
use crate::validate::ComponentPath;

/// Why the installed schemas cannot govern one stored component.
///
/// Every variant is repairable without editing the experiment — by installing
/// or restoring a plugin — or names exactly what a future migration command
/// would have to change.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CapabilityGap {
    /// No installed simulation plugin contributes this component type.
    ///
    /// A fact about this installation alone: installing the plugin makes the
    /// component usable again with no edit to the experiment.
    #[error("no installed simulation plugin contributes this component type")]
    SchemaNotInstalled,
    /// The installed schema no longer declares a property the component holds.
    #[error("the installed schema does not declare property `{property}`")]
    PropertyNotDeclared {
        /// The stored property the schema has no declaration for.
        property: PropertyName,
    },
    /// A stored value is not of the kind the installed schema declares.
    #[error("property `{property}` is declared as a {expected}, but a {stored} is held")]
    PropertyKindChanged {
        /// The property.
        property: PropertyName,
        /// The kind the installed schema declares.
        expected: &'static str,
        /// The kind the stored value is.
        stored: &'static str,
    },
    /// A stored quantity does not measure the dimension the installed schema
    /// declares.
    #[error("property `{property}` is declared in {expected}, but the stored value is in {stored}")]
    PropertyDimensionChanged {
        /// The property.
        property: PropertyName,
        /// The dimension the installed schema declares.
        expected: Dimension,
        /// The dimension the stored value carries.
        stored: Dimension,
    },
    /// The installed schema requires a property the component does not author.
    #[error("the installed schema requires property `{property}`, which is not authored")]
    RequiredPropertyMissing {
        /// The property the schema requires.
        property: PropertyName,
    },
}

impl CapabilityGap {
    /// A stable identifier for this reason, independent of wording.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SchemaNotInstalled => "schema_not_installed",
            Self::PropertyNotDeclared { .. } => "property_not_declared",
            Self::PropertyKindChanged { .. } => "property_kind_changed",
            Self::PropertyDimensionChanged { .. } => "property_dimension_changed",
            Self::RequiredPropertyMissing { .. } => "required_property_missing",
        }
    }

    /// `true` when the contributing plugin is simply absent, rather than
    /// present and disagreeing with what is stored.
    ///
    /// The distinction decides what a restore may do: reinstating content
    /// under a schema that *is* installed and rejects it would assert values
    /// that declaration refuses, while reinstating content whose plugin is
    /// merely absent is no different from having loaded it.
    pub const fn is_absent(&self) -> bool {
        matches!(self, Self::SchemaNotInstalled)
    }
}

/// One component this installation cannot govern, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDiagnostic {
    /// Which component of which object.
    pub path: ComponentPath,
    /// Why it cannot be used here.
    pub gap: CapabilityGap,
}

impl fmt::Display for CapabilityDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.gap)
    }
}

/// How many components are in each capability state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySummary {
    /// Objects holding at least one component with a gap.
    pub objects: usize,
    /// Components whose contributing plugin is not installed.
    pub absent: usize,
    /// Components an installed schema refuses.
    pub incompatible: usize,
}

/// Which of an experiment's components the installed schemas can govern.
///
/// Ordered by object and then component type, so a listing, an event payload
/// and a diff over two reports are all deterministic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilityReport {
    diagnostics: Vec<CapabilityDiagnostic>,
}

impl CapabilityReport {
    /// Project an experiment's contents against the installed schemas.
    ///
    /// Costs one pass over every attached component. Callers that hold a
    /// report across edits should keep it rather than rebuild it — see
    /// [`Self::retain_present`].
    pub fn of(snapshot: &ExperimentSnapshot, schemas: &SchemaRegistry) -> Self {
        let mut diagnostics = Vec::new();
        for (id, object) in snapshot.objects() {
            for (type_id, component) in &object.components {
                if let Some(gap) = gap_of(type_id, component, schemas) {
                    diagnostics.push(CapabilityDiagnostic {
                        path: ComponentPath {
                            object: *id,
                            component: type_id.clone(),
                        },
                        gap,
                    });
                }
            }
        }
        Self { diagnostics }
    }

    /// Every gap, in object and component-type order.
    pub fn diagnostics(&self) -> &[CapabilityDiagnostic] {
        &self.diagnostics
    }

    /// The gap for one component, if it has one.
    pub fn gap(&self, object: ObjectId, component: &ComponentTypeId) -> Option<&CapabilityGap> {
        self.diagnostics
            .binary_search_by(|diagnostic| {
                (diagnostic.path.object, &diagnostic.path.component).cmp(&(object, component))
            })
            .ok()
            .map(|index| &self.diagnostics[index].gap)
    }

    /// Every gap on one object.
    pub fn object(&self, object: ObjectId) -> impl Iterator<Item = &CapabilityDiagnostic> {
        self.diagnostics
            .iter()
            .filter(move |diagnostic| diagnostic.path.object == object)
    }

    /// `true` when every attached component is governed by an installed
    /// schema — the condition workload compilation requires.
    pub fn is_complete(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Counts by state, for a status line or a change event.
    pub fn summary(&self) -> CapabilitySummary {
        let mut summary = CapabilitySummary::default();
        let mut last_object = None;
        for diagnostic in &self.diagnostics {
            if last_object != Some(diagnostic.path.object) {
                summary.objects += 1;
                last_object = Some(diagnostic.path.object);
            }
            if diagnostic.gap.is_absent() {
                summary.absent += 1;
            } else {
                summary.incompatible += 1;
            }
        }
        summary
    }

    /// Drop diagnostics for content `snapshot` no longer holds.
    ///
    /// This is the whole maintenance an accepted edit needs. An edit cannot
    /// *introduce* a gap — [`crate::validate`] refuses a batch that touches
    /// content an installed schema does not govern, and anything it attaches
    /// was checked against that schema on the way in — so the only way an
    /// edit changes this projection is by removing content. Adopting a
    /// different schema registry, or restoring captured contents, does need a
    /// rebuild through [`Self::of`].
    pub fn retain_present(&mut self, snapshot: &ExperimentSnapshot) {
        self.diagnostics.retain(|diagnostic| {
            snapshot
                .object(diagnostic.path.object)
                .is_some_and(|object| object.components.contains_key(&diagnostic.path.component))
        });
    }
}

/// How an object takes part in a run under the installed schemas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Participation {
    /// Carries no components: drawn, never simulated.
    ///
    /// Valid rather than incomplete. It is the state every composed object
    /// passes through, and a scene may legitimately hold markers.
    Inert,
    /// Carries at least one component an installed schema governs.
    ///
    /// Note what this does *not* say: nothing here is about Dynamics. A static
    /// field source or a particle emitter takes part in a run without opting
    /// into integration, and an object with Dynamics and nothing else is
    /// integrated with no force acting on it (ADR 0020).
    Participating,
    /// Carries components, but this installation can govern none of them.
    ///
    /// The content is preserved and the object is still editable elsewhere;
    /// what it *does* in a run cannot be answered until the contributing
    /// plugins are installed.
    Unavailable,
}

impl Object {
    /// How this object takes part in a run under `schemas`.
    ///
    /// Deliberately a question asked *of the installation* rather than a field
    /// on the object: the same authored object participates or does not
    /// depending on which plugins are installed, so storing the answer would
    /// be storing something that can silently go stale.
    ///
    /// This does not distinguish a physical component from a probe or a
    /// camera; no schema declares that yet, and the simulation-plugin contract
    /// (X-PLUGIN) owns adding it.
    pub fn participation(&self, schemas: &SchemaRegistry) -> Participation {
        if self.components.is_empty() {
            return Participation::Inert;
        }
        if self
            .components
            .iter()
            .any(|(type_id, component)| gap_of(type_id, component, schemas).is_none())
        {
            Participation::Participating
        } else {
            Participation::Unavailable
        }
    }
}

/// Check one stored component against the installed declaration for its type.
///
/// The single definition of "this installation can govern that component". The
/// candidate validator and this projection both call it, so an edit can never
/// be accepted against rules the projection would report a gap for.
pub(crate) fn gap_of(
    type_id: &ComponentTypeId,
    component: &ObjectComponent,
    schemas: &SchemaRegistry,
) -> Option<CapabilityGap> {
    let Some(schema) = schemas.get(type_id) else {
        return Some(CapabilityGap::SchemaNotInstalled);
    };

    for (name, value) in &component.properties {
        let Some(declared) = schema.properties.get(name) else {
            return Some(CapabilityGap::PropertyNotDeclared {
                property: name.clone(),
            });
        };
        match (&declared.kind, value) {
            (
                PropertyKind::Quantity { dimension },
                PropertyValue::Quantity {
                    dimension: held, ..
                },
            ) => {
                if dimension != held {
                    return Some(CapabilityGap::PropertyDimensionChanged {
                        property: name.clone(),
                        expected: *dimension,
                        stored: *held,
                    });
                }
            }
            (PropertyKind::Boolean, PropertyValue::Boolean(_))
            | (PropertyKind::Text, PropertyValue::Text(_)) => {}
            (expected, stored) => {
                return Some(CapabilityGap::PropertyKindChanged {
                    property: name.clone(),
                    expected: expected.label(),
                    stored: stored.kind_label(),
                });
            }
        }
    }

    for required in schema.required_properties() {
        if !component.properties.contains_key(required) {
            return Some(CapabilityGap::RequiredPropertyMissing {
                property: required.clone(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kagami_catalog::{ComponentName, ComponentSchema, PluginId, PropertySchema, SchemaVersion};

    use super::*;
    use crate::name::DisplayName;
    use crate::{Experiment, ExperimentCommand, Limits, ObjectSpec, update};

    fn component_type(plugin: &str, name: &str) -> ComponentTypeId {
        ComponentTypeId::new(
            PluginId::new(plugin).expect("valid identifier"),
            ComponentName::new(name).expect("valid identifier"),
        )
    }

    fn mass() -> ComponentTypeId {
        component_type("kagami.mass_sources", "inertial_mass")
    }

    fn property(name: &str) -> PropertyName {
        PropertyName::new(name).expect("valid identifier")
    }

    fn schemas() -> SchemaRegistry {
        SchemaRegistry::new().with(
            ComponentSchema::new(mass(), SchemaVersion(1)).with_property(
                property("mass"),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            ),
        )
    }

    /// An experiment holding one object with one mass component.
    fn massive() -> Experiment {
        let mut properties = BTreeMap::new();
        properties.insert(property("mass"), crate::AuthoredValue::si("1"));
        let spec = ObjectSpec::new(DisplayName::new("Earth").expect("valid label"))
            .with_component(mass(), properties);
        let (experiment, _) = update(
            &Experiment::new(),
            &[ExperimentCommand::CreateObject(Box::new(spec))],
            &schemas(),
            &Limits::DEFAULT,
        )
        .expect("accepted")
        .adopt();
        experiment
    }

    #[test]
    fn an_experiment_every_schema_governs_reports_nothing() {
        let report = CapabilityReport::of(&massive().snapshot(), &schemas());
        assert!(report.is_complete());
        assert_eq!(report.summary(), CapabilitySummary::default());
    }

    #[test]
    fn an_uninstalled_plugin_leaves_an_absent_gap_naming_the_component() {
        let experiment = massive();
        let snapshot = experiment.snapshot();
        let report = CapabilityReport::of(&snapshot, &SchemaRegistry::new());

        assert!(!report.is_complete());
        let id = *snapshot.objects().keys().next().expect("one object");
        assert_eq!(
            report.gap(id, &mass()),
            Some(&CapabilityGap::SchemaNotInstalled)
        );
        assert!(report.gap(id, &mass()).expect("a gap").is_absent());
        // A diagnostic names one addressable thing and why it cannot be used.
        assert_eq!(
            report.diagnostics()[0].to_string(),
            "object-0/kagami.mass_sources/inertial_mass: no installed simulation plugin \
             contributes this component type"
        );
        assert_eq!(
            report.summary(),
            CapabilitySummary {
                objects: 1,
                absent: 1,
                incompatible: 0,
            }
        );
        assert_eq!(report.object(id).count(), 1);
    }

    #[test]
    fn a_reshaped_schema_leaves_an_incompatible_gap_rather_than_an_absent_one() {
        let experiment = massive();
        let snapshot = experiment.snapshot();
        // The plugin is installed, but now declares the property as a length.
        let reshaped = SchemaRegistry::new().with(
            ComponentSchema::new(mass(), SchemaVersion(2)).with_property(
                property("mass"),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::LENGTH,
                }),
            ),
        );
        let report = CapabilityReport::of(&snapshot, &reshaped);
        let id = *snapshot.objects().keys().next().expect("one object");
        let gap = report.gap(id, &mass()).expect("the dimension moved");
        assert_eq!(gap.code(), "property_dimension_changed");
        assert!(!gap.is_absent());
        assert_eq!(report.summary().incompatible, 1);
    }

    #[test]
    fn a_version_bump_alone_is_not_a_gap() {
        // The declaration moved from v1 to v7 without changing what it asks
        // for. Reporting that as unusable would make every plugin release
        // break every document, and deciding which *steps* are compatible
        // regardless of shape belongs to the plugin contract.
        let experiment = massive();
        let bumped = SchemaRegistry::new().with(
            ComponentSchema::new(mass(), SchemaVersion(7)).with_property(
                property("mass"),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            ),
        );
        assert!(CapabilityReport::of(&experiment.snapshot(), &bumped).is_complete());
    }

    #[test]
    fn a_newly_required_property_is_reported_rather_than_defaulted() {
        let experiment = massive();
        let demanding = SchemaRegistry::new().with(
            ComponentSchema::new(mass(), SchemaVersion(2))
                .with_property(
                    property("mass"),
                    PropertySchema::required(PropertyKind::Quantity {
                        dimension: Dimension::MASS,
                    }),
                )
                .with_property(
                    property("rest_frame"),
                    PropertySchema::required(PropertyKind::Text),
                ),
        );
        let snapshot = experiment.snapshot();
        let report = CapabilityReport::of(&snapshot, &demanding);
        let id = *snapshot.objects().keys().next().expect("one object");
        assert_eq!(
            report.gap(id, &mass()),
            Some(&CapabilityGap::RequiredPropertyMissing {
                property: property("rest_frame"),
            })
        );
    }

    #[test]
    fn participation_does_not_depend_on_dynamics() {
        let experiment = massive();
        let snapshot = experiment.snapshot();
        let object = snapshot.objects().values().next().expect("one object");

        // A mass source and nothing else: it takes part in a run.
        assert_eq!(
            object.participation(&schemas()),
            Participation::Participating
        );
        // The same object, with its plugin uninstalled.
        assert_eq!(
            object.participation(&SchemaRegistry::new()),
            Participation::Unavailable
        );
    }

    #[test]
    fn an_object_with_no_components_is_inert_under_any_installation() {
        let (experiment, report) = update(
            &Experiment::new(),
            &[ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
                DisplayName::new("Marker").expect("valid label"),
            )))],
            &SchemaRegistry::new(),
            &Limits::DEFAULT,
        )
        .expect("accepted")
        .adopt();
        let snapshot = experiment.snapshot();
        let object = snapshot
            .object(report.first_created().expect("one object"))
            .expect("exists");
        assert_eq!(object.participation(&schemas()), Participation::Inert);
        assert_eq!(
            object.participation(&SchemaRegistry::new()),
            Participation::Inert
        );
    }

    #[test]
    fn removing_content_prunes_its_diagnostics_without_a_rebuild() {
        let experiment = massive();
        let bare = SchemaRegistry::new();
        let mut report = CapabilityReport::of(&experiment.snapshot(), &bare);
        assert_eq!(report.summary().absent, 1);

        let id = *experiment
            .snapshot()
            .objects()
            .keys()
            .next()
            .expect("one object");
        let (experiment, _) = update(
            &experiment,
            &[ExperimentCommand::RemoveObject(id)],
            &bare,
            &Limits::DEFAULT,
        )
        .expect("removing unavailable content needs no schema")
        .adopt();

        report.retain_present(&experiment.snapshot());
        assert!(report.is_complete());
        assert_eq!(
            report,
            CapabilityReport::of(&experiment.snapshot(), &bare),
            "pruning must agree with a full rebuild"
        );
    }
}
