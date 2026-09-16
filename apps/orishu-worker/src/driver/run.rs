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
    /// The one operation slot is already occupied; no step backlog is accepted.
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
    Step(OperationControl, oneshot::Sender<Result<RunView, RunError>>),
    Stop(OperationControl, oneshot::Sender<Result<RunView, RunError>>),
    Field(
        orishu_workload::ComponentInstanceId,
        oneshot::Sender<Result<FieldSnapshot, RunError>>,
    ),
}
struct Pending {
    request: Request,
    // Held until execution/reply disposal, not merely until dequeue.
    _slot: OperationSlot,
}

/// Client of one retained executor. Clones share one non-waiting operation slot.
/// Dropping all handles unloads after current work finishes; explicit `unload`
/// additionally interrupts active guest work and invalidates every clone.
#[derive(Clone)]
pub struct RunHandle {
    input: blocking::SyncSender<Pending>,
    occupied: Arc<AtomicBool>,
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
    pub async fn step(&self, control: OperationControl) -> Result<RunView, RunError> {
        let control = runtime!(control.with_parent(self.fence.cancellation()))?;
        runtime!(control.check())?;
        let (reply, receive) = oneshot::channel();
        self.submit(Request::Step(control, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
    }
    /// Terminal stop, serialized with steps. A busy run refuses; callers may
    /// cancel the active operation first. No resume or implicit reinitialization.
    pub async fn stop(&self, control: OperationControl) -> Result<RunView, RunError> {
        let control = runtime!(control.with_parent(self.fence.cancellation()))?;
        runtime!(control.check())?;
        let (reply, receive) = oneshot::channel();
        self.submit(Request::Stop(control, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
    }
    /// Acquire a bounded immutable field lease from the last accepted boundary.
    /// Sampling runs independently; observer failures cannot block future steps.
    pub async fn acquire_field(
        &self,
        instance: orishu_workload::ComponentInstanceId,
    ) -> Result<FieldSnapshot, RunError> {
        let (reply, receive) = oneshot::channel();
        self.submit(Request::Field(instance, reply))?;
        receive.await.map_err(|_| RunError::Closed)?
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
        let handle = RunHandle {
            input,
            occupied: Arc::new(AtomicBool::new(false)),
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
            execution.serve(receiver);
        });
        receive.await.map_err(|_| RunError::Closed)??;
        Ok(handle)
    }
    fn serve(&mut self, receiver: blocking::Receiver<Pending>) {
        while self.lease.fence().is_current() {
            let pending = match receiver.recv_timeout(IDLE_POLL) {
                Ok(pending) => pending,
                Err(blocking::RecvTimeoutError::Timeout) => continue,
                Err(blocking::RecvTimeoutError::Disconnected) => return,
            };
            if !self.lease.fence().is_current() {
                return;
            }
            let fence = self.lease.fence();
            let Pending { request, _slot } = pending;
            let terminal = match request {
                Request::Step(control, reply) => {
                    let result = self.step_owned(&fence, control);
                    let terminal = matches!(result, Err(RunError::Publication));
                    if terminal {
                        fence.cancellation().cancel();
                    }
                    drop(_slot);
                    let _ = reply.send(result);
                    terminal
                }
                Request::Stop(control, reply) => {
                    let result = self.stop_owned(&fence, control);
                    let terminal = matches!(result, Err(RunError::Publication));
                    if terminal {
                        fence.cancellation().cancel();
                    }
                    drop(_slot);
                    let _ = reply.send(result);
                    terminal
                }
                Request::Field(instance, reply) => {
                    let result = runtime!(self.admitted.run.acquire_field(&instance))
                        .map_err(RunError::from);
                    drop(_slot);
                    let _ = reply.send(result);
                    false
                }
            };
            if terminal {
                fence.cancellation().cancel();
                return;
            }
        }
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
