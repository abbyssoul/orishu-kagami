//! Kagami's document server: the sole mechanism by which an experiment is
//! created or modified.
//!
//! ADR 0004 gives Kagami a document authority, ADR 0006 requires the UI and
//! MCP to reach it identically, and ADR 0019 makes [`DocumentAuthority`] it.
//! A UI gesture, an MCP tool call, an undo, a redo, and a catalog
//! instantiation all become the same [`ExperimentCommandEnvelope`] before
//! anything is decided. No adapter receives mutable access to the model, and
//! successful transport never implies acceptance.
//!
//! ```text
//! apps/kagami        UI, MCP adapter, dialogs      <- imperative shell
//!      |  ExperimentCommandEnvelope   ^ SessionView / ExperimentEvent
//! kagami-session     this crate                    <- the document server
//!      |  update(model, commands)     ^ Rejection
//! kagami-document    model, transition, history    <- pure, sans-IO
//! ```
//!
//! # What this crate owns
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`identity`] | who submitted a command, and which submission it was |
//! | [`command`] | what an adapter submits, and the guards it submits with |
//! | [`outcome`] | what the authority decided, and what a view can catch up on |
//! | [`authority`] | the current revision, the decision, the history, the events |
//! | [`persist`] | which revision is on disk, and where |
//! | [`view`] | what an adapter reads |
//! | [`wire`] | the explicit, versioned shape an adapter converts through |
//!
//! It owns none of: what a valid experiment is (that is `kagami-document`),
//! transports, authentication, tool schemas, presentation state, or any part
//! of a run.
//!
//! # A session, end to end
//!
//! ```
//! # use kagami_catalog::SchemaRegistry;
//! # use kagami_document::{DisplayName, ExperimentCommand, Limits, ObjectSpec};
//! # use kagami_session::{
//! #     ActorId, CommandId, DocumentAuthority, ExperimentChange,
//! #     ExperimentCommandEnvelope, SessionCommand,
//! # };
//! // An installation with no simulation plugins yet: objects can still be
//! // created and placed, they just carry no physics.
//! let mut authority = DocumentAuthority::new(SchemaRegistry::new(), Limits::DEFAULT);
//!
//! let create = ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
//!     DisplayName::new("Earth")?,
//! )));
//! let envelope = ExperimentCommandEnvelope::new(
//!     CommandId::new("ui-1")?,
//!     ActorId::new("ui")?,
//!     SessionCommand::Edit(vec![create]),
//! )
//! // Guarded: composed against the revision the caller read.
//! .guarded_by(authority.revision());
//! let accepted = authority.submit(envelope.clone())?;
//!
//! assert_eq!(authority.snapshot().object_count(), 1);
//! assert!(authority.is_dirty());
//! assert_eq!(authority.history_status().undo.as_deref(), Some("Add object"));
//!
//! // Idempotent: resending the same request replays rather than creating a
//! // second object, which is what lets an adapter retry a lost response
//! // safely.
//! let replayed = authority.submit(envelope)?;
//! assert!(replayed.replayed);
//! assert_eq!(replayed.revision, accepted.revision);
//! assert_eq!(authority.snapshot().object_count(), 1);
//!
//! // Replay answers the request that earned the outcome, not the identity
//! // alone: reusing it for something else is a refusal, not another actor's
//! // result.
//! let reused = authority.submit(ExperimentCommandEnvelope::new(
//!     CommandId::new("ui-1")?,
//!     ActorId::new("agent")?,
//!     SessionCommand::Undo,
//! ));
//! assert_eq!(
//!     reused.expect_err("a different request").code(),
//!     "command_identity_conflict"
//! );
//!
//! // Undo is a command, so the UI and an MCP client share one history.
//! let undone = authority.submit(ExperimentCommandEnvelope::new(
//!     CommandId::new("mcp-1")?,
//!     ActorId::new("agent")?,
//!     SessionCommand::Undo,
//! ))?;
//! assert_eq!(
//!     undone.change,
//!     ExperimentChange::Undone { label: "Add object".to_owned() }
//! );
//! assert_eq!(authority.snapshot().object_count(), 0);
//! // Restoring moved the revision *forward*.
//! assert!(undone.revision > accepted.revision);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![deny(missing_docs)]

pub mod authority;
pub mod command;
pub mod identity;
pub mod outcome;
pub mod persist;
pub mod view;
pub mod wire;

pub use authority::{
    DocumentAuthority, MAX_COMMAND_HISTORY, MAX_EVENT_HISTORY, MAX_REPLAY_COMMANDS,
};
pub use command::{ExperimentCommandEnvelope, SessionCommand};
pub use identity::{ActorId, CommandId, IdentityError, MAX_IDENTITY_BYTES};
pub use outcome::{Acceptance, EventSeq, ExperimentChange, ExperimentEvent, SessionRejection};
pub use persist::{DocumentTarget, MAX_TARGET_BYTES, SaveAcknowledgement, TargetError};
pub use view::{HistoryStatus, SessionView};
pub use wire::{WireAcceptance, WireEnvelope, WireEvent, WireRejection, WireSessionCommand};
