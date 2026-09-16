//! Daemon-owned single-admission coordinator; HTTP is an authenticated adapter.
//! No filesystem IO or JIT runs in the formation owner or a receipt read lock.
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use orishu::model::{
    run::{RunDescriptor, RunIdentity},
    run_load::{LoadOutcome, LoadReceipt, LoadRefusal, LoadRequest},
};
use orishu_runtime::{AdmissionLimits, OperationControl};
use tokio::{
    io::AsyncRead,
    sync::{oneshot, watch},
};

use crate::{
    driver::{
        run::RunHandle,
        scientific::{AdmissionError, AdmissionService, DeliveryLimits},
    },
    workload_receipts::{BeginLoad, ReceiptHistory, ReceiptStore, ReceiptStoreError},
};

/// Host-owned scientific policy; no submitter may expand these budgets.
#[derive(Clone)]
pub struct LoadPolicy {
    /// Guest/scientific admission bounds.
    pub admission: AdmissionLimits,
    /// Portable archive assembly/verification bounds.
    pub archive: orishu_plugin::archive::ArchiveLimits,
    /// Exact resource denials, checked independently of cache residency.
    pub denied: BTreeSet<orishu_workload::ArtifactDigest>,
    /// Complete-body limits, subordinate to the total operation timeout.
    pub delivery: DeliveryLimits,
    /// Total admission wall-time policy, not simulation time.
    pub timeout: Duration,
}
impl Default for LoadPolicy {
    fn default() -> Self {
        Self {
            admission: Default::default(),
            archive: Default::default(),
            denied: Default::default(),
            delivery: Default::default(),
            timeout: Duration::from_secs(60),
        }
    }
}

/// Bounded coordinator failures; scientific refusals are durable receipts.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Another new admission or retained live run owns the only slot.
    #[error("worker workload slot is busy")]
    Busy,
    /// Coordinator shutdown, missing executor or unusable lifecycle state.
    #[error("worker load coordinator is closed")]
    Closed,
    /// Invalid host timeout or no async executor for dispatch.
    #[error("worker load coordinator policy or executor is unavailable")]
    Policy,
    /// Receipt persistence/conflict/capacity failed; no success is fabricated.
    #[error(transparent)]
    Receipt(#[from] ReceiptStoreError),
    /// Detached job exited without a known final reply. Never retry with a new ID
    /// automatically; retrieve the original operation or recover its journal.
    #[error("worker load outcome is unknown")]
    OutcomeUnknown,
    /// No retained run with exactly the requested identity exists.
    #[error("requested worker run is not retained")]
    RunUnavailable,
}

/// One response receiver, not ownership of admission or its accepted run.
pub struct LoadSubmission(oneshot::Receiver<Result<LoadReceipt, LoadError>>);
impl LoadSubmission {
    /// Dropping this future/receiver cannot abort daemon-owned work.
    pub async fn outcome(self) -> Result<LoadReceipt, LoadError> {
        self.0.await.map_err(|_| LoadError::OutcomeUnknown)?
    }
}

#[derive(Default)]
struct Lifecycle {
    closed: bool,
    active: Option<OperationControl>,
    run: Option<RunHandle>,
}
struct State {
    service: AdmissionService,
    delivery: DeliveryLimits,
    timeout: Duration,
    // Taken by the single job, restored only when all its IO finishes.
    store: Mutex<Option<ReceiptStore>>,
    history: Mutex<Result<ReceiptHistory, ()>>,
    lifecycle: Mutex<Lifecycle>,
    idle: watch::Sender<bool>,
}
impl State {
    fn close(&self) {
        if let Ok(mut life) = self.lifecycle.lock() {
            life.closed = true;
            if let Some(control) = &life.active {
                control.cancel();
            }
            if let Some(run) = life.run.take() {
                run.unload();
            }
        }
    }
    fn publish_history(&self, store: &ReceiptStore) {
        if let Ok(mut history) = self.history.lock() {
            *history = store.history().map_err(|_| ());
        }
    }
    fn retain(&self, run: RunHandle) {
        match self.lifecycle.lock() {
            Ok(mut life) if !life.closed => {
                life.run = Some(run);
            }
            _ => run.unload(),
        }
    }
}
struct Lifetime(Arc<State>);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Cloneable IO handle. The daemon retains one owner independently of clients;
/// dropping the last owner cancels admission and revokes all retained-run clones.
#[derive(Clone)]
pub struct LoadCoordinator(Arc<Lifetime>);

