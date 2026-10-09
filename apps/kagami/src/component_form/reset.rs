//! Two-stage native replacement: checked component intent, then explicit physics.
//! No candidate or schema is adopted until the full reset lane accepts everything.
use super::ComponentForm;
use crate::{
    model::Model,
    physics_form::{PhysicsForm, dependencies::ComponentIntent},
    scientific_effect::ComponentResetRequest,
};
use uuid::Uuid;

struct Staged {
    intent: ComponentIntent,
    consent: Option<Uuid>,
}

/// Local staged inputs and ownership of a still-running scientific operation.
#[derive(Default)]
pub struct Flow {
    staged: Option<Staged>,
    running: Option<(Uuid, Uuid)>,
}
impl Flow {
    /// Whether replacement choices await complete physics/reset confirmation.
    pub fn is_staged(&self) -> bool {
        self.staged.is_some()
    }
    /// A cancelled operation retains ownership until its actual thread exits.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }
    /// Identity carried by consent and Apply messages, not accepted intent.
    pub fn tokens(component: &ComponentForm, physics: &PhysicsForm) -> (Uuid, Uuid) {
        (component.proposal_id(), physics.generation())
    }
    /// Consent belongs to this exact component/physics pair.
    pub fn confirmed(&self, component: &ComponentForm, physics: &PhysicsForm) -> bool {
        self.staged.as_ref().is_some_and(|s| {
            s.intent.generation == component.proposal_id()
                && s.consent == Some(physics.generation())
        })
    }
}

fn current(model: &Model) -> bool {
    model.component_reset.staged.as_ref().is_some_and(|s| {
        model.component_form.captured_reset()
            && model.component_form.proposal_id() == s.intent.generation
            && model.document.accepts_effect(s.intent.guard)
            && model.inventory_revision == Some(s.intent.revision)
    })
}

pub(crate) fn stage(model: &mut Model, intent: ComponentIntent) {
    if !intent.reset
        || !model.component_form.captured_reset()
        || model.component_form.proposal_id() != intent.generation
        || !model.document.accepts_effect(intent.guard)
        || model.inventory_revision != Some(intent.revision)
        || model.scientific_effects.is_pending()
    {
        model.component_dependencies.notice =
            "Replacement context changed; prepare its choices again.".into();
        return;
    }
    match PhysicsForm::for_component_reset(&model.document, intent.lock.clone()) {
        Ok(form) => {
            model.physics_form = form;
            model.selected = None; // Show the second, explicit physics stage.
            model.component_reset.staged = Some(Staged {
                intent,
                consent: None,
            });
            model.component_dependencies.notice = "Replacement values and choices staged; nothing accepted. Domain and timestep were copied. Select every desired field model and integrator below, check dependencies, then confirm replacing the component and resetting ALL initial scientific state.".into();
        }
        Err(error) => model.component_dependencies.notice = error.into(),
    }
}

/// Clear proposed intent but retain ownership of any cancelled running job.
pub(crate) fn cancel(model: &mut Model) {
    if model.component_reset.running.is_some() {
        model.scientific_effects.cancel();
    }
    if model.component_reset.staged.take().is_some() {
        model.physics_form = Default::default();
    }
}

pub(crate) fn confirm(model: &mut Model, component: Uuid, physics: Uuid, confirmed: bool) {
    let result = (|| {
        if !current(model)
            || Flow::tokens(&model.component_form, &model.physics_form) != (component, physics)
        {
            return Err("Reset consent is stale; use the current component and physics proposal.");
        }
        if confirmed {
            if model.scientific_effects.is_pending()
                || model.component_dependencies.is_pending()
                || model.physics_dependencies.is_pending()
            {
                return Err("Wait for preparation to finish before confirming reset.");
            }
            model.physics_dependencies.resolved_request(
                &model.document,
                &model.physics_form,
                model.inventory_revision,
            )?;
            model.physics_form.component_reset_request(
                &model.kernel_choices,
                model.inventory_revision.unwrap(),
            )?;
        }
        model.component_reset.staged.as_mut().unwrap().consent = confirmed.then_some(physics);
        if !confirmed && model.component_reset.running.is_some() {
            model.scientific_effects.cancel();
        }
        Ok(())
    })();
    model.component_dependencies.notice = match result {
        Ok(()) if confirmed => "Full reset confirmed for these exact component and physics inputs. Apply will replace the component and discard all old field/history state atomically; Undo remains available.".into(),
        Ok(()) => "Full-reset consent withdrawn; experiment unchanged.".into(),
        Err(error) => error.into(),
    };
}

