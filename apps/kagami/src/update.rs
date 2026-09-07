//! Turning one message into either an envelope or a view change.
//!
//! The shape is deliberate: [`update`] dispatches on which *queue* a message
//! belongs to before it does anything else, so there is no arm in which a
//! selection change could accidentally reach the authority or an edit could
//! accidentally be applied locally.

use kagami_document::{DisplayName, ExperimentCommand, ObjectSpec};
use kagami_session::SessionCommand;

use crate::message::{Authoritative, ClientLocal, Message};
use crate::model::{Model, PropertyEdit};

pub fn update(model: &mut Model, message: Message) -> iced::Task<Message> {
    match message {
        Message::Authoritative(intent) => {
            // Any submission ends whatever menu invoked it, so the window does
            // not sit open over a changed document.
            model.open_menu = None;
            submit(model, intent);
        }
        Message::Local(local) => apply_local(model, local),
        Message::Exit => return iced::exit(),
        Message::ExitTimerTick => {
            if let Some(deadline) = model.exit_deadline
                && std::time::Instant::now() >= deadline
            {
                log::info!("self-imposed lifetime reached; exiting");
                return iced::exit();
            }
        }
    }

    iced::Task::none()
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
            // Attached with no values: a schema's required properties are
            // authored next, in the inspector. The authority refuses the
            // incomplete component, which is the honest outcome — the window
            // does not invent a default for a physical quantity.
            model
                .document
                .edit(vec![ExperimentCommand::AttachComponent {
                    object,
                    component,
                    properties: Default::default(),
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
