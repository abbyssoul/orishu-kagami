#![cfg(unix)]
//! Real installed declarations and the production window message path; no guest
//! execution is needed to ask questions or select exact scientific providers.
use kagami::{
    launch::LaunchOptions,
    message::Message,
    model::Model,
    physics_form::{PhysicsAction, dependencies::Action},
    plugins::{InventoryCommand, Package, PluginStore},
    scientific_effect::ScientificPlugins,
    update::update,
};
use orishu_plugin::{resolution::ResolutionOutcome, *};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "../../../plugins/reference/declarations.rs"]
mod declarations;

#[path = "support/component_attachment.rs"]
mod component_attachment;

#[path = "support/component_replacement.rs"]
mod component_replacement;

fn package(name: &str, payloads: Vec<(LocalContributionId, Payload)>, code: &[&[u8]]) -> Package {
    let (root, blobs) = declarations::release(name, payloads, code).unwrap();
    let borrowed = blobs.iter().map(|(d, b)| (*d, b.as_slice())).collect();
    let bytes = bundle::pack(&root, &borrowed, &Limits::default(), Default::default()).unwrap();
    Package::from_bundle(&bytes).unwrap()
}
fn send(model: &mut Model, action: PhysicsAction) {
    let _ = update(model, Message::PhysicsForm(action));
}
fn poll(model: &mut Model) {
    send(model, PhysicsAction::Dependencies(Action::Poll));
    let end = Instant::now() + Duration::from_secs(10);
    while model.physics_dependencies.is_pending() || model.plugin_references.is_pending() {
        assert!(
            Instant::now() < end,
            "{}",
            model.physics_dependencies.notice
        );
        std::thread::sleep(Duration::from_millis(1));
        send(model, PhysicsAction::Dependencies(Action::Poll));
    }
}
fn action(model: &mut Model, action: Action) {
    send(model, PhysicsAction::Dependencies(action));
    poll(model);
}
fn installed(providers: usize) -> (tempfile::TempDir, Arc<PluginStore>, Model) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(PluginStore::open(dir.path()).unwrap());
    let mut revision = 0;
    for i in 0..providers {
        let dynamics = declarations::vocabulary()
            .into_iter()
            .filter(|(id, _)| id.as_str() == "dynamics")
            .collect();
        let package = package(&format!("org.test.provider-{i:03}"), dynamics, &[]);
        revision = store.install(revision, &package, None, None).unwrap();
    }
    // Invalid Wasm is intentional here: discovery must never instantiate it.
    let code: &[u8] = b"not executable wasm";
    let kernel = package(
        "org.test.integrator",
        vec![(
            "euler".parse().unwrap(),
            declarations::euler(ArtifactDigest::sha256_of(code)),
        )],
        &[code],
    );
    revision = store.install(revision, &kernel, None, None).unwrap();
    let choices = store.available_models(&[]).unwrap();
    let model = Model::new(LaunchOptions {
        kernel_choices: choices.kernels,
        plugin_schemas: store.available_components(&[]).unwrap().schemas,
        scientific_plugins: Some(ScientificPlugins {
            store: store.clone(),
            revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    (dir, store, model)
}

fn component_send(model: &mut Model, action: kagami::component_form::Action) {
    let _ = update(model, Message::ComponentForm(action));
}
fn component_poll(model: &mut Model) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        component_send(
            model,
            kagami::component_form::Action::Dependencies(Action::Poll),
        );
        if !model.component_dependencies.is_pending()
            && !model.plugin_management.is_pending()
            && !model.plugin_references.is_pending()
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "{}",
            model.component_dependencies.notice
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn component_action(model: &mut Model, action: Action) {
    component_send(model, kagami::component_form::Action::Dependencies(action));
    component_poll(model);
}

fn property_input(model: &mut Model, name: &str, value: kagami::component_form::properties::Input) {
    let form = model.component_form.properties().unwrap().id();
    component_send(
        model,
        kagami::component_form::Action::SetProperty {
            form,
            property: name.try_into().unwrap(),
            value,
        },
    );
}
fn component_fixture() -> (
    tempfile::TempDir,
    Arc<PluginStore>,
    Model,
    kagami_catalog::ComponentTypeId,
) {
    component_fixture_with_lock(false)
}

fn component_fixture_with_lock(
    locked: bool,
) -> (
    tempfile::TempDir,
    Arc<PluginStore>,
    Model,
    kagami_catalog::ComponentTypeId,
) {
    use kagami::message::Authoritative;
    let (dir, store, _) = installed(1);
    let vocabulary = declarations::vocabulary();
    let dynamics = vocabulary
        .iter()
        .find(|(id, _)| id.as_str() == "dynamics")
        .unwrap()
        .1
        .clone();
    let mut mass = vocabulary
        .iter()
        .find(|(id, _)| id.as_str() == "mass")
        .unwrap()
        .1
        .clone();
    let Payload::Components(schema) = &mut mass else {
        unreachable!()
    };
    schema.scientific.requirements.push(ContractRequirement {
        slot: "dynamics".parse().unwrap(),
        contract: dynamics.contract_ref(&Limits::default()).unwrap(),
    });
    let consumer = package(
        "org.test.component-consumer",
        vec![("mass".parse().unwrap(), mass)],
        &[],
    );
    let revision = store
        .install(store.list().unwrap().revision, &consumer, None, None)
        .unwrap();
    let pin = kagami_catalog::ComponentTypeId::exact(
        consumer
            .release()
            .contribution_ref(&"mass".parse().unwrap())
            .unwrap(),
    )
    .unwrap();
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: store.available_components(&[]).unwrap().schemas,
        kernel_choices: store.available_models(&[]).unwrap().kernels,
        scientific_plugins: Some(ScientificPlugins {
            store: store.clone(),
            revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let id = *model.document.snapshot().objects().keys().next().unwrap();
    // Deliberately seed an older unlocked document for repair tests. New native
    // attachment now resolves and saves a lock from its first component.
    if locked {
        let before = model.document.snapshot().clone();
        let _ = update(
            &mut model,
            Authoritative::AttachComponent(id, pin.clone()).into(),
        );
        assert_eq!(model.document.snapshot(), &before);
        component_attachment::accept(&mut model);
        assert_eq!(
            model
                .document
                .snapshot()
                .dependencies()
                .unwrap()
                .selection()
                .bindings
                .len(),
            1
        );
    } else {
        assert!(
            model
                .document
                .edit(vec![kagami_document::ExperimentCommand::AttachComponent {
                    object: id,
                    component: pin.clone(),
                    properties: kagami::plugins::component_defaults(
                        model.document.schemas().get(&pin).unwrap()
                    ),
                }])
        );
    }
    assert!(
        model.document.snapshot().objects()[&id]
            .components
            .contains_key(&pin),
        "{:?}",
        model.document.notice
    );
    component_poll(&mut model);
    let second = package(
        "org.test.provider-001",
        vec![("dynamics".parse().unwrap(), dynamics)],
        &[],
    );
    store.install(revision, &second, None, None).unwrap();
    let _ = update(
        &mut model,
        Message::Plugins(kagami::plugins::window::Action::Refresh),
    );
    component_poll(&mut model);
    assert!(
        model.document.schemas().get(&pin).is_none(),
        "ambiguous declaration needs a provider choice"
    );
    (dir, store, model, pin)
}

#[test]
fn available_first_attachment_pins_dependencies_before_inventory_can_change() {
    let (dir, store, mut model, pin) = component_fixture_with_lock(true);
    let saved = model.document.snapshot().dependencies().unwrap().clone();
    assert_eq!(
        saved.selection().roots,
        vec![pin.contribution().unwrap().clone()]
    );
    let chosen = saved.selection().bindings[0].provider.clone();
    let provider = store
        .list()
        .unwrap()
        .releases
        .into_iter()
        .find(|r| r.release == chosen.release);
    assert!(provider.is_some());
    // A second compatible provider was installed after the actual first Add.
    // Rechecking must reuse the now-persisted choice instead of asking again.
    component_send(&mut model, kagami::component_form::Action::Load);
    component_action(&mut model, Action::Check);
    let report = model.component_dependencies.report().unwrap();
    let ResolutionOutcome::Resolved { selection, .. } = report.outcome() else {
        panic!(
            "saved provider must remain selected: {:?}",
            report.outcome()
        );
    };
    assert_eq!(selection.bindings, saved.selection().bindings);
    let token = report.token();
    component_action(&mut model, Action::ApplyLock { report: token });
    let path = dir.path().join("first-resolved.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    assert_eq!(offline.document.snapshot().dependencies(), Some(&saved));
    assert!(
        offline
            .document
            .snapshot()
            .objects()
            .values()
            .any(|o| o.components.contains_key(&pin))
    );
    assert!(!offline.document.is_dirty());
}

#[test]
fn first_unavailable_attachment_resolves_then_extends_a_lock_without_rebinding() {
    use kagami::physics_form::dependencies::Proposal;
    use kagami::{component_form::Action as C, message::Authoritative};
    let (_dir, _store, mut model, pin) = component_fixture();
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let _ = update(
        &mut model,
        Authoritative::DetachComponent(object, pin.clone()).into(),
    );
    assert!(
        model
            .unresolved_components
            .contains(pin.contribution().unwrap())
    );
    let before = model.document.snapshot().clone();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(object, pin.clone()).into(),
    );
    assert!(model.component_form.current(&model.document));
    assert_eq!(
        model.component_form.attachment(),
        Some(&(object, pin.clone()))
    );
    assert_eq!(model.document.snapshot(), &before);
    assert!(model.document.schemas().get(&pin).is_none());
    component_action(&mut model, Action::Check);
    let report = model.component_dependencies.report().unwrap();
    let token = report.token();
    let ResolutionOutcome::Unavailable { issues, .. } = report.outcome() else {
        panic!("must choose");
    };
    let chosen = issues[0].candidates[1].clone();
    let alternative = issues[0].candidates[0].clone();
    component_action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 1,
        },
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    assert_eq!(
        model.document.snapshot().revision().get(),
        before.revision().get() + 1
    );
    assert!(
        model
            .document
            .snapshot()
            .object(object)
            .unwrap()
            .components
            .contains_key(&pin)
    );
    assert!(model.document.schemas().get(&pin).is_some());
    let lock = model.document.snapshot().dependencies().unwrap().clone();
    assert_eq!(lock.selection().bindings[0].provider, chosen);
    assert!(!model.scientific_effects.is_pending());
    let _ = update(&mut model, Authoritative::Undo.into());
    assert_eq!(model.document.snapshot().objects(), before.objects());
    assert!(model.document.snapshot().dependencies().is_none());
    let _ = update(&mut model, Authoritative::Redo.into());
    assert_eq!(model.document.snapshot().dependencies(), Some(&lock));

    // Promote an already selected dependency to another attached root. The old
    // root and all its exact edges must remain intact, even with alternatives.
    let provider_pin = kagami_catalog::ComponentTypeId::exact(chosen.clone()).unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(object, provider_pin.clone()).into(),
    );
    let mut replacement = lock.selection().bindings[0].clone();
    replacement.provider = alternative.clone();
    assert!(model.component_form.bind(replacement).is_err());
    assert!(
        model
            .component_form
            .forget_binding(&lock.selection().bindings[0].requirement)
            .is_err()
    );
    component_action(&mut model, Action::Check);
    let report = model.component_dependencies.report().unwrap();
    assert!(matches!(
        report.outcome(),
        ResolutionOutcome::Resolved { .. }
    ));
    let token = report.token();
    component_action(&mut model, Action::ApplyLock { report: token });
    let expanded = model.document.snapshot().dependencies().unwrap();
    assert_eq!(expanded.selection().roots.len(), 2);
    assert_eq!(expanded.selection().bindings, lock.selection().bindings);
    assert!(
        model
            .document
            .snapshot()
            .object(object)
            .unwrap()
            .components
            .contains_key(&provider_pin)
    );

    // A genuinely new release/root can also join the locked scene without
    // changing the first object's dependency choice.
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let second = *model
        .document
        .snapshot()
        .objects()
        .keys()
        .next_back()
        .unwrap();
    let other = kagami_catalog::ComponentTypeId::exact(alternative.clone()).unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(second, other).into(),
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    let extended = model.document.snapshot().dependencies().unwrap();
    assert!(extended.selection().roots.contains(&alternative));
    assert_eq!(extended.selection().bindings, lock.selection().bindings);

    // Cancellation discards only a proposal, never changes accepted intent.
    component_send(&mut model, C::Load);
    let before = model.document.snapshot().clone();
    component_send(&mut model, C::Cancel);
    assert!(!model.component_form.current(&model.document));
    assert_eq!(model.document.snapshot(), &before);
}

#[test]
fn stale_or_cancelled_first_attachment_does_not_install_schemas_or_partial_roots() {
    use kagami::{component_form::Action as C, message::Authoritative};
    let (_dir, _store, mut model, pin) = component_fixture();
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let _ = update(
        &mut model,
        Authoritative::DetachComponent(object, pin.clone()).into(),
    );
    for (cancel, preview) in [(false, false), (true, false), (false, true), (true, true)] {
        let _ = update(
            &mut model,
            Authoritative::AttachComponent(object, pin.clone()).into(),
        );
        component_action(&mut model, Action::Check);
        let token = model.component_dependencies.report().unwrap().token();
        component_action(
            &mut model,
            Action::Choose {
                report: token,
                issue: 0,
                candidate: 1,
            },
        );
        component_action(&mut model, Action::Check);
        let token = model.component_dependencies.report().unwrap().token();
        model.component_dependencies.act(
            if preview {
                Action::LoadProperties { report: token }
            } else {
                Action::ApplyLock { report: token }
            },
            &model.document,
            &mut model.component_form,
            &[],
            model.inventory_revision,
        );
        assert!(model.component_dependencies.is_pending());
        if cancel {
            component_send(&mut model, C::Cancel);
        } else {
            assert!(
                model
                    .document
                    .edit(vec![kagami_document::ExperimentCommand::RenameObject {
                        object,
                        name: kagami_document::DisplayName::new("intervening edit").unwrap(),
                    }])
            );
        }
        let before = model.document.snapshot().clone();
        component_poll(&mut model);
        assert_eq!(model.document.snapshot(), &before);
        assert!(model.document.snapshot().dependencies().is_none());
        assert!(model.document.schemas().get(&pin).is_none());
        assert!(model.component_form.properties().is_none());
        assert!(
            !model
                .document
                .snapshot()
                .object(object)
                .unwrap()
                .components
                .contains_key(&pin)
        );
    }
}

#[test]
fn property_messages_cannot_cross_attachment_forms_and_oversized_sources_are_refused() {
    use kagami::{
        component_form::{Action as C, properties::Input},
        message::Authoritative,
    };
    let (_dir, _store, mut model) = installed(1);
    let pin = model
        .document
        .schemas()
        .schemas()
        .find(|s| s.type_id.contribution().is_some())
        .unwrap()
        .type_id
        .clone();
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(object, pin.clone()).into(),
    );
    let old = model.component_form.properties().unwrap().id();
    component_send(&mut model, C::Cancel);
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(object, pin).into(),
    );
    let fields = model.component_form.properties().unwrap();
    assert_ne!(fields.id(), old);
    let property = fields.schema().properties.keys().next().unwrap().clone();
    let before = fields.values().clone();
    component_send(
        &mut model,
        C::SetProperty {
            form: old,
            property: property.clone(),
            value: Input::Quantity("3 kg".into()),
        },
    );
    assert_eq!(model.component_form.properties().unwrap().values(), &before);
    assert!(model.component_dependencies.notice.contains("stale"));
    let form = model.component_form.properties().unwrap().id();
    let oversized = "x".repeat(model.document.limits().max_expression_bytes + 1);
    component_send(
        &mut model,
        C::SetProperty {
            form,
            property,
            value: Input::Quantity(oversized),
        },
    );
    assert_eq!(model.component_form.properties().unwrap().values(), &before);
    assert!(
        model
            .document
            .snapshot()
            .object(object)
            .unwrap()
            .components
            .is_empty()
    );
    assert!(model.document.snapshot().dependencies().is_none());
}

#[test]
fn missing_defaults_can_be_entered_without_publishing_partial_components_or_schemas() {
    use kagami::component_form::properties::Input;
    use kagami::message::Authoritative;
    let (_dir, store, _) = installed(1);
    let mut payload = declarations::vocabulary()
        .into_iter()
        .find(|(id, _)| id.as_str() == "mass")
        .unwrap()
        .1;
    let Payload::Components(declaration) = &mut payload else {
        unreachable!()
    };
    let PropertyType::Quantity {
        default_expression, ..
    } = &mut declaration.scientific.properties[0].schema
    else {
        unreachable!()
    };
    *default_expression = None;
    declaration.scientific.properties.extend([
        Property {
            id: "flag".parse().unwrap(),
            required: true,
            schema: PropertyType::Boolean { default: None },
        },
        Property {
            id: "label".parse().unwrap(),
            required: true,
            schema: PropertyType::Text {
                default: None,
                max_bytes: 4,
            },
        },
    ]);
    let missing = package(
        "org.test.missing-default",
        vec![("mass".parse().unwrap(), payload)],
        &[],
    );
    let revision = store
        .install(store.list().unwrap().revision, &missing, None, None)
        .unwrap();
    let pin = kagami_catalog::ComponentTypeId::exact(
        missing
            .release()
            .contribution_ref(&"mass".parse().unwrap())
            .unwrap(),
    )
    .unwrap();
    // Simulate an unavailable capability projection. Verified preparation, not
    // this local projection, supplies the schema for final authority validation.
    let mut model = Model::new(LaunchOptions {
        scientific_plugins: Some(ScientificPlugins {
            store,
            revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let before = model.document.snapshot().clone();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(object, pin.clone()).into(),
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert!(model.document.notice.is_some());
    assert_eq!(model.document.snapshot(), &before);
    assert!(model.document.schemas().get(&pin).is_none());
    assert!(model.document.snapshot().dependencies().is_none());
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::LoadProperties { report: token });
    assert!(model.component_form.properties().is_some());
    assert!(model.document.schemas().get(&pin).is_none());
    assert_eq!(model.document.snapshot(), &before);

    // Physical dimensionality and declared text constraints remain authority
    // decisions. Failed saves retain the local values for correction.
    for (source, label) in [("2 m", "ok"), ("2 kg", "too long")] {
        property_input(&mut model, "source", Input::Quantity(source.into()));
        property_input(&mut model, "flag", Input::Boolean(false));
        property_input(&mut model, "label", Input::Text(label.into()));
        assert_eq!(model.document.snapshot(), &before);
        component_action(&mut model, Action::Check);
        let token = model.component_dependencies.report().unwrap().token();
        component_action(&mut model, Action::ApplyLock { report: token });
        assert!(model.document.notice.is_some());
        assert_eq!(model.document.snapshot(), &before);
        assert!(model.document.schemas().get(&pin).is_none());
        assert!(
            model
                .component_form
                .properties()
                .unwrap()
                .values()
                .contains_key(&"source".try_into().unwrap())
        );
    }
    property_input(&mut model, "label", Input::Text(String::new()));
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    model.component_dependencies.act(
        Action::ApplyLock { report: token },
        &model.document,
        &mut model.component_form,
        &[],
        model.inventory_revision,
    );
    assert!(model.component_dependencies.is_pending());
    property_input(&mut model, "source", Input::Quantity("3 kg".into()));
    component_poll(&mut model);
    assert_eq!(
        model.document.snapshot(),
        &before,
        "an edited local value invalidates in-flight Apply"
    );
    assert!(model.document.schemas().get(&pin).is_none());
    property_input(&mut model, "source", Input::Quantity("1 kg + 1 kg".into()));
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert!(
        model.document.notice.is_none(),
        "{:?}; {}",
        model.document.notice,
        model.component_dependencies.notice
    );
    let accepted = model.document.snapshot().clone();
    assert_eq!(accepted.revision().get(), before.revision().get() + 1);
    let values = &accepted.object(object).unwrap().components[&pin].properties;
    assert_eq!(
        values[&"source".try_into().unwrap()].source(),
        Some("1 kg + 1 kg")
    );
    assert_eq!(values[&"source".try_into().unwrap()].si_value(), Some(2.0));
    assert_eq!(
        values[&"flag".try_into().unwrap()],
        kagami_document::PropertyValue::Boolean(false)
    );
    assert_eq!(
        values[&"label".try_into().unwrap()],
        kagami_document::PropertyValue::Text(String::new())
    );
    assert!(accepted.dependencies().is_some());
    let path = _dir.path().join("authored-properties.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    let offline_values = &offline
        .document
        .snapshot()
        .object(object)
        .unwrap()
        .components[&pin]
        .properties;
    assert_eq!(offline_values.len(), values.len());
    for (name, expected) in values {
        assert_eq!(offline_values[name].source(), expected.source());
    }
    assert!(
        !offline_values[&"source".try_into().unwrap()].is_priced(),
        "offline absence preserves authored sources, not invented schema evaluation"
    );
    assert_eq!(
        offline_values[&"flag".try_into().unwrap()],
        values[&"flag".try_into().unwrap()]
    );
    assert_eq!(
        offline_values[&"label".try_into().unwrap()],
        values[&"label".try_into().unwrap()]
    );
    assert_eq!(
        offline.document.snapshot().dependencies(),
        accepted.dependencies()
    );
    let _ = update(&mut model, Authoritative::Undo.into());
    assert_eq!(model.document.snapshot().objects(), before.objects());
    assert!(model.document.snapshot().dependencies().is_none());
    let _ = update(&mut model, Authoritative::Redo.into());
    assert_eq!(model.document.snapshot().objects(), accepted.objects());
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let second = *model
        .document
        .snapshot()
        .objects()
        .keys()
        .next_back()
        .unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(second, pin.clone()).into(),
    );
    assert!(
        model.component_form.current(&model.document),
        "an already locked root still offers per-object values"
    );
    property_input(&mut model, "source", Input::Quantity("5 kg".into()));
    property_input(&mut model, "flag", Input::Boolean(true));
    property_input(&mut model, "label", Input::Text("two".into()));
    component_attachment::accept(&mut model);
    assert_eq!(
        model.document.snapshot().dependencies(),
        accepted.dependencies()
    );
    assert_eq!(
        model.document.snapshot().object(object),
        accepted.object(object)
    );
    let second_values =
        &model.document.snapshot().object(second).unwrap().components[&pin].properties;
    assert_eq!(
        second_values[&"source".try_into().unwrap()].si_value(),
        Some(5.0)
    );
    assert_eq!(
        second_values[&"flag".try_into().unwrap()],
        kagami_document::PropertyValue::Boolean(true)
    );
}

#[test]
fn native_removals_prune_only_unused_roots_atomically_without_loading_kernels() {
    use kagami::{component_form::Action as C, message::Authoritative};
    use kagami_document::ExperimentCommand as Edit;
    let (_dir, _store, mut model, pin) = component_fixture();
    component_send(&mut model, C::Load);
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 1,
        },
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    let lock = model.document.snapshot().dependencies().unwrap().clone();
    let first = *model.document.snapshot().objects().keys().next().unwrap();
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let second = *model
        .document
        .snapshot()
        .objects()
        .keys()
        .next_back()
        .unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(second, pin.clone()).into(),
    );
    component_attachment::accept(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let before = model.document.snapshot().clone();
    let _ = update(&mut model, Authoritative::RemoveObject(first).into());
    assert_eq!(
        model.document.snapshot().revision().get(),
        before.revision().get() + 1
    );
    assert!(Arc::ptr_eq(
        model.document.snapshot().dependencies().unwrap(),
        &lock
    ));
    let _ = update(
        &mut model,
        Authoritative::DetachComponent(second, pin.clone()).into(),
    );
    let empty = model.document.snapshot().dependencies().unwrap();
    assert!(empty.selection().roots.is_empty());
    assert!(empty.selection().contributions.is_empty());
    assert!(empty.selection().bindings.is_empty());
    assert!(!model.scientific_effects.is_pending());
    let detached = model.document.snapshot().clone();
    let _ = update(
        &mut model,
        Authoritative::DetachComponent(second, pin.clone()).into(),
    );
    assert_eq!(
        model.document.snapshot(),
        &detached,
        "invalid repeat cannot change the graph"
    );
    let _ = update(&mut model, Authoritative::Undo.into());
    assert_eq!(model.document.snapshot().dependencies(), Some(&lock));
    assert!(
        model
            .document
            .snapshot()
            .object(second)
            .unwrap()
            .components
            .contains_key(&pin)
    );
    let _ = update(&mut model, Authoritative::Redo.into());
    assert_eq!(
        model.document.snapshot().dependencies(),
        detached.dependencies()
    );
    let _ = update(&mut model, Authoritative::Undo.into());

    // Seed a second explicit root using the existing typed batch interface. The
    // native addition/resolution dialog is separate work; removal must preserve
    // a shared member when it is also directly attached to the surviving object.
    let provider = lock.selection().bindings[0].provider.clone();
    let provider_pin = kagami_catalog::ComponentTypeId::exact(provider.clone()).unwrap();
    let mut roots = lock.selection().roots.clone();
    roots.push(provider.clone());
    roots.sort();
    let expanded = Arc::new(
        lock.for_roots(&roots, model.document.limits().dependencies)
            .unwrap(),
    );
    let properties =
        kagami::plugins::component_defaults(model.document.schemas().get(&provider_pin).unwrap());
    assert!(model.document.edit(vec![
        Edit::AttachComponent {
            object: second,
            component: provider_pin.clone(),
            properties
        },
        Edit::AdoptDependencies(expanded.clone()),
    ]));
    let _ = update(
        &mut model,
        Authoritative::DetachComponent(second, pin.clone()).into(),
    );
    let remaining = model.document.snapshot().dependencies().unwrap();
    assert_eq!(remaining.selection().roots, vec![provider.clone()]);
    assert_eq!(remaining.selection().contributions, vec![provider]);
    assert!(remaining.selection().bindings.is_empty());
    let _ = update(&mut model, Authoritative::Undo.into());
    assert_eq!(model.document.snapshot().dependencies(), Some(&expanded));
    let path = _dir.path().join("removal-offline.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    let mut offline = Model::new(LaunchOptions::default());
    assert!(offline.document.open(path.clone(), false));
    assert!(offline.document.schemas().get(&pin).is_none());
    let _ = update(&mut offline, Authoritative::RemoveObject(second).into());
    assert!(
        offline.document.notice.is_none(),
        "{:?}",
        offline.document.notice
    );
    assert_eq!(offline.document.snapshot().object_count(), 0);
    assert!(
        offline
            .document
            .snapshot()
            .dependencies()
            .unwrap()
            .selection()
            .contributions
            .is_empty()
    );
    let _ = update(&mut offline, Authoritative::Undo.into());
    assert_eq!(offline.document.snapshot().dependencies(), Some(&expanded));
    assert!(offline.document.snapshot().object(second).is_some());
    let mut limited = Model::new(LaunchOptions::default());
    limited.document = kagami::document::Document::new(
        kagami_catalog::SchemaRegistry::new(),
        kagami_document::Limits {
            max_commands_per_batch: 1,
            ..Default::default()
        },
    );
    assert!(limited.document.open(path, false));
    let unchanged = limited.document.snapshot().clone();
    let _ = update(&mut limited, Authoritative::RemoveObject(second).into());
    assert!(limited.document.notice.is_some());
    assert_eq!(
        limited.document.snapshot(),
        &unchanged,
        "the paired edit must fit the receiving batch budget"
    );
    let _ = update(&mut model, Authoritative::RemoveObject(second).into());
    assert_eq!(model.document.snapshot().object_count(), 0);
    assert!(
        model
            .document
            .snapshot()
            .dependencies()
            .unwrap()
            .selection()
            .contributions
            .is_empty()
    );
    component_poll(&mut model);
    let references = model
        .document
        .plugin_references(Default::default())
        .unwrap();
    assert!(!references.releases[&lock.selection().bindings[0].provider.release].current);
    let _ = update(&mut model, Authoritative::Undo.into());
    assert_eq!(model.document.snapshot().dependencies(), Some(&expanded));
    assert!(model.document.snapshot().object(second).is_some());
}

#[test]
fn component_only_choices_are_guarded_undoable_offline_and_reused_by_physics() {
    use kagami::component_form::Action as C;
    use kagami::message::Authoritative;
    let (_dir, _store, mut model, pin) = component_fixture();
    let before = model.document.snapshot().clone();
    component_send(&mut model, C::Load);
    component_action(&mut model, Action::Check);
    assert_eq!(model.document.snapshot(), &before);
    let report = model.component_dependencies.report().unwrap();
    let token = report.token();
    let ResolutionOutcome::Unavailable { issues, .. } = report.outcome() else {
        panic!("component must ask before selecting provider")
    };
    assert_eq!(issues[0].candidate_count, 2);
    let chosen = issues[0].candidates[1].clone();
    component_action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 1,
        },
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    let lock = model
        .document
        .snapshot()
        .dependencies()
        .expect(&model.component_dependencies.notice)
        .clone();
    assert_eq!(
        lock.selection().roots,
        vec![pin.contribution().unwrap().clone()]
    );
    assert_eq!(lock.selection().bindings[0].provider, chosen);
    assert!(model.document.snapshot().setup().scientific().is_none());
    assert!(!model.scientific_effects.is_pending());
    assert!(
        model.document.schemas().get(&pin).is_some(),
        "selected verified schema restores authoring capability"
    );
    assert!(model.component_dependencies.notice.contains("saved"));

    let path = _dir.path().join("component-only.kagami");
    assert!(
        model
            .document
            .save(Some(path.clone()), "2026-09-17T00:00:00Z".into()),
        "{:?}",
        model.document.notice
    );
    let document = kagami_session::decode_document(&std::fs::read(path).unwrap()).unwrap();
    let offline = document
        .into_experiment(
            &kagami_catalog::SchemaRegistry::new(),
            &kagami_document::Limits::default(),
        )
        .unwrap();
    assert_eq!(offline.snapshot().dependencies(), Some(&lock));
    let _ = update(&mut model, Authoritative::Undo.into());
    assert!(model.document.snapshot().dependencies().is_none());
    let _ = update(&mut model, Authoritative::Redo.into());
    assert_eq!(model.document.snapshot().dependencies(), Some(&lock));
    component_poll(&mut model);
    let report = model
        .document
        .plugin_references(Default::default())
        .unwrap();
    assert!(report.releases[&chosen.release].current);

    send(&mut model, PhysicsAction::Kernel(0));
    action(&mut model, Action::Check);
    let report = model.physics_dependencies.report().unwrap();
    match report.outcome() {
        ResolutionOutcome::Unavailable { issues, .. } => assert!(
            issues
                .iter()
                .all(|i| i.contribution != *pin.contribution().unwrap()),
            "saved component choices must not become ambiguous again"
        ),
        ResolutionOutcome::Resolved { selection, .. } => assert!(
            selection
                .bindings
                .iter()
                .any(|b| b == &lock.selection().bindings[0])
        ),
        _ => panic!("unexpected revision"),
    }
}

#[test]
fn component_apply_rechecks_external_inventory_and_document_changes() {
    use kagami::component_form::Action as C;
    let (_dir, store, mut model, _pin) = component_fixture();
    component_send(&mut model, C::Load);
    component_action(&mut model, Action::Check);
    let report = model.component_dependencies.report().unwrap();
    let token = report.token();
    component_action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 0,
        },
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    let before = model.document.snapshot().clone();
    store
        .submit(
            model.inventory_revision.unwrap(),
            InventoryCommand::SetEnabled {
                plugin_id: "org.test.provider-000".parse().unwrap(),
                enabled: false,
            },
        )
        .unwrap();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert_eq!(model.document.snapshot(), &before);
    assert!(model.document.snapshot().dependencies().is_none());
    assert!(
        model
            .component_dependencies
            .notice
            .contains("StaleRevision")
            || model.component_dependencies.notice.contains("Inventory"),
        "{}",
        model.component_dependencies.notice
    );
    let _ = update(
        &mut model,
        Message::Plugins(kagami::plugins::window::Action::Refresh),
    );
    component_poll(&mut model);
    component_send(&mut model, C::Load);
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    // Change the document after the report. Neither old Apply nor old form Check
    // may silently accept choices for the new revision.
    let _ = update(
        &mut model,
        kagami::message::Authoritative::CreateObject.into(),
    );
    let before = model.document.snapshot().clone();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert_eq!(model.document.snapshot(), &before);
    component_action(&mut model, Action::Check);
    assert!(model.component_dependencies.report().is_none());
}

#[test]
fn component_adoption_discards_a_completion_after_an_intervening_edit() {
    use kagami::component_form::Action as C;
    let (_dir, _store, mut model, _) = component_fixture();
    component_send(&mut model, C::Load);
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 0,
        },
    );
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    // Schedule through the production controller, but delay its completion fold
    // until after the edit. This remains deterministic even if IO finishes fast.
    model.component_dependencies.act(
        Action::ApplyLock { report: token },
        &model.document,
        &mut model.component_form,
        &[],
        model.inventory_revision,
    );
    assert!(model.component_dependencies.is_pending());
    assert!(
        model
            .document
            .edit(vec![kagami_document::ExperimentCommand::CreateObject(
                Box::new(kagami_document::ObjectSpec::new(
                    kagami_document::DisplayName::new("intervening").unwrap()
                ))
            )])
    );
    component_poll(&mut model);
    assert!(model.document.snapshot().dependencies().is_none());
    assert!(!model.component_form.current(&model.document));
}

