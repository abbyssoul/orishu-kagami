//! What the window can ask for, split by who decides it.
//!
//! `TODO.md` records the Doom 3 framing this comes from: a client produces
//! commands and the server is authoritative about whether they applied.
//! Conflating the two queues is the failure mode, so they are separate types
//! here and [`Message::intent`] is the only place either is produced.
//!
//! - [`Authoritative`] reaches [`crate::document::Document`] as an envelope.
//!   Whether it applied is the authority's answer, not the window's.
//! - [`ClientLocal`] never leaves the window. Selection, expansion, search,
//!   which menu is open: none of it is experiment intent (ADR 0012), and
//!   `tests/authoring.rs` asserts that no client-local message can produce an
//!   envelope.
//! - [`WorkspaceIntent`] is neither. A mode change is decided by
//!   `kagami_session::Workspace`, produces no envelope, and yields a
//!   consequence the shell carries out (ADR 0022).
//!
//! # Where the camera sits in that split
//!
//! Camera and projection are client-local: they produce no envelope and the
//! authority never hears about them. But they are not *ephemeral*. In
//! Authoring mode they advance the separate view revision and mark the file
//! dirty; in Observation/replay they change a throwaway copy and dirty
//! nothing. Which of the two happens is decided in [`crate::update`], by the
//! mode — never by the message.

use std::path::PathBuf;

use kagami_catalog::{ComponentTypeId, PropertyName};
use kagami_document::ObjectId;
use kagami_session::{CameraMotion, Projection, RunAttachment, SceneScale};

use crate::model::{Menu, Tool};

/// A change to the experiment. Decided by the authority.
#[derive(Debug, Clone, PartialEq)]
pub enum Authoritative {
    /// Add an object with a default name.
    CreateObject,
    /// Remove the selected object.
    RemoveObject(ObjectId),
    /// Rename an object.
    RenameObject(ObjectId, String),
    /// Attach a component the installed schemas declare.
    AttachComponent(ObjectId, ComponentTypeId),
    /// Detach a component.
    DetachComponent(ObjectId, ComponentTypeId),
    /// Commit an edited property, as the text the user typed.
    SetProperty {
        /// Which object.
        object: ObjectId,
        /// Which of its components.
        component: ComponentTypeId,
        /// Which property.
        property: PropertyName,
        /// What the user wrote. An expression, not a number.
        source: String,
    },
    /// Reverse the last accepted edit.
    Undo,
    /// Reapply the last reversed edit.
    Redo,
    /// Start a new experiment, having decided about unsaved changes.
    New {
        /// Whether the user chose to discard unsaved changes.
        discard_unsaved: bool,
    },
    /// Open a document, having decided about unsaved changes.
    Open {
        /// The file to read.
        path: PathBuf,
        /// Whether the user chose to discard unsaved changes.
        discard_unsaved: bool,
    },
    /// Write the experiment, to its own file or a chosen one.
    Save {
        /// `None` saves to the current target.
        path: Option<PathBuf>,
    },
}

/// A change to what the window is showing. Decided here.
///
/// None of this is experiment intent, so none of it advances a revision,
/// dirties the document, or enters undo (ADR 0012).
#[derive(Debug, Clone, PartialEq)]
pub enum ClientLocal {
    /// Which object the inspector describes.
    Select(Option<ObjectId>),
    /// Show or hide a subtree.
    ToggleExpanded(ObjectId),
    /// Hide an object in the viewport. Presentation, not authored state.
    ToggleHidden(ObjectId),
    /// Filter the tree.
    Search(String),
    /// Which pointer tool is active.
    SelectTool(Tool),
    /// Open or close a menu.
    ToggleMenu(Menu),
    /// Close any open menu.
    CloseMenu,
    /// Open the settings panel.
    OpenSettings,
    /// Close the settings panel.
    CloseSettings,
    /// Type into a property field without committing it.
    EditProperty {
        /// Which object.
        object: ObjectId,
        /// Which of its components.
        component: ComponentTypeId,
        /// Which property.
        property: PropertyName,
        /// The text so far.
        source: String,
    },
    /// Abandon an in-progress property edit.
    CancelPropertyEdit,
    /// Dismiss the status notice.
    DismissNotice,
    /// Move the camera, as a gesture the viewport recognised.
    MoveCamera(CameraMotion),
    /// Choose a projection.
    SetProjection(Projection),
    /// Choose a scene scale, from a preset.
    SetScale(SceneScale),
    /// Type into the metres-per-unit field without committing it.
    ///
    /// Held rather than parsed per keystroke, the same reason a property
    /// expression is: `1 n` is not a scale, and a revision per character would
    /// dirty the file on the way to a value nobody asked for yet.
    EditScale(String),
    /// Commit what was typed into the metres-per-unit field.
    SubmitScale(String),
    /// Abandon an in-progress scale entry.
    CancelScaleEdit,
}

