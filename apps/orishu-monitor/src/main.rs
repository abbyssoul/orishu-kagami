//! Binary entry point. Parses the (deliberately tiny) command line, then hands
//! over to [`orishu_monitor::runtime::run`].
//!
//! The only options are `--help` and `--version`. Connection, credential, and
//! output options belong to the later live-integration slice, where they must
//! be designed together with `orishuctl` rather than guessed at here.

use std::process::ExitCode;

use clap::Parser;

use orishu_monitor::runtime;

const ABOUT: &str = "Interactive terminal shell for administering an orishu cluster";

const LONG_ABOUT: &str = "\
Interactive terminal shell for administering an orishu cluster.

This build is the terminal shell only: it does not connect to an orishu worker
and reports no cluster state. Use orishuctl for implemented administration.

Requires a terminal on stdin and stdout; --help and --version do not.";

#[derive(Debug, Parser)]
#[command(
    name = "orishu-monitor",
    version,
    about = ABOUT,
    long_about = LONG_ABOUT
)]
struct Cli {}

fn main() -> ExitCode {
    let Cli {} = Cli::parse();

    match runtime::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The terminal session has already been restored by this point, so
            // stderr is the operator's ordinary terminal again.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }

    /// `--help` and `--version` are the whole surface. In particular there is
    /// no host, credential, config, or log-level option: those are the live
    /// integration's to design alongside the CLI's.
    #[test]
    fn the_command_line_takes_no_options_beyond_help_and_version() {
        assert!(Cli::try_parse_from(["orishu-monitor"]).is_ok());

        for rejected in [
            ["orishu-monitor", "--host", "127.0.0.1:6655"],
            ["orishu-monitor", "-H", "127.0.0.1:6655"],
            ["orishu-monitor", "--config", "/etc/monitor.toml"],
            ["orishu-monitor", "--log-level", "debug"],
        ] {
            assert!(
                Cli::try_parse_from(rejected).is_err(),
                "{rejected:?} should not parse"
            );
        }
    }

    /// Both must work without a terminal, so neither may be routed through the
    /// interactive path.
    #[test]
    fn help_and_version_are_handled_by_the_parser() {
        for flag in ["--help", "--version"] {
            let error = Cli::try_parse_from(["orishu-monitor", flag])
                .expect_err("clap reports help and version as an early exit");

            assert!(matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ));
            assert!(!error.to_string().is_empty());
        }
    }
}