#[test]
fn component_proposal_discards_only_reported_unused_bindings_explicitly() {
    use kagami::{component_form::Action as C, physics_form::dependencies::Proposal};
    let (_dir, _store, mut model, pin) = component_fixture();
    component_send(&mut model, C::Load);
    component_action(&mut model, Action::Check);
    let report = model.component_dependencies.report().unwrap();
    let token = report.token();
    let ResolutionOutcome::Unavailable { issues, .. } = report.outcome() else {
        unreachable!()
    };
    let provider = issues[0].candidates[0].clone();
    component_action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 0,
        },
    );
    let unused = resolution::RequirementKey {
        consumer: pin.contribution().unwrap().clone(),
        slot: "unused".parse().unwrap(),
    };
    model
        .component_form
        .bind(resolution::ProviderBinding {
            requirement: unused.clone(),
            provider,
        })
        .unwrap();
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    let required = resolution::RequirementKey {
        consumer: pin.contribution().unwrap().clone(),
        slot: "dynamics".parse().unwrap(),
    };
    component_action(
        &mut model,
        Action::ForgetUnused {
            report: token,
            requirement: required,
        },
    );
    assert_eq!(model.component_form.bindings().count(), 2);
    component_action(
        &mut model,
        Action::ForgetUnused {
            report: token,
            requirement: unused,
        },
    );
    assert_eq!(model.component_form.bindings().count(), 1);
    assert!(model.document.snapshot().dependencies().is_none());
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert_eq!(
        model
            .document
            .snapshot()
            .dependencies()
            .unwrap()
            .selection()
            .bindings
            .len(),
        1
    );
}

