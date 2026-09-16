use clap::Parser;
use kagami::launch::LaunchOptions;
use kagami::model::Model;
use kagami::{subscription, update, view};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use orishu::client::ClusterAddress;

const ENV_HELP: &str = "\
ENVIRONMENT:
    WGPU_BACKEND        vulkan | gl | metal | dx12 (comma-separated list)
    ICED_PRESENT_MODE   vsync | no_vsync | fifo | fifo_relaxed | mailbox | immediate
    RUST_LOG            e.g. kagami=debug";

#[derive(Parser)]
#[command(
    name = "kagami",
    version,
    about = "Kagami — a native UI for authoring and viewing physical-field scenes",
    after_help = ENV_HELP
)]
struct Cli {
    /// Headless product operations (no window or MCP listener).
    #[command(subcommand)]
    command: Option<Command>,
    /// Orishu cluster address: a Unix socket path, IP:port, or hostname with optional port.
    /// Defaults to the local per-user Orishu worker socket, or — on platforms without
    /// Unix domain sockets — to the local worker's TCP endpoint on 127.0.0.1:6680.
    #[arg(
        short = 'H',
        long,
        value_parser = ClusterAddress::parse,
        env = "ORISHU_HOST"
    )]
    host: Option<ClusterAddress>,

    /// Open the window normally, then quit on its own after SECONDS. Use
    /// this for automated testing, or the first time you try a windowed
    /// run on a machine where one has previously misbehaved.
    #[arg(long, value_name = "SECONDS")]
    exit_after: Option<f64>,

    /// Open this scene file at startup instead of the built-in demo scene.
    /// Fails to start, rather than falling back to the demo scene, if the
    /// file does not exist.
    #[arg(value_name = "SCENE")]
    scene: Option<PathBuf>,

    /// Enable the embedded MCP server from startup, on the loopback endpoint
    /// 127.0.0.1:8642. The endpoint and a fresh bearer token are printed to
    /// the console. The server is off unless this flag is passed or it is
    /// enabled from the window; it never binds a non-loopback address.
    #[arg(long)]
    mcp: bool,
    /// Local plugin inventory (same path as `plugin --directory`).
    #[arg(long)]
    plugin_directory: Option<PathBuf>,
    /// Enable a logical plugin for this invocation only; repeat for several.
    #[arg(long)]
    enable_plugin: Vec<orishu_plugin::PluginId>,
    /// Disable every release of a logical plugin for this invocation only.
    #[arg(long)]
    disable_plugin: Vec<orishu_plugin::PluginId>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Manage locally built simulation plugins.
    Plugin(kagami::plugins::cli::PluginArgs),
    /// Export a saved captured experiment into a new portable workload bundle.
    Export(kagami::export::ExportArgs),
}

fn main() -> ExitCode {
    env_logger::init();

    let cli = Cli::parse();

    if let Some(command) = cli.command {
        if cli.scene.is_some() || cli.exit_after.is_some() || cli.mcp {
            eprintln!("Error: window/scene/MCP options do not apply to headless commands");
            return ExitCode::FAILURE;
        }
        if cli.plugin_directory.is_some()
            || !cli.enable_plugin.is_empty()
            || !cli.disable_plugin.is_empty()
        {
            eprintln!(
                "Error: startup plugin options do not apply to headless commands; put directory/override options after the command"
            );
            return ExitCode::FAILURE;
        }
        return match command {
            Command::Plugin(args) => kagami::plugins::cli::run(args),
            Command::Export(args) => kagami::export::run(args),
        };
    }

    if let Some(path) = &cli.scene
        && !path.exists()
    {
        eprintln!("Error: scene file not found: {}", path.display());
        return ExitCode::FAILURE;
    }

    let plugins = match startup_plugins(cli.plugin_directory, cli.enable_plugin, cli.disable_plugin)
    {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Error loading plugins: {error}");
            return ExitCode::FAILURE;
        }
    };
    let options = LaunchOptions {
        cluster_address: cli.host.unwrap_or_default(),
        exit_after: cli
            .exit_after
            .map(|seconds| Duration::from_secs_f64(seconds.max(0.1))),
        open_path: cli.scene,
        mcp: cli.mcp,
        plugin_schemas: plugins.schemas,
        plugin_notice: plugins.notice,
        #[cfg(unix)]
        scientific_plugins: plugins.scientific,
        #[cfg(unix)]
        kernel_choices: plugins.kernels,
    };

    // AutoVsync (iced's default) blocks each frame on the compositor's
    // vblank signal, which hangs on a DisplayLink (evdi) output. See
    // docs/troubleshooting-graphics.md. `ICED_PRESENT_MODE` still overrides
    // this if set.
    let settings = iced::Settings {
        vsync: false,
        ..iced::Settings::default()
    };

    let result = iced::application(
        move || Model::new(options.clone()),
        update::update,
        view::view,
    )
    .title(Model::title)
    .subscription(subscription::subscription)
    .settings(settings)
    .run();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

