use crate::message::{Authoritative, ClientLocal, Message};
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
    // Replacing a document with unsaved changes is a question, and the
    // authority refuses until it is answered. The window answers it by asking
    // the user here; an MCP caller answers it as a request field (ADR 0006).
    let discard = |model: &Model| !model.document.is_dirty() || confirm_discard();

    let (left, items): (f32, Vec<(&'static str, Message)>) = match model.open_menu? {
        MenuId::File => (
            FILE_LEFT,
            vec![
                (
                    "New",
                    Authoritative::New {
                        discard_unsaved: discard(model),
                    }
                    .into(),
                ),
                ("Open", open_message(discard(model))),
                ("Save", Authoritative::Save { path: None }.into()),
                ("Save As", save_as_message()),
                ("Settings", ClientLocal::OpenSettings.into()),
                ("Exit", Message::Exit),
            ],
        ),
        MenuId::Edit => (
            EDIT_LEFT,
            vec![
                ("Undo", Authoritative::Undo.into()),
                ("Redo", Authoritative::Redo.into()),
            ],
        ),
        MenuId::Help => (HELP_LEFT, vec![("About", ClientLocal::CloseMenu.into())]),
    };

    let catcher =
        mouse_area(Space::new().width(Fill).height(Fill)).on_press(ClientLocal::CloseMenu.into());

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
        .on_press(ClientLocal::ToggleMenu(id).into())
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

/// Ask the user whether to discard unsaved changes.
///
/// The window's way of answering a question the authority refuses to answer
/// for anyone. A native dialog, because this is the imperative shell and that
/// is what it is for.
fn confirm_discard() -> bool {
    rfd::MessageDialog::new()
        .set_title("Unsaved changes")
        .set_description("This experiment has unsaved changes. Discard them?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

/// Pick a file to open, or do nothing.
fn open_message(discard_unsaved: bool) -> Message {
    match rfd::FileDialog::new().pick_file() {
        Some(path) => Authoritative::Open {
            path,
            discard_unsaved,
        }
        .into(),
        None => ClientLocal::CloseMenu.into(),
    }
}

/// Pick a file to save to, or do nothing.
fn save_as_message() -> Message {
    match rfd::FileDialog::new().save_file() {
        Some(path) => Authoritative::Save { path: Some(path) }.into(),
        None => ClientLocal::CloseMenu.into(),
    }
}
