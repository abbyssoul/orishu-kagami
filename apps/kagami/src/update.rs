//! Turning one message into either an envelope, a view change, or a mode
//! change.
//!
//! The shape is deliberate: [`update`] dispatches on which *queue* a message
//! belongs to before it does anything else, so there is no arm in which a
//! selection change could accidentally reach the authority or an edit could
//! accidentally be applied locally.
//!
//! The mode gate is not here. It sits on
//! [`Document::submit`](crate::document::Document::submit), the one route to
//! the authority, so no call site added later can forget it. ADR 0022 keeps
//! client mode out of the authority's own reasons for refusing a command, and
//! because the gate runs before an envelope is built, an edit arriving in the
//! wrong mode cannot change the mode as a side effect of being refused.

use kagami_document::{DisplayName, ExperimentCommand, ObjectSpec};
use kagami_session::{
    AuthoringView, LeaveConsequence, SceneScale, SessionCommand, ViewAdjustment, ViewError,
};

use crate::mcp::{self, McpState, SessionState};
use crate::message::{Authoritative, ClientLocal, McpControl, Message, WorkspaceIntent};
use crate::model::{Model, PropertyEdit};

mod removal;

pub fn update(model: &mut Model, message: Message) -> iced::Task<Message> {
    let task = match message {
        #[cfg(unix)]
        Message::Catalog(action) => {
            if action == crate::catalog_form::Action::Open {
                model.plugin_management.open = false;
            }
            let action = if action == crate::catalog_form::Action::PickDirectory {
                if !model.catalog.is_pending()
                    && !model.scientific_effects.is_pending()
                    && model.is_authoring()
                {
                    rfd::FileDialog::new()
                        .pick_folder()
                        .map(crate::catalog_form::Action::Load)
                } else {
                    None
                }
            } else {
                Some(action)
            };
            if let Some(action) = action {
                model.catalog.act(
                    action,
                    &mut model.document,
                    &mut model.scientific_effects,
                    model.inventory_revision,
                );
            }
            iced::Task::none()
        }
        #[cfg(unix)]
        Message::Plugins(action) => {
            if matches!(
                &action,
                crate::plugins::window::Action::ConfirmRemoval(_)
                    | crate::plugins::window::Action::Mutate(
                        crate::plugins::InventoryCommand::Remove { .. }
                    )
            ) && !model.plugin_references.settled(&model.document)
            {
                model.plugin_management.notice = "Complete reference reconciliation before removal; acknowledgement cannot bypass unknown usage.".into();
            } else if action == crate::plugins::window::Action::RetryReferences {
                model.plugin_references.retry(&mut model.document);
            } else if let crate::plugins::window::Action::References(release) = action {
                match model
                    .document
                    .plugin_references(kagami_session::PluginReferenceLimits::default())
                {
                    Ok(report) => {
                        let usage = report.releases.get(&release).copied().unwrap_or_default();
                        model.plugin_management.inspection = Some(format!(
                            "{release}\nReferences at document revision {}:\nCurrent experiment: {}\nUndo/redo: {}\nRetained command data: {}\nPoint-in-time report: re-query after edits. This session only; no scan of unopened files or other processes. This report does not acquire a filesystem lease.",
                            model.document.snapshot().revision().get(),
                            usage.current,
                            usage.history,
                            usage.receipts
                        ));
                    }
                    Err(error) => {
                        model.plugin_management.inspection = None;
                        model.plugin_management.notice = format!(
                            "Reference query incomplete: {error}. Absence of references cannot be assumed."
                        );
                    }
                }
            } else if let crate::plugins::window::Action::PickBundle(update) = action {
                if !model.plugin_management.is_pending()
                    && model.plugin_management.listing.is_some()
                    && let Some(path) = rfd::FileDialog::new().pick_file()
                {
                    model
                        .plugin_management
                        .act(crate::plugins::window::Action::Install { path, update });
                }
            } else {
                model.plugin_management.act(action);
            }
            iced::Task::none()
        }
        #[cfg(unix)]
        Message::Workload(action) => {
            use crate::workload_preparation::Action;
            let result = match action {
                Action::Name(value) => {
                    model.workload_preparation.edit_name(value);
                    Ok(())
                }
                Action::Formation(value) => {
                    model.workload_preparation.edit_formation(value);
                    Ok(())
                }
                Action::Prepare if model.run.submission().is_none() && !model.run.is_pending() => {
                    model.workload_preparation.prepare(&model.document)
                }
                Action::Cancel => {
                    model.workload_preparation.cancel();
                    Ok(())
                }
                Action::Export => {
                    if model.workload_preparation.ready(&model.document).is_some()
                        && !model.workload_preparation.is_pending()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_file_name("experiment.orishu")
                            .save_file()
                    {
                        return update(model, Message::Workload(Action::ExportTo(path)));
                    }
                    Ok(())
                }
                Action::ExportTo(path) => model.workload_preparation.export(&model.document, path),
                Action::Submit => {
                    if !model.workload_preparation.is_pending()
                        && let Some(workload) = model.workload_preparation.ready(&model.document)
                    {
                        match model.workload_preparation.formation.parse() {
                            Ok(formation) => model.run.submit(workload, formation),
                            Err(_) => model.workload_preparation.notice = "Enter the exact target formation ID; no automatic formation selection or lock is performed.".into(),
                        }
                    }
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                model.workload_preparation.notice = error.to_string();
            }
            iced::Task::none()
        }
        Message::Run(action) => {
            if action != crate::run::Action::Observe || model.is_authoring() {
                let attaching = action == crate::run::Action::Observe && !model.run.is_pending();
                model.run.act(action);
                if attaching && model.run.is_pending() {
                    model.run_attach_guard = model.document.authoring_guard();
                }
            }
            iced::Task::none()
        }
        #[cfg(unix)]
        Message::ComponentForm(action) => {
            let handled = match &action {
                crate::component_form::Action::Cancel => {
                    crate::component_form::reset::cancel(model);
                    if model.component_extension.is_some() {
                        model.scientific_effects.cancel();
                    }
                    model.component_form = Default::default();
                    model.component_dependencies.notice =
                        "Component proposal cancelled; experiment unchanged.".into();
                    true
                }
                crate::component_form::Action::ConfirmReset {
                    component,
                    physics,
                    confirmed,
                } => {
                    crate::component_form::reset::confirm(model, *component, *physics, *confirmed);
                    true
                }
                crate::component_form::Action::ApplyReset { component, physics } => {
                    crate::component_form::reset::apply(model, *component, *physics);
                    true
                }
                crate::component_form::Action::ConfirmHistory {
                    proposal,
                    confirmed,
                } => {
                    match model.component_form.confirm_history(
                        &model.document,
                        *proposal,
                        *confirmed,
                    ) {
                        Ok(()) => {
                            if !confirmed && model.component_extension.is_some() {
                                model.scientific_effects.cancel();
                            }
                            model.component_dependencies.notice = if *confirmed {
                                "History regeneration confirmed for these component inputs; fields and existing providers will be retained.".into()
                            } else {
                                "History regeneration consent withdrawn; experiment unchanged."
                                    .into()
                            };
                        }
                        Err(error) => model.component_dependencies.notice = error.into(),
                    }
                    true
                }
                _ => false,
            };
            if !handled
                && !model.scientific_effects.is_pending()
                && !model.physics_dependencies.is_pending()
            {
                match action {
                    crate::component_form::Action::BeginReplacement { object, component }
                        if !model.component_dependencies.is_pending() =>
                    {
                        match crate::component_form::ComponentForm::begin_replacement(
                            &model.document,
                            object,
                            component,
                        ) {
                            Ok(form) => {
                                crate::component_form::reset::cancel(model);
                                model.component_form = form;
                                model.selected = Some(object);
                                model.document.notice = None;
                                model.component_dependencies.notice = "Choose an exact replacement below. Old values remain until acceptance; new values are configured explicitly, not copied by name.".into();
                            }
                            Err(error) => model.document.notice = Some(error.into()),
                        }
                    }
                    crate::component_form::Action::ReplaceWith {
                        proposal,
                        component,
                    } if !model.component_dependencies.is_pending() => {
                        match model.component_form.replace_with(
                            &model.document,
                            proposal,
                            component,
                        ) {
                            Ok(form) => {
                                crate::component_form::reset::cancel(model);
                                model.component_form = form;
                                model.document.notice = None;
                                model.component_dependencies.notice = "Replacement proposed. Configure new values, check dependencies and explicitly replace to accept all changes together.".into();
                            }
                            Err(error) => model.document.notice = Some(error.into()),
                        }
                    }
                    crate::component_form::Action::BeginReplacement { .. }
                    | crate::component_form::Action::ReplaceWith { .. } => {}
                    crate::component_form::Action::SetProperty {
                        form,
                        property,
                        value,
                    } => {
                        model.component_dependencies.notice = match model.component_form.set_property(&model.document, form, property, value) {
                            Ok(()) => "Local property changed; check dependencies again before saving. The experiment is unchanged.".into(),
                            Err(error) => error.into(),
                        };
                    }
                    crate::component_form::Action::Cancel
                    | crate::component_form::Action::ConfirmReset { .. }
                    | crate::component_form::Action::ApplyReset { .. }
                    | crate::component_form::Action::ConfirmHistory { .. } => {
                        unreachable!("handled above")
                    }
                    crate::component_form::Action::Load
                        if !model.component_dependencies.is_pending() =>
                    {
                        match crate::component_form::ComponentForm::from_document(&model.document) {
                            Ok(form) => {
                                model.component_form = form;
                                model.component_dependencies.notice = if model
                                    .component_form
                                    .captured_reset()
                                {
                                    "Captured component choices copied. Changed bindings require staging these choices and explicitly resetting complete physics; nothing is accepted here.".into()
                                } else {
                                    "Accepted component choices copied; check dependencies before saving.".into()
                                };
                            }
                            Err(error) => model.component_dependencies.notice = error.into(),
                        }
                    }
                    crate::component_form::Action::Load => {}
                    crate::component_form::Action::Dependencies(action) => {
                        model.component_dependencies.act(
                            action,
                            &model.document,
                            &mut model.component_form,
                            &[],
                            model.inventory_revision,
                        )
                    }
                }
            }
            iced::Task::none()
        }
        #[cfg(unix)]
        Message::PhysicsForm(action) => {
            use crate::physics_form::PhysicsAction;
            if model.component_form.captured_reset()
                && (!model.component_reset.is_staged()
                    || matches!(
                        action,
                        PhysicsAction::LoadCaptured
                            | PhysicsAction::Reset
                            | PhysicsAction::Apply
                            | PhysicsAction::ApplyField(_)
                            | PhysicsAction::Confirm(_)
                            | PhysicsAction::ConfirmField(_, _)
                    ))
            {
                model.document.notice = Some("Stage replacement component choices, then use the combined full-reset confirmation and Apply. Ordinary physics/field actions cannot accept this replacement.".into());
            } else if model.component_dependencies.is_pending() {
                // Its shared polling tick still reaches the completion fold below.
            } else if let PhysicsAction::Dependencies(action) = action {
                if !model.scientific_effects.is_pending() {
                    model.physics_dependencies.act(
                        action,
                        &model.document,
                        &mut model.physics_form,
                        &model.kernel_choices,
                        model.inventory_revision,
                    );
                }
            } else if action == PhysicsAction::LoadCaptured {
                let result = model
                    .inventory_revision
                    .ok_or("No plugin inventory is configured.")
                    .and_then(|revision| {
                        crate::physics_form::PhysicsForm::from_document(
                            &model.document,
                            &model.kernel_choices,
                            revision,
                        )
                    });
                match result {
                    Ok(form) => {
                        model.physics_form = form;
                        model.document.notice = Some("Captured settings copied; the experiment is unchanged. Confirm a field-only parameter reset or the whole-setup reset explicitly.".into());
                    }
                    Err(error) => model.document.notice = Some(error.into()),
                }
            } else if let PhysicsAction::ApplyField(index) = action {
                if model.physics_dependencies.is_pending() {
                    model.document.notice =
                        Some("Wait for the dependency read before initializing physics.".into());
                    return iced::Task::none();
                }
                let request = model
                    .inventory_revision
                    .ok_or("No plugin inventory is configured.")
                    .and_then(|revision| {
                        model.physics_form.field_request(
                            &model.document,
                            &model.kernel_choices,
                            revision,
                            index,
                        )
                    });
                match request {
                    Ok((guard, input)) => match model.scientific_effects.configure_field(
                        &model.document,
                        guard,
                        input.instance,
                        input.configuration,
                    ) {
                        Ok(()) => {
                            model.physics_form.edit(
                                PhysicsAction::ConfirmField(index, false),
                                &model.kernel_choices,
                            );
                            model.document.notice = Some("Preparing this field only; other fields and integrator history are preserved. Nothing changes until atomic acceptance.".into());
                        }
                        Err(error) => model.document.notice = Some(error.to_string()),
                    },
                    Err(error) => model.document.notice = Some(error.into()),
                }
            } else if action == PhysicsAction::Apply {
                if model.physics_dependencies.is_pending() {
                    model.document.notice = Some(
                        "Wait for the dependency read to finish before initializing physics."
                            .into(),
                    );
                    return iced::Task::none();
                }
                let request = model
                    .inventory_revision
                    .ok_or("No plugin inventory is configured.")
                    .and_then(|revision| {
                        model.physics_form.request(&model.kernel_choices, revision)
                    })
                    .and_then(|mut request| {
                        request.selection = model.physics_form.selection_with_scene(
                            &model.kernel_choices,
                            request.selection.expected_inventory_revision,
                            model.document.snapshot(),
                        )?;
                        Ok(request)
                    });
                match request {
                    Ok(request) => {
                        match model.scientific_effects.configure(&model.document, request) {
                            Ok(()) => {
                                model.physics_form.confirmed = false;
                                model.document.notice = Some("Preparing selected physics; no partial initial state will be adopted.".into());
                            }
                            Err(error) => model.document.notice = Some(error.to_string()),
                        }
                    }
                    Err(error) => model.document.notice = Some(error.into()),
                }
            } else {
                model.physics_form.edit(action, &model.kernel_choices);
            }
            iced::Task::none()
        }
        #[cfg(unix)]
        Message::Scientific(action) => {
            use crate::message::ScientificAction;
            match action {
                ScientificAction::Configure(request) => {
                    match model.scientific_effects.configure(&model.document, *request) {
                        Ok(()) => model.document.notice = Some("Preparing scientific setup in the background; nothing is adopted until every kernel succeeds.".into()),
                        Err(error) => model.document.notice = Some(error.to_string()),
                    }
                }
                ScientificAction::Reinitialize(instance) => {
                    match model.scientific_effects.reinitialize(&model.document, instance) {
                        Ok(()) => model.document.notice = Some("Reinitializing field in the background; authored state is unchanged until acceptance.".into()),
                        Err(error) => model.document.notice = Some(error.to_string()),
                    }
                }
                ScientificAction::Cancel => model.scientific_effects.cancel(),
                ScientificAction::Poll => {}
            }
            iced::Task::none()
        }
        Message::Authoritative(intent) => {
            // Any submission ends whatever menu invoked it, so the window does
            // not sit open over a changed document.
            model.open_menu = None;
            submit(model, intent);
            iced::Task::none()
        }
        Message::Local(local) => {
            apply_local(model, local);
            iced::Task::none()
        }
        Message::Workspace(intent) => {
            model.open_menu = None;
            change_mode(model, intent);
            iced::Task::none()
        }
        Message::Mcp(control) => mcp_control(model, control),
        Message::Exit => return iced::exit(),
        Message::ExitTimerTick => {
            if let Some(deadline) = model.exit_deadline
                && std::time::Instant::now() >= deadline
            {
                log::info!("self-imposed lifetime reached; exiting");
                return iced::exit();
            }
            iced::Task::none()
        }
    };

    #[cfg(unix)]
    {
        let refresh = model.plugin_management.poll();
        if (model.plugin_management.take_changed_revision().is_some()
            || refresh
                .as_ref()
                .is_some_and(|r| model.inventory_revision != Some(r.plugins.revision)))
            && let Err(error) = model.document.mark_reference_update()
        {
            model.plugin_references.invalidate(error.to_string());
        }
        if let Some(refresh) = refresh
            && model.inventory_revision != Some(refresh.plugins.revision)
        {
            model.catalog.refresh_plugins(refresh.plugins.clone());
            model.document.adopt_schemas(refresh.schemas);
            model
                .scientific_effects
                .refresh_plugins(refresh.plugins.clone());
            model
                .physics_dependencies
                .refresh_plugins(refresh.plugins.clone());
            model
                .component_dependencies
                .refresh_plugins(refresh.plugins.clone());
            model.component_form = Default::default();
            model.unresolved_components = refresh.unresolved_components;
            model
                .workload_preparation
                .refresh_plugins(refresh.plugins.clone());
            model.inventory_revision = Some(refresh.plugins.revision);
            model.kernel_choices = refresh.kernels;
            model.physics_form = Default::default();
            model.plugin_management.notice.push_str(" Unapplied physics-form choices were cleared; prepared candidates were invalidated. Captured settings can be copied back explicitly.");
            model.forget_missing();
        }
        if model.component_extension.is_some_and(|generation| {
            generation != model.component_form.proposal_id()
                || !model.component_form.history_confirmed()
        }) {
            model.scientific_effects.cancel();
        }
        crate::component_form::reset::before_poll(model);
        model
            .catalog
            .before_scientific(&model.document, &mut model.scientific_effects);
        let scientific_accepted = model.scientific_effects.poll(&mut model.document);
        model.catalog.after_scientific(
            &model.document,
            &model.scientific_effects,
            scientific_accepted,
        );
        if scientific_accepted {
            model.forget_missing();
        }
        if model.component_extension.is_some() && !model.scientific_effects.is_pending() {
            model.component_extension = None;
            if scientific_accepted {
                model.component_form = Default::default();
                model.component_dependencies.notice = "Component, choices and integrator history accepted together; fields unchanged.".into();
            } else {
                model.component_form.clear_history_consent();
                model.component_dependencies.notice =
                    model.document.notice.clone().unwrap_or_else(|| {
                        "Captured addition was refused; confirm and retry explicitly.".into()
                    });
            }
        }
        crate::component_form::reset::after_poll(model, scientific_accepted);
        model.catalog.poll(
            &mut model.document,
            &mut model.scientific_effects,
            model.inventory_revision,
        );
        model.workload_preparation.poll(&model.document);
        let component_context = model.document.authoring_guard();
        model.component_dependencies.poll(
            &mut model.document,
            &mut model.component_form,
            model.inventory_revision,
        );
        if let Some(intent) = model.component_dependencies.take_scientific_intent() {
            if intent.reset {
                crate::component_form::reset::stage(model, intent);
            } else if model.component_form.proposal_id() != intent.generation
                || !model.component_form.history_confirmed()
                || model.inventory_revision != Some(intent.revision)
                || !model.document.accepts_effect(intent.guard)
            {
                model.component_dependencies.notice =
                    "Captured addition context changed; check and confirm again.".into();
            } else {
                match model.scientific_effects.extend_components(
                    &model.document,
                    intent.guard,
                    intent.attachments,
                    intent.lock,
                ) {
                    Ok(()) => model.component_extension = Some(intent.generation),
                    Err(error) => {
                        model.component_form.clear_history_consent();
                        model.component_dependencies.notice = error.to_string();
                    }
                }
            }
        }
        model.queue_len = usize::from(model.scientific_effects.is_pending());
        if model.document.authoring_guard() != component_context {
            // Successful component admission ends the local proposal. In
            // particular, replacement buttons must not keep targeting a removed
            // component after the new revision is already being displayed.
            model.component_form = Default::default();
        }
        model.physics_dependencies.poll(
            &mut model.document,
            &mut model.physics_form,
            model.inventory_revision,
        );
        crate::component_form::reset::before_poll(model);
        // Component-lock adoption can add dependency-only references. Start the
        // reconciliation in this same fold, before its polling subscription is
        // decided, rather than waiting for an unrelated future user event.
        model.plugin_references.drive(&mut model.document);
    }

    // An asynchronous read must not pull the user out of a newly edited/opened
    // draft or a different observation session after the original consent.
    if model
        .run_attach_guard
        .is_some_and(|guard| !model.document.accepts_effect(guard))
    {
        model.run.detach();
        model.run_attach_guard = None;
    }
    if let Some(label) = model.run.poll() {
        if model.is_authoring() {
            change_mode(
                model,
                WorkspaceIntent::Observe(kagami_session::RunAttachment::remote(label)),
            );
        } else {
            model.run.detach();
        }
    }
    if !model.run.is_pending() {
        model.run_attach_guard = None;
    }
    model.run.synchronize_scale(model.current_view().scale());

    // Keep the projection MCP reads honest with whatever just changed. Reading
    // the live document here means `kagami_status` always reports the revision,
    // dirty state, and mode the window last drew — never a stale or duplicate
    // model.
    if let McpState::Running(running) = &model.mcp {
        running.refresh(&model.document);
    }

    task
}

/// Enable, disable, or observe the embedded MCP server.
///
/// None of these touch the experiment, a run, or the cluster connection (ADR
/// 0006): disabling is not a run control, and a running simulation keeps
/// running. A disable that would cut off connected clients asks to confirm
/// first, naming how many lose access in the view.
fn mcp_control(model: &mut Model, control: McpControl) -> iced::Task<Message> {
    match control {
        McpControl::Enable => {
            model.open_menu = None;
            model.mcp = mcp::enable(
                SessionState::from_document(&model.document),
                mcp::DEFAULT_ADDR,
            );
        }
        McpControl::Disable => {
            if let McpState::Running(running) = &mut model.mcp {
                let connected = running.connection_count().unwrap_or(0);
                if connected > 0 && !running.awaiting_disable_confirm() {
                    // Name the consequence and wait for an explicit confirm.
                    running.request_disable_confirm();
                } else {
                    running.disable();
                    model.mcp = McpState::Disabled;
                }
            }
        }
        McpControl::ConfirmDisable => {
            if let McpState::Running(running) = &model.mcp {
                running.disable();
            }
            model.mcp = McpState::Disabled;
        }
        McpControl::CancelDisable => {
            if let McpState::Running(running) = &mut model.mcp {
                running.cancel_disable_confirm();
            }
        }
        McpControl::CopyToken => {
            if let McpState::Running(running) = &model.mcp {
                return iced::clipboard::write(running.token().to_owned());
            }
        }
        McpControl::Poll => {
            if let McpState::Running(running) = &mut model.mcp
                && let Err(reason) = running.poll()
            {
                // The server died after binding; show the failure honestly.
                model.mcp = McpState::Failed(reason);
            }
        }
    }

    iced::Task::none()
}

/// Enter or leave Observation/replay.
///
/// The decision is `kagami_session::Workspace`'s; what happens here is the
/// presentation side of it — copying the authoring camera in on entry,
/// discarding the observer's camera on the way out — plus carrying out the
/// consequence the machine returned.
fn change_mode(model: &mut Model, intent: WorkspaceIntent) {
    match intent {
        WorkspaceIntent::Observe(attachment) => match model.document.observe(attachment) {
            Ok(()) => {
                // ADR 0022: entering copies the current authoring view as the
                // initial camera. Every change from here is made to the copy.
                model.observing_view = Some(model.document.authoring_view());
            }
            Err(rejection) => model.document.notice = Some(rejection.to_string()),
        },
        WorkspaceIntent::EditInitialConditions => {
            match model.document.edit_initial_conditions() {
                Ok(consequence) => {
                    // The observer's camera is dropped rather than kept: it was
                    // never the authored view, and nothing inherits it.
                    model.observing_view = None;
                    if matches!(consequence, LeaveConsequence::Detach { .. }) {
                        model.run.detach();
                    }
                    execute(consequence);
                }
                Err(rejection) => model.document.notice = Some(rejection.to_string()),
            }
        }
    }
}

/// Carry out what leaving Observation/replay costs.
///
/// Remote projection detachment has already happened in `change_mode`; it sends
/// no worker command. The local preview adapter is still K-PREVIEW work, so its
/// unhandled stop is logged explicitly.
fn execute(consequence: LeaveConsequence) {
    match consequence {
        LeaveConsequence::StopPreview { run } => {
            log::info!("would stop local preview `{run}` (K-PREVIEW owns the run adapter)");
        }
        LeaveConsequence::Detach { run } => {
            log::debug!("detached from run `{run}`; no worker command sent");
        }
    }
}

/// Send one authoritative intent to the document.
///
/// Nothing here decides whether the change happened: it asks, and the
/// authority answers. What the window does afterwards is limited to tidying
/// presentation state that no longer names anything.
fn submit(model: &mut Model, intent: Authoritative) {
    let accepted = match intent {
        Authoritative::CreateObject => {
            let name = model.next_object_name();
            match DisplayName::new(name) {
                Ok(name) => edit_scene(
                    model,
                    vec![ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
                        name,
                    )))],
                ),
                Err(error) => {
                    model.document.notice = Some(error.to_string());
                    false
                }
            }
        }
        Authoritative::RemoveObject(object) => {
            remove_scene(model, removal::Removal::Object(object))
        }
        Authoritative::RenameObject(object, name) => match DisplayName::new(name) {
            Ok(name) => edit_scene(
                model,
                vec![ExperimentCommand::RenameObject { object, name }],
            ),
            Err(error) => {
                model.document.notice = Some(error.to_string());
                false
            }
        },
        Authoritative::AttachComponent(object, component) => {
            // Every exact Add offers per-object inputs and complete provider intent.
            // Captured additions additionally require history-regeneration consent.
            #[cfg(unix)]
            if component.contribution().is_some() {
                if model.scientific_effects.is_pending()
                    || model.physics_dependencies.is_pending()
                    || model.component_dependencies.is_pending()
                {
                    model.document.notice = Some("Finish the pending authoring operation before proposing another attachment.".into());
                    return;
                }
                match crate::component_form::ComponentForm::for_attachment(
                    &model.document,
                    object,
                    component,
                ) {
                    Ok(form) => {
                        model.component_form = form;
                        model.component_dependencies.notice = "Component attachment proposed. Check dependencies and save to accept it with the complete lock.".into();
                        model.selected = Some(object);
                        model.document.notice = None;
                    }
                    Err(error) => model.document.notice = Some(error.into()),
                }
                return;
            }
            // Clicking Add authors the selected plugin's declared defaults.
            // No quantity is invented when a declaration has no default; an
            // incomplete proposal is still refused by the same authority.
            let properties = model
                .document
                .schemas()
                .get(&component)
                .map(crate::plugins::component_defaults)
                .unwrap_or_default();
            edit_scene(
                model,
                vec![ExperimentCommand::AttachComponent {
                    object,
                    component,
                    properties,
                }],
            )
        }
        Authoritative::DetachComponent(object, component) => {
            remove_scene(model, removal::Removal::Component(object, component))
        }
        Authoritative::SetProperty {
            object,
            component,
            property,
            source,
        } => {
            let accepted = edit_scene(
                model,
                vec![ExperimentCommand::SetComponentProperty {
                    object,
                    component,
                    property,
                    // What the user typed, as an expression. The magnitude is
                    // the model's to derive (ADR 0005).
                    value: kagami_document::AuthoredValue::si(source),
                }],
            );
            if accepted {
                model.editing = None;
            }
            accepted
        }
        Authoritative::Undo => model.document.submit(SessionCommand::Undo),
        Authoritative::Redo => model.document.submit(SessionCommand::Redo),
        Authoritative::New { discard_unsaved } => {
            let accepted = model.document.new_experiment(discard_unsaved);
            if accepted {
                model.next_object_number = 1;
            }
            accepted
        }
        Authoritative::Open {
            path,
            discard_unsaved,
        } => model.document.open(path, discard_unsaved),
        Authoritative::Save { path } => model.document.save(path, now()),
    };

    if accepted {
        model.forget_missing();
    }
}

