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
//!   `tests/queues.rs` asserts that no client-local message can produce an
//!   envelope.

use std::path::PathBuf;

use kagami_catalog::{ComponentTypeId, PropertyName};
use kagami_document::ObjectId;

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
}

/// One thing the window was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Something the authority decides.
    Authoritative(Authoritative),
    /// Something the window decides.
    Local(ClientLocal),
    /// Quit.
    Exit,
    /// Periodic tick used to notice `--exit-after`'s deadline has passed.
    ExitTimerTick,
}

impl Message {
    /// The authoritative intent this message carries, if any.
    ///
    /// The one place a message becomes a candidate for an envelope. A
    /// client-local message answers `None` here — that is the property
    /// `tests/queues.rs` checks, and it is checkable precisely because the
    /// mapping is a function rather than a `match` arm buried in `update`.
    pub fn intent(&self) -> Option<&Authoritative> {
        match self {
            Self::Authoritative(intent) => Some(intent),
            Self::Local(_) | Self::Exit | Self::ExitTimerTick => None,
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
