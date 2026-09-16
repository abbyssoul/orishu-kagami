//! Embedding the server in the running app.
//!
//! Following Field CAD's desktop pattern: enabling spawns a dedicated OS thread
//! with its own minimal current-thread tokio runtime, sharing the session
//! projection through an `Arc<std::sync::Mutex<_>>` with the synchronous iced
//! loop. A bounded ready-channel reports bind success or failure so the UI
//! thread waits at most `BIND_TIMEOUT` and never blocks on the serve loop; a
//! fatal-channel reports a server that died after a successful bind; a
//! [`CancellationToken`] stops the server on disable.
//!
//! A failed MCP start never blocks the app: [`enable`] returns
//! [`McpState::Failed`] with a reason, MCP stays off, and authoring and
//! visualization remain fully usable.

use std::io;
use std::net::SocketAddr;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::document::Document;
use crate::mcp::server::McpServer;
use crate::mcp::session::SessionState;
use crate::mcp::transport::{self, McpConnections};

/// How long the UI thread waits for the server to confirm it bound.
const BIND_TIMEOUT: Duration = Duration::from_secs(2);

/// The server's lifecycle state, as the app model holds it.
pub enum McpState {
    /// No server; no port bound (the default).
    Disabled,
    /// A server bound and serving.
    Running(McpRunning),
    /// A start or a running server that failed, with a reason to show.
    Failed(String),
}

impl McpState {
    /// The running server, if any, for the UI to read.
    pub fn running(&self) -> Option<&McpRunning> {
        match self {
            Self::Running(running) => Some(running),
            Self::Disabled | Self::Failed(_) => None,
        }
    }
}

/// A bound, serving MCP server and everything the UI needs to show it.
pub struct McpRunning {
    /// The bearer credential. Shown masked; copied in full on request.
    token: String,
    /// The bound endpoint.
    addr: SocketAddr,
    /// Stops the server when cancelled.
    ct: CancellationToken,
    /// Reports a server that died after binding.
    fatal: mpsc::Receiver<String>,
    /// The live session table, for the connected-client count.
    connections: McpConnections,
    /// The projection MCP reads; rewritten from the live document each update.
    session: Arc<Mutex<SessionState>>,
    /// The count last observed by [`Self::poll`]. `None` means "checking".
    connection_count: Option<usize>,
    /// Whether a disable is waiting for the user to confirm losing clients.
    awaiting_disable_confirm: bool,
}

/// Enable a server bound to `addr` over the given initial session projection.
///
/// The projection is populated before the server starts, so the first
/// `kagami_status` already reflects the live document. Returns
/// [`McpState::Running`] on a successful bind, otherwise [`McpState::Failed`].
pub fn enable(initial: SessionState, addr: SocketAddr) -> McpState {
    let session = Arc::new(Mutex::new(initial));
    let connections = McpConnections::new();
    let token = transport::generate_token();
    let ct = CancellationToken::new();

    let (ready_tx, ready_rx) = mpsc::channel::<Result<SocketAddr, String>>();
    let (fatal_tx, fatal_rx) = mpsc::channel::<String>();

    let thread_session = session.clone();
    let thread_connections = connections.clone();
    let thread_token = token.clone();
    let thread_ct = ct.clone();

    let spawned = std::thread::Builder::new()
        .name("kagami-mcp".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("starting the MCP runtime: {error}")));
                    return;
                }
            };

            runtime.block_on(async move {
                let listener = match transport::bind_http(addr).await {
                    Ok(listener) => listener,
                    Err(error) => {
                        let _ = ready_tx.send(Err(bind_error_message(addr, &error)));
                        return;
                    }
                };
                // Ready carries the address actually bound (the real port even
                // when `:0` was requested), sent only after a successful bind,
                // so the UI learns success or failure but never waits on the
                // serve loop.
                let bound = match listener.local_addr() {
                    Ok(bound) => bound,
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!("reading the bound address: {error}")));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(bound));

                let server = McpServer::new(thread_session);
                if let Err(error) = transport::serve_http(
                    listener,
                    server,
                    thread_token,
                    thread_connections,
                    thread_ct,
                )
                .await
                {
                    let _ = fatal_tx.send(error);
                }
            });
        });

    if let Err(error) = spawned {
        ct.cancel();
        return McpState::Failed(format!("starting the MCP thread: {error}"));
    }

    match ready_rx.recv_timeout(BIND_TIMEOUT) {
        Ok(Ok(bound)) => McpState::Running(McpRunning {
            token,
            addr: bound,
            ct,
            fatal: fatal_rx,
            connections,
            session,
            connection_count: Some(0),
            awaiting_disable_confirm: false,
        }),
        Ok(Err(reason)) => {
            ct.cancel();
            McpState::Failed(reason)
        }
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            ct.cancel();
            McpState::Failed(format!(
                "the MCP server did not confirm startup within {BIND_TIMEOUT:?}"
            ))
        }
    }
}

