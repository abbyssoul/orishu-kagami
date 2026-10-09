//! Window-owned remote run projection. Orishu alone advances scientific state.
//! One off-window job; detached reads cannot reattach, uncertain commands retain
//! their original intent for explicit receipt lookup. No document/guest access.
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
    Load {
        request: LoadRequest,
        bundle: Option<Arc<Vec<u8>>>,
    },
    Observe {
        status: RunStatus,
        refresh: bool,
        attach: bool,
        scale: kagami_session::SceneScale,
    },
    Command {
        request: RunCommandRequest,
        lookup: bool,
    },
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
}
struct Pending {
    generation: u64,
    field_generation: Option<u64>,
    command: bool,
    work: JoinHandle<Result<Reply, String>>,
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

/// Exact upload intent and frozen source retained across reply loss and draft
/// changes. The server's receipt, not current-run discovery, proves attribution.
pub struct Submission {
    pub request: LoadRequest,
    pub source_context: uuid::Uuid,
    pub source_revision: u64,
    pub receipt: Option<LoadReceipt>,
    bundle: Arc<Vec<u8>>,
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
    pending: Option<Pending>,
    generation: u64,
    selected: Option<RunStatus>,
    objects: Option<Projection>,
    attached: bool,
    intent: Option<RunCommandRequest>,
    receipt: Option<RunCommandReceipt>,
    submission: Option<Submission>,
    pub open: bool,
    pub notice: String,
    pub numeric_view: bool,
    desired_scale: kagami_session::SceneScale,
    pub fields: fields::Inspector,
}
impl Controller {
    pub fn new(connection: Option<Connection>) -> Self {
        Self {
            connection,
            pending: None,
            generation: 0,
            selected: None,
            objects: None,
            attached: false,
            intent: None,
            receipt: None,
            submission: None,
            open: false,
            numeric_view: false,
            desired_scale: kagami_session::SceneScale::METRE,
            fields: fields::Inspector::default(),
            notice:
                "Inspect an externally submitted run. The open document is not its initial scene."
                    .into(),
        }
    }
    pub fn configured(&self) -> bool {
        self.connection.is_some()
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
        self.intent.as_ref()
    }
    pub fn receipt(&self) -> Option<&RunCommandReceipt> {
        self.receipt.as_ref()
    }
    pub fn submission(&self) -> Option<&Submission> {
        self.submission.as_ref()
    }
    pub fn can_submit(&self) -> bool {
        self.configured()
            && !self.is_pending()
            && !self.attached
            && self.intent.is_none()
            && self.submission.is_none()
    }
    pub fn source_revision(&self) -> Option<u64> {
        let submission = self.submission.as_ref()?;
        (submission.accepted()? == self.selected.as_ref()?.descriptor())
            .then_some(submission.source_revision)
    }
    #[cfg(unix)]
    pub fn submit(
        &mut self,
        workload: &crate::workload_preparation::FrozenWorkload,
        formation: orishu::model::cluster::FormationId,
    ) {
        if !self.can_submit() {
            self.notice = "Detach and resolve/clear existing submission before a new load.".into();
            return;
        }
        let request = LoadRequest::new(
            format!("kagami-load-{}", uuid::Uuid::new_v4())
                .parse()
                .expect("bounded ID"),
            formation,
            workload.report.workload,
        );
        self.selected = None;
        self.receipt = None;
        self.objects = None;
        self.submission = Some(Submission {
            request: request.clone(),
            source_context: workload.source_context,
            source_revision: workload.source_revision,
            receipt: None,
            bundle: workload.bytes.clone(),
        });
        self.start(Job::Load {
            request,
            bundle: Some(workload.bytes.clone()),
        });
    }
    pub fn can_control(&self) -> bool {
        self.attached
            && !self.is_pending()
            && self.intent.is_none()
            && self.objects.is_some()
            && self
                .selected
                .as_ref()
                .is_some_and(|s| s.phase() == RunPhase::Ready)
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
        match action {
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
                if !self.attached && self.pending.as_ref().is_some_and(|p| !p.command) {
                    self.detach();
                }
                return;
            }
            Action::Poll => return,
            Action::ClearLoad
                if !self.is_pending()
                    && !self.attached
                    && self.submission.as_ref().is_some_and(|s| !s.unresolved()) =>
            {
                self.submission = None;
                self.notice =
                    "Final submission history cleared locally. No worker state changed.".into();
                return;
            }
            _ => {}
        }
        if self.pending.is_some() {
            self.notice = "A run request still owns the background slot.".into();
            return;
        }
        let job = match action {
            Action::ReconcileLoad | Action::ResubmitLoad => {
                let Some(s) = &self.submission else {
                    return;
                };
                if !s.unresolved() {
                    return;
                }
                Job::Load {
                    request: s.request.clone(),
                    bundle: (action == Action::ResubmitLoad).then(|| s.bundle.clone()),
                }
            }
            Action::InspectLoaded if !self.attached && self.intent.is_none() => {
                let Some(descriptor) = self.submission.as_ref().and_then(Submission::accepted)
                else {
                    return;
                };
                Job::Status(descriptor.clone())
            }
            Action::Discover
                if !self.attached
                    && self.intent.is_none()
                    && !self.submission.as_ref().is_some_and(Submission::unresolved) =>
            {
                Job::Discover
            }
            Action::Observe
                if !self.attached
                    && self.intent.is_none()
                    && !self.submission.as_ref().is_some_and(Submission::unresolved) =>
            {
                let Some(status) = self.selected.clone() else {
                    self.notice = "Inspect a retained run first.".into();
                    return;
                };
                Job::Observe {
                    scale: self.desired_scale,
                    status,
                    refresh: false,
                    attach: true,
                }
            }
            Action::Refresh if self.attached => {
                let Some(status) = self.selected.clone() else {
                    return;
                };
                Job::Observe {
                    scale: self.desired_scale,
                    status,
                    refresh: true,
                    attach: false,
                }
            }
            Action::Step | Action::Finish if self.can_control() => {
                let status = self.selected.as_ref().expect("control has selected run");
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
                self.intent = Some(request.clone());
                self.receipt = None;
                Job::Command {
                    request,
                    lookup: false,
                }
            }
            Action::Reconcile | Action::ResubmitOriginal => {
                let Some(request) = self.intent.clone() else {
                    return;
                };
                Job::Command {
                    request,
                    lookup: action == Action::Reconcile,
                }
            }
            _ => {
                self.notice = "Action unavailable; inspect/refresh or reconcile the exact pending command first.".into();
                return;
            }
        };
        self.start(job);
    }
    fn start(&mut self, job: Job) {
        let Some(connection) = self.connection.clone() else {
            self.notice =
                "Start Kagami with --operator-token-file to enable authenticated run access."
                    .into();
            return;
        };
        let command = matches!(job, Job::Command { .. } | Job::Load { .. });
        let field_generation = matches!(job, Job::Field(_)).then(|| self.fields.generation());
        if !matches!(job, Job::Field(_) | Job::Geometry { .. }) {
            self.fields.clear();
        }
        match std::thread::Builder::new().name("kagami-run".into()).spawn(move || perform(connection, job)) {
            Ok(work) => {
                self.pending = Some(Pending { generation: self.generation, field_generation, command, work });
                self.notice = "Request pending. Displayed values remain an identified snapshot, not a live stream.".into();
            },
            Err(_) => self.notice = "Cannot start run request; reconcile retained command intent before another action.".into(),
        }
    }
    /// Detach is local and never sends finish/stop. Keep capacity until any old
    /// job exits, and keep possibly submitted command intent for reconciliation.
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
        let reply = pending.work.join().unwrap_or_else(|_| {
            Err("Run request thread failed; reconcile any retained command intent.".into())
        });
        if pending.generation != self.generation && !pending.command {
            return None;
        }
        if pending
            .field_generation
            .is_some_and(|generation| generation != self.fields.generation())
        {
            return None;
        }
        match reply {
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
            Ok(Reply::Load(receipt)) => {
                self.notice = match receipt.as_ref().map(LoadReceipt::state) {
                    Some(LoadState::Finished(LoadOutcome::Accepted { .. })) => "Load accepted historically. Inspect this exact accepted run, then observe explicitly; acceptance alone is not current availability.",
                    Some(LoadState::Finished(LoadOutcome::Refused { .. })) => "Load refused. No initial state was published by this operation.",
                    Some(LoadState::Finished(LoadOutcome::Indeterminate)) => "Load outcome indeterminate. Original frozen bytes and intent retained; no new submission allowed.",
                    Some(LoadState::Pending) => "Load pending. Reconcile the original submission explicitly.",
                    None => "No recorded load receipt. Original frozen bytes and intent retained; no automatic retry.",
                }.into();
                self.submission.as_mut().expect("load owns intent").receipt = receipt;
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
                        self.selected = Some(status.clone()); self.intent = None;
                        "Command applied. Refresh committed values; the receipt is historical, not an observation."
                    },
                    Some(RunCommandState::Finished(RunCommandOutcome::Refused { .. })) => {
                        self.intent = None; "Command refused. Refresh before another command."
                    },
                    Some(RunCommandState::Finished(RunCommandOutcome::Indeterminate)) => "Command outcome indeterminate. Original intent retained; no fresh command will be issued.",
                    Some(RunCommandState::Pending) => "Command pending. Reconcile this same operation explicitly.",
                    None => "No recorded receipt for the original intent. It remains unresolved; no automatic resubmission.",
                }.into();
                self.receipt = receipt;
            }
        }
        None
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
            Job::Load { request, bundle } => {
                // Cold upload copy is bounded at 128 MiB; the original bytes are
                // retained for exact explicit resubmission after reply loss.
                let receipt = if let Some(bundle) = bundle { Some(client.scientific().submit(&request, bundle.as_ref().clone()).await?) }
                    else { client.scientific().lookup(&request).await? };
                Ok(Reply::Load(receipt))
            },
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
            Job::Command { request, lookup } => {
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
    }
    .expect("bounded validated metadata");
    if reflected(&metadata) {
        return Err("Worker response contained credential material.".into());
    }
    Ok(reply)
}
