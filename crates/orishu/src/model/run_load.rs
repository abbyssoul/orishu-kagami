//! Identified scientific admission facts, not mutable run status.
//!
//! IO adapters must bound/preflight encoded inputs before using serde. Neither
//! transport lengths nor archive representation participate in retry identity.
use orishu_identity::{FormationId, NodeId};
use orishu_workload::WorkloadDigest;
use serde::{Deserialize, Serialize};

use super::{cluster::OperationId, run::RunDescriptor};

/// Versioned complete-upload media type: big-endian u32 CBOR request length,
/// exactly that request, then the complete portable workload bundle to body EOF.
pub const RUN_LOAD_MEDIA_TYPE: &str = "application/vnd.orishu.run-load.v1";
/// Host/client preflight cap for the small request portion, before allocation.
pub const MAX_LOAD_REQUEST_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum RequestVersion {
    #[serde(rename = "orishu.run-load-request/v1")]
    V1,
}

/// Logical request: exact retries are scoped by formation and operation ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadRequest {
    api_version: RequestVersion,
    operation_id: OperationId,
    formation_id: FormationId,
    workload_id: WorkloadDigest,
}

impl LoadRequest {
    /// Construct intent. This neither reserves execution nor validates artifacts.
    pub fn new(
        operation_id: OperationId,
        formation_id: FormationId,
        workload_id: WorkloadDigest,
    ) -> Self {
        Self {
            api_version: RequestVersion::V1,
            operation_id,
            formation_id,
            workload_id,
        }
    }

    /// Client-assigned retry identity, not a workload or run ID.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
    /// Expected formation, checked by the execution owner before new admission.
    pub fn formation_id(&self) -> &FormationId {
        &self.formation_id
    }
    /// Expected immutable root, independently checked before JIT.
    pub fn workload_id(&self) -> WorkloadDigest {
        self.workload_id
    }
}

/// Bounded terminal reasons; detailed guest/input diagnostics are not receipts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadRefusal {
    /// The formation could not grant or retain the execution lease.
    FormationUnavailable,
    /// Exact bounded body delivery did not complete.
    Delivery,
    /// Complete closure or scientific admission failed validation.
    InvalidWorkload,
    /// Host policy denied execution or resources.
    Policy,
    /// Cancellation completed before any initial state was published.
    Cancelled,
    /// Initial-state publication was definitively refused by the owner.
    Publication,
}

/// Durable operation outcome. Acceptance is historical, not proof of a live run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
pub enum LoadOutcome {
    /// The formation owner published the initial boundary of this execution.
    Accepted {
        /// Owner-issued identity; must match the request and use a nonzero epoch.
        descriptor: RunDescriptor,
    },
    /// The caller knows that this attempt did not publish initial state.
    Refused {
        /// Stable non-payload-bearing refusal.
        reason: LoadRefusal,
    },
    /// An interrupted attempt may have published; do not automatically retry it.
    Indeterminate,
}

/// Reservation is explicitly distinct from either acceptance or refusal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    content = "outcome",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum LoadState {
    /// Durable intent exists, but no final receipt has been recorded.
    Pending,
    /// Immutable historical decision (including uncertainty).
    Finished(LoadOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ReceiptVersion {
    #[serde(rename = "orishu.run-load-receipt/v1")]
    V1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReceiptWire {
    api_version: ReceiptVersion,
    request: LoadRequest,
    source_node_id: NodeId,
    state: LoadState,
}

/// Validated, versioned admission fact. No credentials, paths or mutable status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ReceiptWire", into = "ReceiptWire")]
pub struct LoadReceipt {
    request: LoadRequest,
    source_node_id: NodeId,
    state: LoadState,
}

/// A receipt must not attribute an unrelated execution to an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("load receipt execution does not match its request")]
pub struct ReceiptIdentityError;

impl LoadReceipt {
    /// Construct a receipt, checking accepted execution identity even for serde.
    pub fn new(
        request: LoadRequest,
        source_node_id: NodeId,
        state: LoadState,
    ) -> Result<Self, ReceiptIdentityError> {
        if let LoadState::Finished(LoadOutcome::Accepted { descriptor }) = &state {
            let run = descriptor.identity();
            if run.formation_id() != request.formation_id()
                || run.workload_id() != request.workload_id()
                || run.workload_epoch().get() == 0
            {
                return Err(ReceiptIdentityError);
            }
        }
        Ok(Self {
            request,
            source_node_id,
            state,
        })
    }

    /// Original intent, retained unchanged on replay.
    pub fn request(&self) -> &LoadRequest {
        &self.request
    }
    /// Producing node at reservation, not the node currently serving this fact.
    pub fn source_node_id(&self) -> &NodeId {
        &self.source_node_id
    }
    /// Historical state. A pending receipt is never run acceptance.
    pub fn state(&self) -> &LoadState {
        &self.state
    }
}

impl TryFrom<ReceiptWire> for LoadReceipt {
    type Error = ReceiptIdentityError;
    fn try_from(value: ReceiptWire) -> Result<Self, Self::Error> {
        Self::new(value.request, value.source_node_id, value.state)
    }
}

impl From<LoadReceipt> for ReceiptWire {
    fn from(value: LoadReceipt) -> Self {
        Self {
            api_version: ReceiptVersion::V1,
            request: value.request,
            source_node_id: value.source_node_id,
            state: value.state,
        }
    }
}
