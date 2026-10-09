//! Window-owned remote run projection. Orishu alone advances scientific state.
//! One off-window job; detached reads cannot reattach. Submissions and commands
//! are recorded durably before they are sent, so a restart restores them for
//! explicit reconciliation. No document/guest access.
use intents::{Ledger, Source, Target};
use orishu::{
    client::{
        ClusterAddress, credential_file,
        http_client::{HttClientOptions, HttpClusterClient, ScientificError},
    },
    model::{run::RunDescriptor, run_command::*, run_load::*, run_observation::*},
};
use std::{path::PathBuf, sync::Arc, thread::JoinHandle};
pub mod fields;
pub mod geometry;
pub mod intents;
#[cfg(unix)]
pub mod journal;
mod recovery;
pub use recovery::Recovery;

/// Process-local connection inputs, never persisted in experiment/run metadata.
#[derive(Clone, Debug)]
pub struct Connection {
    pub address: ClusterAddress,
    pub token_file: PathBuf,
    pub ca_cert: Option<PathBuf>,
}
/// User intents and completion polling; no numerical buffers in UI messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Close,
    Discover,
    Observe,
    Refresh,
    Step,
    Finish,
    Reconcile,
    ResubmitOriginal,
    ReconcileLoad,
    ResubmitLoad,
    InspectLoaded,
    ClearLoad,
    NumericView(bool),
    Field(fields::Action),
    Poll,
}

enum Job {
    Field(fields::Job),
    Geometry {
        samples: Arc<Vec<geometry::Sample>>,
        total: usize,
        scale: kagami_session::SceneScale,
    },
    Discover,
    Status(RunDescriptor),
    /// `begin` records new intent before any byte is sent.
    Load {
        request: LoadRequest,
        upload: Upload,
        begin: Option<(Target, Source)>,
    },
    ClearLoad,
    Observe {
        status: RunStatus,
        refresh: bool,
        attach: bool,
        scale: kagami_session::SceneScale,
    },
    Command {
        request: RunCommandRequest,
        lookup: bool,
        begin: Option<Target>,
    },
}
/// What a load job sends: nothing (receipt lookup), retained bytes, or the
/// journal's verified bytes after a restart.
enum Upload {
    Lookup,
    Send(Arc<Vec<u8>>),
    Stored,
}
enum Reply {
    Field(fields::Reply),
    Geometry {
        samples: Arc<Vec<geometry::Sample>>,
        geometry: geometry::Geometry,
    },
    Discovered(Option<RunStatus>),
    Load(Option<LoadReceipt>),
    Observed {
        status: RunStatus,
        projection: Projection,
        attach: bool,
    },
    Command(Option<RunCommandReceipt>),
    Cleared,
}
/// A job's reply, plus the durable ledger after any journal write. The window
/// mirrors that ledger, so it never shows intent that is not recorded.
struct Outcome {
    reply: Result<Reply, String>,
    ledger: Option<(Ledger, Option<String>)>,
    /// A reply that arrived but could not be recorded.
    unrecorded: Option<String>,
}
struct Pending {
    generation: u64,
    field_generation: Option<u64>,
    /// Adopt even after detach: the job changed durable intent.
    records: bool,
    work: JoinHandle<Outcome>,
}

/// Bounded numeric table cached off-window; rendering never rehashes a payload.
pub struct Projection {
    objects: ObservedObjects,
    pub source: orishu_plugin::execution::SnapshotSource,
    pub count: usize,
    pub force_boundary: Option<u64>,
    pub rows: Vec<NumericRow>,
    samples: Arc<Vec<geometry::Sample>>,
    pub geometry: geometry::Geometry,
}
pub struct NumericRow {
    pub object: orishu_plugin::execution::ObjectState,
    pub force: Option<orishu_plugin::execution::Force>,
}

