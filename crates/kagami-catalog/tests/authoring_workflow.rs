//! The catalog authority as its adapters will actually drive it.
//!
//! ADR 0006 requires the UI and MCP surfaces to be equivalent. The way this
//! crate makes that testable is by having no adapter-shaped API at all: both
//! surfaces build the same [`CatalogCommandEnvelope`] and read the same
//! [`CatalogView`]. The parity test below is therefore not a mock of two
//! transports; it is two callers submitting the same command values and
//! asserting the decisions are identical.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use kagami_catalog::authority::ValidationReport;
use kagami_catalog::document::{
    ComponentDocument, MetadataDocument, PropertyValueDocument, QuantityDocument, SpecDocument,
};
use kagami_catalog::{
    ActorId, CatalogAuthority, CatalogChange, CatalogCommand, CatalogCommandEnvelope,
    CatalogOutcome, CatalogQuery, CatalogRejection, CatalogRevision, CatalogView, CommandId,
    InstantiationRequest, Limits, PropertyName, TemplateDocument, WriteTarget,
};
use orishu_variables::Namespace;
use support::{component, example_catalog_root, full_registry, identity};

fn workspace() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary catalog root");
    for name in ["planets.yaml", "particles.yaml", "scenarios.yaml"] {
        fs::copy(
            example_catalog_root().join(name),
            directory.path().join(name),
        )
        .expect("the shipped catalogs are readable");
    }
    directory
}

fn authority(root: &Path) -> CatalogAuthority {
    CatalogAuthority::open(root, full_registry(), Limits::DEFAULT)
}

/// A template document as an adapter would build it from user input.
fn moon_document(mass: &str) -> TemplateDocument {
    kagami_catalog::document::new(
        MetadataDocument {
            description: Some("A newly authored body".to_owned()),
            ..MetadataDocument::new(
                "planets".try_into().unwrap(),
                "kuiper_body".try_into().unwrap(),
            )
        },
        SpecDocument {
            components: vec![ComponentDocument {
                component_type: component("kagami.mass_sources", "inertial_mass"),
                properties: [(
                    PropertyName::new("mass").unwrap(),
                    PropertyValueDocument::Quantity(QuantityDocument::in_unit(mass, "kg")),
                )]
                .into_iter()
                .collect(),
            }],
            ..SpecDocument::default()
        },
    )
}

fn envelope(id: &str, actor: &str, command: CatalogCommand) -> CatalogCommandEnvelope {
    CatalogCommandEnvelope::new(
        CommandId::new(id).unwrap(),
        ActorId::new(actor).unwrap(),
        command,
    )
}

fn create_kuiper_body(mass: &str) -> CatalogCommand {
    CatalogCommand::Create {
        file: PathBuf::from("planets.yaml"),
        document: Box::new(moon_document(mass)),
    }
}

fn entries(authority: &CatalogAuthority) -> Vec<kagami_catalog::EntrySummary> {
    let CatalogView::List { entries, .. } = authority.query(&CatalogQuery::List) else {
        panic!("expected a list view");
    };
    entries
}

#[test]
fn a_full_create_update_delete_cycle_advances_one_revision_at_a_time() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let start = authority.revision();

    let created = authority
        .submit(envelope(
            "create",
            "researcher",
            create_kuiper_body("1.0e21"),
        ))
        .expect("creating a new template succeeds");
    assert_eq!(created.revision(), start.next());
    let identity = identity("planets", "kuiper_body");
    let entry = authority.set().get(&identity).expect("it is now loaded");
    assert!(entry.result.is_available());
    let source = entry.source.clone();

    let updated = authority
        .submit(envelope(
            "update",
            "researcher",
            CatalogCommand::Update {
                target: WriteTarget::new(&source, identity.clone()),
                document: Box::new(moon_document("2.0e21")),
            },
        ))
        .expect("updating it succeeds");
    assert_eq!(updated.revision(), start.next().next());

    let deleted = authority
        .submit(envelope(
            "delete",
            "researcher",
            CatalogCommand::Delete {
                target: WriteTarget::new(
                    &authority.set().get(&identity).unwrap().source.clone(),
                    identity.clone(),
                ),
            },
        ))
        .expect("deleting it succeeds");
    assert_eq!(deleted.revision(), start.next().next().next());
    assert!(authority.set().get(&identity).is_none());

    let changes: Vec<_> = authority
        .events()
        .map(|event| event.change.clone())
        .collect();
    assert_eq!(
        changes,
        vec![
            CatalogChange::Created(identity.clone()),
            CatalogChange::Updated {
                previous: identity.clone(),
                current: identity.clone(),
            },
            CatalogChange::Deleted(identity),
        ]
    );
}

