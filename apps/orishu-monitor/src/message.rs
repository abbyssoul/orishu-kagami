//! What can happen to the shell, as values.
//!
//! Every input source — a key, a terminal resize — is normalized into one of
//! these before it reaches [`crate::update::update`], so the core never learns
//! which device produced it. Translating a terminal event into a message is the
//! shell's job and lives in [`crate::input`].

use crate::model::{Section, TerminalSize};

/// A single thing that happened, ready to be folded into the model.
///
/// There is no tick variant: nothing in this slice animates or ages, so a
/// periodic message would only produce identical frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// The operator asked to leave the application.
    Quit,
    /// Show the help overlay if it is hidden, hide it if it is shown.
    ToggleHelp,
    /// Close the topmost transient surface, if any.
    Dismiss,
    /// Move to the section after the active one.
    NextSection,
    /// Move to the section before the active one.
    PreviousSection,
    /// Go straight to a named section.
    SelectSection(Section),
    /// Move the selection, or the help overlay's scroll, one row up.
    MoveUp,
    /// Move the selection, or the help overlay's scroll, one row down.
    MoveDown,
    /// The terminal is now this size.
    Resized(TerminalSize),
}