/// A change of workspace mode. Decided by `kagami_session::Workspace`.
///
/// Its own arm because it is neither of the other two: no envelope reaches the
/// authority, and unlike presentation it changes whether authoring is routed
/// at all. ADR 0022 requires the transition to be explicit in both
/// directions, which is why there is a message for it rather than a side
/// effect of something else.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceIntent {
    /// Attach to a run and enter Observation/replay.
    ///
    /// Nothing in this build produces one: submitting a run belongs to K-RUN
    /// and previewing one to K-PREVIEW, and neither exists yet. The transition
    /// is implemented and tested so that when a run authority arrives it has
    /// one explicit way in.
    Observe(RunAttachment),
    /// Return to Authoring, stopping a local preview or detaching from a
    /// remote run.
    EditInitialConditions,
}

/// One thing the window was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Something the authority decides.
    Authoritative(Authoritative),
    /// Something the window decides.
    Local(ClientLocal),
    /// A change of mode.
    Workspace(WorkspaceIntent),
    /// Quit.
    Exit,
    /// Periodic tick used to notice `--exit-after`'s deadline has passed.
    ExitTimerTick,
}

impl Message {
    /// The authoritative intent this message carries, if any.
    ///
    /// The one place a message becomes a candidate for an envelope. Everything
    /// else answers `None` here — that is the property `tests/authoring.rs`
    /// checks, and it is checkable precisely because the mapping is a function
    /// rather than a `match` arm buried in `update`.
    pub fn intent(&self) -> Option<&Authoritative> {
        match self {
            Self::Authoritative(intent) => Some(intent),
            Self::Local(_) | Self::Workspace(_) | Self::Exit | Self::ExitTimerTick => None,
        }
    }
}

impl From<Authoritative> for Message {
    fn from(intent: Authoritative) -> Self {
        Self::Authoritative(intent)
    }
}

impl From<ClientLocal> for Message {
    fn from(local: ClientLocal) -> Self {
        Self::Local(local)
    }
}

impl From<WorkspaceIntent> for Message {
    fn from(intent: WorkspaceIntent) -> Self {
        Self::Workspace(intent)
    }
}

/// The adapter conversion between the renderer's gesture and the window's.
///
/// The shell's job, and the reason `kagami-renderer` and `kagami-session` each
/// keep their own spelling of a camera motion: the persisted contract must not
/// depend on a renderer's representation, and the renderer must not depend on
/// the document server. Six lines here is what buys both.
impl From<kagami_renderer::CameraMotion> for Message {
    fn from(motion: kagami_renderer::CameraMotion) -> Self {
        let motion = match motion {
            kagami_renderer::CameraMotion::Orbit { dx, dy } => CameraMotion::Orbit {
                dx: f64::from(dx),
                dy: f64::from(dy),
            },
            kagami_renderer::CameraMotion::Pan { dx, dy } => CameraMotion::Pan {
                dx: f64::from(dx),
                dy: f64::from(dy),
            },
            kagami_renderer::CameraMotion::Dolly { amount } => CameraMotion::Dolly {
                amount: f64::from(amount),
            },
        };
        ClientLocal::MoveCamera(motion).into()
    }
}
