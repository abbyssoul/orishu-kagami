#![cfg(unix)]

#[path = "../../../crates/orishu-plugin/tests/support/mod.rs"]
mod support;

#[path = "support/component_attachment.rs"]
mod component_attachment;

#[path = "support/source_v2.rs"]
mod source_v2;

use kagami::plugins::{Code, InventoryCommand as Command, Package, PluginStore};
use orishu_plugin::{
    bundle::{self, BundleLimits},
    resolution::*,
    *,
};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command as Process, Stdio},
};
use support::*;

// These fixtures assert exact nonblocking flock outcomes. Spawning a child on
// another test thread inherits all open file descriptions until exec, so even
// unrelated temporary inventories can briefly keep each other's locks alive.
// Linux strace reproduced close -> EAGAIN during another fixture's clone/exec.
// Isolate fixture processes, not the explicit thread/process races inside tests.
fn isolate_process_spawns() -> std::sync::MutexGuard<'static, ()> {
    static ISOLATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ISOLATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn package(name: &str, label: &str) -> Package {
    let (mut root, blobs) = release(
        name,
        &declarations(),
        &[b"classical-kernel-fixture", b"euler-kernel-fixture"],
    );
    root.0.metadata.version_label = label.into();
    let bytes = bundle::pack(
        &root,
        &borrowed(&blobs),
        &Limits::default(),
        BundleLimits::default(),
    )
    .unwrap();
    Package::from_bundle(&bytes).unwrap()
}

fn constrained_package(label: &str) -> Package {
    let declaration = payload(
        KnownPoint::Components,
        declaration(
            base("org.example.inertia"),
            serde_json::json!({
                "role":"dynamics", "bindings":{"inertialMass":"inertial-mass"},
                "properties":[
                    {"id":"inertial-mass","required":true,"schema":{"kind":"quantity","dimension":[0,1,0,0,0,0,0],"defaultExpression":"2 kg","minimumSI":1.0,"maximumSI":10.0}},
                    {"id":"label","required":false,"schema":{"kind":"text","maxBytes":4,"default":"test"}},
                    {"id":"visible","required":false,"schema":{"kind":"boolean","default":true}}
                ]
            }),
        ),
    );
    let (mut root, blobs) = release("org.example.constrained", &[("dynamics", declaration)], &[]);
    root.0.metadata.version_label = label.into();
    Package::from_bundle(
        &bundle::pack(
            &root,
            &borrowed(&blobs),
            &Limits::default(),
            BundleLimits::default(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn window_plugins(model: &mut kagami::model::Model, action: kagami::plugins::window::Action) {
    use kagami::{message::Message, update::update};
    let _ = update(model, Message::Plugins(action));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while model.plugin_management.is_pending() || model.plugin_references.is_pending() {
        assert!(
            std::time::Instant::now() < deadline,
            "{}",
            model.plugin_management.notice
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        let _ = update(
            model,
            Message::Plugins(kagami::plugins::window::Action::Poll),
        );
    }
}

#[test]
fn native_management_installs_updates_refreshes_and_preserves_exact_authored_pins() {
    let _process_isolation = isolate_process_spawns();
    use kagami::{
        launch::LaunchOptions,
        message::{Authoritative, Message},
        model::Model,
        plugins::window::Action,
        scientific_effect::ScientificPlugins,
        update::update,
    };
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(PluginStore::open(&dir.path().join("plugins")).unwrap());
    let mut model = Model::new(LaunchOptions {
        scientific_plugins: Some(ScientificPlugins {
            store: store.clone(),
            revision: 0,
            overrides: vec![],
        }),
        ..Default::default()
    });
    window_plugins(&mut model, Action::Open);
    assert!(model.plugin_management.open);
    assert_eq!(
        model.plugin_management.listing.as_ref().unwrap().revision,
        0
    );
    let bad = dir.path().join("invalid.zip");
    fs::write(&bad, b"not a plugin bundle").unwrap();
    window_plugins(
        &mut model,
        Action::Install {
            path: bad,
            update: None,
        },
    );
    assert_eq!(store.list().unwrap().revision, 0);
    assert!(store.list().unwrap().releases.is_empty());
    let first = constrained_package("v1");
    let bundle = dir.path().join("first.zip");
    first.write_bundle(&bundle).unwrap();
    window_plugins(
        &mut model,
        Action::Install {
            path: bundle,
            update: None,
        },
    );
    assert_eq!(model.inventory_revision, Some(1));
    let origin = store.list().unwrap().releases[0].origin.clone().unwrap();
    assert_eq!(origin.path(), dir.path().join("first.zip"));
    window_plugins(&mut model, Action::Inspect(first.release().id()));
    assert!(
        model
            .plugin_management
            .inspection
            .as_ref()
            .unwrap()
            .contains("Local origin (historical only)")
    );
    let pin = kagami_catalog::ComponentTypeId::exact(
        first
            .release()
            .contribution_ref(&"dynamics".parse().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(model.document.schemas().get(&pin).is_some());
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let id = *model.document.snapshot().objects().keys().next().unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(id, pin.clone()).into(),
    );
    component_attachment::accept(&mut model);
    assert!(
        model
            .document
            .save(Some(dir.path().join("document.kagami")), "test".into())
    );
    let authored = model.document.snapshot().clone();
    let history = model.document.view().history.clone();
    window_plugins(&mut model, Action::References(first.release().id()));
    assert!(
        model
            .plugin_management
            .inspection
            .as_ref()
            .unwrap()
            .contains("Current experiment: true")
    );
    let plugin_id = first.release().root().0.metadata.plugin_id.clone();
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetEnabled {
            plugin_id: plugin_id.clone(),
            enabled: false,
        }),
    );
    assert_eq!(model.inventory_revision, Some(2));
    assert!(model.document.schemas().get(&pin).is_none());
    assert_eq!(model.document.snapshot(), &authored);
    assert_eq!(model.document.view().history, history);
    assert!(!model.document.is_dirty());
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetEnabled {
            plugin_id: plugin_id.clone(),
            enabled: true,
        }),
    );
    assert!(model.document.schemas().get(&pin).is_some());
    let next = constrained_package("v2");
    let bundle = dir.path().join("next.zip");
    next.write_bundle(&bundle).unwrap();
    window_plugins(
        &mut model,
        Action::Install {
            path: bundle,
            update: Some(plugin_id.clone()),
        },
    );
    assert_eq!(
        model
            .plugin_management
            .listing
            .as_ref()
            .unwrap()
            .releases
            .len(),
        2
    );
    assert_eq!(model.document.snapshot(), &authored);
    assert!(
        store
            .list()
            .unwrap()
            .releases
            .iter()
            .any(|r| r.release == next.release().id() && r.is_default)
    );
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetDefault {
            plugin_id: plugin_id.clone(),
            release: first.release().id(),
        }),
    );
    window_plugins(&mut model, Action::Inspect(first.release().id()));
    assert!(
        model
            .plugin_management
            .inspection
            .as_ref()
            .unwrap()
            .contains("dynamics")
    );
    let _ = kagami::view::view(&model);
    // An external CLI writer makes a displayed revision stale. No implicit retry.
    let revision = store.list().unwrap().revision;
    store
        .submit(
            revision,
            Command::SetEnabled {
                plugin_id: plugin_id.clone(),
                enabled: false,
            },
        )
        .unwrap();
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetEnabled {
            plugin_id: plugin_id.clone(),
            enabled: true,
        }),
    );
    assert!(model.plugin_management.notice.contains("StaleRevision"));
    assert_eq!(store.list().unwrap().revision, revision + 1);
    window_plugins(&mut model, Action::Refresh);
    assert!(model.document.schemas().get(&pin).is_none());
    assert_eq!(model.document.snapshot(), &authored);
    assert!(!model.document.is_dirty());
    // An exact retained reference refuses the ordinary confirmation. Explicit
    // acknowledgement de-registers it without rewriting the saved document.
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetDefault {
            plugin_id: plugin_id.clone(),
            release: next.release().id(),
        }),
    );
    let before_remove = store.list().unwrap().revision;
    window_plugins(
        &mut model,
        Action::Remove {
            plugin_id,
            release: first.release().id(),
        },
    );
    window_plugins(&mut model, Action::ConfirmRemoval(false));
    assert!(
        model.plugin_management.notice.contains("InUse"),
        "{}",
        model.plugin_management.notice
    );
    assert_eq!(store.list().unwrap().revision, before_remove);
    window_plugins(&mut model, Action::ConfirmRemoval(true));
    assert_eq!(store.list().unwrap().revision, before_remove + 1);
    assert!(model.plugin_management.removal.is_none());
    assert_eq!(model.document.snapshot(), &authored);
    assert!(!model.document.is_dirty());
    let _ = update(&mut model, Message::Plugins(Action::Close));
    assert!(!model.plugin_management.open);
}

