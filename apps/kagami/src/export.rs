//! Headless, non-mutating document compilation and new-file workload publication.
//! No GUI, MCP listener, initialization, guest execution or cluster submission.
use clap::Args;
use orishu_plugin::{PluginId, resolution::ResolutionOutcome};
use serde::Serialize;
use std::{path::PathBuf, process::ExitCode};

/// `kagami export`: explicitly selected saved input, new output and inventory.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Saved scientific experiment. Legacy setup requires explicit migration first.
    pub experiment: PathBuf,
    /// New portable workload file. Existing files/symlinks are never overwritten.
    #[arg(long)]
    pub output: PathBuf,
    /// Human-readable workload name; captured into immutable workload metadata.
    #[arg(long)]
    pub name: orishu_workload::WorkloadName,
    /// Local plugin inventory; defaults to KAGAMI_PLUGIN_DIR or the user data root.
    #[arg(long, env = "KAGAMI_PLUGIN_DIR")]
    pub directory: Option<PathBuf>,
    /// Refuse if the inventory revision differs; no implicit retry or re-selection.
    #[arg(long)]
    pub expected_inventory_revision: Option<u64>,
    /// Enable a logical plugin for this export only; repeat for several.
    #[arg(long)]
    pub enable_plugin: Vec<PluginId>,
    /// Disable a logical plugin for this export only.
    #[arg(long)]
    pub disable_plugin: Vec<PluginId>,
    /// Print one versioned structured success/refusal object.
    #[arg(long)]
    pub json: bool,
}
/// Bounded diagnostic plus complete bounded resolver choices when applicable.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    /// Boundary that refused the operation.
    pub stage: &'static str,
    /// Stable domain code where available; never infer acceptance from exit output.
    pub code: String,
    /// Bounded explanation; filesystem errors do not expose paths.
    pub message: String,
    /// Exact resolver response, including options or stale revision, not a guess.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Box<ResolutionOutcome>>,
}
impl ExportError {
    fn new(stage: &'static str, code: impl Into<String>, message: impl std::fmt::Display) -> Self {
        let mut message = message.to_string();
        let mut end = message.len().min(512);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        Self {
            stage,
            code: code.into(),
            message,
            resolution: None,
        }
    }
}
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}: {}", self.stage, self.code, self.message)
    }
}
impl std::error::Error for ExportError {}

