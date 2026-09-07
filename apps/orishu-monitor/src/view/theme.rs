//! Style tokens.
//!
//! Every colour and emphasis the shell uses is named here, so a panel asks for
//! a role ("this is an unavailable state") rather than for a colour. Nothing
//! outside this module constructs a [`Style`].
//!
//! Each token pairs its colour with a [`Modifier`], because colour alone is not
//! a distinction on a monochrome terminal, under `NO_COLOR`, or for a reader
//! who cannot separate the hues. Removing the colour from any token below must
//! still leave the emphasis that carries the meaning.

use ratatui::style::{Color, Modifier, Style};

/// The shell's palette.
///
/// A unit struct rather than a set of free constants: the follow-up
/// live-integration slice will want a second instance for a light terminal, and
/// threading a value is what makes that a parameter rather than a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Theme;

impl Theme {
    /// The default palette.
    pub const fn dark() -> Self {
        Self
    }

    /// The application identity in the header.
    pub fn header(self) -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// The section the body is currently showing.
    pub fn active_tab(self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// A section that can be switched to.
    pub fn inactive_tab(self) -> Style {
        Style::default().fg(Color::Gray)
    }

    /// The frame around a panel.
    pub fn panel_border(self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// A panel's title.
    pub fn panel_title(self) -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// Ordinary body text.
    pub fn body(self) -> Style {
        Style::default().fg(Color::Gray)
    }

    /// A label naming a value.
    pub fn label(self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// A value beside its label.
    pub fn value(self) -> Style {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    /// An honest "there is nothing here, and here is why" state.
    pub fn unavailable(self) -> Style {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::ITALIC)
    }

    /// The status area's informational text.
    pub fn status(self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// The currently selected row of a list.
    pub fn selected_row(self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }

    /// A key in the footer hint or the help overlay.
    pub fn hint_key(self) -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// What a key does, beside the key itself.
    pub fn hint_action(self) -> Style {
        Style::default().fg(Color::Gray)
    }

    /// The help overlay's frame and background.
    pub fn overlay(self) -> Style {
        Style::default().fg(Color::Cyan).bg(Color::Black)
    }

    /// The compact fallback shown when the terminal is too small to lay out.
    pub fn too_small(self) -> Style {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Colour is an enhancement, not the distinction. Stripping every
    /// foreground and background must leave the emphasis that separates a
    /// token from ordinary body text.
    #[test]
    fn distinctions_survive_a_terminal_without_colour() {
        let theme = Theme::dark();

        for (name, style) in [
            ("header", theme.header()),
            ("active_tab", theme.active_tab()),
            ("panel_title", theme.panel_title()),
            ("value", theme.value()),
            ("unavailable", theme.unavailable()),
            ("selected_row", theme.selected_row()),
            ("hint_key", theme.hint_key()),
            ("too_small", theme.too_small()),
        ] {
            assert!(
                !style.add_modifier.is_empty(),
                "{name} is distinguished by colour alone"
            );
        }
    }

    /// The selected row must be visible without any colour support at all, so
    /// it carries no colour to lose.
    #[test]
    fn the_selected_row_is_marked_without_colour() {
        let selected = Theme::dark().selected_row();

        assert_eq!(selected.fg, None);
        assert_eq!(selected.bg, None);
        assert!(selected.add_modifier.contains(Modifier::REVERSED));
    }
}
