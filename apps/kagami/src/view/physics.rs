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
    let replacing = model.component_form.captured_reset();
    if replacing && !model.component_reset.is_staged() {
        return column![super::component_choices::view(model), text("Stage the replacement component's values and provider choices before selecting complete reset physics.").size(11)].into();
    }
    let active = model.is_authoring()
        && !model.scientific_effects.is_pending()
        && !model.physics_dependencies.is_pending()
        && !model.component_dependencies.is_pending();
    if model.kernel_choices.is_empty() {
        return column![
            super::component_choices::view(model),
            text("No enabled field models or integrators."),
            text("Install or enable local bundles in Plugins, then Refresh this inventory.")
                .size(11)
        ]
        .into();
    }
    let mut content = column![super::component_choices::view(model), text("Configure physics").size(16), text("Select an integrator and field models explicitly. Parameters use declared defaults unless overridden. New proposals use binary64 and direct-quality samples; copied settings retain their policies. Unsupported inputs/dependencies are refused.").size(11)].spacing(8);
    if replacing {
        content = content.push(text("FULL REPLACEMENT: domain/timestep were copied, but no old model, parameters, precision or sampling policy were selected implicitly. Select every field model and the integrator to keep. New defaults are shown below. The integrator governs objects carrying its exact bound Dynamics component; other objects remain kinematic. Review complete provider choices before consent.").size(11));
    }
    if model.document.snapshot().setup().scientific().is_some() && !replacing {
        let mut copy = button("Copy captured settings into form");
        if active {
            copy = copy.on_press(Message::PhysicsForm(PhysicsAction::LoadCaptured));
        }
        content = content.push(copy);
    }
    if form.is_captured() {
        content = content.push(text("Editing captured settings: exact model/instance IDs, dependency pins, precision and sampling policy are preserved. A field-only reset preserves other fields/history and requires unchanged domain, timestep and other model parameters. The whole-setup Apply below resets all initial fields/history.").size(11));
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
            if form.is_captured() && choice.contract == orishu_plugin::ExecutionContractId::Field {
                let mut consent = checkbox(form.field_confirmed(i))
                    .label("Reset only this field's initial state with its edited parameters; allow Undo.")
                    .text_size(11);
                let mut apply = button("Apply only this field's parameters");
                if active {
                    consent = consent.on_toggle(move |v| {
                        Message::PhysicsForm(PhysicsAction::ConfirmField(i, v))
                    });
                    if form.field_confirmed(i) {
                        apply = apply.on_press(Message::PhysicsForm(PhysicsAction::ApplyField(i)));
                    }
                }
                content = content.push(consent).push(apply);
            }
        }
    }
    if let Some(error) = form.parameters.error() {
        content = content.push(text(error).size(12));
    }
    content = content.push(dependencies(model, active));
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
    if replacing {
        use crate::component_form::{Action, reset::Flow};
        let (component, physics) = Flow::tokens(&model.component_form, form);
        let confirmed = model.component_reset.confirmed(&model.component_form, form);
        let consent = checkbox(confirmed).label("Apply displayed component/provider changes, replace complete physics, and reset ALL fields/integrator history. No old state or component values are migrated. Allow Undo.").text_size(11)
            .on_toggle_maybe((active || model.component_reset.is_running()).then_some(move |confirmed| Message::ComponentForm(Action::ConfirmReset { component, physics, confirmed })));
        let apply = button("Apply component choices and reset all physics")
            .width(Length::Fill)
            .on_press_maybe((active && confirmed).then_some(Message::ComponentForm(
                Action::ApplyReset { component, physics },
            )));
        return content.push(consent).push(apply).into();
    }
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

fn dependencies(model: &Model, active: bool) -> Element<'_, Message> {
    dependency_choices(
        &model.physics_dependencies,
        model.physics_form.bindings(),
        model.physics_form.is_captured(),
        active,
        |action| Message::PhysicsForm(PhysicsAction::Dependencies(action)),
    )
}