/// Exact recorded upload intent, kept across reply loss, draft changes and
/// restarts. The server's receipt, not current-run discovery, proves attribution.
pub struct Submission {
    pub target: Target,
    pub request: LoadRequest,
    pub source_context: uuid::Uuid,
    pub source_revision: u64,
    pub receipt: Option<LoadReceipt>,
    /// Bytes retained in this session. After a restart the journal has them.
    bundle: Option<Arc<Vec<u8>>>,
}
impl Submission {
    pub fn unresolved(&self) -> bool {
        !matches!(
            self.receipt.as_ref().map(LoadReceipt::state),
            Some(LoadState::Finished(
                LoadOutcome::Accepted { .. } | LoadOutcome::Refused { .. }
            ))
        )
    }
    pub fn accepted(&self) -> Option<&RunDescriptor> {
        match self.receipt.as_ref()?.state() {
            LoadState::Finished(LoadOutcome::Accepted { descriptor }) => Some(descriptor),
            _ => None,
        }
    }
}
impl Projection {
    fn new(objects: ObservedObjects, scale: kagami_session::SceneScale) -> Self {
        let view = objects.view();
        let count = view.objects().len();
        let samples: Vec<_> = (0..count.min(kagami_renderer::MAX_MARKERS))
            .map(|i| {
                let object = view.objects().get(i).expect("validated object");
                geometry::Sample {
                    position_metres: object.kinematics.position_metres.map(|v| v.get()),
                    dynamic: object.inertial_mass_kilograms.is_some(),
                }
            })
            .collect();
        let geometry = geometry::project(&samples, count, scale);
        let mut dynamic = 0;
        let rows = (0..count.min(100))
            .map(|i| {
                let object = view.objects().get(i).expect("validated record");
                let force = if object.inertial_mass_kilograms.is_some() {
                    let force = view.forces().and_then(|forces| forces.get(dynamic));
                    dynamic += 1;
                    force
                } else {
                    None
                };
                NumericRow { object, force }
            })
            .collect();
        let source = view.source().clone();
        let force_boundary = view.force_evaluation_boundary();
        Self {
            objects,
            source,
            count,
            force_boundary,
            rows,
            samples: Arc::new(samples),
            geometry,
        }
    }
}

/// One bounded projection, independent of the editable experiment. `RunLabel`
/// remains a mode/display key only: every network call uses the full typed run.
pub struct Controller {
    connection: Option<Connection>,
    recovery: Recovery,
    /// Why submissions and commands are disabled; `None` when recording works.
    blocked: Option<String>,
    pending: Option<Pending>,
    generation: u64,
    selected: Option<RunStatus>,
    objects: Option<Projection>,
    attached: bool,
    intent: Option<(Target, RunCommandRequest)>,
    receipt: Option<RunCommandReceipt>,
    submission: Option<Submission>,
    /// Recorded intent addressed to another worker than this session's.
    load_elsewhere: Option<String>,
    command_elsewhere: Option<String>,
    pub open: bool,
    pub notice: String,
    pub numeric_view: bool,
    desired_scale: kagami_session::SceneScale,
    pub fields: fields::Inspector,
}

const NO_ACCESS: &str = "Run access is off. Start Kagami with --operator-token-file.";
const PENDING: &str = "A run request is in progress. Wait until it completes.";
const DETACH_FIRST: &str = "Return to the authoring document first.";
const COMMAND_FIRST: &str = "Reconcile the recorded run command first.";
const LOAD_FIRST: &str = "Reconcile the recorded submission first.";

