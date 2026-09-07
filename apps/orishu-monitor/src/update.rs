//! Folding a message into the model.
//!
//! `update` is pure and total: it reads only its arguments, performs no IO, and
//! has an answer for every message in every state. That is what lets the tests
//! below exercise the same transitions the running application uses, with no
//! terminal attached.

use crate::keys;
use crate::message::Message;
use crate::model::Model;

/// The model that results from `message` arriving while `model` is current.
///
/// While the help overlay is open it takes navigation for itself: `Up`/`Down`
/// scroll the key list, and section switching is suspended, so the operator
/// cannot silently change what is behind the overlay. Exit and resize are never
/// suspended — a quit request and the terminal's actual size are true
/// regardless of which surface is on top.
pub fn update(model: Model, message: Message) -> Model {
    match message {
        Message::Quit => model.requesting_exit(),

        Message::Resized(size) => model.with_size(size),

        Message::ToggleHelp => {
            let visible = !model.help_visible();
            model.with_help_visible(visible)
        }

        Message::Dismiss => {
            if model.help_visible() {
                model.with_help_visible(false)
            } else {
                model
            }
        }

        Message::MoveUp => {
            if model.help_visible() {
                let scroll = model.help_scroll().move_up(help_row_count());
                model.with_help_scroll(scroll)
            } else {
                let selection = model.selection().move_up(model.section_row_count());
                model.with_selection(selection)
            }
        }

        Message::MoveDown => {
            if model.help_visible() {
                let scroll = model.help_scroll().move_down(help_row_count());
                model.with_help_scroll(scroll)
            } else {
                let selection = model.selection().move_down(model.section_row_count());
                model.with_selection(selection)
            }
        }

        Message::NextSection => {
            if model.help_visible() {
                model
            } else {
                let next = model.section().next();
                model.with_section(next)
            }
        }

        Message::PreviousSection => {
            if model.help_visible() {
                model
            } else {
                let previous = model.section().previous();
                model.with_section(previous)
            }
        }

        Message::SelectSection(section) => {
            if model.help_visible() {
                model
            } else {
                model.with_section(section)
            }
        }
    }
}

