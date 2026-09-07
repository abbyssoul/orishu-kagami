//! The object list, rendered from the authority's read projection.
//!
//! There is no scene model here any more. What the tree shows is a snapshot at
//! one revision, so it can never display half of an edit, and the only state
//! it adds is which rows are open and which are hidden — both client-local.

use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};
use kagami_document::{Object, ObjectId, Participation};

use crate::message::{Authoritative, ClientLocal, Message};
use crate::model::Model;

pub fn view(model: &Model) -> Element<'_, Message> {
    let snapshot = model.document.snapshot();
    let query = model.search_query.to_lowercase();

    let mut rows = column![].spacing(2);
    let mut shown = 0usize;
    for (id, object) in snapshot.objects() {
        if !matches(object, &query) {
            continue;
        }
        shown += 1;
        rows = rows.push(object_row(model, *id, object));
    }

    let heading = text(format!("Objects ({shown})")).size(14);
    let empty: Element<'_, Message> = if shown == 0 {
        text(if snapshot.object_count() == 0 {
            "No objects yet."
        } else {
            "Nothing matches."
        })
        .size(12)
        .into()
    } else {
        Space::new().into()
    };

    container(
        column![
            row![
                text_input("Search", &model.search_query)
                    .on_input(|query| ClientLocal::Search(query).into())
                    .width(Length::Fill),
                button(text("+"))
                    .on_press(Authoritative::CreateObject.into())
                    .padding(4),
            ]
            .spacing(6),
            heading,
            scrollable(column![rows, empty].spacing(4)).height(Length::Fill),
        ]
        .spacing(8),
    )
    .width(Length::Fixed(260.0))
    .height(Length::Fill)
    .padding(8)
    .into()
}

fn object_row<'a>(model: &'a Model, id: ObjectId, object: &'a Object) -> Element<'a, Message> {
    let selected = model.selected == Some(id);
    let hidden = model.hidden.contains(&id);

    // What an object *does* in a run comes from the installed schemas, not
    // from anything the window knows about it.
    let participation = match object.participation(model.document.schemas()) {
        Participation::Inert => "",
        Participation::Participating => " ·",
        Participation::Unavailable => " !",
    };
    let label = format!(
        "{}{}{}",
        if selected { "▸ " } else { "  " },
        object.name.as_str(),
        participation
    );

    row![
        button(text(label).size(13))
            .on_press(ClientLocal::Select(Some(id)).into())
            .width(Length::Fill)
            .padding(4),
        button(text(if hidden { "○" } else { "●" }).size(12))
            .on_press(ClientLocal::ToggleHidden(id).into())
            .padding(4),
        button(text("×").size(12))
            .on_press(Authoritative::RemoveObject(id).into())
            .padding(4),
    ]
    .spacing(2)
    .into()
}

/// Whether an object survives the search filter.
fn matches(object: &Object, query: &str) -> bool {
    query.is_empty() || object.name.as_str().to_lowercase().contains(query)
}
