//! Explicit selection/reset form; no contribution IDs or domain defaults select code.
use crate::{
    message::Message,
    model::Model,
    physics_form::{ParameterAction, PhysicsAction},
};
use iced::{
    Element, Length,
    widget::{button, checkbox, column, row, text, text_input},
};
use orishu_plugin::{Property, PropertyType, execution::ConfigurationInput};

pub(super) fn view(model: &Model) -> Element<'_, Message> {
    let form = &model.physics_form;
    let active = model.is_authoring() && !model.scientific_effects.is_pending();
    let mut content = column![text("Configure physics").size(16), text("Select an integrator and field models explicitly. Parameters use declared defaults unless overridden. New proposals use binary64 and direct-quality samples; copied settings retain their policies. Unsupported inputs/dependencies are refused.").size(11)].spacing(8);
    if model.kernel_choices.is_empty() {
        return column![text("No enabled field models or integrators."), text("Install local plugin bundles with kagami plugin, then reopen the app to refresh this inventory.").size(11)].into();
    }
    if model.document.snapshot().setup().scientific().is_some() {
        let mut copy = button("Copy captured settings into form");
        if active {
            copy = copy.on_press(Message::PhysicsForm(PhysicsAction::LoadCaptured));
        }
        content = content.push(copy);
    }
    if form.is_captured() {
        content = content.push(text("Editing captured settings: exact model/instance IDs, dependency pins, precision and sampling policy are preserved. Apply resets all initial fields/history.").size(11));
        let mut reset = button("Start new physics proposal");
        if active {
            reset = reset.on_press(Message::PhysicsForm(PhysicsAction::Reset));
        }
        content = content.push(reset);
    }
    for (i, choice) in model.kernel_choices.iter().enumerate() {
        let mut select = checkbox(form.selected.contains(&i))
            .label(format!("{:?}: {}", choice.contract, choice.label))
            .text_size(11);
        if active && !form.is_captured() {
            select = select.on_toggle(move |_| Message::PhysicsForm(PhysicsAction::Kernel(i)));
        }
        content = content.push(select);
        if form.selected.contains(&i) {
            for property in &choice.configuration {
                content = content.push(parameter(model, i, property, active));
            }
        }
    }
    if let Some(error) = form.parameters.error() {
        content = content.push(text(error).size(12));
    }
    content = content.push(text("Domain corners (metres): lower / upper").size(12));
    for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
        let mut lower = text_input("lower m", &form.lower[axis]);
        let mut upper = text_input("upper m", &form.upper[axis]);
        if active {
            lower = lower.on_input(move |v| Message::PhysicsForm(PhysicsAction::Lower(axis, v)));
            upper = upper.on_input(move |v| Message::PhysicsForm(PhysicsAction::Upper(axis, v)));
        }
        content = content.push(row![text(*label), lower, upper].spacing(4));
    }
    let mut grid = checkbox(form.grid)
        .label("Cartesian cells (otherwise continuous)")
        .text_size(11);
    if active {
        grid = grid.on_toggle(|v| Message::PhysicsForm(PhysicsAction::Grid(v)));
    }
    content = content.push(grid);
    if form.grid {
        let mut cells = row![].spacing(4);
        for axis in 0..3 {
            let mut field = text_input("cells", &form.cells[axis]);
            if active {
                field =
                    field.on_input(move |v| Message::PhysicsForm(PhysicsAction::Cells(axis, v)));
            }
            cells = cells.push(field);
        }
        content = content.push(cells);
    }
    let mut step = text_input("seconds per step", &form.step);
    if active {
        step = step.on_input(|v| Message::PhysicsForm(PhysicsAction::Step(v)));
    }
    content = content
        .push(text("Fixed timestep (seconds)").size(12))
        .push(step);
    let mut consent = checkbox(form.confirmed).label("Replace domain/physics and reset all initial field states and integrator history. Preserve objects; allow Undo.").text_size(11);
    let mut apply = button("Apply physics / initialize").width(Length::Fill);
    if active {
        consent = consent.on_toggle(|v| Message::PhysicsForm(PhysicsAction::Confirm(v)));
        if form.confirmed {
            apply = apply.on_press(Message::PhysicsForm(PhysicsAction::Apply));
        }
    }
    content.push(consent).push(apply).into()
}