#[test]
fn prepared_component_lock_requires_complete_edges_bounds_and_current_enablement() {
    use kagami::plugins::{Code, PrepareAuthoringLockOutcome};
    use orishu_plugin::authoring_lock::{LockLimits, SelectionLock};
    let (_dir, store, model, pin) = component_fixture();
    let revision = store
        .submit(
            model.inventory_revision.unwrap(),
            InventoryCommand::SetEnabled {
                plugin_id: "org.test.provider-001".parse().unwrap(),
                enabled: false,
            },
        )
        .unwrap();
    let request = resolution::ResolutionRequest {
        expected_inventory_revision: revision,
        roots: vec![pin.contribution().unwrap().clone()],
        bindings: vec![],
    };
    let ResolutionOutcome::Resolved { selection, .. } = store.resolve(&request, &[]).unwrap()
    else {
        panic!("single provider resolves")
    };
    let lock = Arc::new(SelectionLock::new(selection, LockLimits::default()).unwrap());
    let provider = lock.selection().bindings[0].provider.clone();
    let PrepareAuthoringLockOutcome::Ready(prepared) = store
        .prepare_authoring_lock(lock.clone(), revision, &[], LockLimits::default())
        .unwrap()
    else {
        panic!("ready")
    };
    assert_eq!(prepared.lock(), &lock);
    assert!(prepared.schemas().get(&pin).is_some());
    assert_eq!(
        store
            .submit(
                revision,
                InventoryCommand::Remove {
                    plugin_id: "org.test.provider-000".parse().unwrap(),
                    release: provider.release,
                    ack_open_references: false
                }
            )
            .unwrap_err()
            .code,
        Code::InUse
    );
    let incomplete = Arc::new(
        SelectionLock::new(
            resolution::Selection {
                roots: request.roots.clone(),
                contributions: request.roots,
                bindings: vec![],
            },
            LockLimits::default(),
        )
        .unwrap(),
    );
    assert_eq!(
        store
            .prepare_authoring_lock(incomplete, revision, &[], LockLimits::default())
            .unwrap_err()
            .code,
        Code::InvalidSelection
    );
    assert_eq!(
        store
            .prepare_authoring_lock(
                lock.clone(),
                revision,
                &[],
                LockLimits {
                    bytes: 1,
                    ..LockLimits::default()
                }
            )
            .unwrap_err()
            .code,
        Code::LimitExceeded
    );
    let next = store
        .submit(
            revision,
            InventoryCommand::SetEnabled {
                plugin_id: "org.test.provider-000".parse().unwrap(),
                enabled: false,
            },
        )
        .unwrap();
    assert!(store.guard_revision(prepared.inventory_revision()).is_err());
    assert!(matches!(
        store
            .prepare_authoring_lock(lock, next, &[], LockLimits::default())
            .unwrap(),
        PrepareAuthoringLockOutcome::Unresolved(ResolutionOutcome::Unavailable { .. })
    ));
}

