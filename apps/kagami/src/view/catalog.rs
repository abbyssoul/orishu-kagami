//! Bounded catalog pages and explicit instance inputs; no authoritative UI state.
use crate::{
    catalog_form::{Action, Input, PAGE},
    message::Message,
    model::Model,
};
use iced::{
    Element, Length,
    widget::{button, checkbox, column, container, row, scrollable, text, text_input},
};

pub(super) fn panel(model: &Model) -> Element<'_, Message> {
    let c = &model.catalog;
    let active = model.is_authoring() && !c.is_pending() && !model.scientific_effects.is_pending();
    let mut content = column![
        text("Object catalog").size(18),
        text("Templates copy components and definitions into the experiment. They do not select a field or integrator; later catalog edits never change created objects.").size(11),
        row![button("Load directory…").on_press_maybe(active.then_some(Message::Catalog(Action::PickDirectory))),
            button("Reload").on_press_maybe((active && c.snapshot().is_some()).then_some(Message::Catalog(Action::Reload))),
            button("Close").on_press(Message::Catalog(Action::Close))].spacing(6),
        text(&c.notice).size(11),
    ].spacing(8);
    if let Some(snapshot) = c.snapshot() {
        let catalog = c.catalog_token();
        content = content.push(text(format!("{} entries; showing {}–{}. Select a valid or unavailable entry to inspect and resolve it.", snapshot.entries().len(),
            c.offset.saturating_add(1).min(snapshot.entries().len()), (c.offset + PAGE).min(snapshot.entries().len()))).size(11));
        for (index, entry) in snapshot
            .entries()
            .iter()
            .enumerate()
            .skip(c.offset)
            .take(PAGE)
        {
            let label = entry
                .identity
                .as_ref()
                .map(|id| format!("{}/{}", id.catalog, id.template))
                .unwrap_or_else(|| entry.source.file.display().to_string());
            content = content.push(
                button(text(format!("{label} — {}", entry.result.state())).size(12))
                    .on_press_maybe(
                        (active && entry.result.template().is_some())
                            .then_some(Message::Catalog(Action::Select { catalog, index })),
                    ),
            );
            if !entry.result.is_available() {
                let diagnostic = match &entry.result {
                    kagami_catalog::LoadResult::Unavailable { reasons, .. } => {
                        format!("{:?}", reasons.iter().take(4).collect::<Vec<_>>())
                    }
                    kagami_catalog::LoadResult::Invalid { diagnostics } => {
                        format!("{:?}", diagnostics.iter().take(4).collect::<Vec<_>>())
                    }
                    _ => String::new(),
                };
                content =
                    content.push(text(diagnostic.chars().take(512).collect::<String>()).size(10));
            }
        }
        content = content.push(
            row![
                button("Previous").on_press_maybe((c.offset > 0).then_some(Message::Catalog(
                    Action::Page {
                        catalog,
                        offset: c.offset.saturating_sub(PAGE)
                    }
                ))),
                button("Next").on_press_maybe(
                    (c.offset + PAGE < snapshot.entries().len()).then_some(Message::Catalog(
                        Action::Page {
                            catalog,
                            offset: c.offset + PAGE
                        }
                    ))
                ),
            ]
            .spacing(6),
        );
        for error in snapshot.file_errors().iter().take(8) {
            content = content.push(
                text(
                    format!("{}: {:?}", error.file.display(), error.reason)
                        .chars()
                        .take(512)
                        .collect::<String>(),
                )
                .size(10),
            );
        }
    }
    let form = &c.form;
    if let Some(template) = form.template() {
        let proposal = form.token();
        let input_token = form.input_token();
        content = content
            .push(
                text(format!(
                    "Create {}/{}",
                    template.identity.catalog, template.identity.template
                ))
                .size(16),
            )
            .push(button("Cancel proposal").on_press(Message::Catalog(Action::Cancel)));
        if let Some(fingerprint) = form.fingerprint() {
            content = content.push(text(format!("Exact template: {fingerprint}")).size(10));
        }
        for component in &template.spec.components {
            content = content.push(
                text(format!(
                    "Component {}: {}",
                    component.name, component.type_id
                ))
                .size(10),
            );
        }
        if !form.current(&model.document) {
            content = content.push(
                text("Document or mode changed. Select the template again; nothing was accepted.")
                    .size(11),
            );
        } else {
            content = content.push(text_input("Object name", &form.name).on_input_maybe(
                active.then_some(move |s| {
                    Message::Catalog(Action::Edit {
                        form: input_token,
                        input: Input::Name(s),
                    })
                }),
            ));
            for (axis, name) in ["x", "y", "z"].into_iter().enumerate() {
                content = content.push(
                    row![
                        text(format!("{name}: metres")).size(11),
                        text_input("0", &form.position[axis])
                            .width(Length::Fixed(85.0))
                            .on_input_maybe(active.then_some(move |metres| Message::Catalog(
                                Action::Edit {
                                    form: input_token,
                                    input: Input::Position { axis, metres }
                                }
                            ))),
                        text("m/s").size(11),
                        text_input("0", &form.velocity[axis])
                            .width(Length::Fixed(85.0))
                            .on_input_maybe(active.then_some(move |metres_per_second| {
                                Message::Catalog(Action::Edit {
                                    form: input_token,
                                    input: Input::Velocity {
                                        axis,
                                        metres_per_second,
                                    },
                                })
                            })),
                    ]
                    .spacing(6),
                );
            }
            for (name, parameter) in &template.spec.parameters {
                let field = name.clone();
                let reset = name.clone();
                let value = form.overrides().and_then(|b| b.get(name));
                let default = parameter.default.expression().source();
                content = content
                    .push(
                        text(format!(
                            "{} ({})",
                            name,
                            parameter
                                .default
                                .unit()
                                .map(|u| u.to_string())
                                .unwrap_or_else(|| "SI expression".into())
                        ))
                        .size(12),
                    )
                    .push(
                        text_input(default, value.map_or("", String::as_str)).on_input_maybe(
                            active.then_some(move |source| {
                                Message::Catalog(Action::Edit {
                                    form: input_token,
                                    input: Input::Parameter {
                                        name: field.clone(),
                                        source: Some(source),
                                    },
                                })
                            }),
                        ),
                    )
                    .push(button("Use template default").on_press_maybe(
                        (active && value.is_some()).then_some(Message::Catalog(Action::Edit {
                            form: input_token,
                            input: Input::Parameter {
                                name: reset,
                                source: None,
                            },
                        })),
                    ));
            }
            content = content.push(super::physics::dependency_choices(
                &c.dependencies,
                form.bindings(),
                false,
                active,
                |a| Message::Catalog(Action::Dependencies(a)),
            ));
            if form.captured {
                content = content.push(checkbox(form.confirmed()).label("Create this object and regenerate initial integrator history; retain fields and existing providers. Undo is available.").text_size(11)
                    .on_toggle(move |confirmed| Message::Catalog(Action::Confirm { proposal, confirmed })));
            }
            content = content.push(
                button(if form.captured {
                    "Create and regenerate history"
                } else {
                    "Create object"
                })
                .on_press_maybe(
                    (active && (!form.captured || form.confirmed()))
                        .then_some(Message::Catalog(Action::Apply { proposal })),
                ),
            );
        }
    }
    container(scrollable(content))
        .padding(10)
        .width(Length::Fixed(420.0))
        .height(Length::Fill)
        .into()
}
