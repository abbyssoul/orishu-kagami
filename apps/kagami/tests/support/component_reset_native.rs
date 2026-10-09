//! Actual native replacement messages -> complete consent -> real guest reset.
use super::*;
use kagami::{
    component_form::{Action as C, properties::Input, reset::Flow},
    physics_form::{PhysicsAction as P, dependencies::Action as D},
};

fn component(model: &mut Model, action: C) {
    let _ = update(model, Message::ComponentForm(action));
}
fn physics(model: &mut Model, action: P) {
    let _ = update(model, Message::PhysicsForm(action));
}
fn drain(model: &mut Model) {
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while model.component_dependencies.is_pending() || model.physics_dependencies.is_pending() {
        let _ = update(model, Message::Scientific(ScientificAction::Poll));
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn component_check(model: &mut Model) -> uuid::Uuid {
    component(model, C::Dependencies(D::Check));
    drain(model);
    model
        .component_dependencies
        .report()
        .expect("component report")
        .token()
}
fn physics_check(model: &mut Model) {
    physics(model, P::Dependencies(D::Check));
    drain(model);
    assert!(
        matches!(
            model.physics_dependencies.report().unwrap().outcome(),
            ResolutionOutcome::Resolved { .. }
        ),
        "{}",
        model.physics_dependencies.notice
    );
}
fn tokens(model: &Model) -> (uuid::Uuid, uuid::Uuid) {
    Flow::tokens(&model.component_form, &model.physics_form)
}
fn confirm(model: &mut Model, confirmed: bool) {
    let (component_id, physics_id) = tokens(model);
    component(
        model,
        C::ConfirmReset {
            component: component_id,
            physics: physics_id,
            confirmed,
        },
    );
}
fn apply(model: &mut Model) {
    let (component_id, physics_id) = tokens(model);
    component(
        model,
        C::ApplyReset {
            component: component_id,
            physics: physics_id,
        },
    );
}
fn visible(model: &mut Model, value: bool) {
    component(
        model,
        C::SetProperty {
            form: model.component_form.properties().unwrap().id(),
            property: "visible".try_into().unwrap(),
            value: Input::Boolean(value),
        },
    );
}

struct NativeFixture {
    source: Fixture,
    new: kagami_catalog::ComponentTypeId,
    object: kagami_document::ObjectId,
}
fn native_fixture() -> NativeFixture {
    let mut source = fixture();
    source
        .model
        .scientific_effects
        .extend_components(
            &source.model.document,
            source.model.document.authoring_guard().unwrap(),
            source.attachments.clone(),
            source.lock.clone(),
        )
        .unwrap();
    poll(&mut source.model);
    assert!(
        source.model.document.notice.is_none(),
        "{:?}",
        source.model.document.notice
    );
    let replacement = package(
        "org.test.native.replacement",
        vec![
            (
                "tag".parse().unwrap(),
                Payload::Components(Declaration {
                    scientific: ComponentSchema {
                        name: "org.test.native.replacement.tag".parse().unwrap(),
                        version: 1.try_into().unwrap(),
                        requirements: vec![],
                        properties: vec![Property {
                            id: "visible".parse().unwrap(),
                            required: true,
                            schema: PropertyType::Boolean {
                                default: Some(false),
                            },
                        }],
                        role: ComponentRole::Data,
                        bindings: Default::default(),
                    },
                    presentation: None,
                }),
            ),
            (
                "euler".parse().unwrap(),
                declarations::euler(ArtifactDigest::sha256_of(EULER)),
            ),
            (
                "newtonian".parse().unwrap(),
                declarations::newtonian(ArtifactDigest::sha256_of(GRAVITY)),
            ),
        ],
        &[EULER, GRAVITY],
    );
    source.plugins.revision = source
        .installed
        .store
        .install(source.plugins.revision, &replacement, None, None)
        .unwrap();
    let new = kagami_catalog::ComponentTypeId::exact(
        replacement
            .release()
            .contribution_ref(&"tag".parse().unwrap())
            .unwrap(),
    )
    .unwrap();
    let path = source
        .installed
        ._dir
        .path()
        .join("native-reset-source.kagami");
    assert!(
        source
            .model
            .document
            .save(Some(path.clone()), "test".into())
    );
    // Deliberately start with the source declaration projection only. Selected
    // property inspection must not install the newly available component schema.
    source.model = Model::new(LaunchOptions {
        open_path: Some(path),
        plugin_schemas: source.model.document.schemas().clone(),
        scientific_plugins: Some(source.plugins.clone()),
        kernel_choices: source
            .installed
            .store
            .available_models(&[])
            .unwrap()
            .kernels,
        ..Default::default()
    });
    let object = *source
        .model
        .document
        .snapshot()
        .objects()
        .keys()
        .next()
        .unwrap();
    NativeFixture {
        source,
        new,
        object,
    }
}

fn propose(f: &mut NativeFixture) {
    let model = &mut f.source.model;
    component(
        model,
        C::BeginReplacement {
            object: f.object,
            component: f.source.tag.clone(),
        },
    );
    component(
        model,
        C::ReplaceWith {
            proposal: model.component_form.proposal_id(),
            component: f.new.clone(),
        },
    );
    assert!(model.component_form.captured_reset());
    let report = component_check(model);
    component(model, C::Dependencies(D::LoadProperties { report }));
    drain(model);
    assert!(
        model.component_form.properties().is_some(),
        "{}",
        model.component_dependencies.notice
    );
    assert!(model.document.schemas().get(&f.new).is_none());
    visible(model, true);
}
fn stage(model: &mut Model) {
    let report = component_check(model);
    component(model, C::Dependencies(D::ApplyLock { report }));
    drain(model);
    assert!(
        model.component_reset.is_staged(),
        "{}",
        model.component_dependencies.notice
    );
    assert!(
        model.physics_form.selected.is_empty(),
        "new models must be explicitly selected"
    );
    assert!(!model.scientific_effects.is_pending());
}
fn choose_new_physics(f: &mut NativeFixture) {
    let model = &mut f.source.model;
    let indices: Vec<_> = model
        .kernel_choices
        .iter()
        .enumerate()
        .filter(|(_, k)| k.contribution.release == f.new.contribution().unwrap().release)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(indices.len(), 2);
    for index in indices {
        physics(model, P::Kernel(index));
    }
    physics_check(model);
}

#[test]
fn native_reset_stages_then_requires_exact_combined_consent_and_exports_new_physics() {
    let mut f = native_fixture();
    let before = f.source.model.document.snapshot().clone();
    propose(&mut f);
    stage(&mut f.source.model);
    confirm(&mut f.source.model, true);
    assert!(
        !f.source
            .model
            .component_reset
            .confirmed(&f.source.model.component_form, &f.source.model.physics_form)
    );
    choose_new_physics(&mut f);
    // Ordinary physics/history consent cannot authorize replacement.
    physics(&mut f.source.model, P::Confirm(true));
    physics(&mut f.source.model, P::Apply);
    apply(&mut f.source.model);
    assert!(!f.source.model.scientific_effects.is_pending());
    assert_eq!(f.source.model.document.snapshot(), &before);
    confirm(&mut f.source.model, true);
    let expired = tokens(&f.source.model);
    physics(&mut f.source.model, P::Step("0.02".into()));
    component(
        &mut f.source.model,
        C::ConfirmReset {
            component: expired.0,
            physics: expired.1,
            confirmed: true,
        },
    );
    assert!(
        !f.source
            .model
            .component_reset
            .confirmed(&f.source.model.component_form, &f.source.model.physics_form)
    );
    physics_check(&mut f.source.model);
    confirm(&mut f.source.model, true);
    component(
        &mut f.source.model,
        C::ApplyReset {
            component: expired.0,
            physics: expired.1,
        },
    );
    assert!(!f.source.model.scientific_effects.is_pending());
    apply(&mut f.source.model);
    assert!(
        f.source.model.scientific_effects.is_pending(),
        "{}",
        f.source.model.component_dependencies.notice
    );
    assert_eq!(f.source.model.document.snapshot(), &before);
    confirm(&mut f.source.model, false);
    poll(&mut f.source.model);
    assert_eq!(f.source.model.document.snapshot(), &before);
    assert!(f.source.model.document.schemas().get(&f.new).is_none());
    assert!(!f.source.model.component_reset.is_running());
    confirm(&mut f.source.model, true);
    apply(&mut f.source.model);
    assert!(
        f.source.model.scientific_effects.is_pending(),
        "{}",
        f.source.model.component_dependencies.notice
    );
    poll(&mut f.source.model);
    assert!(
        f.source.model.document.notice.is_none(),
        "{:?}",
        f.source.model.document.notice
    );
    let after = f.source.model.document.snapshot().clone();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    let body = after.object(f.object).unwrap();
    assert!(!body.components.contains_key(&f.source.tag));
    assert_eq!(
        body.components[&f.new].properties[&"visible".try_into().unwrap()].authored(),
        kagami_document::AuthoredValue::Boolean(true)
    );
    assert!(
        after
            .objects()
            .iter()
            .any(|(id, o)| *id != f.object && o.components.contains_key(&f.source.tag))
    );
    let old = before.setup().scientific().unwrap();
    let new = after.setup().scientific().unwrap();
    assert_eq!(new.time_step().seconds(), 0.02);
    assert!(
        new.declarations()
            .descriptor()
            .kernel_instances
            .iter()
            .all(|k| k.contribution.release == f.new.contribution().unwrap().release)
    );
    assert!(new.captures().values().all(|new| {
        old.captures()
            .values()
            .all(|old| !Arc::ptr_eq(&new.state, &old.state))
    }));
    assert!(!f.source.model.component_reset.is_staged());
    assert!(
        f.source
            .model
            .document
            .submit(kagami_session::SessionCommand::Undo)
    );
    assert_eq!(f.source.model.document.snapshot().setup(), before.setup());
    assert_eq!(
        f.source.model.document.snapshot().objects(),
        before.objects()
    );
    assert_eq!(
        f.source.model.document.snapshot().dependencies(),
        before.dependencies()
    );
    assert!(
        f.source
            .model
            .document
            .submit(kagami_session::SessionCommand::Redo)
    );
    let path = f.source.installed._dir.path().join("native-reset.kagami");
    assert!(
        f.source
            .model
            .document
            .save(Some(path.clone()), "test".into())
    );
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    assert_eq!(offline.document.snapshot().setup(), after.setup());
    assert_eq!(
        offline.document.snapshot().dependencies(),
        after.dependencies()
    );
    let exported = kagami::export::compile_snapshot(
        offline.document.snapshot(),
        &f.source.plugins,
        "native-reset".parse().unwrap(),
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
    drop(f);
    drop(offline);
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        portable.manifest_bytes(),
        portable.blobs(),
        orishu_runtime::RunScope {
            workload: portable.verified().root(),
            run: ArtifactDigest::sha256_of(b"native reset"),
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
        .scientific_batch::<ObjectState>(Default::default())
        .unwrap()
        .iter()
        .collect();
    assert_eq!(objects.len(), 2);
    assert!(objects[0].kinematics.velocity_metres_per_second[0].get() > 0.0);
    assert!(objects[1].kinematics.velocity_metres_per_second[0].get() < 0.0);
}

#[test]
fn cancelled_stale_changed_and_failed_native_reset_never_adopt_partial_intent() {
    let mut f = native_fixture();
    let before = f.source.model.document.snapshot().clone();
    propose(&mut f);
    let report = component_check(&mut f.source.model);
    component(
        &mut f.source.model,
        C::Dependencies(D::ApplyLock { report }),
    );
    component(&mut f.source.model, C::Cancel);
    drain(&mut f.source.model);
    assert!(!f.source.model.component_reset.is_staged());
    assert_eq!(f.source.model.document.snapshot(), &before);
    assert!(f.source.model.document.schemas().get(&f.new).is_none());

    propose(&mut f);
    stage(&mut f.source.model);
    choose_new_physics(&mut f);
    confirm(&mut f.source.model, true);
    visible(&mut f.source.model, false);
    assert!(
        !f.source.model.component_reset.is_staged(),
        "component input edit discards old physics/consent"
    );
    apply(&mut f.source.model);
    assert!(!f.source.model.scientific_effects.is_pending());
    stage(&mut f.source.model);
    choose_new_physics(&mut f);
    let field = *f
        .source
        .model
        .physics_form
        .selected
        .iter()
        .find(|i| f.source.model.kernel_choices[**i].contract == ExecutionContractId::Field)
        .unwrap();
    physics(
        &mut f.source.model,
        P::Parameter(kagami::physics_form::ParameterAction {
            kernel: field,
            property: "exclusion-radius".parse().unwrap(),
            input: Some(ConfigurationInput::Expression {
                source: "-4 mm".into(),
            }),
        }),
    );
    physics_check(&mut f.source.model);
    confirm(&mut f.source.model, true);
    apply(&mut f.source.model);
    assert!(f.source.model.scientific_effects.is_pending());
    poll(&mut f.source.model);
    assert_eq!(f.source.model.document.snapshot(), &before);
    assert!(f.source.model.document.schemas().get(&f.new).is_none());
    assert!(
        !f.source
            .model
            .component_reset
            .confirmed(&f.source.model.component_form, &f.source.model.physics_form)
    );
    assert!(
        f.source
            .model
            .document
            .notice
            .as_ref()
            .unwrap()
            .contains("kernel rejected invalid input")
    );

    physics(
        &mut f.source.model,
        P::Parameter(kagami::physics_form::ParameterAction {
            kernel: field,
            property: "exclusion-radius".parse().unwrap(),
            input: Some(ConfigurationInput::Expression {
                source: "4 mm".into(),
            }),
        }),
    );
    physics_check(&mut f.source.model);
    confirm(&mut f.source.model, true);
    apply(&mut f.source.model);
    assert!(f.source.model.scientific_effects.is_pending());
    assert!(f.source.model.document.edit(vec![Edit::RenameObject {
        object: f.object,
        name: DisplayName::new("intervening").unwrap()
    }]));
    let intervening = f.source.model.document.snapshot().clone();
    poll(&mut f.source.model);
    assert_eq!(f.source.model.document.snapshot(), &intervening);
    assert!(f.source.model.document.schemas().get(&f.new).is_none());
    assert!(!f.source.model.component_reset.is_staged());
}

#[test]
fn captured_binding_only_change_resets_with_explicit_provider_and_no_object_edits() {
    let mut f = native_fixture();
    let dynamics = declarations::vocabulary()
        .into_iter()
        .find(|(id, _)| id.as_str() == "dynamics")
        .unwrap();
    let alternative = package("org.test.alternate.dynamics", vec![dynamics], &[]);
    f.source.plugins.revision = f
        .source
        .installed
        .store
        .install(f.source.plugins.revision, &alternative, None, None)
        .unwrap();
    let _ = update(
        &mut f.source.model,
        Message::Plugins(kagami::plugins::window::Action::Refresh),
    );
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while f.source.model.plugin_management.is_pending() {
        let _ = update(
            &mut f.source.model,
            Message::Plugins(kagami::plugins::window::Action::Poll),
        );
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        f.source.model.inventory_revision,
        Some(f.source.plugins.revision)
    );
    let before = f.source.model.document.snapshot().clone();
    component(&mut f.source.model, C::Load);
    assert!(f.source.model.component_form.captured_reset());
    assert!(f.source.model.component_form.attachment().is_none());
    let report = component_check(&mut f.source.model);
    let requirement = RequirementKey {
        consumer: f.source.tag.contribution().unwrap().clone(),
        slot: "dynamics".parse().unwrap(),
    };
    component(
        &mut f.source.model,
        C::Dependencies(D::Browse {
            report,
            requirement: requirement.clone(),
        }),
    );
    drain(&mut f.source.model);
    let report = f.source.model.component_dependencies.report().unwrap();
    let candidate = report
        .alternatives()
        .unwrap()
        .candidates
        .iter()
        .position(|c| c.release == alternative.release().id())
        .unwrap();
    let token = report.token();
    component(
        &mut f.source.model,
        C::Dependencies(D::ChooseInstalled {
            report: token,
            candidate,
        }),
    );
    stage(&mut f.source.model);
    assert_eq!(f.source.model.document.snapshot(), &before);
    let indices: Vec<_> = f
        .source
        .model
        .kernel_choices
        .iter()
        .enumerate()
        .filter(|(_, k)| k.contribution.release == f.new.contribution().unwrap().release)
        .map(|(i, _)| i)
        .collect();
    let integrator = f.source.model.kernel_choices[indices
        .iter()
        .copied()
        .find(|i| f.source.model.kernel_choices[*i].contract == ExecutionContractId::Dynamics)
        .unwrap()]
    .contribution
    .clone();
    for index in indices {
        physics(&mut f.source.model, P::Kernel(index));
    }
    physics(&mut f.source.model, P::Dependencies(D::Check));
    drain(&mut f.source.model);
    let report = f
        .source
        .model
        .physics_dependencies
        .report()
        .unwrap()
        .token();
    physics(
        &mut f.source.model,
        P::Dependencies(D::Browse {
            report,
            requirement: RequirementKey {
                consumer: integrator,
                slot: "dynamics".parse().unwrap(),
            },
        }),
    );
    drain(&mut f.source.model);
    let report = f.source.model.physics_dependencies.report().unwrap();
    let candidate = report
        .alternatives()
        .unwrap()
        .candidates
        .iter()
        .position(|c| c.release == f.source.installed.vocabulary)
        .unwrap();
    let token = report.token();
    physics(
        &mut f.source.model,
        P::Dependencies(D::ChooseInstalled {
            report: token,
            candidate,
        }),
    );
    physics_check(&mut f.source.model);
    confirm(&mut f.source.model, true);
    apply(&mut f.source.model);
    assert!(
        f.source.model.scientific_effects.is_pending(),
        "{}",
        f.source.model.component_dependencies.notice
    );
    poll(&mut f.source.model);
    assert!(
        f.source.model.document.notice.is_none(),
        "{:?}",
        f.source.model.document.notice
    );
    let after = f.source.model.document.snapshot();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    assert_eq!(after.objects(), before.objects());
    let binding = after
        .dependencies()
        .unwrap()
        .selection()
        .bindings
        .iter()
        .find(|b| b.requirement == requirement)
        .unwrap();
    assert_eq!(binding.provider.release, alternative.release().id());
    assert_ne!(after.dependencies(), before.dependencies());
    assert!(
        f.source
            .model
            .document
            .submit(kagami_session::SessionCommand::Undo)
    );
    assert_eq!(
        f.source.model.document.snapshot().dependencies(),
        before.dependencies()
    );
    assert_eq!(f.source.model.document.snapshot().setup(), before.setup());
}
