//! The Overview section.
//!
//! In this slice Overview describes the shell itself and states, plainly, that
//! there is no worker integration to summarise. It shows no cluster figures at
//! all — not even zeroes, which a reader would be entitled to read as "a worker
//! answered, and the cluster is empty".

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::theme::Theme;
use crate::model::{Model, Section};

/// Draw the Overview panel into `area`.
pub fn render(frame: &mut Frame, area: Rect, model: &Model, theme: Theme) {
    let block = Block::bordered()
        .border_style(theme.panel_border())
        .title(Span::styled(
            format!(" {} ", Section::Overview.title()),
            theme.panel_title(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("application  ", theme.label()),
            Span::styled(super::APPLICATION_NAME, theme.value()),
        ]),
        Line::from(vec![
            Span::styled("version      ", theme.label()),
            Span::styled(env!("CARGO_PKG_VERSION"), theme.value()),
        ]),
        Line::from(vec![
            Span::styled("stage        ", theme.label()),
            Span::styled("terminal shell", theme.value()),
        ]),
        Line::raw(""),
        Line::styled(model.integration().explanation(), theme.unavailable()),
        Line::raw(""),
        Line::styled(
            "Use orishuctl for implemented cluster administration.",
            theme.body(),
        ),
        Line::styled(
            "Press ? for the keys this shell implements.",
            theme.status(),
        ),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}
