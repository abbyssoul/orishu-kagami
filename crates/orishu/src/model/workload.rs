use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_validate::{Validate, validate_deser};
use uom::si::f64::{Length, Time};
use uom::si::length::meter;
use uom::si::time::second;

use crate::model::checkpoint::CheckpointId;
use crate::model::manifest::{self, MANIFEST_API_VERSION, Manifest as DefManifest, ObjectMeta};
use crate::model::{QuerySet, quantity};

/// Workload manifest kind
pub const WORKLOAD_MANIFEST_KIND: &str = "Workload";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", untagged)]
pub enum ExternalResource {
    Image {
        /// If set, defines a URI to external resource
        uri: String,
    },
    Inline {
        data: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DomainType {
    Hydrodynamics,
    Electromagnetic,
    Gravity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    /// If set, defines where to get executable math model
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ExternalResource>,
}

/// Full set of conditions a worker must satisfy to run a workload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadRequirements {
    /// Minimum hardware capabilities.
    #[serde(default)]
    pub hardware: HardwareRequirements,
    /// Version of the host ABI expected by the simulation WASM package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_abi_version: Option<String>,
    /// Determinism contract: numeric mode, reduction order, halo exchange
    /// requirements, random seed policy, and step barrier mode.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub execution_profile: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitialConditionsSpec {
    /// If set, defines a where to obtain a resource describing initial configuration of the domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ExternalResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeometrySpec {
    /// If set, defines a where to obtain geometric configuration of the domain
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<ExternalResource>,
}

/// Full set of conditions a worker must satisfy to run a workload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadInputs {
    /// Geometry / Static boundaries that make-up the universe
    /// (mesh, particle distribution, sources and sinks, etc.).
    pub geometry: Option<GeometrySpec>,

    /// What fills the universe at t = 0
    /// Initial flows, fields configuration, etc.
    pub initial_conditions: Option<InitialConditionsSpec>,
}

/// A space interval of a given length divided in steps of equal size.
/// Invariant: step size is < length
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceSegment {
    #[serde(
        deserialize_with = "quantity::deserialize_length",
        serialize_with = "quantity::serialize_length"
    )]
    pub length: Length,

    #[serde(
        deserialize_with = "quantity::deserialize_length",
        serialize_with = "quantity::serialize_length"
    )]
    pub step: Length,
}

