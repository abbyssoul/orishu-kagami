use crate::message::Message;
use crate::model::{Menu as MenuId, Model};
use iced::widget::{Space, button, column, container, mouse_area, row, text};
use iced::{Element, Fill, Length, Padding};

const BAR_HEIGHT: f32 = 36.0;
const FILE_LEFT: f32 = 8.0;
const EDIT_LEFT: f32 = 64.0;
const HELP_LEFT: f32 = 128.0;

pub fn view(model: &Model) -> Element<'_, Message> {
    container(
        row![
            menu_button("File", MenuId::File, model.open_menu),
            menu_button("Edit", MenuId::Edit, model.open_menu),
            menu_button("Help", MenuId::Help, model.open_menu),
        ]
        .spacing(2),
    )
    .padding(4)
    .into()
}

/// The floating dropdown for the currently open menu, plus a full-window
/// click-catcher that closes it, meant to be layered on top of the rest of
/// the UI in a `stack!` rather than pushing it down.
pub fn overlay(model: &Model) -> Option<Element<'_, Message>> {
    let (left, items): (f32, Vec<(&'static str, Message)>) = match model.open_menu? {
        MenuId::File => (
            FILE_LEFT,
            vec![
                ("New", Message::FileNew),
                ("Open", Message::FileOpen),
                ("Save", Message::FileSave),
                ("Save As", Message::FileSaveAs),
                ("Settings", Message::FileSettings),
                ("Exit", Message::FileExit),
            ],
        ),
        MenuId::Edit => (
            EDIT_LEFT,
            vec![("Undo", Message::EditUndo), ("Redo", Message::EditRedo)],
        ),
        MenuId::Help => (HELP_LEFT, vec![("About", Message::HelpAbout)]),
    };

    let catcher = mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::MenuClosed);

    let panel = container(dropdown(items))
        .width(Fill)
        .height(Fill)
        .padding(Padding::ZERO.top(BAR_HEIGHT).left(left));

    Some(
        iced::widget::stack![catcher, panel]
            .width(Fill)
            .height(Fill)
            .into(),
    )
}

fn menu_button(label: &'static str, id: MenuId, open: Option<MenuId>) -> Element<'static, Message> {
    let style = if open == Some(id) {
        button::secondary
    } else {
        button::text
    };

    button(text(label))
        .on_press(Message::MenuToggled(id))
        .style(style)
        .into()
}

fn dropdown(items: Vec<(&'static str, Message)>) -> Element<'static, Message> {
    let mut list = column![].spacing(1).padding(4);

    for (label, message) in items {
        list = list.push(
            button(text(label).width(Length::Fill))
                .width(Length::Fixed(160.0))
                .on_press(message)
                .style(button::text),
        );
    }

    container(list).style(container::bordered_box).into()
}