fn parameter<'a>(
    model: &'a Model,
    kernel: usize,
    property: &'a Property,
    active: bool,
) -> Element<'a, Message> {
    let input = model.physics_form.parameters.get(kernel, &property.id);
    let mut content = column![
        text(format!(
            "{}{}",
            property.id,
            if property.required {
                " (required)"
            } else {
                " (optional)"
            }
        ))
        .size(12)
    ]
    .spacing(4);
    let mut provide = checkbox(input.is_some())
        .label("Provide value (otherwise default / omitted)")
        .text_size(11);
    if active {
        let id = property.id.clone();
        provide = provide.on_toggle(move |provided| {
            Message::PhysicsForm(PhysicsAction::Parameter(ParameterAction {
                kernel,
                property: id.clone(),
                input: provided.then(|| default_input(property)),
            }))
        });
    }
    content = content.push(provide);
    match &property.schema {
        PropertyType::Quantity {
            dimension,
            default_expression,
            minimum_si,
            maximum_si,
        } => {
            let value = match input {
                Some(ConfigurationInput::Expression { source }) => source.as_str(),
                _ => default_expression.as_deref().unwrap_or(""),
            };
            let mut field = text_input("quantity expression with units", value);
            if active && input.is_some() {
                let id = property.id.clone();
                field = field.on_input(move |source| {
                    Message::PhysicsForm(PhysicsAction::Parameter(ParameterAction {
                        kernel,
                        property: id.clone(),
                        input: Some(ConfigurationInput::Expression { source }),
                    }))
                });
            }
            content = content.push(field).push(
                text(format!(
                    "Dimension: {dimension}; SI range: {} … {}",
                    minimum_si.map_or_else(|| "unbounded".into(), |v| v.get().to_string()),
                    maximum_si.map_or_else(|| "unbounded".into(), |v| v.get().to_string())
                ))
                .size(10),
            );
        }
        PropertyType::Text { default, max_bytes } => {
            let value = match input {
                Some(ConfigurationInput::Text { value }) => value.as_str(),
                _ => default.as_deref().unwrap_or(""),
            };
            let mut field = text_input("text value", value);
            if active && input.is_some() {
                let id = property.id.clone();
                field = field.on_input(move |value| {
                    Message::PhysicsForm(PhysicsAction::Parameter(ParameterAction {
                        kernel,
                        property: id.clone(),
                        input: Some(ConfigurationInput::Text { value }),
                    }))
                });
            }
            content = content
                .push(field)
                .push(text(format!("Maximum {max_bytes} UTF-8 bytes")).size(10));
        }
        PropertyType::Boolean { default } => {
            let value = match input {
                Some(ConfigurationInput::Boolean { value }) => *value,
                _ => default.unwrap_or(false),
            };
            let mut field = checkbox(value)
                .label(if input.is_none() && default.is_none() {
                    "Not provided"
                } else {
                    "Value"
                })
                .text_size(11);
            if active && input.is_some() {
                let id = property.id.clone();
                field = field.on_toggle(move |value| {
                    Message::PhysicsForm(PhysicsAction::Parameter(ParameterAction {
                        kernel,
                        property: id.clone(),
                        input: Some(ConfigurationInput::Boolean { value }),
                    }))
                });
            }
            content = content.push(field);
        }
    }
    content.into()
}

fn default_input(property: &Property) -> ConfigurationInput {
    match &property.schema {
        PropertyType::Quantity {
            default_expression, ..
        } => ConfigurationInput::Expression {
            source: default_expression.clone().unwrap_or_default(),
        },
        PropertyType::Boolean { default } => ConfigurationInput::Boolean {
            value: default.unwrap_or(false),
        },
        PropertyType::Text { default, .. } => ConfigurationInput::Text {
            value: default.clone().unwrap_or_default(),
        },
    }
}