/// The daemon's distinct owner: dropping it shuts down even when client handles
/// survive. Ordinary cloned handles do not carry this responsibility.
pub(crate) struct DaemonLoadOwner(pub(crate) LoadCoordinator);
impl Drop for DaemonLoadOwner {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}
impl LoadCoordinator {
    /// Bind an already opened journal to one admission service. This performs no
    /// IO and advertises no capability. `RunningWorker` installs/retains it once.
    pub fn new(
        service: AdmissionService,
        store: ReceiptStore,
        delivery: DeliveryLimits,
        timeout: Duration,
    ) -> Result<Self, LoadError> {
        if timeout.is_zero() || delivery.timeout.is_zero() || delivery.bytes == 0 {
            return Err(LoadError::Policy);
        }
        OperationControl::new(timeout).map_err(|_| LoadError::Policy)?;
        let history = store.history()?;
        let (idle, _) = watch::channel(true);
        Ok(Self(Arc::new(Lifetime(Arc::new(State {
            service,
            delivery,
            timeout,
            store: Mutex::new(Some(store)),
            history: Mutex::new(Ok(history)),
            lifecycle: Mutex::new(Lifecycle::default()),
            idle,
        })))))
    }
    /// Read the last published durable fact without filesystem IO or waiting for
    /// JIT. Authenticate first. A concurrent final write may still appear Pending.
    pub fn lookup(&self, request: &LoadRequest) -> Result<Option<LoadReceipt>, LoadError> {
        let history = self.0.0.history.lock().map_err(|_| LoadError::Closed)?;
        history
            .as_ref()
            .map_err(|_| ReceiptStoreError::Poisoned)?
            .lookup(request)
            .map_err(Into::into)
    }
    /// Dispatch exactly one new load, or replay history without polling the body.
    /// The reader MUST be an authenticated, body-bounded stream. The detached job
    /// retains its own budget/ticket: request cancellation cannot roll it back.
    pub fn submit<R: AsyncRead + Unpin + Send + 'static>(
        &self,
        request: LoadRequest,
        reader: R,
        announced_bytes: u64,
    ) -> Result<LoadSubmission, LoadError> {
        let (reply, receive) = oneshot::channel();
        if let Some(receipt) = self.lookup(&request)? {
            let _ = reply.send(Ok(receipt));
            return Ok(LoadSubmission(receive));
        }
        let executor = tokio::runtime::Handle::try_current().map_err(|_| LoadError::Policy)?;
        let state = &self.0.0;
        let mut life = state.lifecycle.lock().map_err(|_| LoadError::Closed)?;
        if life.closed {
            return Err(LoadError::Closed);
        }
        if life.active.is_some() {
            return Err(LoadError::Busy);
        }
        // Read history again under the admission gate: a previous job may have
        // completed between our first lookup and acquiring this mutex.
        if let Some(receipt) = self.lookup(&request)? {
            let _ = reply.send(Ok(receipt));
            return Ok(LoadSubmission(receive));
        }
        if let Some(run) = &life.run {
            if run.view().is_ok() {
                return Err(LoadError::Busy);
            }
            run.unload();
            life.run = None;
        }
        let control = OperationControl::new(state.timeout).map_err(|_| LoadError::Policy)?;
        let store = state
            .store
            .lock()
            .map_err(|_| LoadError::Closed)?
            .take()
            .ok_or(LoadError::Closed)?;
        life.active = Some(control.clone());
        state.idle.send_replace(false);
        let guard = JobGuard {
            state: Arc::clone(state),
            healthy: false,
        };
        // A shutting-down executor may discard the future at dispatch. Its
        // guard must be able to take the lifecycle lock even in that case.
        drop(life);
        executor.spawn(async move {
            let mut guard = guard;
            let result = load(
                &guard.state,
                store,
                request,
                reader,
                announced_bytes,
                control,
            )
            .await;
            let outcome = if let Ok((store, outcome)) = result {
                guard.state.publish_history(&store);
                if let Ok(mut slot) = guard.state.store.lock() {
                    *slot = Some(store);
                    guard.healthy = true;
                }
                outcome
            } else {
                Err(LoadError::OutcomeUnknown)
            };
            // Guard owns the active slot until final IO and store disposal/return.
            drop(guard);
            let _ = reply.send(outcome);
        });
        Ok(LoadSubmission(receive))
    }
    /// Discover the usable retained execution even if receipt persistence failed.
    /// This is a live host projection, not a replacement historical receipt.
    /// Authenticate the adapter before exposing it to a client.
    pub fn retained_descriptor(&self) -> Result<Option<RunDescriptor>, LoadError> {
        let life = self.0.0.lifecycle.lock().map_err(|_| LoadError::Closed)?;
        Ok(life
            .run
            .as_ref()
            .filter(|run| run.view().is_ok())
            .map(|run| run.descriptor().clone()))
    }
    /// Retrieve only the exact live execution; historical acceptance cannot
    /// silently resolve to another epoch or a newly loaded workload.
    pub fn run(&self, identity: &RunIdentity) -> Result<RunHandle, LoadError> {
        let life = self.0.0.lifecycle.lock().map_err(|_| LoadError::Closed)?;
        let run = life
            .run
            .as_ref()
            .filter(|run| run.descriptor().identity() == identity)
            .ok_or(LoadError::RunUnavailable)?;
        run.view().map_err(|_| LoadError::RunUnavailable)?;
        Ok(run.clone())
    }
    /// Revoke admission/run ownership without blocking on disk or native JIT.
    /// Late known acceptance remains a historical receipt, never a false refusal.
    pub fn shutdown(&self) {
        self.0.0.close();
    }
    /// Wait for admission and its final receipt IO to finish. Callers choose a
    /// wait deadline; this does not assert all native retained-run disposal ended.
    pub async fn wait_admission(&self) {
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
        // Poison recovery is cleanup only; public operations still fail closed
        // on the poisoned mutex. Publish idle while holding the same gate as a
        // new submission, so a prior completion cannot overwrite its busy flag.
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

async fn load<R: AsyncRead + Unpin + Send + 'static>(
    state: &State,
    store: ReceiptStore,
    request: LoadRequest,
    reader: R,
    length: u64,
    control: OperationControl,
) -> Result<(ReceiptStore, Result<LoadReceipt, LoadError>), ()> {
    // History is checked before looking at the current source/formation. Stale
    // new requests are refused by the owner without body polling, after intent.
    let source = match state.service.source_node() {
        Ok(source) => source,
        Err(_) => return Ok((store, Err(LoadError::Closed))),
    };
    let (mut store, begun) = tokio::task::spawn_blocking(move || {
        let mut store = store;
        let begun = store.begin(request, source);
        (store, begun)
    })
    .await
    .map_err(|_| ())?;
    state.publish_history(&store);
    let ticket = match begun {
        Ok(BeginLoad::Started(ticket)) => ticket,
        Ok(BeginLoad::Replay(receipt)) => return Ok((store, Ok(receipt))),
        Err(error) => return Ok((store, Err(error.into()))),
    };
    let request = ticket.receipt().request();
    let admitted = async {
        let prepared = state
            .service
            .prepare_for(request.formation_id().clone(), control.clone())
            .await?;
        let candidate = prepared
            .receive(reader, length, request.workload_id(), state.delivery)
            .await?;
        candidate.confirm().await
    }
    .await;
    let outcome = match admitted {
        Err(error) => LoadOutcome::Refused {
            reason: refusal(&error),
        },
        Ok(admitted) => match admitted.start(control).await {
            Ok(run) => {
                let descriptor = run.descriptor().clone();
                state.retain(run);
                LoadOutcome::Accepted { descriptor }
            }
            // Start's public error includes lost publication acknowledgement.
            // Never infer non-publication from that combined error surface.
            Err(_) => LoadOutcome::Indeterminate,
        },
    };
    tokio::task::spawn_blocking(move || {
        let result = store.finish(&ticket, outcome).map_err(Into::into);
        (store, result)
    })
    .await
    .map_err(|_| ())
}

fn refusal(error: &AdmissionError) -> LoadRefusal {
    match error {
        AdmissionError::Driver(_)
        | AdmissionError::Reservation(_)
        | AdmissionError::CoordinationDeadline => LoadRefusal::FormationUnavailable,
        AdmissionError::Delivery(_) => LoadRefusal::Delivery,
        AdmissionError::Limit | AdmissionError::Denied(_) => LoadRefusal::Policy,
        AdmissionError::Interrupted(_) => LoadRefusal::Cancelled,
        _ => LoadRefusal::InvalidWorkload,
    }
}
