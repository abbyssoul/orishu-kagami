//! Object/history atomicity through real Components and native authoring messages.
use super::*;
use kagami::{
    launch::LaunchOptions,
    message::{Authoritative, Message, ScientificAction},
    model::Model,
    physics_form::PhysicsAction,
    scientific_effect::ScientificPlugins,
    update::update,
};
use kagami_document::{DisplayName, ExperimentCommand as Edit, ObjectSpec, Transform, Vector3};

fn poll(model: &mut Model) {
    let until = std::time::Instant::now() + Duration::from_secs(75);
    while model.scientific_effects.is_pending() {
        let _ = update(model, Message::Scientific(ScientificAction::Poll));
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn author(model: &mut Model, action: Authoritative) {
    let _ = update(model, Message::Authoritative(action));
}

#[test]
fn object_edits_regenerate_only_history_and_commit_atomically_with_real_allocator_ids() {
    let installed = Installed::new();
    let components = installed.store.available_components(&[]).unwrap();
    let choices = installed.store.available_models(&[]).unwrap();
    let component = kagami_catalog::ComponentTypeId::exact(ContributionRef {
        release: installed.vocabulary,
        extension_point: "orishu.model.components/v1".parse().unwrap(),
        local_id: "dynamics".parse().unwrap(),
    })
    .unwrap();
    let properties = component_defaults(components.schemas.get(&component).unwrap());
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: components.schemas,
        kernel_choices: choices.kernels,
        scientific_plugins: Some(ScientificPlugins {
            store: Arc::new(PluginStore::open(installed._dir.path()).unwrap()),
            revision: choices.revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    author(&mut model, Authoritative::CreateObject);
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    author(&mut model, Authoritative::CreateObject);
    let removed = *model
        .document
        .snapshot()
        .objects()
        .keys()
        .next_back()
        .unwrap();
    author(&mut model, Authoritative::RemoveObject(removed));
    for i in 0..model.kernel_choices.len() {
        let _ = update(&mut model, Message::PhysicsForm(PhysicsAction::Kernel(i)));
    }
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Confirm(true)),
    );
    let _ = update(&mut model, Message::PhysicsForm(PhysicsAction::Apply));
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let original = model.document.snapshot().clone();
    let old = original.setup().scientific().unwrap();
    let field = old
        .captures()
        .iter()
        .find(|(_, c)| c.context.execution_contract == ExecutionContractId::Field)
        .unwrap()
        .0
        .clone();
    let integrator = old
        .captures()
        .iter()
        .find(|(_, c)| c.context.execution_contract == ExecutionContractId::Dynamics)
        .unwrap()
        .0
        .clone();

    // The shared preparation is not an acceptance bypass. It may expose a new
    // initial packet, but normal update still requires a matching history blob.
    let input_path = installed._dir.path().join("history-edit.kagami");
    assert!(model.document.save(Some(input_path.clone()), "test".into()));
    let loaded = kagami_session::store::load(
        &kagami_session::store::RealFileStore,
        &kagami_session::DocumentTarget::new(input_path.clone()).unwrap(),
    )
    .unwrap();
    let experiment = loaded
        .document
        .into_experiment(model.document.schemas(), model.document.limits())
        .unwrap();
    let attach = vec![Edit::AttachComponent {
        object,
        component: component.clone(),
        properties: properties.clone(),
    }];
    let proposal = kagami_document::update::prepare_history_edit(
        &experiment,
        &attach,
        model.document.schemas(),
        model.document.limits(),
    )
    .unwrap();
    let projected = proposal
        .initial_dynamics(&integrator, BulkLimits::default())
        .unwrap();
    assert_eq!(
        Batch::<DynamicEntity>::read(&projected, BulkLimits::default())
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        kagami_document::update(
            &experiment,
            &attach,
            model.document.schemas(),
            model.document.limits()
        ),
        Err(kagami_document::Rejection::Scientific(
            kagami_document::scientific::ScientificError::History
        ))
    ));
    assert!(
        kagami_document::update::prepare_history_edit(
            &experiment,
            &[Edit::SetTimeStep(
                kagami_document::TimeStep::new(0.1).unwrap()
            )],
            model.document.schemas(),
            model.document.limits()
        )
        .is_err()
    );
    assert!(
        kagami_document::update::prepare_history_edit(
            &experiment,
            &vec![
                Edit::RenameObject {
                    object,
                    name: DisplayName::new("bounded").unwrap()
                };
                model.document.limits().max_commands_per_batch
            ],
            model.document.schemas(),
            model.document.limits()
        )
        .is_err()
    );

    // Add Dynamics from the actual inspector action: neither the component nor
    // the history is visible until the complete candidate is accepted.
    author(
        &mut model,
        Authoritative::AttachComponent(object, component.clone()),
    );
    super::component_extension::begin_native_addition(&mut model);
    assert!(model.scientific_effects.is_pending());
    assert_eq!(model.document.snapshot(), &original);
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let attached = model.document.snapshot().clone();
    assert_eq!(attached.revision().get(), original.revision().get() + 1);
    assert!(
        attached
            .object(object)
            .unwrap()
            .components
            .contains_key(&component)
    );
    let current = attached.setup().scientific().unwrap();
    assert!(Arc::ptr_eq(
        &old.captures()[&field].state,
        &current.captures()[&field].state
    ));
    assert_ne!(
        old.captures()[&integrator].history_entities,
        current.captures()[&integrator].history_entities
    );
    assert!(model.document.submit(kagami_session::SessionCommand::Undo));
    assert!(
        model
            .document
            .snapshot()
            .object(object)
            .unwrap()
            .components
            .is_empty()
    );
    assert_eq!(model.document.snapshot().setup().scientific(), Some(old));
    assert!(model.document.submit(kagami_session::SessionCommand::Redo));
    assert_eq!(
        model.document.snapshot().setup().scientific(),
        Some(current)
    );
    assert!(
        !model.scientific_effects.is_pending(),
        "undo/redo never initialize guests"
    );

    // A mass edit follows the same path, while malformed units remain an
    // ordinary refusal and never execute a guest.
    let property =
        kagami_catalog::schema::property_name(&"inertial-mass".parse().unwrap()).unwrap();
    author(
        &mut model,
        Authoritative::SetProperty {
            object,
            component: component.clone(),
            property: property.clone(),
            source: "2 m".into(),
        },
    );
    assert!(!model.scientific_effects.is_pending());
    assert_eq!(
        model.document.snapshot().setup().scientific(),
        Some(current)
    );
    author(
        &mut model,
        Authoritative::SetProperty {
            object,
            component: component.clone(),
            property,
            source: "2 kg".into(),
        },
    );
    assert!(model.scientific_effects.is_pending());
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let mass_edit = model.document.snapshot().clone();
    let packet = Batch::<DynamicEntity>::read(
        mass_edit.setup().scientific().unwrap().captures()[&integrator]
            .history_entities
            .as_ref()
            .unwrap(),
        BulkLimits::default(),
    )
    .unwrap();
    assert_eq!(packet.get(0).unwrap().inertial_mass_kilograms.get(), 2.0);

    // Compound edits mint from real high-water counters (the removed identity
    // must not be reused), and preserve fields even when positions/members change.
    let commands = vec![
        Edit::SetTransform {
            object,
            transform: Transform {
                translation: Vector3::new(2.0, 3.0, 4.0).unwrap(),
                ..Default::default()
            },
        },
        Edit::CreateObject(Box::new(
            ObjectSpec::new(DisplayName::new("new member").unwrap())
                .with_component(component.clone(), properties),
        )),
    ];
    assert!(
        !model.document.edit(commands.clone()),
        "normal authority still rejects stale history"
    );
    let before = model.document.snapshot().clone();
    model
        .scientific_effects
        .edit_scene(&model.document, commands)
        .unwrap();
    assert_eq!(model.document.snapshot(), &before);
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    // Persist component-only intent too. Later removal must update this graph
    // and regenerate history in the same revision, never leaving either stale.
    let root = component.contribution().unwrap().clone();
    let lock = Arc::new(
        orishu_plugin::authoring_lock::SelectionLock::new(
            orishu_plugin::resolution::Selection {
                roots: vec![root.clone()],
                contributions: vec![root],
                bindings: vec![],
            },
            model.document.limits().dependencies,
        )
        .unwrap(),
    );
    assert!(
        model
            .document
            .edit(vec![Edit::AdoptDependencies(lock.clone())])
    );
    let after = model.document.snapshot().clone();
    let created = *after.objects().keys().next_back().unwrap();
    assert!(created > removed);
    let setup = after.setup().scientific().unwrap();
    assert!(Arc::ptr_eq(
        &old.captures()[&field].state,
        &setup.captures()[&field].state
    ));
    let history = setup.captures()[&integrator]
        .history_entities
        .as_ref()
        .unwrap();
    let packet = Batch::<DynamicEntity>::read(history, BulkLimits::default()).unwrap();
    assert_eq!(packet.len(), 2);
    assert_eq!(
        packet
            .get(0)
            .unwrap()
            .kinematics
            .position_metres
            .map(FiniteF64::get),
        [2.0, 3.0, 4.0]
    );
    assert_eq!(packet.get(1).unwrap().id.0, created.get());

    // Capture the nonempty edited scene as a portable workload for independent
    // admission after the UI, inventory and original files are gone.
    assert!(model.document.save(Some(input_path.clone()), "test".into()));
    let output = installed._dir.path().join("history-edit.orishu");
    kagami::export::execute(kagami::export::ExportArgs {
        experiment: input_path,
        output: output.clone(),
        name: "history-edit".parse().unwrap(),
        directory: Some(installed._dir.path().to_path_buf()),
        expected_inventory_revision: Some(choices.revision),
        enable_plugin: vec![],
        disable_plugin: vec![],
        json: false,
    })
    .unwrap();
    let portable = std::fs::read(output).unwrap();

    // Cancellation cannot publish just the removal, or just the history.
    model
        .scientific_effects
        .edit_scene(&model.document, vec![Edit::RemoveObject(created)])
        .unwrap();
    model.scientific_effects.cancel();
    poll(&mut model);
    assert_eq!(model.document.snapshot(), &after);
    model
        .scientific_effects
        .edit_scene(&model.document, vec![Edit::RemoveObject(created)])
        .unwrap();
    author(
        &mut model,
        Authoritative::RenameObject(object, "intervening edit".into()),
    );
    let intervening = model.document.snapshot().clone();
    poll(&mut model);
    assert_eq!(
        model.document.snapshot(),
        &intervening,
        "stale history completion cannot discard an intervening ordinary edit"
    );
    assert!(model.document.snapshot().object(created).is_some());
    author(&mut model, Authoritative::RemoveObject(created));
    assert!(model.scientific_effects.is_pending());
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    assert_eq!(model.document.snapshot().object_count(), 1);
    assert!(Arc::ptr_eq(
        model.document.snapshot().dependencies().unwrap(),
        &lock
    ));
    // A stale completion must not publish even an otherwise valid pruned lock.
    // Schedule directly, then edit the authority before any completion fold;
    // correctness does not depend on whether the worker finishes quickly.
    let pruned = Arc::new(
        lock.for_roots(&[], model.document.limits().dependencies)
            .unwrap(),
    );
    model
        .scientific_effects
        .edit_scene(
            &model.document,
            vec![
                Edit::DetachComponent {
                    object,
                    component: component.clone(),
                },
                Edit::AdoptDependencies(pruned),
            ],
        )
        .unwrap();
    assert!(model.document.edit(vec![Edit::RenameObject {
        object,
        name: DisplayName::new("keep the intervening revision").unwrap(),
    }]));
    let intervening = model.document.snapshot().clone();
    poll(&mut model);
    assert_eq!(model.document.snapshot(), &intervening);
    assert_eq!(model.document.snapshot().dependencies(), Some(&lock));
    let before_detach = model.document.snapshot().clone();
    author(
        &mut model,
        Authoritative::DetachComponent(object, component),
    );
    assert!(model.scientific_effects.is_pending());
    assert_eq!(model.document.snapshot(), &before_detach);
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    assert_eq!(
        model.document.snapshot().revision().get(),
        before_detach.revision().get() + 1
    );
    let detached = model.document.snapshot().clone();
    assert!(
        detached
            .dependencies()
            .unwrap()
            .selection()
            .contributions
            .is_empty()
    );
    assert!(Arc::ptr_eq(
        &detached.setup().scientific().unwrap().captures()[&field].state,
        &old.captures()[&field].state,
    ));
    author(&mut model, Authoritative::Undo);
    assert_eq!(model.document.snapshot().dependencies(), Some(&lock));
    assert_eq!(
        model.document.snapshot().setup().scientific(),
        before_detach.setup().scientific()
    );
    author(&mut model, Authoritative::Redo);
    assert_eq!(
        model.document.snapshot().dependencies(),
        detached.dependencies()
    );
    assert_eq!(
        model.document.snapshot().setup().scientific(),
        detached.setup().scientific()
    );
    assert!(!model.scientific_effects.is_pending());
    let history = &model
        .document
        .snapshot()
        .setup()
        .scientific()
        .unwrap()
        .captures()[&integrator];
    assert_eq!(
        Batch::<DynamicEntity>::read(
            history.history_entities.as_ref().unwrap(),
            BulkLimits::default()
        )
        .unwrap()
        .len(),
        0
    );
    drop(model);
    drop(installed);
    let bundle =
        orishu_plugin::workload::bundle::read(&portable, Default::default(), Default::default())
            .unwrap();
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        bundle.manifest_bytes(),
        bundle.blobs(),
        orishu_runtime::RunScope {
            workload: bundle.verified().root(),
            run: ArtifactDigest::sha256_of(b"history edit"),
            epoch: 1,
        },
        &Default::default(),
        Default::default(),
        control(),
    )
    .unwrap();
    admitted.run.advance(control()).unwrap();
}
