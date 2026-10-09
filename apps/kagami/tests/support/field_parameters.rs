//! Field-only edits through the same native messages as the inspector form.
use super::*;
use kagami::{
    launch::LaunchOptions,
    message::{Message, ScientificAction},
    model::Model,
    physics_form::{ParameterAction, PhysicsAction},
    scientific_effect::{EffectError, ScientificPlugins},
    update::update,
};

fn act(model: &mut Model, action: PhysicsAction) {
    let _ = update(model, Message::PhysicsForm(action));
}
fn poll(model: &mut Model) {
    let until = std::time::Instant::now() + Duration::from_secs(75);
    while model.scientific_effects.is_pending() {
        let _ = update(model, Message::Scientific(ScientificAction::Poll));
        assert!(
            std::time::Instant::now() < until,
            "scientific edit timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn parameter(model: &mut Model, kernel: usize, property: &str, source: &str) {
    act(
        model,
        PhysicsAction::Parameter(ParameterAction {
            kernel,
            property: property.parse().unwrap(),
            input: Some(ConfigurationInput::Expression {
                source: source.into(),
            }),
        }),
    );
}

#[test]
fn targeted_parameters_preserve_other_fields_and_history_and_guard_the_copied_revision() {
    let installed = Installed::new();
    // A distinct test field family using the same independently instantiated
    // reference executable. This is not two solvers competing for one family.
    let mut family = declarations::vocabulary()
        .into_iter()
        .find(|(id, _)| id.as_str() == "gravity")
        .unwrap()
        .1;
    let Payload::Fields(declaration) = &mut family else {
        unreachable!()
    };
    declaration.scientific.name = "org.test.secondary-field".parse().unwrap();
    let mut solver = declarations::newtonian(ArtifactDigest::sha256_of(GRAVITY));
    let Payload::FieldModels(declaration) = &mut solver else {
        unreachable!()
    };
    declaration.scientific.name = "org.test.secondary-model".parse().unwrap();
    declaration
        .scientific
        .requirements
        .iter_mut()
        .find(|r| r.slot.as_str() == "gravity")
        .unwrap()
        .contract = family.contract_ref(&Default::default()).unwrap();
    let extra = package(
        "org.test.secondary",
        vec![
            ("family".parse().unwrap(), family),
            ("model".parse().unwrap(), solver),
        ],
        &[GRAVITY],
    );
    let revision = installed
        .store
        .install(
            installed.request.expected_inventory_revision,
            &extra,
            None,
            None,
        )
        .unwrap();
    let available = installed.store.available_components(&[]).unwrap();
    let choices = installed.store.available_models(&[]).unwrap();
    let component = kagami_catalog::ComponentTypeId::exact(ContributionRef {
        release: installed.vocabulary,
        extension_point: "orishu.model.components/v1".parse().unwrap(),
        local_id: "dynamics".parse().unwrap(),
    })
    .unwrap();
    let object = kagami_document::ObjectSpec::new(
        kagami_document::DisplayName::new("history member").unwrap(),
    )
    .with_component(
        component.clone(),
        component_defaults(available.schemas.get(&component).unwrap()),
    );
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: available.schemas,
        kernel_choices: choices.kernels,
        scientific_plugins: Some(ScientificPlugins {
            store: Arc::new(PluginStore::open(installed._dir.path()).unwrap()),
            revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    assert!(
        model
            .document
            .edit(vec![kagami_document::ExperimentCommand::CreateObject(
                Box::new(object)
            )])
    );
    for index in 0..model.kernel_choices.len() {
        act(&mut model, PhysicsAction::Kernel(index));
    }
    act(&mut model, PhysicsAction::Confirm(true));
    act(&mut model, PhysicsAction::Apply);
    assert!(model.scientific_effects.is_pending());
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let before = model.document.snapshot().clone();
    let old = before.setup().scientific().unwrap();
    assert_eq!(old.captures().len(), 3);
    let field = model
        .kernel_choices
        .iter()
        .position(|c| c.contribution.local_id.as_str() == "newtonian")
        .unwrap();
    let dynamics = model
        .kernel_choices
        .iter()
        .position(|c| c.contract == ExecutionContractId::Dynamics)
        .unwrap();
    let field_id = old
        .declarations()
        .descriptor()
        .kernel_instances
        .iter()
        .find(|k| k.contribution == model.kernel_choices[field].contribution)
        .unwrap()
        .instance_id
        .clone();
    act(&mut model, PhysicsAction::LoadCaptured);
    parameter(&mut model, field, "exclusion-radius", "4 mm");
    act(&mut model, PhysicsAction::ApplyField(field));
    assert!(
        !model.scientific_effects.is_pending(),
        "field-only consent is required"
    );
    act(&mut model, PhysicsAction::Confirm(true));
    act(&mut model, PhysicsAction::ApplyField(field));
    assert!(
        !model.scientific_effects.is_pending(),
        "whole-setup consent is not field consent"
    );

    // A field-only edit cannot silently discard unrelated draft edits.
    for edit in [
        PhysicsAction::Step("0.02".into()),
        PhysicsAction::Upper(0, "20".into()),
        PhysicsAction::Grid(true),
    ] {
        act(&mut model, PhysicsAction::LoadCaptured);
        act(&mut model, edit);
        act(&mut model, PhysicsAction::ConfirmField(field, true));
        assert!(
            model
                .physics_form
                .field_request(&model.document, &model.kernel_choices, revision, field)
                .unwrap_err()
                .contains("Domain/timestep")
        );
    }
    act(&mut model, PhysicsAction::LoadCaptured);
    parameter(&mut model, dynamics, "capacity", "32");
    act(&mut model, PhysicsAction::ConfirmField(field, true));
    assert!(
        model
            .physics_form
            .field_request(&model.document, &model.kernel_choices, revision, field)
            .unwrap_err()
            .contains("Other models")
    );
    act(&mut model, PhysicsAction::ConfirmField(dynamics, true));
    assert!(
        model
            .physics_form
            .field_request(&model.document, &model.kernel_choices, revision, dynamics)
            .unwrap_err()
            .contains("Only a captured field")
    );
    act(&mut model, PhysicsAction::LoadCaptured);
    parameter(&mut model, field, "exclusion-radius", "2 kg");
    act(&mut model, PhysicsAction::ConfirmField(field, true));
    act(&mut model, PhysicsAction::ApplyField(field));
    assert!(model.scientific_effects.is_pending());
    poll(&mut model);
    assert!(model.document.notice.is_some());
    assert_eq!(model.document.snapshot(), &before);

    parameter(&mut model, field, "exclusion-radius", "4 mm");
    assert!(!model.physics_form.field_confirmed(field));
    act(&mut model, PhysicsAction::ConfirmField(field, true));
    assert!(
        model
            .physics_form
            .field_request(&model.document, &model.kernel_choices, revision + 1, field)
            .is_err()
    );
    act(&mut model, PhysicsAction::ApplyField(field));
    assert!(model.scientific_effects.is_pending());
    assert!(!model.physics_form.field_confirmed(field));
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let after = model.document.snapshot().clone();
    let new = after.setup().scientific().unwrap();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    assert_eq!(
        new.declarations().descriptor(),
        old.declarations().descriptor()
    );
    assert_eq!(new.domain(), old.domain());
    assert_eq!(new.time_step(), old.time_step());
    for (id, capture) in old.captures() {
        if *id == field_id {
            assert_ne!(capture.configuration, new.captures()[id].configuration);
            assert!(
                !Arc::ptr_eq(&capture.state, &new.captures()[id].state),
                "the selected field was initialized anew"
            );
        } else {
            assert_eq!(capture, &new.captures()[id]);
            assert!(
                Arc::ptr_eq(&capture.state, &new.captures()[id].state),
                "other fields/history retain the same allocation"
            );
        }
    }
    let resolved = ResolvedConfiguration::from_cbor(
        &new.captures()[&field_id].configuration,
        &Default::default(),
    )
    .unwrap();
    assert!(
        matches!(resolved.get("exclusion-radius"), Some(ConfigurationValue::Quantity { value_si, dimension }) if value_si.get() == 0.004 && *dimension == orishu_variables::Dimension::LENGTH)
    );
    // Old copied inputs cannot overwrite the newly accepted capture.
    act(&mut model, PhysicsAction::ConfirmField(field, true));
    assert!(
        model
            .physics_form
            .field_request(&model.document, &model.kernel_choices, revision, field)
            .unwrap_err()
            .contains("context changed")
    );
    assert!(model.document.submit(kagami_session::SessionCommand::Undo));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(old));
    assert!(model.document.submit(kagami_session::SessionCommand::Redo));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(new));

    // The effect API itself enforces input bounds, target kind and source guard.
    let guard = model.document.authoring_guard().unwrap();
    let history = new
        .captures()
        .iter()
        .find(|(_, c)| c.context.execution_contract == ExecutionContractId::Dynamics)
        .unwrap()
        .0
        .clone();
    assert!(matches!(
        model
            .scientific_effects
            .configure_field(&model.document, guard, history, vec![]),
        Err(EffectError::Unavailable)
    ));
    let oversized = AuthoredConfigurationProperty {
        id: "exclusion-radius".parse().unwrap(),
        input: ConfigurationInput::Expression {
            source: "0".repeat(model.document.limits().max_expression_bytes + 1),
        },
    };
    assert!(matches!(
        model.scientific_effects.configure_field(
            &model.document,
            guard,
            field_id.clone(),
            vec![oversized]
        ),
        Err(EffectError::Limit)
    ));
    assert!(!model.scientific_effects.is_pending());
    act(&mut model, PhysicsAction::LoadCaptured);
    parameter(&mut model, field, "exclusion-radius", "8 mm");
    act(&mut model, PhysicsAction::ConfirmField(field, true));
    act(&mut model, PhysicsAction::ApplyField(field));
    assert!(model.scientific_effects.is_pending());
    model.scientific_effects.cancel();
    poll(&mut model);
    assert_eq!(model.document.snapshot().setup().scientific(), Some(new));

    // Persist and export the new capture, not a recipe that reruns initialization.
    let path = installed._dir.path().join("field-edit.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    assert!(model.document.open(path.clone(), true));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(new));
    assert!(
        matches!(
            model.scientific_effects.configure_field(
                &model.document,
                guard,
                field_id.clone(),
                vec![]
            ),
            Err(EffectError::Document)
        ),
        "reopening the same file changes the document incarnation"
    );
    let output = installed._dir.path().join("field-edit.orishu");
    kagami::export::execute(kagami::export::ExportArgs {
        experiment: path,
        output: output.clone(),
        name: "field-edit".parse().unwrap(),
        directory: Some(installed._dir.path().to_path_buf()),
        expected_inventory_revision: Some(revision),
        enable_plugin: vec![],
        disable_plugin: vec![],
        json: false,
    })
    .unwrap();
    let bytes = std::fs::read(output).unwrap();
    let bundle =
        orishu_plugin::workload::bundle::read(&bytes, Default::default(), Default::default())
            .unwrap();
    drop(model);
    drop(installed);
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        bundle.manifest_bytes(),
        bundle.blobs(),
        orishu_runtime::RunScope {
            workload: bundle.verified().root(),
            run: ArtifactDigest::sha256_of(b"targeted field parameter test"),
            epoch: 1,
        },
        &Default::default(),
        Default::default(),
        control(),
    )
    .unwrap();
    admitted.run.advance(control()).unwrap();
}