impl Validate for SpaceSegment {
    type Error = String;
    fn validate(&self) -> Result<(), Self::Error> {
        if self.length.is_sign_negative() {
            Err(format!(
                "space-segment length {} is negative",
                self.length
                    .into_format_args(meter, uom::fmt::DisplayStyle::Abbreviation)
            ))
        } else if self.step.is_sign_negative() {
            Err(format!(
                "space-segment step {} is negative",
                self.step
                    .into_format_args(meter, uom::fmt::DisplayStyle::Abbreviation),
            ))
        } else if self.length < self.step {
            Err(format!(
                "space-segment length {} is less than the step size {}",
                self.length
                    .into_format_args(meter, uom::fmt::DisplayStyle::Abbreviation),
                self.step
                    .into_format_args(meter, uom::fmt::DisplayStyle::Abbreviation),
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", untagged)]
pub enum AnisotropicDimension {
    Uniform(
        #[serde(
            deserialize_with = "quantity::deserialize_length",
            serialize_with = "quantity::serialize_length"
        )]
        Length,
    ),
    Segments(Vec<SpaceSegment>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", untagged)]
pub enum SpaceDiscretization {
    #[serde(
        deserialize_with = "quantity::deserialize_length",
        serialize_with = "quantity::serialize_length"
    )]
    Uniform(Length),

    Anisotropic(Vec<AnisotropicDimension>),
}

impl SpaceDiscretization {
    fn validate(&self, dim: u8, bounds: &DomainBoundsSpec) -> Result<(), String> {
        match self {
            SpaceDiscretization::Uniform(ds) => {
                if ds.value <= 0.0 {
                    return Err("uniform space-step must be positive".into());
                }
                match bounds {
                    DomainBoundsSpec::Cube(side) => {
                        if side < ds {
                            return Err("uniform space-step exceeds domain bounds".into());
                        }
                    }
                    DomainBoundsSpec::Hyperrectangle(sides) => {
                        for (i, side) in sides.iter().enumerate() {
                            if side < ds {
                                return Err(format!(
                                    "uniform space-step exceeds domain bounds for dimension {i}"
                                ));
                            }
                        }
                    }
                    DomainBoundsSpec::Mesh(_) => {}
                }
                Ok(())
            }
            SpaceDiscretization::Anisotropic(dimensions) => {
                let l = dimensions.len();

                if l < (dim as usize) {
                    return Err(format!(
                        "too few dimensions {l} / {dim} discretization configured"
                    ));
                } else if l > (dim as usize) {
                    return Err(format!(
                        "too many dimensions {l} / {dim} discretization configured"
                    ));
                }

                for (i, aniso_dim) in dimensions.iter().enumerate() {
                    let bound = match bounds {
                        DomainBoundsSpec::Cube(side) => Some(*side),
                        DomainBoundsSpec::Hyperrectangle(sides) => sides.get(i).copied(),
                        DomainBoundsSpec::Mesh(_) => None,
                    };
                    if let Some(bound) = bound {
                        match aniso_dim {
                            AnisotropicDimension::Uniform(step) => {
                                if step.value <= 0.0 {
                                    return Err(format!(
                                        "space-step for dimension {i} must be positive"
                                    ));
                                }

                                if &bound < step {
                                    return Err(format!(
                                        "space-step exceeds domain bounds for dimension {i}"
                                    ));
                                }
                            }
                            AnisotropicDimension::Segments(segs) => {
                                if segs.is_empty() {
                                    return Err(format!(
                                        "space-step has no segments for dimension {i}"
                                    ));
                                }
                                for seg in segs {
                                    seg.validate().map_err(|e| format!("dimension {i}: {e}"))?;
                                    if bound < seg.step {
                                        return Err(format!(
                                            "space-step exceeds domain bounds for dimension {i}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimeSegment {
    #[serde(
        deserialize_with = "quantity::deserialize_time",
        serialize_with = "quantity::serialize_time"
    )]
    pub duration: Time,

    #[serde(
        deserialize_with = "quantity::deserialize_time",
        serialize_with = "quantity::serialize_time"
    )]
    pub step: Time,
}

impl Validate for TimeSegment {
    type Error = String;
    fn validate(&self) -> Result<(), Self::Error> {
        if self.duration.value <= 0.0 {
            Err(format!(
                "time-segment duration {} is negative",
                self.duration
                    .into_format_args(second, uom::fmt::DisplayStyle::Abbreviation)
            ))
        } else if self.step.value <= 0.0 {
            Err(format!(
                "time-segment step {} is negative",
                self.step
                    .into_format_args(second, uom::fmt::DisplayStyle::Abbreviation),
            ))
        } else if self.duration < self.step {
            Err(format!(
                "time-segment duration {} is less than the step size {}",
                self.duration
                    .into_format_args(second, uom::fmt::DisplayStyle::Abbreviation),
                self.step
                    .into_format_args(second, uom::fmt::DisplayStyle::Abbreviation),
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", untagged)]
pub enum TimeDiscretization {
    Uniform(
        #[serde(
            deserialize_with = "quantity::deserialize_time",
            serialize_with = "quantity::serialize_time"
        )]
        Time,
    ),

    Anisotropic(Vec<TimeSegment>),
}

impl Validate for TimeDiscretization {
    type Error = String;
    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            TimeDiscretization::Uniform(dt) => {
                if dt.value <= 0.0 {
                    Err("uniform time-step must be positive".into())
                } else {
                    Ok(())
                }
            }
            TimeDiscretization::Anisotropic(time_segments) => {
                if time_segments.is_empty() {
                    return Err("no time segmentation defined".into());
                }
                for seg in time_segments {
                    seg.validate()?;
                }
                Ok(())
            }
        }
    }
}

/// Spec of the Cube Geometry
/// Cube is multidimensional box with all sides of the same length.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DomainDiscretization {
    /// Space discretization specification.
    pub space: SpaceDiscretization,

    /// Time discretization specification.
    pub time: TimeDiscretization,
}

impl DomainDiscretization {
    fn validate(&self, dimensions: u8, bounds: &DomainBoundsSpec) -> Result<(), String> {
        self.space
            .validate(dimensions, bounds)
            .and_then(|_| self.time.validate())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", untagged)]
pub enum DomainBoundsSpec {
    /// Size spec: 1m, 1km, 1mm 0.25cm etc.
    Cube(
        #[serde(
            deserialize_with = "quantity::deserialize_length",
            serialize_with = "quantity::serialize_length"
        )]
        Length,
    ),

    Hyperrectangle(
        #[serde(
            deserialize_with = "quantity::deserialize_lengths",
            serialize_with = "quantity::serialize_lengths"
        )]
        Vec<Length>,
    ),

    Mesh(ExternalResource),
}

impl DomainBoundsSpec {
    fn validate(&self, dim: u8) -> Result<(), String> {
        match self {
            DomainBoundsSpec::Cube(ds) => {
                if ds.value > 0.0 {
                    Ok(())
                } else {
                    Err("domain side length must be positive".into())
                }
            }
            DomainBoundsSpec::Hyperrectangle(items) => {
                if items.len() < (dim as usize) {
                    Err(format!(
                        "Hyperrectangle only defines {} / {} dimensions",
                        items.len(),
                        dim
                    ))
                } else if items.len() > (dim as usize) {
                    Err(format!(
                        "Hyperrectangle defines {} / {} too many dimensions",
                        items.len(),
                        dim
                    ))
                } else {
                    for (i, side) in items.iter().enumerate() {
                        if side.value < 0.0 {
                            return Err(format!(
                                "Hyperrectangle space domain bounds for dimension {i} is negative",
                            ));
                        }
                    }
                    Ok(())
                }
            }
            DomainBoundsSpec::Mesh(_) => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[validate_deser]
pub struct DomainSpec {
    /// Dimensionality of this domain.
    /// 3 for 3d space. 2 for flat-earthers. 4 for fanciness.
    pub dimensions: u8,

    pub bounds: DomainBoundsSpec,
    // pub cube: Option<CubeGeometrySpec>,
    // pub hyperrectangle: Option<HyperrectangleGeometrySpec>,
    /// If set, defines a URI to obtain executable math model
    // #[serde(default, skip_serializing_if = "Option::is_none")]
    // pub mesh: Option<ExternalResource>,

    /// Spec for how space and time of the domain are sampled
    pub discretization: DomainDiscretization,
}

impl Validate for DomainSpec {
    type Error = String;
    fn validate(&self) -> Result<(), Self::Error> {
        if self.dimensions == 0 {
            return Err(".dimensions: 0 is invalid number for spatial dimensions".into());
        }

        self.bounds
            .validate(self.dimensions)
            .and_then(|_| self.discretization.validate(self.dimensions, &self.bounds))
    }
}

/// Workload spec defining desired state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadSpec {
    /// The physical domain being simulated (e.g. `electrodynamics`, `hydrodynamics`).
    pub domain_type: DomainType,

    /// Discretization and simulation parameters: grid resolution, time step size,
    /// duration, physics constants, etc. What physical region exists.
    pub domain: DomainSpec,

    /// Reference to the WASM package implementing the governing equations.
    pub model: ModelSpec,

    /// Reference to an external resource defining the starting state
    /// (mesh, particle distribution, field configuration, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<WorkloadInputs>,

    /// Full set of conditions a worker must satisfy to run this workload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<WorkloadRequirements>,
}

/// Minimum hardware capabilities required by a workload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_cpu_cores: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_memory_bytes: Option<u64>,
    /// Required CPU architecture (e.g. `x86_64`, `aarch64`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Required accelerators (GPU model, OpenCL version, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accelerators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_opencl_version: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Workload runtime state
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimulationState {
    Running,
    Stopped,
}
/// The runtime state of the workload.
///
/// Modelled as an enum so that each phase carries only the fields that are
/// meaningful in that state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkloadStatus {
    /// No workload is loaded.
    NotLoaded,

    /// A workload manifest is being distributed and validated across nodes.
    Loading,

    /// All eligible nodes have loaded the workload and are awaiting a start
    /// command.
    Ready,

    /// The simulation is actively evolving.
    Running {
        /// Wall-clock time when the current run epoch started.
        // started_at: chrono::DateTime<chrono::Utc>,
        /// Current simulation time step reached by the cluster.
        simulation_time: f64,
        /// Per-partition convergence indicators.
        convergence_metrics: HashMap<String, f64>,
        /// Assignment of simulation-space partitions to node IDs.
        partition_map: HashMap<String, String>,
    },

    /// The simulation has been stopped (gracefully or forced) and can
    /// potentially be resumed from a checkpoint.
    Stopped {
        /// Simulation time at the moment the simulation was stopped.
        simulation_time: f64,
        /// Per-partition convergence indicators at stop time.
        convergence_metrics: HashMap<String, f64>,
        /// Partition-to-node assignment that was active when stopped.
        partition_map: HashMap<String, String>,
    },

    /// Loading or execution failed.
    Error {
        /// Human-readable description of the error.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Manifest — Kubernetes-style Custom Resource Definition of the workload
// ---------------------------------------------------------------------------
/// The top-level manifest resource, following Kubernetes CRD conventions.
/// The workload specification: everything needed to load and run a simulation.
///
/// ```yaml
/// apiVersion: orishu.dev/v1
/// kind: Workload
/// metadata:
///   name: em-cavity-resonance
///   labels:
///     domain: electrodynamics
/// spec:
///   domainType: electrodynamics
///   model:
///     image:
///       uri: "oci://registry.example.com/sim/hydro:v1"
///   domain:
///     dimensions: 3
///     bounds: 1m
///     discretization:
///       space: 1mm
///       time: 15ms  
///   inputs:
///     geometry:
///       mesh:
///         uri: "https://github.com/abbyssoul/orishu/blob/main/examples/spaces/air-tunnel.3mf"
///     initialConditions:
///       image:
///         uri: "https://github.com/abbyssoul/orishu/blob/main/examples/spaces/snapshot.orishu"
///   ...
/// ```
pub type Manifest = DefManifest<WorkloadSpec, WorkloadStatus>;

impl Manifest {
    /// Parse a manifest from a reader. Accepts both JSON and YAML.
    ///
    /// Uses YAML parsing, which is a superset of JSON, so both formats are
    /// handled in a single streaming pass without buffering the entire input.
    /// After successful deserialization, `apiVersion` and `kind` are validated.
    pub fn from_reader<R: std::io::Read>(reader: R) -> Result<Self, manifest::ParseError> {
        let manifest: Self = serde_yaml::from_reader(reader).map_err(manifest::ParseError::Yaml)?;
        Self::validate(manifest)
    }

    /// Parse a manifest from a string. Accepts both JSON and YAML.
    pub fn parse(input: &str) -> Result<Self, manifest::ParseError> {
        let manifest: Self = serde_json::from_str(input)
            .map_err(manifest::ParseError::Json)
            .or_else(|_| serde_yaml::from_str(input).map_err(manifest::ParseError::Yaml))?;
        Self::validate(manifest)
    }

    /// Convenience: parse from a byte slice (UTF-8).
    pub fn parse_bytes(input: &[u8]) -> Result<Self, manifest::ParseError> {
        let s = std::str::from_utf8(input).map_err(manifest::ParseError::InvalidUtf8)?;
        Self::parse(s)
    }

    fn validate(manifest: Self) -> Result<Self, manifest::ParseError> {
        if manifest.api_version != MANIFEST_API_VERSION || manifest.kind != WORKLOAD_MANIFEST_KIND {
            return Err(manifest::ParseError::InvalidResource {
                api_version: manifest.api_version,
                kind: manifest.kind,
            });
        }
        if let Err(derr) = manifest.spec.domain.validate() {
            return Err(manifest::ParseError::InvalidSpec { details: derr });
        }
        Ok(manifest)
    }
}

// ---------------------------------------------------------------------------
// Workload runtime types (unchanged from previous design)
// ---------------------------------------------------------------------------

/// The runtime state of the currently loaded workload, as seen by the cluster.
///
/// Modelled as an enum so that each phase carries only the fields that are
/// meaningful in that state.
#[derive(Debug, Clone)]
pub enum WorkloadStatusMessage {
    /// No workload is loaded.
    NotLoaded,

    /// A workload manifest is being distributed and validated across nodes.
    Loading { manifest: ObjectMeta },

    /// All eligible nodes have loaded the workload and are awaiting a start
    /// command.
    Ready { manifest: ObjectMeta },

    /// The simulation is actively evolving.
    Running {
        manifest: ObjectMeta,
        /// Wall-clock time when the current run epoch started.
        started_at: chrono::DateTime<chrono::Utc>,
        /// Current simulation time step reached by the cluster.
        simulation_time: f64,
        /// Per-partition convergence indicators.
        convergence_metrics: HashMap<String, f64>,
        /// Assignment of simulation-space partitions to node IDs.
        partition_map: HashMap<String, String>,
    },

    /// The simulation has been stopped (gracefully or forced) and can
    /// potentially be resumed from a checkpoint.
    Stopped {
        manifest: ObjectMeta,
        /// Simulation time at the moment the simulation was stopped.
        simulation_time: f64,
        /// Per-partition convergence indicators at stop time.
        convergence_metrics: HashMap<String, f64>,
        /// Partition-to-node assignment that was active when stopped.
        partition_map: HashMap<String, String>,
    },

    /// Loading or execution failed.
    Error {
        /// Present if the manifest was successfully parsed before the error.
        manifest: Option<ObjectMeta>,
        /// Human-readable description of the error.
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
    /// System-generated UID of the workload
    pub uid: String,
    pub epoch: u64,
    pub phase: WorkloadStatus,
}

/// Controls how the simulation is started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulationStateTransitionMode {
    /// Default. Wait for all eligible nodes to report `Ready` before beginning
    /// computation simultaneously across the cluster.
    WaitForAll,
    /// Each node begins computation as soon as it has loaded the workload,
    /// without waiting for all nodes to be ready. Late-joining nodes
    /// synchronize from a checkpoint or catch-up snapshot before contributing.
    Immediate,
}

impl QuerySet for &SimulationStateTransitionMode {
    fn to_query(&self) -> String {
        match self {
            SimulationStateTransitionMode::WaitForAll => "".to_string(),
            SimulationStateTransitionMode::Immediate => "force=true".to_owned(),
        }
    }
}

/// Patch message to express the Simulation state intent.
/// See [Protocol client/workload API section](./docs/protocol-client.md#workload-api)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationStateIntent {
    /// What's the target state? Running or Stopped
    pub run_state: SimulationState,
    /// How the workload should transition to a new state:
    /// "Coordinated" (default) or "Immediate"
    pub transition_mode: SimulationStateTransitionMode,
    /// When set, implies running only a fixed number of steps
    pub steps_limit: Option<u128>,
    /// How often take a checkpoint.
    /// None - no checkpoint in this run.
    /// Positive number, take a checkpoint every number of steps.
    pub checkpoint: Option<u64>,

    /// Checkpoint ID to reset simulation state to if required.
    /// Must be a valid checkpoint ID for the loaded workload.
    /// Note that server can only reset Stopped simulation.
    pub reset_to: Option<CheckpointId>,
}

/// A single frame in the live simulation stream.
#[derive(Debug, Clone)]
pub enum SimulationFrame {
    /// Full current state, sent once when the client connects.
    Snapshot(WorkloadStatus),
    /// Incremental state update.
    Delta {
        simulation_time: f64,
        partition_updates: HashMap<String, String>,
    },
    /// Stream ended; carries the reason.
    EndOfStream { reason: SimulationEndReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationEndReason {
    Stopped,
    Completed,
    Error,
}

/// Handle to a live simulation stream. Yields `SimulationFrame`s until the
/// simulation ends or the client drops the handle.
///
/// The transport implementation will back this with an SSE or WebSocket
/// connection. The stub exposes the intended interface only.
pub struct SimulationStream {
    _priv: (),
}

impl SimulationStream {
    /// Returns the next frame from the stream, or `None` on end-of-stream.
    pub async fn next(&mut self) -> Option<Result<SimulationFrame, crate::client::ClientError>> {
        todo!("SimulationStream::next not yet implemented")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::model::manifest;

    use super::*;
    use pretty_assertions::assert_eq;

    const MINIMAL_JSON: &str = r#"{
        "apiVersion": "orishu.dev/v1",
        "kind": "Workload",
        "metadata": { "name": "test-workload" },
        "spec": {
            "domainType": "hydrodynamics",
            "model": {
                "image": { "uri": "oci://registry.example.com/sim/hydro:v1" }
            },
            "domain": {
                "dimensions": 2,
                "bounds": "1m",
                "discretization": {
                    "space": "1mm",
                    "time":  "15ms"
                }
            }
        }
    }"#;

    const MINIMAL_YAML: &str = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: test-workload
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 2
    bounds: 1m
    discretization:
      space: 1mm
      time: 15ms
"#;

    const FULL_YAML: &str = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: em-cavity-resonance
  namespace: cluster_xyz
  labels:
    domain: electrodynamics
    priority: high
spec:
  domainType: electromagnetic
  model:
    image:
        uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 3
    bounds:
      - 100cm
      - 20cm
      - 30cm

    discretization:
      space: 1mm
      time: 15ms

  inputs:
    geometry:
      mesh:
          uri: "https://github.com/abbyssoul/orishu/blob/main/examples/spaces/air-tunnel.3mf"
    initialConditions:
      image:
          uri: "https://github.com/abbyssoul/orishu/blob/main/examples/spaces/snapshot.orishu"

  requirements:
    hardware:
      minCpuCores: 4
      minMemoryBytes: 8589934592
      architecture: x86_64
      accelerators:
        - "NVIDIA A100"
      minOpenclVersion: "3.0"
    runtimeAbiVersion: "1.2.0"
    executionProfile:
      numericMode: deterministic
      reductionOrder: fixed
"#;

    #[test]
    fn parse_minimal_json() {
        let r = Manifest::parse(MINIMAL_JSON);
        assert!(
            r.is_ok(),
            "failed to parse json: {} Error: {}",
            MINIMAL_JSON,
            r.unwrap_err()
        );

        let m = r.unwrap();
        assert_eq!(m.api_version, MANIFEST_API_VERSION);
        assert_eq!(m.kind, WORKLOAD_MANIFEST_KIND);
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("test-workload").unwrap()
        );
        assert_eq!(m.metadata.namespace, None);
        assert!(m.metadata.labels.is_empty());
        assert_eq!(m.spec.domain_type, DomainType::Hydrodynamics);
        assert_eq!(
            m.spec.model.image,
            Some(ExternalResource::Image {
                uri: "oci://registry.example.com/sim/hydro:v1".to_string()
            })
        );

        assert!(matches!(m.spec.domain.bounds, DomainBoundsSpec::Cube(_)));
        assert!(m.spec.inputs.is_none());
        assert!(m.spec.requirements.is_none());
    }

    #[test]
    fn parse_minimal_yaml() {
        let m = Manifest::parse(MINIMAL_YAML).unwrap();
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("test-workload").unwrap()
        );
        assert_eq!(m.spec.domain_type, DomainType::Hydrodynamics);
        assert_eq!(
            m.spec.model.image,
            Some(ExternalResource::Image {
                uri: "oci://registry.example.com/sim/hydro:v1".to_string()
            })
        );
    }

    #[test]
    fn parse_full_yaml() {
        let m = Manifest::parse(FULL_YAML).unwrap();
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("em-cavity-resonance").unwrap()
        );
        assert_eq!(m.metadata.namespace.as_deref(), Some("cluster_xyz"));
        assert_eq!(m.metadata.labels.get("domain").unwrap(), "electrodynamics");
        assert_eq!(m.metadata.labels.get("priority").unwrap(), "high");

        assert_eq!(m.spec.domain_type, DomainType::Electromagnetic);

        if let DomainBoundsSpec::Hyperrectangle(ref v) = m.spec.domain.bounds {
            assert_eq!(v.len(), 3);
            assert_eq!(v[0], Length::new::<uom::si::length::centimeter>(100.0));
            assert_eq!(v[1], Length::new::<uom::si::length::centimeter>(20.0));
            assert_eq!(v[2], Length::new::<uom::si::length::centimeter>(30.0));
        } else {
            panic!("Expected DomainBoundSpec::Hyperrectangle");
        }

        assert!(m.spec.requirements.is_some());

        let hw = &m.spec.requirements.as_ref().unwrap().hardware;
        assert_eq!(hw.min_cpu_cores, Some(4));
        assert_eq!(hw.min_memory_bytes, Some(8_589_934_592));
        assert_eq!(hw.architecture.as_deref(), Some("x86_64"));
        assert_eq!(hw.accelerators, vec!["NVIDIA A100"]);
        assert_eq!(hw.min_opencl_version.as_deref(), Some("3.0"));

        assert_eq!(
            m.spec
                .requirements
                .as_ref()
                .unwrap()
                .runtime_abi_version
                .as_deref(),
            Some("1.2.0")
        );
        assert_eq!(
            m.spec
                .requirements
                .as_ref()
                .unwrap()
                .execution_profile
                .get("numericMode")
                .unwrap(),
            "deterministic"
        );
    }

    #[test]
    fn parse_wrong_api_version() {
        let input = MINIMAL_JSON.replace("orishu.dev/v1", "orishu.dev/v99");
        let err = Manifest::parse(&input).unwrap_err();
        match err {
            manifest::ParseError::InvalidResource { api_version, kind } => {
                assert_eq!(api_version, "orishu.dev/v99");
                assert_eq!(kind, WORKLOAD_MANIFEST_KIND);
            }
            other => panic!("expected InvalidResource, got: {other}"),
        }
    }

    #[test]
    fn parse_wrong_kind() {
        let input = MINIMAL_JSON.replace("Workload", "SomethingElse");
        let err = Manifest::parse(&input).unwrap_err();
        match err {
            manifest::ParseError::InvalidResource { kind, .. } => {
                assert_eq!(kind, "SomethingElse");
            }
            other => panic!("expected InvalidResource, got: {other}"),
        }
    }

    #[test]
    fn parse_invalid_json_falls_back_to_yaml() {
        // Trailing comma is invalid JSON but the content is not valid YAML either
        // if structured as JSON. This test verifies the fallback path runs.
        // We use valid YAML to confirm it succeeds via the fallback.
        let m = Manifest::parse(MINIMAL_YAML).unwrap();
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("test-workload").unwrap()
        );
    }

    #[test]
    fn parse_garbage_returns_error() {
        let err = Manifest::parse("not a manifest at all {{{").unwrap_err();
        assert!(matches!(err, manifest::ParseError::Yaml(_)));
    }

    #[test]
    fn parse_missing_required_model_field() {
        let input = r#"{
            "apiVersion": "orishu.dev/v1",
            "kind": "Workload",
            "metadata": { "name": "x" },
            "spec": {
                "domainType": "hydrodynamics"
            }
        }"#;
        // Missing domain and model — should fail deserialization.
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("domain") || msg.contains("model"),
            "error should mention missing field, got: {msg}"
        );
    }

    #[test]
    fn parse_missing_required_image_url_field() {
        let input = r#"{
            "apiVersion": "orishu.dev/v1",
            "kind": "Workload",
            "metadata": { "name": "x" },
            "spec": {
                "domainType": "hydrodynamics",
                "domain": {
                    "dimensions": 2,
                    "bounds": "1m",
                    "discretization": {
                        "space": { "uniform": "1mm" },
                        "time":  { "uniform": "15ms" }
                    }
                },
                "model": { "image": {}},
            }
        }"#;
        // Missing model.url — should fail deserialization.
        let parser_result = Manifest::parse(input);
        assert!(
            parser_result.is_err(),
            "was expecting error parsing manifest, go OK"
        );

        let err = parser_result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("domain") || msg.contains("model"),
            "error should mention missing field, got: {msg}"
        );
    }

    #[test]
    fn parse_rejects_nested_mesh_image_block() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: malformed-geometry
spec:
  domainType: electromagnetic
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 3
    bounds: 100cm
    discretization:
      space: 1mm
      time: 15ms
  inputs:
    geometry:
      mesh:
        image:
          uri: "https://example.com/mesh.3mf"
"#;

        let parser_result = Manifest::parse(input);
        assert!(
            parser_result.is_err(),
            "was expecting error parsing manifest, go OK!"
        );

        let err = parser_result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("ExternalResource") || msg.contains("geometry"),
            "error should mention the malformed geometry resource, got: {msg}"
        );
    }

    #[test]
    fn parse_bytes_valid_utf8() {
        let m = Manifest::parse_bytes(MINIMAL_JSON.as_bytes()).unwrap();
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("test-workload").unwrap()
        );
    }

    #[test]
    fn parse_bytes_invalid_utf8() {
        let bad = &[0xff, 0xfe, 0xfd];
        let err = Manifest::parse_bytes(bad).unwrap_err();
        assert!(matches!(err, manifest::ParseError::InvalidUtf8(_)));
    }

    #[test]
    fn json_round_trip() {
        let original = Manifest::parse(MINIMAL_JSON).unwrap();
        let serialized = serde_json::to_string_pretty(&original).unwrap();

        let parser_result = Manifest::parse(&serialized);
        assert!(
            parser_result.is_ok(),
            "failed to parse serialized data back: {}\nerror: {}",
            serialized,
            parser_result.unwrap_err()
        );

        let parsed = parser_result.unwrap();
        assert_eq!(original.metadata.name, parsed.metadata.name);
        assert_eq!(original.spec.domain_type, parsed.spec.domain_type);
        assert_eq!(original.spec.model, parsed.spec.model);
        assert_eq!(original.spec.domain, parsed.spec.domain);
    }

    #[test]
    fn optional_fields_default_when_absent() {
        let m = Manifest::parse(MINIMAL_JSON).unwrap();
        assert_eq!(m.spec.inputs, None);
        assert_eq!(m.spec.requirements, None);
    }

    #[test]
    fn parse_non_uniform_space_discretization_uniform_time() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 6
    bounds: 3m
    discretization:
      space:
      - - length: 0.08m
          step: 2mm
        - length: 0.12m
          step: 0.03m
      - 0.12m
      - 3mm
      - - length: 8cm
          step: 2mm
        - length: 9cm
          step: 2.1mm
        - length: 1cm
          step: 1mm
      -  1.74cm
      -  31mm
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let parser_result = Manifest::parse(input);
        assert!(
            parser_result.is_ok(),
            "was expecting to parsing input manifest, go Err: {}",
            parser_result.unwrap_err(),
        );

        let m = parser_result.unwrap();
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("non-uniform-discretization").unwrap()
        );
        assert_eq!(m.spec.domain_type, DomainType::Electromagnetic);
        assert!(m.spec.requirements.is_none());

