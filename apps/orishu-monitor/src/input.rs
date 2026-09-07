//! Turning terminal events into messages.
//!
//! This is the whole of the shell's input side, and it is a pure function of
//! the event: no state, no IO. Keeping it here is what keeps
//! [`crate::update`] free of terminal concepts, and what makes the key map
//! testable by constructing events rather than by pressing keys.

use crossterm::event::{Event, KeyEvent, KeyEventKind};

use crate::keys::{self, Binding};
use crate::message::Message;
use crate::model::TerminalSize;

/// The message `event` means, or `None` if the shell does not act on it.
///
/// Unmapped keys, key releases, mouse input, focus changes, and pastes all
/// return `None`; the runner draws no frame for them, so an input burst the
/// shell has no use for costs nothing but the read.
pub fn message_for(event: &Event) -> Option<Message> {
    match event {
        Event::Key(key) => message_for_key(key),
        Event::Resize(columns, rows) => Some(Message::Resized(TerminalSize::new(*columns, *rows))),
        _ => None,
    }
}

/// The message `key` means, or `None` if it is not a documented binding.
///
/// Key releases are discarded rather than treated as presses: terminals that
/// report them would otherwise act on every binding twice.
fn message_for_key(key: &KeyEvent) -> Option<Message> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    keys::binding_for(key).map(Binding::message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};

    use crate::model::Section;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn each_documented_key_produces_its_message() {
        assert_eq!(
            message_for(&press(KeyCode::Tab)),
            Some(Message::NextSection)
        );
        assert_eq!(
            message_for(&press(KeyCode::BackTab)),
            Some(Message::PreviousSection)
        );
        assert_eq!(
            message_for(&press(KeyCode::Char('1'))),
            Some(Message::SelectSection(Section::Overview))
        );
        assert_eq!(
            message_for(&press(KeyCode::Char('2'))),
            Some(Message::SelectSection(Section::Members))
        );
        assert_eq!(message_for(&press(KeyCode::Up)), Some(Message::MoveUp));
        assert_eq!(
            message_for(&press(KeyCode::Char('k'))),
            Some(Message::MoveUp)
        );
        assert_eq!(message_for(&press(KeyCode::Down)), Some(Message::MoveDown));
        assert_eq!(
            message_for(&press(KeyCode::Char('j'))),
            Some(Message::MoveDown)
        );
        assert_eq!(
            message_for(&press(KeyCode::Char('?'))),
            Some(Message::ToggleHelp)
        );
        assert_eq!(message_for(&press(KeyCode::Esc)), Some(Message::Dismiss));
        assert_eq!(message_for(&press(KeyCode::Char('q'))), Some(Message::Quit));
    }

    #[test]
    fn control_c_quits() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert_eq!(message_for(&event), Some(Message::Quit));
    }

    #[test]
    fn a_resize_carries_the_new_size() {
        assert_eq!(
            message_for(&Event::Resize(120, 40)),
            Some(Message::Resized(TerminalSize::new(120, 40)))
        );
        assert_eq!(
            message_for(&Event::Resize(0, 0)),
            Some(Message::Resized(TerminalSize::new(0, 0)))
        );
    }

    #[test]
    fn unmapped_keys_produce_nothing() {
        assert_eq!(message_for(&press(KeyCode::Char('x'))), None);
        assert_eq!(message_for(&press(KeyCode::F(1))), None);
        assert_eq!(message_for(&press(KeyCode::PageDown)), None);
    }

    /// Terminals in kitty-protocol mode report releases; acting on them would
    /// run every binding twice.
    #[test]
    fn key_releases_are_not_presses() {
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        let repeat = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));

        assert_eq!(message_for(&release), None);
        assert_eq!(message_for(&repeat), Some(Message::Quit));
    }

    /// Mouse capture is not enabled and focus events are not acted on, so both
    /// must fold to nothing rather than to a default message.
    #[test]
    fn other_terminal_events_produce_nothing() {
        let mouse = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(message_for(&mouse), None);
        assert_eq!(message_for(&Event::FocusGained), None);
        assert_eq!(message_for(&Event::FocusLost), None);
        assert_eq!(message_for(&Event::Paste("q".to_owned())), None);
    }
}
