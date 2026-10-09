//! Headless immutable-workload delivery. No document mutation, plugin inventory,
//! field initialization, implicit membership changes or retry loop.
use clap::{Args, Subcommand, ValueEnum};
use orishu::{
    client::{
        ClusterAddress, credential_file,
        http_client::{HttClientOptions, HttpClusterClient, ScientificError},
    },
    model::{
        cluster::{FormationId, OperationId},
        run::{RunDescriptor, RunIdentity, WorkloadEpoch},
        run_command::{
            RunCommand, RunCommandError, RunCommandOutcome, RunCommandReceipt, RunCommandRequest,
            RunCommandState, RunStatus, RunStatusRequest,
        },
        run_load::{LoadOutcome, LoadReceipt, LoadRequest, LoadState},
        run_observation::{ObjectObservationRequest, ObservedObjects},
    },
};
use serde::Serialize;
use std::{num::NonZeroU64, path::PathBuf, process::ExitCode};
mod field;
use field::{FieldArgs, SampleArgs, SampleIntent, SampleReport};

#[cfg(unix)]
const MAX_BUNDLE_BYTES: usize = 128 * 1024 * 1024;

/// Common scientific client options, following `kagami [-H worker] workload`.
#[derive(Debug, Args)]
pub struct WorkloadArgs {
    /// Explicit private worker operator token file; no raw token arguments.
    #[arg(long, env = "ORISHU_OPERATOR_TOKEN_FILE")]
    pub operator_token_file: PathBuf,
    /// Additional trusted TLS certificate for the selected worker, not a client key.
    #[arg(long)]
    pub ca_cert: Option<PathBuf>,
    /// Print one compact versioned JSON outcome (default: indented JSON).
    #[arg(long, global = true)]
    pub json: bool,
    /// No automatic polling or retry: inspect the original request explicitly.
    #[command(subcommand)]
    pub command: WorkloadCommand,
}

/// Exact intent retained by the caller, never guessed from a fresh status read.
#[derive(Clone, Debug, Args)]
pub struct IntentArgs {
    /// Expected formation identity. Lock it explicitly using operator tooling.
    #[arg(long)]
    pub formation_id: FormationId,
    /// Caller-chosen operation identity; preserve it when reconciling lost replies.
    #[arg(long)]
    pub operation_id: OperationId,
    /// Expected immutable root, as reported by `kagami export`.
    #[arg(long)]
    pub workload_id: orishu_workload::WorkloadDigest,
}
impl IntentArgs {
    fn request(&self) -> LoadRequest {
        LoadRequest::new(
            self.operation_id.clone(),
            self.formation_id.clone(),
            self.workload_id,
        )
    }
}

/// Exact owner-issued run tuple. No lookup of "current" supplies missing fields.
#[derive(Clone, Debug, Args)]
pub struct RunArgs {
    /// Formation identity from the accepted run descriptor.
    #[arg(long)]
    pub formation_id: FormationId,
    /// Immutable workload root from the accepted run descriptor.
    #[arg(long)]
    pub workload_id: orishu_workload::WorkloadDigest,
    /// Nonzero epoch allocated by the worker, not a locally selected revision.
    #[arg(long)]
    pub workload_epoch: NonZeroU64,
}
impl RunArgs {
    fn identity(&self) -> RunIdentity {
        RunIdentity::new(
            self.formation_id.clone(),
            self.workload_id,
            WorkloadEpoch::new(self.workload_epoch.get()),
        )
    }
}

/// Full immutable command intent, retained unchanged during reconciliation.
#[derive(Clone, Debug, Args)]
pub struct RunIntentArgs {
    #[command(flatten)]
    pub run: RunArgs,
    /// Caller-chosen command identity; never reuse with different intent.
    #[arg(long)]
    pub operation_id: OperationId,
    /// Exact committed boundary expected before this action; never auto-refreshed.
    #[arg(long)]
    pub expected_boundary: u64,
}
impl RunIntentArgs {
    fn request(&self, action: RunCommand) -> Result<RunCommandRequest, RunCommandError> {
        RunCommandRequest::new(
            self.operation_id.clone(),
            self.run.identity(),
            self.expected_boundary,
            action,
        )
    }
}

/// Original action required by receipt lookup, not a new action to execute.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CommandAction {
    Step,
    Finish,
}
impl From<CommandAction> for RunCommand {
    fn from(value: CommandAction) -> Self {
        match value {
            CommandAction::Step => Self::Step,
            CommandAction::Finish => Self::Finish,
        }
    }
}