#[test]
fn actual_cli_removal_cannot_bypass_a_pending_document_then_requires_acknowledgement() {
    let _process_isolation = isolate_process_spawns();
    use kagami::{
        launch::LaunchOptions, model::Model, plugins::window::Action,
        scientific_effect::ScientificPlugins,
    };
    use kagami_document::{DisplayName, ExperimentCommand as Edit, ObjectSpec};
    use kagami_session::SessionCommand;
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(PluginStore::open(dir.path()).unwrap());
    let package = constrained_package("leases");
    store.install(0, &package, None, None).unwrap();
    let available = store.available_components(&[]).unwrap();
    let kind = available.schemas.schemas().next().unwrap().type_id.clone();
    let values = kagami::plugins::component_defaults(available.schemas.get(&kind).unwrap());
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: available.schemas,
        scientific_plugins: Some(ScientificPlugins {
            store: store.clone(),
            revision: 1,
            overrides: vec![],
        }),
        ..Default::default()
    });
    assert!(
        model
            .document
            .submit(SessionCommand::Edit(vec![Edit::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("leased").unwrap())
                    .with_component(kind.clone(), values.clone())
            ))]))
    );
    let remove = |ack: bool| {
        let mut command = Process::new(env!("CARGO_BIN_EXE_kagami"));
        command
            .args(["plugin", "--directory"])
            .arg(dir.path())
            .args(["--json", "remove"])
            .arg(package.release().root().0.metadata.plugin_id.to_string())
            .arg(package.release().id().to_string());
        if ack {
            command.arg("--ack-open-references");
        }
        let output = command.output().unwrap();
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    // The old startup scan cannot lift the guard for the new document generation.
    assert_eq!(remove(true)["error"]["code"], "Busy");
    window_plugins(&mut model, Action::Poll);
    assert!(
        model.plugin_references.settled(&model.document),
        "{}",
        model.plugin_references.notice
    );
    assert_eq!(remove(false)["error"]["code"], "InUse");
    assert_eq!(remove(true)["ok"], true);
    let snapshot = model.document.snapshot().clone();
    window_plugins(&mut model, Action::Refresh);
    assert!(
        model.plugin_references.missing.is_empty(),
        "an existing exact lease survives deregistration"
    );
    assert_eq!(model.document.snapshot(), &snapshot);
    // Re-registration still sees the same live reader. Closing the document
    // owner releases it; no stale PID or lease metadata needs manual cleanup.
    store
        .install(store.list().unwrap().revision, &package, None, None)
        .unwrap();
    assert_eq!(remove(false)["error"]["code"], "InUse");
    assert!(model.document.submit(SessionCommand::Undo));
    window_plugins(&mut model, Action::Poll);
    assert_eq!(remove(false)["error"]["code"], "InUse");
    assert!(model.document.submit(SessionCommand::New {
        discard_unsaved: true
    }));
    window_plugins(&mut model, Action::Poll);
    assert_eq!(
        remove(false)["error"]["code"],
        "InUse",
        "accepted creation request still retained"
    );
    for _ in 0..310 {
        assert!(model.document.submit(SessionCommand::BeginInteractiveEdit));
    }
    window_plugins(&mut model, Action::Poll);
    assert!(
        model
            .plugin_references
            .report
            .as_ref()
            .unwrap()
            .releases
            .is_empty()
    );
    assert_eq!(
        remove(false)["ok"],
        true,
        "last history/receipt reference released"
    );
    // Reuse the exact component from the retained test snapshot to prove that
    // closing a session also releases a newly acquired live lease.
    store
        .install(store.list().unwrap().revision, &package, None, None)
        .unwrap();
    window_plugins(&mut model, Action::Refresh);
    assert!(
        model
            .document
            .submit(SessionCommand::Edit(vec![Edit::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("reopened").unwrap())
                    .with_component(kind.clone(), values.clone())
            ))]))
    );
    window_plugins(&mut model, Action::Poll);
    assert_eq!(remove(false)["error"]["code"], "InUse");
    drop(model);
    assert_eq!(remove(false)["ok"], true);
}