        if let DomainBoundsSpec::Cube(side) = m.spec.domain.bounds {
            assert_eq!(side, Length::new::<uom::si::length::meter>(3.0));
        } else {
            panic!("Expected DomainBoundSpec::Cube");
        }

        if let SpaceDiscretization::Anisotropic(det) = m.spec.domain.discretization.space {
            assert_eq!(det.len(), 6);
            assert_eq!(
                det[0],
                AnisotropicDimension::Segments(vec![
                    SpaceSegment {
                        length: Length::new::<uom::si::length::centimeter>(8.0),
                        step: Length::new::<uom::si::length::millimeter>(2.0)
                    },
                    SpaceSegment {
                        length: Length::new::<uom::si::length::centimeter>(12.0),
                        step: Length::new::<uom::si::length::centimeter>(3.0)
                    }
                ])
            );
            assert_eq!(
                det[1],
                AnisotropicDimension::Uniform(Length::new::<uom::si::length::centimeter>(12.0))
            );
            assert_eq!(
                det[2],
                AnisotropicDimension::Uniform(Length::new::<uom::si::length::millimeter>(3.0))
            );
            assert_eq!(
                det[3],
                AnisotropicDimension::Segments(vec![
                    SpaceSegment {
                        length: Length::new::<uom::si::length::centimeter>(8.0),
                        step: Length::new::<uom::si::length::millimeter>(2.0)
                    },
                    SpaceSegment {
                        length: Length::new::<uom::si::length::centimeter>(9.0),
                        step: Length::new::<uom::si::length::millimeter>(2.1)
                    },
                    SpaceSegment {
                        length: Length::new::<uom::si::length::centimeter>(1.0),
                        step: Length::new::<uom::si::length::millimeter>(1.0)
                    }
                ])
            );
            assert_eq!(
                det[4],
                AnisotropicDimension::Uniform(Length::new::<uom::si::length::centimeter>(1.74))
            );
            assert_eq!(
                det[5],
                AnisotropicDimension::Uniform(Length::new::<uom::si::length::millimeter>(31.0))
            );
        } else {
            panic!(
                "Expected space discretization to be SpaceDiscretization::Anisotropic got something else instead"
            );
        }