/// Whole-upload admission, exact manual control, and separate historical/live reads.
#[derive(Debug, Subcommand)]
pub enum WorkloadCommand {
    /// Verify and submit a previously exported complete portable bundle once.
    Submit {
        /// Immutable portable workload file, not an editable experiment/plugin.
        bundle: PathBuf,
        #[command(flatten)]
        intent: IntentArgs,
    },
    /// Retrieve the historical receipt for exactly the original operation.
    Receipt(IntentArgs),
    /// Discover the worker's presently usable run (not a restored receipt).
    Current,
    /// Advance exactly one fixed step at the supplied run/boundary.
    Step(RunIntentArgs),
    /// Terminally finish integration at this boundary; NOT pause or unload.
    Finish(RunIntentArgs),
    /// Read the exact original command receipt without executing it again.
    CommandReceipt {
        #[command(flatten)]
        intent: RunIntentArgs,
        /// Original command action, including when reconciling a lost reply.
        #[arg(long, value_enum)]
        action: CommandAction,
    },
    /// Read committed metadata for exactly this run, not its command history.
    Status(RunArgs),
    /// Read complete numeric objects/forces at one exact committed boundary.
    Objects {
        #[command(flatten)]
        run: RunArgs,
        /// Exact committed boundary to read; never automatically refreshed.
        #[arg(long)]
        boundary: u64,
    },
    /// Describe exact field state, model and available typed sampling channels.
    Field(FieldArgs),
    /// Sample selected field channels at explicit SI points, without advancing.
    Sample(SampleArgs),
}
impl WorkloadCommand {
    fn name(&self) -> &'static str {
        match self {
            Self::Submit { .. } => "submit",
            Self::Receipt(_) => "receipt",
            Self::Current => "current",
            Self::Step(_) => "step",
            Self::Finish(_) => "finish",
            Self::CommandReceipt { .. } => "command-receipt",
            Self::Status(_) => "status",
            Self::Objects { .. } => "objects",
            Self::Field(_) => "field",
            Self::Sample(_) => "sample",
        }
    }
    fn request(&self) -> Result<Option<ReportRequest>, RunCommandError> {
        Ok(match self {
            Self::Submit { intent, .. } | Self::Receipt(intent) => {
                Some(ReportRequest::Load(intent.request()))
            }
            Self::Current => None,
            Self::Step(intent) => Some(ReportRequest::Command(intent.request(RunCommand::Step)?)),
            Self::Finish(intent) => {
                Some(ReportRequest::Command(intent.request(RunCommand::Finish)?))
            }
            Self::CommandReceipt { intent, action } => {
                Some(ReportRequest::Command(intent.request((*action).into())?))
            }
            Self::Status(run) => Some(ReportRequest::Status(RunStatusRequest::new(run.identity()))),
            Self::Objects { run, boundary } => Some(ReportRequest::Objects(
                ObjectObservationRequest::new(run.identity(), *boundary),
            )),
            Self::Field(args) => Some(ReportRequest::Field(args.request())),
            Self::Sample(args) => Some(ReportRequest::Sample(args.intent())),
        })
    }
    fn mutates(&self) -> bool {
        matches!(self, Self::Submit { .. } | Self::Step(_) | Self::Finish(_))
    }
    fn report_version(&self) -> &'static str {
        match self {
            Self::Submit { .. } | Self::Receipt(_) | Self::Current => "kagami.workload-command/v1",
            Self::Objects { .. } => "kagami.object-observation/v1",
            Self::Field(_) | Self::Sample(_) => "kagami.field-observation/v1",
            _ => "kagami.run-command/v1",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum ReportRequest {
    Load(LoadRequest),
    Command(RunCommandRequest),
    Status(RunStatusRequest),
    Objects(ObjectObservationRequest),
    Field(orishu::model::run_observation::FieldObservationRequest),
    Sample(SampleIntent),
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ReportReceipt {
    Load(LoadReceipt),
    Command(RunCommandReceipt),
}

/// A fact-oriented outcome: transport success is never called run acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Accepted,
    Applied,
    Pending,
    Refused,
    Indeterminate,
    NotFound,
    Live,
    Observed,
    Empty,
    Error,
}
/// Bounded diagnostics; never include credential contents or file paths.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    stage: &'static str,
    code: String,
    message: String,
    /// Sending may have succeeded. Query the exact original request; do not mint a new ID.
    submission_outcome_unknown: bool,
}
impl Failure {
    fn local(stage: &'static str, code: &'static str, message: impl ToString) -> Self {
        Self {
            stage,
            code: code.into(),
            message: message.to_string(),
            submission_outcome_unknown: false,
        }
    }
    fn network(error: ScientificError, submitting: bool) -> Self {
        let uncertain = submitting && !matches!(error, ScientificError::Input(_));
        let code = match &error {
            ScientificError::Input(_) => "invalid_request".into(),
            ScientificError::Transport => "transport".into(),
            ScientificError::Deadline => "deadline".into(),
            ScientificError::Protocol(_) => "protocol".into(),
            ScientificError::Http { code, .. } => code.clone(),
        };
        Self {
            stage: "client",
            code,
            message: error.to_string(),
            submission_outcome_unknown: uncertain,
        }
    }
}