struct StartupPlugins {
    schemas: kagami_catalog::SchemaRegistry,
    notice: Option<String>,
    #[cfg(unix)]
    scientific: Option<kagami::scientific_effect::ScientificPlugins>,
    #[cfg(unix)]
    kernels: Vec<kagami::plugins::KernelChoice>,
}

fn startup_plugins(
    directory: Option<PathBuf>,
    enabled: Vec<orishu_plugin::PluginId>,
    disabled: Vec<orishu_plugin::PluginId>,
) -> Result<StartupPlugins, String> {
    #[cfg(unix)]
    {
        let directory = directory
            .map_or_else(kagami::plugins::default_directory, Ok)
            .map_err(|e| e.to_string())?;
        let store = kagami::plugins::PluginStore::open(&directory).map_err(|e| e.to_string())?;
        let overrides: Vec<_> = enabled
            .into_iter()
            .map(|p| (p, true))
            .chain(disabled.into_iter().map(|p| (p, false)))
            .collect();
        let available = store
            .available_components(&overrides)
            .map_err(|e| e.to_string())?;
        let notice = (!available.unavailable.is_empty()).then(|| format!("{} plugin component contribution(s) require dependency selection or repair and are unavailable. No substitute provider was selected.", available.unavailable.len()));
        let models = store
            .available_models(&overrides)
            .map_err(|e| e.to_string())?;
        if models.revision != available.revision {
            return Err("Plugin inventory changed during startup; retry explicitly.".into());
        }
        Ok(StartupPlugins {
            kernels: models.kernels,
            schemas: available.schemas,
            notice,
            scientific: Some(kagami::scientific_effect::ScientificPlugins {
                store: std::sync::Arc::new(store),
                revision: available.revision,
                overrides,
            }),
        })
    }
    #[cfg(not(unix))]
    {
        if directory.is_some() || !enabled.is_empty() || !disabled.is_empty() {
            return Err(
                "secure local plugin inventory IO is not yet implemented on this platform".into(),
            );
        }
        Ok(StartupPlugins {
            schemas: Default::default(),
            notice: Some("Local plugin inventory is not supported on this platform yet.".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headless_export_with_exact_inventory_guard() {
        let cli = Cli::try_parse_from([
            "kagami",
            "export",
            "scene.kagami",
            "--output",
            "scene.orishu",
            "--name",
            "gravity",
            "--expected-inventory-revision",
            "42",
            "--directory",
            "/tmp/plugins",
            "--enable-plugin",
            "org.example.gravity",
            "--json",
        ])
        .unwrap();
        let Some(Command::Export(args)) = cli.command else {
            panic!("export command")
        };
        assert_eq!(args.experiment, PathBuf::from("scene.kagami"));
        assert_eq!(args.output, PathBuf::from("scene.orishu"));
        assert_eq!(args.expected_inventory_revision, Some(42));
        assert_eq!(args.enable_plugin[0].as_str(), "org.example.gravity");
        assert!(args.json);
        for args in [
            vec!["kagami", "export", "scene.kagami"],
            vec![
                "kagami",
                "export",
                "scene.kagami",
                "--output",
                "scene.orishu",
            ],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn parses_orishu_cluster_address() {
        let cli = Cli::try_parse_from(["kagami", "--host", "cluster.example:9000"])
            .expect("a valid Orishu host should parse");

        assert_eq!(
            cli.host,
            Some(ClusterAddress::Host {
                host: "cluster.example".to_owned(),
                port: 9000,
            })
        );
    }

    #[test]
    fn parses_process_only_plugin_overrides_without_management_command() {
        let cli = Cli::try_parse_from([
            "kagami",
            "--plugin-directory",
            "/tmp/plugins",
            "--enable-plugin",
            "org.example.a",
            "--disable-plugin",
            "org.example.b",
        ])
        .unwrap();
        assert_eq!(cli.enable_plugin[0].as_str(), "org.example.a");
        assert_eq!(cli.disable_plugin[0].as_str(), "org.example.b");
        assert!(cli.command.is_none());
        assert!(Cli::try_parse_from(["kagami", "--enable-plugin", "bad_name"]).is_err());
    }
}
