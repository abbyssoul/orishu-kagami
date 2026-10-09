//! Identified fixed-profile run commands and historical facts. These types do
//! not execute commands. IO adapters must bound/preflight bytes before serde.
use orishu_identity::NodeId;
use serde::{Deserialize, Serialize};

use super::{
    cluster::OperationId,
    run::{RunDescriptor, RunIdentity},
};

/// Small command envelope cap, independent of workload/observation buffers.
pub const MAX_RUN_COMMAND_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum StatusRequestVersion {
    #[serde(rename = "orishu.run-status-request/v1")]
    V1,
}

/// Exact read-only status query. It never substitutes a currently selected run
/// or attributes a previously submitted command to a changed boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunStatusRequest {
    api_version: StatusRequestVersion,
    run: RunIdentity,
}
impl RunStatusRequest {
    /// Query this exact identity. An unallocated/absent run is unavailable.
    pub fn new(run: RunIdentity) -> Self {
        Self {
            api_version: StatusRequestVersion::V1,
            run,
        }
    }
    /// Full immutable execution identity requested by the observer.
    pub fn run(&self) -> &RunIdentity {
        &self.run
    }
}

/// Explicit manual-execution actions. Finish is terminal, never resumable pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunCommand {
    /// Compute and publish exactly one fixed step from the expected boundary.
    Step,
    /// Permanently stop integration at the expected boundary; retain observations.
    Finish,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum RequestVersion {
    #[serde(rename = "orishu.run-command-request/v1")]
    V1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestWire {
    api_version: RequestVersion,
    operation_id: OperationId,
    run: RunIdentity,
    expected_boundary: u64,
    command: RunCommand,
}

/// Exact intent: replay key is (formation, operation ID), and changing any other
/// field conflicts. Identity is never resolved through a mutable "current run".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RequestWire", into = "RequestWire")]
pub struct RunCommandRequest {
    operation_id: OperationId,
    run: RunIdentity,
    expected_boundary: u64,
    command: RunCommand,
}

/// Stable bounded invalid-fact classification, with no arbitrary input payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunCommandError {
    /// Epoch zero or an impossible next boundary is not executable intent.
    #[error("invalid run command request")]
    Request,
    /// Status has an unallocated epoch or invalid SI simulation time.
    #[error("invalid committed run status")]
    Status,
    /// Claimed application does not match the exact request/action/boundary.
    #[error("run command outcome does not match request")]
    Outcome,
}

impl RunCommandRequest {
    /// Construct bounded typed intent; no authority or execution is acquired.
    pub fn new(
        operation_id: OperationId,
        run: RunIdentity,
        expected_boundary: u64,
        command: RunCommand,
    ) -> Result<Self, RunCommandError> {
        if run.workload_epoch().get() == 0
            || (command == RunCommand::Step && expected_boundary == u64::MAX)
        {
            return Err(RunCommandError::Request);
        }
        Ok(Self {
            operation_id,
            run,
            expected_boundary,
            command,
        })
    }
    /// Caller-assigned stable retry identity.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
    /// Complete exact execution identity, including owner-issued epoch.
    pub fn run(&self) -> &RunIdentity {
        &self.run
    }
    /// Never automatically refreshed on stale-boundary refusal or lost reply.
    pub fn expected_boundary(&self) -> u64 {
        self.expected_boundary
    }
    /// Exact action whose outcome is being reconciled.
    pub fn command(&self) -> RunCommand {
        self.command
    }
}
impl TryFrom<RequestWire> for RunCommandRequest {
    type Error = RunCommandError;
    fn try_from(value: RequestWire) -> Result<Self, Self::Error> {
        Self::new(
            value.operation_id,
            value.run,
            value.expected_boundary,
            value.command,
        )
    }
}
impl From<RunCommandRequest> for RequestWire {
    fn from(value: RunCommandRequest) -> Self {
        Self {
            api_version: RequestVersion::V1,
            operation_id: value.operation_id,
            run: value.run,
            expected_boundary: value.expected_boundary,
            command: value.command,
        }
    }
}