/// Versioned command result suitable for scripts. Original intent survives errors;
/// credentials and artifact bytes do not. A historical receipt is not a live run.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    api_version: &'static str,
    command: &'static str,
    outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<ReportRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<ReportReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run: Option<RunDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<RunStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation: Option<ObservationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<orishu_plugin::execution::FieldObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    samples: Option<SampleReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Failure>,
}
impl Report {
    fn contains_credential(&self, credential: &str) -> bool {
        // Operator tokens are exact ASCII hex, so JSON escaping cannot hide one.
        serde_json::to_vec(self)
            .expect("bounded report")
            .windows(credential.len())
            .any(|bytes| bytes == credential.as_bytes())
    }
    /// 0 accepted/applied/live/observed; 1 local/transport/protocol error; 10 pending; 11 refused;
    /// 12 indeterminate; 13 missing receipt/no currently usable run. Ordinary
    /// command-line syntax errors retain Clap's exit 2, never mean Pending.
    pub fn exit_code(&self) -> u8 {
        match self.outcome {
            Outcome::Accepted | Outcome::Applied | Outcome::Live | Outcome::Observed => 0,
            Outcome::Error => 1,
            Outcome::Pending => 10,
            Outcome::Refused => 11,
            Outcome::Indeterminate => 12,
            Outcome::NotFound | Outcome::Empty => 13,
        }
    }
    fn fail(mut self, error: Failure) -> Self {
        self.error = Some(error);
        self
    }
    fn receipt(mut self, receipt: Option<LoadReceipt>) -> Self {
        self.outcome = match receipt.as_ref().map(LoadReceipt::state) {
            None => Outcome::NotFound,
            Some(LoadState::Pending) => Outcome::Pending,
            Some(LoadState::Finished(LoadOutcome::Accepted { .. })) => Outcome::Accepted,
            Some(LoadState::Finished(LoadOutcome::Refused { .. })) => Outcome::Refused,
            Some(LoadState::Finished(LoadOutcome::Indeterminate)) => Outcome::Indeterminate,
        };
        self.receipt = receipt.map(ReportReceipt::Load);
        self
    }
    fn command_receipt(mut self, receipt: Option<RunCommandReceipt>) -> Self {
        self.outcome = match receipt.as_ref().map(RunCommandReceipt::state) {
            None => Outcome::NotFound,
            Some(RunCommandState::Pending) => Outcome::Pending,
            Some(RunCommandState::Finished(RunCommandOutcome::Applied { .. })) => Outcome::Applied,
            Some(RunCommandState::Finished(RunCommandOutcome::Refused { .. })) => Outcome::Refused,
            Some(RunCommandState::Finished(RunCommandOutcome::Indeterminate)) => {
                Outcome::Indeterminate
            }
        };
        self.receipt = receipt.map(ReportReceipt::Command);
        self
    }
}