/// Ordinary edits stay synchronous and need no executable installation. Only a
/// specific history-coherence refusal starts the guarded scientific lane; other
/// validation errors still go through normal authority diagnostics/receipts.
fn edit_scene(model: &mut Model, commands: Vec<ExperimentCommand>) -> bool {
    #[cfg(unix)]
    if model.is_authoring()
        && model.document.snapshot().setup().scientific().is_some()
        && matches!(
            kagami_document::update(
                model.document.experiment(),
                &commands,
                model.document.schemas(),
                model.document.limits(),
            ),
            Err(kagami_document::Rejection::Scientific(
                kagami_document::scientific::ScientificError::History
            ))
        )
    {
        model.document.notice = Some(match model.scientific_effects.edit_scene(&model.document, commands) {
            Ok(()) => "Preparing object edits and initial integrator history together; field states are unchanged. The old revision remains active until acceptance.".into(),
            Err(error) => error.to_string(),
        });
        return false;
    }
    model.document.edit(commands)
}

fn remove_scene(model: &mut Model, removal: removal::Removal) -> bool {
    match removal.commands(&model.document) {
        Ok(commands) => edit_scene(model, commands),
        Err(error) => {
            model.document.notice = Some(error.to_string());
            false
        }
    }
}

/// Apply one view change. None of this reaches the authority.
fn apply_local(model: &mut Model, local: ClientLocal) {
    match local {
        ClientLocal::Select(object) => model.selected = object,
        ClientLocal::ToggleExpanded(object) => {
            if !model.expanded.remove(&object) {
                model.expanded.insert(object);
            }
        }
        ClientLocal::ToggleHidden(object) => {
            if !model.hidden.remove(&object) {
                model.hidden.insert(object);
            }
        }
        ClientLocal::Search(query) => model.search_query = query,
        ClientLocal::SelectTool(tool) => model.active_tool = tool,
        ClientLocal::ToggleMenu(menu) => {
            model.open_menu = if model.open_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
        }
        ClientLocal::CloseMenu => model.open_menu = None,
        ClientLocal::OpenSettings => {
            model.settings_open = true;
            model.open_menu = None;
        }
        ClientLocal::CloseSettings => model.settings_open = false,
        ClientLocal::EditProperty {
            object,
            component,
            property,
            source,
        } => {
            model.editing = Some(PropertyEdit {
                object,
                component,
                property,
                source,
            });
        }
        ClientLocal::CancelPropertyEdit => model.editing = None,
        ClientLocal::DismissNotice => model.document.notice = None,

        // The view changes whose destination depends on the mode. In Authoring
        // they advance the separate view revision and dirty the file; while
        // observing they change a copy that is thrown away (ADR 0022). None of
        // them ever produces an envelope.
        ClientLocal::MoveCamera(motion) => {
            observing_or_authoring(
                model,
                |view| view.moved(motion).map(|view| (view, ViewAdjustment::None)),
                |document| document.move_camera(motion),
            );
        }
        ClientLocal::SetProjection(projection) => {
            observing_or_authoring(
                model,
                |view| Ok((view.with_projection(projection), ViewAdjustment::None)),
                |document| document.set_projection(projection),
            );
        }
        ClientLocal::SetScale(scale) => {
            observing_or_authoring(
                model,
                |view| view.with_scale(scale),
                |document| document.set_scale(scale),
            );
        }
        ClientLocal::EditScale(source) => model.scale_entry = Some(source),
        ClientLocal::CancelScaleEdit => model.scale_entry = None,
        ClientLocal::SubmitScale(source) => match SceneScale::parse(&source) {
            Ok(scale) => {
                let accepted = observing_or_authoring(
                    model,
                    |view| view.with_scale(scale),
                    |document| document.set_scale(scale),
                );
                // The entry closes only if the scale was taken. A refusal
                // leaves it as typed so it can be corrected rather than
                // retyped — and "accepted" is the right question, not "left a
                // notice", because an accepted change that adjusted the camera
                // leaves one too.
                if accepted {
                    model.scale_entry = None;
                }
            }
            // A scale that does not name a length is reported and the field is
            // left as typed, so the user can correct it rather than retype it.
            Err(error) => model.document.notice = Some(error.to_string()),
        },
    }
}

