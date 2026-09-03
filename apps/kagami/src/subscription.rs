use crate::message::Message;
use crate::model::Model;
use iced::event::{self, Event};
use iced::keyboard::{self, Key};
use iced::window;
use std::time::Duration;

pub fn subscription(model: &Model) -> iced::Subscription<Message> {
    let timer = if model.exit_deadline.is_some() {
        iced::time::every(Duration::from_millis(100)).map(|_| Message::ExitTimerTick)
    } else {
        iced::Subscription::none()
    };

    iced::Subscription::batch([timer, iced::event::listen_with(exit_shortcut)])
}

/// Ctrl+Q (Cmd+Q on macOS), the standard "quit the app" shortcut.
fn exit_shortcut(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    if status == event::Status::Captured {
        return None;
    }

    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };

    (key.as_ref() == Key::Character("q") && modifiers.command()).then_some(Message::FileExit)
}