#[test]
fn writing_one_entry_leaves_every_other_entry_in_the_file_intact() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let before = entries(&authority).len();

    authority
        .submit(envelope(
            "create",
            "researcher",
            create_kuiper_body("1.0e21"),
        ))
        .unwrap();
    assert_eq!(entries(&authority).len(), before + 1);
    assert_eq!(authority.set().summary().invalid, 0);
    assert_eq!(authority.set().summary().unavailable, 0);

    // The file header comment written by hand is still there.
    let text = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();
    assert!(
        text.starts_with("# Notable Solar System bodies"),
        "the authored file header survives a write: {}",
        &text[..80.min(text.len())]
    );
}

#[test]
fn the_ui_path_and_the_mcp_path_produce_the_same_decision() {
    // Two adapters, two independent catalog roots, the same command values.
    let ui_root = workspace();
    let mcp_root = workspace();
    let mut ui = authority(ui_root.path());
    let mut mcp = authority(mcp_root.path());

    for (index, command) in [
        create_kuiper_body("1.0e21"),
        CatalogCommand::Validate {
            text: "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata: {catalog: \
                   planets, name: broken}\nspec:\n  helpers:\n    x: {expression: \"1 +\"}\n"
                .to_owned(),
        },
        CatalogCommand::Reload,
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("command-{index}");
        let from_ui = ui.submit(envelope(&id, "ui", command.clone()));
        let from_mcp = mcp.submit(envelope(&id, "mcp", command));
        match (from_ui, from_mcp) {
            (Ok(left), Ok(right)) => assert_eq!(left, right, "outcomes differ at command {index}"),
            (left, right) => panic!("one path failed at command {index}: {left:?} / {right:?}"),
        }
        assert_eq!(ui.revision(), mcp.revision());
    }

    assert_eq!(entries(&ui), entries(&mcp));
}

#[test]
fn a_rejected_command_leaves_the_revision_the_projection_and_the_files_untouched() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let before_revision = authority.revision();
    let before_entries = entries(&authority);
    let before_file = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();

    let error = authority
        .submit(envelope(
            "create-duplicate",
            "researcher",
            CatalogCommand::Create {
                file: PathBuf::from("planets.yaml"),
                document: Box::new(kagami_catalog::document::new(
                    MetadataDocument::new(
                        "planets".try_into().unwrap(),
                        "earth".try_into().unwrap(),
                    ),
                    SpecDocument::default(),
                )),
            },
        ))
        .unwrap_err();
    assert!(matches!(error, CatalogRejection::DuplicateIdentity { .. }));

    assert_eq!(authority.revision(), before_revision);
    assert_eq!(entries(&authority), before_entries);
    assert_eq!(
        fs::read_to_string(directory.path().join("planets.yaml")).unwrap(),
        before_file
    );
}

#[test]
fn renaming_a_shipped_template_onto_a_sibling_identity_is_refused_before_the_file_changes() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let before_revision = authority.revision();
    let before_entries = entries(&authority);
    let before_file = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();

    let earth = identity("planets", "earth");
    let source = authority.set().get(&earth).unwrap().source.clone();
    let error = authority
        .submit(envelope(
            "rename-earth-to-mars",
            "researcher",
            CatalogCommand::Update {
                target: WriteTarget::new(&source, earth),
                document: Box::new(kagami_catalog::document::new(
                    MetadataDocument::new(
                        "planets".try_into().unwrap(),
                        "mars".try_into().unwrap(),
                    ),
                    SpecDocument::default(),
                )),
            },
        ))
        .unwrap_err();
    assert!(matches!(error, CatalogRejection::DuplicateIdentity { .. }));

    assert_eq!(authority.revision(), before_revision);
    assert_eq!(entries(&authority), before_entries);
    assert_eq!(
        fs::read_to_string(directory.path().join("planets.yaml")).unwrap(),
        before_file
    );
}

#[test]
fn a_rename_publishes_the_retired_identity_so_a_cached_listing_converges() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let mut cached: Vec<_> = entries(&authority)
        .into_iter()
        .map(|entry| entry.identity)
        .collect();

    let earth = identity("planets", "earth");
    let source = authority.set().get(&earth).unwrap().source.clone();
    let outcome = authority
        .submit(envelope(
            "rename-earth-to-terra",
            "researcher",
            CatalogCommand::Update {
                target: WriteTarget::new(&source, earth.clone()),
                document: Box::new(kagami_catalog::document::new(
                    MetadataDocument::new(
                        "planets".try_into().unwrap(),
                        "terra".try_into().unwrap(),
                    ),
                    SpecDocument::default(),
                )),
            },
        ))
        .expect("renaming onto a free identity succeeds");

    let terra = identity("planets", "terra");
    let CatalogOutcome::Updated {
        previous, current, ..
    } = &outcome
    else {
        panic!("expected an update outcome");
    };
    assert_eq!(previous, &earth);
    assert_eq!(current, &terra);

    // Applying the event stream alone reaches the authority's own listing.
    for event in authority.events() {
        let CatalogChange::Updated { previous, current } = &event.change else {
            panic!("expected one update event, got {:?}", event.change);
        };
        let row = cached
            .iter_mut()
            .find(|identity| identity.as_ref() == Some(previous))
            .expect("the retired row is there to remove");
        *row = Some(current.clone());
    }
    let authoritative: Vec<_> = entries(&authority)
        .into_iter()
        .map(|entry| entry.identity)
        .collect();
    assert_eq!(cached, authoritative);
}

