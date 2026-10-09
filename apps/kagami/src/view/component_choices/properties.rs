//! Attachment inputs remain local until the paired component/lock is accepted.
use crate::{
    component_form::{
        Action,
        properties::{Input, Properties},
    },
    message::Message,
};
use iced::{
    Element,
    widget::{button, column, row, text, text_input},
};
use kagami_catalog::PropertyKind;
use kagami_document::AuthoredValue;

pub(super) fn view(fields: &Properties, active: bool) -> Element<'_, Message> {
    let mut content = column![text("Proposed component values").size(14),
        text("Quantities retain expression source. Units, variables and constraints are checked on Save. Unset differs from empty text or false.").size(11)].spacing(5);
    for (name, property) in &fields.schema().properties {
        let form = fields.id();
        let selected = fields.values().get(name);
        let mut controls = column![
            text(format!(
                "{name}{}",
                if property.required {
                    " (required)"
                } else {
                    " (optional)"
                }
            ))
            .size(12)
        ]
        .spacing(3);
        match &property.kind {
            PropertyKind::Quantity { dimension } => {
                let source = match selected {
                    Some(AuthoredValue::Quantity { expression, .. }) => expression.as_str(),
                    _ => "",
                };
                let name = name.clone();
                let mut field = text_input("SI expression / units", source);
                if active {
                    field = field.on_input(move |source| {
                        Message::ComponentForm(Action::SetProperty {
                            form,
                            property: name.clone(),
                            value: Input::Quantity(source),
                        })
                    });
                }
                controls = controls
                    .push(text(format!("Dimension: {dimension}")).size(11))
                    .push(field);
            }
            PropertyKind::Text => {
                let source = match selected {
                    Some(AuthoredValue::Text(source)) => source.as_str(),
                    _ => "",
                };
                let name = name.clone();
                let mut field = text_input("Text", source);
                if active {
                    let name = name.clone();
                    field = field.on_input(move |source| {
                        Message::ComponentForm(Action::SetProperty {
                            form,
                            property: name.clone(),
                            value: Input::Text(source),
                        })
                    });
                }
                controls = controls.push(field);
                if selected.is_none() {
                    controls =
                        controls.push(button("Set empty text").on_press_maybe(active.then(|| {
                            Message::ComponentForm(Action::SetProperty {
                                form,
                                property: name.clone(),
                                value: Input::Text(String::new()),
                            })
                        })));
                }
            }
            PropertyKind::Boolean => {
                controls = controls.push(
                    row![
                        button("false").on_press_maybe(active.then(|| Message::ComponentForm(
                            Action::SetProperty {
                                form,
                                property: name.clone(),
                                value: Input::Boolean(false)
                            }
                        ))),
                        button("true").on_press_maybe(active.then(|| Message::ComponentForm(
                            Action::SetProperty {
                                form,
                                property: name.clone(),
                                value: Input::Boolean(true)
                            }
                        ))),
                    ]
                    .spacing(5),
                );
            }
        }
        let state = match selected {
            None => "unset",
            Some(AuthoredValue::Boolean(true)) => "true",
            Some(AuthoredValue::Boolean(false)) => "false",
            Some(_) => "set",
        };
        controls = controls.push(
            row![
                text(state).size(11),
                button("Unset").on_press_maybe((active && selected.is_some()).then(|| {
                    Message::ComponentForm(Action::SetProperty {
                        form,
                        property: name.clone(),
                        value: Input::Unset,
                    })
                })),
            ]
            .spacing(5),
        );
        content = content.push(controls);
    }
    content.into()
}
