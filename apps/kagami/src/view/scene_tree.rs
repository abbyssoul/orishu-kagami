use crate::message::Message;
use crate::model::Model;
use crate::scene_model::{SceneNode, SceneTree};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

const INDENT: f32 = 16.0;

pub fn view(model: &Model) -> Element<'_, Message> {
    let search = text_input("Search...", &model.search_query)
        .on_input(Message::SearchChanged)
        .width(Length::Fill);

    let query = model.search_query.trim();
    let mut tree = column![].spacing(2);

    for root in model.scene.filtered_roots(query) {
        tree = tree.push(view_node(root, model, query, 0));
    }

    container(
        column![search, scrollable(tree).height(Length::Fill)]
            .spacing(8)
            .padding(8),
    )
    .width(Length::Fixed(280.0))
    .height(Length::Fill)
    .into()
}

fn view_node<'a>(
    node: &'a SceneNode,
    model: &Model,
    query: &str,
    depth: u16,
) -> Element<'a, Message> {
    let is_expanded = model.expanded.contains(&node.id);
    let is_selected = model.selected == Some(node.id);
    let has_children = !node.children.is_empty();

    let expand_icon = if !has_children {
        " "
    } else if is_expanded {
        "\u{25be}"
    } else {
        "\u{25b8}"
    };

    let eye_icon = if node.visible { "\u{1f441}" } else { "-" };

    let row = row![
        Space::new().width(Length::Fixed(depth as f32 * INDENT)),
        button(text(expand_icon))
            .on_press_maybe(has_children.then_some(Message::NodeExpandToggled(node.id)))
            .style(button::text)
            .width(Length::Fixed(20.0)),
        button(text(eye_icon))
            .on_press(Message::NodeVisibilityToggled(node.id))
            .style(button::text)
            .width(Length::Fixed(24.0)),
        button(text(node.name.clone()).width(Length::Fill))
            .on_press(Message::NodeSelected(node.id))
            .style(if is_selected {
                button::secondary
            } else {
                button::text
            })
            .width(Length::Fill),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);

    let mut column = column![row];

    if has_children && is_expanded {
        for child in &node.children {
            if SceneTree::node_matches(child, query) {
                column = column.push(view_node(child, model, query, depth + 1));
            }
        }
    }

    column.into()
}
