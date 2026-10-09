//! Daemon-owned identified fixed-step commands. Journal IO stays off the
//! formation owner; scientific work stays in the existing retained executor.
use crate::{
    driver::{
        DriverError,
        run::{RunError, RunHandle, RunView},
        scientific::AdmissionError,
    },
    workload_load::{LoadCoordinator, LoadError},
    workload_receipts::{BeginCommand, CommandReceiptStore, JournalHistory, ReceiptStoreError},
};
use orishu::model::{run::RunIdentity, run_command::*};
use orishu_runtime::OperationControl;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{oneshot, watch};

/// Small non-payload-bearing coordination failure, distinct from a receipt.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// Another command owns the single non-queuing slot.
    #[error("run command coordinator is busy")]
    Busy,
    /// Coordinator or daemon owner is closed; new work was not dispatched.
    #[error("run command coordinator is closed")]
    Closed,
    /// Invalid host policy or no executor for detached dispatch.
    #[error("run command policy or executor is unavailable")]
    Policy,
    /// Exact execution is not currently usable; never resolved to a newer run.
    #[error("requested run is unavailable")]
    RunUnavailable,
    /// Durable history failed or the request conflicted; no success fabricated.
    #[error(transparent)]
    Receipt(#[from] ReceiptStoreError),
    /// Detached work or publication may have completed without a known outcome.
    #[error("run command outcome is unknown")]
    OutcomeUnknown,
}

