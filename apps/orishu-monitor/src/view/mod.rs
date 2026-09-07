//! Drawing the model.
//!
//! Rendering is a projection: it takes `&Model` and writes cells. It never
//! moves a selection, consumes an event, starts a refresh, or performs IO, so
//! drawing the same model twice produces the same frame and drawing it once
//! changes nothing the next message will see.
//!
//! The layout is computed from the frame area alone, with saturating
//! arithmetic, so there is no size at which a panel underflows or a constraint
//! is invalid. Terminals below [`crate::model::MIN_COLUMNS`] by
//! [`crate::model::MIN_ROWS`] get [`render_too_small`] instead of the panels.

pub mod help;
pub mod members;
pub mod overview;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::keys;
use crate::model::{Model, Section, SizeClass};
use theme::Theme;

/// The name the shell presents itself under.
pub const APPLICATION_NAME: &str = "orishu-monitor";

/// Draw `model` into `frame`.
///
/// The single entry point for rendering, in production and in tests alike.
pub fn render(frame: &mut Frame, model: &Model) {
    let theme = Theme::dark();
    let area = frame.area();

    if model.size_class() == SizeClass::TooSmall {
        render_too_small(frame, area, theme);
        return;
    }

    let [header, body, status, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(frame, header, model, theme);
    match model.section() {
        Section::Overview => overview::render(frame, body, model, theme),
        Section::Members => members::render(frame, body, model, theme),
    }
    render_status(frame, status, model, theme);
    render_footer(frame, footer, theme);

    // Last, so it covers the panels rather than being covered by them.
    if model.help_visible() {
        help::render(frame, area, model, theme);
    }
}

/// The compact fallback for a terminal with no room for the panels.
///
/// Deliberately a plain paragraph over the whole area, one short phrase per
/// row: it has to remain readable in the terminal that triggered it, which is
/// narrower than [`crate::model::MIN_COLUMNS`], so no row may rely on width the
/// caller has just been told it does not have. Rows past the end are simply not
/// drawn, which is what makes this safe down to a single cell.
pub fn render_too_small(frame: &mut Frame, area: Rect, theme: Theme) {
    let lines = vec![
        Line::styled("too small", theme.too_small()),
        Line::styled(
            format!(
                "needs {}x{}",
                crate::model::MIN_COLUMNS,
                crate::model::MIN_ROWS
            ),
            theme.body(),
        ),
        Line::styled(format!("have {}x{}", area.width, area.height), theme.body()),
        Line::styled("resize, or", theme.status()),
        Line::styled("q to quit", theme.status()),
    ];

    frame.render_widget(Paragraph::new(lines), area);
}

/// The application identity and the section tabs.
fn render_header(frame: &mut Frame, area: Rect, model: &Model, theme: Theme) {
    let mut spans = vec![
        Span::styled(APPLICATION_NAME, theme.header()),
        Span::styled("  ", theme.body()),
    ];

    for (index, section) in Section::ALL.iter().enumerate() {
        let style = if *section == model.section() {
            theme.active_tab()
        } else {
            theme.inactive_tab()
        };
        spans.push(Span::styled(
            format!(" {}:{} ", index + 1, section.title()),
            style,
        ));
        spans.push(Span::raw(" "));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The status area: informational text about the shell's own state.
fn render_status(frame: &mut Frame, area: Rect, model: &Model, theme: Theme) {
    let line = Line::from(vec![
        Span::styled("status ", theme.label()),
        Span::styled(model.integration().summary(), theme.unavailable()),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

/// The one-line key hint, built from the same table the help overlay renders.
fn render_footer(frame: &mut Frame, area: Rect, theme: Theme) {
    let mut spans = Vec::new();
    for binding in footer_bindings() {
        if !spans.is_empty() {
            spans.push(Span::styled("  ", theme.hint_action()));
        }
        spans.push(Span::styled(binding.keys_label(), theme.hint_key()));
        spans.push(Span::styled(
            format!(" {}", binding.action()),
            theme.hint_action(),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The bindings worth a permanent hint: how to move, how to get help, and how
/// to leave. The rest are in the overlay `?` opens.
fn footer_bindings() -> impl Iterator<Item = &'static keys::Binding> {
    keys::BINDINGS.iter().filter(|binding| {
        matches!(
            binding.message(),
            crate::message::Message::NextSection
                | crate::message::Message::ToggleHelp
                | crate::message::Message::Quit
        )
    })
}

/// A `width` x `height` rectangle centred in `area`, clamped to fit.
///
/// Saturating throughout, so an overlay asked for more room than exists is
/// simply given `area` rather than a rectangle that starts outside it.
pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);

    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_centred_rectangle_sits_inside_its_area() {
        let area = Rect::new(0, 0, 100, 30);
        let centred = centered(area, 40, 10);

        assert_eq!(centred, Rect::new(30, 10, 40, 10));
    }

    /// An overlay bigger than the terminal must be clamped, not offset off the
    /// screen.
    #[test]
    fn a_rectangle_larger_than_its_area_is_clamped_to_it() {
        let area = Rect::new(0, 0, 10, 4);

        assert_eq!(centered(area, 80, 20), area);
        assert_eq!(
            centered(Rect::new(0, 0, 0, 0), 80, 20),
            Rect::new(0, 0, 0, 0)
        );
    }

    #[test]
    fn the_footer_hints_at_navigation_help_and_quitting() {
        let actions: Vec<_> = footer_bindings().map(keys::Binding::action).collect();

        assert_eq!(actions, ["next section", "toggle this help", "quit"]);
    }
}