#[test]
fn absent_release_and_invalid_inventory_are_not_the_same_lease_result() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let release = constrained_package("absent").release().id();
    assert!(store.lease_if_installed(release).unwrap().is_none());
    fs::write(dir.path().join("index.json"), b"not an index").unwrap();
    assert!(store.lease_if_installed(release).is_err());
}

#[test]
fn native_inventory_preferences_do_not_override_process_only_disable() {
    let _process_isolation = isolate_process_spawns();
    use kagami::{
        launch::LaunchOptions, model::Model, plugins::window::Action,
        scientific_effect::ScientificPlugins,
    };
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(PluginStore::open(&dir.path().join("plugins")).unwrap());
    let p = constrained_package("v1");
    store.install(0, &p, None, None).unwrap();
    let plugin_id = p.release().root().0.metadata.plugin_id.clone();
    let mut model = Model::new(LaunchOptions {
        scientific_plugins: Some(ScientificPlugins {
            store: store.clone(),
            revision: 0,
            overrides: vec![(plugin_id.clone(), false)],
        }),
        ..Default::default()
    });
    window_plugins(&mut model, Action::Open);
    assert_eq!(
        model.plugin_management.overrides(),
        &[(plugin_id.clone(), false)]
    );
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetEnabled {
            plugin_id: plugin_id.clone(),
            enabled: false,
        }),
    );
    window_plugins(
        &mut model,
        Action::Mutate(Command::SetEnabled {
            plugin_id,
            enabled: true,
        }),
    );
    assert!(store.list().unwrap().releases[0].enabled);
    assert!(
        model
            .document
            .schemas()
            .schemas()
            .all(|s| s.type_id.contribution().is_none())
    );
}

