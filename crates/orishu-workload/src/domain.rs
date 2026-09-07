//! What physical region exists, and how it is sampled.
//!
//! # Deliberately minimal, and why
//!
//! This is the smallest domain description that makes a workload digest
//! meaningful: without it, two workloads simulating different regions with the
//! same components would share an identity.
//!
//! It is not yet the full domain. Three things are deferred, and each has a
//! reason rather than an omission:
//!
//! - **Anisotropic and segmented discretisation**, which
//!   `crates/orishu/src/model/workload.rs` prototypes, needs the same
//!   validation rules ported rather than reinvented.
//! - **Unit-typed quantities.** The prototype uses `uom`, which this crate must
//!   not acquire: `orishu-workload` sits below Kagami's authoring crates and
//!   everything it depends on, they depend on. Values here are canonical SI
//!   magnitudes — metres and seconds — which is what
//!   [`orishu_variables::Quantity`] resolves to anyway.
//! - **Expression-bearing fields.** `docs/protocol-client.md` specifies that
//!   expression-capable numeric fields retain authored source under a declared
//!   language version. That representation is owned by the variables
//!   integration; a manifest carries resolved magnitudes until it lands.
//!
//! [`orishu_variables::Quantity`]: https://docs.rs/orishu-variables

use serde::{Deserialize, Serialize};

use crate::ids::ParameterName;
use crate::limits::Limits;
use crate::value::{FiniteF64, ScalarValue};

/// The extent of the simulated region, in metres.
///
/// Tagged by a `shape` field rather than externally tagged, so one spelling
/// works in JSON and YAML alike and matches the canonical form exactly. An
/// externally tagged enum would be `{"cube": {...}}` in JSON but `!Cube {...}`
/// in YAML, which would make the two authoring codecs disagree about the same
/// workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "shape",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DomainBounds {
    /// A hypercube: every side the same length.
    Cube {
        /// Side length, in metres.
        side_metres: FiniteF64,
    },
    /// A box with one length per dimension.
    Box {
        /// Side lengths, in metres, one per spatial dimension.
        side_metres: Vec<FiniteF64>,
    },
}

/// How space and time are sampled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Discretization {
    /// Uniform spatial step, in metres.
    pub space_metres: FiniteF64,
    /// Committed-boundary time step, in seconds.
    pub time_seconds: FiniteF64,
    /// The selected temporal integration scheme, where the graph exposes more
    /// than one.
    ///
    /// Authoritative workload intent, not a worker-local preference that may be
    /// substituted at load time. Absent when the admitted graph supports
    /// exactly one scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<Integration>,
}

/// The selected temporal integration scheme and its configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Integration {
    /// The scheme identifier, drawn from the closed set the admitted component
    /// graph and step plan support.
    pub scheme: crate::ids::ParameterName,
    /// Scheme-specific stepping configuration.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub parameters: std::collections::BTreeMap<ParameterName, ScalarValue>,
}

/// The physical region a workload evolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainSpec {
    /// Spatial dimensionality.
    pub dimensions: u8,
    /// The region's extent.
    pub bounds: DomainBounds,
    /// How that region is sampled.
    pub discretization: Discretization,
}

/// Why a domain is not internally consistent.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// Zero spatial dimensions, or more than the reader accepts.
    #[error("a domain must have between 1 and {max} spatial dimensions, got {found}")]
    Dimensions {
        /// Dimensions declared.
        found: u8,
        /// Dimensions permitted.
        max: u8,
    },
    /// A box declares a different number of sides than the domain has
    /// dimensions.
    #[error("a {dimensions}-dimensional domain needs {dimensions} side lengths, got {found}")]
    BoundsRank {
        /// Dimensions declared.
        dimensions: u8,
        /// Side lengths supplied.
        found: usize,
    },
    /// A length or step is zero or negative.
    #[error("{field} must be positive, got {found}")]
    NotPositive {
        /// Which value.
        field: &'static str,
        /// The value supplied.
        found: FiniteF64,
    },
    /// A sampling step is larger than the region it samples.
    #[error("the {axis} step {step} exceeds the domain extent {extent}")]
    StepExceedsExtent {
        /// Which axis.
        axis: &'static str,
        /// The step supplied.
        step: FiniteF64,
        /// The extent it must fit inside.
        extent: FiniteF64,
    },
}