/// Fixed-profile integration state, distinct from network/worker health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunPhase {
    /// Can accept another manually requested step; no background scheduler implied.
    Ready,
    /// Terminal finish accepted; retained state remains available until disposal.
    Finished,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum StatusVersion {
    #[serde(rename = "orishu.run-status/v1")]
    V1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatusWire {
    api_version: StatusVersion,
    descriptor: RunDescriptor,
    boundary: u64,
    time_seconds: f64,
    phase: RunPhase,
}

/// Small committed metadata projection, not field/object observations, restored
/// state or workload identity. Embedding in a receipt makes it a historical fact.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "StatusWire", into = "StatusWire")]
pub struct RunStatus {
    descriptor: RunDescriptor,
    boundary: u64,
    time_seconds: f64,
    phase: RunPhase,
}
impl RunStatus {
    /// Validate finite nonnegative SI time and a nonzero allocated execution epoch.
    pub fn new(
        descriptor: RunDescriptor,
        boundary: u64,
        time_seconds: f64,
        phase: RunPhase,
    ) -> Result<Self, RunCommandError> {
        if descriptor.identity().workload_epoch().get() == 0
            || !time_seconds.is_finite()
            || time_seconds < 0.0
        {
            return Err(RunCommandError::Status);
        }
        Ok(Self {
            descriptor,
            boundary,
            time_seconds,
            phase,
        })
    }
    /// Exact execution source, never inferred from labels or serving node.
    pub fn descriptor(&self) -> &RunDescriptor {
        &self.descriptor
    }
    /// Owner-accepted boundary, never a speculative attempt counter.
    pub fn boundary(&self) -> u64 {
        self.boundary
    }
    /// Simulation time in SI seconds, not wall time or playback time.
    pub fn time_seconds(&self) -> f64 {
        self.time_seconds
    }
    /// Whether the fixed executor can advance or has terminally finished.
    pub fn phase(&self) -> RunPhase {
        self.phase
    }
}
impl TryFrom<StatusWire> for RunStatus {
    type Error = RunCommandError;
    fn try_from(value: StatusWire) -> Result<Self, Self::Error> {
        Self::new(
            value.descriptor,
            value.boundary,
            value.time_seconds,
            value.phase,
        )
    }
}
impl From<RunStatus> for StatusWire {
    fn from(value: RunStatus) -> Self {
        Self {
            api_version: StatusVersion::V1,
            descriptor: value.descriptor,
            boundary: value.boundary,
            time_seconds: value.time_seconds,
            phase: value.phase,
        }
    }
}

/// Known non-application. Publication/reply uncertainty is not a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "camelCase", deny_unknown_fields)]
pub enum RunCommandRefusal {
    /// Another operation occupied the non-queuing executor slot.
    Busy,
    /// No usable executor with exactly the requested identity was available.
    RunUnavailable,
    /// Compared inside the executor before any computation/publication.
    StaleBoundary {
        /// Exact committed boundary observed by the serialized executor.
        actual: u64,
    },
    /// Operation cancellation/deadline prevented application.
    Cancelled,
    /// Scientific validation, guest or lifecycle refusal before publication.
    Scientific,
    /// Formation publication capacity refused before computation began.
    Capacity,
}
/// Historical application fact, not present availability or scientific persistence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
pub enum RunCommandOutcome {
    /// The owner acknowledged this action's resulting committed metadata.
    Applied { status: RunStatus },
    /// This command is known not to have been applied.
    Refused { reason: RunCommandRefusal },
    /// Application may have happened; never automatically execute again.
    Indeterminate,
}
/// Durable reservation is distinct from known application.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    content = "outcome",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum RunCommandState {
    /// Recorded intent, not an execution acknowledgement.
    Pending,
    /// Immutable historical outcome.
    Finished(RunCommandOutcome),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum ReceiptVersion {
    #[serde(rename = "orishu.run-command-receipt/v1")]
    V1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReceiptWire {
    api_version: ReceiptVersion,
    request: RunCommandRequest,
    source_node_id: NodeId,
    state: RunCommandState,
}
/// Validated correlated command receipt, using the same checks during serde.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ReceiptWire", into = "ReceiptWire")]
pub struct RunCommandReceipt {
    request: RunCommandRequest,
    source_node_id: NodeId,
    state: RunCommandState,
}
impl RunCommandReceipt {
    /// Refuse cross-run, wrong-boundary or wrong-phase application claims.
    pub fn new(
        request: RunCommandRequest,
        source_node_id: NodeId,
        state: RunCommandState,
    ) -> Result<Self, RunCommandError> {
        match &state {
            RunCommandState::Finished(RunCommandOutcome::Applied { status }) => {
                let (boundary, phase) = match request.command {
                    RunCommand::Step => (
                        request
                            .expected_boundary
                            .checked_add(1)
                            .ok_or(RunCommandError::Outcome)?,
                        RunPhase::Ready,
                    ),
                    RunCommand::Finish => (request.expected_boundary, RunPhase::Finished),
                };
                if status.descriptor.identity() != request.run()
                    || status.boundary != boundary
                    || status.phase != phase
                {
                    return Err(RunCommandError::Outcome);
                }
            }
            RunCommandState::Finished(RunCommandOutcome::Refused {
                reason: RunCommandRefusal::StaleBoundary { actual },
            }) if *actual == request.expected_boundary => return Err(RunCommandError::Outcome),
            _ => {}
        }
        Ok(Self {
            request,
            source_node_id,
            state,
        })
    }
    /// Original immutable intent; changing any field is an identity conflict.
    pub fn request(&self) -> &RunCommandRequest {
        &self.request
    }
    /// Historical producing worker, not a routing hint for a newer incarnation.
    pub fn source_node_id(&self) -> &NodeId {
        &self.source_node_id
    }
    /// Recorded command outcome; Applied does not imply a still-live run.
    pub fn state(&self) -> &RunCommandState {
        &self.state
    }
}
impl TryFrom<ReceiptWire> for RunCommandReceipt {
    type Error = RunCommandError;
    fn try_from(value: ReceiptWire) -> Result<Self, Self::Error> {
        Self::new(value.request, value.source_node_id, value.state)
    }
}
impl From<RunCommandReceipt> for ReceiptWire {
    fn from(value: RunCommandReceipt) -> Self {
        Self {
            api_version: ReceiptVersion::V1,
            request: value.request,
            source_node_id: value.source_node_id,
            state: value.state,
        }
    }
}
