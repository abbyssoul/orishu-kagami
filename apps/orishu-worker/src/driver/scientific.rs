//! Bounded off-owner scientific admission and owner-confirmed handoff.
//!
//! No installer, initialization or public worker API lives here. The formation
//! owner allocates the descriptor after closure verification. The execution
//! adapter retains its lease through disposal; confirmation alone is not Loaded.
use super::{execution::*, *};
use orishu_plugin::{
    ArtifactDigest,
    archive::ArchiveLimits,
    workload::{WorkloadError, bundle},
};
use orishu_runtime::{
    AdmissionLimits, AdmittedWorkload, ContextRejection, Interruption, KernelRejection,
    OperationControl, RunRejection, RunScope, Sandbox,
};
use std::{collections::BTreeSet, sync::Arc};

const COORDINATION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DENIALS: usize = 4096;

mod delivery;
pub use delivery::{DeliveryError, DeliveryLimits};

/// Stable admission categories; arbitrary native/JIT/trap strings are not copied
/// to operator messages. Closure and guest-domain failures retain typed detail.
#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    /// The bounded worker owner lane rejected or lost the request.
    #[error(transparent)]
    Driver(#[from] DriverError),
    /// Formation eligibility, exclusivity or stale identity refusal.
    #[error(transparent)]
    Reservation(#[from] ExecutionError),
    /// Queue coordination exceeded its own wall-time bound.
    #[error("worker admission coordination deadline exceeded")]
    CoordinationDeadline,
    /// Input or host policy exceeded the explicit admission budget.
    #[error("worker admission byte or policy limit exceeded")]
    Limit,
    /// Bounded body delivery failed before scientific admission.
    #[error(transparent)]
    Delivery(#[from] DeliveryError),
    /// Submitted content differs from the caller's explicitly requested root.
    #[error("delivered workload root differs from requested identity")]
    UnexpectedWorkload {
        /// Root named by the identified submission.
        expected: orishu_workload::WorkloadDigest,
        /// Root independently verified from delivered bytes.
        actual: orishu_workload::WorkloadDigest,
    },
    /// Portable root/selected/scientific closure was rejected independently.
    #[error(transparent)]
    Closure(Box<WorkloadError>),
    /// A required exact artifact is denied regardless of cache residency.
    #[error("required workload resource is denied: {0}")]
    Denied(ArtifactDigest),
    /// This job or its formation lifetime was cancelled/expired.
    #[error(transparent)]
    Interrupted(Interruption),
    /// Stable guest scientific refusal.
    #[error(transparent)]
    Kernel(KernelRejection),
    /// Standard scientific invocation input mismatch.
    #[error(transparent)]
    Context(ContextRejection),
    /// Stable shared run-owner refusal.
    #[error(transparent)]
    Run(RunRejection),
    /// Component/JIT/host protocol failure, without arbitrary native text.
    #[error("worker scientific sandbox admission failed")]
    Sandbox,
    /// The off-owner job panicked or its executor exited.
    #[error("worker scientific admission executor exited")]
    ExecutorExited,
}
macro_rules! runtime {
    ($value:expr) => {
        $value.map_err(|e| {
            if let Some(r) = e.downcast_ref::<orishu_runtime::Interruption>() {
                AdmissionError::Interrupted(*r)
            } else if let Some(r) = e.downcast_ref::<orishu_runtime::KernelRejection>() {
                AdmissionError::Kernel(*r)
            } else if let Some(r) = e.downcast_ref::<orishu_runtime::ContextRejection>() {
                AdmissionError::Context(*r)
            } else if let Some(r) = e.downcast_ref::<orishu_runtime::RunRejection>() {
                AdmissionError::Run(*r)
            } else {
                AdmissionError::Sandbox
            }
        })
    };
}
pub(super) use runtime;

/// One worker's immutable admission policy and shared engine. The formation lease,
/// not a per-instance semaphore, enforces exclusivity even across service clones.
#[derive(Clone)]
pub struct AdmissionService {
    owner: Handle,
    sandbox: Arc<Sandbox>,
    limits: AdmissionLimits,
    archive: ArchiveLimits,
    denied: Arc<BTreeSet<ArtifactDigest>>,
}
impl AdmissionService {
    #[cfg(unix)]
    pub(crate) fn source_node(&self) -> Result<orishu::model::node::NodeId, DriverError> {
        Ok(self.owner.view()?.summary.source_node_id)
    }
    /// Prepare through a real worker adapter using the caller's expected formation.
    /// The process generation is read locally and rechecked by the owner; a stale
    /// request never silently follows a replacement formation. Authenticate first.
    pub async fn prepare_for(
        &self,
        expected_formation: orishu_membership::FormationId,
        control: OperationControl,
    ) -> Result<PreparedAdmission, AdmissionError> {
        let generation = self.owner.view()?.generation;
        self.prepare(generation, expected_formation, control).await
    }
    /// Bind an already initialized engine and host policy to one worker. Engine
    /// construction belongs outside the membership owner. No capabilities or
    /// HTTP routes are enabled by constructing this service.
    pub fn new(
        owner: Handle,
        sandbox: Arc<Sandbox>,
        limits: AdmissionLimits,
        archive: ArchiveLimits,
        denied: BTreeSet<ArtifactDigest>,
    ) -> Result<Self, AdmissionError> {
        if denied.len() > MAX_DENIALS {
            return Err(AdmissionError::Limit);
        }
        Ok(Self {
            owner,
            sandbox,
            limits,
            archive,
            denied: Arc::new(denied),
        })
    }
    /// Reserve before input IO/compilation. Four owner slots are required at
    /// entry: reservation request, release, epoch allocation and confirmation. No
    /// work is queued waiting for another workload to finish.
    pub async fn prepare(
        &self,
        generation: Generation,
        formation: orishu_membership::FormationId,
        control: OperationControl,
    ) -> Result<PreparedAdmission, AdmissionError> {
        runtime!(control.check())?;
        let completion = self
            .owner
            .control
            .clone()
            .try_reserve_owned()
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        let allocation_permit =
            self.owner
                .control
                .clone()
                .try_reserve_owned()
                .map_err(|e| match e {
                    mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                    mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
                })?;
        let receive = self.owner.reserve_execution(generation, formation)?;
        let lease = tokio::time::timeout(COORDINATION_TIMEOUT, receive)
            .await
            .map_err(|_| AdmissionError::CoordinationDeadline)?
            .map_err(|_| DriverError::Closed)??;
        let control = runtime!(control.with_parent(lease.fence().cancellation()))?;
        runtime!(control.check())?;
        Ok(PreparedAdmission {
            service: self.clone(),
            lease,
            completion,
            allocation_permit: Some(allocation_permit),
            control,
            #[cfg(test)]
            before_admission: None,
        })
    }
}

/// One capacity-backed pending job. Drop releases it without starting computation.
pub struct PreparedAdmission {
    service: AdmissionService,
    lease: ExecutionLease,
    completion: mpsc::OwnedPermit<Control>,
    allocation_permit: Option<mpsc::OwnedPermit<Control>>,
    control: OperationControl,
    #[cfg(test)]
    before_admission: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
}
impl PreparedAdmission {
    /// Maximum portable input bytes the IO adapter may read for this job.
    pub fn input_limit(&self) -> usize {
        self.service.archive.max_bytes
    }
    /// Read-only reservation identity/cancellation for the adapter's IO operation.
    pub fn fence(&self) -> ExecutionFence {
        self.lease.fence()
    }
    /// Verify the portable closure and admit actual Components off the Tokio
    /// owner thread. Dropping this future cancels guest work, but a native JIT
    /// already in progress still owns the lease until it actually returns.
    /// The formation owner pins a fresh run identity to this lease after complete
    /// closure verification. A submitter cannot supply or rebind execution scope.
    pub async fn validate(self, bytes: Arc<[u8]>) -> Result<AdmissionCandidate, AdmissionError> {
        self.validate_input(bytes, None).await
    }

    // Preserve owned delivery storage through validation without converting a
    // Vec to Arc<[u8]> (which would copy the whole input at peak retention).
    async fn validate_input(
        mut self,
        bytes: impl AsRef<[u8]> + Send + 'static,
        expected: Option<orishu_workload::WorkloadDigest>,
    ) -> Result<AdmissionCandidate, AdmissionError> {
        if bytes.as_ref().len() > self.input_limit() {
            return Err(AdmissionError::Limit);
        }
        runtime!(self.control.check())?;
        let mut cancellation = CancelOnDrop(Some(self.control.clone()));
        let result = tokio::task::spawn_blocking(move || {
            runtime!(self.control.check())?;
            #[cfg(test)]
            if let Some((entered, release)) = &self.before_admission {
                entered.send(()).unwrap();
                release.recv().unwrap();
                runtime!(self.control.check())?;
            }
            let portable = bundle::read(
                bytes.as_ref(),
                self.service.limits.profile,
                self.service.archive,
            )
            .map_err(|e| AdmissionError::Closure(Box::new(e)))?;
            if let Some(expected) = expected
                && portable.verified().root() != expected
            {
                return Err(AdmissionError::UnexpectedWorkload {
                    expected,
                    actual: portable.verified().root(),
                });
            }
            let root = ArtifactDigest::sha256_of(portable.manifest_bytes());
            if let Some(denied) = std::iter::once(&root)
                .chain(portable.blobs().keys())
                .find(|d| self.service.denied.contains(d))
            {
                return Err(AdmissionError::Denied(*denied));
            }
            runtime!(self.control.check())?;
            let (reply, receive) = std::sync::mpsc::sync_channel(1);
            send_reserved_control(
                self.allocation_permit.take().ok_or(DriverError::Closed)?,
                Control::AllocateRun {
                    fence: self.lease.fence(),
                    workload: portable.verified().root(),
                    control: self.control.clone(),
                    reply,
                },
            );
            let allocation = receive
                .recv_timeout(COORDINATION_TIMEOUT)
                .map_err(|error| match error {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {
                        AdmissionError::CoordinationDeadline
                    }
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        AdmissionError::Driver(DriverError::Closed)
                    }
                })??;
            runtime!(self.control.check())?;
            let admitted = runtime!(orishu_runtime::admit(
                self.service.sandbox.clone(),
                portable.manifest_bytes(),
                portable.blobs(),
                allocation.scope.clone(),
                &self.service.denied,
                self.service.limits,
                self.control.clone()
            ))?;
            runtime!(self.control.check())?;
            Ok(AdmissionCandidate {
                admitted,
                prepared: self,
                allocation,
            })
        })
        .await
        .map_err(|_| AdmissionError::ExecutorExited)?;
        cancellation.0 = None;
        result
    }
}
struct CancelOnDrop(Option<OperationControl>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.cancel();
        }
    }
}