impl DomainSpec {
    /// Checks that the domain describes a region something could be simulated
    /// in.
    ///
    /// This is a structural and arithmetic check, not a numerical-stability
    /// one: it rejects a domain that cannot be sampled at all, and says nothing
    /// about whether the resulting scheme converges.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] for a zero or oversized dimensionality, a bounds
    /// rank that disagrees with it, a non-positive extent or step, or a step
    /// that does not fit inside its axis.
    pub fn validate(&self, limits: &Limits) -> Result<(), DomainError> {
        if self.dimensions == 0 || self.dimensions > limits.max_domain_dimensions {
            return Err(DomainError::Dimensions {
                found: self.dimensions,
                max: limits.max_domain_dimensions,
            });
        }

        let space = self.discretization.space_metres;
        if !space.is_positive() {
            return Err(DomainError::NotPositive {
                field: "the spatial step",
                found: space,
            });
        }
        if !self.discretization.time_seconds.is_positive() {
            return Err(DomainError::NotPositive {
                field: "the time step",
                found: self.discretization.time_seconds,
            });
        }

        let extents: Vec<FiniteF64> = match &self.bounds {
            DomainBounds::Cube { side_metres } => {
                vec![*side_metres; usize::from(self.dimensions)]
            }
            DomainBounds::Box { side_metres } => {
                if side_metres.len() != usize::from(self.dimensions) {
                    return Err(DomainError::BoundsRank {
                        dimensions: self.dimensions,
                        found: side_metres.len(),
                    });
                }
                side_metres.clone()
            }
        };

        for extent in extents {
            if !extent.is_positive() {
                return Err(DomainError::NotPositive {
                    field: "a domain side length",
                    found: extent,
                });
            }
            if space > extent {
                return Err(DomainError::StepExceedsExtent {
                    axis: "spatial",
                    step: space,
                    extent,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite(value: f64) -> FiniteF64 {
        FiniteF64::new(value).expect("a finite test value")
    }

    fn cube(dimensions: u8, side: f64, space: f64, time: f64) -> DomainSpec {
        DomainSpec {
            dimensions,
            bounds: DomainBounds::Cube {
                side_metres: finite(side),
            },
            discretization: Discretization {
                space_metres: finite(space),
                time_seconds: finite(time),
                integration: None,
            },
        }
    }

    #[test]
    fn a_well_formed_cube_domain_validates() {
        assert_eq!(
            cube(3, 1.0, 0.001, 1.5e-11).validate(&Limits::DEFAULT),
            Ok(())
        );
    }

    #[test]
    fn zero_dimensions_are_refused() {
        assert_eq!(
            cube(0, 1.0, 0.1, 1.0).validate(&Limits::DEFAULT),
            Err(DomainError::Dimensions { found: 0, max: 4 })
        );
    }

    #[test]
    fn more_dimensions_than_the_reader_accepts_are_refused() {
        assert_eq!(
            cube(9, 1.0, 0.1, 1.0).validate(&Limits::DEFAULT),
            Err(DomainError::Dimensions { found: 9, max: 4 })
        );
    }

    #[test]
    fn a_box_must_declare_one_side_per_dimension() {
        let domain = DomainSpec {
            dimensions: 3,
            bounds: DomainBounds::Box {
                side_metres: vec![finite(1.0), finite(0.2)],
            },
            discretization: Discretization {
                space_metres: finite(0.001),
                time_seconds: finite(1.0),
                integration: None,
            },
        };
        assert_eq!(
            domain.validate(&Limits::DEFAULT),
            Err(DomainError::BoundsRank {
                dimensions: 3,
                found: 2
            })
        );
    }

    #[test]
    fn a_step_larger_than_its_axis_is_refused() {
        assert!(matches!(
            cube(2, 0.5, 1.0, 1.0).validate(&Limits::DEFAULT),
            Err(DomainError::StepExceedsExtent {
                axis: "spatial",
                ..
            })
        ));
    }

    #[test]
    fn a_non_positive_step_is_refused() {
        assert!(matches!(
            cube(2, 1.0, 0.0, 1.0).validate(&Limits::DEFAULT),
            Err(DomainError::NotPositive { .. })
        ));
        assert!(matches!(
            cube(2, 1.0, 0.1, -1.0).validate(&Limits::DEFAULT),
            Err(DomainError::NotPositive { .. })
        ));
    }

    #[test]
    fn a_domain_refuses_an_unrecognised_field() {
        let json = r#"{
            "dimensions": 3,
            "bounds": {"shape": "cube", "sideMetres": 1.0},
            "discretization": {"spaceMetres": 0.001, "timeSeconds": 1.0},
            "resolution": 512
        }"#;
        assert!(serde_json::from_str::<DomainSpec>(json).is_err());
    }
}
