//! Explicit provider replacement through fresh initialization and atomic adoption.
use super::*;
use kagami::scientific_effect::{ComponentResetRequest, EffectError, ScientificEffects};
use kagami_catalog::ComponentTypeId;

struct Replacement {
    source: Fixture,
    request: ComponentResetRequest,
    kinds: Vec<ComponentTypeId>,
}

fn replacement() -> Replacement {
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

    // Identical scientific contracts from independently identified providers.
    // Both OLD packages are disabled: reset must not prepare their executables
    // or fall back to their declarations to initialize the new setup.
    let vocabulary = package(
        "org.test.replacement.vocabulary",
        declarations::vocabulary(),
        &[],
    );
    let solvers = package(
        "org.test.replacement.solvers",
        vec![
            (
                "newtonian".parse().unwrap(),
                declarations::newtonian(ArtifactDigest::sha256_of(GRAVITY)),
            ),
            (
                "euler".parse().unwrap(),
                declarations::euler(ArtifactDigest::sha256_of(EULER)),
            ),
        ],
        &[GRAVITY, EULER],
    );
    let store = &source.installed.store;
    let mut revision = store
        .install(source.plugins.revision, &vocabulary, None, None)
        .unwrap();
    revision = store.install(revision, &solvers, None, None).unwrap();
    for id in [
        "org.orishu.reference.vocabulary",
        "org.orishu.reference.solvers",
    ] {
        revision = store
            .submit(
                revision,
                InventoryCommand::SetEnabled {
                    plugin_id: id.parse().unwrap(),
                    enabled: false,
                },
            )
            .unwrap();
    }
    source.plugins.revision = revision;
    source.model.scientific_effects = ScientificEffects::new(Some(source.plugins.clone()));
    let kernels: Vec<_> = source
        .installed
        .uses
        .iter()
        .map(|old| SelectedKernel {
            contribution: solvers
                .release()
                .contribution_ref(&old.contribution.local_id)
                .unwrap(),
            ..old.clone()
        })
        .collect();
    let kinds: Vec<_> = ["dynamics", "mass"]
        .into_iter()
        .map(|id| {
            ComponentTypeId::exact(
                vocabulary
                    .release()
                    .contribution_ref(&id.parse().unwrap())
                    .unwrap(),
            )
            .unwrap()
        })
        .collect();
    let schemas = store.available_components(&[]).unwrap().schemas;
    let mut edits = vec![];
    for (object, value) in source.model.document.snapshot().objects() {
        for old in value.components.keys().filter(|c| **c != source.tag) {
            edits.push(Edit::DetachComponent {
                object: *object,
                component: old.clone(),
            });
        }
        for kind in &kinds {
            let mut properties = component_defaults(schemas.get(kind).unwrap());
            // Explicitly requested values, not copied from the previous provider.
            if kind.contribution().unwrap().local_id.as_str() == "dynamics" {
                properties.insert(
                    "inertial_mass".try_into().unwrap(),
                    kagami_document::AuthoredValue::si("2 kg"),
                );
            }
            edits.push(Edit::AttachComponent {
                object: *object,
                component: kind.clone(),
                properties,
            });
        }
    }
    let mut roots: Vec<_> = kinds
        .iter()
        .chain(std::iter::once(&source.tag))
        .map(|k| k.contribution().unwrap().clone())
        .collect();
    roots.sort();
    let resolve = |roots| {
        let ResolutionOutcome::Resolved { selection, .. } = store
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
            panic!("only replacement providers enabled")
        };
        selection
    };
    let lock = Arc::new(
        SelectionLock::new(
            resolve(roots.clone()),
            source.model.document.limits().dependencies,
        )
        .unwrap(),
    );
    roots.extend(kernels.iter().map(|k| k.contribution.clone()));
    roots.sort();
    let selection = resolve(roots);
    let mut field = request("newtonian");
    field.configuration = vec![AuthoredConfigurationProperty {
        id: "exclusion-radius".parse().unwrap(),
        input: ConfigurationInput::Expression {
            source: "4 mm".into(),
        },
    }];
    let request = ComponentResetRequest {
        edits,
        dependencies: lock,
        setup: SetupRequest {
            selection: ResolutionRequest {
                expected_inventory_revision: revision,
                roots: selection.roots,
                bindings: selection.bindings,
            },
            kernels,
            initialization: vec![request("euler"), field],
            time_step: kagami_document::TimeStep::new(0.02).unwrap(),
        },
    };
    Replacement {
        source,
        request,
        kinds,
    }
}