/// Route one view change to the ephemeral observing copy or to the document.
///
/// One function so the mode decision is made once. `observing` returns a
/// candidate rather than mutating, because an [`AuthoringView`] is validated
/// as a whole — there is no field to assign that would leave it consistent.
///
/// It also returns whatever the change had to adjust, and that is reported the
/// same way in both modes: a camera moved twelve orders of magnitude by a
/// scale change is just as surprising while watching a run as while authoring
/// one, even though only the authoring case touches the file.
/// Returns whether the change was accepted — which is not the same as whether
/// it left a notice, since an accepted change that had to adjust the camera
/// leaves one too.
fn observing_or_authoring(
    model: &mut Model,
    observing: impl FnOnce(AuthoringView) -> Result<(AuthoringView, ViewAdjustment), ViewError>,
    authoring: impl FnOnce(&mut crate::document::Document) -> bool,
) -> bool {
    match model.observing_view {
        Some(view) => match observing(view) {
            Ok((changed, adjustment)) => {
                model.observing_view = Some(changed);
                model.document.notice = adjustment.message();
                true
            }
            Err(error) => {
                model.document.notice = Some(error.to_string());
                false
            }
        },
        None => authoring(&mut model.document),
    }
}

/// The timestamp a save stamps on the document.
///
/// Read here rather than inside the codec, so encoding stays a pure function
/// of its inputs and a round trip is testable as a value.
fn now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    // Seconds since the epoch, spelled so it is obviously not a wall-clock
    // format anyone should parse. A real timestamp arrives with the first
    // dependency that already formats one.
    format!("{seconds}")
}