impl Controller {
    /// Restores the durable intent of `recovery`. Nothing is sent until the
    /// user acts.
    pub fn new(connection: Option<Connection>, recovery: Recovery) -> Self {
        let mut controller = Self {
            connection,
            blocked: None,
            pending: None,
            generation: 0,
            selected: None,
            objects: None,
            attached: false,
            intent: None,
            receipt: None,
            submission: None,
            load_elsewhere: None,
            command_elsewhere: None,
            open: false,
            numeric_view: false,
            desired_scale: kagami_session::SceneScale::METRE,
            fields: fields::Inspector::default(),
            notice:
                "Inspect an externally submitted run. The open document is not its initial scene."
                    .into(),
            recovery,
        };
        let (ledger, blocked) = controller.recovery.snapshot();
        controller.blocked = blocked;
        controller.adopt(&ledger);
        let unresolved =
            ledger.command().is_some() || ledger.load().is_some_and(|load| !load.is_final());
        if unresolved {
            controller.notice = "Recovered unresolved run operations from an earlier session. Nothing was sent automatically; reconcile them explicitly.".into();
        } else if ledger.load().is_some() {
            controller.notice =
                "Restored the final submission history from an earlier session.".into();
        }
        controller
    }
    pub fn configured(&self) -> bool {
        self.connection.is_some()
    }
    /// Why submissions and run commands are disabled, if they are.
    pub fn recording_blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    /// Reproject only on a changed scale; camera motion never rehashes, decodes
    /// or reallocates the full snapshot. Old-scale geometry is hidden meanwhile.
    pub fn synchronize_scale(&mut self, scale: kagami_session::SceneScale) {
        self.desired_scale = scale;
        if self.attached
            && !self.is_pending()
            && let Some(frame) = &self.objects
            && frame.geometry.scale != scale
        {
            self.start(Job::Geometry {
                samples: frame.samples.clone(),
                total: frame.count,
                scale,
            });
        }
        if self.attached
            && !self.is_pending()
            && self.intent.is_none()
            && let Some(job) = self.fields.reproject(scale)
        {
            self.start(Job::Field(job));
        }
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn attached(&self) -> bool {
        self.attached
    }
    pub fn status(&self) -> Option<&RunStatus> {
        self.selected.as_ref()
    }
    pub fn objects(&self) -> Option<&Projection> {
        self.objects.as_ref()
    }
    pub fn command(&self) -> Option<&RunCommandRequest> {
        self.intent.as_ref().map(|(_, request)| request)
    }
    pub fn receipt(&self) -> Option<&RunCommandReceipt> {
        self.receipt.as_ref()
    }
    pub fn submission(&self) -> Option<&Submission> {
        self.submission.as_ref()
    }
    pub fn can_submit(&self) -> bool {
        self.check_submit().is_ok()
    }
    pub fn source_revision(&self) -> Option<u64> {
        let submission = self.submission.as_ref()?;
        (submission.accepted()? == self.selected.as_ref()?.descriptor())
            .then_some(submission.source_revision)
    }
    pub fn can_control(&self) -> bool {
        self.check(&Action::Step).is_ok()
    }

    /// Why a new submission is unavailable. The view shows the reason on the
    /// disabled action; `submit` refuses with the same reason.
    pub fn check_submit(&self) -> Result<(), &str> {
        self.ready()?;
        self.recording()?;
        if self.attached {
            return Err(DETACH_FIRST);
        }
        if self.intent.is_some() {
            return Err(COMMAND_FIRST);
        }
        match &self.submission {
            Some(s) if s.unresolved() => Err(LOAD_FIRST),
            Some(_) => {
                Err("Clear the final submission history before you submit another workload.")
            }
            None => Ok(()),
        }
    }
    /// Why `action` is unavailable. The view shows the reason on the disabled
    /// action; `act` refuses with the same reason.
    pub fn check(&self, action: &Action) -> Result<(), &str> {
        if matches!(
            action,
            Action::Open | Action::Close | Action::Poll | Action::NumericView(_) | Action::Field(_)
        ) {
            return Ok(());
        }
        self.ready()?;
        let unresolved_load = self.submission.as_ref().is_some_and(Submission::unresolved);
        match action {
            Action::Discover | Action::Observe | Action::InspectLoaded => {
                if self.attached {
                    return Err(DETACH_FIRST);
                }
                if self.intent.is_some() {
                    return Err(COMMAND_FIRST);
                }
                if unresolved_load {
                    return Err(LOAD_FIRST);
                }
                match action {
                    Action::Observe if self.selected.is_none() => {
                        Err("Inspect a retained run first.")
                    }
                    Action::InspectLoaded
                        if self
                            .submission
                            .as_ref()
                            .and_then(Submission::accepted)
                            .is_none() =>
                    {
                        Err("The submission was not accepted.")
                    }
                    _ => Ok(()),
                }
            }
            Action::Refresh if !self.attached => Err("Observe a run first."),
            Action::Refresh => Ok(()),
            Action::Step | Action::Finish => {
                self.recording()?;
                if !self.attached {
                    return Err("Observe a run first.");
                }
                if self.intent.is_some() {
                    return Err(COMMAND_FIRST);
                }
                if self.objects.is_none() {
                    return Err("Refresh committed values first.");
                }
                if self.selected.as_ref().map(RunStatus::phase) != Some(RunPhase::Ready) {
                    return Err("The run does not accept commands in its current phase.");
                }
                Ok(())
            }
            Action::Reconcile | Action::ResubmitOriginal => {
                self.recording()?;
                if self.intent.is_none() {
                    return Err("No run command is recorded.");
                }
                self.command_elsewhere.as_deref().map_or(Ok(()), Err)
            }
            Action::ReconcileLoad | Action::ResubmitLoad => {
                self.recording()?;
                if !unresolved_load {
                    return Err("No unresolved submission is recorded.");
                }
                self.load_elsewhere.as_deref().map_or(Ok(()), Err)
            }
            Action::ClearLoad => {
                self.recording()?;
                if self.attached {
                    return Err(DETACH_FIRST);
                }
                match &self.submission {
                    None => Err("No submission is recorded."),
                    Some(s) if s.unresolved() => {
                        Err("Only a final submission can be cleared. Reconcile it first.")
                    }
                    Some(_) => Ok(()),
                }
            }
            Action::Open
            | Action::Close
            | Action::Poll
            | Action::NumericView(_)
            | Action::Field(_) => unreachable!("handled above"),
        }
    }
    fn ready(&self) -> Result<(), &str> {
        if !self.configured() {
            return Err(NO_ACCESS);
        }
        if self.is_pending() {
            return Err(PENDING);
        }
        Ok(())
    }
    fn recording(&self) -> Result<(), &str> {
        self.blocked.as_deref().map_or(Ok(()), Err)
    }

    /// Mirror the durable ledger. Bytes retained in this session stay in
    /// memory for the same request; restored intent reads them from the journal.
    fn adopt(&mut self, ledger: &Ledger) {
        let elsewhere = |target: &Target| {
            let address = &self.connection.as_ref()?.address;
            (target.address() != address).then(|| {
                format!(
                    "This operation was sent to worker {}, but this session uses worker {address}. Restart Kagami with --host {} to reconcile it.",
                    target.address(),
                    target.address()
                )
            })
        };
        self.load_elsewhere = ledger.load().and_then(|intent| elsewhere(intent.target()));
        self.command_elsewhere = ledger
            .command()
            .and_then(|intent| elsewhere(intent.target()));
        let retained = self.submission.take();
        self.submission = ledger.load().map(|intent| Submission {
            target: intent.target().clone(),
            request: intent.request().clone(),
            source_context: intent.source().incarnation,
            source_revision: intent.source().revision,
            receipt: intent.receipt().cloned(),
            bundle: retained
                .filter(|s| &s.request == intent.request())
                .and_then(|s| s.bundle),
        });
        self.intent = ledger
            .command()
            .map(|intent| (intent.target().clone(), intent.request().clone()));
    }

    #[cfg(unix)]
    pub fn submit(
        &mut self,
        workload: &crate::workload_preparation::FrozenWorkload,
        formation: orishu::model::cluster::FormationId,
    ) {
        if let Err(reason) = self.check_submit() {
            self.notice = reason.into();
            return;
        }
        let address = self.connection.as_ref().expect("checked").address.clone();
        let target = match Target::new(address, formation.clone()) {
            Ok(target) => target,
            Err(error) => {
                self.notice = error.to_string();
                return;
            }
        };
        let request = LoadRequest::new(
            format!("kagami-load-{}", uuid::Uuid::new_v4())
                .parse()
                .expect("bounded ID"),
            formation,
            workload.report.workload,
        );
        let source = Source {
            incarnation: workload.source_context,
            revision: workload.source_revision,
        };
        self.selected = None;
        self.receipt = None;
        self.objects = None;
        // Shown while the job records and sends it; the ledger replaces it.
        self.submission = Some(Submission {
            target: target.clone(),
            request: request.clone(),
            source_context: workload.source_context,
            source_revision: workload.source_revision,
            receipt: None,
            bundle: Some(workload.bytes.clone()),
        });
        self.start(Job::Load {
            request,
            upload: Upload::Send(workload.bytes.clone()),
            begin: Some((target, source)),
        });
    }
    pub fn act(&mut self, action: Action) {
        if let Action::Field(action) = action {
            let status = (self.attached && self.intent.is_none())
                .then_some(self.selected.as_ref())
                .flatten();
            match self
                .fields
                .act(action, status, self.pending.is_none(), self.desired_scale)
            {
                Ok(Some(job)) => self.start(Job::Field(job)),
                Ok(None) => {
                    self.notice =
                        "Field query settings changed locally; no scientific state changed.".into()
                }
                Err(reason) => self.notice = reason.into(),
            }
            return;
        }
        if let Err(reason) = self.check(&action) {
            self.notice = reason.into();
            return;
        }
        let job = match action {
            Action::NumericView(value) => {
                self.numeric_view = value;
                return;
            }
            Action::Open => {
                self.open = true;
                return;
            }
            Action::Close => {
                self.open = false;
                if !self.attached && self.pending.as_ref().is_some_and(|p| !p.records) {
                    self.detach();
                }
                return;
            }
            Action::Poll | Action::Field(_) => return,
            Action::ClearLoad => Job::ClearLoad,
            Action::ReconcileLoad | Action::ResubmitLoad => {
                let s = self.submission.as_ref().expect("checked");
                Job::Load {
                    request: s.request.clone(),
                    upload: match (&action, &s.bundle) {
                        (Action::ReconcileLoad, _) => Upload::Lookup,
                        (_, Some(bytes)) => Upload::Send(bytes.clone()),
                        (_, None) => Upload::Stored,
                    },
                    begin: None,
                }
            }
            Action::InspectLoaded => Job::Status(
                self.submission
                    .as_ref()
                    .and_then(Submission::accepted)
                    .expect("checked")
                    .clone(),
            ),
            Action::Discover => Job::Discover,
            Action::Observe => Job::Observe {
                scale: self.desired_scale,
                status: self.selected.clone().expect("checked"),
                refresh: false,
                attach: true,
            },
            Action::Refresh => Job::Observe {
                scale: self.desired_scale,
                status: self.selected.clone().expect("attached run is selected"),
                refresh: true,
                attach: false,
            },
            Action::Step | Action::Finish => {
                let status = self.selected.as_ref().expect("checked");
                let request = RunCommandRequest::new(
                    format!("kagami-{}", uuid::Uuid::new_v4())
                        .parse()
                        .expect("bounded UUID"),
                    status.descriptor().identity().clone(),
                    status.boundary(),
                    if action == Action::Step {
                        RunCommand::Step
                    } else {
                        RunCommand::Finish
                    },
                );
                let Ok(request) = request else {
                    self.notice = "Command preconditions are invalid.".into();
                    return;
                };
                let address = self.connection.as_ref().expect("checked").address.clone();
                let target = match Target::new(address, request.run().formation_id().clone()) {
                    Ok(target) => target,
                    Err(error) => {
                        self.notice = error.to_string();
                        return;
                    }
                };
                self.receipt = None;
                Job::Command {
                    request,
                    lookup: false,
                    begin: Some(target),
                }
            }
            Action::Reconcile | Action::ResubmitOriginal => Job::Command {
                request: self.command().expect("checked").clone(),
                lookup: action == Action::Reconcile,
                begin: None,
            },
        };
        self.start(job);
    }
    fn start(&mut self, job: Job) {
        let Some(connection) = self.connection.clone() else {
            self.notice = NO_ACCESS.into();
            return;
        };
        let records = matches!(job, Job::Command { .. } | Job::Load { .. } | Job::ClearLoad);
        let field_generation = matches!(job, Job::Field(_)).then(|| self.fields.generation());
        if !matches!(job, Job::Field(_) | Job::Geometry { .. }) {
            self.fields.clear();
        }
        let recovery = self.recovery.clone();
        match std::thread::Builder::new()
            .name("kagami-run".into())
            .spawn(move || run_job(connection, job, recovery, records))
        {
            Ok(work) => {
                self.pending = Some(Pending {
                    generation: self.generation,
                    field_generation,
                    records,
                    work,
                });
                self.notice = "Request pending. Displayed values remain an identified snapshot, not a live stream.".into();
            }
            Err(_) => {
                // Nothing was recorded or sent: the journal write is on the job.
                let (ledger, blocked) = self.recovery.snapshot();
                self.blocked = blocked;
                self.adopt(&ledger);
                self.notice = "Cannot start the run request. Nothing was sent.".into();
            }
        }
    }
    /// Detach is local and never sends finish/stop. Keep capacity until any old
    /// job exits, and keep recorded command intent for reconciliation.
    pub fn detach(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("session generation exhausted");
        self.attached = false;
        self.objects = None;
        self.fields.clear();
        self.notice =
            "Detached. The worker run is unchanged; the open authoring document is restored."
                .into();
    }
    /// Adopt only a completed bounded reply. Returns a display/mode key on first
    /// attachment; the shell must also enter the existing session mode gate.
    pub fn poll(&mut self) -> Option<kagami_session::RunLabel> {
        if !self.pending.as_ref().is_some_and(|p| p.work.is_finished()) {
            return None;
        }
        let pending = self.pending.take().expect("finished job");
        let outcome = pending.work.join().unwrap_or_else(|_| Outcome {
            reply: Err("Run request thread failed. Reconcile the recorded operation.".into()),
            ledger: None,
            unrecorded: None,
        });
        // Durable intent is adopted even after detach: it is not a view.
        if let Some((ledger, blocked)) = &outcome.ledger {
            self.blocked = blocked.clone();
            self.adopt(ledger);
        }
        if pending.generation != self.generation && !pending.records {
            return None;
        }
        if pending
            .field_generation
            .is_some_and(|generation| generation != self.fields.generation())
        {
            return None;
        }
        let unrecorded = outcome.unrecorded.map(|reason| {
            format!(" The reply could not be recorded: {reason} Reconcile again later; the worker keeps its receipt.")
        });
        match outcome.reply {
            Ok(Reply::Field(reply)) => {
                self.fields.adopt(reply);
                self.notice = "Exact-boundary field read. Sampling does not advance the run; descriptor reads do not pin later queries. Refresh/inspect again explicitly after advancement.".into();
            }
            Ok(Reply::Geometry { samples, geometry }) => {
                if let Some(frame) = &mut self.objects
                    && Arc::ptr_eq(&frame.samples, &samples)
                {
                    frame.geometry = geometry;
                    self.notice = "Position markers reprojected locally; committed scientific state is unchanged.".into();
                }
            }
            Err(message) => {
                self.notice = message;
            }
            Ok(Reply::Discovered(status)) => {
                self.selected = status;
                self.notice = if self.selected.is_some() {
                    "Retained run inspected. Observe explicitly to leave authoring."
                } else {
                    "No live retained run. Historical receipts do not restore one."
                }
                .into();
            }
            Ok(Reply::Cleared) => {
                self.notice =
                    "Final submission history cleared locally. No worker state changed.".into();
            }
            Ok(Reply::Load(receipt)) => {
                self.notice = match receipt.as_ref().map(LoadReceipt::state) {
                    Some(LoadState::Finished(LoadOutcome::Accepted { .. })) => "Load accepted historically. Inspect this exact accepted run, then observe explicitly; acceptance alone is not current availability.",
                    Some(LoadState::Finished(LoadOutcome::Refused { .. })) => "Load refused. No initial state was published by this operation.",
                    Some(LoadState::Finished(LoadOutcome::Indeterminate)) => "Load outcome indeterminate. Original frozen bytes and intent retained; no new submission allowed.",
                    Some(LoadState::Pending) => "Load pending. Reconcile the original submission explicitly.",
                    None => "No recorded load receipt. Original frozen bytes and intent retained; no automatic retry.",
                }.to_owned() + unrecorded.as_deref().unwrap_or_default();
            }
            Ok(Reply::Observed {
                status,
                projection,
                attach,
            }) => {
                let label = status
                    .descriptor()
                    .digest()
                    .expect("validated descriptor")
                    .to_string();
                self.selected = Some(status);
                self.objects = Some(projection);
                self.attached = true;
                self.notice = "Exact committed snapshot. Refresh explicitly for later boundaries; markers show positions, not physical object sizes.".into();
                if attach {
                    return Some(kagami_session::RunLabel::new(label).expect("bounded digest"));
                }
            }
            Ok(Reply::Command(receipt)) => {
                self.objects = None;
                self.notice = match receipt.as_ref().map(RunCommandReceipt::state) {
                    Some(RunCommandState::Finished(RunCommandOutcome::Applied { status })) => {
                        self.selected = Some(status.clone());
                        "Command applied. Refresh committed values; the receipt is historical, not an observation."
                    },
                    Some(RunCommandState::Finished(RunCommandOutcome::Refused { .. })) => "Command refused. Refresh before another command.",
                    Some(RunCommandState::Finished(RunCommandOutcome::Indeterminate)) => "Command outcome indeterminate. Original intent retained; no fresh command will be issued.",
                    Some(RunCommandState::Pending) => "Command pending. Reconcile this same operation explicitly.",
                    None => "No recorded receipt for the original intent. It remains unresolved; no automatic resubmission.",
                }.to_owned() + unrecorded.as_deref().unwrap_or_default();
                self.receipt = receipt;
            }
        }
        None
    }
}

/// Record new intent, send, then record the validated reply. A failure to
/// record new intent sends nothing. A transport failure leaves the recorded
/// intent unresolved for explicit reconciliation.
fn run_job(connection: Connection, job: Job, recovery: Recovery, records: bool) -> Outcome {
    let not_sent = |reason: String| format!("Nothing was sent. {reason}");
    let reply = (|| {
        let job = match job {
            Job::ClearLoad => {
                recovery.clear_load()?;
                return Ok(Reply::Cleared);
            }
            Job::Load {
                request,
                upload,
                begin,
            } => {
                if let Some((target, source)) = begin {
                    let Upload::Send(bytes) = &upload else {
                        unreachable!("new uploads send their bytes")
                    };
                    recovery
                        .begin_load(target, request.clone(), source, bytes)
                        .map_err(not_sent)?;
                }
                let upload = match upload {
                    Upload::Stored => Upload::Send(Arc::new(
                        recovery
                            .stored_bundle(request.workload_id())
                            .map_err(not_sent)?,
                    )),
                    upload => upload,
                };
                Job::Load {
                    request,
                    upload,
                    begin: None,
                }
            }
            Job::Command {
                request,
                lookup,
                begin: Some(target),
            } => {
                recovery
                    .begin_command(target, request.clone())
                    .map_err(not_sent)?;
                Job::Command {
                    request,
                    lookup,
                    begin: None,
                }
            }
            job => job,
        };
        perform(connection, job)
    })();
    let unrecorded = match &reply {
        Ok(Reply::Load(receipt)) => recovery.record_load(receipt.clone()).err(),
        Ok(Reply::Command(receipt)) => recovery.record_command(receipt.clone()).err(),
        _ => None,
    };
    Outcome {
        reply,
        ledger: records.then(|| recovery.snapshot()),
        unrecorded,
    }
}

fn perform(connection: Connection, job: Job) -> Result<Reply, String> {
    let job = match job {
        Job::Field(fields::Job::Vectors(request)) => {
            return fields::vectors::project(request)
                .map(|v| Reply::Field(fields::Reply::Vectors(Box::new(v))))
                .map_err(str::to_owned);
        }
        Job::Geometry {
            samples,
            total,
            scale,
        } => {
            let geometry = geometry::project(&samples, total, scale);
            return Ok(Reply::Geometry { samples, geometry });
        }
        other => other,
    };
    let credential = credential_file::load(&connection.token_file)
        .map_err(|_| "Cannot read a secure operator credential file.".to_owned())?;
    let orishu::client::Credentials::Token(marker) = &credential else {
        unreachable!("token loader")
    };
    let marker = marker.clone();
    let client = HttpClusterClient::new(
        connection.address,
        HttClientOptions {
            credentials: Some(credential),
            tls_cert: connection.ca_cert,
            ..Default::default()
        },
    )
    .map_err(|_| "Cannot initialize trusted worker connection.".to_owned())?;
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize run request executor.".to_owned())?;
    let reply = executor.block_on(async {
        match job {
            Job::Field(job) => fields::perform(&client, job).await.map(Reply::Field),
            Job::Geometry { .. } => unreachable!("local projection handled before connection IO"),
            Job::Discover => {
                let Some(run) = client.scientific().current().await? else { return Ok(Reply::Discovered(None)); };
                Ok(Reply::Discovered(client.scientific().status(&RunStatusRequest::new(run.identity().clone())).await?))
            },
            Job::Status(descriptor) => Ok(Reply::Discovered(client.scientific().status(&RunStatusRequest::new(descriptor.identity().clone())).await?)),
            Job::Load { request, upload, .. } => {
                // Cold upload copy is bounded at 128 MiB; the original bytes are
                // retained for exact explicit resubmission after reply loss.
                let receipt = match upload {
                    Upload::Send(bundle) => Some(client.scientific().submit(&request, bundle.as_ref().clone()).await?),
                    Upload::Lookup => client.scientific().lookup(&request).await?,
                    Upload::Stored => unreachable!("stored bytes are read before connection IO"),
                };
                Ok(Reply::Load(receipt))
            },
            Job::ClearLoad => unreachable!("local journal job"),
            Job::Observe { mut status, refresh, attach, scale } => {
                if refresh {
                    status = client.scientific().status(&RunStatusRequest::new(status.descriptor().identity().clone())).await?
                        .ok_or(ScientificError::Protocol("requested retained run unavailable"))?;
                }
                let objects = client.scientific().objects(&ObjectObservationRequest::new(status.descriptor().identity().clone(), status.boundary())).await?
                    .ok_or(ScientificError::Protocol("requested retained run unavailable"))?;
                let projection = Projection::new(objects, scale);
                if !matches!(projection.source, orishu_plugin::execution::SnapshotSource::Committed { time_seconds, .. } if time_seconds.get() == status.time_seconds()) {
                    return Err(ScientificError::Protocol("status and observation time disagree"));
                }
                Ok(Reply::Observed { status, projection, attach })
            },
            Job::Command { request, lookup, .. } => {
                let receipt = if lookup { client.scientific().command_lookup(&request).await? } else { Some(client.scientific().command(&request).await?) };
                Ok(Reply::Command(receipt))
            },
        }
    }).map_err(|error| match error {
        ScientificError::Http { status, code, .. } if !code.contains(&marker) => format!("Worker refused the request: HTTP {status}, {code}. Refresh/reconcile explicitly."),
        _ => "Run request failed validation or transport. Refresh reads or reconcile the original command; no retry was sent.".to_owned(),
    })?;
    // Never allow an untrusted worker to reflect a credential into window state.
    let reflected = |bytes: &[u8]| bytes.windows(marker.len()).any(|s| s == marker.as_bytes());
    let metadata = match &reply {
        Reply::Field(reply) => {
            if reply.reflects(&marker) {
                return Err("Worker response contained credential material.".into());
            }
            Ok(vec![])
        }
        Reply::Geometry { .. } => unreachable!("no remote data in local projection"),
        Reply::Discovered(status) => serde_json::to_vec(status),
        Reply::Observed {
            status, projection, ..
        } => {
            if reflected(projection.objects.bytes()) {
                return Err("Worker response contained credential material.".into());
            }
            serde_json::to_vec(status)
        }
        Reply::Command(receipt) => serde_json::to_vec(receipt),
        Reply::Load(receipt) => serde_json::to_vec(receipt),
        Reply::Cleared => unreachable!("local journal job"),
    }
    .expect("bounded validated metadata");
    if reflected(&metadata) {
        return Err("Worker response contained credential material.".into());
    }
    Ok(reply)
}

#[cfg(test)]
mod tests;
