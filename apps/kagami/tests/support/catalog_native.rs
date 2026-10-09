//! Actual native actions over real catalog and plugin authorities.
use super::*;
use kagami::catalog_form::{Action as C, Input};

fn act(model: &mut Model, action: C) {
    let _ = update(model, Message::Catalog(action));
}
fn drain(model: &mut Model) {
    let until = std::time::Instant::now() + Duration::from_secs(90);
    while model.catalog.is_pending() || model.scientific_effects.is_pending() {
        act(model, C::Poll);
        assert!(
            std::time::Instant::now() < until,
            "{}",
            model.catalog.notice
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn load(f: &mut Fixture) -> std::path::PathBuf {
    let root = f.installed._dir.path().join("templates");
    std::fs::create_dir_all(&root).unwrap();
    let set = catalog(f);
    let bytes = set
        .get(&identity())
        .unwrap()
        .result
        .template()
        .unwrap()
        .canonical_bytes();
    std::fs::write(root.join("body.yaml"), bytes).unwrap();
    let before = f.model.document.snapshot().clone();
    act(&mut f.model, C::Open);
    act(&mut f.model, C::Load(root.clone()));
    drain(&mut f.model);
    assert_eq!(f.model.document.snapshot(), &before);
    assert!(f.model.document.has_catalog());
    assert_eq!(f.model.catalog.snapshot().unwrap().entries().len(), 1);
    root
}
fn select(model: &mut Model) {
    let catalog = model.catalog.catalog_token();
    act(model, C::Select { catalog, index: 0 });
    assert!(
        model.catalog.form.current(&model.document),
        "{}",
        model.catalog.notice
    );
}
fn edit(model: &mut Model, input: Input) {
    let form = model.catalog.form.input_token();
    act(model, C::Edit { form, input });
}
fn check(model: &mut Model) {
    act(
        model,
        C::Dependencies(kagami::physics_form::dependencies::Action::Check),
    );
    drain(model);
    assert!(
        matches!(
            model.catalog.dependencies.report().map(|r| r.outcome()),
            Some(ResolutionOutcome::Resolved { .. })
        ),
        "{}",
        model.catalog.dependencies.notice
    );
}
fn apply(model: &mut Model, consent: bool) {
    let proposal = model.catalog.form.token();
    if consent {
        act(
            model,
            C::Confirm {
                proposal,
                confirmed: true,
            },
        );
    }
    act(model, C::Apply { proposal });
}

#[test]
fn uncaptured_native_catalog_revalidates_selected_schemas_and_preserves_offline_data() {
    let mut f = fixture();
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::New {
                discard_unsaved: true
            })
    );
    let root = load(&mut f);
    assert!(
        !f.model.catalog.snapshot().unwrap().entries()[0]
            .result
            .is_available(),
        "tag schema is not adopted yet"
    );
    select(&mut f.model);
    let input_token = f.model.catalog.form.input_token();
    let initial_generation = f.model.catalog.form.token();
    act(
        &mut f.model,
        C::Edit {
            form: input_token,
            input: Input::Position {
                axis: 3,
                metres: "1".into(),
            },
        },
    );
    act(
        &mut f.model,
        C::Edit {
            form: input_token,
            input: Input::Parameter {
                name: "unknown".try_into().unwrap(),
                source: Some("1".into()),
            },
        },
    );
    act(
        &mut f.model,
        C::Edit {
            form: input_token,
            input: Input::Parameter {
                name: "amount".try_into().unwrap(),
                source: Some("1".repeat(1025)),
            },
        },
    );
    assert_eq!(f.model.catalog.form.token(), initial_generation);
    // Several queued keystrokes use one stable field identity, while each edit
    // invalidates the consent/resolver generation independently.
    for name in ["n", "ne", "new body"] {
        act(
            &mut f.model,
            C::Edit {
                form: input_token,
                input: Input::Name(name.into()),
            },
        );
    }
    assert_eq!(f.model.catalog.form.name, "new body");
    edit(
        &mut f.model,
        Input::Parameter {
            name: "amount".try_into().unwrap(),
            source: Some("3".into()),
        },
    );
    edit(
        &mut f.model,
        Input::Position {
            axis: 0,
            metres: "-2".into(),
        },
    );
    edit(
        &mut f.model,
        Input::Velocity {
            axis: 1,
            metres_per_second: "1.5".into(),
        },
    );
    check(&mut f.model);
    assert!(f.model.document.schemas().get(&f.tag).is_none());
    let before = f.model.document.snapshot().clone();
    apply(&mut f.model, false);
    drain(&mut f.model);
    assert_eq!(
        f.model.document.snapshot().revision().get(),
        before.revision().get() + 1,
        "{}",
        f.model.catalog.notice
    );
    assert!(f.model.document.schemas().get(&f.tag).is_some());
    let after = f.model.document.snapshot().clone();
    let object = after.objects().values().next().unwrap();
    assert_eq!(object.name.as_str(), "new body");
    assert_eq!(
        object.transform.translation,
        Vector3::new(-2.0, 0.0, 0.0).unwrap()
    );
    assert_eq!(object.velocity.linear, Vector3::new(0.0, 1.5, 0.0).unwrap());
    assert!(object.provenance.is_some());
    assert_eq!(after.dependencies(), Some(&f.lock));
    assert!(after.setup().scientific().is_none());
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::Undo)
    );
    assert_eq!(f.model.document.snapshot().object_count(), 0);
    assert_eq!(f.model.document.snapshot().variable_count(), 0);
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::Redo)
    );
    let old_catalog = f.model.catalog.catalog_token();
    act(&mut f.model, C::Reload);
    drain(&mut f.model);
    assert_ne!(old_catalog, f.model.catalog.catalog_token());
    assert!(
        f.model.catalog.snapshot().unwrap().entries()[0]
            .result
            .is_available(),
        "Reload adopts the current schema projection through the catalog authority"
    );
    act(
        &mut f.model,
        C::Select {
            catalog: old_catalog,
            index: 0,
        },
    );
    assert!(!f.model.catalog.form.current(&f.model.document));
    assert_eq!(f.model.document.snapshot().objects(), after.objects());
    std::fs::remove_file(root.join("body.yaml")).unwrap();
    act(&mut f.model, C::Reload);
    drain(&mut f.model);
    assert!(f.model.catalog.snapshot().unwrap().entries().is_empty());
    assert_eq!(f.model.document.snapshot().objects(), after.objects());
    let path = f.installed._dir.path().join("native-template.kagami");
    assert!(f.model.document.save(Some(path.clone()), "test".into()));
    let offline = Model::new(LaunchOptions {
        open_path: Some(path),
        ..Default::default()
    });
    assert_eq!(offline.document.snapshot().variables(), after.variables());
    assert_eq!(
        offline.document.snapshot().dependencies(),
        after.dependencies()
    );
    assert!(!offline.document.has_catalog());
}