        if let TimeDiscretization::Uniform(det) = m.spec.domain.discretization.time {
            assert_eq!(det, Time::new::<uom::si::time::millisecond>(15.0));
        } else {
            panic!(
                "Expected time discretization to be TimeDiscretization::Uniform got something else instead"
            );
        }
    }

    #[test]
    fn parse_non_uniform_space_discretization_mismatches_dimensions_too_many() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 1
    bounds: 3m
    discretization:
      space:
      - - length: 0.08m
          step: 2mm
        - length: 0.12m
          step: 0.03m
      - 0.12m
      - 3mm
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("too many dimensions 3 / 1 discretization configured"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn parse_non_uniform_space_discretization_mismatches_dimensions_too_few() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 4
    bounds: 3m
    discretization:
      space:
      - 0.12m
      - - length: 0.08m
          step: 2mm
        - length: 0.12m
          step: 0.03m
      - 3mm
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("too few dimensions 3 / 4 discretization configured"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn parse_non_uniform_space_discretization_with_way_too_big_steps() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 6
    bounds: 3m
    discretization:
      space:
      - - length: 0.08m
          step: 2mm
        - length: 0.12m
          step: 0.03m
      - 0.12m
      - 3mm
      - - length: 8cm
          step: 2mm
        - length: 9cm
          step: 12.1m
        - length: 1cm
          step: 1mm
      -  1.74cm
      -  31mm
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("space-segment length 0.09 m is less than the step size 12.1 m"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn parse_non_uniform_time_discretization_uniform_space() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 3
    bounds: 3m
    discretization:
      space: 1.25mm
      time:
      - duration: 3.1s
        step: 1.2ms
      - duration: 16.1s
        step: 80.7ms
      - duration: 6s
        step: 1.75s
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let parser_result = Manifest::parse(input);
        assert!(
            parser_result.is_ok(),
            "was expecting to parsing input manifest, go Err: {}",
            parser_result.unwrap_err(),
        );

        let m = parser_result.unwrap();
        assert_eq!(
            m.metadata.name,
            manifest::Name::from_str("non-uniform-discretization").unwrap()
        );
        assert_eq!(m.spec.domain_type, DomainType::Electromagnetic);
        assert!(m.spec.requirements.is_none());

        if let DomainBoundsSpec::Cube(side) = m.spec.domain.bounds {
            assert_eq!(side, Length::new::<uom::si::length::meter>(3.0));
        } else {
            panic!("Expected DomainBoundSpec::Cube");
        }

        if let SpaceDiscretization::Uniform(det) = m.spec.domain.discretization.space {
            assert_eq!(det, Length::new::<uom::si::length::millimeter>(1.25));
        } else {
            panic!(
                "Expected space discretization to be SpaceDiscretization::Uniform got something else instead"
            );
        }

        if let TimeDiscretization::Anisotropic(det) = m.spec.domain.discretization.time {
            assert_eq!(det.len(), 3);
            assert_eq!(
                det[0],
                TimeSegment {
                    duration: Time::new::<uom::si::time::second>(3.1),
                    step: Time::new::<uom::si::time::millisecond>(1.2)
                }
            );
            assert_eq!(
                det[1],
                TimeSegment {
                    duration: Time::new::<uom::si::time::second>(16.1),
                    step: Time::new::<uom::si::time::millisecond>(80.7)
                }
            );
            assert_eq!(
                det[2],
                TimeSegment {
                    duration: Time::new::<uom::si::time::second>(6.0),
                    step: Time::new::<uom::si::time::second>(1.75)
                }
            );
        } else {
            panic!(
                "Expected time discretization to be TimeDiscretization::Anisotropic got something else instead"
            );
        }
    }

    #[test]
    fn parse_non_uniform_time_discretization_too_big_steps() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 1
    bounds: 3m
    discretization:
      space: 1.25mm
      time:
      - duration: 3.1s
        step: 1.2ms
      - duration: 16.1s
        step: 80.7s
      - duration: 6s
        step: 1.75s
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("time-segment duration 16.1 s is less than the step size 80.7 s"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn validate_hyperrectangle_too_few_dimension_mismatch() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: bad-hyperrect
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 3
    bounds:
      - 100cm
      - 20cm
    discretization:
      space: 1mm
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Hyperrectangle only defines 2 / 3 dimensions"),
            "expected hyperrectangle dimension error, got: {msg}"
        );
    }

    #[test]
    fn validate_hyperrectangle_too_many_dimension_mismatch() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: bad-hyperrect
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 1
    bounds:
      - 100cm
      - 20cm
      - 130cm
      - 830cm
    discretization:
      space: 1mm
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Hyperrectangle defines 4 / 1 too many dimensions"),
            "expected hyperrectangle dimension error, got: {msg}"
        );
    }

    #[test]
    fn validate_hyperrectangle_dimensions_match() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: good-hyperrect
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 2
    bounds:
      - 100cm
      - 20cm
    discretization:
      space: 1mm
      time: 15ms
