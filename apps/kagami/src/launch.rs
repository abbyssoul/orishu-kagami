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
    /// Enable the embedded MCP server from startup (`--mcp`), on the default
    /// loopback endpoint. The endpoint and token are reported to the console.
    pub mcp: bool,
    /// Verified enabled component vocabulary supplied by startup IO. Kept out of
    /// `Model::new` so model tests never touch a user's installed plugins.
    pub plugin_schemas: kagami_catalog::SchemaRegistry,
    /// Bounded startup availability notice, never a silent provider substitution.
    pub plugin_notice: Option<String>,
    /// Startup-selected inventory for background scientific authoring effects.
    #[cfg(unix)]
    pub scientific_plugins: Option<crate::scientific_effect::ScientificPlugins>,
    /// Explicit enabled computation choices, not automatically selected kernels.
    #[cfg(unix)]
    pub kernel_choices: Vec<crate::plugins::KernelChoice>,
}
