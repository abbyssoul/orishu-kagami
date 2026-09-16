//! The MCP request handler: one honest, read-only tool over the live session.
//!
//! [`McpServer`] is the rmcp [`ServerHandler`] the transport mounts. It holds
//! the shared [`SessionState`] projection behind an `Arc<std::sync::Mutex<_>>`
//! so the async server thread and the synchronous UI thread lock the same value
//! without either needing the other's runtime. The lock is recovered with
//! [`PoisonError::into_inner`] on every access: a panic reachable from one MCP
//! request must never crash the app on its next frame.
//!
//! Exactly one tool exists in slices 1–7 — `kagami_status`. It proves the whole
//! tool path (schema, invocation, structured result) end-to-end before the
//! domain tools of slices 8–9 depend on it. It is read-only, and it never
//! advances wall-clock time: that is the app's frame loop, not a client
//! decision.

use std::sync::{Arc, Mutex, PoisonError};

use rmcp::{
    ErrorData, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use serde::Serialize;

use crate::mcp::session::SessionState;

/// The handler cloned once per MCP session by the transport's service factory.
///
/// Cloning shares the one [`SessionState`] (the `Arc`) and the tool router, so
/// every session reads the same live projection.
#[derive(Clone)]
pub struct McpServer {
    /// The live session projection, shared with the UI thread that writes it.
    session: Arc<Mutex<SessionState>>,
    /// The generated dispatch table for the `#[tool]` methods below.
    tool_router: ToolRouter<Self>,
}

impl McpServer {
    /// Build a handler over the shared session projection.
    pub fn new(session: Arc<Mutex<SessionState>>) -> Self {
        Self {
            session,
            tool_router: Self::tool_router(),
        }
    }

    /// A cloned snapshot of the session, taken under the poison-recovered lock.
    fn snapshot(&self) -> SessionState {
        self.session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[tool_router]
impl McpServer {
    #[tool(
        description = "Report Kagami's identity and the open experiment's status \
                       (revision, unsaved changes, object count, workspace mode, \
                       and undo/redo availability). Read-only; it changes nothing."
    )]
    async fn kagami_status(&self) -> Result<CallToolResult, ErrorData> {
        ok_json(&self.snapshot())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Kagami's live session over MCP (ADR 0006). This is a transport over the \
             same session the UI drives, not a second authority. The tool surface is \
             read-only in this build; authoring and run-control tools arrive with the \
             authorities they command.",
        )
    }
}

/// Encode a value as the single JSON text block a tool returns on success.
fn ok_json<T: Serialize>(value: &T) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string(value)
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use kagami_catalog::SchemaRegistry;
    use kagami_document::Limits;

    fn server_over_empty_session() -> McpServer {
        let document = Document::new(SchemaRegistry::new(), Limits::DEFAULT);
        let state = SessionState::from_document(&document);
        McpServer::new(Arc::new(Mutex::new(state)))
    }

    #[test]
    fn kagami_status_returns_the_session_as_a_json_text_block() {
        let server = server_over_empty_session();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a current-thread runtime builds");

        let result = runtime
            .block_on(server.kagami_status())
            .expect("the status tool succeeds");

        assert_ne!(result.is_error, Some(true));
        let ContentBlock::Text(block) = result
            .content
            .into_iter()
            .next()
            .expect("one content block")
        else {
            panic!("kagami_status returns a text block");
        };
        let value: serde_json::Value =
            serde_json::from_str(&block.text).expect("the block is JSON");
        assert_eq!(value["app"]["name"], "kagami");
        assert_eq!(value["document"]["revision"], 0);
    }
}
