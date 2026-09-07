//! The window's behaviour, without a window.
//!
//! Everything K6 promises that is not a pixel is a function over values, so it
//! is tested here: which queue a message belongs to, that an edit reaches the
//! authority as an envelope, that a refusal leaves the view showing the last
//! accepted revision, and that the document lifecycle works end to end through
//! a real file.
//!
//! The windowed check stays manual (`make run-kagami`), because what it adds
//! is whether the thing looks right.

use std::collections::BTreeMap;

use kagami::message::{Authoritative, ClientLocal, Message};
use kagami::model::{Menu, Model, Tool};
use kagami::update::update;
use kagami_catalog::{ComponentName, ComponentTypeId, PluginId, PropertyName};
use kagami_document::ObjectId;

/// A model with no window and no Orishu connection.
fn model() -> Model {
    Model::new(kagami::launch::LaunchOptions {
        cluster_address: Default::default(),
        exit_after: None,
        open_path: None,
    })
}

fn send(model: &mut Model, message: impl Into<Message>) {
    let _ = update(model, message.into());
}

/// The object identity of the only object, panicking if there is not exactly
/// one.
fn only_object(model: &Model) -> ObjectId {
    let snapshot = model.document.snapshot();
    assert_eq!(snapshot.object_count(), 1, "expected exactly one object");
    *snapshot.objects().keys().next().expect("one object")
}

fn component(plugin: &str, name: &str) -> ComponentTypeId {
    ComponentTypeId::new(
        PluginId::new(plugin).expect("valid identifier"),
        ComponentName::new(name).expect("valid identifier"),
    )
}

/// The first component type the app's registry offers.
fn some_registered_component(model: &Model) -> ComponentTypeId {
    model
        .document
        .schemas()
        .schemas()
        .next()
        .expect("the app starts with a registry")
        .type_id
        .clone()
}

#[test]
fn no_client_local_message_can_produce_an_envelope() {
    // The Doom 3 split, asserted rather than described: a client produces
    // commands, and the server decides. A camera move or a selection is not a
    // command, and `Message::intent` is the only place either becomes one.
    // Any real identity will do; the point is what these messages are, not
    // which object they name.
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);

    let local = [
        ClientLocal::Select(Some(object)),
        ClientLocal::Select(None),
        ClientLocal::ToggleExpanded(object),
        ClientLocal::ToggleHidden(object),
        ClientLocal::Search("earth".to_owned()),
        ClientLocal::SelectTool(Tool::Move),
        ClientLocal::ToggleMenu(Menu::File),
        ClientLocal::CloseMenu,
        ClientLocal::OpenSettings,
        ClientLocal::CloseSettings,
        ClientLocal::EditProperty {
            object,
            component: component("kagami.dynamics", "dynamics"),
            property: PropertyName::new("inertial_mass").expect("valid"),
            source: "1".to_owned(),
        },
        ClientLocal::CancelPropertyEdit,
        ClientLocal::DismissNotice,
    ];

    for message in local {
        let message = Message::from(message);
        assert_eq!(
            message.intent(),
            None,
            "a client-local message must never become an envelope: {message:?}"
        );
    }

    // And the non-message messages are not intents either.
    assert_eq!(Message::Exit.intent(), None);
    assert_eq!(Message::ExitTimerTick.intent(), None);
}

#[test]
fn client_local_messages_change_no_revision() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);
    let revision = model.document.view().revision();

    send(&mut model, ClientLocal::Select(Some(object)));
    send(&mut model, ClientLocal::ToggleHidden(object));
    send(&mut model, ClientLocal::Search("anything".to_owned()));
    send(&mut model, ClientLocal::SelectTool(Tool::DrawFields));
    send(&mut model, ClientLocal::ToggleMenu(Menu::Edit));

    // The window changed; the experiment did not.
    assert_eq!(model.selected, Some(object));
    assert!(model.hidden.contains(&object));
    assert_eq!(model.search_query, "anything");
    assert_eq!(model.document.view().revision(), revision);
    assert!(
        model.document.is_dirty(),
        "unchanged by view state either way"
    );
}

#[test]
fn creating_an_object_goes_through_the_authority() {
    let mut model = model();
    assert_eq!(model.document.snapshot().object_count(), 0);

    send(&mut model, Authoritative::CreateObject);

    assert_eq!(model.document.snapshot().object_count(), 1);
    assert!(model.document.is_dirty());
    // One accepted edit is one undo entry, labelled with what it did.
    assert_eq!(
        model.document.view().history.undo.as_deref(),
        Some("Add object")
    );
}

#[test]
fn a_refused_edit_is_reported_and_changes_nothing() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);
    let before = model.document.view().revision();

    // An empty name is not a display name. The window must not paint this as
    // though it happened.
    send(
        &mut model,
        Authoritative::RenameObject(object, String::new()),
    );

    assert!(
        model.document.notice.is_some(),
        "a refusal must be reported, not swallowed"
    );
    assert_eq!(model.document.view().revision(), before);
    assert_eq!(
        model
            .document
            .snapshot()
            .object(object)
            .expect("still there")
            .name
            .as_str(),
        "Object 1",
        "the view still shows the last accepted revision"
    );
}

#[test]
fn attaching_a_component_offers_its_schema_and_refuses_it_incomplete() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);
    let component = some_registered_component(&model);

    // The bundled schema requires a property, so attaching it bare is refused
    // — the window does not invent a mass.
    send(
        &mut model,
        Authoritative::AttachComponent(object, component.clone()),
    );
    assert!(model.document.notice.is_some());
    assert!(
        model
            .document
            .snapshot()
            .object(object)
            .expect("exists")
            .components
            .is_empty()
    );
}

