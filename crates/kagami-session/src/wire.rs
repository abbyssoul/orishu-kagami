//! An explicit, versioned representation of what crosses the session
//! boundary.
//!
//! The same boundary [`kagami_document::wire`] draws for the model, one layer
//! up: this module owns the shape of a *submission* and of what the authority
//! answers with. An in-process MCP adapter converts through these types rather
//! than through [`ExperimentCommandEnvelope`] and [`Acceptance`] directly, so
//! the internal types stay free to change and the published shape changes only
//! deliberately.
//!
//! # What this is not
//!
//! - **Not a transport.** Nothing here frames, authenticates, or delivers
//!   anything. An adapter that owns a transport builds it on top.
//! - **Not a collaboration protocol.** ADR 0012 keeps live multi-writer
//!   editing out of the first product. A shape that can be encoded is not a
//!   promise about who may send it, over what, or with what compatibility
//!   guarantee — that needs the decision ADR 0012 defers, not a serde derive.
//! - **Not the experiment file.** Task K4 owns the saved document.
//!
//! # Which direction each type converts
//!
//! [`WireEnvelope`] is *inbound* and converts both ways: an adapter submits.
//! [`WireAcceptance`], [`WireEvent`] and [`WireRejection`] are *outbound*: they
//! encode what the authority decided. They deserialize — a golden fixture has
//! to round-trip, and a client may want to read one — but there is
//! deliberately no path from them back into authority state, because an
//! outcome is something the authority produces, never something it is told.

use kagami_document::{
    CapabilitySummary, ExperimentRevision, ExperimentSnapshot, GestureId, ObjectId, WireCommand,
    WireError,
};
use serde::{Deserialize, Serialize};

use crate::command::{ExperimentCommandEnvelope, SessionCommand};
use crate::identity::{ActorId, CommandId};
use crate::outcome::{Acceptance, ExperimentChange, ExperimentEvent, SessionRejection};

/// The version of the representation in this module.
///
/// Distinct from [`kagami_document::WIRE_VERSION`]: the envelope shape and the
/// command shape are owned by different crates and can move independently.
///
/// Not stamped on every message. A submission and its answer cross one
/// boundary an adapter establishes once, so the version belongs to that
/// boundary; [`kagami_document::WireSnapshot`] carries its version because a
/// read projection can be stored and read back later, when nothing is left to
/// ask.
pub const WIRE_VERSION: u32 = 1;

/// One thing an adapter can ask the document authority to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WireSessionCommand {
    /// Apply a batch of authoring commands as one atomic edit.
    Edit {
        /// The batch. An empty one is refused by the authority, not here: the
        /// message is well-formed, and the rule that an edit must carry a
        /// command has one owner.
        commands: Vec<WireCommand>,
    },
    /// Restore the experiment as it stood before the most recent edit.
    Undo,
    /// Reapply the most recently undone edit.
    Redo,
    /// Open an interactive edit.
    BeginInteractiveEdit,
    /// Close the open interactive edit.
    EndInteractiveEdit,
}

/// A command together with the guards and attribution it was submitted with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEnvelope {
    /// Identity of this submission, for idempotent replay.
    pub command_id: CommandId,
    /// Who submitted it.
    pub actor: ActorId,
    /// The revision the caller composed the command against. Absent opts out
    /// of the guard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<ExperimentRevision>,
    /// The interactive edit this submission joins, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture: Option<u64>,
    /// The command itself.
    pub command: WireSessionCommand,
}

impl WireEnvelope {
    /// Encode an envelope.
    pub fn of(envelope: &ExperimentCommandEnvelope) -> Self {
        Self {
            command_id: envelope.command_id.clone(),
            actor: envelope.actor.clone(),
            expected_revision: envelope.expected_revision,
            gesture: envelope.gesture.map(GestureId::get),
            command: match &envelope.command {
                SessionCommand::Edit(commands) => WireSessionCommand::Edit {
                    commands: commands.iter().map(WireCommand::of).collect(),
                },
                SessionCommand::Undo => WireSessionCommand::Undo,
                SessionCommand::Redo => WireSessionCommand::Redo,
                SessionCommand::BeginInteractiveEdit => WireSessionCommand::BeginInteractiveEdit,
                SessionCommand::EndInteractiveEdit => WireSessionCommand::EndInteractiveEdit,
            },
        }
    }

