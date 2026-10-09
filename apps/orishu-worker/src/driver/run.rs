//! Retained single-node executor and formation-serialized publication.
//! No guest work or scientific buffers enter the formation owner's mailbox.
use super::{execution::*, scientific::*, *};
use orishu_plugin::FiniteF64;
use orishu_runtime::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc as blocking,
};

const IDLE_POLL: Duration = Duration::from_millis(50);
const PUBLICATION_WAIT: Duration = Duration::from_secs(5);
const OBSERVATION_CAPACITY: usize = 8;

/// Small immutable internal projection of the last owner-accepted boundary.
/// This is not a formation-v1 wire resource, an observation frame or a checkpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct RunView {
    scope: RunScope,
    boundary: u64,
    time: FiniteF64,
    stopped: bool,
}
impl RunView {
    fn new(scope: &RunScope, state: &CommittedState, stopped: bool) -> Self {
        Self {
            scope: scope.clone(),
            boundary: state.boundary(),
            time: state.time_seconds(),
            stopped,
        }
    }
    /// Exact immutable workload/run/epoch source.
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    /// Accepted fixed simulation boundary, never an attempt number.
    pub fn boundary(&self) -> u64 {
        self.boundary
    }
    /// Accepted simulation time in SI seconds.
    pub fn time_seconds(&self) -> FiniteF64 {
        self.time
    }
    /// Terminal integration stop; observation remains available until unload.
    pub fn stopped(&self) -> bool {
        self.stopped
    }
}