/// Actual validated runtime, still a candidate until its owner confirms the
/// generation/lease in a serialized turn. Fields cannot be forged by adapters.
pub struct AdmissionCandidate {
    // Dispose large runtime objects before releasing the worker slot.
    admitted: AdmittedWorkload,
    prepared: PreparedAdmission,
    allocation: RunAllocation,
}
impl AdmissionCandidate {
    /// Independently verified root and owner-allocated run descriptor scope.
    pub fn scope(&self) -> &RunScope {
        self.admitted.run.scope()
    }
    /// Immutable descriptor assigned by the formation owner, not the submitter.
    pub fn descriptor(&self) -> &orishu::model::run::RunDescriptor {
        &self.allocation.descriptor
    }
    /// Confirm through pre-reserved owner capacity; no heavy bytes or runtime
    /// destructor enter the membership lane. Caller loss releases the candidate.
    pub async fn confirm(self) -> Result<ValidatedAdmission, AdmissionError> {
        runtime!(self.prepared.control.check())?;
        let PreparedAdmission {
            service,
            lease,
            completion,
            control,
            ..
        } = self.prepared;
        // Keep disposal ordering structural across refusal, timeout and future
        // cancellation too: the runtime must die before its slot is released.
        // A standalone local `lease` would drop before the partially moved self.
        let handoff = ValidatedAdmission {
            admitted: self.admitted,
            lease,
            owner: service.owner,
            allocation: self.allocation,
            #[cfg(test)]
            before_step_publish: None,
        };
        let (reply, receive) = oneshot::channel();
        send_reserved_control(
            completion,
            Control::ConfirmScientificAdmission {
                fence: handoff.lease.fence(),
                scope: handoff.admitted.run.scope().clone(),
                control,
                reply,
            },
        );
        tokio::time::timeout(COORDINATION_TIMEOUT, receive)
            .await
            .map_err(|_| AdmissionError::CoordinationDeadline)?
            .map_err(|_| DriverError::Closed)??;
        Ok(handoff)
    }
}

