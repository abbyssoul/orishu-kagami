//! Atomic explicit replacement through native messages and the shared resolver.
use super::*;
use kagami::{
    component_form::{Action as C, properties::Input},
    message::Authoritative,
};
use kagami_catalog::ComponentTypeId;

fn choose(
    model: &mut Model,
    object: kagami_document::ObjectId,
    previous: ComponentTypeId,
    next: ComponentTypeId,
) {
    component_send(
        model,
        C::BeginReplacement {
            object,
            component: previous,
        },
    );
    assert!(
        model.component_form.current(&model.document),
        "{:?}",
        model.document.notice
    );
    let proposal = model.component_form.proposal_id();
    component_send(
        model,
        C::ReplaceWith {
            proposal,
            component: next,
        },
    );
    assert!(
        model.component_form.attachment().is_some(),
        "{:?}",
        model.document.notice
    );
}

#[test]
fn replacement_prunes_only_unreachable_intent_and_restores_objects_and_locks_together() {
    for (shared, promote) in [(false, false), (true, false), (false, true), (true, true)] {
        let (dir, store, mut model, previous) = component_fixture_with_lock(true);
        let original = model.document.snapshot().dependencies().unwrap().clone();
        let object = *model.document.snapshot().objects().keys().next().unwrap();
        let provider = original.selection().bindings[0].provider.clone();
        let reference = if promote {
            provider.clone()
        } else {
            let release = store
                .list()
                .unwrap()
                .releases
                .into_iter()
                .find(|r| r.plugin_id.as_str() == "org.test.provider-001")
                .unwrap()
                .release;
            ContributionRef {
                release,
                extension_point: "orishu.model.components/v1".parse().unwrap(),
                local_id: "dynamics".parse().unwrap(),
            }
        };
        let next = ComponentTypeId::exact(reference.clone()).unwrap();
        if shared {
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
                Authoritative::AttachComponent(second, previous.clone()).into(),
            );
            component_attachment::accept(&mut model);
        }
        let before = model.document.snapshot().clone();
        choose(&mut model, object, previous.clone(), next.clone());
        assert_eq!(model.document.snapshot(), &before);
        assert_eq!(model.component_form.bindings().count(), usize::from(shared));
        property_input(&mut model, "inertial_mass", Input::Quantity("7 kg".into()));
        component_attachment::accept(&mut model);
        assert!(
            model.component_form.replacement().is_none(),
            "acceptance ends the replacement chooser"
        );
        let after = model.document.snapshot().clone();
        assert_eq!(after.revision().get(), before.revision().get() + 1);
        let edited = after.object(object).unwrap();
        assert!(!edited.components.contains_key(&previous));
        assert_eq!(
            edited.components[&next].properties[&"inertial_mass".try_into().unwrap()].si_value(),
            Some(7.0)
        );
        assert_eq!(after.object_count(), before.object_count());
        for (id, other) in before.objects() {
            if *id != object {
                assert_eq!(after.object(*id), Some(other));
            }
        }
        let lock = after.dependencies().unwrap();
        assert_eq!(lock.selection().roots.len(), 1 + usize::from(shared));
        assert!(lock.selection().roots.contains(&reference));
        assert_eq!(
            lock.selection()
                .roots
                .contains(previous.contribution().unwrap()),
            shared
        );
        assert_eq!(lock.selection().bindings.len(), usize::from(shared));
        if shared {
            assert_eq!(lock.selection().bindings, original.selection().bindings);
        } else {
            assert_eq!(lock.selection().contributions, vec![reference]);
        }
        let path = dir.path().join("replacement.kagami");
        assert!(model.document.save(Some(path.clone()), "test".into()));
        let offline = Model::new(LaunchOptions {
            open_path: Some(path),
            ..Default::default()
        });
        assert_eq!(
            offline.document.snapshot().dependencies(),
            after.dependencies()
        );
        assert!(
            offline
                .document
                .snapshot()
                .object(object)
                .unwrap()
                .components
                .contains_key(&next)
        );
        let _ = update(&mut model, Authoritative::Undo.into());
        assert_eq!(model.document.snapshot().objects(), before.objects());
        assert_eq!(
            model.document.snapshot().dependencies(),
            before.dependencies()
        );
        let _ = update(&mut model, Authoritative::Redo.into());
        assert_eq!(model.document.snapshot().objects(), after.objects());
        assert_eq!(
            model.document.snapshot().dependencies(),
            after.dependencies()
        );
    }
}

#[test]
fn invalid_values_stale_choices_and_cancelled_replacements_never_detach_the_original() {
    let (_dir, _store, mut model, previous) = component_fixture_with_lock(true);
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let next = ComponentTypeId::exact(
        model
            .document
            .snapshot()
            .dependencies()
            .unwrap()
            .selection()
            .bindings[0]
            .provider
            .clone(),
    )
    .unwrap();
    component_send(
        &mut model,
        C::BeginReplacement {
            object,
            component: previous.clone(),
        },
    );
    let expired = model.component_form.proposal_id();
    component_action(&mut model, Action::Check);
    assert!(
        model.component_dependencies.report().is_none(),
        "a picker is not a resolved graph"
    );
    component_send(&mut model, C::Cancel);
    component_send(
        &mut model,
        C::BeginReplacement {
            object,
            component: previous.clone(),
        },
    );
    component_send(
        &mut model,
        C::ReplaceWith {
            proposal: expired,
            component: next.clone(),
        },
    );
    assert!(model.component_form.attachment().is_none());
    assert!(model.document.notice.as_ref().unwrap().contains("stale"));
    choose(&mut model, object, previous.clone(), next.clone());
    let before = model.document.snapshot().clone();
    property_input(&mut model, "inertial_mass", Input::Quantity("2 m".into()));
    component_action(&mut model, Action::Check);
    let token = model.component_dependencies.report().unwrap().token();
    component_action(&mut model, Action::ApplyLock { report: token });
    assert_eq!(model.document.snapshot(), &before);
    assert!(model.document.notice.is_some());

    for cancel in [false, true] {
        choose(&mut model, object, previous.clone(), next.clone());
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
        if cancel {
            component_send(&mut model, C::Cancel);
        } else {
            assert!(
                model
                    .document
                    .edit(vec![kagami_document::ExperimentCommand::RenameObject {
                        object,
                        name: kagami_document::DisplayName::new("intervening edit").unwrap()
                    }])
            );
        }
        let before = model.document.snapshot().clone();
        component_poll(&mut model);
        assert_eq!(model.document.snapshot(), &before);
        assert!(
            model
                .document
                .snapshot()
                .object(object)
                .unwrap()
                .components
                .contains_key(&previous)
        );
        assert!(
            !model
                .document
                .snapshot()
                .object(object)
                .unwrap()
                .components
                .contains_key(&next)
        );
    }
}
