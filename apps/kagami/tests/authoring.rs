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

use kagami::message::{Authoritative, ClientLocal, Message, WorkspaceIntent};
use kagami::model::{Menu, Model, Tool};
use kagami::update::update;
use kagami_catalog::{ComponentName, ComponentTypeId, PluginId, PropertyName};
use kagami_document::{ObjectId, Vector3};
use kagami_session::{CameraMotion, CameraPose, Projection, RunAttachment, RunLabel, SceneScale};

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
        // A camera move and a projection choice are client-local even though
        // they are *saved*: the authority never hears about either.
        ClientLocal::MoveCamera(CameraMotion::Orbit { dx: 10.0, dy: 4.0 }),
        ClientLocal::SetProjection(Projection::Orthographic),
    ];

    for message in local {
        let message = Message::from(message);
        assert_eq!(
            message.intent(),
            None,
            "a client-local message must never become an envelope: {message:?}"
        );
    }

    // A mode change is a third thing, and also not an envelope.
    for intent in [
        WorkspaceIntent::Observe(RunAttachment::remote(run("run-1"))),
        WorkspaceIntent::EditInitialConditions,
    ] {
        let message = Message::from(intent);
        assert_eq!(
            message.intent(),
            None,
            "a mode change must never become an envelope: {message:?}"
        );
    }

    // And the non-message messages are not intents either.
    assert_eq!(Message::Exit.intent(), None);
    assert_eq!(Message::ExitTimerTick.intent(), None);
}

fn run(label: &str) -> RunLabel {
    RunLabel::new(label).expect("a constant label is valid")
}

/// Put `model` into Observation/replay, watching a cluster run.
fn observe(model: &mut Model) {
    send(
        model,
        WorkspaceIntent::Observe(RunAttachment::remote(run("cluster-1"))),
    );
    assert!(!model.is_authoring(), "the transition must have taken");
}

#[test]
fn a_camera_change_dirties_the_file_without_touching_the_experiment() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let path = save_to(&mut model, "camera");
    assert!(!model.document.is_dirty());

    let revision = model.document.view().revision();
    let undo = model.document.view().history.undo.clone();
    let view_revision = model.document.view_revision();

    send(
        &mut model,
        ClientLocal::MoveCamera(CameraMotion::Orbit { dx: 40.0, dy: 8.0 }),
    );

    // ADR 0022's whole point: the file is modified, and the science is not.
    assert!(
        model.document.is_dirty(),
        "an unsaved view modifies the file"
    );
    assert!(
        !model.document.experiment_is_dirty(),
        "the experiment itself is exactly as it was saved"
    );
    assert_eq!(model.document.view().revision(), revision);
    assert_eq!(
        model.document.view().history.undo,
        undo,
        "moving a camera is not an undo entry"
    );
    assert!(
        model.document.view_revision() > view_revision,
        "the separate view revision is what advanced"
    );
    assert!(model.title().starts_with('*'), "{}", model.title());

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn choosing_the_active_projection_again_leaves_a_saved_file_clean() {
    let mut model = model();
    let path = save_to(&mut model, "projection-noop");
    assert!(!model.document.is_dirty());
    let view_revision = model.document.view_revision();

    let active = model.current_view().projection();
    send(&mut model, ClientLocal::SetProjection(active));

    assert!(
        !model.document.is_dirty(),
        "a control clicked twice must not make a saved file look modified"
    );
    assert_eq!(model.document.view_revision(), view_revision);

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn the_projection_and_camera_survive_save_and_reopen() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    send(
        &mut model,
        ClientLocal::SetProjection(Projection::Orthographic),
    );
    send(
        &mut model,
        ClientLocal::MoveCamera(CameraMotion::Dolly { amount: 3.0 }),
    );
    send(
        &mut model,
        ClientLocal::MoveCamera(CameraMotion::Pan {
            dx: 25.0,
            dy: -10.0,
        }),
    );
    let saved = model.document.authoring_view();

    let path = save_to(&mut model, "view-round-trip");
    assert!(!model.document.is_dirty(), "saving clears both halves");

    let mut reopened = self::model();
    send(
        &mut reopened,
        Authoritative::Open {
            path: path.clone(),
            discard_unsaved: false,
        },
    );
    assert_eq!(reopened.document.notice, None);
    assert_eq!(
        reopened.document.authoring_view(),
        saved,
        "reopening returns to the view that was saved"
    );
    assert!(
        !reopened.document.is_dirty(),
        "an adopted view is what is on disk, so it is clean"
    );

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn a_new_experiment_starts_from_the_default_view_and_is_clean() {
    let mut model = model();
    send(
        &mut model,
        ClientLocal::SetProjection(Projection::Orthographic),
    );
    assert!(model.document.is_dirty());

    send(
        &mut model,
        Authoritative::New {
            discard_unsaved: true,
        },
    );

    assert_eq!(
        model.current_view().projection(),
        Projection::Perspective,
        "a new experiment opens at the default camera"
    );
    assert!(
        !model.document.is_dirty(),
        "an untitled experiment must not be born modified"
    );
}

#[test]
fn observing_refuses_authoring_and_the_authority_never_sees_it() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let revision = model.document.view().revision();
    observe(&mut model);

    for intent in [
        Authoritative::CreateObject,
        Authoritative::Undo,
        Authoritative::New {
            discard_unsaved: true,
        },
    ] {
        send(&mut model, intent);
        assert!(
            model.document.notice.is_some(),
            "the refusal must be reported, not swallowed"
        );
    }

    // Nothing reached the authority: same revision, same contents, and the
    // undo entry from before the mode change is still the one there.
    assert_eq!(model.document.view().revision(), revision);
    assert_eq!(model.document.snapshot().object_count(), 1);
    assert_eq!(
        model.document.view().history.undo.as_deref(),
        Some("Add object")
    );
    assert!(
        !model.is_authoring(),
        "a refused edit cannot change the mode"
    );
}