/// One response, not ownership of the operation or the retained run.
pub struct CommandSubmission(oneshot::Receiver<Result<RunCommandReceipt, CommandError>>);
impl CommandSubmission {
    /// Dropping this receiver cannot cancel daemon-owned execution or receipt IO.
    pub async fn outcome(self) -> Result<RunCommandReceipt, CommandError> {
        self.0.await.map_err(|_| CommandError::OutcomeUnknown)?
    }
}
#[derive(Default)]
struct Lifecycle {
    closed: bool,
    active: Option<OperationControl>,
}
struct State {
    loads: LoadCoordinator,
    timeout: Duration,
    store: Mutex<Option<CommandReceiptStore>>,
    history: Mutex<Result<JournalHistory<RunCommandReceipt>, ()>>,
    lifecycle: Mutex<Lifecycle>,
    idle: watch::Sender<bool>,
    #[cfg(test)]
    barriers: Mutex<[Option<TestBarrier>; 2]>,
}
impl State {
    fn close(&self) {
        if let Ok(mut life) = self.lifecycle.lock() {
            life.closed = true;
            if let Some(control) = &life.active {
                control.cancel();
            }
        }
        // LoadCoordinator/its daemon owner retains the run. Closing this command
        // lane is not an implicit finish/unload or permission to erase results.
    }
    fn publish_history(&self, store: &CommandReceiptStore) {
        if let Ok(mut history) = self.history.lock() {
            *history = store.history().map_err(|_| ());
        }
    }
}
struct Lifetime(Arc<State>);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Bounded IO-side coordinator. Clones are request handles; the process retains
/// a distinct daemon owner so request loss does not control execution lifetime.
#[derive(Clone)]
pub struct RunCommandCoordinator(Arc<Lifetime>);
pub(crate) struct DaemonCommandOwner(pub(crate) RunCommandCoordinator);
impl Drop for DaemonCommandOwner {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

impl RunCommandCoordinator {
    /// Bind an already opened journal to the daemon's existing load/run authority.
    /// No IO, route enablement, plugin resolution or membership changes occur here.
    pub fn new(
        loads: LoadCoordinator,
        store: CommandReceiptStore,
        timeout: Duration,
    ) -> Result<Self, CommandError> {
        if timeout.is_zero() {
            return Err(CommandError::Policy);
        }
        OperationControl::new(timeout).map_err(|_| CommandError::Policy)?;
        let history = store.history()?;
        let (idle, _) = watch::channel(true);
        Ok(Self(Arc::new(Lifetime(Arc::new(State {
            loads,
            timeout,
            store: Mutex::new(Some(store)),
            history: Mutex::new(Ok(history)),
            lifecycle: Mutex::new(Lifecycle::default()),
            idle,
            #[cfg(test)]
            barriers: Mutex::new([None, None]),
        })))))
    }
    /// Exact durable history, even after finish, unload, shutdown or run replacement.
    /// Authenticate before exposing it. No IO or scientific operation is queued.
    pub fn lookup(
        &self,
        request: &RunCommandRequest,
    ) -> Result<Option<RunCommandReceipt>, CommandError> {
        self.0
            .0
            .history
            .lock()
            .map_err(|_| CommandError::Closed)?
            .as_ref()
            .map_err(|_| ReceiptStoreError::Poisoned)?
            .lookup(request)
            .map_err(Into::into)
    }
    /// Read owner-accepted metadata only for the exact requested live run. This
    /// remains independent of journal health and does not attribute a command.
    pub fn status(&self, identity: &RunIdentity) -> Result<RunStatus, CommandError> {
        let run = self
            .0
            .0
            .loads
            .run(identity)
            .map_err(|_| CommandError::RunUnavailable)?;
        let view = run.view().map_err(|_| CommandError::RunUnavailable)?;
        project(&run, &view)
    }
    /// Reserve one detached command. Historical replay/conflict precedes current
    /// run checks. A new command is never queued behind another operation.
    pub fn submit(&self, request: RunCommandRequest) -> Result<CommandSubmission, CommandError> {
        let (reply, receive) = oneshot::channel();
        if let Some(receipt) = self.lookup(&request)? {
            let _ = reply.send(Ok(receipt));
            return Ok(CommandSubmission(receive));
        }
        let executor = tokio::runtime::Handle::try_current().map_err(|_| CommandError::Policy)?;
        let state = &self.0.0;
        let mut life = state.lifecycle.lock().map_err(|_| CommandError::Closed)?;
        if life.closed {
            return Err(CommandError::Closed);
        }
        if life.active.is_some() {
            return Err(CommandError::Busy);
        }
        if let Some(receipt) = self.lookup(&request)? {
            let _ = reply.send(Ok(receipt));
            return Ok(CommandSubmission(receive));
        }
        let control = OperationControl::new(state.timeout).map_err(|_| CommandError::Policy)?;
        let source = state
            .loads
            .source_node()
            .map_err(|_| CommandError::Closed)?;
        let store = state
            .store
            .lock()
            .map_err(|_| CommandError::Closed)?
            .take()
            .ok_or(CommandError::Closed)?;
        life.active = Some(control.clone());
        state.idle.send_replace(false);
        let guard = JobGuard {
            state: Arc::clone(state),
            healthy: false,
        };
        drop(life);
        executor.spawn(async move {
            let mut guard = guard;
            let result = execute(&guard.state, store, request, source, control).await;
            let outcome = if let Ok((store, outcome)) = result {
                guard.state.publish_history(&store);
                if let Ok(mut slot) = guard.state.store.lock() {
                    *slot = Some(store);
                    guard.healthy = true;
                }
                outcome
            } else {
                Err(CommandError::OutcomeUnknown)
            };
            drop(guard);
            let _ = reply.send(outcome);
        });
        Ok(CommandSubmission(receive))
    }
    /// Reject new commands and cancel pending computation, not already committed
    /// outcomes. Final receipt IO continues independently of response lifetimes.
    pub fn shutdown(&self) {
        self.0.0.close();
    }
    /// Wait for the current command and receipt IO to finish. Callers choose a
    /// wait deadline; this is not a guarantee of global native runtime disposal.
    pub async fn wait_idle(&self) {
        let mut idle = self.0.0.idle.subscribe();
        let _ = idle.wait_for(|idle| *idle).await;
    }
}

struct JobGuard {
    state: Arc<State>,
    healthy: bool,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        if !self.healthy
            && let Ok(mut history) = self.state.history.lock()
        {
            *history = Err(());
        }
        let mut life = self
            .state
            .lifecycle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(control) = life.active.take() {
            control.cancel();
        }
        self.state.idle.send_replace(true);
    }
}

