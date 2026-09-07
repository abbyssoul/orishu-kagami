//! The help overlay: the authoritative list of implemented keys.
//!
//! It renders [`crate::keys::BINDINGS`], which is also what dispatches input,
//! so the overlay cannot document a key the shell does not implement or omit
//! one it does.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::theme::Theme;
use crate::keys::{self, Binding};
use crate::model::Model;

/// The overlay's title, also used by tests to recognise it.
pub const TITLE: &str = "Help";

/// Width taken by the key column, so the actions line up.
const KEY_COLUMN: usize = 14;

/// Draw the help overlay centred over `area`.
///
/// The overlay is sized to its content but clamped to the terminal, and only as
/// many rows as fit are drawn, starting at the model's help scroll. A short
/// terminal therefore shows part of the list and can scroll to the rest, rather
/// than silently hiding it.
pub fn render(frame: &mut Frame, area: Rect, model: &Model, theme: Theme) {
    let bindings = keys::BINDINGS;
    // Two border rows, plus one row per binding.
    let wanted_height = u16::try_from(bindings.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let wanted_width = u16::try_from(KEY_COLUMN + longest_action() + 4).unwrap_or(u16::MAX);
    let overlay = super::centered(area, wanted_width, wanted_height);

    let block = Block::bordered()
        .border_style(theme.overlay())
        .title(Span::styled(format!(" {TITLE} "), theme.panel_title()));
    let inner = block.inner(overlay);

    // `Clear` is what makes this an overlay rather than a blend with the panel
    // underneath it.
    frame.render_widget(Clear, overlay);
    frame.render_widget(block, overlay);

    let visible = usize::from(inner.height);
    let offset = model
        .help_scroll()
        .resolved(bindings.len())
        .unwrap_or(0)
        .min(bindings.len().saturating_sub(visible));
    let lines: Vec<Line> = bindings
        .iter()
        .skip(offset)
        .take(visible)
        .map(|binding| binding_line(binding, theme))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// One row of the key table: the keys, padded, then what they do.
fn binding_line(binding: &Binding, theme: Theme) -> Line<'static> {
    let keys = binding.keys_label();
    let padding = KEY_COLUMN.saturating_sub(keys.chars().count());

    Line::from(vec![
        Span::styled(keys, theme.hint_key()),
        Span::raw(" ".repeat(padding.max(1))),
        Span::styled(binding.action(), theme.hint_action()),
    ])
}

/// The widest action text, so the overlay is wide enough for all of them.
fn longest_action() -> usize {
    keys::BINDINGS
        .iter()
        .map(|binding| binding.action().chars().count())
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_binding_gets_a_row_with_its_keys_and_its_action() {
        for binding in keys::BINDINGS {
            let line = binding_line(binding, Theme::dark());
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();

            assert!(text.contains(&binding.keys_label()), "{text:?}");
            assert!(text.contains(binding.action()), "{text:?}");
        }
    }

    /// Keys and actions must stay in separate columns even when the key label
    /// is as wide as the column.
    #[test]
    fn a_wide_key_label_still_leaves_a_gap_before_its_action() {
        for binding in keys::BINDINGS {
            let line = binding_line(binding, Theme::dark());
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();

            assert!(
                text.contains(&format!("{} ", binding.keys_label())),
                "{text:?}"
            );
        }
    }
}