#[test]
fn captured_native_template_requires_current_consent_and_survives_inventory_free_execution() {
    let mut f = fixture();
    load(&mut f);
    let before = f.model.document.snapshot().clone();
    for x in ["-2", "2"] {
        select(&mut f.model);
        edit(
            &mut f.model,
            Input::Position {
                axis: 0,
                metres: x.into(),
            },
        );
        edit(
            &mut f.model,
            Input::Parameter {
                name: "amount".try_into().unwrap(),
                source: Some("2".into()),
            },
        );
        check(&mut f.model);
        apply(&mut f.model, false);
        assert!(!f.model.catalog.is_pending());
        let old = f.model.catalog.form.token();
        act(
            &mut f.model,
            C::Confirm {
                proposal: old,
                confirmed: true,
            },
        );
        edit(&mut f.model, Input::Name("particle".into()));
        assert!(!f.model.catalog.form.confirmed());
        act(
            &mut f.model,
            C::Confirm {
                proposal: old,
                confirmed: true,
            },
        );
        assert!(!f.model.catalog.form.confirmed());
        check(&mut f.model);
        let prior = f.model.document.snapshot().clone();
        let current = f.model.catalog.form.token();
        act(
            &mut f.model,
            C::Confirm {
                proposal: current,
                confirmed: true,
            },
        );
        act(&mut f.model, C::Apply { proposal: old });
        assert!(
            f.model.catalog.form.confirmed(),
            "stale Apply cannot revoke current consent"
        );
        apply(&mut f.model, true);
        // Creation isn't acceptance of a staged schema or partial object.
        assert_eq!(f.model.document.snapshot(), &prior);
        drain(&mut f.model);
        assert_eq!(
            f.model.document.snapshot().revision().get(),
            prior.revision().get() + 1,
            "{} {:?}",
            f.model.catalog.notice,
            f.model.document.notice
        );
    }
    let after = f.model.document.snapshot().clone();
    assert_eq!(after.object_count(), 4);
    for (id, capture) in before.setup().scientific().unwrap().captures() {
        if capture.context.execution_contract == ExecutionContractId::Field {
            assert!(Arc::ptr_eq(
                &capture.state,
                &after.setup().scientific().unwrap().captures()[id].state
            ));
        }
    }
    let exported = kagami::export::compile_snapshot(
        &after,
        &f.plugins,
        "native-catalog".parse().unwrap(),
        Default::default(),
        Default::default(),
        f.model.document.limits(),
    )
    .unwrap();
    let bundle = orishu_plugin::workload::bundle::read(
        &exported.bytes,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    drop(f);
    let mut runtime = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        bundle.manifest_bytes(),
        bundle.blobs(),
        orishu_runtime::RunScope {
            workload: bundle.verified().root(),
            run: ArtifactDigest::sha256_of(b"native catalog"),
            epoch: 1,
        },
        &Default::default(),
        Default::default(),
        control(),
    )
    .unwrap();
    runtime.run.advance(control()).unwrap();
    let objects: Vec<_> = runtime
        .run
        .state()
        .objects()
        .scientific_batch::<ObjectState>(Default::default())
        .unwrap()
        .iter()
        .collect();
    assert!(objects[2].kinematics.velocity_metres_per_second[0].get() > 0.0);
    assert!(objects[3].kinematics.velocity_metres_per_second[0].get() < 0.0);
}

