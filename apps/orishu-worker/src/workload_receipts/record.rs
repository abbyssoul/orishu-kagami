//! Two sealed journal profiles share one verified IO implementation. No plugin
//! or network input can select filenames, schemas or a custom record policy.
use super::{LOCK, ReceiptStoreError, SNAPSHOT, STAGING};
use orishu::model::{
    cluster::{FormationId, OperationId},
    node::NodeId,
    run_command::*,
    run_load::{LoadOutcome, LoadReceipt, LoadRequest, LoadState},
};
use serde::{Serialize, de::DeserializeOwned};

mod sealed {
    pub trait Sealed {}
}
/// Sealed worker-local persistence policy, shared only by load and command facts.
/// Associated constant paths are compile-time owned, never supplied by callers.
pub trait ReceiptRecord: sealed::Sealed + Clone + PartialEq + Serialize + DeserializeOwned {
    /// Immutable typed intent.
    type Request: Clone + PartialEq;
    /// A terminal fact, including explicitly indeterminate application.
    type Outcome;
    /// Exact bounded snapshot schema.
    const VERSION: &'static str;
    /// Committed fd-relative file name.
    const SNAPSHOT: &'static str;
    /// Uncommitted fd-relative staging name.
    const STAGING: &'static str;
    /// Profile-specific single-writer lock.
    const LOCK: &'static str;
    /// Scientific status time uses finite CBOR floats; load/peer facts do not.
    const FINITE_FLOATS: bool;
    /// Namespace key; equality of complete intent is checked separately.
    fn key(request: &Self::Request) -> (&FormationId, &OperationId);
    /// Complete immutable request.
    fn request(&self) -> &Self::Request;
    /// Construct intent that has not yet acquired execution permission.
    fn pending(request: Self::Request, source: NodeId) -> Self;
    /// Whether the record can be finalized once in this store incarnation.
    fn is_pending(&self) -> bool;
    /// Validate an attributed terminal fact; this does not prove execution itself.
    fn finished(&self, outcome: Self::Outcome) -> Result<Self, ReceiptStoreError>;
    /// Recover unfinished intent conservatively, never as known non-application.
    fn recover(&self) -> Self;
}

impl sealed::Sealed for LoadReceipt {}
impl ReceiptRecord for LoadReceipt {
    type Request = LoadRequest;
    type Outcome = LoadOutcome;
    const VERSION: &'static str = "orishu.worker-load-receipts/v1";
    const SNAPSHOT: &'static str = SNAPSHOT;
    const STAGING: &'static str = STAGING;
    const LOCK: &'static str = LOCK;
    const FINITE_FLOATS: bool = false;
    fn key(request: &LoadRequest) -> (&FormationId, &OperationId) {
        (request.formation_id(), request.operation_id())
    }
    fn request(&self) -> &LoadRequest {
        self.request()
    }
    fn pending(request: LoadRequest, source: NodeId) -> Self {
        Self::new(request, source, LoadState::Pending).expect("pending load")
    }
    fn is_pending(&self) -> bool {
        self.state() == &LoadState::Pending
    }
    fn finished(&self, outcome: LoadOutcome) -> Result<Self, ReceiptStoreError> {
        Self::new(
            self.request().clone(),
            self.source_node_id().clone(),
            LoadState::Finished(outcome),
        )
        .map_err(|_| ReceiptStoreError::Outcome)
    }
    fn recover(&self) -> Self {
        if self.is_pending() {
            self.finished(LoadOutcome::Indeterminate)
                .expect("indeterminate load")
        } else {
            self.clone()
        }
    }
}

impl sealed::Sealed for RunCommandReceipt {}
impl ReceiptRecord for RunCommandReceipt {
    type Request = RunCommandRequest;
    type Outcome = RunCommandOutcome;
    const VERSION: &'static str = "orishu.worker-run-command-receipts/v1";
    const SNAPSHOT: &'static str = "run-command-receipts.cbor";
    const STAGING: &'static str = ".run-command-receipts.pending";
    const LOCK: &'static str = ".run-command-receipts.lock";
    const FINITE_FLOATS: bool = true;
    fn key(request: &RunCommandRequest) -> (&FormationId, &OperationId) {
        (request.run().formation_id(), request.operation_id())
    }
    fn request(&self) -> &RunCommandRequest {
        self.request()
    }
    fn pending(request: RunCommandRequest, source: NodeId) -> Self {
        Self::new(request, source, RunCommandState::Pending).expect("pending command")
    }
    fn is_pending(&self) -> bool {
        self.state() == &RunCommandState::Pending
    }
    fn finished(&self, outcome: RunCommandOutcome) -> Result<Self, ReceiptStoreError> {
        Self::new(
            self.request().clone(),
            self.source_node_id().clone(),
            RunCommandState::Finished(outcome),
        )
        .map_err(|_| ReceiptStoreError::Outcome)
    }
    fn recover(&self) -> Self {
        if self.is_pending() {
            self.finished(RunCommandOutcome::Indeterminate)
                .expect("indeterminate command")
        } else {
            self.clone()
        }
    }
}
