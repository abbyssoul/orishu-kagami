//! The numerical setup an experiment declares, and which plugins model it.
//!
//! Deliberately the smallest thing an experiment needs in order to be
//! *describable*. The authoritative discretization schema belongs to the
//! shared workload format, and per-plugin configuration belongs to the
//! simulation-plugin contract; neither exists yet, and inventing a placeholder
//! schema here would create a second definition that has to be reconciled
//! later. What is here is what an authoring UI must be able to show and edit
//! before either lands, and it is versioned with the document like everything
//! else.

use std::collections::BTreeSet;

use kagami_catalog::PluginId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::geometry::{GeometryError, Vector3};

/// Why a setup value could not be constructed.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SetupError {
    /// A coordinate was not finite.
    #[error(transparent)]
    Geometry(#[from] GeometryError),
    /// A grid axis had no cells.
    #[error("grid resolution must be at least one cell on every axis")]
    EmptyGrid,
    /// The time step was not a positive finite number of seconds.
    #[error("time step must be a positive finite number of seconds")]
    InvalidTimeStep,
}

/// How a field behaves at one face of the domain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryCondition {
    /// The axis wraps.
    #[default]
    Periodic,
    /// The value is fixed at the boundary.
    Dirichlet,
    /// The derivative is fixed at the boundary.
    Neumann,
    /// Outgoing waves leave without reflecting.
    Absorbing,
}

/// The axis-aligned region an experiment is computed over, and how finely.
///
/// Decoded through [`Domain::new`], so an inverted region or an empty grid
/// axis is refused at the boundary rather than becoming a domain no solver can
/// discretize.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "DomainRepr")]
pub struct Domain {
    lower: Vector3,
    upper: Vector3,
    cells: [u32; 3],
    boundary: [BoundaryCondition; 3],
}

/// The wire shape of a [`Domain`], before its constructor has seen it.
#[derive(Deserialize)]
struct DomainRepr {
    lower: Vector3,
    upper: Vector3,
    cells: [u32; 3],
    boundary: [BoundaryCondition; 3],
}

impl TryFrom<DomainRepr> for Domain {
    type Error = SetupError;

    fn try_from(value: DomainRepr) -> Result<Self, Self::Error> {
        Self::new(value.lower, value.upper, value.cells, value.boundary)
    }
}

impl Domain {
    /// Build a domain, refusing an empty or inverted region and a grid with an
    /// empty axis.
    pub fn new(
        lower: Vector3,
        upper: Vector3,
        cells: [u32; 3],
        boundary: [BoundaryCondition; 3],
    ) -> Result<Self, SetupError> {
        if upper.x() <= lower.x() || upper.y() <= lower.y() || upper.z() <= lower.z() {
            return Err(SetupError::Geometry(GeometryError::EmptyBounds));
        }
        if cells.contains(&0) {
            return Err(SetupError::EmptyGrid);
        }
        Ok(Self {
            lower,
            upper,
            cells,
            boundary,
        })
    }

    /// The lower corner, in metres.
    pub const fn lower(&self) -> Vector3 {
        self.lower
    }

    /// The upper corner, in metres.
    pub const fn upper(&self) -> Vector3 {
        self.upper
    }

    /// Cell counts, in x, y, z order.
    pub const fn cells(&self) -> [u32; 3] {
        self.cells
    }

    /// Boundary conditions, in x, y, z order.
    pub const fn boundary(&self) -> [BoundaryCondition; 3] {
        self.boundary
    }
}

impl Default for Domain {
    /// A one-metre periodic cube on a 32³ grid.
    ///
    /// A default exists because a new experiment must be openable and
    /// editable before anyone has chosen a discretization, and refusing to
    /// represent that state would push a `None` into every consumer. It is a
    /// starting point a user is expected to replace, not a scientific claim.
    fn default() -> Self {
        let half = Vector3::new(0.5, 0.5, 0.5).expect("finite constant");
        let lower = Vector3::new(-0.5, -0.5, -0.5).expect("finite constant");
        Self {
            lower,
            upper: half,
            cells: [32, 32, 32],
            boundary: [BoundaryCondition::Periodic; 3],
        }
    }
}

/// The fixed simulation time step, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct TimeStep(f64);

impl TimeStep {
    /// Build a time step, refusing a non-positive or non-finite value.
    pub fn new(seconds: f64) -> Result<Self, SetupError> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(SetupError::InvalidTimeStep);
        }
        Ok(Self(seconds))
    }

    /// The step, in seconds.
    pub const fn seconds(self) -> f64 {
        self.0
    }
}

impl Default for TimeStep {
    /// One millisecond: small enough to be a plausible starting point for the
    /// scales this product targets, and, like [`Domain::default`], expected to
    /// be replaced rather than relied upon.
    fn default() -> Self {
        Self(1.0e-3)
    }
}

