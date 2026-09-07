//! The shell's key bindings, as one table.
//!
//! [`BINDINGS`] is the single authority for what a key does. The help overlay
//! renders it, the footer hint summarises it, and [`crate::input`] dispatches
//! through it — so a key that works but is undocumented, or is documented but
//! does nothing, is not expressible.
//!
//! Bindings are fixed. Operator-configurable shortcuts are deliberately out of
//! scope for this slice.
//!
//! This module names crossterm's [`KeyCode`] and [`KeyModifiers`], which are
//! plain data enums rather than terminal IO. That is what lets one table serve
//! both the pure lookup and the pure render.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::message::Message;
use crate::model::Section;

/// One physical key, with the modifier that distinguishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    code: KeyCode,
    control: bool,
    label: &'static str,
}

impl KeyStroke {
    const fn plain(code: KeyCode, label: &'static str) -> Self {
        Self {
            code,
            control: false,
            label,
        }
    }

    const fn control(code: KeyCode, label: &'static str) -> Self {
        Self {
            code,
            control: true,
            label,
        }
    }

    /// How the key is written in the help overlay.
    pub const fn label(self) -> &'static str {
        self.label
    }

    /// Whether `event` is this key.
    ///
    /// Only Control distinguishes bindings here, so Shift and Alt are ignored:
    /// `?` arrives with Shift held on most layouts, and Shift-Tab arrives as
    /// its own [`KeyCode::BackTab`].
    pub fn matches(self, event: &KeyEvent) -> bool {
        event.code == self.code && event.modifiers.contains(KeyModifiers::CONTROL) == self.control
    }
}

/// A documented key binding: the keys that trigger it, and what it does.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    strokes: &'static [KeyStroke],
    action: &'static str,
    message: Message,
}

impl Binding {
    /// The keys that trigger this binding.
    pub const fn strokes(&self) -> &'static [KeyStroke] {
        self.strokes
    }

    /// A short description of what the binding does.
    pub const fn action(&self) -> &'static str {
        self.action
    }

    /// The message the binding produces.
    pub const fn message(&self) -> Message {
        self.message
    }

    /// The keys joined for display, for example `"Up / k"`.
    pub fn keys_label(&self) -> String {
        self.strokes
            .iter()
            .map(|stroke| stroke.label())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

/// Every key the shell implements, in help-overlay order.
pub const BINDINGS: &[Binding] = &[
    Binding {
        strokes: &[KeyStroke::plain(KeyCode::Tab, "Tab")],
        action: "next section",
        message: Message::NextSection,
    },
    Binding {
        strokes: &[KeyStroke::plain(KeyCode::BackTab, "Shift-Tab")],
        action: "previous section",
        message: Message::PreviousSection,
    },
    Binding {
        strokes: &[KeyStroke::plain(KeyCode::Char('1'), "1")],
        action: "go to Overview",
        message: Message::SelectSection(Section::Overview),
    },
    Binding {
        strokes: &[KeyStroke::plain(KeyCode::Char('2'), "2")],
        action: "go to Members",
        message: Message::SelectSection(Section::Members),
    },
    Binding {
        strokes: &[
            KeyStroke::plain(KeyCode::Up, "Up"),
            KeyStroke::plain(KeyCode::Char('k'), "k"),
        ],
        action: "move up",
        message: Message::MoveUp,
    },
    Binding {
        strokes: &[
            KeyStroke::plain(KeyCode::Down, "Down"),
            KeyStroke::plain(KeyCode::Char('j'), "j"),
        ],
        action: "move down",
        message: Message::MoveDown,
    },
    Binding {
        strokes: &[KeyStroke::plain(KeyCode::Char('?'), "?")],
        action: "toggle this help",
        message: Message::ToggleHelp,
    },
    Binding {
        strokes: &[KeyStroke::plain(KeyCode::Esc, "Esc")],
        action: "close help / back",
        message: Message::Dismiss,
    },
    Binding {
        strokes: &[
            KeyStroke::plain(KeyCode::Char('q'), "q"),
            KeyStroke::control(KeyCode::Char('c'), "Ctrl-C"),
        ],
        action: "quit",
        message: Message::Quit,
    },
];

/// The binding `event` triggers, if any.
///
/// Returns `None` for every unmapped key, which is what lets the runner treat
/// an unknown keystroke as costing no work.
pub fn binding_for(event: &KeyEvent) -> Option<&'static Binding> {
    BINDINGS
        .iter()
        .find(|binding| binding.strokes.iter().any(|stroke| stroke.matches(event)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_binding_documents_at_least_one_key_and_an_action() {
        for binding in BINDINGS {
            assert!(!binding.strokes().is_empty(), "{:?}", binding.message());
            assert!(!binding.action().is_empty(), "{:?}", binding.message());
            assert!(!binding.keys_label().is_empty());
        }
    }

    /// A key claimed by two bindings would dispatch by table order, which is
    /// not something the help overlay could show.
    #[test]
    fn no_key_is_claimed_by_two_bindings() {
        let mut seen = Vec::new();
        for stroke in BINDINGS.iter().flat_map(Binding::strokes) {
            assert!(
                !seen.contains(&(stroke.code, stroke.control)),
                "{} is bound twice",
                stroke.label()
            );
            seen.push((stroke.code, stroke.control));
        }
    }

    /// The table is the help text *and* the dispatcher, so every documented
    /// key must resolve back to the binding that documents it.
    #[test]
    fn every_documented_key_dispatches_to_its_own_binding() {
        for binding in BINDINGS {
            for stroke in binding.strokes() {
                let modifiers = if stroke.control {
                    KeyModifiers::CONTROL
                } else {
                    KeyModifiers::NONE
                };
                let found = binding_for(&KeyEvent::new(stroke.code, modifiers))
                    .expect("a documented key must be dispatchable");
                assert_eq!(found.message(), binding.message(), "{}", stroke.label());
            }
        }
    }

    #[test]
    fn unmapped_keys_and_wrong_modifiers_dispatch_to_nothing() {
        assert!(binding_for(&KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)).is_none());
        assert!(binding_for(&KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)).is_none());
        // `q` quits, but Ctrl-Q is not a binding.
        assert!(binding_for(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)).is_none());
        // Ctrl-C quits, but a bare `c` is not a binding.
        assert!(binding_for(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)).is_none());
    }

    /// Shift is held for `?` on most layouts; it must not defeat the lookup.
    #[test]
    fn shift_does_not_change_which_binding_a_key_triggers() {
        let shifted = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);

        assert_eq!(
            binding_for(&shifted).map(Binding::message),
            Some(Message::ToggleHelp)
        );
    }

    #[test]
    fn keys_are_labelled_for_display() {
        let quit = BINDINGS
            .iter()
            .find(|binding| binding.message() == Message::Quit)
            .expect("quit is a documented binding");

        assert_eq!(quit.keys_label(), "q / Ctrl-C");
    }
}
