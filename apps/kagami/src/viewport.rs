use crate::message::Message;
use crate::model::Model;
use iced::widget::shader;
use iced::{Element, Length};

pub fn view(model: &Model) -> Element<'_, Message> {
    shader(&model.scene_program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