#[test]
fn exact_provider_pages_and_choices_are_bounded_read_only_and_context_guarded() {
    let (_dir, store, mut model) = installed(33);
    send(&mut model, PhysicsAction::Kernel(0));
    let before = model.document.snapshot().clone();
    action(&mut model, Action::Check);
    assert!(!model.scientific_effects.is_pending());
    assert_eq!(model.document.snapshot(), &before);
    assert!(!model.document.is_dirty());
    let report = model.physics_dependencies.report().unwrap();
    let first = report.token();
    let ResolutionOutcome::Unavailable {
        issues, truncated, ..
    } = report.outcome()
    else {
        panic!("must ask")
    };
    assert!(!truncated);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].candidate_count, 33);
    assert_eq!(issues[0].candidates.len(), 32);
    action(
        &mut model,
        Action::Page {
            report: first,
            issue: 0,
            offset: usize::MAX,
        },
    );
    assert_eq!(model.physics_dependencies.report().unwrap().token(), first);
    action(
        &mut model,
        Action::Page {
            report: first,
            issue: 0,
            offset: 32,
        },
    );
    let report = model.physics_dependencies.report().unwrap();
    let second = report.token();
    assert_ne!(first, second);
    assert_eq!(report.offset(0), 32);
    let ResolutionOutcome::Unavailable { issues, .. } = report.outcome() else {
        panic!("must ask")
    };
    assert_eq!(issues[0].candidates.len(), 1);
    let exact = issues[0].candidates[0].clone();
    assert!(
        report
            .provider_plugin(&exact)
            .unwrap()
            .starts_with("org.test.provider-")
    );
    action(
        &mut model,
        Action::Choose {
            report: first,
            issue: 0,
            candidate: 0,
        },
    );
    assert_eq!(
        model.physics_form.bindings().count(),
        0,
        "delayed first-page click cannot choose from second page"
    );
    action(
        &mut model,
        Action::Choose {
            report: second,
            issue: 0,
            candidate: 1,
        },
    );
    assert_eq!(model.physics_form.bindings().count(), 0);
    action(
        &mut model,
        Action::Choose {
            report: second,
            issue: 0,
            candidate: 0,
        },
    );
    assert_eq!(
        model.physics_form.bindings().next().unwrap().provider,
        exact
    );
    assert!(!model.physics_form.confirmed);
    assert!(model.physics_dependencies.report().is_none());
    action(&mut model, Action::Check);
    assert!(matches!(
        model.physics_dependencies.report().unwrap().outcome(),
        ResolutionOutcome::Resolved { .. }
    ));
    send(&mut model, PhysicsAction::Confirm(true));
    let request = model
        .physics_form
        .request(&model.kernel_choices, model.inventory_revision.unwrap())
        .unwrap();
    assert_eq!(request.selection.bindings[0].provider, exact);
    assert_eq!(model.document.snapshot(), &before);

    // A form edit changes the context even if the document revision is unchanged.
    send(&mut model, PhysicsAction::Dependencies(Action::Check));
    send(&mut model, PhysicsAction::Step("0.02".into()));
    poll(&mut model);
    assert!(model.physics_dependencies.report().is_none());
    assert_eq!(
        model.physics_form.bindings().count(),
        1,
        "unrelated numeric edit preserves deliberate provider pin"
    );
    action(&mut model, Action::Check);
    assert!(model.physics_dependencies.report().is_some());
    // An unseen external inventory mutation invalidates the next response.
    store
        .submit(
            model.inventory_revision.unwrap(),
            InventoryCommand::SetEnabled {
                plugin_id: "org.test.provider-000".parse().unwrap(),
                enabled: false,
            },
        )
        .unwrap();
    action(&mut model, Action::Check);
    assert!(model.physics_dependencies.report().is_none());
    assert!(
        model.physics_dependencies.notice.contains("StaleRevision"),
        "{}",
        model.physics_dependencies.notice
    );
    assert_eq!(
        model.physics_form.bindings().next().unwrap().provider,
        exact,
        "no fallback or migration"
    );
    assert_eq!(model.document.snapshot(), &before);
}