fn check_core_preparation(source: &mut Fixture, request: &ComponentResetRequest) {
    let path = source.installed._dir.path().join("reset-core.kagami");
    assert!(
        source
            .model
            .document
            .save(Some(path.clone()), "test".into())
    );
    let loaded = kagami_session::store::load(
        &kagami_session::store::RealFileStore,
        &kagami_session::DocumentTarget::new(path).unwrap(),
    )
    .unwrap();
    let experiment = loaded
        .document
        .into_experiment(
            source.model.document.schemas(),
            source.model.document.limits(),
        )
        .unwrap();
    let before = experiment.snapshot();
    let PrepareSelectionOutcome::Ready(selected) = source
        .installed
        .store
        .prepare_selection(
            &request.setup.selection,
            &[],
            &request.setup.kernels,
            Default::default(),
        )
        .unwrap()
    else {
        panic!("exact replacement selection")
    };
    let mut schemas = source.model.document.schemas().clone();
    for schema in source
        .installed
        .store
        .available_components(&[])
        .unwrap()
        .schemas
        .schemas()
    {
        schemas.insert(schema.clone());
    }
    let mut commands = request.edits.clone();
    commands.push(Edit::AdoptDependencies(request.dependencies.clone()));
    // Normal command acceptance must still reject the edits with the old capture.
    assert!(
        kagami_document::update::update(
            &experiment,
            &commands,
            &schemas,
            source.model.document.limits(),
        )
        .is_err()
    );
    let prepare = |commands: &[Edit]| {
        kagami_document::update::prepare_scientific_reset(
            &experiment,
            commands,
            &schemas,
            selected.compiled().verified(),
            source.model.document.limits(),
        )
    };
    let proposal = prepare(&commands).unwrap();
    let packet = proposal
        .initial_dynamics(&"euler".parse().unwrap(), BulkLimits::default())
        .unwrap();
    let values = Batch::<DynamicEntity>::read(&packet, BulkLimits::default()).unwrap();
    assert_eq!(values.len(), 2);
    assert!(
        values
            .iter()
            .all(|e| e.inertial_mass_kilograms.get() == 2.0)
    );
    assert!(
        proposal
            .initial_dynamics(
                &"euler".parse().unwrap(),
                BulkLimits {
                    bytes: 1,
                    records: 2
                }
            )
            .is_err()
    );
    commands.push(Edit::AdoptScientificSetup(Arc::new(
        before.setup().scientific().unwrap().clone(),
    )));
    assert!(
        prepare(&commands).is_err(),
        "preparation cannot smuggle setup commands"
    );
    assert_eq!(experiment.snapshot(), before);
}

