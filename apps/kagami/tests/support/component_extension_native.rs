//! Native captured-Add proposal, consent and asynchronous handoff boundaries.
use super::*;
use kagami::{
    component_form::{Action as C, properties::Input},
    message::Authoritative,
    physics_form::dependencies::Action as D,
};

fn send(model: &mut Model, action: C) {
    let _ = update(model, Message::ComponentForm(action));
}
pub(super) fn confirm(model: &mut Model, confirmed: bool) {
    let proposal = model.component_form.proposal_id();
    send(
        model,
        C::ConfirmHistory {
            proposal,
            confirmed,
        },
    );
}
fn drain_dependencies(model: &mut Model) {
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while model.component_dependencies.is_pending() {
        send(model, C::Dependencies(D::Poll));
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub(super) fn dependency(model: &mut Model, action: D) {
    send(model, C::Dependencies(action));
    drain_dependencies(model);
}
fn check(model: &mut Model) -> uuid::Uuid {
    dependency(model, D::Check);
    model
        .component_dependencies
        .report()
        .expect("dependency report")
        .token()
}
fn load_fields(model: &mut Model) {
    let report = check(model);
    dependency(model, D::LoadProperties { report });
    assert!(
        model.component_form.properties().is_some(),
        "{}",
        model.component_dependencies.notice
    );
}
fn edit_visible(model: &mut Model, value: bool) {
    let form = model.component_form.properties().unwrap().id();
    send(
        model,
        C::SetProperty {
            form,
            property: "visible".try_into().unwrap(),
            value: Input::Boolean(value),
        },
    );
}
fn propose(
    model: &mut Model,
    object: kagami_document::ObjectId,
    tag: &kagami_catalog::ComponentTypeId,
) {
    let _ = update(
        model,
        Authoritative::AttachComponent(object, tag.clone()).into(),
    );
    assert!(
        model.component_form.current(&model.document),
        "{:?}",
        model.document.notice
    );
    assert!(model.component_form.captured_addition());
}

#[test]
fn native_captured_add_requires_current_consent_and_cancellation_crosses_the_handoff() {
    let Fixture {
        installed,
        plugins,
        mut model,
        tag,
        ..
    } = fixture();
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let original = model.document.snapshot().clone();
    propose(&mut model, object, &tag);
    load_fields(&mut model);
    assert!(model.document.schemas().get(&tag).is_none());
    assert_eq!(model.document.snapshot(), &original);

    let expired = model.component_form.proposal_id();
    confirm(&mut model, true);
    edit_visible(&mut model, true);
    assert!(
        !model.component_form.history_confirmed(),
        "editing invalidates consent"
    );
    send(
        &mut model,
        C::ConfirmHistory {
            proposal: expired,
            confirmed: true,
        },
    );
    assert!(
        !model.component_form.history_confirmed(),
        "an old checkbox cannot consent to new values"
    );
    let report = check(&mut model);
    dependency(&mut model, D::ApplyLock { report });
    assert!(!model.scientific_effects.is_pending());
    assert!(model.component_dependencies.notice.contains("Confirm"));
    assert_eq!(model.document.snapshot(), &original);

    // Withdraw consent after the checked lock hands off to real initialization.
    begin_native_addition(&mut model);
    assert_eq!(model.document.snapshot(), &original);
    assert!(model.document.schemas().get(&tag).is_none());
    confirm(&mut model, false);
    poll(&mut model);
    assert_eq!(model.document.snapshot(), &original);
    assert!(model.document.schemas().get(&tag).is_none());
    assert!(!model.component_form.history_confirmed());

    // Cancel while a dependency read is outstanding, without ever starting JIT.
    let report = check(&mut model);
    confirm(&mut model, true);
    model.component_dependencies.act(
        D::ApplyLock { report },
        &model.document,
        &mut model.component_form,
        &[],
        model.inventory_revision,
    );
    send(&mut model, C::Cancel);
    drain_dependencies(&mut model);
    assert!(!model.scientific_effects.is_pending());
    assert_eq!(model.document.snapshot(), &original);
    assert!(model.document.schemas().get(&tag).is_none());

    // Fresh explicit proposal is accepted as one scientific/document revision.
    propose(&mut model, object, &tag);
    load_fields(&mut model);
    edit_visible(&mut model, true);
    begin_native_addition(&mut model);
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let accepted = model.document.snapshot().clone();
    assert_eq!(accepted.revision().get(), original.revision().get() + 1);
    assert!(model.document.schemas().get(&tag).is_some());
    assert_eq!(
        accepted.object(object).unwrap().components[&tag].properties
            [&"visible".try_into().unwrap()]
            .authored(),
        kagami_document::AuthoredValue::Boolean(true)
    );
    assert!(
        model.component_form.attachment().is_none(),
        "successful handoff ends the proposal"
    );
    for (id, field) in original.setup().scientific().unwrap().captures() {
        if field.context.execution_contract == ExecutionContractId::Field {
            assert_eq!(
                accepted.setup().scientific().unwrap().captures()[id],
                *field
            );
            assert!(Arc::ptr_eq(
                &accepted.setup().scientific().unwrap().captures()[id].state,
                &field.state
            ));
        }
    }
    assert!(model.document.submit(kagami_session::SessionCommand::Undo));
    assert_eq!(model.document.snapshot().objects(), original.objects());
    assert_eq!(
        model.document.snapshot().dependencies(),
        original.dependencies()
    );
    assert_eq!(model.document.snapshot().setup(), original.setup());
    assert!(model.document.submit(kagami_session::SessionCommand::Redo));
    assert_eq!(model.document.snapshot().setup(), accepted.setup());
    let path = installed._dir.path().join("native-captured-add.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    assert_eq!(offline.document.snapshot().setup(), accepted.setup());
    assert_eq!(
        offline.document.snapshot().dependencies(),
        accepted.dependencies()
    );
    kagami::export::compile_snapshot(
        offline.document.snapshot(),
        &plugins,
        "native-add".parse().unwrap(),
        Default::default(),
        Default::default(),
        offline.document.limits(),
    )
    .unwrap();

    // Reusing the now-locked root still offers distinct values and fresh consent.
    let second = *model
        .document
        .snapshot()
        .objects()
        .keys()
        .next_back()
        .unwrap();
    let existing_lock = model.document.snapshot().dependencies().unwrap().clone();
    propose(&mut model, second, &tag);
    assert!(!model.component_form.history_confirmed());
    assert!(
        !model
            .document
            .snapshot()
            .object(second)
            .unwrap()
            .components
            .contains_key(&tag)
    );
    edit_visible(&mut model, false);
    begin_native_addition(&mut model);
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    assert!(Arc::ptr_eq(
        model.document.snapshot().dependencies().unwrap(),
        &existing_lock
    ));
    assert_eq!(
        model.document.snapshot().object(second).unwrap().components[&tag].properties
            [&"visible".try_into().unwrap()]
            .authored(),
        kagami_document::AuthoredValue::Boolean(false)
    );
    assert_eq!(
        model.document.snapshot().object(object).unwrap().components[&tag].properties
            [&"visible".try_into().unwrap()]
            .authored(),
        kagami_document::AuthoredValue::Boolean(true)
    );
}

#[test]
fn native_captured_add_discards_document_edits_on_both_sides_of_the_handoff() {
    let Fixture {
        installed: _installed,
        plugins: _plugins,
        mut model,
        tag,
        ..
    } = fixture();
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    for after_handoff in [false, true] {
        propose(&mut model, object, &tag);
        load_fields(&mut model);
        confirm(&mut model, true);
        let report = check(&mut model);
        model.component_dependencies.act(
            D::ApplyLock { report },
            &model.document,
            &mut model.component_form,
            &[],
            model.inventory_revision,
        );
        if after_handoff {
            drain_dependencies(&mut model);
            assert!(model.scientific_effects.is_pending());
        }
        assert!(model.document.edit(vec![Edit::RenameObject {
            object,
            name: DisplayName::new("keep concurrent edit").unwrap()
        }]));
        let before = model.document.snapshot().clone();
        drain_dependencies(&mut model);
        poll(&mut model);
        assert_eq!(model.document.snapshot(), &before);
        assert!(model.document.schemas().get(&tag).is_none());
        assert!(
            !model
                .document
                .snapshot()
                .object(object)
                .unwrap()
                .components
                .contains_key(&tag)
        );
    }
}