#[test]
fn document_changes_invalidate_a_pending_read_and_model_changes_clear_local_bindings() {
    use kagami_document::{DisplayName, ExperimentCommand, ObjectSpec};
    let (_dir, _store, mut model) = installed(2);
    send(&mut model, PhysicsAction::Kernel(0));
    send(&mut model, PhysicsAction::Dependencies(Action::Check));
    assert!(
        model
            .document
            .edit(vec![ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("new").unwrap())
            ))])
    );
    poll(&mut model);
    assert!(model.physics_dependencies.report().is_none());
    action(&mut model, Action::Check);
    let token = model.physics_dependencies.report().unwrap().token();
    action(
        &mut model,
        Action::Choose {
            report: token,
            issue: 0,
            candidate: 0,
        },
    );
    assert_eq!(model.physics_form.bindings().count(), 1);
    send(&mut model, PhysicsAction::Kernel(0));
    assert_eq!(model.physics_form.bindings().count(), 0);
    assert!(model.physics_dependencies.report().is_none());
}

#[test]
fn explicit_installed_browser_selects_an_old_release_without_changing_defaults() {
    for count in [1, 33] {
        let (_dir, store, mut initial) = installed(count);
        poll(&mut initial);
        drop(initial);
        let listing = store.list().unwrap();
        let old = listing
            .releases
            .iter()
            .find(|r| r.plugin_id.as_str() == "org.test.provider-000")
            .unwrap()
            .release;
        let payloads = declarations::vocabulary()
            .into_iter()
            .filter(|(id, _)| id.as_str() == "dynamics")
            .collect();
        let (mut root, blobs) =
            declarations::release("org.test.provider-000", payloads, &[]).unwrap();
        root.0.metadata.version_label = "new-default".into();
        let borrowed = blobs.iter().map(|(d, b)| (*d, b.as_slice())).collect();
        let next = Package::from_bundle(
            &bundle::pack(&root, &borrowed, &Limits::default(), Default::default()).unwrap(),
        )
        .unwrap();
        let revision = store
            .install(
                listing.revision,
                &next,
                Some(&next.release().root().0.metadata.plugin_id),
                None,
            )
            .unwrap();
        let mut model = Model::new(LaunchOptions {
            kernel_choices: store.available_models(&[]).unwrap().kernels,
            plugin_schemas: store.available_components(&[]).unwrap().schemas,
            scientific_plugins: Some(ScientificPlugins {
                store: store.clone(),
                revision,
                overrides: vec![],
            }),
            ..Default::default()
        });
        send(&mut model, PhysicsAction::Kernel(0));
        action(&mut model, Action::Check);
        let report = model.physics_dependencies.report().unwrap();
        let requirement = match report.outcome() {
            ResolutionOutcome::Resolved { selection, .. } => {
                assert_eq!(count, 1);
                assert_eq!(selection.bindings[0].provider.release, next.release().id());
                selection.bindings[0].requirement.clone()
            }
            ResolutionOutcome::Unavailable { issues, .. } => {
                assert_eq!(
                    issues[0].candidate_count, count,
                    "old release is not an automatic candidate"
                );
                assert!(!issues[0].candidates.iter().any(|p| p.release == old));
                issues[0].requirement.clone().unwrap()
            }
            _ => panic!("unexpected revision"),
        };
        let token = report.token();
        action(
            &mut model,
            Action::Browse {
                report: token,
                requirement,
            },
        );
        let report = model.physics_dependencies.report().unwrap();
        assert_eq!(report.alternatives().unwrap().total, count + 1);
        assert_eq!(model.physics_form.bindings().count(), 0);
        if count == 33 {
            let first = report.token();
            action(
                &mut model,
                Action::BrowsePage {
                    report: first,
                    offset: 32,
                },
            );
            action(
                &mut model,
                Action::ChooseInstalled {
                    report: first,
                    candidate: 0,
                },
            );
            assert_eq!(
                model.physics_form.bindings().count(),
                0,
                "old page click cannot select"
            );
            let second = model.physics_dependencies.report().unwrap().token();
            action(
                &mut model,
                Action::BrowsePage {
                    report: second,
                    offset: 0,
                },
            );
        }
        let mut report = model.physics_dependencies.report().unwrap();
        if !report
            .alternatives()
            .unwrap()
            .candidates
            .iter()
            .any(|p| p.release == old)
        {
            let token = report.token();
            action(
                &mut model,
                Action::BrowsePage {
                    report: token,
                    offset: 32,
                },
            );
            report = model.physics_dependencies.report().unwrap();
        }
        let candidate = report
            .alternatives()
            .unwrap()
            .candidates
            .iter()
            .position(|p| p.release == old)
            .unwrap();
        let token = report.token();
        action(
            &mut model,
            Action::ChooseInstalled {
                report: token,
                candidate,
            },
        );
        assert_eq!(
            model
                .physics_form
                .bindings()
                .next()
                .unwrap()
                .provider
                .release,
            old
        );
        action(&mut model, Action::Check);
        let ResolutionOutcome::Resolved { selection, .. } =
            model.physics_dependencies.report().unwrap().outcome()
        else {
            panic!("explicit binding resolves")
        };
        assert_eq!(selection.bindings[0].provider.release, old);
        let after = store.list().unwrap();
        assert_eq!(after.revision, revision);
        assert!(
            after
                .releases
                .iter()
                .find(|r| r.release == next.release().id())
                .unwrap()
                .is_default
        );
        assert!(!model.document.is_dirty());
        assert!(!model.scientific_effects.is_pending());
    }
}