#[test]
fn a_property_is_edited_as_text_and_committed_as_an_expression() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);
    let component = component("kagami.gravity", "gravitational_source");
    let property = PropertyName::new("mass").expect("valid");

    // Attach with the property in the same batch, as the inspector does once
    // a value has been typed.
    let mut properties = BTreeMap::new();
    properties.insert(property.clone(), kagami_document::AuthoredValue::si("1 kg"));
    assert!(
        model
            .document
            .edit(vec![kagami_document::ExperimentCommand::AttachComponent {
                object,
                component: component.clone(),
                properties,
            }])
    );

    // Typing is client-local: no revision, no undo entry.
    let revision = model.document.view().revision();
    send(
        &mut model,
        ClientLocal::EditProperty {
            object,
            component: component.clone(),
            property: property.clone(),
            source: "2 kg".to_owned(),
        },
    );
    assert_eq!(model.document.view().revision(), revision);
    assert!(model.editing.is_some());

    // Committing is one edit, and the magnitude is *derived* from the text.
    send(
        &mut model,
        Authoritative::SetProperty {
            object,
            component: component.clone(),
            property: property.clone(),
            source: "2 kg".to_owned(),
        },
    );
    assert_eq!(model.editing, None, "a committed edit closes the field");
    let snapshot = model.document.snapshot();
    let value = &snapshot
        .object(object)
        .expect("exists")
        .component(&component)
        .expect("attached")
        .properties[&property];
    assert_eq!(value.source(), Some("2 kg"), "the intent is what is stored");
    assert_eq!(value.si_value(), Some(2.0));
}

#[test]
fn undo_and_redo_are_submitted_commands() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    assert_eq!(model.document.snapshot().object_count(), 1);

    send(&mut model, Authoritative::Undo);
    assert_eq!(model.document.snapshot().object_count(), 0);
    assert_eq!(
        model.document.view().history.redo.as_deref(),
        Some("Add object")
    );

    send(&mut model, Authoritative::Redo);
    assert_eq!(model.document.snapshot().object_count(), 1);
}

#[test]
fn undo_with_nothing_to_undo_is_refused_rather_than_ignored() {
    let mut model = model();
    send(&mut model, Authoritative::Undo);
    assert!(model.document.notice.is_some());
    // The affordance knows this in advance, so the button is disabled.
    assert_eq!(model.document.view().history.undo, None);
}

#[test]
fn a_new_experiment_needs_a_decision_about_unsaved_changes() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    assert!(model.document.is_dirty());

    send(
        &mut model,
        Authoritative::New {
            discard_unsaved: false,
        },
    );
    assert_eq!(
        model.document.snapshot().object_count(),
        1,
        "the authority must not discard anything nobody agreed to"
    );
    assert!(model.document.notice.is_some());

    send(
        &mut model,
        Authoritative::New {
            discard_unsaved: true,
        },
    );
    assert_eq!(model.document.snapshot().object_count(), 0);
    assert!(!model.document.is_dirty());
}

#[test]
fn selection_of_a_removed_object_is_forgotten() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);
    send(&mut model, ClientLocal::Select(Some(object)));
    send(&mut model, ClientLocal::ToggleHidden(object));

    send(&mut model, Authoritative::RemoveObject(object));

    // Presentation state naming something that no longer exists is not wrong
    // so much as meaningless; identities are never reused, so nothing could
    // silently rebind either way.
    assert_eq!(model.selected, None);
    assert!(model.hidden.is_empty());
}

#[test]
fn saving_and_reopening_round_trips_through_a_real_file() {
    let directory = std::env::temp_dir().join(format!("kagami-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a temporary directory");
    let path = directory.join("orbit.kagami");

    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let object = only_object(&model);
    send(
        &mut model,
        Authoritative::RenameObject(object, "Earth".to_owned()),
    );

    send(
        &mut model,
        Authoritative::Save {
            path: Some(path.clone()),
        },
    );
    assert_eq!(model.document.notice, None, "the save should succeed");
    assert!(!model.document.is_dirty(), "saving makes it clean");
    assert!(path.exists());
    assert!(
        model.title().starts_with("orbit.kagami"),
        "{}",
        model.title()
    );

    // A fresh window opens it and sees the same experiment.
    let mut reopened = self::model();
    send(
        &mut reopened,
        Authoritative::Open {
            path,
            discard_unsaved: false,
        },
    );
    assert_eq!(reopened.document.notice, None);
    assert_eq!(reopened.document.snapshot().object_count(), 1);
    let snapshot = reopened.document.snapshot();
    assert_eq!(
        snapshot
            .objects()
            .values()
            .next()
            .expect("one")
            .name
            .as_str(),
        "Earth"
    );
    // Opening establishes what is on disk as clean, and clears history so the
    // first undo does not empty a workspace someone just opened.
    assert!(!reopened.document.is_dirty());
    assert_eq!(reopened.document.view().history.undo, None);

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_title_says_which_document_and_whether_it_is_saved() {
    let mut model = model();
    assert_eq!(model.title(), "Untitled — Kagami");

    send(&mut model, Authoritative::CreateObject);
    assert_eq!(
        model.title(),
        "*Untitled — Kagami",
        "unsaved changes are visible in the title"
    );
}