/// Successful immutable export, not numerical admission or execution authority.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    /// Canonical root identity, independent of archive bytes and output path.
    pub workload: orishu_workload::WorkloadDigest,
    /// Inventory revision from which exact code was frozen.
    pub inventory_revision: u64,
    /// Number of distinct required blobs, excluding the root entry.
    pub artifacts: usize,
    /// Actual complete portable file size.
    pub bytes: usize,
}
/// CLI output adapter; a refusal always returns a nonzero exit status.
pub fn run(args: ExportArgs) -> ExitCode {
    let json = args.json;
    let result = execute(args);
    let success = result.is_ok();
    let output = match result {
        Ok(result) => {
            serde_json::json!({"apiVersion":"kagami.workload-export/v1","ok":true,"result":result})
        }
        Err(error) => {
            serde_json::json!({"apiVersion":"kagami.workload-export/v1","ok":false,"error":error})
        }
    };
    if json {
        println!("{output}");
    } else if success {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).expect("structured outcome")
        );
    } else {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&output).expect("structured refusal")
        );
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Read one captured file, freeze its exact installed selection, compile/verify
/// its closure and publish a new durable portable bundle. The input is never
/// rewritten/recovered from a backup, and no default/alternate provider is chosen.
#[cfg(unix)]
pub fn execute(args: ExportArgs) -> Result<ExportReport, ExportError> {
    use crate::plugins::{self, PluginStore, PrepareSelectionOutcome, files};
    use orishu_plugin::{
        archive::ArchiveLimits,
        resolution::{ProviderBinding, RequirementKey, ResolutionRequest},
        workload::{ProfileLimits, bundle},
    };
    let local =
        |stage| move |e: plugins::Error| ExportError::new(stage, format!("{:?}", e.code), e);
    let bytes = files::read_file(
        &args.experiment,
        kagami_session::store::MAX_DOCUMENT_BYTES as usize,
    )
    .map_err(local("read"))?;
    let document = kagami_session::document::decode_document(&bytes)
        .map_err(|e| ExportError::new("document", e.code(), e))?;
    let experiment = document
        .into_experiment(&Default::default(), &Default::default())
        .map_err(|e| ExportError::new("document", e.code(), e))?;
    drop(bytes);
    let snapshot = experiment.snapshot();
    let setup = snapshot.setup().scientific().ok_or_else(|| ExportError::new("document", "scientific_setup_required", "legacy experiments require explicit model selection and captured initialization before export"))?;
    let path = args
        .directory
        .map_or_else(plugins::default_directory, Ok)
        .map_err(local("inventory"))?;
    let store = PluginStore::open(&path).map_err(local("inventory"))?;
    let expected = match args.expected_inventory_revision {
        Some(revision) => revision,
        None => store.list().map_err(local("inventory"))?.revision,
    };
    let selected = setup.declarations().descriptor();
    let request = ResolutionRequest {
        expected_inventory_revision: expected,
        roots: selected.roots.clone(),
        bindings: selected
            .bindings
            .iter()
            .map(|b| ProviderBinding {
                requirement: RequirementKey {
                    consumer: b.consumer.clone(),
                    slot: b.requirement_slot.clone(),
                },
                provider: b.provider.clone(),
            })
            .collect(),
    };
    let overrides: Vec<_> = args
        .enable_plugin
        .into_iter()
        .map(|id| (id, true))
        .chain(args.disable_plugin.into_iter().map(|id| (id, false)))
        .collect();
    let policy = ProfileLimits::default();
    let prepared = match store
        .prepare_selection(
            &request,
            &overrides,
            &selected.kernel_instances,
            policy.selection,
        )
        .map_err(local("selection"))?
    {
        PrepareSelectionOutcome::Ready(p) => p,
        PrepareSelectionOutcome::Unresolved(resolution) => {
            let code = if matches!(resolution, ResolutionOutcome::StaleRevision { .. }) {
                "stale_inventory_revision"
            } else {
                "selection_unavailable"
            };
            let mut error = ExportError::new(
                "selection",
                code,
                "the document's exact plugin selection is not available; inspect the structured resolver response",
            );
            error.resolution = Some(Box::new(resolution));
            return Err(error);
        }
    };
    let compiled = crate::workload::compile_captured(
        &snapshot,
        &prepared,
        orishu_workload::WorkloadMeta::new(args.name),
        policy,
        &Default::default(),
    )
    .map_err(|e| ExportError::new("compile", "workload_refused", e))?;
    let blobs = compiled
        .blobs()
        .iter()
        .map(|(d, b)| (*d, b.as_ref()))
        .collect();
    let output = bundle::pack(
        compiled.compiled().manifest_bytes(),
        &blobs,
        policy,
        ArchiveLimits::default(),
    )
    .map_err(|e| ExportError::new("pack", "workload_bundle_refused", e))?;
    files::create_file(&args.output, &output).map_err(local("publish"))?;
    Ok(ExportReport {
        workload: compiled.compiled().verified().root(),
        inventory_revision: prepared.inventory_revision(),
        artifacts: compiled.blobs().len(),
        bytes: output.len(),
    })
}

/// This platform must not silently fall back to unsafe plugin/inventory IO.
#[cfg(not(unix))]
pub fn execute(_: ExportArgs) -> Result<ExportReport, ExportError> {
    Err(ExportError::new(
        "platform",
        "unsupported_platform",
        "secure local plugin and export IO is not implemented on this platform",
    ))
}
