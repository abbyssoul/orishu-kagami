use crate::message::{Authoritative, ClientLocal, Message};
use crate::model::{Model, Tool};
use iced::widget::{button, container, row, rule, text};
use iced::{Element, Length};

pub fn view(model: &Model) -> Element<'_, Message> {
    container(
        row![
            tool_button("Select", Tool::Select, model.active_tool),
            tool_button("Move", Tool::Move, model.active_tool),
            tool_button("Draw fields", Tool::DrawFields, model.active_tool),
            rule::vertical(1),
            history_button(
                model.document.view().history.undo.as_deref(),
                "Undo",
                Authoritative::Undo
            ),
            history_button(
                model.document.view().history.redo.as_deref(),
                "Redo",
                Authoritative::Redo
            ),
            rule::vertical(1),
            // No run authority exists yet, so these say so rather than
            // pretending (K-RUN owns them).
            text("Run: unavailable"),
            rule::vertical(1),
            text(format!("Queue {}", model.queue_len)),
            rule::vertical(1),
            text(format!("Orishu: {} (offline)", model.cluster_address)),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding([4, 8])
    .width(Length::Fill)
    .height(Length::Fixed(36.0))
    .into()
}

fn tool_button(label: &'static str, tool: Tool, active: Tool) -> Element<'static, Message> {
    let style = if tool == active {
        button::secondary
    } else {
        button::text
    };

    button(text(label))
        .on_press(ClientLocal::SelectTool(tool).into())
        .style(style)
        .into()
}

/// An undo or redo button, labelled with what it will do.
///
/// Disabled honestly when there is nothing to reverse: the label comes from
/// the authority's `HistoryStatus`, so the button cannot offer something the
/// authority would refuse.
fn history_button(
    entry: Option<&str>,
    verb: &'static str,
    intent: Authoritative,
) -> Element<'static, Message> {
    let label = match entry {
        Some(edit) => format!("{verb} {edit}"),
        None => verb.to_owned(),
    };
    let mut control = button(text(label)).style(button::text);
    if entry.is_some() {
        control = control.on_press(intent.into());
    }
    control.into()
}