/// Perform one command against the explicit worker. Initialization and local
/// verification finish before any request; no document or inventory is opened.
/// This synchronous CLI shell creates its own executor; async/window/MCP adapters
/// should use the shared async scientific client rather than nest this executor.
pub fn execute(host: ClusterAddress, args: WorkloadArgs) -> Report {
    let report = Report {
        api_version: args.command.report_version(),
        command: args.command.name(),
        outcome: Outcome::Error,
        request: None,
        receipt: None,
        run: None,
        status: None,
        observation: None,
        field: None,
        samples: None,
        error: None,
    };
    let report = match args.command.request() {
        Ok(request) => Report { request, ..report },
        Err(error) => return report.fail(Failure::local("request", "invalid_request", error)),
    };
    if let Some(ReportRequest::Sample(intent)) = &report.request
        && let Err(error) = intent.validate()
    {
        return report.fail(error);
    }
    let credential = match credential_file::load(&args.operator_token_file) {
        Ok(value) => value,
        Err(error) => {
            return report.fail(Failure::local(
                "credential",
                "invalid_credential_file",
                error,
            ));
        }
    };
    let credential_marker = match &credential {
        orishu::client::Credentials::Token(token) => token.clone(),
        _ => unreachable!("explicit operator token loader"),
    };
    if report.contains_credential(&credential_marker) {
        return Report {
            request: None,
            ..report
        }
        .fail(Failure::local(
            "request",
            "credential_in_intent",
            "credential material cannot be used in request identity",
        ));
    }
    let bundle = if let WorkloadCommand::Submit { bundle, intent } = &args.command {
        match read_bundle(bundle, intent.workload_id) {
            Ok(bytes) => Some(bytes),
            Err(error) => return report.fail(error),
        }
    } else {
        None
    };
    let client = match HttpClusterClient::new(
        host,
        HttClientOptions {
            credentials: Some(credential),
            tls_cert: args.ca_cert,
            ..Default::default()
        },
    ) {
        Ok(value) => value,
        Err(_) => {
            return report.fail(Failure::local(
                "client",
                "configuration",
                "cannot initialize trusted worker connection",
            ));
        }
    };
    let executor = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(value) => value,
        Err(_) => {
            return report.fail(Failure::local(
                "client",
                "executor",
                "cannot initialize command executor",
            ));
        }
    };
    let original_request = report.request.clone();
    let mutates = args.command.mutates();
    let response = executor.block_on(async {
        match (args.command, original_request.as_ref()) {
            (WorkloadCommand::Submit { .. }, Some(ReportRequest::Load(intent))) => {
                let result = client
                    .scientific()
                    .submit(intent, bundle.expect("verified bundle"))
                    .await;
                match result {
                    Ok(receipt) => report.receipt(Some(receipt)),
                    Err(error) => report.fail(Failure::network(error, true)),
                }
            }
            (WorkloadCommand::Receipt(_), Some(ReportRequest::Load(intent))) => {
                let result = client.scientific().lookup(intent).await;
                match result {
                    Ok(receipt) => report.receipt(receipt),
                    Err(error) => report.fail(Failure::network(error, false)),
                }
            }
            (WorkloadCommand::Current, None) => match client.scientific().current().await {
                Ok(run) => Report {
                    outcome: if run.is_some() {
                        Outcome::Live
                    } else {
                        Outcome::Empty
                    },
                    run,
                    ..report
                },
                Err(error) => report.fail(Failure::network(error, false)),
            },
            (
                WorkloadCommand::Step(_) | WorkloadCommand::Finish(_),
                Some(ReportRequest::Command(intent)),
            ) => match client.scientific().command(intent).await {
                Ok(receipt) => report.command_receipt(Some(receipt)),
                Err(error) => report.fail(Failure::network(error, true)),
            },
            (WorkloadCommand::CommandReceipt { .. }, Some(ReportRequest::Command(intent))) => {
                match client.scientific().command_lookup(intent).await {
                    Ok(receipt) => report.command_receipt(receipt),
                    Err(error) => report.fail(Failure::network(error, false)),
                }
            }
            (WorkloadCommand::Status(_), Some(ReportRequest::Status(intent))) => {
                match client.scientific().status(intent).await {
                    Ok(status) => Report {
                        outcome: if status.is_some() {
                            Outcome::Live
                        } else {
                            Outcome::Empty
                        },
                        status,
                        ..report
                    },
                    Err(error) => report.fail(Failure::network(error, false)),
                }
            }
            (WorkloadCommand::Objects { .. }, Some(ReportRequest::Objects(intent))) => {
                match client.scientific().objects(intent).await {
                    Ok(observation) => Report {
                        outcome: if observation.is_some() {
                            Outcome::Observed
                        } else {
                            Outcome::Empty
                        },
                        observation: observation.map(ObservationReport),
                        ..report
                    },
                    Err(error) => report.fail(Failure::network(error, false)),
                }
            }
            (WorkloadCommand::Field(_), Some(ReportRequest::Field(intent))) => {
                match client.scientific().field(intent).await {
                    Ok(field) => Report {
                        outcome: if field.is_some() {
                            Outcome::Observed
                        } else {
                            Outcome::Empty
                        },
                        field,
                        ..report
                    },
                    Err(error) => report.fail(Failure::network(error, false)),
                }
            }
            (WorkloadCommand::Sample(_), Some(ReportRequest::Sample(intent))) => {
                field::sample(&client, intent, report).await
            }
            _ => unreachable!("command constructs its own typed intent"),
        }
    });
    if response.contains_credential(&credential_marker) {
        return Report {
            api_version: response.api_version,
            command: response.command,
            outcome: Outcome::Error,
            request: original_request,
            receipt: None,
            run: None,
            status: None,
            observation: None,
            field: None,
            samples: None,
            error: Some(Failure {
                stage: "client",
                code: "credential_reflection".into(),
                message: "worker response contained credential material and was not reported"
                    .into(),
                submission_outcome_unknown: mutates,
            }),
        };
    }
    response
}

