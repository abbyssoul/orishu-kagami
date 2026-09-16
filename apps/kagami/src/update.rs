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

pub fn update(model: &mut Model, message: Message) -> iced::Task<Message> {
    let task = match message {
        #[cfg(unix)]
        Message::PhysicsForm(action) => {
            use crate::physics_form::PhysicsAction;
            if action == PhysicsAction::LoadCaptured {
                let result = model
                    .document
                    .snapshot()
                    .setup()
                    .scientific()
                    .ok_or("No captured scientific setup to copy.")
                    .and_then(|setup| {
                        crate::physics_form::PhysicsForm::from_captured(
                            setup,
                            &model.kernel_choices,
                            model
                                .inventory_revision
                                .ok_or("No plugin inventory is configured.")?,
                        )
                    });
                match result {
                    Ok(form) => {
                        model.physics_form = form;
                        model.document.notice = Some("Captured settings copied into the form. The experiment is unchanged; Apply will reset initial states only after confirmation.".into());
                    }
                    Err(error) => model.document.notice = Some(error.into()),
                }
            } else if action == PhysicsAction::Apply {
                let request = model.inventory_revision.ok_or("No plugin inventory is configured.")
                    .and_then(|revision| model.physics_form.request(&model.kernel_choices, revision))
                    .and_then(|mut request| {
                        let mut roots: std::collections::BTreeSet<_> = request.selection.roots.into_iter().collect();
                        for object in model.document.snapshot().objects().values() {
                            for kind in object.components.keys() {
                                let contribution = kind.contribution().ok_or("Legacy components require explicit migration before configuring plugin physics.")?;
                                if !roots.contains(contribution) {
                                    if roots.len() == orishu_plugin::selected::SelectionLimits::default().contributions { return Err("Scene contribution roots exceed the selection budget."); }
                                    roots.insert(contribution.clone());
                                }
                            }
                        }
                        request.selection.roots = roots.into_iter().collect();
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
        if model.scientific_effects.poll(&mut model.document) {
            model.forget_missing();
        }
        model.queue_len = usize::from(model.scientific_effects.is_pending());
    }

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
                    execute(consequence);
                }
                Err(rejection) => model.document.notice = Some(rejection.to_string()),
            }
        }
    }
}

/// Carry out what leaving Observation/replay costs.
///
/// There is nothing to carry out yet: stopping a local preview is K-PREVIEW's
/// and detaching from a cluster run is K-RUN's, and neither adapter exists. It
/// is logged rather than dropped so the consequence is visibly unhandled
/// instead of quietly assumed — and the shape the run adapters have to satisfy
/// is already here.
fn execute(consequence: LeaveConsequence) {
    match consequence {
        LeaveConsequence::StopPreview { run } => {
            log::info!("would stop local preview `{run}` (K-PREVIEW owns the run adapter)");
        }
        LeaveConsequence::Detach { run } => {
            log::info!("would detach from run `{run}`, which keeps executing (K-RUN owns it)");
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
                Ok(name) => model
                    .document
                    .edit(vec![ExperimentCommand::CreateObject(Box::new(
                        ObjectSpec::new(name),
                    ))]),
                Err(error) => {
                    model.document.notice = Some(error.to_string());
                    false
                }
            }
        }
        Authoritative::RemoveObject(object) => model
            .document
            .edit(vec![ExperimentCommand::RemoveObject(object)]),
        Authoritative::RenameObject(object, name) => match DisplayName::new(name) {
            Ok(name) => model
                .document
                .edit(vec![ExperimentCommand::RenameObject { object, name }]),
            Err(error) => {
                model.document.notice = Some(error.to_string());
                false
            }
        },
        Authoritative::AttachComponent(object, component) => {
            // Clicking Add authors the selected plugin's declared defaults.
            // No quantity is invented when a declaration has no default; an
            // incomplete proposal is still refused by the same authority.
            let properties = model
                .document
                .schemas()
                .get(&component)
                .map(crate::plugins::component_defaults)
                .unwrap_or_default();
            model
                .document
                .edit(vec![ExperimentCommand::AttachComponent {
                    object,
                    component,
                    properties,
                }])
        }
        Authoritative::DetachComponent(object, component) => {
            model
                .document
                .edit(vec![ExperimentCommand::DetachComponent {
                    object,
                    component,
                }])
        }
        Authoritative::SetProperty {
            object,
            component,
            property,
            source,
        } => {
            let accepted = model
                .document
                .edit(vec![ExperimentCommand::SetComponentProperty {
                    object,
                    component,
                    property,
                    // What the user typed, as an expression. The magnitude is
                    // the model's to derive (ADR 0005).
                    value: kagami_document::AuthoredValue::si(source),
                }]);
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