pub(crate) fn apply(model: &mut Model, component: Uuid, physics: Uuid) {
    let result = (|| {
        if !current(model)
            || Flow::tokens(&model.component_form, &model.physics_form) != (component, physics)
            || !model
                .component_reset
                .confirmed(&model.component_form, &model.physics_form)
        {
            return Err(
                "Confirm the exact current component/physics full-reset proposal first.".to_owned(),
            );
        }
        if model.component_dependencies.is_pending()
            || model.physics_dependencies.is_pending()
            || model.scientific_effects.is_pending()
        {
            return Err("Wait for the active preparation to finish.".to_owned());
        }
        let staged = model.component_reset.staged.as_ref().unwrap();
        let mut setup = model
            .physics_form
            .component_reset_request(&model.kernel_choices, staged.intent.revision)
            .map_err(str::to_owned)?;
        setup.selection = model
            .physics_dependencies
            .resolved_request(
                &model.document,
                &model.physics_form,
                model.inventory_revision,
            )
            .map_err(str::to_owned)?;
        model
            .scientific_effects
            .reset_components(
                &model.document,
                staged.intent.guard,
                ComponentResetRequest {
                    edits: staged.intent.attachments.clone(),
                    dependencies: staged.intent.lock.clone(),
                    setup,
                },
            )
            .map_err(|e| e.to_string())?;
        model.component_reset.running = Some((component, physics));
        Ok(())
    })();
    // A real preflight refusal also requires fresh consent. A duplicate/stale
    // Apply must not revoke consent for an operation that already owns the lane.
    if result.is_err()
        && !model.scientific_effects.is_pending()
        && Flow::tokens(&model.component_form, &model.physics_form) == (component, physics)
        && let Some(staged) = &mut model.component_reset.staged
    {
        staged.consent = None;
    }
    model.component_dependencies.notice = match result {
        Ok(()) => "Preparing complete replacement; no component, schema, choice or scientific state changes until atomic acceptance.".into(),
        Err(error) => error,
    };
}

pub(crate) fn before_poll(model: &mut Model) {
    if model.component_reset.staged.is_some() && !current(model) {
        cancel(model);
        model.component_dependencies.notice = "Replacement inputs/context changed. Stage component choices and configure the complete reset again.".into();
    }
    if model.component_reset.running.is_some_and(|tokens| {
        tokens != Flow::tokens(&model.component_form, &model.physics_form)
            || !model
                .component_reset
                .confirmed(&model.component_form, &model.physics_form)
    }) {
        model.scientific_effects.cancel();
    }
}

pub(crate) fn after_poll(model: &mut Model, accepted: bool) {
    if model.component_reset.running.is_none() || model.scientific_effects.is_pending() {
        return;
    }
    model.component_reset.running = None;
    if accepted {
        model.component_reset.staged = None;
        model.component_form = Default::default();
        model.physics_form = Default::default();
        model.component_dependencies.notice = "Component, provider choices, schemas and freshly initialized physics accepted in one undoable revision.".into();
    } else {
        if let Some(staged) = &mut model.component_reset.staged {
            staged.consent = None;
        }
        model.component_dependencies.notice = model.document.notice.clone().unwrap_or_else(|| "Reset refused; original experiment retained. Correct inputs and explicitly confirm/retry.".into());
    }
}
