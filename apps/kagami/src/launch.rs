use std::path::PathBuf;
use std::time::Duration;

use orishu::client::ClusterAddress;

/// What a caller (currently only `main.rs`'s CLI parsing) can ask of a run
/// before the window exists.
///
#[derive(Clone, Default)]
pub struct LaunchOptions {
    /// Orishu client-plane endpoint selected for this session.
    pub cluster_address: ClusterAddress,
    /// Quit by itself after this long, instead of running until the window
    /// closes — for automated testing on a machine where a windowed run has
    /// previously misbehaved.
    pub exit_after: Option<Duration>,
    /// Open this document at startup instead of the built-in demo scene.
    pub open_path: Option<PathBuf>,
}