/// The number of scrollable rows the help overlay has.
fn help_row_count() -> usize {
    keys::BINDINGS.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExitIntent, Section, SizeClass, TerminalSize};

    fn shell() -> Model {
        Model::new(TerminalSize::new(100, 30))
    }

    fn fold(model: Model, messages: &[Message]) -> Model {
        messages
            .iter()
            .fold(model, |model, message| update(model, *message))
    }

    #[test]
    fn tab_cycles_forward_through_every_section_and_wraps() {
        let model = shell();
        assert_eq!(model.section(), Section::Overview);

        let model = update(model, Message::NextSection);
        assert_eq!(model.section(), Section::Members);

        let model = update(model, Message::NextSection);
        assert_eq!(model.section(), Section::Overview);
    }

    #[test]
    fn shift_tab_cycles_backward_and_wraps() {
        let model = update(shell(), Message::PreviousSection);
        assert_eq!(model.section(), Section::Members);

        let model = update(model, Message::PreviousSection);
        assert_eq!(model.section(), Section::Overview);
    }

    #[test]
    fn a_section_can_be_selected_directly() {
        let model = update(shell(), Message::SelectSection(Section::Members));
        assert_eq!(model.section(), Section::Members);

        let model = update(model, Message::SelectSection(Section::Overview));
        assert_eq!(model.section(), Section::Overview);
    }

    #[test]
    fn help_opens_and_closes_on_the_same_key() {
        let model = update(shell(), Message::ToggleHelp);
        assert!(model.help_visible());

        let model = update(model, Message::ToggleHelp);
        assert!(!model.help_visible());
    }

    #[test]
    fn escape_closes_help_and_does_nothing_when_no_overlay_is_open() {
        let opened = update(shell(), Message::ToggleHelp);
        let closed = update(opened, Message::Dismiss);
        assert!(!closed.help_visible());

        // Nothing left to dismiss: the state must be untouched, not reset.
        let again = update(closed.clone(), Message::Dismiss);
        assert_eq!(again, closed);
    }

    /// Closing the overlay must not leave a scroll behind for the next time it
    /// is opened.
    #[test]
    fn reopening_help_starts_at_the_top() {
        let model = fold(
            shell(),
            &[
                Message::ToggleHelp,
                Message::MoveDown,
                Message::MoveDown,
                Message::Dismiss,
                Message::ToggleHelp,
            ],
        );

        assert!(model.help_visible());
        assert_eq!(model.help_scroll().index(), 0);
    }

    #[test]
    fn help_scrolling_stays_inside_the_key_list() {
        let mut model = update(shell(), Message::ToggleHelp);
        for _ in 0..(keys::BINDINGS.len() + 10) {
            model = update(model, Message::MoveDown);
        }
        assert_eq!(
            model.help_scroll().resolved(keys::BINDINGS.len()),
            Some(keys::BINDINGS.len() - 1)
        );

        for _ in 0..(keys::BINDINGS.len() + 10) {
            model = update(model, Message::MoveUp);
        }
        assert_eq!(model.help_scroll().index(), 0);
    }

    /// The overlay owns navigation while it is up, so dismissing it reveals
    /// exactly the section the operator left.
    #[test]
    fn the_open_overlay_suspends_section_switching_and_selection() {
        let before = update(shell(), Message::SelectSection(Section::Members));
        let after = fold(
            before.clone(),
            &[
                Message::ToggleHelp,
                Message::NextSection,
                Message::PreviousSection,
                Message::SelectSection(Section::Overview),
                Message::Dismiss,
            ],
        );

        assert_eq!(after, before);
    }

    #[test]
    fn quitting_is_recorded_as_exit_intent_from_any_surface() {
        assert_eq!(
            update(shell(), Message::Quit).exit_intent(),
            ExitIntent::Requested
        );

        let from_help = fold(shell(), &[Message::ToggleHelp, Message::Quit]);
        assert!(from_help.exit_intent().is_requested());
    }

    #[test]
    fn nothing_else_requests_an_exit() {
        let model = fold(
            shell(),
            &[
                Message::NextSection,
                Message::PreviousSection,
                Message::SelectSection(Section::Members),
                Message::ToggleHelp,
                Message::MoveUp,
                Message::MoveDown,
                Message::Dismiss,
                Message::Resized(TerminalSize::new(10, 3)),
            ],
        );

        assert_eq!(model.exit_intent(), ExitIntent::Running);
    }

    #[test]
    fn a_resize_changes_the_size_class_in_both_directions() {
        let model = shell();
        assert_eq!(model.size_class(), SizeClass::Usable);

        let shrunk = update(model, Message::Resized(TerminalSize::new(20, 5)));
        assert_eq!(shrunk.size_class(), SizeClass::TooSmall);
        assert_eq!(shrunk.size(), TerminalSize::new(20, 5));

        let grown = update(shrunk, Message::Resized(TerminalSize::new(120, 40)));
        assert_eq!(grown.size_class(), SizeClass::Usable);
    }

    /// A resize is only a resize: shrinking below the minimum and growing back
    /// must return the operator to the surface they were on.
    #[test]
    fn a_resize_changes_nothing_but_the_size() {
        let before = fold(shell(), &[Message::SelectSection(Section::Members)]);
        let after = fold(
            before.clone(),
            &[
                Message::Resized(TerminalSize::new(20, 5)),
                Message::Resized(TerminalSize::new(100, 30)),
            ],
        );

        assert_eq!(after, before);
    }

    /// A section with nothing in it navigates without moving anything, which
    /// is every section in this slice.
    #[test]
    fn selection_in_an_empty_section_stays_resolved_to_nothing() {
        let model = fold(
            shell(),
            &[Message::MoveDown, Message::MoveDown, Message::MoveUp],
        );

        assert_eq!(model.section_row_count(), 0);
        assert_eq!(model.selection().resolved(0), None);
    }

    /// Every binding must be foldable from every state without panicking, in
    /// both surfaces and at a degenerate size.
    #[test]
    fn every_documented_message_folds_from_every_surface() {
        for binding in keys::BINDINGS {
            for base in [shell(), update(shell(), Message::ToggleHelp)] {
                let tiny = update(base, Message::Resized(TerminalSize::new(0, 0)));
                let _ = update(tiny, binding.message());
            }
        }
    }
}