pub(super) fn dependency_choices<'a>(
    controller: &'a crate::physics_form::dependencies::Controller,
    bindings: impl Iterator<Item = orishu_plugin::resolution::ProviderBinding>,
    captured: bool,
    active: bool,
    message: fn(crate::physics_form::dependencies::Action) -> Message,
) -> Element<'a, Message> {
    use crate::physics_form::dependencies::Action;
    use orishu_plugin::resolution::{ResolutionOutcome, UnavailableReason};
    let control = |label: &'static str, action, enabled: bool| {
        button(label).on_press_maybe(enabled.then_some(message(action)))
    };
    let mut content = column![
        text("Dependency providers").size(14),
        text("Checking never runs kernels or edits the experiment. Multiple compatible providers require an explicit choice; Apply revalidates every pin.").size(11),
        text(&controller.notice).size(11),
        control("Check dependencies", Action::Check, active),
    ].spacing(6);
    let mut has_bindings = false;
    for binding in bindings {
        has_bindings = true;
        content = content.push(
            text(format!(
                "{} / {} → {} / {} / {}",
                binding.requirement.consumer.local_id,
                binding.requirement.slot,
                binding.provider.release,
                binding.provider.extension_point,
                binding.provider.local_id
            ))
            .size(10),
        );
    }
    if !captured && has_bindings {
        content = content.push(control(
            "Clear local provider choices",
            Action::Clear,
            active,
        ));
    }
    if let Some(report) = controller.report() {
        match report.outcome() {
            ResolutionOutcome::Resolved {
                inventory_revision,
                selection,
            } => {
                content = content.push(text(format!("Resolved at inventory {inventory_revision}: {} contributions, {} exact dependency bindings.", selection.contributions.len(), selection.bindings.len())).size(11));
                for binding in &selection.bindings {
                    content = content
                        .push(
                            text(format!(
                                "{} / {} → {} / {}",
                                binding.requirement.consumer.local_id,
                                binding.requirement.slot,
                                report
                                    .provider_plugin(&binding.provider)
                                    .unwrap_or("unknown plugin"),
                                binding.provider.release
                            ))
                            .size(10),
                        )
                        .push(control(
                            "Browse installed alternatives",
                            Action::Browse {
                                report: report.token(),
                                requirement: binding.requirement.clone(),
                            },
                            active && !captured,
                        ));
                }
            }
            ResolutionOutcome::Unavailable {
                inventory_revision,
                issues,
                truncated,
            } => {
                content = content.push(
                    text(format!(
                        "Unresolved at inventory {inventory_revision}. {}",
                        if *truncated {
                            "Diagnostic list is truncated; do not assume all failures are shown."
                        } else {
                            ""
                        }
                    ))
                    .size(11),
                );
                for (i, issue) in issues.iter().enumerate() {
                    content = content.push(
                        text(format!(
                            "{} / {} / {}: {:?}\nRequirement: {}",
                            issue.contribution.release,
                            issue.contribution.extension_point,
                            issue.contribution.local_id,
                            issue.reason,
                            issue.requirement.as_ref().map_or_else(
                                || "root".into(),
                                |r| format!(
                                    "{} / {} / {} / {}",
                                    r.consumer.release,
                                    r.consumer.extension_point,
                                    r.consumer.local_id,
                                    r.slot
                                )
                            )
                        ))
                        .size(10),
                    );
                    if let Some(requirement) = &issue.requirement {
                        if issue.reason == UnavailableReason::UnusedBinding {
                            content = content.push(control(
                                "Discard unused local binding",
                                Action::ForgetUnused {
                                    report: report.token(),
                                    requirement: requirement.clone(),
                                },
                                active && !captured,
                            ));
                        }
                        content = content.push(control(
                            "Browse installed alternatives",
                            Action::Browse {
                                report: report.token(),
                                requirement: requirement.clone(),
                            },
                            active && !captured,
                        ));
                    }
                    if issue.reason != UnavailableReason::AmbiguousProvider {
                        continue;
                    }
                    let start = report.offset(i);
                    content = content.push(
                        text(format!(
                            "Candidates {}–{} of {}",
                            start + 1,
                            start + issue.candidates.len(),
                            issue.candidate_count
                        ))
                        .size(10),
                    );
                    for (n, provider) in issue.candidates.iter().enumerate() {
                        content = content
                            .push(
                                text(format!(
                                    "{}\n{} / {} / {}",
                                    report
                                        .provider_plugin(provider)
                                        .unwrap_or("Unavailable plugin identity"),
                                    provider.release,
                                    provider.extension_point,
                                    provider.local_id
                                ))
                                .size(10),
                            )
                            .push(control(
                                "Choose this exact provider",
                                Action::Choose {
                                    report: report.token(),
                                    issue: i,
                                    candidate: n,
                                },
                                active && !captured,
                            ));
                    }
                    let page_size =
                        orishu_plugin::resolution::ResolutionLimits::default().max_candidates;
                    let next = start + issue.candidates.len();
                    content = content.push(
                        row![
                            control(
                                "Previous candidates",
                                Action::Page {
                                    report: report.token(),
                                    issue: i,
                                    offset: start.saturating_sub(page_size)
                                },
                                active && start > 0
                            ),
                            control(
                                "Next candidates",
                                Action::Page {
                                    report: report.token(),
                                    issue: i,
                                    offset: next
                                },
                                active && next < issue.candidate_count
                            ),
                        ]
                        .spacing(4),
                    );
                }
            }
            ResolutionOutcome::StaleRevision { expected, actual } => {
                content = content.push(text(format!("Inventory changed ({expected} → {actual}); Refresh plugins, then check again.")).size(11));
            }
        }
        if let Some(page) = report.alternatives() {
            content = content.push(text(format!("Explicit installed providers for {} / {} ({} candidates). Includes non-default releases; local-only dependency slots have no alternatives.", page.requirement.consumer.local_id, page.requirement.slot, page.total)).size(11));
            for (candidate, provider) in page.candidates.iter().enumerate() {
                content = content
                    .push(
                        text(format!(
                            "{}\n{} / {} / {}",
                            report.provider_plugin(provider).unwrap_or("unknown plugin"),
                            provider.release,
                            provider.extension_point,
                            provider.local_id
                        ))
                        .size(10),
                    )
                    .push(control(
                        "Use this installed provider",
                        Action::ChooseInstalled {
                            report: report.token(),
                            candidate,
                        },
                        active && !captured,
                    ));
            }
            let size = orishu_plugin::resolution::ResolutionLimits::default().max_candidates;
            let next = page.offset + page.candidates.len();
            content = content.push(
                row![
                    control(
                        "Previous installed providers",
                        Action::BrowsePage {
                            report: report.token(),
                            offset: page.offset.saturating_sub(size)
                        },
                        active && page.offset > 0
                    ),
                    control(
                        "Next installed providers",
                        Action::BrowsePage {
                            report: report.token(),
                            offset: next
                        },
                        active && next < page.total
                    ),
                ]
                .spacing(4),
            );
        }
    }
    content.into()
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
