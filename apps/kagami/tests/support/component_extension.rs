//! Captured graph extension through independently verified declarations and real history.
use super::*;
use kagami::{
    launch::LaunchOptions,
    message::{Message, ScientificAction},
    model::Model,
    scientific_effect::{ScientificPlugins, SetupRequest},
    update::update,
};
use kagami_document::{DisplayName, ExperimentCommand as Edit, ObjectSpec, Transform, Vector3};
use orishu_plugin::authoring_lock::SelectionLock;

#[path = "catalog_extension.rs"]
mod catalog;
#[path = "component_extension_native.rs"]
mod native;
#[path = "component_reset.rs"]
mod reset;
#[path = "component_reset_native.rs"]
mod reset_native;

/// Drive consent and dependency preparation, stopping at the scientific handoff.
pub(super) fn begin_native_addition(model: &mut Model) {
    native::confirm(model, true);
    native::dependency(model, kagami::physics_form::dependencies::Action::Check);
    let report = model
        .component_dependencies
        .report()
        .expect("resolved choices")
        .token();
    native::dependency(
        model,
        kagami::physics_form::dependencies::Action::ApplyLock { report },
    );
    assert!(
        model.scientific_effects.is_pending(),
        "{} {:?}",
        model.component_dependencies.notice,
        model.document.notice
    );
}

fn poll(model: &mut Model) {
    let until = std::time::Instant::now() + Duration::from_secs(75);
    while model.scientific_effects.is_pending() {
        let _ = update(model, Message::Scientific(ScientificAction::Poll));
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Fixture {
    installed: Installed,
    plugins: ScientificPlugins,
    model: Model,
    lock: Arc<SelectionLock>,
    attachments: Vec<Edit>,
    tag: kagami_catalog::ComponentTypeId,
}
fn fixture() -> Fixture {
    let installed = Installed::new();
    let schemas = installed.store.available_components(&[]).unwrap().schemas;
    let dynamics = declarations::vocabulary()
        .into_iter()
        .find(|(id, _)| id.as_str() == "dynamics")
        .unwrap()
        .1;
    let tag = package(
        "org.test.extension",
        vec![(
            "tag".parse().unwrap(),
            Payload::Components(Declaration {
                scientific: ComponentSchema {
                    name: "org.test.extension.tag".parse().unwrap(),
                    version: 1.try_into().unwrap(),
                    requirements: vec![ContractRequirement {
                        slot: "dynamics".parse().unwrap(),
                        contract: dynamics.contract_ref(&Limits::default()).unwrap(),
                    }],
                    properties: vec![Property {
                        id: "visible".parse().unwrap(),
                        required: true,
                        schema: PropertyType::Boolean {
                            default: Some(false),
                        },
                    }],
                    role: ComponentRole::Data,
                    bindings: RoleBindings::default(),
                },
                presentation: None,
            }),
        )],
        &[],
    );
    let revision = installed
        .store
        .install(
            installed.request.expected_inventory_revision,
            &tag,
            None,
            None,
        )
        .unwrap();
    let tag = kagami_catalog::ComponentTypeId::exact(
        tag.release()
            .contribution_ref(&"tag".parse().unwrap())
            .unwrap(),
    )
    .unwrap();
    let plugins = ScientificPlugins {
        store: Arc::new(PluginStore::open(installed._dir.path()).unwrap()),
        revision,
        overrides: vec![],
    };
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: schemas,
        scientific_plugins: Some(plugins.clone()),
        ..Default::default()
    });
    for x in [-1.0, 1.0] {
        assert!(model.document.edit(vec![Edit::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("body").unwrap())
                    .with_transform(Transform::at(Vector3::new(x, 0.0, 0.0).unwrap()),)
            ))]));
    }
    let mut selection = installed.request.clone();
    selection.expected_inventory_revision = revision;
    model
        .scientific_effects
        .configure(
            &model.document,
            SetupRequest {
                selection,
                kernels: installed.uses.clone(),
                initialization: vec![request("euler"), request("newtonian")],
                time_step: kagami_document::TimeStep::new(0.01).unwrap(),
            },
        )
        .unwrap();
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    assert!(
        model.document.schemas().get(&tag).is_none(),
        "test starts without the extension's capability"
    );
    let components = installed.store.available_components(&[]).unwrap();
    let kinds: Vec<_> = ["dynamics", "mass"]
        .into_iter()
        .map(|id| {
            kagami_catalog::ComponentTypeId::exact(ContributionRef {
                release: installed.vocabulary,
                extension_point: "orishu.model.components/v1".parse().unwrap(),
                local_id: id.parse().unwrap(),
            })
            .unwrap()
        })
        .chain(std::iter::once(tag.clone()))
        .collect();
    let mut roots: Vec<_> = kinds
        .iter()
        .map(|c| c.contribution().unwrap().clone())
        .collect();
    roots.sort();
    let ResolutionOutcome::Resolved { selection, .. } = installed
        .store
        .resolve(
            &ResolutionRequest {
                expected_inventory_revision: revision,
                roots,
                bindings: vec![],
            },
            &[],
        )
        .unwrap()
    else {
        panic!("unique extension")
    };
    let lock =
        Arc::new(SelectionLock::new(selection, model.document.limits().dependencies).unwrap());
    let attachments = model
        .document
        .snapshot()
        .objects()
        .keys()
        .flat_map(|object| {
            kinds.iter().map(|kind| Edit::AttachComponent {
                object: *object,
                component: kind.clone(),
                properties: component_defaults(components.schemas.get(kind).unwrap()),
            })
        })
        .collect();
    Fixture {
        installed,
        plugins,
        model,
        lock,
        attachments,
        tag,
    }
}

