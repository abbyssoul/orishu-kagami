//! The Members section.
//!
//! The list is genuinely empty: this build asks no worker for a membership
//! projection, so there is nothing to list and nothing is invented. The panel
//! says why, in wording that cannot be mistaken for a connection that was tried
//! and failed.
//!
//! The list rendering below is driven by [`Model::section_row_count`], so the
//! follow-up live-integration slice changes where rows come from rather than
//! how they are selected and drawn.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use super::theme::Theme;
use crate::model::{Model, Section};

/// Draw the Members panel into `area`.
pub fn render(frame: &mut Frame, area: Rect, model: &Model, theme: Theme) {
    let count = model.section_row_count();
    let block = Block::bordered()
        .border_style(theme.panel_border())
        .title(Span::styled(
            format!(" {} ", Section::Members.title()),
            theme.panel_title(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if count == 0 {
        render_empty(frame, inner, model, theme);
        return;
    }

    // Unreachable in this slice; kept so selection and rendering are the same
    // code path once a membership projection is adopted.
    let items: Vec<ListItem> = (0..count)
        .map(|index| ListItem::new(Line::styled(format!("member {index}"), theme.body())))
        .collect();
    let mut state = ListState::default();
    state.select(model.selection().resolved(count));

    frame.render_stateful_widget(
        List::new(items).highlight_style(theme.selected_row()),
        inner,
        &mut state,
    );
}

/// The honest empty state.
fn render_empty(frame: &mut Frame, area: Rect, model: &Model, theme: Theme) {
    let lines = vec![
        Line::styled("No member data.", theme.unavailable()),
        Line::raw(""),
        Line::styled(model.integration().explanation(), theme.body()),
        Line::raw(""),
        Line::styled(
            "Membership will appear here once the worker projection it reads is \
             accepted and served.",
            theme.status(),
        ),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}