/// Owner-confirmed handoff to the worker's retained execution adapter via `start`.
/// Not a published Loaded resource or permission to mutate state without fencing.
pub struct ValidatedAdmission {
    pub(super) admitted: AdmittedWorkload,
    pub(super) lease: ExecutionLease,
    pub(super) owner: Handle,
    pub(super) allocation: RunAllocation,
    #[cfg(test)]
    pub(super) before_step_publish: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
}
impl ValidatedAdmission {
    /// Owner-issued immutable run descriptor, retained independently of routing.
    pub fn descriptor(&self) -> &orishu::model::run::RunDescriptor {
        &self.allocation.descriptor
    }
    /// Inspect independently validated initial state/provenance without mutation.
    pub fn admitted(&self) -> &AdmittedWorkload {
        &self.admitted
    }
    /// Inspect the original generation/identity; revocation does not erase provenance.
    pub fn fence(&self) -> ExecutionFence {
        self.lease.fence()
    }
    /// Transfer to an execution owner. Retain the lease until runtime disposal;
    /// scientific publication must be fenced atomically, not by a prior
    /// boolean read. Prefer `start` for the worker's retained execution adapter.
    pub fn into_parts(self) -> (AdmittedWorkload, ExecutionLease) {
        (self.admitted, self.lease)
    }
}

#[cfg(test)]
mod tests;
