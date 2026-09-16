//! HTTP transport and authentication for the embedded MCP server.
//!
//! Rebuilt against Kagami from Field CAD's reference transport, HTTP-only: no
//! stdio and no Unix socket, because the server is embedded in the GUI app, not
//! spawned by a client. The shape that matters is the `bind`/`serve` split —
//! [`bind_http`] returns the bound [`TcpListener`] so the caller learns bind
//! success and the real address without waiting on the serve loop, and
//! [`serve_http`] then runs until its [`CancellationToken`] fires.
//!
//! Security posture (ADR 0006):
//!
//! - **Loopback only.** [`bind_http`] refuses a non-loopback address outright;
//!   exposing the server beyond this machine is a separate threat-model
//!   decision. rmcp's own `Host`-header allowlist (loopback by default) guards
//!   against DNS-rebinding on top of that.
//! - **Bearer on every request, even on loopback.** Another local process or
//!   user could otherwise reach the session. The token is compared in constant
//!   time and is a fresh per-enable UUID that is never persisted.
//! - **Bounded bodies.** The service caps request bodies so an oversized POST
//!   is rejected rather than buffered without limit.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::mcp::server::McpServer;

/// The default loopback endpoint, matching Field CAD so one agent configuration
/// works for both products.
pub const DEFAULT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8642);

/// The path the Streamable HTTP service is mounted at.
const MOUNT_PATH: &str = "/mcp";

/// The largest request body accepted. Generous for the tiny JSON-RPC envelopes
/// this build handles, but bounded so an oversized POST is refused, not
/// buffered without limit (`GUIDELINES.md`).
const MAX_REQUEST_BODY_BYTES: usize = 1 << 20;

/// Bind the MCP endpoint, refusing any non-loopback address.
///
/// Returns the listener so the caller can read the real bound address (useful
/// when binding an ephemeral `:0` port in tests) before handing it to
/// [`serve_http`].
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] for a non-loopback address, or the
/// OS bind error (for example, address already in use).
pub async fn bind_http(addr: SocketAddr) -> io::Result<TcpListener> {
    if !addr.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to bind the MCP server to non-loopback address {addr}: \
                 the embedded server is loopback-only"
            ),
        ));
    }
    TcpListener::bind(addr).await
}

/// Serve Streamable HTTP on `listener` until `ct` is cancelled.
///
/// Mounts the rmcp service at `/mcp` behind a bearer-token layer, sharing
/// `connections`' session table so the count is live. On cancellation the
/// server shuts down gracefully; outstanding requests fail with a clear result
/// rather than hanging.
///
/// # Errors
///
/// Returns a description if the HTTP server loop ends with an error.
pub async fn serve_http(
    listener: TcpListener,
    server: McpServer,
    token: String,
    connections: McpConnections,
    ct: CancellationToken,
) -> Result<(), String> {
    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(ct.clone())
        .with_max_request_body_bytes(MAX_REQUEST_BODY_BYTES);

    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        connections.sessions.clone(),
        config,
    );

    let router = require_bearer_token(Router::new().nest_service(MOUNT_PATH, service), token);

    let shutdown = {
        let ct = ct.clone();
        async move { ct.cancelled().await }
    };

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|error| format!("MCP HTTP transport ended: {error}"))
}

/// A fresh bearer credential for one enable. A v4 UUID, never persisted.
pub fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Live count of connected MCP sessions, over rmcp's own session table.
///
/// Constructed by the caller *before* the server starts, so the UI can hold it
/// immediately, then handed to [`serve_http`] as the session store. The count
/// is non-blocking: [`Self::count`] returns `None` for the instant a request
/// holds the table's lock — a momentary "checking", not zero. It can also lag a
/// client that vanished without closing its session until the server notices;
/// the UI states that caveat.
#[derive(Clone, Default)]
pub struct McpConnections {
    sessions: Arc<LocalSessionManager>,
}

impl McpConnections {
    /// A fresh, empty connection table.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of connected sessions, or `None` on momentary contention.
    pub fn count(&self) -> Option<usize> {
        self.sessions
            .sessions
            .try_read()
            .ok()
            .map(|sessions| sessions.len())
    }
}

/// Wrap `router` so every request must carry the bearer `token`.
fn require_bearer_token(router: Router, token: String) -> Router {
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let token = token.clone();
            async move {
                let authorized = request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.strip_prefix("Bearer "))
                    .is_some_and(|provided| {
                        constant_time_eq(provided.as_bytes(), token.as_bytes())
                    });

                if authorized {
                    next.run(request).await
                } else {
                    unauthorized()
                }
            }
        },
    ))
}

/// The 401 returned when the bearer token is missing or wrong.
fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        "missing or invalid bearer token",
    )
        .into_response()
}

/// Length-checked, XOR-accumulating byte comparison in time independent of
/// where the first mismatch is. Comparing tokens with `==` would leak their
/// shared prefix length through timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_fresh_per_call() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn constant_time_eq_matches_only_equal_bytes() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn a_fresh_connection_table_counts_zero() {
        assert_eq!(McpConnections::new().count(), Some(0));
    }

    #[test]
    fn binding_a_non_loopback_address_is_refused() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime builds");
        let addr: SocketAddr = "10.0.0.1:8642".parse().expect("a valid address");

        let error = runtime
            .block_on(bind_http(addr))
            .expect_err("a non-loopback bind is refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
