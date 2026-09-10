use crate::message::{Authoritative, ClientLocal, Message, WorkspaceIntent};
use crate::model::{Model, Tool};
use iced::widget::{Row, button, container, row, rule, text};
use iced::{Element, Length};
use kagami_session::Projection;

pub fn view(model: &Model) -> Element<'_, Message> {
    let mut controls = row![
        // The mode, first and always. ADR 0022 requires it to be unmistakable
        // which of the two things the window is doing.
        text(model.mode_label()),
        rule::vertical(1),
        tool_button("Select", Tool::Select, model.active_tool),
        tool_button("Move", Tool::Move, model.active_tool),
        tool_button("Draw fields", Tool::DrawFields, model.active_tool),
        rule::vertical(1),
        projection_button(Projection::Perspective, model.current_view().projection()),
        projection_button(Projection::Orthographic, model.current_view().projection()),
        // The active scale, always visible. A scale that is in effect but
        // invisible is how someone misreads a scene by twelve orders of
        // magnitude; the control that *changes* it is in the settings panel,
        // where there is room for a typed value.
        text(format!("Scale: {}", model.current_view().scale())),
        rule::vertical(1),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    // Undo and redo are *absent* while observing, not merely disabled. A
    // disabled undo button still says "this is an editable experiment", which
    // is precisely the confusion the two modes exist to prevent.
    controls = if model.is_authoring() {
        history_controls(model, controls)
    } else {
        controls.push(
            button(text("Edit initial conditions"))
                .on_press(WorkspaceIntent::EditInitialConditions.into())
                .style(button::secondary),
        )
    };

    container(
        controls
            .push(rule::vertical(1))
            // No run authority exists yet, so this says so rather than
            // pretending (K-RUN owns it).
            .push(text("Run: unavailable"))
            .push(rule::vertical(1))
            .push(text(format!("Queue {}", model.queue_len)))
            .push(rule::vertical(1))
            .push(text(format!("Orishu: {} (offline)", model.cluster_address))),
    )
    .padding([4, 8])
    .width(Length::Fill)
    .height(Length::Fixed(36.0))
    .into()
}

fn history_controls<'a>(model: &'a Model, controls: Row<'a, Message>) -> Row<'a, Message> {
    controls
        .push(history_button(
            model.document.view().history.undo.as_deref(),
            "Undo",
            Authoritative::Undo,
        ))
        .push(history_button(
            model.document.view().history.redo.as_deref(),
            "Redo",
            Authoritative::Redo,
        ))
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

/// One projection choice, reporting whether it is the active one.
///
/// Both are always offered and the active one is styled, rather than one
/// toggle whose label has to be read carefully: the user story asks for a
/// control that "clearly exposes Perspective and Orthographic projection and
/// reports the active choice".
fn projection_button(projection: Projection, active: Projection) -> Element<'static, Message> {
    let style = if projection == active {
        button::secondary
    } else {
        button::text
    };

    button(text(projection.label()))
        .on_press(ClientLocal::SetProjection(projection).into())
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