#[test]
fn explicitly_chosen_dependencies_survive_capture_file_export_and_inventory_free_execution() {
    use orishu_runtime::{OperationControl, Sandbox, SandboxLimits};
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(PluginStore::open(dir.path()).unwrap());
    let a = package("org.test.vocabulary-a", declarations::vocabulary(), &[]);
    let b = package("org.test.vocabulary-b", declarations::vocabulary(), &[]);
    let chosen = b.release().id();
    let unwanted = a.release().id();
    store.install(0, &a, None, None).unwrap();
    store.install(1, &b, None, None).unwrap();
    let (mut root, blobs) =
        declarations::release("org.test.vocabulary-b", declarations::vocabulary(), &[]).unwrap();
    root.0.metadata.version_label = "new-default".into();
    let borrowed = blobs.iter().map(|(d, b)| (*d, b.as_slice())).collect();
    let newer = Package::from_bundle(
        &bundle::pack(&root, &borrowed, &Limits::default(), Default::default()).unwrap(),
    )
    .unwrap();
    store
        .install(
            2,
            &newer,
            Some(&newer.release().root().0.metadata.plugin_id),
            None,
        )
        .unwrap();
    let gravity: &[u8] =
        include_bytes!("../../../crates/orishu-runtime/tests/fixtures/newtonian.component.wasm");
    let euler: &[u8] =
        include_bytes!("../../../crates/orishu-runtime/tests/fixtures/euler.component.wasm");
    let kernels = package(
        "org.test.solvers",
        vec![
            (
                "gravity".parse().unwrap(),
                declarations::newtonian(ArtifactDigest::sha256_of(gravity)),
            ),
            (
                "euler".parse().unwrap(),
                declarations::euler(ArtifactDigest::sha256_of(euler)),
            ),
        ],
        &[gravity, euler],
    );
    let revision = store.install(3, &kernels, None, None).unwrap();
    let plugins = ScientificPlugins {
        store: store.clone(),
        revision,
        overrides: vec![],
    };
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: store.available_components(&[]).unwrap().schemas,
        kernel_choices: store.available_models(&[]).unwrap().kernels,
        scientific_plugins: Some(plugins.clone()),
        ..Default::default()
    });
    // Two composed bodies, with exact components from the deliberately selected
    // non-default vocabulary release. Author defaults through real UI commands.
    for (name, x) in [("left", -1.0), ("right", 1.0)] {
        let object =
            kagami_document::ObjectSpec::new(kagami_document::DisplayName::new(name).unwrap())
                .with_transform(kagami_document::Transform::at(
                    kagami_document::Vector3::new(x, 0.0, 0.0).unwrap(),
                ));
        assert!(
            model
                .document
                .edit(vec![kagami_document::ExperimentCommand::CreateObject(
                    Box::new(object)
                )])
        );
        let id = *model.document.snapshot().objects().keys().last().unwrap();
        for local in ["mass", "dynamics"] {
            let pin = kagami_catalog::ComponentTypeId::exact(
                b.release()
                    .contribution_ref(&local.parse().unwrap())
                    .unwrap(),
            )
            .unwrap();
            let _ = update(
                &mut model,
                kagami::message::Authoritative::AttachComponent(id, pin).into(),
            );
            if model.component_form.current(&model.document) {
                component_attachment::accept(&mut model);
            }
        }
        assert_eq!(model.document.snapshot().objects()[&id].components.len(), 2);
    }
    component_send(&mut model, kagami::component_form::Action::Load);
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    let component_lock = model
        .document
        .snapshot()
        .dependencies()
        .expect(&model.component_dependencies.notice)
        .clone();
    assert_eq!(component_lock.selection().roots.len(), 2);
    for index in 0..model.kernel_choices.len() {
        send(&mut model, PhysicsAction::Kernel(index));
    }
    let initial = model.document.snapshot().clone();
    let mut resolved = false;
    for _ in 0..32 {
        action(&mut model, Action::Check);
        let report = model.physics_dependencies.report().unwrap();
        match report.outcome() {
            ResolutionOutcome::Resolved { .. } => {
                resolved = true;
                break;
            }
            ResolutionOutcome::Unavailable { issues, .. } => {
                let (_, diagnostic) = issues
                    .iter()
                    .enumerate()
                    .find(|(_, i)| i.reason == resolution::UnavailableReason::AmbiguousProvider)
                    .unwrap();
                let requirement = diagnostic.requirement.clone().unwrap();
                let token = report.token();
                action(
                    &mut model,
                    Action::Browse {
                        report: token,
                        requirement,
                    },
                );
                let report = model.physics_dependencies.report().unwrap();
                let candidate = report
                    .alternatives()
                    .unwrap()
                    .candidates
                    .iter()
                    .position(|p| p.release == chosen)
                    .unwrap();
                let token = report.token();
                action(
                    &mut model,
                    Action::ChooseInstalled {
                        report: token,
                        candidate,
                    },
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(model.document.snapshot(), &initial);
        assert!(!model.scientific_effects.is_pending());
    }
    assert!(resolved, "bounded transitive choices must resolve");
    let bindings: Vec<_> = model.physics_form.bindings().collect();
    assert!(
        bindings.len() > 1,
        "root and transitive decisions are captured"
    );
    assert!(bindings.iter().all(|b| b.provider.release == chosen));
    send(&mut model, PhysicsAction::Confirm(true));
    send(&mut model, PhysicsAction::Apply);
    assert!(model.scientific_effects.is_pending());
    let end = Instant::now() + Duration::from_secs(75);
    while model.scientific_effects.is_pending() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(10));
        let _ = update(
            &mut model,
            Message::Scientific(kagami::message::ScientificAction::Poll),
        );
    }
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    poll(&mut model);
    // The component form's accepted intent survives scientific initialization.
    assert_eq!(
        model.document.snapshot().dependencies(),
        Some(&component_lock)
    );
    let captured_before_choices = model.document.snapshot().clone();
    component_send(&mut model, kagami::component_form::Action::Load);
    assert!(model.component_form.current(&model.document));
    assert!(model.component_form.captured_reset());
    assert_eq!(model.document.snapshot(), &captured_before_choices);
    assert!(!model.scientific_effects.is_pending());
    let (object, body) = captured_before_choices.objects().iter().next().unwrap();
    component_send(
        &mut model,
        kagami::component_form::Action::BeginReplacement {
            object: *object,
            component: body.components.keys().next().unwrap().clone(),
        },
    );
    assert!(model.component_form.current(&model.document));
    assert!(model.component_form.captured_reset());
    assert!(!model.scientific_effects.is_pending());
    assert_eq!(model.document.snapshot(), &captured_before_choices);
    component_send(&mut model, kagami::component_form::Action::Cancel);
    let snapshot = model.document.snapshot().clone();
    let descriptor = snapshot
        .setup()
        .scientific()
        .unwrap()
        .declarations()
        .descriptor();
    for explicit in &bindings {
        assert!(
            descriptor
                .bindings
                .iter()
                .any(|b| b.consumer == explicit.requirement.consumer
                    && b.requirement_slot == explicit.requirement.slot
                    && b.provider == explicit.provider)
        );
    }
    assert!(
        !descriptor
            .contributions
            .iter()
            .any(|c| c.release == unwanted)
    );
    assert!(
        !descriptor
            .contributions
            .iter()
            .any(|c| c.release == newer.release().id()),
        "explicit old-release choices do not drag the current default into the workload"
    );
    send(&mut model, PhysicsAction::LoadCaptured);
    action(&mut model, Action::Check);
    assert!(model.physics_form.is_captured());
    assert!(
        matches!(
            model.physics_dependencies.report().unwrap().outcome(),
            ResolutionOutcome::Resolved { .. }
        ),
        "captured exact bindings are reused despite competing default providers"
    );
    action(&mut model, Action::Clear);
    assert_eq!(model.document.snapshot(), &snapshot);
    let input = dir.path().join("chosen.kagami");
    assert!(model.document.save(Some(input.clone()), "test".into()));
    // No installed inventory is needed to preserve exact captures on file open.
    let mut offline = Model::new(Default::default());
    assert!(offline.document.open(input, true));
    assert_eq!(
        offline.document.snapshot().dependencies(),
        Some(&component_lock)
    );
    assert_eq!(
        offline.document.snapshot().setup().scientific(),
        snapshot.setup().scientific()
    );
    let exported = kagami::export::compile_snapshot(
        offline.document.snapshot(),
        &plugins,
        "chosen-providers".parse().unwrap(),
        Default::default(),
        Default::default(),
        offline.document.limits(),
    )
    .unwrap();
    let portable = orishu_plugin::workload::bundle::read(
        &exported.bytes,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(portable.verified().selection().descriptor(), descriptor);
    // Once delivered, neither inventory, document nor local proposal is consulted.
    drop(model);
    drop(offline);
    drop(plugins);
    drop(store);
    drop(dir);
    let control = || OperationControl::new(Duration::from_secs(60)).unwrap();
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        portable.manifest_bytes(),
        portable.blobs(),
        orishu_runtime::RunScope {
            workload: portable.verified().root(),
            run: ArtifactDigest::sha256_of(b"explicit provider test"),
            epoch: 1,
        },
        &Default::default(),
        Default::default(),
        control(),
    )
    .unwrap();
    admitted.run.advance(control()).unwrap();
    let objects: Vec<_> = admitted
        .run
        .state()
        .objects()
        .scientific_batch::<orishu_plugin::execution::ObjectState>(Default::default())
        .unwrap()
        .iter()
        .collect();
    assert_eq!(objects.len(), 2);
    assert!(objects.iter().all(|o| o.inertial_mass_kilograms.is_some()));
    assert!(objects[0].kinematics.velocity_metres_per_second[0].get() > 0.0);
    assert!(objects[1].kinematics.velocity_metres_per_second[0].get() < 0.0);
}
