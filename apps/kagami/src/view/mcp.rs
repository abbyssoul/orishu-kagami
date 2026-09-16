//! The MCP server section of the settings panel.
//!
//! Shows the three lifecycle states — disabled, running, failed — and the
//! controls for each. While running it shows the endpoint, the masked token
//! with a copy action, and the connected-client count (including the explicit
//! zero case and the stale-session caveat). Disabling with clients connected
//! names the consequence and asks to confirm (ADR 0006, slice 5).

use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::mcp::McpState;
use crate::message::{McpControl, Message};
use crate::model::Model;

/// The MCP block rendered inside the settings panel.
pub fn view(model: &Model) -> Element<'_, Message> {
    let body: Element<'_, Message> = match &model.mcp {
        McpState::Disabled => disabled(),
        McpState::Running(running) => self::running(running),
        McpState::Failed(reason) => failed(reason),
    };

    column![text("MCP server").size(14), body].spacing(6).into()
}

/// Off: no port bound, one action to turn it on.
fn disabled() -> Element<'static, Message> {
    column![
        text("Disabled. No port is bound and no client can reach the session.").size(11),
        button(text("Enable")).on_press(McpControl::Enable.into()),
    ]
    .spacing(6)
    .into()
}

/// A failed start or a server that died after binding, with a reason and retry.
fn failed(reason: &str) -> Element<'_, Message> {
    column![
        text(format!("Failed: {reason}")).size(11),
        button(text("Enable")).on_press(McpControl::Enable.into()),
    ]
    .spacing(6)
    .into()
}

/// Running: endpoint, masked token, connected count, and disable.
fn running(running: &crate::mcp::McpRunning) -> Element<'_, Message> {
    let count = match running.connection_count() {
        Some(0) => "No clients connected.".to_owned(),
        Some(1) => "1 client connected.".to_owned(),
        Some(n) => format!("{n} clients connected."),
        None => "Checking connections…".to_owned(),
    };

    let controls = column![
        text(format!("Endpoint: {}", running.endpoint())).size(11),
        row![
            text(format!("Token: {}", running.masked_token())).size(11),
            button(text("Copy")).on_press(McpControl::CopyToken.into()),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
        text(count).size(11),
        text("A client that vanished without closing its session may stay counted until the server notices.")
            .size(10),
    ]
    .spacing(6);

    // With clients connected, the first Disable asks to confirm; the confirmed
    // action is what actually stops the server.
    let action: Element<'_, Message> = if running.awaiting_disable_confirm() {
        let connected = running.connection_count().unwrap_or(0);
        let noun = if connected == 1 { "client" } else { "clients" };
        column![
            text(format!(
                "Disabling now cuts off {connected} {noun}. Confirm?"
            ))
            .size(11),
            row![
                button(text("Confirm disable")).on_press(McpControl::ConfirmDisable.into()),
                button(text("Cancel")).on_press(McpControl::CancelDisable.into()),
            ]
            .spacing(6),
        ]
        .spacing(6)
        .into()
    } else {
        button(text("Disable"))
            .on_press(McpControl::Disable.into())
            .into()
    };

    column![controls, action]
        .spacing(6)
        .width(Length::Fill)
        .into()
}
