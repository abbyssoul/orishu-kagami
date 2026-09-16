//! The embedded, loopback-only MCP server (ADR 0006).
//!
//! Kept app-local, not a workspace crate: the server has exactly one caller,
//! and `crates/kagami-session`'s dependency budget forbids the async runtime,
//! network, and MCP SDK this needs (its `tests/dependencies.rs` enforces that).
//! The migration admission test in `docs/migration.md` says a module stays
//! inside its only caller until a second one exists.
//!
//! This server is a **transport over the one live session the UI drives**, not
//! a second authority (ADR 0006). In this build (slices 1–7) it exposes the
//! server lifecycle — enable/disable in the running app, `--mcp` at startup,
//! per-enable bearer credentials, connected-client visibility — and one honest,
//! read-only tool, `kagami_status`. The authoring and run-control tools grow
//! with the authorities they command (slices 8–9); they are absent here, not
//! stubbed.
//!
//! The pieces:
//!
//! - [`session`] — the serializable projection of the live session MCP reads.
//! - [`server`] — the rmcp handler and the `kagami_status` tool.
//! - [`transport`] — HTTP bind/serve, bearer auth, connection counting.
//! - [`embed`] — the dedicated-thread runtime and the lifecycle state the app
//!   model holds.

pub mod embed;
pub mod server;
pub mod session;
pub mod transport;

pub use embed::{McpRunning, McpState, enable, report_startup};
pub use session::SessionState;
pub use transport::DEFAULT_ADDR;