#[test]
fn startup_vocabulary_drives_the_real_add_component_action_and_saved_reopen() {
    let _process_isolation = isolate_process_spawns();
    use kagami::{launch::LaunchOptions, message::Authoritative, model::Model, update::update};
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(PluginStore::open(&dir.path().join("plugins")).unwrap());
    let package = constrained_package("startup");
    store.install(0, &package, None, None).unwrap();
    let pin = kagami_catalog::ComponentTypeId::exact(
        package
            .release()
            .contribution_ref(&"dynamics".parse().unwrap())
            .unwrap(),
    )
    .unwrap();
    let available = store.available_components(&[]).unwrap();
    assert_eq!(available.revision, 1);
    assert_eq!(available.schemas.len(), 1);
    assert!(available.unavailable.is_empty());
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: available.schemas,
        scientific_plugins: Some(kagami::scientific_effect::ScientificPlugins {
            store: store.clone(),
            revision: available.revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    let _ = update(&mut model, Authoritative::CreateObject.into());
    let object = *model.document.snapshot().objects().keys().next().unwrap();
    let _ = update(
        &mut model,
        Authoritative::AttachComponent(object, pin.clone()).into(),
    );
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
    component_attachment::accept(&mut model);
    let lock = model.document.snapshot().dependencies().unwrap().clone();
    assert_eq!(
        lock.selection().roots,
        vec![pin.contribution().unwrap().clone()]
    );
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let value = &model.document.snapshot().object(object).unwrap().components[&pin];
    assert_eq!(
        value.properties[&"inertial_mass".try_into().unwrap()].source(),
        Some("2 kg")
    );
    assert_eq!(
        value.properties[&"inertial_mass".try_into().unwrap()].si_value(),
        Some(2.0)
    );
    assert_eq!(
        value.properties[&"visible".try_into().unwrap()],
        kagami_document::PropertyValue::Boolean(true)
    );
    let before = model.document.snapshot().clone();
    let _ = update(&mut model, Authoritative::Undo.into());
    assert!(
        !model
            .document
            .snapshot()
            .object(object)
            .unwrap()
            .components
            .contains_key(&pin)
    );
    let _ = update(&mut model, Authoritative::Redo.into());
    assert_eq!(model.document.snapshot().objects(), before.objects());

    let path = dir.path().join("pinned.kagami");
    assert!(model.document.save(Some(path.clone()), "test".into()));
    let newer = constrained_package("startup-new-default");
    let plugin_id = package.release().root().0.metadata.plugin_id.clone();
    store.install(1, &newer, Some(&plugin_id), None).unwrap();
    let reopened = Model::new(LaunchOptions {
        plugin_schemas: store.available_components(&[]).unwrap().schemas,
        open_path: Some(path.clone()),
        ..Default::default()
    });
    assert!(
        reopened.document.notice.is_none(),
        "{:?}",
        reopened.document.notice
    );
    assert_eq!(reopened.document.snapshot().objects(), before.objects());
    assert_eq!(reopened.document.snapshot().dependencies(), Some(&lock));
    assert!(!reopened.document.is_dirty());
    let unavailable = Model::new(LaunchOptions {
        plugin_schemas: store
            .available_components(&[(plugin_id, false)])
            .unwrap()
            .schemas,
        open_path: Some(path),
        ..Default::default()
    });
    assert!(
        unavailable
            .document
            .snapshot()
            .object(object)
            .unwrap()
            .components
            .contains_key(&pin)
    );
    assert!(!unavailable.document.capabilities().is_complete());

    let plugin = package.release().root().0.metadata.plugin_id.clone();
    assert!(
        store
            .available_components(&[(plugin.clone(), false)])
            .unwrap()
            .schemas
            .is_empty()
    );
    assert_eq!(store.list().unwrap().revision, 2);
    assert!(
        store
            .available_components(&[(plugin.clone(), true), (plugin, false)])
            .is_err()
    );
    assert!(
        store
            .available_components(&[("org.example.missing".parse().unwrap(), true)])
            .is_err()
    );
}

#[test]
fn dormant_component_does_not_hide_independent_startup_vocabulary() {
    let _process_isolation = isolate_process_spawns();
    let mut d = declarations();
    let required = d[3].1.contract_ref(&Limits::default()).unwrap();
    let Payload::Components(component) = &mut d[0].1 else {
        unreachable!()
    };
    component.scientific.requirements.push(ContractRequirement {
        slot: "needs-field".parse().unwrap(),
        contract: required,
    });
    let (root, blobs) = release("org.example.partial", &d[..2], &[]);
    let package = Package::from_bundle(
        &bundle::pack(
            &root,
            &borrowed(&blobs),
            &Limits::default(),
            BundleLimits::default(),
        )
        .unwrap(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    store.install(0, &package, None, None).unwrap();
    let available = store.available_components(&[]).unwrap();
    assert_eq!(available.schemas.len(), 1);
    assert_eq!(available.unavailable.len(), 1);
    assert_eq!(available.unavailable[0].local_id.as_str(), "mass");
    assert_eq!(
        available
            .schemas
            .schemas()
            .next()
            .unwrap()
            .type_id
            .contribution()
            .unwrap()
            .local_id
            .as_str(),
        "dynamics"
    );
}

#[test]
fn real_startup_refuses_conflicting_or_unknown_overrides_before_opening_a_window() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let package = constrained_package("flags");
    store.install(0, &package, None, None).unwrap();
    for flags in [
        vec![
            "--enable-plugin",
            "org.example.constrained",
            "--disable-plugin",
            "org.example.constrained",
        ],
        vec!["--enable-plugin", "org.example.missing"],
    ] {
        let output = Process::new(env!("CARGO_BIN_EXE_kagami"))
            .arg("--plugin-directory")
            .arg(dir.path())
            .args(flags)
            .arg("--mcp")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Error loading plugins:"));
        assert_eq!(store.list().unwrap().revision, 1);
    }
}

#[test]
fn installed_exact_schemas_govern_authority_without_rewriting_pins_or_defaulting_values() {
    let _process_isolation = isolate_process_spawns();
    use kagami_document::{AuthoredValue, DisplayName, ExperimentCommand, ObjectSpec};
    use kagami_session::{
        ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    };
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let package = constrained_package("one");
    let reference = package
        .release()
        .contribution_ref(&"dynamics".parse().unwrap())
        .unwrap();
    let pin = kagami_catalog::ComponentTypeId::exact(reference.clone()).unwrap();
    store.install(0, &package, None, None).unwrap();
    let mut request = ResolutionRequest {
        expected_inventory_revision: 1,
        roots: vec![reference],
        bindings: vec![],
    };
    let resolved = store.resolve_authoring(&request, &[]).unwrap();
    assert!(matches!(
        resolved.outcome,
        ResolutionOutcome::Resolved { .. }
    ));
    let schema = resolved.schemas.get(&pin).unwrap();
    assert_eq!(
        schema.plugin_declaration().unwrap().role,
        ComponentRole::Dynamics
    );
    assert_eq!(
        schema
            .plugin_declaration()
            .unwrap()
            .bindings
            .inertial_mass
            .as_ref()
            .unwrap()
            .as_str(),
        "inertial-mass"
    );
    let mass = "inertial_mass".try_into().unwrap();
    assert!(
        matches!(schema.properties[&mass].plugin_declaration(), Some(PropertyType::Quantity { default_expression: Some(source), .. }) if source == "2 kg")
    );
    let mut authority = DocumentAuthority::new(resolved.schemas, kagami_document::Limits::DEFAULT);
    let create = |id: &str, properties| {
        ExperimentCommandEnvelope::new(
            CommandId::new(id).unwrap(),
            ActorId::new("test").unwrap(),
            SessionCommand::Edit(vec![ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("particle").unwrap())
                    .with_component(pin.clone(), properties),
            ))]),
        )
    };
    let initial = authority.revision();
    for (id, expression) in [("low", "0.5 kg"), ("high", "11 kg")] {
        let error = authority
            .submit(create(
                id,
                [(mass.clone(), AuthoredValue::si(expression))].into(),
            ))
            .unwrap_err();
        assert_eq!(error.code(), "property_constraint");
        assert_eq!(authority.revision(), initial);
        assert_eq!(authority.snapshot().object_count(), 0);
    }
    assert!(
        authority
            .submit(create("missing", Default::default()))
            .is_err()
    );
    let error = authority
        .submit(create(
            "long",
            [
                (mass.clone(), AuthoredValue::si("2 kg")),
                (
                    "label".try_into().unwrap(),
                    AuthoredValue::Text("ééé".into()),
                ),
            ]
            .into(),
        ))
        .unwrap_err();
    assert_eq!(error.code(), "property_constraint");
    authority
        .submit(create("valid", [(mass, AuthoredValue::si("2 kg"))].into()))
        .unwrap();
    let original = authority.snapshot();
    // The real experiment codec retains the pin, while hydration independently
    // re-evaluates authored source against its selected declaration.
    use kagami_session::document::{
        DocumentMetadata, ExperimentDocument, StoredValue, decode_document,
    };
    let saved = ExperimentDocument::of(
        authority.experiment(),
        &original,
        &Default::default(),
        DocumentMetadata {
            generator: "plugin-test".into(),
            created: "test".into(),
            saved: "test".into(),
            saved_revision: 1,
        },
    );
    let mut decoded = decode_document(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let reopened = decoded
        .clone()
        .into_experiment(authority.schemas(), &kagami_document::Limits::DEFAULT)
        .unwrap();
    assert_eq!(reopened.snapshot().objects(), original.objects());
    decoded.experiment.objects[0].components[0]
        .properties
        .insert(
            "inertial_mass".try_into().unwrap(),
            StoredValue::Quantity {
                expression: "11 kg".into(),
                unit: None,
            },
        );
    assert_eq!(
        decoded
            .into_experiment(authority.schemas(), &kagami_document::Limits::DEFAULT)
            .unwrap_err()
            .code(),
        "property_constraint"
    );

    // Updating the default release does not reinterpret this document's pin.
    let newer = constrained_package("two");
    let plugin = newer.release().root().0.metadata.plugin_id.clone();
    store.install(1, &newer, Some(&plugin), None).unwrap();
    request.expected_inventory_revision = 2;
    let resolved = store.resolve_authoring(&request, &[]).unwrap();
    assert!(resolved.schemas.get(&pin).is_some());
    authority.adopt_schemas(resolved.schemas, ActorId::new("inventory").unwrap());
    assert_eq!(authority.snapshot(), original);
    assert!(authority.capabilities().is_complete());

    // Process-only disablement makes the pin unavailable; it changes no intent
    // or persisted inventory setting, and no alternate provider appears.
    let disabled = store
        .resolve_authoring(&request, &[(plugin, false)])
        .unwrap();
    assert!(matches!(
        disabled.outcome,
        ResolutionOutcome::Unavailable { .. }
    ));
    assert!(disabled.schemas.is_empty());
    authority.adopt_schemas(disabled.schemas, ActorId::new("inventory").unwrap());
    assert_eq!(authority.snapshot(), original);
    assert!(!authority.capabilities().is_complete());
    assert_eq!(store.list().unwrap().revision, 2);
    request.expected_inventory_revision = 1;
    let stale = store.resolve_authoring(&request, &[]).unwrap();
    assert!(matches!(
        stale.outcome,
        ResolutionOutcome::StaleRevision { .. }
    ));
    assert!(stale.schemas.is_empty());
}

#[test]
fn pinned_catalog_aliases_and_parameter_overrides_use_verified_plugin_constraints() {
    let _process_isolation = isolate_process_spawns();
    use kagami_catalog::{
        ComponentSchema, ComponentTypeId, InstantiationRequest, SchemaRegistry, TemplateIdentity,
        document, load, materialize, resolve,
    };
    let package = constrained_package("catalog");
    let schema =
        ComponentSchema::from_plugin(package.release(), &"dynamics".parse().unwrap()).unwrap();
    let pin = schema.type_id.clone();
    let registry = SchemaRegistry::new().with(schema);
    let document = document::new(
        document::MetadataDocument::new("test".try_into().unwrap(), "particle".try_into().unwrap()),
        serde_json::from_value(serde_json::json!({
            "parameters":{"scale":{"default":"2"}},
            "components":[{"type":pin,"name":"body", "properties":{"inertial_mass":{"quantity":"test.particle.scale"}}}]
        })).unwrap(),
    );
    let text = serde_json::to_string(&document).unwrap();
    let limits = kagami_catalog::Limits::DEFAULT;
    let parsed = load::parse_stream(Path::new("test.yaml"), &text, &limits);
    let set = resolve::resolve(parsed.documents, vec![], &registry, &limits);
    assert!(matches!(
        set.entries()[0].result,
        kagami_catalog::LoadResult::Available { .. }
    ));
    let mut request = InstantiationRequest::new(
        TemplateIdentity::new("test".try_into().unwrap(), "particle".try_into().unwrap()),
        Default::default(),
    );
    let candidate = materialize(&set, &registry, &request).unwrap();
    assert_eq!(candidate.components[0].type_id, pin);
    assert_eq!(
        candidate.resolve_standalone().unwrap()["body.inertial_mass"],
        2.0
    );
    request
        .bindings
        .insert("scale".try_into().unwrap(), "11".into());
    assert!(matches!(
        materialize(&set, &registry, &request),
        Err(kagami_catalog::InstantiationError::PropertyConstraint { .. })
    ));
    // A parameter override cannot smuggle metres into a mass by discarding its
    // derived dimension before the component checks the value.
    request
        .bindings
        .insert("scale".try_into().unwrap(), "2 m".into());
    let wrong_units = materialize(&set, &registry, &request);
    assert!(
        matches!(
            wrong_units,
            Err(kagami_catalog::InstantiationError::PropertyConstraint { .. })
        ),
        "{wrong_units:?}"
    );
    request
        .bindings
        .insert("scale".try_into().unwrap(), "2000 g".into());
    assert_eq!(
        materialize(&set, &registry, &request)
            .unwrap()
            .resolve_standalone()
            .unwrap()["body.inertial_mass"],
        2.0
    );
    let bad = text.replace("\"default\":\"2\"", "\"default\":\"11\"");
    let parsed = load::parse_stream(Path::new("bad.yaml"), &bad, &limits);
    assert!(matches!(
        resolve::resolve(parsed.documents, vec![], &registry, &limits).entries()[0].result,
        kagami_catalog::LoadResult::Invalid { .. }
    ));
    let wrong_dimension = text.replace("\"default\":\"2\"", "\"default\":\"2 m\"");
    let parsed = load::parse_stream(Path::new("wrong-dimension.yaml"), &wrong_dimension, &limits);
    assert!(matches!(
        resolve::resolve(parsed.documents, vec![], &registry, &limits).entries()[0].result,
        kagami_catalog::LoadResult::Invalid { .. }
    ));
    let unit_source = text.replace("\"default\":\"2\"", "\"default\":\"2000 g\"");
    let parsed = load::parse_stream(Path::new("unit-source.yaml"), &unit_source, &limits);
    assert!(matches!(
        resolve::resolve(parsed.documents, vec![], &registry, &limits).entries()[0].result,
        kagami_catalog::LoadResult::Available { .. }
    ));
    let alternate: ComponentTypeId = serde_json::from_value(
        serde_json::json!({"plugin":"org.example.constrained","name":"dynamics"}),
    )
    .unwrap();
    assert!(registry.get(&alternate).is_none());
}
fn source(path: &Path) -> Package {
    let contributions:Vec<_>=declarations().into_iter().map(|(id,p)| {
        let source=match &p {
            Payload::Components(p)=>serde_json::to_value(p),Payload::Fields(p)=>serde_json::to_value(p),
            Payload::Observables(p)=>serde_json::to_value(p),Payload::Constants(p)=>serde_json::to_value(p),
            Payload::FieldModels(p)=>serde_json::to_value(p),Payload::Integrators(p)=>serde_json::to_value(p),
        }.unwrap();
        fs::write(path.join(format!("{id}.json")),serde_json::to_vec(&source).unwrap()).unwrap();
        serde_json::json!({"localId":id,"extensionPoint":p.point().as_str(),"path":format!("{id}.json")})
    }).collect();
    fs::write(path.join("classical.wasm"), b"classical-kernel-fixture").unwrap();
    fs::write(path.join("euler.wasm"), b"euler-kernel-fixture").unwrap();
    fs::write(path.join("plugin.json"),serde_json::to_vec(&serde_json::json!({
        "apiVersion":"orishu.plugin-source/v1","metadata":{"pluginId":"org.example.source","versionLabel":"test"},
        "contributions":contributions,"artifacts":[{"path":"classical.wasm","mediaType":"application/wasm"},{"path":"euler.wasm","mediaType":"application/wasm"}]
    })).unwrap()).unwrap();
    Package::load(path).unwrap()
}
#[test]
fn source_pack_and_safe_output_roundtrip() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let package = source(dir.path());
    assert_eq!(
        package.origin().unwrap().kind(),
        kagami::plugins::OriginKind::LocalSourceDirectory
    );
    let output = dir.path().join("complete.okplugin");
    package.write_bundle(&output).unwrap();
    assert_eq!(
        Package::load(&output).unwrap().release().id(),
        package.release().id()
    );
    assert_eq!(
        package.write_bundle(&output).unwrap_err().code,
        Code::InvalidSelection
    );
    let expected = fs::read(&output).unwrap();
    assert_eq!(expected, package.pack().unwrap());
}

