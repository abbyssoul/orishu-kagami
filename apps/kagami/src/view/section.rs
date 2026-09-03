use crate::message::Message;
use iced::widget::{column, container, text};
use iced::{Element, Length};

/// Reusable collapsible-section scaffold used to build the right-panel
/// inspector out of independent per-component "sections" (Transform,
/// Info, and eventually field/solver/etc. sections per object kind).
pub fn view_section<'a>(title: &str, content: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text(title.to_string()).size(14),
        container(content).width(Length::Fill).padding(8),
    ]
    .spacing(4)
    .into()
}