#[test]
fn a_second_editor_working_from_a_stale_revision_is_refused() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let read_at = authority.revision();

    authority
        .submit(envelope("first", "editor_a", create_kuiper_body("1.0e21")))
        .unwrap();

    let error = authority
        .submit(envelope("second", "editor_b", create_kuiper_body("9.0e21")).guarded_by(read_at))
        .unwrap_err();
    assert!(matches!(error, CatalogRejection::RevisionConflict { .. }));
}

#[test]
fn validating_authored_text_reports_diagnostics_without_writing_anything() {
    let directory = workspace();
    let mut authority = authority(directory.path());
    let before = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();

    let outcome = authority
        .submit(envelope(
            "validate",
            "researcher",
            CatalogCommand::Validate {
                text: "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata: {catalog: \
                       planets, name: probe}\nspec:\n  helpers:\n    x: {expression: \"1\", unit: \
                       furlong}\n"
                    .to_owned(),
            },
        ))
        .unwrap();

    let CatalogOutcome::Validated { revision, reports } = outcome else {
        panic!("expected a validation outcome");
    };
    assert_eq!(revision, CatalogRevision::INITIAL);
    assert!(matches!(
        reports.as_slice(),
        [ValidationReport { diagnostics, .. }] if diagnostics.len() == 1
    ));
    assert_eq!(
        fs::read_to_string(directory.path().join("planets.yaml")).unwrap(),
        before
    );
}

#[test]
fn a_reload_after_an_external_edit_changes_only_the_catalog_projection() {
    let directory = workspace();
    let mut authority = authority(directory.path());

    // Instantiate first, then edit the file behind the authority's back.
    let object = authority
        .instantiate(&InstantiationRequest::new(
            identity("planets", "earth"),
            Namespace::new("objects.o1"),
        ))
        .unwrap();
    let materialized = object.resolve_standalone().unwrap();

    let text = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();
    fs::write(
        directory.path().join("planets.yaml"),
        text.replace("5.97e+24", "1.0"),
    )
    .unwrap();

    authority
        .submit(envelope("reload", "watcher", CatalogCommand::Reload))
        .unwrap();

    assert_eq!(authority.revision(), CatalogRevision::INITIAL.next());
    assert_eq!(
        object.resolve_standalone().unwrap(),
        materialized,
        "reload never reaches an object that was already materialised"
    );
}

#[test]
fn an_entry_summary_carries_what_a_browser_needs_to_show_and_to_guard_an_edit() {
    let directory = workspace();
    let authority = authority(directory.path());
    let CatalogView::Entry { entry, revision } =
        authority.query(&CatalogQuery::Get(identity("scenarios", "scaled_earth")))
    else {
        panic!("expected an entry view");
    };
    let summary = entry.expect("scaled_earth is loaded");

    assert_eq!(revision, CatalogRevision::INITIAL);
    assert_eq!(summary.state, "available");
    assert!(
        summary.fingerprint.is_some(),
        "an edit can be guarded by it"
    );
    assert_eq!(
        summary.description.as_deref(),
        Some("An Earth-mass body whose mass and radius scale together")
    );
    assert_eq!(
        summary.parameters,
        vec![kagami_catalog::ParameterName::new("mass_fraction").unwrap()]
    );
    assert!(
        summary
            .components
            .contains(&component("kagami.mass_sources", "inertial_mass")),
        "{:?}",
        summary.components
    );
    assert!(summary.diagnostics.is_empty());
    assert!(summary.unavailable.is_empty());
}

#[test]
fn an_unavailable_entry_reports_what_is_missing_rather_than_disappearing() {
    let directory = workspace();
    let authority = CatalogAuthority::open(
        directory.path(),
        support::mass_only_registry(),
        Limits::DEFAULT,
    );
    let summaries = entries(&authority);
    let electron = summaries
        .iter()
        .find(|summary| summary.identity == Some(identity("particles", "electron")))
        .expect("the entry is still listed");
    assert_eq!(electron.state, "unavailable");
    assert!(
        electron
            .unavailable
            .iter()
            .any(|reason| reason.contains("charge_source")),
        "{:?}",
        electron.unavailable
    );
}
