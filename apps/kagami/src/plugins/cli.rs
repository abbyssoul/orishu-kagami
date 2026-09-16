//! CLI is an adapter over the same local authority available to UI/MCP.
use super::{Code, Error};
use clap::{Args, Subcommand};
use orishu_plugin::{PluginId, PluginReleaseId};
use std::path::PathBuf;

/// Headless plugin-management options; no window or MCP listener is started.
#[derive(Debug, Args)]
pub struct PluginArgs {
    /// Local inventory directory (defaults to XDG_DATA_HOME/kagami/plugins).
    #[arg(long, global = true, env = "KAGAMI_PLUGIN_DIR")]
    pub directory: Option<PathBuf>,
    /// Emit a versioned structured outcome instead of display text.
    #[arg(long, global = true)]
    pub json: bool,
    /// Reject mutation if this revision is no longer current.
    #[arg(long, global = true)]
    pub expected_revision: Option<u64>,
    /// Explicit local operation.
    #[command(subcommand)]
    pub command: PluginCommand,
}
/// Accepted MVP local management grammar.
#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Validate a source directory or bundle without running code.
    Validate { path: PathBuf },
    /// Pack externally built source inputs into a new bundle.
    Pack {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Inspect a bundle/source path or exact installed release ID.
    Inspect { target: String },
    /// Install a bundle; additional releases preserve the existing default.
    Install {
        bundle: PathBuf,
        #[arg(long)]
        expect_release: Option<PluginReleaseId>,
    },
    /// Install an explicit replacement release and choose it as default.
    Update {
        plugin_id: PluginId,
        bundle: PathBuf,
        #[arg(long)]
        expect_release: Option<PluginReleaseId>,
    },
    /// List logical plugins' defaults, or every installed release.
    List {
        #[arg(long)]
        all_releases: bool,
    },
    /// Choose a default without changing documents' pinned selections.
    SetDefault {
        plugin_id: PluginId,
        release: PluginReleaseId,
    },
    /// Enable a logical plugin for future authoring selections.
    Enable { plugin_id: PluginId },
    /// Disable a logical plugin without interrupting accepted runs.
    Disable { plugin_id: PluginId },
    /// De-register a release, retaining cached data and existing readers.
    Remove {
        plugin_id: PluginId,
        release: PluginReleaseId,
        #[arg(long)]
        ack_open_references: bool,
    },
}
/// Execute and display a bounded v1 outcome. A failed operation returns nonzero;
/// successfully printing its diagnostic is not command acceptance.
pub fn run(args: PluginArgs) -> std::process::ExitCode {
    let json = args.json;
    let result = execute(args);
    let success = result.is_ok();
    if json {
        let output = match result {
            Ok(value) => {
                serde_json::json!({"apiVersion":"kagami.plugin-command/v1","ok":true,"result":value})
            }
            Err(error) => {
                serde_json::json!({"apiVersion":"kagami.plugin-command/v1","ok":false,"error":error})
            }
        };
        println!("{}", serde_json::to_string(&output).expect("JSON outcome"));
    } else {
        match result {
            Ok(value) => println!(
                "{}",
                serde_json::to_string_pretty(&value).expect("JSON outcome")
            ),
            Err(error) => eprintln!("{error}"),
        }
    }
    if success {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
#[cfg(not(unix))]
fn execute(_: PluginArgs) -> Result<serde_json::Value, Error> {
    Err(Error::new(
        Code::UnsupportedVersion,
        "secure local plugin IO is not implemented on this platform yet",
    ))
}
#[cfg(unix)]
fn execute(args: PluginArgs) -> Result<serde_json::Value, Error> {
    use super::{InventoryCommand, Package, PluginStore};
    use PluginCommand as C;
    let summary =
        |p: &Package| serde_json::json!({"release":p.release().id(),"manifest":p.release().root()});
    // Read-only file operations do not initialize or mutate the installed store.
    match &args.command {
        C::Validate { path } => return Ok(summary(&Package::load(path)?)),
        C::Pack { source, output } => {
            if !source.is_dir() {
                return Err(Error::new(
                    Code::InvalidSelection,
                    "pack expects a source directory",
                ));
            }
            let p = Package::load(source)?;
            p.write_bundle(output)?;
            return Ok(summary(&p));
        }
        C::Inspect { target } if !target.starts_with("sha256:") => {
            return Ok(summary(&Package::load(std::path::Path::new(target))?));
        }
        _ => (),
    }
    let path = match args.directory {
        Some(path) => path,
        None => super::default_directory()?,
    };
    let store = PluginStore::open(&path)?;
    let revision = args.expected_revision.unwrap_or(store.list()?.revision);
    let changed = match args.command {
        C::List { all_releases } => {
            let mut listing = store.list()?;
            if !all_releases {
                listing.releases.retain(|e| e.is_default);
            }
            return serde_json::to_value(listing)
                .map_err(|_| Error::new(Code::Malformed, "listing serialization failed"));
        }
        C::Inspect { target } => return Ok(summary(&store.inspect(target.parse()?)?)),
        C::Install {
            bundle,
            expect_release,
        } => {
            let package = Package::from_bundle(&super::package::read_file(
                &bundle,
                orishu_plugin::bundle::BundleLimits::default().max_bytes,
            )?)?;
            store.install(revision, &package, None, expect_release)?
        }
        C::Update {
            plugin_id,
            bundle,
            expect_release,
        } => {
            let package = Package::from_bundle(&super::package::read_file(
                &bundle,
                orishu_plugin::bundle::BundleLimits::default().max_bytes,
            )?)?;
            store.install(revision, &package, Some(&plugin_id), expect_release)?
        }
        C::SetDefault { plugin_id, release } => store.submit(
            revision,
            InventoryCommand::SetDefault { plugin_id, release },
        )?,
        C::Enable { plugin_id } => store.submit(
            revision,
            InventoryCommand::SetEnabled {
                plugin_id,
                enabled: true,
            },
        )?,
        C::Disable { plugin_id } => store.submit(
            revision,
            InventoryCommand::SetEnabled {
                plugin_id,
                enabled: false,
            },
        )?,
        C::Remove {
            plugin_id,
            release,
            ack_open_references,
        } => store.submit(
            revision,
            InventoryCommand::Remove {
                plugin_id,
                release,
                ack_open_references,
            },
        )?,
        C::Validate { .. } | C::Pack { .. } => {
            unreachable!("file operations returned before opening store")
        }
    };
    Ok(serde_json::json!({"revision":changed}))
}
