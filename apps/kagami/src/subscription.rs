use crate::message::{McpControl, Message};
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

    // While the server is running, poll it non-blockingly so the displayed
    // count and liveness stay honest — and so a server that died after binding
    // moves to `Failed` rather than looking alive.
    let mcp_poll = if model.mcp.running().is_some() {
        iced::time::every(Duration::from_millis(500)).map(|_| Message::Mcp(McpControl::Poll))
    } else {
        iced::Subscription::none()
    };

    #[cfg(unix)]
    let scientific = if model.scientific_effects.is_pending() {
        iced::time::every(Duration::from_millis(100))
            .map(|_| Message::Scientific(crate::message::ScientificAction::Poll))
    } else {
        iced::Subscription::none()
    };
    #[cfg(not(unix))]
    let scientific = iced::Subscription::none();
    iced::Subscription::batch([
        {
            #[cfg(unix)]
            {
                if model.catalog.is_pending() {
                    iced::time::every(Duration::from_millis(100))
                        .map(|_| Message::Catalog(crate::catalog_form::Action::Poll))
                } else {
                    iced::Subscription::none()
                }
            }
            #[cfg(not(unix))]
            {
                iced::Subscription::none()
            }
        },
        {
            #[cfg(unix)]
            {
                if model.physics_dependencies.is_pending()
                    || model.component_dependencies.is_pending()
                {
                    iced::time::every(Duration::from_millis(100)).map(|_| {
                        Message::PhysicsForm(crate::physics_form::PhysicsAction::Dependencies(
                            crate::physics_form::dependencies::Action::Poll,
                        ))
                    })
                } else {
                    iced::Subscription::none()
                }
            }
            #[cfg(not(unix))]
            {
                iced::Subscription::none()
            }
        },
        {
            #[cfg(unix)]
            {
                if model.plugin_management.is_pending() || model.plugin_references.is_pending() {
                    iced::time::every(Duration::from_millis(100))
                        .map(|_| Message::Plugins(crate::plugins::window::Action::Poll))
                } else {
                    iced::Subscription::none()
                }
            }
            #[cfg(not(unix))]
            {
                iced::Subscription::none()
            }
        },
        {
            #[cfg(unix)]
            {
                if model.workload_preparation.is_pending() {
                    iced::time::every(Duration::from_millis(100))
                        .map(|_| Message::Workload(crate::workload_preparation::Action::Poll))
                } else {
                    iced::Subscription::none()
                }
            }
            #[cfg(not(unix))]
            {
                iced::Subscription::none()
            }
        },
        if model.run.is_pending() {
            iced::time::every(Duration::from_millis(100))
                .map(|_| Message::Run(crate::run::Action::Poll))
        } else {
            iced::Subscription::none()
        },
        timer,
        mcp_poll,
        scientific,
        iced::event::listen_with(exit_shortcut),
    ])
}

/// Ctrl+Q (Cmd+Q on macOS), the standard "quit the app" shortcut.
fn exit_shortcut(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    if status == event::Status::Captured {
        return None;
    }

    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };

    (key.as_ref() == Key::Character("q") && modifiers.command()).then_some(Message::Exit)
}
