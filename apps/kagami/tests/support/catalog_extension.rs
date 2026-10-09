//! Exact catalog snapshot -> composed objects/definitions -> real captured run.
use super::*;
use kagami_catalog::{
    CatalogSet, ContentFingerprint, Template, TemplateIdentity, document as wire,
};
use kagami_session::InstantiationSpec;

#[path = "catalog_native.rs"]
mod native;

fn identity() -> TemplateIdentity {
    TemplateIdentity::new("bodies".try_into().unwrap(), "particle".try_into().unwrap())
}
fn catalog(f: &Fixture) -> CatalogSet {
    let components = f
        .lock
        .selection()
        .roots
        .iter()
        .map(|pin| {
            let names: &[&str] = match pin.local_id.as_str() {
                "dynamics" => &["inertial_mass"],
                "mass" => &["source", "response"],
                "tag" => &["visible"],
                _ => unreachable!(),
            };
            wire::ComponentDocument {
                component_type: kagami_catalog::ComponentTypeId::exact(pin.clone()).unwrap(),
                name: Some(pin.local_id.as_str().try_into().unwrap()),
                properties: names
                    .iter()
                    .map(|name| {
                        (
                            name.to_owned().try_into().unwrap(),
                            if *name == "visible" {
                                wire::PropertyValueDocument::Boolean(true)
                            } else {
                                wire::PropertyValueDocument::Quantity(
                                    wire::QuantityDocument::canonical("bodies.particle.amount"),
                                )
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect();
    let doc = wire::new(
        wire::MetadataDocument::new("bodies".try_into().unwrap(), "particle".try_into().unwrap()),
        wire::SpecDocument {
            dependencies: Some(f.lock.clone()),
            components,
            parameters: [(
                "amount".try_into().unwrap(),
                wire::ParameterDocument {
                    default: "1".into(),
                    unit: Some("kg".into()),
                    description: None,
                },
            )]
            .into(),
            ..Default::default()
        },
    );
    let template = Template::from_document(&doc, &Default::default()).unwrap();
    let bytes = template.canonical_bytes();
    let parsed = kagami_catalog::parse_stream(
        std::path::Path::new("particle.yaml"),
        std::str::from_utf8(&bytes).unwrap(),
        &Default::default(),
    );
    let schemas = f.installed.store.available_components(&[]).unwrap().schemas;
    let catalog = kagami_catalog::resolve(parsed.documents, vec![], &schemas, &Default::default());
    assert!(
        catalog.get(&identity()).unwrap().result.is_available(),
        "{:?}",
        catalog.entries()
    );
    catalog
}
fn install_catalog(f: &mut Fixture) -> InstantiationSpec {
    let catalog = catalog(f);
    let fingerprint = catalog.get(&identity()).unwrap().fingerprint.unwrap();
    f.model.document.adopt_catalog(catalog);
    InstantiationSpec::new(identity(), DisplayName::new("catalog body").unwrap())
        .expecting(fingerprint)
}
fn start(f: &mut Fixture, spec: InstantiationSpec) {
    f.model
        .scientific_effects
        .instantiate_template(
            &f.model.document,
            f.model.document.authoring_guard().unwrap(),
            spec,
            f.lock.clone(),
        )
        .unwrap();
}

#[test]
fn captured_catalog_additions_preserve_fields_scopes_and_self_contained_execution() {
    let mut f = fixture();
    assert!(
        f.model
            .document
            .edit(vec![Edit::CreateObject(Box::new(ObjectSpec::new(
                DisplayName::new("discarded").unwrap()
            )))])
    );
    let discarded = *f
        .model
        .document
        .snapshot()
        .objects()
        .keys()
        .next_back()
        .unwrap();
    assert!(f.model.document.edit(vec![Edit::RemoveObject(discarded)]));
    let before = f.model.document.snapshot().clone();
    let spec = install_catalog(&mut f).binding("amount".try_into().unwrap(), "2");
    let fingerprint = spec.expected_fingerprint.unwrap();
    for (index, x) in [-2.0, 2.0].into_iter().enumerate() {
        let input = if index == 0 {
            spec.clone()
        } else {
            install_catalog(&mut f).binding("amount".try_into().unwrap(), "3")
        };
        let prior = f.model.document.snapshot().clone();
        start(
            &mut f,
            input.at(Transform::at(Vector3::new(x, 0.0, 0.0).unwrap())),
        );
        assert_eq!(f.model.document.snapshot(), &prior);
        if index == 0 {
            assert!(f.model.document.schemas().get(&f.tag).is_none());
        }
        // This effect owns the exact accepted source snapshot, not a live link.
        f.model.document.adopt_catalog(CatalogSet::default());
        poll(&mut f.model);
        assert!(
            f.model.document.notice.is_none(),
            "{:?}",
            f.model.document.notice
        );
        assert_eq!(
            f.model.document.snapshot().revision().get(),
            prior.revision().get() + 1
        );
    }
    let after = f.model.document.snapshot().clone();
    let created: Vec<_> = after
        .objects()
        .iter()
        .filter(|(id, _)| id.get() > discarded.get())
        .collect();
    assert_eq!(created.len(), 2);
    assert!(created[0].0.get() > discarded.get());
    for (id, object) in &created {
        assert_eq!(object.provenance.as_ref().unwrap().fingerprint, fingerprint);
        let namespace = format!("objects.object_{}", id.get());
        assert!(
            after
                .variables()
                .values()
                .any(|v| v.namespace.as_str() == namespace)
        );
        assert!(object.components.values().flat_map(|c| c.properties.values()).any(|p| {
            matches!(p.authored(), kagami_document::AuthoredValue::Quantity { expression, .. } if expression.contains(&namespace))
        }));
    }
    let old = before.setup().scientific().unwrap();
    let new = after.setup().scientific().unwrap();
    for (id, capture) in old.captures() {
        if capture.context.execution_contract == ExecutionContractId::Field {
            assert!(Arc::ptr_eq(&capture.state, &new.captures()[id].state));
        } else {
            let history = Batch::<DynamicEntity>::read(
                new.captures()[id].history_entities.as_ref().unwrap(),
                Default::default(),
            )
            .unwrap();
            assert_eq!(history.len(), 2);
            assert_eq!(
                history
                    .iter()
                    .map(|e| e.inertial_mass_kilograms.get())
                    .collect::<Vec<_>>(),
                vec![2.0, 3.0]
            );
        }
    }
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::Undo)
    );
    assert_eq!(f.model.document.snapshot().object_count(), 3);
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::Redo)
    );
    assert_eq!(f.model.document.snapshot().setup(), after.setup());
    assert_eq!(f.model.document.snapshot().variables(), after.variables());
    let path = f.installed._dir.path().join("catalog-captured.kagami");
    assert!(f.model.document.save(Some(path.clone()), "test".into()));
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    assert_eq!(offline.document.snapshot().setup(), after.setup());
    assert_eq!(offline.document.snapshot().variables(), after.variables());
    assert!(!offline.document.has_catalog());
    let bundle = kagami::export::compile_snapshot(
        offline.document.snapshot(),
        &f.plugins,
        "catalog".parse().unwrap(),
        Default::default(),
        Default::default(),
        offline.document.limits(),
    )
    .unwrap();
    let portable = orishu_plugin::workload::bundle::read(
        &bundle.bytes,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    drop(offline);
    drop(f);
    let mut admitted = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        portable.manifest_bytes(),
        portable.blobs(),
        orishu_runtime::RunScope {
            workload: portable.verified().root(),
            run: ArtifactDigest::sha256_of(b"catalog composed"),
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
    assert_eq!(objects.len(), 4);
    assert!(objects[2].kinematics.velocity_metres_per_second[0].get() > 0.0);
    assert!(objects[3].kinematics.velocity_metres_per_second[0].get() < 0.0);
}

#[test]
fn catalog_failure_cancellation_and_compound_bounds_leave_source_and_schemas_intact() {
    let mut f = fixture();
    let spec = install_catalog(&mut f);
    for case in [
        "stale_template",
        "invalid_binding",
        "cancel",
        "stale_document",
    ] {
        let mut input = spec.clone();
        if case == "stale_template" {
            input.expected_fingerprint = Some(ContentFingerprint::of(b"not selected"));
        }
        if case == "invalid_binding" {
            input
                .bindings
                .insert("amount".try_into().unwrap(), "unknown_variable".into());
        }
        start(&mut f, input);
        if case == "cancel" {
            f.model.scientific_effects.cancel();
        }
        if case == "stale_document" {
            let object = *f.model.document.snapshot().objects().keys().next().unwrap();
            assert!(f.model.document.edit(vec![Edit::RenameObject {
                object,
                name: DisplayName::new("intervening").unwrap()
            }]));
        }
        let before = f.model.document.snapshot().clone();
        poll(&mut f.model);
        assert_eq!(f.model.document.snapshot(), &before, "{case}");
        assert!(f.model.document.schemas().get(&f.tag).is_none());
        assert!(f.model.document.notice.is_some(), "{case}");
    }
    let guard = f.model.document.authoring_guard().unwrap();
    let mut unpinned = spec.clone();
    unpinned.expected_fingerprint = None;
    assert!(
        f.model
            .scientific_effects
            .instantiate_template(&f.model.document, guard, unpinned, f.lock.clone())
            .is_err()
    );
    let mut excessive = spec;
    excessive.bindings.insert(
        "amount".try_into().unwrap(),
        "1".repeat(f.model.document.limits().max_expression_bytes + 1),
    );
    assert!(
        f.model
            .scientific_effects
            .instantiate_template(&f.model.document, guard, excessive, f.lock.clone())
            .is_err()
    );
    let definition = Edit::DefineVariable(Box::new(kagami_document::VariableSpec::new(
        "captured".try_into().unwrap(),
        "1",
    )));
    assert!(
        f.model
            .scientific_effects
            .extend_components(
                &f.model.document,
                guard,
                vec![definition.clone()],
                f.lock.clone()
            )
            .is_err()
    );
    let too_many = vec![definition; f.model.document.limits().max_commands_per_batch - 1];
    assert!(
        f.model
            .scientific_effects
            .compose_scene(&f.model.document, guard, too_many, f.lock.clone())
            .is_err()
    );
    assert!(
        f.model
            .scientific_effects
            .compose_scene(
                &f.model.document,
                guard,
                vec![Edit::AdoptDependencies(f.lock.clone())],
                f.lock
            )
            .is_err()
    );
    assert!(!f.model.scientific_effects.is_pending());
}

#[test]
fn compound_addition_accepts_two_objects_and_a_definition_as_one_history_entry() {
    let mut f = fixture();
    let before = f.model.document.snapshot().clone();
    let definition = Edit::DefineVariable(Box::new(kagami_document::VariableSpec::new(
        "compound_mass".try_into().unwrap(),
        "4 kg",
    )));
    let mut commands = vec![definition.clone()];
    for x in [-2.0, 2.0] {
        let mut spec = ObjectSpec::new(DisplayName::new("composed body").unwrap())
            .with_transform(Transform::at(Vector3::new(x, 0.0, 0.0).unwrap()));
        for attachment in f.attachments.iter().take(3) {
            let Edit::AttachComponent {
                component,
                properties,
                ..
            } = attachment
            else {
                unreachable!();
            };
            let properties = properties
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        match value {
                            kagami_document::AuthoredValue::Quantity { .. } => {
                                kagami_document::AuthoredValue::si("compound_mass")
                            }
                            _ => value.clone(),
                        },
                    )
                })
                .collect();
            spec = spec.with_component(component.clone(), properties);
        }
        commands.push(Edit::CreateObject(Box::new(spec)));
    }
    f.model
        .scientific_effects
        .compose_scene(
            &f.model.document,
            f.model.document.authoring_guard().unwrap(),
            commands,
            f.lock.clone(),
        )
        .unwrap();
    assert_eq!(f.model.document.snapshot(), &before);
    poll(&mut f.model);
    assert!(
        f.model.document.notice.is_none(),
        "{:?}",
        f.model.document.notice
    );
    let after = f.model.document.snapshot().clone();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    assert_eq!(after.object_count(), 4);
    assert_eq!(after.variable_count(), 1);
    for (id, capture) in before.setup().scientific().unwrap().captures() {
        let current = &after.setup().scientific().unwrap().captures()[id];
        if capture.context.execution_contract == ExecutionContractId::Field {
            assert!(Arc::ptr_eq(&capture.state, &current.state));
        } else {
            let history = Batch::<DynamicEntity>::read(
                current.history_entities.as_ref().unwrap(),
                Default::default(),
            )
            .unwrap();
            assert_eq!(history.len(), 2);
            assert!(
                history
                    .iter()
                    .all(|e| e.inertial_mass_kilograms.get() == 4.0)
            );
        }
    }
    // Define means new: this lane cannot overwrite an existing variable.
    f.model
        .scientific_effects
        .compose_scene(
            &f.model.document,
            f.model.document.authoring_guard().unwrap(),
            vec![definition],
            f.lock.clone(),
        )
        .unwrap();
    poll(&mut f.model);
    assert!(f.model.document.notice.is_some());
    assert_eq!(f.model.document.snapshot(), &after);
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::Undo)
    );
    assert_eq!(f.model.document.snapshot().objects(), before.objects());
    assert_eq!(f.model.document.snapshot().variables(), before.variables());
    assert_eq!(f.model.document.snapshot().setup(), before.setup());
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::Redo)
    );
    assert_eq!(f.model.document.snapshot().objects(), after.objects());
    assert_eq!(f.model.document.snapshot().variables(), after.variables());
}

