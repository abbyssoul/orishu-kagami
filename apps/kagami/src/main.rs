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
    /// Orishu cluster address: a Unix socket path, IP:port, or hostname with optional port.
    /// Defaults to the local per-user Orishu worker socket.
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
}

fn main() -> ExitCode {
    env_logger::init();

    let cli = Cli::parse();

    if let Some(path) = &cli.scene
        && !path.exists()
    {
        eprintln!("Error: scene file not found: {}", path.display());
        return ExitCode::FAILURE;
    }

    let options = LaunchOptions {
        cluster_address: cli.host.unwrap_or_default(),
        exit_after: cli
            .exit_after
            .map(|seconds| Duration::from_secs_f64(seconds.max(0.1))),
        open_path: cli.scene,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
