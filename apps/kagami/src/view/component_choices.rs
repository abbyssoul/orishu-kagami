//! Reuse provider discovery without asking the author to select a field/solver.
use crate::{component_form::Action, message::Message, model::Model, physics_form::dependencies};
use iced::{
    Element,
    widget::{button, checkbox, column, text},
};
use orishu_plugin::resolution::ResolutionOutcome;

mod properties;

pub(super) fn view(model: &Model) -> Element<'_, Message> {
    let active = model.is_authoring()
        && !model.scientific_effects.is_pending()
        && !model.physics_dependencies.is_pending()
        && !model.component_dependencies.is_pending();
    let current = model.component_form.current(&model.document);
    let mut content = column![
        text("Component provider choices").size(16),
        text("Resolve component dependencies before acceptance. Captured Add preserves fields/providers and regenerates history with consent. Captured replacement instead stages new values/choices, then requires complete physics and explicit full-reset consent. Legacy logical components remain unresolved.").size(11),
        button("Load current component choices").on_press_maybe(active.then_some(Message::ComponentForm(Action::Load))),
    ].spacing(6);
    if current
        || model.component_form.attachment().is_some()
        || model.component_form.replacement().is_some()
    {
        content = content.push(
            button("Cancel component proposal").on_press(Message::ComponentForm(Action::Cancel)),
        );
    }
    if !current {
        return content
            .push(text(&model.component_dependencies.notice).size(11))
            .push(text("Load the current document before checking or saving choices.").size(11))
            .into();
    }
    if let Some((object, component)) = model.component_form.attachment() {
        content = content.push(
            text(format!(
                "Proposed component: {component} on object {object}."
            ))
            .size(11),
        );
    }
    if let Some((_, previous)) = model.component_form.replacement() {
        content = content.push(text(format!("Replace {previous}. Old values will be removed only on acceptance. New values come from the new declaration/defaults or your explicit input; no automatic value conversion.")).size(11));
        if model.component_form.attachment().is_none() {
            return content.push(text("Choose a replacement from this object's component list. The experiment is unchanged.").size(11)).into();
        }
    }
    if let Some(fields) = model.component_form.properties() {
        content = content.push(properties::view(fields, active));
    } else if model.component_form.attachment().is_some() {
        content = content.push(text("Check dependencies, then load the selected component's property fields to enter values without attaching it.").size(11));
    }
    if model.component_form.captured_reset() {
        content = content.push(text("Replacement may explicitly change shared dependency providers. Review every changed binding: its effect is not limited to this object's component. Staging never changes the experiment; changing component inputs later discards the staged physics proposal and consent.").size(11));
        if model.component_reset.is_staged() {
            content = content.push(text("Component choices staged. Complete the physics/reset proposal below (Scientific setup / fields). All existing fields/history will be reinitialized; none is migrated.").size(11));
        }
    }
    if model.component_form.captured_addition() {
        let proposal = model.component_form.proposal_id();
        content = content.push(checkbox(model.component_form.history_confirmed())
            .label("Add this component and regenerate initial integrator history. Preserve fields and existing providers; allow Undo.")
            .text_size(11)
            .on_toggle_maybe((active || model.component_extension.is_some()).then_some(move |confirmed| Message::ComponentForm(Action::ConfirmHistory { proposal, confirmed }))));
    }
    content = content.push(super::physics::dependency_choices(
        &model.component_dependencies,
        model.component_form.bindings(),
        false,
        active,
        |action| Message::ComponentForm(Action::Dependencies(action)),
    ));
    if let Some(report) = model.component_dependencies.report()
        && matches!(report.outcome(), ResolutionOutcome::Resolved { .. })
    {
        if model.component_form.attachment().is_some()
            && model.component_form.properties().is_none()
        {
            content = content.push(
                button("Load property fields (no document change)").on_press_maybe(
                    active.then_some(Message::ComponentForm(Action::Dependencies(
                        dependencies::Action::LoadProperties {
                            report: report.token(),
                        },
                    ))),
                ),
            );
        }
        let label = if model.component_form.captured_reset() {
            "Stage reset choices (no document change)"
        } else if model.component_form.replacement().is_some() {
            "Replace component and save choices"
        } else if model.component_form.captured_addition() {
            "Add component and regenerate history"
        } else if model.component_form.attachment().is_some() {
            "Attach component and save choices"
        } else {
            "Save component choices (undoable)"
        };
        let can_apply = active
            && (!model.component_form.captured_addition()
                || model.component_form.history_confirmed());
        content = content.push(button(label).on_press_maybe(can_apply.then_some(
            Message::ComponentForm(Action::Dependencies(dependencies::Action::ApplyLock {
                report: report.token(),
            })),
        )));
    }
    content.into()
}
