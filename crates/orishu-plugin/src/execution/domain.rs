//! Versioned three-dimensional domain/discretization input. Physical field
//! boundary conditions belong to the selected model's declared configuration.
use crate::{
    Error, ErrorCode, FiniteF64, Limits, codec,
    projection::{Project, object},
};
use orishu_resource::ApiVersion;
use orishu_workload::canonical::CanonicalValue as V;
use serde::{Deserialize, Serialize};

/// Portable domain descriptor schema, one domain per grant.
pub const DOMAIN_SCHEMA: &str = "orishu.simulation.domain/v1";
/// Hard byte bound for this small fixed-rank descriptor.
pub const MAX_DOMAIN_BYTES: usize = 4096;
/// Declared spatial discretization, not a runtime partition or opaque buffer layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SpatialDiscretization {
    /// Analytic/continuous spatial model; no artificial field grid is implied.
    Continuous,
    /// Cartesian cell counts in x/y/z order, with cell edges at the box corners.
    CartesianCells {
        /// Positive counts; total and representability are validated.
        cells: [u32; 3],
    },
}
/// Raw SI Cartesian domain descriptor. Origin is explicit through lower/upper
/// corners; units and orientation are fixed by this version's world frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainDescriptor {
    /// Exactly `orishu.simulation.domain/v1`.
    pub api_version: ApiVersion,
    /// Lower world-frame corner in metres, in x/y/z order.
    pub lower_metres: [FiniteF64; 3],
    /// Upper world-frame corner in metres, in x/y/z order.
    pub upper_metres: [FiniteF64; 3],
    /// Model's explicit spatial scheme, not the renderer's sampling density.
    pub discretization: SpatialDiscretization,
}
/// Host/authoring admission policy for declared grids; not a request to allocate.
#[derive(Clone, Copy, Debug)]
pub struct DomainLimits {
    /// Maximum checked product of Cartesian cell counts. Continuous needs no cells.
    pub cells: u64,
}
impl Default for DomainLimits {
    fn default() -> Self {
        Self { cells: 16_777_216 }
    }
}
fn limits() -> Limits {
    Limits {
        max_payload_bytes: MAX_DOMAIN_BYTES,
        max_values: 128,
        max_depth: 8,
        max_schema_items: 3,
        max_text_bytes: 128,
        ..Limits::default()
    }
}
impl DomainDescriptor {
    /// Check finite representable positive extents and, for a grid, cell product
    /// and distinguishable cell spacing before any downstream grid allocation.
    pub fn validate(&self, limits: DomainLimits) -> Result<(), Error> {
        if self.api_version.as_str() != DOMAIN_SCHEMA {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "apiVersion",
                "unsupported domain version",
            ));
        }
        let mut extents = [0.0; 3];
        for (i, length) in extents.iter_mut().enumerate() {
            *length = self.upper_metres[i].get() - self.lower_metres[i].get();
            if !length.is_finite() || *length <= 0.0 {
                return Err(Error::malformed(
                    "bounds",
                    "domain extents must be positive finite and representable",
                ));
            }
        }
        if let SpatialDiscretization::CartesianCells { cells } = self.discretization {
            let mut count = 1u64;
            for (i, n) in cells.into_iter().enumerate() {
                if n == 0 {
                    return Err(Error::malformed("cells", "domain grid axis is empty"));
                }
                count = count
                    .checked_mul(u64::from(n))
                    .filter(|v| *v <= limits.cells)
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::LimitExceeded,
                            "cells",
                            "domain cell budget exceeded",
                        )
                    })?;
                let step = extents[i] / f64::from(n);
                if step <= 0.0
                    || self.lower_metres[i].get() + step <= self.lower_metres[i].get()
                    || self.upper_metres[i].get() - step >= self.upper_metres[i].get()
                {
                    return Err(Error::malformed(
                        "cells",
                        "domain cell spacing is not representable",
                    ));
                }
            }
        }
        Ok(())
    }
    /// Canonical cold descriptor bytes, without a guessed numerical boundary policy.
    pub fn to_cbor(&self, bounds: DomainLimits) -> Result<Vec<u8>, Error> {
        self.validate(bounds)?;
        codec::encode(&self.project(), &limits(), MAX_DOMAIN_BYTES)
    }
    /// Bounded canonical reader; unsupported geometry/discretization is refused.
    pub fn from_cbor(bytes: &[u8], bounds: DomainLimits) -> Result<Self, Error> {
        let value: Self = codec::structured_from_cbor(bytes, &limits(), MAX_DOMAIN_BYTES)?;
        if value.to_cbor(bounds)? != bytes {
            return Err(Error::malformed("domain", "noncanonical domain descriptor"));
        }
        Ok(value)
    }
}
impl Project for SpatialDiscretization {
    fn project(&self) -> V {
        match self {
            Self::Continuous => object(vec![("kind", Some(V::text("continuous")))]),
            Self::CartesianCells { cells } => object(vec![
                ("kind", Some(V::text("cartesian-cells"))),
                (
                    "cells",
                    Some(V::Array(cells.iter().map(Project::project).collect())),
                ),
            ]),
        }
    }
}
impl Project for DomainDescriptor {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            (
                "lowerMetres",
                Some(V::Array(
                    self.lower_metres.iter().map(Project::project).collect(),
                )),
            ),
            (
                "upperMetres",
                Some(V::Array(
                    self.upper_metres.iter().map(Project::project).collect(),
                )),
            ),
            ("discretization", Some(self.discretization.project())),
        ])
    }
}
