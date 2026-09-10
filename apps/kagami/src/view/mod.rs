mod inspector;
mod menu;
mod scene_tree;
mod section;
mod toolbar;

use crate::message::{ClientLocal, Message, WorkspaceIntent};
use crate::model::Model;
use crate::viewport;
use iced::widget::{Space, button, column, container, pick_list, row, stack, text, text_input};
use iced::{Element, Length};
use kagami_session::SceneScale;

pub fn view(model: &Model) -> Element<'_, Message> {
    let right_panel = if model.settings_open {
        settings_panel(model)
    } else if model.is_authoring() {
        inspector::view(model)
    } else {
        // The inspector *is* document mutation — schema-driven fields, attach,
        // detach. Observation/replay replaces it rather than disabling it,
        // because a greyed-out property field still reads as an editable
        // experiment (ADR 0022).
        observation_panel(model)
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

/// What the right panel shows while a run is being watched.
///
/// Deliberately thin, and honest about it: playback and observation rendering
/// belong to K-RUN, K-PREVIEW and V-LIVE, and none of them exists. What this
/// panel does carry is the two things ADR 0022 requires to be unmistakable —
/// which run, and that this is not an editable experiment — plus the explicit
/// way back.
fn observation_panel(model: &Model) -> Element<'_, Message> {
    container(
        column![
            text(model.mode_label()).size(16),
            text("Observing a run. The experiment cannot be edited here.").size(12),
            text("Playback and observation rendering are not implemented yet.").size(12),
            button(text("Edit initial conditions"))
                .on_press(WorkspaceIntent::EditInitialConditions.into()),
        ]
        .spacing(8),
    )
    .width(Length::Fixed(300.0))
    .height(Length::Fill)
    .padding(8)
    .into()
}

fn settings_panel(model: &Model) -> Element<'_, Message> {
    container(
        column![
            text("Settings").size(16),
            view_settings(model),
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

/// The scene-scale control: presets, and a distance per unit.
///
/// Here rather than on the toolbar because a typed length needs room, and
/// because this is a precise setting rather than a frequent gesture. The
/// toolbar keeps the readout, so the active scale is never hidden behind a
/// panel someone has to open.
///
/// Presets submit immediately — choosing one is atomic, like choosing a
/// projection. The typed field commits on submit, because `1 n` is not a scale
/// and a revision per keystroke would dirty the file on the way to a value
/// nobody has finished asking for.
fn view_settings(model: &Model) -> Element<'_, Message> {
    let active = model.current_view().scale();
    let typed = model
        .scale_entry
        .clone()
        .unwrap_or_else(|| format!("{} m", active.metres()));

    column![
        text("Scene scale").size(14),
        text("How many metres one viewport unit represents.").size(11),
        // The `'static` preset slice, so offering it every frame allocates
        // nothing.
        pick_list(SceneScale::PRESETS, Some(active), |scale| {
            ClientLocal::SetScale(scale).into()
        }),
        row![
            text("metres / unit").size(12),
            text_input("1 m", &typed)
                .on_input(|source| ClientLocal::EditScale(source).into())
                .on_submit(ClientLocal::SubmitScale(typed.clone()).into())
                .width(Length::Fixed(110.0)),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
        // Reading a unit-bearing entry costs nothing: the expression engine
        // already treats a unit as part of the language.
        text("Accepts a unit, e.g. `1 nm` or `1 AU`.").size(11),
    ]
    .spacing(6)
    .into()
}