"#;
        let result = Manifest::parse(input);
        assert!(
            result.is_ok(),
            "expected valid manifest, got error: {}",
            result.unwrap_err()
        );
        if let DomainBoundsSpec::Hyperrectangle(ref v) = result.unwrap().spec.domain.bounds {
            assert_eq!(v.len(), 2);
        } else {
            panic!("Expected DomainBoundsSpec::Hyperrectangle");
        }
    }

    #[test]
    fn validate_space_segment_step_exceeds_length() {
        let segment = SpaceSegment {
            length: Length::new::<uom::si::length::centimeter>(5.0),
            step: Length::new::<uom::si::length::centimeter>(10.0),
        };
        let err = segment.validate().unwrap_err();
        assert!(
            err.contains("space-segment length 0.05 m is less than the step size 0.1 m"),
            "expected space-segment validation error, got: {err}"
        );
    }

    #[test]
    fn validate_space_segment_step_equals_length() {
        let segment = SpaceSegment {
            length: Length::new::<uom::si::length::centimeter>(5.0),
            step: Length::new::<uom::si::length::centimeter>(5.0),
        };
        assert!(segment.validate().is_ok());
    }

    #[test]
    fn validate_time_segment_step_exceeds_duration() {
        let segment = TimeSegment {
            duration: Time::new::<uom::si::time::second>(1.0),
            step: Time::new::<uom::si::time::second>(5.0),
        };
        let err = segment.validate().unwrap_err();
        assert!(
            err.contains("time-segment duration 1 s is less than the step size 5 s"),
            "expected time-segment validation error, got: {err}"
        );
    }

    #[test]
    fn validate_time_segment_step_equals_duration() {
        let segment = TimeSegment {
            duration: Time::new::<uom::si::time::second>(1.0),
            step: Time::new::<uom::si::time::second>(1.0),
        };
        assert!(segment.validate().is_ok());
    }

    #[test]
    fn validate_domain_zero_dimensions() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: zero-dim
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 0
    bounds: 1m
    discretization:
      space: 1mm
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("0 is invalid number for spatial dimensions"),
            "expected zero-dimensions error, got: {msg}"
        );
    }

    #[test]
    fn validate_domain_empty_space_segments_list() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: zero-dim
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 1
    bounds: 1m
    discretization:
      space: []
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("too few dimensions 0 / 1 discretization configured"),
            "expected zero-dimensions error, got: {msg}"
        );
    }

    #[test]
    fn validate_domain_space_empty_segment() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: zero-dim
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 1
    bounds: 1m
    discretization:
      space: [[]]
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("space-step has no segments for dimension 0"),
            "expected zero-dimensions error, got: {msg}"
        );
    }

    #[test]
    fn validate_uniform_step_exceeds_cube_bounds() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: step-too-big
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 2
    bounds: 1m
    discretization:
      space: 2m
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("uniform space-step exceeds domain bounds"),
            "expected step-exceeds-bounds error, got: {msg}"
        );
    }

    #[test]
    fn validate_uniform_step_exceeds_hyperrectangle_bounds() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: step-too-big-hyperrect
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 2
    bounds:
      - 100cm
      - 5mm
    discretization:
      space: 1cm
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("uniform space-step exceeds domain bounds for dimension 1"),
            "expected step-exceeds-bounds error for dimension 1, got: {msg}"
        );
    }

    #[test]
    fn validate_anisotropic_step_exceeds_cube_bounds() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: aniso-step-too-big
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 2
    bounds: 10cm
    discretization:
      space:
      - 1mm
      - 50cm
      time: 15ms