/// JSON projection of already validated immutable bytes. Serialization streams
/// records rather than creating a second per-object/per-force collection.
struct ObservationReport(ObservedObjects);
impl std::fmt::Debug for ObservationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservationReport")
            .field("digest", &self.0.digest())
            .finish_non_exhaustive()
    }
}
impl Serialize for ObservationReport {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        struct Records<I>(I);
        impl<I> Serialize for Records<I>
        where
            I: Clone + IntoIterator,
            I::Item: Serialize,
        {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_seq(self.0.clone())
            }
        }
        let view = self.0.view();
        let mut result = serializer.serialize_struct("ObjectObservation", 6)?;
        result.serialize_field("digest", &view.id())?;
        result.serialize_field("source", view.source())?;
        result.serialize_field("coverage", "complete-numeric-objects")?;
        result.serialize_field("forceEvaluationBoundary", &view.force_evaluation_boundary())?;
        // Indexed ranges are Clone; shared Batch iteration itself deliberately
        // exposes only an ExactSizeIterator contract, not Clone.
        let objects = view.objects();
        result.serialize_field(
            "objects",
            &Records((0..objects.len()).map(|i| objects.get(i).expect("validated object"))),
        )?;
        let forces = view.forces();
        result.serialize_field(
            "forces",
            &forces.map(|forces| {
                Records((0..forces.len()).map(move |i| forces.get(i).expect("validated force")))
            }),
        )?;
        result.end()
    }
}

#[cfg(unix)]
fn read_bundle(
    path: &std::path::Path,
    expected: orishu_workload::WorkloadDigest,
) -> Result<Vec<u8>, Failure> {
    let bytes = crate::plugins::files::read_file(path, MAX_BUNDLE_BYTES).map_err(|_| {
        Failure::local(
            "bundle",
            "unreadable_bundle",
            "cannot read a bounded regular portable workload file",
        )
    })?;
    let bundle =
        orishu_plugin::workload::bundle::read(&bytes, Default::default(), Default::default())
            .map_err(|_| {
                Failure::local(
                    "bundle",
                    "invalid_bundle",
                    "portable bundle failed bounded closure verification",
                )
            })?;
    if bundle.verified().root() != expected {
        return Err(Failure::local(
            "bundle",
            "root_mismatch",
            "portable bundle does not match the requested workload identity",
        ));
    }
    Ok(bytes)
}
#[cfg(not(unix))]
fn read_bundle(
    _: &std::path::Path,
    _: orishu_workload::WorkloadDigest,
) -> Result<Vec<u8>, Failure> {
    Err(Failure::local(
        "bundle",
        "unsupported_platform",
        "secure portable file loading is not implemented on this platform",
    ))
}

/// Print escaped JSON, including in human-readable mode. A reporting failure does
/// not cancel or undo a submitted workload; the caller still has its explicit ID.
pub fn run(host: ClusterAddress, args: WorkloadArgs) -> ExitCode {
    use std::io::Write;
    let compact = args.json;
    let report = execute(host, args);
    let output = if compact {
        serde_json::to_vec(&report)
    } else {
        serde_json::to_vec_pretty(&report)
    }
    .expect("bounded report");
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&output)
        .and_then(|_| stdout.write_all(b"\n"))
        .is_err()
    {
        eprintln!("cannot report workload outcome; reconcile the original request before retrying");
        return ExitCode::FAILURE;
    }
    ExitCode::from(report.exit_code())
}