#[test]
fn a_complete_resolved_lock_cannot_override_the_templates_provider_choice() {
    let mut f = fixture();
    let spec = install_catalog(&mut f);
    let dynamics = declarations::vocabulary()
        .into_iter()
        .find(|(id, _)| id.as_str() == "dynamics")
        .unwrap();
    let alternate = package("org.test.alternate", vec![dynamics], &[]);
    f.plugins.revision = f
        .installed
        .store
        .install(f.plugins.revision, &alternate, None, None)
        .unwrap();
    let provider = alternate
        .release()
        .contribution_ref(&"dynamics".parse().unwrap())
        .unwrap();
    let mut bindings = f.lock.selection().bindings.clone();
    let tag_requirement = bindings
        .iter_mut()
        .find(|binding| &binding.requirement.consumer == f.tag.contribution().unwrap())
        .unwrap();
    tag_requirement.provider = provider;
    let ResolutionOutcome::Resolved { selection, .. } = f
        .installed
        .store
        .resolve(
            &ResolutionRequest {
                expected_inventory_revision: f.plugins.revision,
                roots: f.lock.selection().roots.clone(),
                bindings,
            },
            &[],
        )
        .unwrap()
    else {
        panic!("explicit alternate is compatible");
    };
    let lock =
        Arc::new(SelectionLock::new(selection, f.model.document.limits().dependencies).unwrap());
    // A valid complete authoring lock is still not permission to replace intent
    // embedded in the exact template the author selected.
    assert!(matches!(
        f.installed
            .store
            .prepare_authoring_lock(
                lock.clone(),
                f.plugins.revision,
                &[],
                f.model.document.limits().dependencies
            )
            .unwrap(),
        kagami::plugins::PrepareAuthoringLockOutcome::Ready(_)
    ));
    f.model.scientific_effects =
        kagami::scientific_effect::ScientificEffects::new(Some(f.plugins.clone()));
    let before = f.model.document.snapshot().clone();
    f.model
        .scientific_effects
        .instantiate_template(
            &f.model.document,
            f.model.document.authoring_guard().unwrap(),
            spec,
            lock,
        )
        .unwrap();
    poll(&mut f.model);
    assert!(
        f.model
            .document
            .notice
            .as_ref()
            .is_some_and(|n| n.contains("dependency composition refused")),
        "{:?}",
        f.model.document.notice
    );
    assert_eq!(f.model.document.snapshot(), &before);
    assert!(f.model.document.schemas().get(&f.tag).is_none());
}
