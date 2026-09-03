use super::section::view_section;
use crate::message::{Axis, Message};
use crate::model::Model;
use crate::scene_model::{NodeId, ObjectKind, SceneNode};
use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

pub fn view(model: &Model) -> Element<'_, Message> {
    let content: Element<Message> = match model
        .selected
        .and_then(|id| model.scene.find(id).map(|node| (id, node)))
    {
        Some((id, node)) if node.kind != ObjectKind::Category => column![
            view_section("Transform", view_transform(id, node)),
            view_section("Info", view_info(node)),
        ]
        .spacing(12)
        .into(),
        Some(_) => text("No inspector for this item").into(),
        None => text("Nothing selected").into(),
    };

    container(content)
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .padding(8)
        .into()
}

fn axis_field<'a>(
    label: &'static str,
    id: NodeId,
    axis: Axis,
    value: f32,
    on_change: fn(NodeId, Axis, f32) -> Message,
) -> Element<'a, Message> {
    row![
        text(label).width(Length::Fixed(16.0)),
        text_input("0.0", &format!("{value:.2}"))
            .on_input(move |raw| on_change(id, axis, raw.parse().unwrap_or(value)))
            .width(Length::Fill),
    ]
    .spacing(6)
    .into()
}

fn view_transform<'a>(id: NodeId, node: &'a SceneNode) -> Element<'a, Message> {
    let [px, py, pz] = node.transform.position;
    let [rx, ry, rz] = node.transform.rotation;

    column![
        text("Position").size(12),
        axis_field("X", id, Axis::X, px, Message::TransformPositionChanged),
        axis_field("Y", id, Axis::Y, py, Message::TransformPositionChanged),
        axis_field("Z", id, Axis::Z, pz, Message::TransformPositionChanged),
        text("Rotation").size(12),
        axis_field("X", id, Axis::X, rx, Message::TransformRotationChanged),
        axis_field("Y", id, Axis::Y, ry, Message::TransformRotationChanged),
        axis_field("Z", id, Axis::Z, rz, Message::TransformRotationChanged),
    ]
    .spacing(6)
    .into()
}

fn view_info(node: &SceneNode) -> Element<'_, Message> {
    let kind = match node.kind {
        ObjectKind::Category => "Category",
        ObjectKind::Planet => "Planet",
        ObjectKind::Probe => "Probe",
        ObjectKind::SlicePlane => "Slice plane",
    };

    column![
        row![
            text("Name").width(Length::Fixed(60.0)),
            text(node.name.clone())
        ],
        row![text("Kind").width(Length::Fixed(60.0)), text(kind)],
    ]
    .spacing(4)
    .into()
}
