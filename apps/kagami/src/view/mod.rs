mod inspector;
mod menu;
mod scene_tree;
mod section;
mod toolbar;

use crate::message::{ClientLocal, Message};
use crate::model::Model;
use crate::viewport;
use iced::widget::{Space, button, column, container, row, stack, text};
use iced::{Element, Length};

pub fn view(model: &Model) -> Element<'_, Message> {
    let right_panel = if model.settings_open {
        settings_panel()
    } else {
        inspector::view(model)
    };

    let base = column![
        menu::view(model),
        toolbar::view(model),
        row![scene_tree::view(model), viewport::view(model), right_panel].height(Length::Fill),
    ];

    // Always wrap in the same `Stack` shape, menu open or not — the shader
    // widget inside `base` keeps its state (the camera) across frames only
    // while its position in the widget tree stays identical; letting the
    // wrapper itself appear/disappear reset the camera on every menu toggle.
    let overlay_layer: Element<'_, Message> =
        menu::overlay(model).unwrap_or_else(|| Space::new().into());

    stack![base, overlay_layer].into()
}

fn settings_panel() -> Element<'static, Message> {
    container(
        column![
            text("Settings").size(16),
            text("Show help on startup"),
            text("Show diagnostics on startup"),
            button(text("Close")).on_press(ClientLocal::CloseSettings.into()),
        ]
        .spacing(8),
    )
    .width(Length::Fixed(280.0))
    .height(Length::Fill)
    .padding(8)
    .into()
}