fn project(run: &RunHandle, view: &RunView) -> Result<RunStatus, CommandError> {
    // Neither a stale handle nor an unrelated internal projection can rebind
    // status provenance. Time remains SI simulation seconds, not wall time.
    let descriptor = run.descriptor();
    if view.scope().run
        != descriptor
            .digest()
            .map_err(|_| CommandError::OutcomeUnknown)?
        || view.scope().epoch != descriptor.identity().workload_epoch().get()
        || view.scope().workload != descriptor.identity().workload_id()
    {
        return Err(CommandError::OutcomeUnknown);
    }
    RunStatus::new(
        descriptor.clone(),
        view.boundary(),
        view.time_seconds().get(),
        if view.stopped() {
            RunPhase::Finished
        } else {
            RunPhase::Ready
        },
    )
    .map_err(|_| CommandError::OutcomeUnknown)
}
fn refused(reason: RunCommandRefusal) -> RunCommandOutcome {
    RunCommandOutcome::Refused { reason }
}
async fn apply(
    state: &State,
    request: &RunCommandRequest,
    control: OperationControl,
) -> RunCommandOutcome {
    if control.check().is_err() {
        return refused(RunCommandRefusal::Cancelled);
    }
    let run = match state.loads.run(request.run()) {
        Ok(run) => run,
        Err(LoadError::RunUnavailable | LoadError::Closed) => {
            return refused(RunCommandRefusal::RunUnavailable);
        }
        Err(_) => return RunCommandOutcome::Indeterminate,
    };
    let result = match request.command() {
        RunCommand::Step => run.step_at(request.expected_boundary(), control).await,
        RunCommand::Finish => run.stop_at(request.expected_boundary(), control).await,
    };
    match result {
        Ok(view) => match project(&run, &view) {
            Ok(status) => RunCommandOutcome::Applied { status },
            Err(_) => RunCommandOutcome::Indeterminate,
        },
        Err(error) => rejected(error),
    }
}
fn rejected(error: RunError) -> RunCommandOutcome {
    match error {
        RunError::Busy => refused(RunCommandRefusal::Busy),
        RunError::StaleBoundary { actual, .. } => {
            refused(RunCommandRefusal::StaleBoundary { actual })
        }
        RunError::Driver(DriverError::Closed) => refused(RunCommandRefusal::RunUnavailable),
        RunError::Driver(_) => refused(RunCommandRefusal::Capacity),
        RunError::Scientific(AdmissionError::Interrupted(_)) => {
            refused(RunCommandRefusal::Cancelled)
        }
        RunError::Scientific(_) => refused(RunCommandRefusal::Scientific),
        // Closed includes losing the oneshot after potential publication. A
        // current-status read cannot reconstruct attribution; preserve uncertainty.
        RunError::Closed | RunError::Publication => RunCommandOutcome::Indeterminate,
    }
}
async fn execute(
    state: &State,
    store: CommandReceiptStore,
    request: RunCommandRequest,
    source: orishu::model::node::NodeId,
    control: OperationControl,
) -> Result<(CommandReceiptStore, Result<RunCommandReceipt, CommandError>), ()> {
    let (mut store, begun) = tokio::task::spawn_blocking(move || {
        let mut store = store;
        let begun = store.begin(request, source);
        (store, begun)
    })
    .await
    .map_err(|_| ())?;
    state.publish_history(&store);
    let ticket = match begun {
        Ok(BeginCommand::Started(ticket)) => ticket,
        Ok(BeginCommand::Replay(receipt)) => return Ok((store, Ok(receipt))),
        Err(error) => return Ok((store, Err(error.into()))),
    };
    #[cfg(test)]
    state.barrier(0).await;
    let mut outcome = apply(state, ticket.receipt().request(), control).await;
    // Treat an impossible attribution as uncertainty, never leave healthy Pending
    // forever or report a false known refusal after potentially committed work.
    if RunCommandReceipt::new(
        ticket.receipt().request().clone(),
        ticket.receipt().source_node_id().clone(),
        RunCommandState::Finished(outcome.clone()),
    )
    .is_err()
    {
        outcome = RunCommandOutcome::Indeterminate;
    }
    #[cfg(test)]
    state.barrier(1).await;
    tokio::task::spawn_blocking(move || {
        let result = store.finish(&ticket, outcome).map_err(Into::into);
        (store, result)
    })
    .await
    .map_err(|_| ())
}

#[cfg(test)]
type TestBarrier = (oneshot::Sender<()>, oneshot::Receiver<()>);
#[cfg(test)]
impl State {
    async fn barrier(&self, stage: usize) {
        let barrier = self.barriers.lock().unwrap()[stage].take();
        if let Some((entered, release)) = barrier {
            entered.send(()).expect("test waiter present");
            release.await.expect("test barrier released");
        }
    }
}
#[cfg(test)]
impl RunCommandCoordinator {
    pub(crate) fn hold(
        &self,
        after_execution: bool,
    ) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered, observed) = oneshot::channel();
        let (release, held) = oneshot::channel();
        let index = usize::from(after_execution);
        assert!(
            self.0.0.barriers.lock().unwrap()[index]
                .replace((entered, held))
                .is_none()
        );
        (observed, release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_executor_errors_never_become_known_refusals() {
        for error in [RunError::Closed, RunError::Publication] {
            assert_eq!(rejected(error), RunCommandOutcome::Indeterminate);
        }
        assert_eq!(rejected(RunError::Busy), refused(RunCommandRefusal::Busy));
        assert_eq!(
            rejected(RunError::StaleBoundary {
                expected: 1,
                actual: 2
            }),
            refused(RunCommandRefusal::StaleBoundary { actual: 2 })
        );
        assert_eq!(
            rejected(RunError::Driver(DriverError::Closed)),
            refused(RunCommandRefusal::RunUnavailable)
        );
        assert_eq!(
            rejected(RunError::Driver(DriverError::Overloaded)),
            refused(RunCommandRefusal::Capacity)
        );
    }
}