#[test]
fn a_camera_change_while_observing_dirties_nothing() {
    let mut model = model();
    let path = save_to(&mut model, "observing-camera");
    assert!(!model.document.is_dirty());
    let authored = model.document.authoring_view();
    let view_revision = model.document.view_revision();

    observe(&mut model);
    // Entering copied the authoring view as the initial camera (ADR 0022).
    assert_eq!(model.current_view(), authored);

    send(
        &mut model,
        ClientLocal::SetProjection(Projection::Orthographic),
    );
    send(
        &mut model,
        ClientLocal::MoveCamera(CameraMotion::Orbit { dx: 60.0, dy: 0.0 }),
    );

    // The window is looking somewhere else; the file is not modified.
    assert_ne!(model.current_view(), authored);
    assert!(
        !model.document.is_dirty(),
        "playback view changes must dirty nothing"
    );
    assert_eq!(model.document.view_revision(), view_revision);
    assert_eq!(
        model.document.authoring_view(),
        authored,
        "the authored view is untouched by an observer's camera"
    );

    // Leaving discards the observer's camera rather than adopting it.
    send(&mut model, WorkspaceIntent::EditInitialConditions);
    assert!(model.is_authoring());
    assert_eq!(model.current_view(), authored);
    assert!(!model.document.is_dirty());

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn leaving_authoring_mode_is_refused_rather_than_ignored() {
    let mut model = model();
    send(&mut model, WorkspaceIntent::EditInitialConditions);
    assert!(model.is_authoring());
    assert!(
        model.document.notice.is_some(),
        "an affordance that quietly does nothing is worse than one that says so"
    );
}

#[test]
fn the_mode_indicator_names_the_run_being_watched() {
    let mut model = model();
    assert_eq!(model.mode_label(), "Authoring");
    observe(&mut model);
    assert_eq!(
        model.mode_label(),
        "Observing cluster-1",
        "ADR 0022 requires the run identity to be unmistakable"
    );
}

/// Save `model` into a fresh temporary directory and return the path.
fn save_to(model: &mut Model, name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("kagami-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a temporary directory");
    let path = directory.join("orbit.kagami");

    send(
        model,
        Authoritative::Save {
            path: Some(path.clone()),
        },
    );
    assert_eq!(model.document.notice, None, "the save should succeed");
    path
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

#[test]
fn replacing_an_experiment_needs_consent_for_unsaved_view_changes() {
    // An unsaved view marks the file modified, so replacing the document must
    // ask about it. Checking only the experiment discarded a carefully framed
    // view without anyone agreeing to lose it.
    let mut model = model();
    send(
        &mut model,
        ClientLocal::SetProjection(Projection::Orthographic),
    );
    assert!(model.document.is_dirty());

    send(
        &mut model,
        Authoritative::New {
            discard_unsaved: false,
        },
    );
    assert_eq!(
        model.current_view().projection(),
        Projection::Orthographic,
        "the view must not be discarded without consent"
    );
    assert!(
        model.document.notice.is_some(),
        "and the refusal is reported"
    );

    // Stated explicitly, it proceeds.
    send(
        &mut model,
        Authoritative::New {
            discard_unsaved: true,
        },
    );
    assert_eq!(model.current_view().projection(), Projection::Perspective);
    assert!(!model.document.is_dirty());
}

#[test]
fn a_scale_change_dirties_the_file_without_touching_the_experiment() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    let path = save_to(&mut model, "scale-dirty");
    assert!(!model.document.is_dirty());

    let revision = model.document.view().revision();
    let undo = model.document.view().history.undo.clone();
    let view_revision = model.document.view_revision();

    send(
        &mut model,
        ClientLocal::SetScale(SceneScale::ASTRONOMICAL_UNIT),
    );

    // Same split as projection and camera: the file is modified, the science
    // is not.
    assert_eq!(model.current_view().scale(), SceneScale::ASTRONOMICAL_UNIT);
    assert!(model.document.is_dirty());
    assert!(!model.document.experiment_is_dirty());
    assert_eq!(model.document.view().revision(), revision);
    assert_eq!(
        model.document.view().history.undo,
        undo,
        "choosing a scale is not an undo entry"
    );
    assert!(model.document.view_revision() > view_revision);

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn choosing_the_active_scale_again_leaves_a_saved_file_clean() {
    let mut model = model();
    let path = save_to(&mut model, "scale-noop");
    let active = model.current_view().scale();
    let view_revision = model.document.view_revision();

    send(&mut model, ClientLocal::SetScale(active));

    assert!(!model.document.is_dirty());
    assert_eq!(model.document.view_revision(), view_revision);

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn the_scale_survives_save_and_reopen() {
    let mut model = model();
    send(&mut model, Authoritative::CreateObject);
    send(&mut model, ClientLocal::SetScale(SceneScale::NANOMETRE));
    send(
        &mut model,
        ClientLocal::MoveCamera(CameraMotion::Dolly { amount: 3.0 }),
    );
    let saved = model.document.authoring_view();
    assert_eq!(saved.scale(), SceneScale::NANOMETRE);

    let path = save_to(&mut model, "scale-round-trip");
    let mut reopened = self::model();
    send(
        &mut reopened,
        Authoritative::Open {
            path: path.clone(),
            discard_unsaved: false,
        },
    );

    assert_eq!(reopened.document.notice, None);
    assert_eq!(reopened.document.authoring_view(), saved);
    assert!(!reopened.document.is_dirty());

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn a_typed_scale_is_read_as_a_length_and_reported_when_it_is_not() {
    let mut model = model();

    // Typing is client-local: no revision until it is committed.
    let view_revision = model.document.view_revision();
    send(&mut model, ClientLocal::EditScale("1 n".to_owned()));
    assert_eq!(model.scale_entry.as_deref(), Some("1 n"));
    assert_eq!(model.document.view_revision(), view_revision);

    send(&mut model, ClientLocal::SubmitScale("1 nm".to_owned()));
    assert_eq!(model.current_view().scale(), SceneScale::NANOMETRE);
    assert_eq!(
        model.scale_entry, None,
        "a committed entry closes the field"
    );

    // A dimension other than length is a mistake, not a shorthand, and the
    // field keeps what was typed so it can be corrected.
    send(&mut model, ClientLocal::SubmitScale("1 kg".to_owned()));
    assert!(model.document.notice.is_some());
    assert_eq!(
        model.current_view().scale(),
        SceneScale::NANOMETRE,
        "a refused entry leaves the scale alone"
    );

    send(&mut model, ClientLocal::CancelScaleEdit);
    assert_eq!(model.scale_entry, None);
}

#[test]
fn a_scale_change_while_observing_dirties_nothing() {
    let mut model = model();
    let path = save_to(&mut model, "observing-scale");
    let authored = model.document.authoring_view();
    let view_revision = model.document.view_revision();

    observe(&mut model);
    send(&mut model, ClientLocal::SetScale(SceneScale::LIGHT_YEAR));

    assert_eq!(model.current_view().scale(), SceneScale::LIGHT_YEAR);
    assert!(!model.document.is_dirty(), "playback scale dirties nothing");
    assert_eq!(model.document.view_revision(), view_revision);
    assert_eq!(model.document.authoring_view(), authored);

    // And leaving discards it, like any other observer view change.
    send(&mut model, WorkspaceIntent::EditInitialConditions);
    assert_eq!(model.current_view(), authored);

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn an_automatic_distance_adjustment_is_reported_to_the_user() {
    // Regression: selecting nanometre scale took the default camera from
    // eighteen metres to two micrometres and said nothing, leaving someone
    // looking at something twelve orders of magnitude away from what they had
    // with no clue why.
    let mut model = model();
    assert!(model.document.set_scale(SceneScale::NANOMETRE));
    assert!(model.document.authoring_view().camera().distance() < 1e-5);

    let notice = model
        .document
        .notice
        .as_deref()
        .expect("an adjustment that large must be reported");
    assert!(notice.contains("Camera distance adjusted"), "{notice}");

    // A change needing no adjustment reports nothing.
    let mut model = self::model();
    let nearby = SceneScale::from_metres(2.0).expect("supported");
    assert!(model.document.set_scale(nearby));
    assert_eq!(model.document.notice, None);
}

#[test]
fn an_adjustment_is_reported_while_observing_too() {
    // Same surprise, same report: a camera moved twelve orders of magnitude is
    // no less confusing while watching a run, even though nothing is saved.
    let mut model = model();
    let path = save_to(&mut model, "observing-adjust");
    observe(&mut model);

    send(&mut model, ClientLocal::SetScale(SceneScale::NANOMETRE));

    let notice = model
        .document
        .notice
        .as_deref()
        .expect("an observer is told as well");
    assert!(notice.contains("Camera distance adjusted"), "{notice}");
    assert!(!model.document.is_dirty(), "and still nothing is saved");

    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}

#[test]
fn an_unreachable_focus_refuses_the_scale_and_keeps_the_typed_entry() {
    let mut model = model();
    // A focus a metre out at metre scale, which nanometre scale cannot reach.
    assert!(
        model.document.set_camera(
            CameraPose::new(Vector3::new(1.0, 0.0, 0.0).expect("finite"), 18.0, 0.4, 0.3,)
                .expect("names a camera")
        )
    );
    let before = model.current_view();

    // As the field is actually driven: typed, then submitted.
    send(&mut model, ClientLocal::EditScale("1 nm".to_owned()));
    send(&mut model, ClientLocal::SubmitScale("1 nm".to_owned()));

    let notice = model
        .document
        .notice
        .as_deref()
        .expect("the refusal is reported");
    assert!(notice.contains("nearer the origin"), "{notice}");
    assert_eq!(
        model.current_view(),
        before,
        "a refused scale changes nothing"
    );
    assert_eq!(
        model.scale_entry.as_deref(),
        Some("1 nm"),
        "the entry stays as typed so it can be corrected"
    );
}