#[test]
fn local_origin_survives_reopen_without_entering_release_or_bundle_identity() {
    let _process_isolation = isolate_process_spawns();
    use std::os::unix::ffi::OsStringExt;
    let dir = tempfile::tempdir().unwrap();
    let inventory = dir.path().join("inventory");
    let original = package("org.example.origins", "one");
    let first = dir.path().join(std::ffi::OsString::from_vec(
        b"first-\xff.okplugin".to_vec(),
    ));
    let second = dir.path().join("second.okplugin");
    // Packaging output currently requires a UTF-8 filename; acquisition still
    // accepts arbitrary Unix filenames supplied by external build tooling.
    fs::write(&first, original.pack().unwrap()).unwrap();
    original.write_bundle(&second).unwrap();
    let a = Package::load_bundle(&first).unwrap();
    let b = Package::load_bundle(&second).unwrap();
    assert_eq!(a.release().id(), b.release().id());
    assert_eq!(a.pack().unwrap(), b.pack().unwrap());
    assert_eq!(a.pack().unwrap(), original.pack().unwrap());
    assert_ne!(a.origin(), b.origin());
    assert!(original.origin().is_none());
    let store = PluginStore::open(&inventory).unwrap();
    store.install(0, &original, None, None).unwrap();
    assert!(store.list().unwrap().releases[0].origin.is_none());
    // Byte-only registration can acquire its first known origin on an explicit
    // reinstall; a later path never silently rewrites that historical fact.
    store.install(1, &a, None, None).unwrap();
    store.install(2, &b, None, None).unwrap();
    assert_eq!(
        store.list().unwrap().releases[0].origin.as_ref(),
        a.origin()
    );
    drop(store);
    fs::remove_file(&first).unwrap();
    let store = PluginStore::open(&inventory).unwrap();
    let inspected = store.inspect(a.release().id()).unwrap();
    assert_eq!(inspected.origin().unwrap().path(), first);
    assert_eq!(inspected.pack().unwrap(), original.pack().unwrap());
    // Scientific retained-package/packing paths never need an origin location.
    let lease = store.lease(a.release().id()).unwrap();
    assert!(store.retained_package(&lease).unwrap().origin().is_none());
    let before = fs::read(inventory.join("index.json")).unwrap();
    assert!(
        store
            .install(
                3,
                &b,
                None,
                Some(package("org.example.origins", "wrong").release().id())
            )
            .is_err()
    );
    assert_eq!(fs::read(inventory.join("index.json")).unwrap(), before);
    let json: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(json["apiVersion"], "orishu.plugin-inventory/v2");
    assert_eq!(
        json["releases"][0]["origin"]["path"]["encoding"],
        "unix-bytes-hex"
    );
}