#[test]
fn captured_addition_preserves_fields_and_executes_with_extended_evidence_without_inventory() {
    let Fixture {
        installed,
        plugins,
        mut model,
        lock,
        attachments,
        tag,
    } = fixture();
    let before = model.document.snapshot().clone();
    let old = before.setup().scientific().unwrap();
    assert!(
        !old.declarations()
            .descriptor()
            .contributions
            .contains(tag.contribution().unwrap())
    );
    model
        .scientific_effects
        .extend_components(
            &model.document,
            model.document.authoring_guard().unwrap(),
            attachments,
            lock.clone(),
        )
        .unwrap();
    assert_eq!(model.document.snapshot(), &before);
    assert!(model.document.schemas().get(&tag).is_none());
    poll(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let after = model.document.snapshot().clone();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    assert_eq!(after.dependencies(), Some(&lock));
    assert!(model.document.schemas().get(&tag).is_some());
    let new = after.setup().scientific().unwrap();
    assert!(
        new.declarations()
            .descriptor()
            .contributions
            .contains(tag.contribution().unwrap())
    );
    assert_eq!(
        new.declarations().descriptor().kernel_instances,
        old.declarations().descriptor().kernel_instances
    );
    for (id, capture) in old.captures() {
        if capture.context.execution_contract == ExecutionContractId::Field {
            assert_eq!(new.captures()[id], *capture);
            assert!(Arc::ptr_eq(&new.captures()[id].state, &capture.state));
        } else {
            let bytes = new.captures()[id].history_entities.as_ref().unwrap();
            assert_eq!(
                Batch::<DynamicEntity>::read(bytes, BulkLimits::default())
                    .unwrap()
                    .len(),
                2
            );
        }
    }
    assert!(model.document.submit(kagami_session::SessionCommand::Undo));
    assert_eq!(model.document.snapshot().objects(), before.objects());
    assert_eq!(model.document.snapshot().setup(), before.setup());
    assert_eq!(
        model.document.snapshot().dependencies(),
        before.dependencies()
    );
    assert!(model.document.submit(kagami_session::SessionCommand::Redo));
    assert_eq!(model.document.snapshot().setup(), after.setup());
    let path = installed._dir.path().join("extension.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
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
        &plugins,
        "extension".parse().unwrap(),
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
    drop(model);
    drop(offline);
    drop(plugins);
    drop(installed);
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        portable.manifest_bytes(),
        portable.blobs(),
        orishu_runtime::RunScope {
            workload: portable.verified().root(),
            run: ArtifactDigest::sha256_of(b"captured extension"),
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
fn stale_cancelled_invalid_and_over_budget_extensions_leave_source_and_capabilities_unchanged() {
    let Fixture {
        mut model,
        lock,
        attachments,
        tag,
        installed: _installed,
        plugins: _plugins,
    } = fixture();
    for case in ["invalid", "cancel", "stale"] {
        let mut commands = attachments.clone();
        if case == "invalid" {
            let Edit::AttachComponent { properties, .. } = &mut commands[0] else {
                unreachable!()
            };
            properties.insert(
                "inertial_mass".try_into().unwrap(),
                kagami_document::AuthoredValue::si("1 m"),
            );
        }
        model
            .scientific_effects
            .extend_components(
                &model.document,
                model.document.authoring_guard().unwrap(),
                commands,
                lock.clone(),
            )
            .unwrap();
        if case == "cancel" {
            model.scientific_effects.cancel();
        }
        if case == "stale" {
            let object = *model.document.snapshot().objects().keys().next().unwrap();
            assert!(model.document.edit(vec![Edit::RenameObject {
                object,
                name: DisplayName::new("intervening").unwrap()
            }]));
        }
        let before = model.document.snapshot().clone();
        poll(&mut model);
        assert_eq!(model.document.snapshot(), &before);
        assert!(model.document.schemas().get(&tag).is_none());
        assert!(model.document.notice.is_some());
        if case == "invalid" {
            assert!(
                model
                    .document
                    .notice
                    .as_ref()
                    .unwrap()
                    .contains("expression resolves"),
                "{:?}",
                model.document.notice
            );
        }
    }
    let guard = model.document.authoring_guard().unwrap();
    let too_many = vec![attachments[0].clone(); model.document.limits().max_commands_per_batch - 1];
    assert!(matches!(
        model
            .scientific_effects
            .extend_components(&model.document, guard, too_many, lock.clone()),
        Err(kagami::scientific_effect::EffectError::Limit)
    ));
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    assert!(
        model
            .scientific_effects
            .extend_components(
                &model.document,
                guard,
                vec![Edit::RemoveObject(object)],
                lock
            )
            .is_err()
    );
    assert!(!model.scientific_effects.is_pending());
}

#[test]
fn core_extension_refuses_changed_kernel_uses_and_rebinding_even_valid_new_declarations() {
    let Fixture {
        installed,
        mut model,
        plugins,
        ..
    } = fixture();
    let path = installed._dir.path().join("core-extension.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    let loaded = kagami_session::store::load(
        &kagami_session::store::RealFileStore,
        &kagami_session::DocumentTarget::new(path).unwrap(),
    )
    .unwrap();
    let experiment = loaded
        .document
        .into_experiment(model.document.schemas(), model.document.limits())
        .unwrap();
    let snapshot = experiment.snapshot();
    let previous = snapshot
        .setup()
        .scientific()
        .unwrap()
        .declarations()
        .descriptor();
    let object = *snapshot.objects().keys().next().unwrap();
    let commands = [Edit::RenameObject {
        object,
        name: DisplayName::new("valid ordinary edit").unwrap(),
    }];
    let request = ResolutionRequest {
        expected_inventory_revision: plugins.revision,
        roots: previous.roots.clone(),
        bindings: previous
            .bindings
            .iter()
            .map(|b| ProviderBinding {
                requirement: RequirementKey {
                    consumer: b.consumer.clone(),
                    slot: b.requirement_slot.clone(),
                },
                provider: b.provider.clone(),
            })
            .collect(),
    };
    let prepare = |request: &ResolutionRequest, kernels: &[SelectedKernel]| {
        let PrepareSelectionOutcome::Ready(p) = installed
            .store
            .prepare_selection(request, &[], kernels, Default::default())
            .unwrap()
        else {
            panic!("exact selection")
        };
        p
    };
    let same = prepare(&request, &previous.kernel_instances);
    assert!(
        kagami_document::update::prepare_extended_history_edit(
            &experiment,
            &commands,
            model.document.schemas(),
            same.compiled().verified(),
            model.document.limits()
        )
        .is_ok()
    );
    let mut changed_kernels = previous.kernel_instances.clone();
    changed_kernels[0].instance_id = "another-instance".parse().unwrap();
    changed_kernels.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    let changed = prepare(&request, &changed_kernels);
    assert!(matches!(
        kagami_document::update::prepare_extended_history_edit(
            &experiment,
            &commands,
            model.document.schemas(),
            changed.compiled().verified(),
            model.document.limits()
        ),
        Err(kagami_document::Rejection::Scientific(
            kagami_document::scientific::ScientificError::Mismatch
        ))
    ));

    let alternative = package(
        "org.test.alternative-vocabulary",
        declarations::vocabulary(),
        &[UNUSED],
    );
    let revision = installed
        .store
        .install(plugins.revision, &alternative, None, None)
        .unwrap();
    let mut rebound = request.clone();
    rebound.expected_inventory_revision = revision;
    // Retain every old member as a root, so refusal specifically tests changed
    // bindings, not merely absence of the old provider from the new closure.
    rebound.roots = previous.contributions.clone();
    for b in &mut rebound.bindings {
        if b.requirement.consumer.local_id.as_str() == "euler" {
            b.provider.release = alternative.release().id();
        }
    }
    let changed = prepare(&rebound, &previous.kernel_instances);
    assert!(previous.contributions.iter().all(|c| {
        changed
            .compiled()
            .verified()
            .descriptor()
            .contributions
            .contains(c)
    }));
    assert!(matches!(
        kagami_document::update::prepare_extended_history_edit(
            &experiment,
            &commands,
            model.document.schemas(),
            changed.compiled().verified(),
            model.document.limits()
        ),
        Err(kagami_document::Rejection::Scientific(
            kagami_document::scientific::ScientificError::Mismatch
        ))
    ));
    assert_eq!(model.document.snapshot().setup(), snapshot.setup());
}