    /// Decode into an envelope, resolving identities against `snapshot`.
    ///
    /// Resolving against a read the caller has actually taken is what makes an
    /// object identity meaningful; see
    /// [`WireCommand::into_command`](kagami_document::WireCommand::into_command).
    /// A decoded envelope is a *submission*, not an acceptance: the authority
    /// still guards, validates and decides it.
    ///
    /// # Errors
    ///
    /// Returns [`WireError`] for an identity that names no object in
    /// `snapshot`, or a unit the product does not know.
    pub fn into_envelope(
        self,
        snapshot: &ExperimentSnapshot,
    ) -> Result<ExperimentCommandEnvelope, WireError> {
        let command = match self.command {
            WireSessionCommand::Edit { commands } => SessionCommand::Edit(
                commands
                    .into_iter()
                    .map(|command| command.into_command(snapshot))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            WireSessionCommand::Undo => SessionCommand::Undo,
            WireSessionCommand::Redo => SessionCommand::Redo,
            WireSessionCommand::BeginInteractiveEdit => SessionCommand::BeginInteractiveEdit,
            WireSessionCommand::EndInteractiveEdit => SessionCommand::EndInteractiveEdit,
        };
        Ok(ExperimentCommandEnvelope {
            command_id: self.command_id,
            actor: self.actor,
            expected_revision: self.expected_revision,
            gesture: self.gesture.map(GestureId::new),
            command,
        })
    }
}

/// What one accepted command did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WireChange {
    /// A batch was applied as one new revision.
    Edited {
        /// The revision it produced.
        revision: ExperimentRevision,
        /// Identities minted, in creation order.
        created: Vec<u64>,
        /// Identities removed, in removal order.
        removed: Vec<u64>,
        /// What a user would call the edit.
        label: String,
    },
    /// A previously captured experiment was restored.
    Undone {
        /// The name of the edit that was reversed.
        label: String,
    },
    /// A previously undone experiment was restored again.
    Redone {
        /// The name of the edit that was reapplied.
        label: String,
    },
    /// An interactive edit was opened.
    GestureOpened {
        /// The identity to put in the following submissions.
        gesture: u64,
    },
    /// An interactive edit was closed.
    GestureClosed {
        /// Which gesture ended.
        gesture: u64,
        /// `true` when the authority closed it rather than the adapter.
        implicit: bool,
    },
    /// The installed component schemas were replaced.
    CapabilityChanged {
        /// Gaps before.
        before: WireCapabilitySummary,
        /// Gaps after.
        after: WireCapabilitySummary,
    },
}

impl WireChange {
    /// Encode a change.
    pub fn of(change: &ExperimentChange) -> Self {
        match change {
            ExperimentChange::Edited { report } => Self::Edited {
                revision: report.revision,
                created: report
                    .created_objects
                    .iter()
                    .copied()
                    .map(ObjectId::get)
                    .collect(),
                removed: report
                    .removed_objects
                    .iter()
                    .copied()
                    .map(ObjectId::get)
                    .collect(),
                label: report.label.clone(),
            },
            ExperimentChange::Undone { label } => Self::Undone {
                label: label.clone(),
            },
            ExperimentChange::Redone { label } => Self::Redone {
                label: label.clone(),
            },
            ExperimentChange::GestureOpened { gesture } => Self::GestureOpened {
                gesture: gesture.get(),
            },
            ExperimentChange::GestureClosed { gesture, implicit } => Self::GestureClosed {
                gesture: gesture.get(),
                implicit: *implicit,
            },
            ExperimentChange::CapabilityChanged { before, after } => Self::CapabilityChanged {
                before: WireCapabilitySummary::of(*before),
                after: WireCapabilitySummary::of(*after),
            },
        }
    }
}

/// How many components the installed schemas cannot govern.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCapabilitySummary {
    /// Objects holding at least one such component.
    pub objects: usize,
    /// Components whose contributing plugin is not installed.
    pub absent: usize,
    /// Components an installed schema refuses.
    pub incompatible: usize,
}

impl WireCapabilitySummary {
    /// Encode a summary.
    pub const fn of(summary: CapabilitySummary) -> Self {
        Self {
            objects: summary.objects,
            absent: summary.absent,
            incompatible: summary.incompatible,
        }
    }
}

/// The authority accepted a command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireAcceptance {
    /// The submission this answers.
    pub command_id: CommandId,
    /// The revision in force afterwards.
    pub revision: ExperimentRevision,
    /// What happened.
    pub change: WireChange,
    /// The event this appended, for a view catching up.
    pub event: u64,
    /// `true` when this is a recorded outcome replayed for a resent request.
    pub replayed: bool,
}

impl WireAcceptance {
    /// Encode an acceptance.
    pub fn of(acceptance: &Acceptance) -> Self {
        Self {
            command_id: acceptance.command_id.clone(),
            revision: acceptance.revision,
            change: WireChange::of(&acceptance.change),
            event: acceptance.event.get(),
            replayed: acceptance.replayed,
        }
    }
}

/// One published change, for a view catching up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEvent {
    /// This event's position in the published sequence.
    pub seq: u64,
    /// The revision in force after the change.
    pub revision: ExperimentRevision,
    /// The submission that caused it, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<CommandId>,
    /// Who submitted it, or who supplied the change.
    pub actor: ActorId,
    /// What happened.
    pub change: WireChange,
}

impl WireEvent {
    /// Encode an event.
    pub fn of(event: &ExperimentEvent) -> Self {
        Self {
            seq: event.seq.get(),
            revision: event.revision,
            command_id: event.command_id.clone(),
            actor: event.actor.clone(),
            change: WireChange::of(&event.change),
        }
    }
}

/// The authority refused a command.
///
/// Carries the stable [`SessionRejection::code`] and the human explanation
/// separately, because an automation client branches on the first and shows
/// the second. Flattened to those two fields on purpose: a rejection's payload
/// varies by variant, and publishing that structure would freeze every
/// document-level refusal shape as protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRejection {
    /// The stable machine-readable reason.
    pub code: String,
    /// The human explanation.
    pub message: String,
}

impl WireRejection {
    /// Encode a refusal.
    pub fn of(rejection: &SessionRejection) -> Self {
        Self {
            code: rejection.code().to_owned(),
            message: rejection.to_string(),
        }
    }
}