#[test]
fn legacy_inventory_reads_without_mutation_and_upgrades_only_on_accepted_write() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let package = package("org.example.legacy-origin", "one");
    store.install(0, &package, None, None).unwrap();
    let index = dir.path().join("index.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
    json["apiVersion"] = "orishu.plugin-inventory/v1".into();
    let before = serde_json::to_vec(&json).unwrap();
    fs::write(&index, &before).unwrap();
    drop(store);
    let store = PluginStore::open(dir.path()).unwrap();
    let listing = store.list().unwrap();
    assert_eq!(listing.revision, 1);
    assert!(listing.releases[0].origin.is_none());
    assert_eq!(fs::read(&index).unwrap(), before);
    let command = Command::SetEnabled {
        plugin_id: package.release().root().0.metadata.plugin_id.clone(),
        enabled: false,
    };
    assert_eq!(
        store.submit(0, command.clone()).unwrap_err().code,
        Code::StaleRevision
    );
    assert_eq!(fs::read(&index).unwrap(), before);
    assert_eq!(store.submit(1, command).unwrap(), 2);
    let changed: serde_json::Value = serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
    assert_eq!(changed["apiVersion"], "orishu.plugin-inventory/v2");
    assert_eq!(
        changed["releases"][0]["release"],
        json["releases"][0]["release"]
    );
    assert!(changed["releases"][0].get("origin").is_none());
    // Version and metadata cannot be mixed to smuggle an unversioned path.
    json["releases"][0]["origin"] =
        serde_json::json!({"kind":"local-bundle","path":{"encoding":"utf8","value":"/tmp/a"}});
    fs::write(&index, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.list().is_err());
    json["apiVersion"] = "orishu.plugin-inventory/v2".into();
    json["releases"][0]["origin"]["path"]["value"] = "relative".into();
    fs::write(&index, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.list().is_err());
}
#[test]
fn side_by_side_install_update_enablement_and_removal_survive_restart() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let a = package("org.example.physics", "one");
    let b = package("org.example.physics", "two");
    let plugin = a.release().root().0.metadata.plugin_id.clone();
    let a_id = a.release().id();
    let b_id = b.release().id();
    assert_eq!(store.install(0, &a, None, Some(a_id)).unwrap(), 1);
    assert_eq!(store.install(1, &b, None, None).unwrap(), 2);
    assert!(
        store
            .list()
            .unwrap()
            .releases
            .iter()
            .find(|e| e.release == a_id)
            .unwrap()
            .is_default
    );
    store
        .submit(
            2,
            Command::SetEnabled {
                plugin_id: plugin.clone(),
                enabled: false,
            },
        )
        .unwrap();
    store.install(3, &b, Some(&plugin), None).unwrap();
    let listing = store.list().unwrap();
    assert_eq!(listing.revision, 4);
    assert!(listing.releases.iter().all(|e| !e.enabled));
    assert!(
        listing
            .releases
            .iter()
            .find(|e| e.release == b_id)
            .unwrap()
            .is_default
    );
    assert_eq!(
        store
            .submit(
                4,
                Command::Remove {
                    plugin_id: plugin.clone(),
                    release: b_id,
                    ack_open_references: false
                }
            )
            .unwrap_err()
            .code,
        Code::InvalidSelection
    );
    let stale = store
        .submit(
            3,
            Command::SetEnabled {
                plugin_id: plugin.clone(),
                enabled: true,
            },
        )
        .unwrap_err();
    assert_eq!(stale.code, Code::StaleRevision);
    assert_eq!(stale.actual_revision, Some(4));
    drop(store);
    let store = PluginStore::open(dir.path()).unwrap();
    assert_eq!(store.inspect(a_id).unwrap().release().id(), a_id);
    store
        .submit(
            4,
            Command::Remove {
                plugin_id: plugin.clone(),
                release: a_id,
                ack_open_references: false,
            },
        )
        .unwrap();
    store
        .submit(
            5,
            Command::Remove {
                plugin_id: plugin,
                release: b_id,
                ack_open_references: false,
            },
        )
        .unwrap();
    assert!(store.list().unwrap().releases.is_empty());
    // Removal is not unrequested physical cache purge.
    assert!(
        dir.path()
            .join("releases")
            .join(a_id.to_string().trim_start_matches("sha256:"))
            .exists()
    );
}
#[test]
fn corrupted_storage_and_failed_install_do_not_publish_an_index() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let p = package("org.example.integrity", "one");
    let digest = p.release().root().0.spec.artifacts[0].digest;
    let target = dir
        .path()
        .join("blobs")
        .join(digest.to_string().trim_start_matches("sha256:"));
    fs::write(&target, b"corrupt").unwrap();
    assert!(store.install(0, &p, None, None).is_err());
    assert_eq!(store.list().unwrap().revision, 0);
    assert!(store.list().unwrap().releases.is_empty());
    fs::remove_file(&target).unwrap();
    // Unreferenced interrupted staging is ignored, never interpreted as an index.
    fs::write(dir.path().join(".stage-interrupted"), b"{partial index").unwrap();
    store.install(0, &p, None, None).unwrap();
    fs::write(&target, b"corrupt").unwrap();
    assert!(store.inspect(p.release().id()).is_err());
    assert_eq!(store.list().unwrap().revision, 1);
}
#[test]
fn symlinks_and_source_escape_are_not_followed() {
    let _process_isolation = isolate_process_spawns();
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    source(dir.path());
    fs::write(outside.path().join("sentinel"), b"do not read or modify").unwrap();
    fs::remove_file(dir.path().join("classical.wasm")).unwrap();
    symlink(
        outside.path().join("sentinel"),
        dir.path().join("classical.wasm"),
    )
    .unwrap();
    assert!(Package::load(dir.path()).is_err());
    let manifest = dir.path().join("plugin.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    for path in ["../sentinel", "/tmp/sentinel", "linked/sentinel"] {
        if path == "linked/sentinel" {
            symlink(outside.path(), dir.path().join("linked")).unwrap();
        }
        data["artifacts"][0]["path"] = path.into();
        fs::write(&manifest, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(Package::load(dir.path()).is_err());
    }
    let store_dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(store_dir.path()).unwrap();
    let p = package("org.example.symlinks", "one");
    let digest = p.release().root().0.spec.artifacts[0].digest;
    symlink(
        outside.path().join("sentinel"),
        store_dir
            .path()
            .join("blobs")
            .join(digest.to_string().trim_start_matches("sha256:")),
    )
    .unwrap();
    assert!(store.install(0, &p, None, None).is_err());
    assert_eq!(
        fs::read(outside.path().join("sentinel")).unwrap(),
        b"do not read or modify"
    );
}
#[test]
fn multiple_writers_cannot_overwrite_a_revision() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    PluginStore::open(dir.path()).unwrap();
    let store_one = PluginStore::open(dir.path()).unwrap();
    let store_two = PluginStore::open(dir.path()).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let one = scope.spawn(|| {
            let p = package("org.example.one", "one");
            barrier.wait();
            store_one.install(0, &p, None, None)
        });
        let two = scope.spawn(|| {
            let p = package("org.example.two", "two");
            barrier.wait();
            store_two.install(0, &p, None, None)
        });
        [one.join().unwrap(), two.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| matches!(e.code, Code::Busy | Code::StaleRevision))
    );
    // Inspect the committed index with the already-open reader. Reopening
    // acquires a separate nonblocking initialization lock, unrelated to the
    // two mutations this test races (and may legitimately report Busy).
    let listing = store_one.list().unwrap();
    assert_eq!(listing.revision, 1);
    assert_eq!(listing.releases.len(), 1);
}
#[test]
fn process_overrides_and_provider_resolution_do_not_rewrite_inventory() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let p = package("org.example.resolve", "one");
    store.install(0, &p, None, None).unwrap();
    let plugin = p.release().root().0.metadata.plugin_id.clone();
    let request = ResolutionRequest {
        expected_inventory_revision: 1,
        roots: vec![
            p.release()
                .contribution_ref(&"classical".parse().unwrap())
                .unwrap(),
        ],
        bindings: vec![],
    };
    assert!(matches!(
        store.resolve(&request, &[]).unwrap(),
        ResolutionOutcome::Resolved { .. }
    ));
    assert!(matches!(
        store.resolve(&request, &[(plugin.clone(), false)]).unwrap(),
        ResolutionOutcome::Unavailable { .. }
    ));
    assert!(store.list().unwrap().releases[0].enabled);
    assert_eq!(
        store
            .resolve(&request, &[(plugin.clone(), true), (plugin, false)])
            .unwrap_err()
            .code,
        Code::InvalidSelection
    );
}
#[test]
fn lease_child() {
    let _process_isolation = isolate_process_spawns();
    let Some(path) = std::env::var_os("KAGAMI_TEST_LEASE_STORE") else {
        return;
    };
    let id = std::env::var("KAGAMI_TEST_LEASE_RELEASE")
        .unwrap()
        .parse()
        .unwrap();
    let store = PluginStore::open(Path::new(&path)).unwrap();
    let lease = store.lease(id).unwrap();
    println!("LEASE_READY");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    assert_eq!(store.retained_package(&lease).unwrap().release().id(), id);
}
#[test]
fn cross_process_lease_survives_acknowledged_deregistration() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let p = package("org.example.leased", "one");
    store.install(0, &p, None, None).unwrap();
    let id = p.release().id();
    let plugin = p.release().root().0.metadata.plugin_id.clone();
    let mut child = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "lease_child", "--nocapture"])
        .env("KAGAMI_TEST_LEASE_STORE", dir.path())
        .env("KAGAMI_TEST_LEASE_RELEASE", id.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        if line.contains("LEASE_READY") {
            break;
        }
    }
    let command = Command::Remove {
        plugin_id: plugin.clone(),
        release: id,
        ack_open_references: false,
    };
    assert_eq!(store.submit(1, command).unwrap_err().code, Code::InUse);
    store
        .submit(
            1,
            Command::Remove {
                plugin_id: plugin,
                release: id,
                ack_open_references: true,
            },
        )
        .unwrap();
    assert!(store.lease(id).is_err());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert!(child.wait().unwrap().success());
}
#[test]
fn real_cli_packs_installs_lists_and_returns_structured_errors() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    source(source_dir.path());
    let bundle = source_dir.path().join("cli.okplugin");
    let run = |args: &[&str]| {
        let output = Process::new(env!("CARGO_BIN_EXE_kagami"))
            .args([
                "plugin",
                "--json",
                "--directory",
                dir.path().to_str().unwrap(),
            ])
            .args(args)
            .output()
            .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
                panic!(
                    "stdout {} stderr {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            });
        assert_eq!(value["ok"].as_bool().unwrap(), output.status.success());
        assert_eq!(value["apiVersion"], "kagami.plugin-command/v2");
        value
    };
    assert_eq!(
        run(&[
            "pack",
            source_dir.path().to_str().unwrap(),
            "--output",
            bundle.to_str().unwrap()
        ])["ok"],
        true
    );
    assert!(!dir.path().join("index.json").exists());
    assert_eq!(
        run(&["install", bundle.to_str().unwrap()])["result"]["revision"],
        1
    );
    let listing = run(&["list", "--all-releases"]);
    assert_eq!(listing["result"]["releases"].as_array().unwrap().len(), 1);
    assert_eq!(
        listing["result"]["releases"][0]["origin"]["kind"],
        "local-bundle"
    );
    assert_eq!(
        listing["result"]["releases"][0]["origin"]["path"]["value"],
        bundle.to_str().unwrap()
    );
    let id = listing["result"]["releases"][0]["release"]
        .as_str()
        .unwrap();
    assert_eq!(
        run(&["inspect", id])["result"]["localOrigin"],
        listing["result"]["releases"][0]["origin"]
    );
    assert_eq!(
        run(&["--expected-revision", "0", "disable", "org.example.source"])["error"]["code"],
        "StaleRevision"
    );
    assert_eq!(run(&["disable", "org.example.source"])["ok"], true);
    assert_eq!(run(&["list"])["result"]["releases"][0]["enabled"], false);
}

#[test]
fn real_cli_respects_another_process_writer_lock() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let store = PluginStore::open(dir.path()).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join("inventory.lock"))
        .unwrap();
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    let output = Process::new(env!("CARGO_BIN_EXE_kagami"))
        .args([
            "plugin",
            "--json",
            "--directory",
            dir.path().to_str().unwrap(),
            "list",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "Busy");
    assert_eq!(store.list().unwrap().revision, 0);
    drop(file);
    assert!(PluginStore::open(dir.path()).is_ok());
}