impl TryFrom<f64> for TimeStep {
    type Error = SetupError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TimeStep> for f64 {
    fn from(value: TimeStep) -> Self {
        value.0
    }
}

/// Which simulation plugins this experiment models its phenomena with.
///
/// Only the enabled set, by identity. A plugin's own configuration is
/// declared by its schema and is the plugin contract's to define; a property
/// bag invented here would be a second, unversioned schema.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginComposition {
    enabled: BTreeSet<PluginId>,
}

impl PluginComposition {
    /// Nothing enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable one plugin. Returns `true` when the set changed, so
    /// a caller can tell a real edit from a no-op.
    pub fn set_enabled(&mut self, plugin: PluginId, enabled: bool) -> bool {
        if enabled {
            self.enabled.insert(plugin)
        } else {
            self.enabled.remove(&plugin)
        }
    }

    /// `true` when `plugin` is enabled.
    pub fn is_enabled(&self, plugin: &PluginId) -> bool {
        self.enabled.contains(plugin)
    }

    /// Every enabled plugin, in identity order.
    pub fn enabled(&self) -> impl Iterator<Item = &PluginId> {
        self.enabled.iter()
    }

    /// How many plugins are enabled.
    pub fn len(&self) -> usize {
        self.enabled.len()
    }

    /// `true` when no plugin is enabled.
    pub fn is_empty(&self) -> bool {
        self.enabled.is_empty()
    }
}

/// The experiment's numerical setup and plugin composition.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// The region and grid.
    pub domain: Domain,
    /// The fixed time step.
    pub time_step: TimeStep,
    /// The enabled simulation plugins.
    pub plugins: PluginComposition,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(x: f64, y: f64, z: f64) -> Vector3 {
        Vector3::new(x, y, z).expect("finite")
    }

    #[test]
    fn a_domain_needs_a_strictly_positive_extent_on_every_axis() {
        let lower = vector(0.0, 0.0, 0.0);
        assert!(
            Domain::new(
                lower,
                vector(1.0, 1.0, 1.0),
                [4, 4, 4],
                [BoundaryCondition::Periodic; 3]
            )
            .is_ok()
        );
        // Flat on z.
        assert!(
            Domain::new(
                lower,
                vector(1.0, 1.0, 0.0),
                [4, 4, 4],
                [BoundaryCondition::Periodic; 3]
            )
            .is_err()
        );
        // Inverted.
        assert!(
            Domain::new(
                vector(1.0, 1.0, 1.0),
                lower,
                [4, 4, 4],
                [BoundaryCondition::Periodic; 3]
            )
            .is_err()
        );
    }

    #[test]
    fn a_grid_axis_cannot_be_empty() {
        assert_eq!(
            Domain::new(
                vector(0.0, 0.0, 0.0),
                vector(1.0, 1.0, 1.0),
                [4, 0, 4],
                [BoundaryCondition::Periodic; 3]
            ),
            Err(SetupError::EmptyGrid)
        );
    }

    #[test]
    fn a_time_step_must_be_positive_and_finite() {
        assert!(TimeStep::new(1.0e-6).is_ok());
        assert_eq!(TimeStep::new(0.0), Err(SetupError::InvalidTimeStep));
        assert_eq!(TimeStep::new(-1.0), Err(SetupError::InvalidTimeStep));
        assert_eq!(TimeStep::new(f64::NAN), Err(SetupError::InvalidTimeStep));
        assert!(serde_json::from_str::<TimeStep>("0").is_err());
    }

    #[test]
    fn plugin_composition_reports_whether_a_change_happened() {
        let plugin = PluginId::new("kagami.gravity").expect("valid identifier");
        let mut composition = PluginComposition::new();
        assert!(composition.set_enabled(plugin.clone(), true));
        assert!(!composition.set_enabled(plugin.clone(), true));
        assert!(composition.is_enabled(&plugin));
        assert!(composition.set_enabled(plugin.clone(), false));
        assert!(!composition.set_enabled(plugin, false));
        assert!(composition.is_empty());
    }

    #[test]
    fn serde_revalidates_a_domain_at_the_document_boundary() {
        let domain = Domain::default();
        let encoded = serde_json::to_string(&domain).expect("encodes");
        assert_eq!(
            serde_json::from_str::<Domain>(&encoded).expect("decodes"),
            domain
        );

        // Decoding straight onto the fields would accept a region no solver
        // can discretize, whatever `Domain::new` says.
        let inverted = r#"{"lower":[1,1,1],"upper":[0,0,0],"cells":[4,4,4],
            "boundary":["periodic","periodic","periodic"]}"#;
        assert!(serde_json::from_str::<Domain>(inverted).is_err());
        let empty_axis = r#"{"lower":[0,0,0],"upper":[1,1,1],"cells":[4,0,4],
            "boundary":["periodic","periodic","periodic"]}"#;
        assert!(serde_json::from_str::<Domain>(empty_axis).is_err());
    }

    #[test]
    fn a_default_setup_round_trips_through_serde() {
        let setup = Setup::default();
        let encoded = serde_json::to_string(&setup).expect("encodes");
        assert_eq!(
            serde_json::from_str::<Setup>(&encoded).expect("decodes"),
            setup
        );
    }
}