"#;
        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("space-step exceeds domain bounds for dimension 1"),
            "expected step-exceeds-bounds error for dimension 1, got: {msg}"
        );
    }

    #[test]
    fn validate_uniform_step_within_bounds_ok() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: step-ok
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
  domain:
    dimensions: 2
    bounds: 1m
    discretization:
      space: 1mm
      time: 15ms
"#;
        let result = Manifest::parse(input);
        assert!(
            result.is_ok(),
            "expected valid manifest, got error: {}",
            result.unwrap_err()
        );
    }

    #[test]
    fn parse_fail_negative_uniform_bounds() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 1
    bounds: "-13m"
    discretization:
      space: 3mm
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("domain side length must be positive"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn parse_fail_negative_non_uniform_bounds() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 3
    bounds:
    - 10m
    - -13m
    - 5m
    discretization:
      space: 3mm
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Hyperrectangle space domain bounds for dimension 1 is negative"),
            "expected domain side length error, got: {msg}"
        );
    }
    #[test]
    fn parse_fail_negative_non_uniform_discretization() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform_negative-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 3
    bounds: 12m
    discretization:
      space: 
       - 3mm
       - 2mm
       - "-6mm"
      time: 15ms
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("space-step for dimension 2 must be positive"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn parse_fail_timetraveling_reverasal() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform_negative-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 3
    bounds: 12m
    discretization:
      space: 36mm
      time: "-15ms"
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("uniform time-step must be positive"),
            "expected space discretization dimension error, got: {msg}"
        );
    }

    #[test]
    fn parse_fail_non_uniform_timetraveling_reverasal() {
        let input = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: non-uniform_negative-discretization
spec:
  domainType: electromagnetic
  domain:
    dimensions: 3
    bounds: 12m
    discretization:
      space: 36mm
      time:
      - duration: 3.1s
        step: 1.2ms
      - duration: 16.1s
        step: -80.7ms
      - duration: 6s
        step: 1.75s
  model:
    image:
      uri: "oci://registry.example.com/sim/hydro:v1"
"#;

        let err = Manifest::parse(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("time-segment step -0.0807"),
            "expected space discretization dimension error, got: {msg}"
        );
    }
}