/// Report a startup-enabled server's endpoint and token to the console.
///
/// For the `--mcp` path, where there may be no visible window to read the
/// masked token from. The token is printed in full here **by design**, so an
/// operator can hand it to a client; in the UI it stays masked. A failure is
/// reported too, and the app still opens.
pub fn report_startup(state: &McpState) {
    match state {
        McpState::Running(running) => {
            eprintln!(
                "MCP server enabled at {} (bearer token: {})",
                running.endpoint(),
                running.token(),
            );
        }
        McpState::Failed(reason) => {
            eprintln!("MCP server failed to start: {reason}");
        }
        McpState::Disabled => {}
    }
}

/// A bind error, naming the likely cause of the common "address in use" case.
fn bind_error_message(addr: SocketAddr, error: &io::Error) -> String {
    if error.kind() == io::ErrorKind::AddrInUse {
        format!(
            "binding {addr}: address already in use — another MCP server may be running, \
             or a just-disabled one has not released the port yet; try again in a moment"
        )
    } else {
        format!("binding {addr}: {error}")
    }
}

impl McpRunning {
    /// The bound endpoint URL a client connects to.
    pub fn endpoint(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// The bound socket address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The full bearer credential, for the copy action.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The credential with only a short prefix shown.
    pub fn masked_token(&self) -> String {
        mask_token(&self.token)
    }

    /// The connected-client count, or `None` while momentarily "checking".
    pub fn connection_count(&self) -> Option<usize> {
        self.connection_count
    }

    /// Whether a disable is awaiting confirmation because clients are connected.
    pub fn awaiting_disable_confirm(&self) -> bool {
        self.awaiting_disable_confirm
    }

    /// Ask for confirmation before disabling with clients connected.
    pub fn request_disable_confirm(&mut self) {
        self.awaiting_disable_confirm = true;
    }

    /// Abandon a pending disable confirmation.
    pub fn cancel_disable_confirm(&mut self) {
        self.awaiting_disable_confirm = false;
    }

    /// Rewrite the projection MCP reads from the live document.
    ///
    /// Called after the document may have changed, so `kagami_status` always
    /// reports the revision, dirty state, and mode the window last drew.
    pub fn refresh(&self, document: &Document) {
        *self.session.lock().unwrap_or_else(PoisonError::into_inner) =
            SessionState::from_document(document);
    }

    /// Refresh the count and check liveness.
    ///
    /// # Errors
    ///
    /// Returns the failure reason if the server thread stopped after a
    /// successful bind, so the caller can move to [`McpState::Failed`].
    pub fn poll(&mut self) -> Result<(), String> {
        if let Some(count) = self.connections.count() {
            self.connection_count = Some(count);
        }
        match self.fatal.try_recv() {
            Ok(reason) => Err(reason),
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => {
                Err("the MCP server thread stopped unexpectedly".to_owned())
            }
        }
    }

    /// Stop the server. The bound port is released as the serve loop unwinds.
    pub fn disable(&self) {
        self.ct.cancel();
    }
}

/// Show only a short prefix of the credential, masking the rest.
fn mask_token(token: &str) -> String {
    const SHOWN: usize = 8;
    let shown: String = token.chars().take(SHOWN).collect();
    if token.chars().count() > SHOWN {
        format!("{shown}…{}", "•".repeat(8))
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_masked_to_a_short_prefix() {
        let masked = mask_token("1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d");
        assert!(masked.starts_with("1a2b3c4d"));
        assert!(!masked.contains("5e6f"));
    }

    #[test]
    fn address_in_use_is_named_as_the_likely_cause() {
        let error = io::Error::new(io::ErrorKind::AddrInUse, "in use");
        let message = bind_error_message(transport::DEFAULT_ADDR, &error);
        assert!(message.contains("address already in use"));
    }
}
