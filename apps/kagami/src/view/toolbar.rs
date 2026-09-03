use crate::message::Message;
use crate::model::{Model, Playback, Tool};
use iced::widget::{button, container, row, rule, text};
use iced::{Element, Length};

pub fn view(model: &Model) -> Element<'_, Message> {
    container(
        row![
            tool_button("Select", Tool::Select, model.active_tool),
            tool_button("Move", Tool::Move, model.active_tool),
            tool_button("Draw fields", Tool::DrawFields, model.active_tool),
            rule::vertical(1),
            action_button("Undo", Message::EditUndo),
            action_button("Redo", Message::EditRedo),
            rule::vertical(1),
            action_button("Play", Message::PlaybackPlay),
            action_button("Pause", Message::PlaybackPause),
            action_button("Step", Message::PlaybackStep),
            text(playback_label(model.playback)),
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

fn playback_label(playback: Playback) -> &'static str {
    match playback {
        Playback::Playing => "• Playing",
        Playback::Paused => "• Paused",
    }
}

fn tool_button(label: &'static str, tool: Tool, active: Tool) -> Element<'static, Message> {
    let style = if tool == active {
        button::secondary
    } else {
        button::text
    };

    button(text(label))
        .on_press(Message::ToolSelected(tool))
        .style(style)
        .into()
}

fn action_button(label: &'static str, message: Message) -> Element<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(button::text)
        .into()
}