#[test]
fn cancelled_changed_and_invalid_native_templates_do_not_publish_data_or_capabilities() {
    let mut f = fixture();
    load(&mut f);
    for case in [
        "invalid",
        "cancel",
        "withdraw",
        "edit_document",
        "change_input",
    ] {
        select(&mut f.model);
        if case == "invalid" {
            edit(
                &mut f.model,
                Input::Parameter {
                    name: "amount".try_into().unwrap(),
                    source: Some("missing_symbol".into()),
                },
            );
        }
        check(&mut f.model);
        let before = f.model.document.snapshot().clone();
        apply(&mut f.model, true);
        if case != "invalid" {
            let until = std::time::Instant::now() + Duration::from_secs(30);
            while !f.model.scientific_effects.is_pending() {
                act(&mut f.model, C::Poll);
                assert!(f.model.catalog.is_pending(), "{}", f.model.catalog.notice);
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        match case {
            "cancel" => act(&mut f.model, C::Cancel),
            "withdraw" => {
                let proposal = f.model.catalog.form.token();
                act(
                    &mut f.model,
                    C::Confirm {
                        proposal,
                        confirmed: false,
                    },
                );
            }
            "change_input" => edit(&mut f.model, Input::Name("different proposal".into())),
            "edit_document" => {
                let object = *f.model.document.snapshot().objects().keys().next().unwrap();
                assert!(f.model.document.edit(vec![Edit::RenameObject {
                    object,
                    name: DisplayName::new("changed").unwrap()
                }]));
            }
            _ => {}
        }
        let expected = f.model.document.snapshot().clone();
        drain(&mut f.model);
        assert_eq!(
            f.model.document.snapshot(),
            &expected,
            "{case}: {}",
            f.model.catalog.notice
        );
        assert_eq!(expected.object_count(), before.object_count());
        assert_eq!(expected.setup(), before.setup());
        assert!(f.model.document.schemas().get(&f.tag).is_none(), "{case}");
    }
}

#[test]
fn native_catalog_asks_for_unbound_providers_and_cannot_change_template_locked_choices() {
    use kagami::physics_form::dependencies::Action as D;
    let mut f = fixture();
    assert!(
        f.model
            .document
            .submit(kagami_session::SessionCommand::New {
                discard_unsaved: true
            })
    );
    let mut template = catalog(&f)
        .get(&identity())
        .unwrap()
        .result
        .template()
        .unwrap()
        .clone();
    let root = load(&mut f);
    let dynamics = declarations::vocabulary()
        .into_iter()
        .find(|(id, _)| id.as_str() == "dynamics")
        .unwrap();
    let alternate = package("org.test.catalog.alternate", vec![dynamics], &[]);
    f.installed
        .store
        .install(f.plugins.revision, &alternate, None, None)
        .unwrap();
    let alternate_pin = alternate
        .release()
        .contribution_ref(&"dynamics".parse().unwrap())
        .unwrap();
    let _ = update(
        &mut f.model,
        Message::Plugins(kagami::plugins::window::Action::Refresh),
    );
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while f.model.plugin_management.is_pending() {
        let _ = update(
            &mut f.model,
            Message::Plugins(kagami::plugins::window::Action::Poll),
        );
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    // The saved v3 edge is intent, even though an alternative is installed.
    select(&mut f.model);
    check(&mut f.model);
    let report = f.model.catalog.dependencies.report().unwrap().token();
    let requirement = f
        .lock
        .selection()
        .bindings
        .iter()
        .find(|b| &b.requirement.consumer == f.tag.contribution().unwrap())
        .unwrap()
        .requirement
        .clone();
    act(
        &mut f.model,
        C::Dependencies(D::Browse {
            report,
            requirement,
        }),
    );
    drain(&mut f.model);
    let report = f.model.catalog.dependencies.report().unwrap();
    let index = report
        .alternatives()
        .unwrap()
        .candidates
        .iter()
        .position(|p| p == &alternate_pin)
        .unwrap();
    let token = report.token();
    act(
        &mut f.model,
        C::Dependencies(D::ChooseInstalled {
            report: token,
            candidate: index,
        }),
    );
    assert!(f.model.catalog.dependencies.notice.contains("fixed"));
    assert_eq!(f.model.document.snapshot().object_count(), 0);
    // A deliberately unlocked v2 template instead exposes the actual ambiguity.
    template.dependencies = None;
    std::fs::write(root.join("body.yaml"), template.canonical_bytes()).unwrap();
    act(&mut f.model, C::Reload);
    drain(&mut f.model);
    select(&mut f.model);
    act(&mut f.model, C::Dependencies(D::Check));
    drain(&mut f.model);
    let report = f.model.catalog.dependencies.report().unwrap();
    let ResolutionOutcome::Unavailable { issues, .. } = report.outcome() else {
        panic!("must not guess a provider");
    };
    let (issue, diagnostic) = issues
        .iter()
        .enumerate()
        .find(|(_, d)| {
            d.reason == orishu_plugin::resolution::UnavailableReason::AmbiguousProvider
                && d.requirement
                    .as_ref()
                    .is_some_and(|r| &r.consumer == f.tag.contribution().unwrap())
        })
        .unwrap();
    let candidate = diagnostic
        .candidates
        .iter()
        .position(|p| p == &alternate_pin)
        .unwrap();
    let report = report.token();
    act(
        &mut f.model,
        C::Dependencies(D::Choose {
            report,
            issue,
            candidate,
        }),
    );
    check(&mut f.model);
    apply(&mut f.model, false);
    drain(&mut f.model);
    let snapshot = f.model.document.snapshot();
    assert_eq!(snapshot.object_count(), 1, "{}", f.model.catalog.notice);
    assert!(snapshot.setup().scientific().is_none());
    assert!(
        snapshot
            .dependencies()
            .unwrap()
            .selection()
            .bindings
            .iter()
            .any(|b| &b.requirement.consumer == f.tag.contribution().unwrap()
                && b.provider == alternate_pin)
    );
}
