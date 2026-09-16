//! The serializable projection the MCP server reads.
//!
//! This is the session seam of ADR 0006: a small, serializable snapshot of the
//! **one live session** the window is driving, not a second model. It is
//! written by the UI thread — from [`Document`]'s already-authoritative
//! [`SessionView`](kagami_session::SessionView) — and read by the MCP server
//! thread through an
//! `Arc<std::sync::Mutex<SessionState>>`. It therefore reports the actual open
//! document at the revision the window last drew, never a duplicate mutable
//! model and never placeholder status.
//!
//! Three properties are load-bearing, and are why this is a projection rather
//! than a handle to the authority:
//!
//! - **It is an app-local adapter mechanism, not the document authority's
//!   public representation.** Nothing here is an [`Experiment`], an `ObjectId`,
//!   a camera, or any process-local pointer — only owned, serializable values.
//!   The document authority, the workspace-mode gate, and the client-owned
//!   presentation state stay exactly where they are.
//! - **It is read-only for this task.** Slices 1–7 expose one honest read
//!   (`kagami_status`); the authoring write path that routes typed commands
//!   through [`Document::submit`] is slice 8 and is deliberately absent, not
//!   stubbed.
//! - **It is shaped to grow.** The document authority (slice 8) and run
//!   authorities (slice 9) add fields here without any transport change.
//!
//! [`Experiment`]: kagami_document::Experiment
//! [`Document::submit`]: crate::document::Document::submit

use kagami_session::WorkspaceMode;
use serde::Serialize;

use crate::document::Document;

/// What `kagami_status` returns: who the app is, and the open document's state.
#[derive(Clone, Debug, Serialize)]
pub struct SessionState {
    /// Which application, and which build, is answering.
    pub app: AppIdentity,
    /// The open experiment's status at the last drawn revision.
    pub document: DocumentStatus,
}

impl SessionState {
    /// Project the live session as the MCP server should report it.
    ///
    /// Reads only [`Document`]'s public view and mode; it takes no lock on and
    /// keeps no reference to the authority, so the value can be handed across
    /// the thread boundary and serialized freely.
    pub fn from_document(document: &Document) -> Self {
        let view = document.view();
        let workspace_mode = match document.mode() {
            WorkspaceMode::Authoring => WorkspaceModeReport::Authoring,
            WorkspaceMode::Observing(attachment) => WorkspaceModeReport::Observing {
                run: attachment.run().to_string(),
            },
        };

        Self {
            app: AppIdentity::current(),
            document: DocumentStatus {
                target: view
                    .target
                    .as_ref()
                    .map(|target| target.path().display().to_string()),
                revision: view.revision().get(),
                dirty: view.dirty,
                object_count: view.experiment.object_count(),
                workspace_mode,
                undo: view.history.undo.clone(),
                redo: view.history.redo.clone(),
                history_depth: view.history.depth,
                history_capacity: view.history.capacity,
            },
        }
    }
}

/// Which application and build is answering an MCP request.
#[derive(Clone, Debug, Serialize)]
pub struct AppIdentity {
    /// The product name.
    pub name: &'static str,
    /// The build version, from the crate's `CARGO_PKG_VERSION`.
    pub version: &'static str,
}

impl AppIdentity {
    /// This build's identity.
    pub fn current() -> Self {
        Self {
            name: "kagami",
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// The open experiment's status, as an external client should see it.
///
/// Every field is derived from [`SessionView`](kagami_session::SessionView), so
/// it cannot disagree with what
/// the window is drawing. Slices 8–9 extend this struct; they do not replace it.
#[derive(Clone, Debug, Serialize)]
pub struct DocumentStatus {
    /// The file the experiment is saved to, if it has ever been written.
    pub target: Option<String>,
    /// The experiment revision these contents are.
    pub revision: u64,
    /// `true` when there are unsaved changes of either kind (experiment or the
    /// saved authoring view).
    pub dirty: bool,
    /// How many objects the experiment currently holds.
    pub object_count: usize,
    /// Whether the window is authoring or observing a run (ADR 0022).
    pub workspace_mode: WorkspaceModeReport,
    /// The name of the edit undo would reverse, if any.
    pub undo: Option<String>,
    /// The name of the edit redo would reapply, if any.
    pub redo: Option<String>,
    /// How many entries the undo history currently holds.
    pub history_depth: usize,
    /// The undo history's retention bound.
    pub history_capacity: usize,
}

/// The workspace mode, reported without leaking the run identity type.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceModeReport {
    /// Editing initial conditions; authoring commands are routed.
    Authoring,
    /// Watching a run; no authoring is routed (ADR 0022).
    Observing {
        /// The run being watched, named for the caller.
        run: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use kagami_catalog::SchemaRegistry;
    use kagami_document::Limits;

    #[test]
    fn a_fresh_session_projects_an_empty_authoring_document() {
        let document = Document::new(SchemaRegistry::new(), Limits::DEFAULT);
        let state = SessionState::from_document(&document);

        assert_eq!(state.app.name, "kagami");
        assert_eq!(state.document.revision, 0);
        assert!(!state.document.dirty);
        assert_eq!(state.document.object_count, 0);
        assert!(state.document.target.is_none());
        assert!(matches!(
            state.document.workspace_mode,
            WorkspaceModeReport::Authoring
        ));
    }

    #[test]
    fn the_projection_serializes_to_an_object_with_app_and_document() {
        let document = Document::new(SchemaRegistry::new(), Limits::DEFAULT);
        let state = SessionState::from_document(&document);

        let value = serde_json::to_value(&state).expect("the projection serializes");
        assert_eq!(value["app"]["name"], "kagami");
        assert_eq!(value["document"]["revision"], 0);
        assert_eq!(value["document"]["workspace_mode"]["kind"], "authoring");
    }
}