/// Internal execution outcome. Public identified commands/receipts remain a
/// separate adapter; a lost reply is not evidence that a command did not commit.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The accepted boundary no longer matches the caller's explicit intent.
    /// No guest computation or publication was attempted for this request.
    #[error("worker run boundary changed (expected {expected}, actual {actual})")]
    StaleBoundary {
        /// Boundary named by the caller, never silently refreshed.
        expected: u64,
        /// Boundary observed inside the serialized executor.
        actual: u64,
    },
    /// The command slot or independent bounded observer ingress is occupied.
    /// Observer requests never acquire command capacity; no step backlog exists.
    #[error("worker run is busy")]
    Busy,
    /// The lifetime was revoked or the executor exited.
    #[error("worker run is closed")]
    Closed,
    /// The formation lane has no capacity. No computation started.
    #[error(transparent)]
    Driver(#[from] DriverError),
    /// Computation/validation refusal; the prior accepted boundary is intact.
    #[error(transparent)]
    Scientific(#[from] AdmissionError),
    /// Publication coordination was rejected or lost. The lifetime is terminated,
    /// not retried against an uncertain private/public boundary pair.
    #[error("worker run publication lost; execution lifetime terminated")]
    Publication,
}

struct OperationSlot(Arc<AtomicBool>);
impl Drop for OperationSlot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
enum Request {
    Step(
        Option<u64>,
        OperationControl,
        oneshot::Sender<Result<RunView, RunError>>,
    ),
    Stop(
        Option<u64>,
        OperationControl,
        oneshot::Sender<Result<RunView, RunError>>,
    ),
}
enum Observation {
    Field(
        orishu_workload::ComponentInstanceId,
        Option<u64>,
        oneshot::Sender<Result<FieldSnapshot, RunError>>,
    ),
    Objects(u64, oneshot::Sender<Result<ObjectSnapshot, RunError>>),
}
struct Pending {
    request: Request,
    // Held until execution/reply disposal, not merely until dequeue.
    _slot: OperationSlot,
}

/// Client of one retained executor. Clones share one non-waiting command slot
/// and an independent eight-request observer ingress. Commands have priority.
/// Dropping all handles unloads after current work finishes; explicit `unload`
/// additionally interrupts active guest work and invalidates every clone.
#[derive(Clone)]
pub struct RunHandle {
    input: blocking::SyncSender<Pending>,
    occupied: Arc<AtomicBool>,
    observations: blocking::SyncSender<Observation>,
    fence: ExecutionFence,
    owner: Handle,
    scope: RunScope,
    descriptor: orishu::model::run::RunDescriptor,
}
impl RunHandle {
    /// Owner-issued execution descriptor. Unlike a label or route, its digest is
    /// the exact source used by the admitted runtime and field observations.
    pub fn descriptor(&self) -> &orishu::model::run::RunDescriptor {
        &self.descriptor
    }
    /// Last owner-accepted metadata, without queueing behind scientific work.
    pub fn view(&self) -> Result<RunView, RunError> {
        if !self.fence.is_current() {
            return Err(RunError::Closed);
        }
        let view = self
            .owner
            .view()?
            .scientific
            .clone()
            .ok_or(RunError::Closed)?;
        if view.scope() != &self.scope || !self.fence.is_current() {
            return Err(RunError::Closed);
        }
        Ok(view)
    }
    fn submit(&self, request: Request) -> Result<(), RunError> {
        if !self.fence.is_current() {
            return Err(RunError::Closed);
        }
        self.occupied
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| RunError::Busy)?;
        let pending = Pending {
            request,
            _slot: OperationSlot(self.occupied.clone()),
        };
        self.input.try_send(pending).map_err(|e| match e {
            blocking::TrySendError::Full(_) => RunError::Busy,
            blocking::TrySendError::Disconnected(_) => RunError::Closed,
        })
    }
    /// Accept at most one explicit fixed step. Dropping the response future does
    /// not undo/cancel accepted work; use the supplied control for cancellation.
    /// This unconditional internal call is not a safe network retry primitive;
    /// identified adapters should preserve a precondition with `step_at`.
    pub async fn step(&self, control: OperationControl) -> Result<RunView, RunError> {
        self.step_expected(None, control).await
    }
    /// Advance only from the exact accepted boundary supplied by the caller.
    /// The executor checks it while holding its operation slot, before guest
    /// work or publication reservation. Adapters must not substitute a read of
    /// `view` followed by unguarded `step`: that check would race another caller.
    /// A lost reply followed by the same expected boundary cannot advance twice.
    /// A stale refusal does not prove which caller advanced the run; identified
    /// command receipts remain the adapter's responsibility.
    pub async fn step_at(
        &self,
        expected_boundary: u64,
        control: OperationControl,
    ) -> Result<RunView, RunError> {
        self.step_expected(Some(expected_boundary), control).await
    }
    async fn step_expected(
        &self,
        expected_boundary: Option<u64>,
        control: OperationControl,
    ) -> Result<RunView, RunError> {
        let control = runtime!(control.with_parent(self.fence.cancellation()))?;
        runtime!(control.check())?;
        let (reply, receive) = oneshot::channel();
        self.submit(Request::Step(expected_boundary, control, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
    }
    /// Terminal stop, serialized with steps. A busy run refuses; callers may
    /// cancel the active operation first. No resume or implicit reinitialization.
    pub async fn stop(&self, control: OperationControl) -> Result<RunView, RunError> {
        self.stop_expected(None, control).await
    }
    /// Terminal stop only at the exact accepted boundary, checked inside the
    /// same executor slot as steps. Repeating a stop at an already stopped,
    /// matching boundary is harmless. This is not the future resumable pause.
    pub async fn stop_at(
        &self,
        expected_boundary: u64,
        control: OperationControl,
    ) -> Result<RunView, RunError> {
        self.stop_expected(Some(expected_boundary), control).await
    }
    async fn stop_expected(
        &self,
        expected_boundary: Option<u64>,
        control: OperationControl,
    ) -> Result<RunView, RunError> {
        let control = runtime!(control.with_parent(self.fence.cancellation()))?;
        runtime!(control.check())?;
        let (reply, receive) = oneshot::channel();
        self.submit(Request::Stop(expected_boundary, control, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
    }
    /// Acquire a bounded immutable field lease from the last accepted boundary.
    /// Sampling runs independently; observer failures cannot block future steps.
    pub async fn acquire_field(
        &self,
        instance: orishu_workload::ComponentInstanceId,
    ) -> Result<FieldSnapshot, RunError> {
        self.field_expected(instance, None).await
    }
    /// Lease only the exact requested committed field boundary. Selection is
    /// checked inside the retained executor, never a separate racy view read.
    pub async fn acquire_field_at(
        &self,
        instance: orishu_workload::ComponentInstanceId,
        boundary: u64,
    ) -> Result<FieldSnapshot, RunError> {
        self.field_expected(instance, Some(boundary)).await
    }
    async fn field_expected(
        &self,
        instance: orishu_workload::ComponentInstanceId,
        boundary: Option<u64>,
    ) -> Result<FieldSnapshot, RunError> {
        let (reply, receive) = oneshot::channel();
        self.observe(Observation::Field(instance, boundary, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
    }
    /// Bounded immutable complete-object lease at exactly this boundary. Pending
    /// acquisition uses observer-only capacity and cannot occupy the step slot.
    pub async fn acquire_objects_at(&self, boundary: u64) -> Result<ObjectSnapshot, RunError> {
        let (reply, receive) = oneshot::channel();
        self.observe(Observation::Objects(boundary, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
    }
    fn observe(&self, request: Observation) -> Result<(), RunError> {
        if !self.fence.is_current() {
            return Err(RunError::Closed);
        }
        self.observations
            .try_send(request)
            .map_err(|error| match error {
                blocking::TrySendError::Full(_) => RunError::Busy,
                blocking::TrySendError::Disconnected(_) => RunError::Closed,
            })
    }
    /// Revoke immediately; physical runtime/lease disposal happens off-owner.
    /// Already leased immutable observations retain their original provenance.
    pub fn unload(&self) {
        self.fence.cancellation().cancel();
    }
}

impl ValidatedAdmission {
    /// Start the retained off-owner executor and publish boundary zero through
    /// the formation owner. The owner-allocated scope was admitted earlier; this
    /// does not allocate a second descriptor or enable any public workload route.
    pub async fn start(self, control: OperationControl) -> Result<RunHandle, RunError> {
        let fence = self.lease.fence();
        let control = runtime!(control.with_parent(fence.cancellation()))?;
        runtime!(control.check())?;
        let completion = reserve(&self.owner)?;
        let (input, receiver) = blocking::sync_channel(1);
        let (observations, observer_requests) = blocking::sync_channel(OBSERVATION_CAPACITY);
        let handle = RunHandle {
            input,
            occupied: Arc::new(AtomicBool::new(false)),
            observations,
            fence,
            owner: self.owner.clone(),
            scope: self.admitted.run.scope().clone(),
            descriptor: self.allocation.descriptor.clone(),
        };
        let (reply, receive) = oneshot::channel();
        tokio::task::spawn_blocking(move || {
            let mut execution = self;
            let initial = RunView::new(
                execution.admitted.run.scope(),
                execution.admitted.run.state(),
                false,
            );
            if publish(completion, &execution.lease.fence(), initial, control).is_err() {
                execution.lease.fence().cancellation().cancel();
                let _ = reply.send(Err(RunError::Publication));
                return;
            }
            if reply.send(Ok(())).is_err() {
                return;
            }
            execution.serve(receiver, observer_requests);
        });
        receive.await.map_err(|_| RunError::Closed)??;
        Ok(handle)
    }
    fn serve(
        &mut self,
        receiver: blocking::Receiver<Pending>,
        observers: blocking::Receiver<Observation>,
    ) {
        while self.lease.fence().is_current() {
            // Commands have priority over every observer acquisition. Observers
            // never own the command slot, block on capacity, run a guest here,
            // or wait for their consumer while the executor serves them.
            let pending = match receiver.try_recv() {
                Ok(pending) => pending,
                Err(blocking::TryRecvError::Disconnected) => return,
                Err(blocking::TryRecvError::Empty) => {
                    if let Ok(request) = observers.try_recv() {
                        if !self.lease.fence().is_current() {
                            return;
                        }
                        self.observe_owned(request);
                        continue;
                    }
                    match receiver.recv_timeout(IDLE_POLL) {
                        Ok(pending) => pending,
                        Err(blocking::RecvTimeoutError::Timeout) => continue,
                        Err(blocking::RecvTimeoutError::Disconnected) => return,
                    }
                }
            };
            if !self.lease.fence().is_current() {
                return;
            }
            let fence = self.lease.fence();
            let Pending { request, _slot } = pending;
            let terminal = match request {
                Request::Step(expected, control, reply) => {
                    let result = self
                        .check_boundary(expected)
                        .and_then(|()| self.step_owned(&fence, control));
                    let terminal = matches!(result, Err(RunError::Publication));
                    if terminal {
                        fence.cancellation().cancel();
                    }
                    drop(_slot);
                    let _ = reply.send(result);
                    terminal
                }
                Request::Stop(expected, control, reply) => {
                    let result = self
                        .check_boundary(expected)
                        .and_then(|()| self.stop_owned(&fence, control));
                    let terminal = matches!(result, Err(RunError::Publication));
                    if terminal {
                        fence.cancellation().cancel();
                    }
                    drop(_slot);
                    let _ = reply.send(result);
                    terminal
                }
            };
            if terminal {
                fence.cancellation().cancel();
                return;
            }
        }
    }
    fn observe_owned(&self, request: Observation) {
        match request {
            Observation::Field(instance, expected, reply) => {
                if reply.is_closed() {
                    return;
                }
                let result = self.check_boundary(expected).and_then(|()| {
                    runtime!(self.admitted.run.acquire_field(&instance)).map_err(RunError::from)
                });
                let _ = reply.send(result);
            }
            Observation::Objects(expected, reply) => {
                if reply.is_closed() {
                    return;
                }
                let result = self.check_boundary(Some(expected)).and_then(|()| {
                    runtime!(self.admitted.run.acquire_objects()).map_err(RunError::from)
                });
                let _ = reply.send(result);
            }
        }
    }
    fn check_boundary(&self, expected: Option<u64>) -> Result<(), RunError> {
        let actual = self.admitted.run.state().boundary();
        if let Some(expected) = expected
            && expected != actual
        {
            return Err(RunError::StaleBoundary { expected, actual });
        }
        Ok(())
    }
    fn step_owned(
        &mut self,
        fence: &ExecutionFence,
        control: OperationControl,
    ) -> Result<RunView, RunError> {
        let completion = reserve(&self.owner)?;
        let mut publication_failed = false;
        #[cfg(test)]
        let hook = self.before_step_publish.take();
        let result = self
            .admitted
            .run
            .advance_with_commit(control.clone(), |scope, candidate| {
                #[cfg(test)]
                if let Some((entered, release)) = hook {
                    entered.send(()).unwrap();
                    release.recv().unwrap();
                }
                if publish(
                    completion,
                    fence,
                    RunView::new(scope, candidate, false),
                    control,
                )
                .is_err()
                {
                    publication_failed = true;
                    return Err(RunRejection::Stopped.into());
                }
                Ok(())
            });
        if publication_failed {
            return Err(RunError::Publication);
        }
        runtime!(result)?;
        Ok(RunView::new(
            self.admitted.run.scope(),
            self.admitted.run.state(),
            false,
        ))
    }
    fn stop_owned(
        &mut self,
        fence: &ExecutionFence,
        control: OperationControl,
    ) -> Result<RunView, RunError> {
        runtime!(control.check())?;
        let view = RunView::new(self.admitted.run.scope(), self.admitted.run.state(), true);
        if self.admitted.run.is_stopped() {
            return Ok(view);
        }
        publish(reserve(&self.owner)?, fence, view.clone(), control)
            .map_err(|_| RunError::Publication)?;
        self.admitted.run.stop();
        Ok(view)
    }
}

fn reserve(owner: &Handle) -> Result<mpsc::OwnedPermit<Control>, DriverError> {
    owner
        .control
        .clone()
        .try_reserve_owned()
        .map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
            mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
        })
}
fn publish(
    completion: mpsc::OwnedPermit<Control>,
    fence: &ExecutionFence,
    next: RunView,
    control: OperationControl,
) -> Result<(), ()> {
    let (reply, receive) = blocking::sync_channel(1);
    send_reserved_control(
        completion,
        Control::PublishScientific {
            fence: fence.clone(),
            next,
            control,
            reply,
        },
    );
    receive
        .recv_timeout(PUBLICATION_WAIT)
        .map_err(|_| ())?
        .map_err(|_| ())
}