#[test]
fn replacement_resets_every_capture_atomically_and_runs_without_old_providers() {
    let Replacement {
        mut source,
        request,
        kinds,
    } = replacement();
    check_core_preparation(&mut source, &request);
    let before = source.model.document.snapshot().clone();
    let old = before.setup().scientific().unwrap();
    let lock = request.dependencies.clone();
    assert!(
        kinds
            .iter()
            .all(|k| source.model.document.schemas().get(k).is_none())
    );
    source
        .model
        .scientific_effects
        .reset_components(
            &source.model.document,
            source.model.document.authoring_guard().unwrap(),
            request,
        )
        .unwrap();
    assert_eq!(source.model.document.snapshot(), &before);
    assert!(
        kinds
            .iter()
            .all(|k| source.model.document.schemas().get(k).is_none())
    );
    poll(&mut source.model);
    assert!(
        source.model.document.notice.is_none(),
        "{:?}",
        source.model.document.notice
    );
    let after = source.model.document.snapshot().clone();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    assert_eq!(after.dependencies(), Some(&lock));
    let new = after.setup().scientific().unwrap();
    assert_eq!(new.time_step().seconds(), 0.02);
    for (id, capture) in new.captures() {
        assert!(!Arc::ptr_eq(&capture.state, &old.captures()[id].state));
        if let Some(bytes) = &capture.history_entities {
            let batch = Batch::<DynamicEntity>::read(bytes, BulkLimits::default()).unwrap();
            assert_eq!(batch.len(), 2);
            assert!(batch.iter().all(|e| e.inertial_mass_kilograms.get() == 2.0));
        }
    }
    assert!(
        new.declarations()
            .descriptor()
            .contributions
            .iter()
            .all(|c| c.release != source.installed.vocabulary)
    );
    assert!(
        kinds
            .iter()
            .all(|k| source.model.document.schemas().get(k).is_some())
    );
    assert!(
        source
            .model
            .document
            .submit(kagami_session::SessionCommand::Undo)
    );
    assert_eq!(source.model.document.snapshot().objects(), before.objects());
    assert_eq!(source.model.document.snapshot().setup(), before.setup());
    assert_eq!(
        source.model.document.snapshot().dependencies(),
        before.dependencies()
    );
    assert!(
        source
            .model
            .document
            .submit(kagami_session::SessionCommand::Redo)
    );
    assert_eq!(source.model.document.snapshot().setup(), after.setup());
    let path = source.installed._dir.path().join("replacement.kagami");
    assert!(
        source
            .model
            .document
            .save(Some(path.clone()), "test".into())
    );
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    assert_eq!(offline.document.snapshot().setup(), after.setup());
    assert_eq!(offline.document.snapshot().dependencies(), Some(&lock));
    let exported = kagami::export::compile_snapshot(
        offline.document.snapshot(),
        &source.plugins,
        "replacement".parse().unwrap(),
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
    drop(offline);
    drop(source);
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        portable.manifest_bytes(),
        portable.blobs(),
        orishu_runtime::RunScope {
            workload: portable.verified().root(),
            run: ArtifactDigest::sha256_of(b"replacement"),
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
fn rejected_cancelled_and_stale_resets_never_adopt_partial_inputs_or_schemas() {
    let Replacement {
        mut source,
        request,
        kinds,
    } = replacement();
    for case in [
        "invalid_values",
        "old_lock",
        "missing_edits",
        "mismatched_selection",
        "late_initialization",
        "cancel",
        "stale",
    ] {
        let mut proposed = request.clone();
        match case {
            "invalid_values" => {
                let Edit::AttachComponent { properties, .. } = proposed
                    .edits
                    .iter_mut()
                    .find(|c| matches!(c, Edit::AttachComponent { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                properties.insert(
                    "inertial_mass".try_into().unwrap(),
                    kagami_document::AuthoredValue::si("1 m"),
                );
            }
            "old_lock" => {
                proposed.dependencies = source
                    .model
                    .document
                    .snapshot()
                    .dependencies()
                    .unwrap()
                    .clone()
            }
            "missing_edits" => proposed.edits.clear(),
            "mismatched_selection" => {
                let tag = source.tag.contribution().unwrap();
                proposed.setup.selection.roots.retain(|r| r != tag);
                proposed
                    .setup
                    .selection
                    .bindings
                    .retain(|b| &b.requirement.consumer != tag);
            }
            "late_initialization" => {
                proposed.setup.initialization[1].configuration[0].input =
                    ConfigurationInput::Expression {
                        source: "-4 mm".into(),
                    }
            }
            _ => {}
        }
        source
            .model
            .scientific_effects
            .reset_components(
                &source.model.document,
                source.model.document.authoring_guard().unwrap(),
                proposed,
            )
            .unwrap();
        if case == "cancel" {
            source.model.scientific_effects.cancel();
        }
        if case == "stale" {
            let object = *source
                .model
                .document
                .snapshot()
                .objects()
                .keys()
                .next()
                .unwrap();
            assert!(source.model.document.edit(vec![Edit::RenameObject {
                object,
                name: DisplayName::new("intervening").unwrap()
            }]));
        }
        let before = source.model.document.snapshot().clone();
        poll(&mut source.model);
        assert_eq!(source.model.document.snapshot(), &before, "{case}");
        assert!(
            kinds
                .iter()
                .all(|k| source.model.document.schemas().get(k).is_none()),
            "{case}"
        );
        assert!(source.model.document.notice.is_some(), "{case}");
        if case == "invalid_values" {
            assert!(
                source
                    .model
                    .document
                    .notice
                    .as_ref()
                    .unwrap()
                    .contains("expression resolves"),
                "{:?}",
                source.model.document.notice
            );
        }
        if case == "late_initialization" {
            assert!(
                source
                    .model
                    .document
                    .notice
                    .as_ref()
                    .unwrap()
                    .contains("kernel rejected invalid input"),
                "{:?}",
                source.model.document.notice
            );
        }
    }
    let mut excess = request.clone();
    excess.edits =
        vec![request.edits[0].clone(); source.model.document.limits().max_commands_per_batch - 1];
    assert!(matches!(
        source.model.scientific_effects.reset_components(
            &source.model.document,
            source.model.document.authoring_guard().unwrap(),
            excess,
        ),
        Err(EffectError::Limit)
    ));
    let mut forbidden = request.clone();
    forbidden
        .edits
        .push(Edit::AdoptDependencies(request.dependencies.clone()));
    assert!(
        source
            .model
            .scientific_effects
            .reset_components(
                &source.model.document,
                source.model.document.authoring_guard().unwrap(),
                forbidden,
            )
            .is_err()
    );
    assert!(!source.model.scientific_effects.is_pending());
}
