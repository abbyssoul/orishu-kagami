use kagami_catalog::{ComponentSchema, ComponentTypeId, SchemaRegistry, SchemaVersion};
use kagami_document::{DisplayName, ExperimentCommand as Edit, Limits, ObjectSpec};
use kagami_session::{
    ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, PluginReferenceError,
    PluginReferenceLimits, SessionCommand,
};
use orishu_plugin::{ContributionRef, PluginReleaseId};

fn pin(byte: u8) -> ComponentTypeId {
    ComponentTypeId::exact(ContributionRef {
        release: format!("sha256:{}", format!("{byte:02x}").repeat(32))
            .parse()
            .unwrap(),
        extension_point: "orishu.model.components/v1".parse().unwrap(),
        local_id: "test-component".parse().unwrap(),
    })
    .unwrap()
}
fn submit(authority: &mut DocumentAuthority, n: usize, command: SessionCommand) {
    authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new(format!("command-{n}")).unwrap(),
            ActorId::new("test").unwrap(),
            command,
        ))
        .unwrap();
}
fn references(authority: &DocumentAuthority) -> kagami_session::PluginReferenceReport {
    authority
        .plugin_references(PluginReferenceLimits::default())
        .unwrap()
}
fn create(kind: ComponentTypeId) -> SessionCommand {
    SessionCommand::Edit(vec![Edit::CreateObject(Box::new(
        ObjectSpec::new(DisplayName::new("test").unwrap()).with_component(kind, Default::default()),
    ))])
}

#[test]
fn exact_pins_survive_undo_redo_and_receipt_eviction_in_independent_sources() {
    let kind = pin(1);
    let release = kind.contribution().unwrap().release;
    let limits = Limits {
        max_undo_depth: 512,
        ..Limits::default()
    };
    let mut authority = DocumentAuthority::new(
        SchemaRegistry::new().with(ComponentSchema::new(kind.clone(), SchemaVersion(1))),
        limits,
    );
    submit(&mut authority, 0, create(kind));
    let usage = references(&authority).releases[&release];
    assert!(usage.current && usage.receipts && !usage.history);
    submit(&mut authority, 1, SessionCommand::Undo);
    let usage = references(&authority).releases[&release];
    assert!(
        !usage.current && usage.history && usage.receipts,
        "redo retains the removed object"
    );
    submit(&mut authority, 2, SessionCommand::Redo);
    assert!(references(&authority).releases[&release].current);
    let object = *authority
        .experiment()
        .snapshot()
        .objects()
        .keys()
        .next()
        .unwrap();
    submit(
        &mut authority,
        3,
        SessionCommand::Edit(vec![Edit::RemoveObject(object)]),
    );
    // Brackets evict accepted request data but do not replace undo contents.
    for n in 4..310 {
        submit(&mut authority, n, SessionCommand::BeginInteractiveEdit);
    }
    let usage = references(&authority).releases[&release];
    assert!(!usage.current && usage.history && !usage.receipts);
    submit(
        &mut authority,
        310,
        SessionCommand::New {
            discard_unsaved: true,
        },
    );
    assert!(references(&authority).releases.is_empty());
}

#[test]
fn limits_refuse_complete_results_instead_of_returning_a_prefix() {
    let a = pin(1);
    let b = pin(2);
    let mut authority = DocumentAuthority::new(
        SchemaRegistry::new()
            .with(ComponentSchema::new(a.clone(), SchemaVersion(1)))
            .with(ComponentSchema::new(b.clone(), SchemaVersion(1))),
        Limits::default(),
    );
    submit(&mut authority, 0, create(a));
    submit(&mut authority, 1, create(b));
    let before = authority.view();
    let complete = references(&authority);
    assert_eq!(complete.releases.len(), 2);
    assert_eq!(
        authority.plugin_references(PluginReferenceLimits {
            work: complete.work - 1,
            releases: 2
        }),
        Err(PluginReferenceError::WorkLimit)
    );
    assert_eq!(
        authority.plugin_references(PluginReferenceLimits {
            work: complete.work,
            releases: 1
        }),
        Err(PluginReferenceError::ReleaseLimit)
    );
    assert_eq!(
        authority
            .plugin_references(PluginReferenceLimits {
                work: complete.work,
                releases: 2
            })
            .unwrap(),
        complete
    );
    assert_eq!(authority.view(), before);
}

#[test]
fn historical_open_receipts_are_distinct_from_the_current_document() {
    let kind = pin(3);
    let release: PluginReleaseId = kind.contribution().unwrap().release;
    let schemas = SchemaRegistry::new().with(ComponentSchema::new(kind.clone(), SchemaVersion(1)));
    let mut source = DocumentAuthority::new(schemas.clone(), Limits::default());
    submit(&mut source, 0, create(kind));
    let mut authority = DocumentAuthority::new(schemas, Limits::default());
    submit(
        &mut authority,
        0,
        SessionCommand::Open {
            experiment: Box::new(source.experiment().clone()),
            target: None,
            discard_unsaved: true,
        },
    );
    submit(
        &mut authority,
        1,
        SessionCommand::New {
            discard_unsaved: true,
        },
    );
    let usage = references(&authority).releases[&release];
    assert!(!usage.current && !usage.history && usage.receipts);
}

#[test]
fn legacy_names_do_not_guess_a_release_but_still_consume_scan_work() {
    let kind = ComponentTypeId::new(
        kagami_catalog::PluginId::new("legacy.plugin").unwrap(),
        kagami_catalog::ComponentName::new("test").unwrap(),
    );
    let mut authority = DocumentAuthority::new(
        SchemaRegistry::new().with(ComponentSchema::new(kind.clone(), SchemaVersion(1))),
        Limits::default(),
    );
    submit(&mut authority, 0, create(kind));
    let result = references(&authority);
    assert!(result.releases.is_empty());
    assert!(result.work > 1);
    assert_eq!(
        authority.plugin_references(PluginReferenceLimits {
            work: 1,
            releases: 0
        }),
        Err(PluginReferenceError::WorkLimit)
    );
}

#[test]
fn detached_reference_image_survives_edits_without_forking_the_authority() {
    let kind = pin(4);
    let release = kind.contribution().unwrap().release;
    let mut authority = DocumentAuthority::new(
        SchemaRegistry::new().with(ComponentSchema::new(kind.clone(), SchemaVersion(1))),
        Limits::default(),
    );
    submit(&mut authority, 0, create(kind));
    let image = authority
        .plugin_reference_snapshot(Default::default())
        .unwrap();
    submit(
        &mut authority,
        1,
        SessionCommand::New {
            discard_unsaved: true,
        },
    );
    for n in 2..310 {
        submit(&mut authority, n, SessionCommand::BeginInteractiveEdit);
    }
    assert!(references(&authority).releases.is_empty());
    let original = std::thread::spawn(move || image.scan(Default::default()).unwrap())
        .join()
        .unwrap();
    assert!(original.releases[&release].current);
}
