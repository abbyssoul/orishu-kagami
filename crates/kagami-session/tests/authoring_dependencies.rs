//! Provider intent survives the real authority and file boundary without an
//! inventory or scientific setup. Synthetic schemas here test ownership only;
//! exact declaration resolution is exercised in orishu-plugin's resolver tests.
use kagami_catalog::{ComponentSchema, ComponentTypeId, SchemaRegistry, SchemaVersion};
use kagami_document::{DisplayName, ExperimentCommand as Edit, Limits, ObjectSpec};
use kagami_session::{
    ActorId, AuthoringView, CommandId, DocumentAuthority, DocumentMetadata,
    ExperimentCommandEnvelope, ExperimentDocument, SessionCommand,
    container::{self, ContainerLimits},
};
use orishu_plugin::{
    ArtifactDigest, ContributionRef,
    archive::{self, Root},
    authoring_lock::{LockLimits, SelectionLock},
    resolution::{ProviderBinding, RequirementKey, Selection},
};
use std::{collections::BTreeMap, sync::Arc};

fn reference(n: u8, point: &str) -> ContributionRef {
    ContributionRef {
        release: format!("sha256:{}", format!("{n:02x}").repeat(32))
            .parse()
            .unwrap(),
        extension_point: point.parse().unwrap(),
        local_id: "test".parse().unwrap(),
    }
}
fn component() -> ComponentTypeId {
    ComponentTypeId::exact(reference(1, "orishu.model.components/v1")).unwrap()
}
fn lock(provider: u8) -> Arc<SelectionLock> {
    let root = component().contribution().unwrap().clone();
    let provider = reference(provider, "orishu.model.constants/v1");
    let mut members = vec![root.clone(), provider.clone()];
    members.sort();
    Arc::new(
        SelectionLock::new(
            Selection {
                roots: vec![root.clone()],
                contributions: members,
                bindings: vec![ProviderBinding {
                    requirement: RequirementKey {
                        consumer: root,
                        slot: "constants".parse().unwrap(),
                    },
                    provider,
                }],
            },
            LockLimits::default(),
        )
        .unwrap(),
    )
}
fn empty() -> Arc<SelectionLock> {
    Arc::new(
        SelectionLock::new(
            Selection {
                roots: vec![],
                contributions: vec![],
                bindings: vec![],
            },
            LockLimits::default(),
        )
        .unwrap(),
    )
}
fn envelope(n: usize, command: SessionCommand) -> ExperimentCommandEnvelope {
    ExperimentCommandEnvelope::new(
        CommandId::new(format!("test-{n}")).unwrap(),
        ActorId::new("test").unwrap(),
        command,
    )
}
fn edit(n: usize, commands: Vec<Edit>) -> ExperimentCommandEnvelope {
    envelope(n, SessionCommand::Edit(commands))
}
fn create() -> Edit {
    Edit::CreateObject(Box::new(
        ObjectSpec::new(DisplayName::new("test").unwrap())
            .with_component(component(), Default::default()),
    ))
}
fn authority(limits: Limits) -> DocumentAuthority {
    DocumentAuthority::new(
        SchemaRegistry::new().with(ComponentSchema::new(component(), SchemaVersion(1))),
        limits,
    )
}

#[test]
fn verified_schema_adoption_and_edit_are_atomic_including_retention_and_replay() {
    let schemas = authority(Limits::default()).schemas().clone();
    let original = lock(2);
    let mut limits = Limits::default();
    limits.scientific.retained_bytes = original.canonical_byte_length() - 1;
    let mut refused = DocumentAuthority::new(SchemaRegistry::new(), limits);
    let command = edit(0, vec![create(), Edit::AdoptDependencies(original.clone())]);
    let before = refused.view();
    let history = refused.history_status();
    let events: Vec<_> = refused.events_since(Default::default()).cloned().collect();
    assert!(
        refused
            .submit_with_schemas(command.clone(), schemas.clone())
            .is_err()
    );
    assert_eq!(refused.view(), before);
    assert_eq!(refused.history_status(), history);
    assert_eq!(
        refused
            .events_since(Default::default())
            .cloned()
            .collect::<Vec<_>>(),
        events
    );
    assert!(refused.schemas().get(&component()).is_none());
    assert_eq!(refused.scientific_retained_bytes().unwrap(), 0);

    let mut accepted = DocumentAuthority::new(SchemaRegistry::new(), Limits::default());
    assert!(
        accepted.submit(command.clone()).is_err(),
        "missing schema cannot authorize attachment"
    );
    let result = accepted
        .submit_with_schemas(command.clone(), schemas.clone())
        .unwrap();
    assert!(!result.replayed);
    assert_eq!(accepted.schemas(), &schemas);
    assert_eq!(accepted.snapshot().dependencies(), Some(&original));
    assert_eq!(accepted.snapshot().object_count(), 1);
    let view = accepted.view();
    let events: Vec<_> = accepted.events_since(Default::default()).cloned().collect();
    assert!(
        accepted
            .submit_with_schemas(command, SchemaRegistry::new())
            .unwrap()
            .replayed
    );
    assert_eq!(accepted.view(), view);
    assert_eq!(
        accepted.schemas(),
        &schemas,
        "a replay cannot replace capabilities"
    );
    assert_eq!(
        accepted
            .events_since(Default::default())
            .cloned()
            .collect::<Vec<_>>(),
        events
    );
    accepted.submit(envelope(1, SessionCommand::Undo)).unwrap();
    assert_eq!(accepted.snapshot().object_count(), 0);
    assert!(accepted.snapshot().dependencies().is_none());
    assert_eq!(
        accepted.schemas(),
        &schemas,
        "capabilities are not experiment history"
    );
    accepted.submit(envelope(2, SessionCommand::Redo)).unwrap();
    assert_eq!(accepted.snapshot().dependencies(), Some(&original));
}
fn captured(authority: &DocumentAuthority) -> ExperimentDocument {
    ExperimentDocument::of(
        authority.experiment(),
        &authority.view().experiment,
        &AuthoringView::default(),
        DocumentMetadata {
            generator: "test".into(),
            created: "test".into(),
            saved: "test".into(),
            saved_revision: authority.revision().get(),
        },
    )
}

fn template_catalog(
    lock: Option<Arc<SelectionLock>>,
    component: ComponentTypeId,
    schemas: &SchemaRegistry,
) -> kagami_catalog::CatalogSet {
    use kagami_catalog::{document::*, *};
    let doc = new(
        MetadataDocument::new("test".try_into().unwrap(), "object".try_into().unwrap()),
        SpecDocument {
            dependencies: lock,
            components: vec![ComponentDocument {
                component_type: component,
                name: None,
                properties: Default::default(),
            }],
            ..Default::default()
        },
    );
    let template = Template::from_document(&doc, &kagami_catalog::Limits::default()).unwrap();
    // Exercise the real catalog YAML encoder, loader and resolver; no fixture IO.
    let text = String::from_utf8(template.canonical_bytes()).unwrap();
    let docs = parse_stream(
        std::path::Path::new("test.yaml"),
        &text,
        &kagami_catalog::Limits::default(),
    );
    resolve(
        docs.documents,
        vec![],
        schemas,
        &kagami_catalog::Limits::default(),
    )
}
fn instantiate(n: usize) -> ExperimentCommandEnvelope {
    envelope(
        n,
        SessionCommand::InstantiateObjectTemplate(Box::new(
            kagami_session::InstantiationSpec::new(
                kagami_catalog::TemplateIdentity::new(
                    "test".try_into().unwrap(),
                    "object".try_into().unwrap(),
                ),
                DisplayName::new("from catalog").unwrap(),
            ),
        )),
    )
}

#[test]
fn catalog_materialization_is_one_locked_revision_with_offline_persistence_and_undo() {
    let original = lock(2);
    let mut authority = authority(Limits::default());
    authority.adopt_catalog(template_catalog(
        Some(original.clone()),
        component(),
        authority.schemas(),
    ));
    let request = instantiate(0);
    authority.submit(request.clone()).unwrap();
    assert_eq!(authority.revision().get(), 1);
    assert_eq!(authority.snapshot().dependencies(), Some(&original));
    assert_eq!(authority.snapshot().object_count(), 1);
    assert!(authority.submit(request).unwrap().replayed);
    let bytes = container::encode(&captured(&authority), ContainerLimits::default()).unwrap();
    let offline = kagami_session::decode_document(&bytes)
        .unwrap()
        .into_experiment(&SchemaRegistry::new(), &Limits::default())
        .unwrap();
    assert_eq!(offline.snapshot().dependencies(), Some(&original));
    assert_eq!(offline.snapshot().object_count(), 1);
    let provider = reference(2, "orishu.model.constants/v1").release;
    assert!(
        authority
            .plugin_references(Default::default())
            .unwrap()
            .releases[&provider]
            .current
    );
    // The catalog is no longer present. Undo/redo restore object and lock, not
    // whatever a template or inventory happens to contain now.
    authority.adopt_catalog(template_catalog(None, component(), authority.schemas()));
    authority.submit(envelope(1, SessionCommand::Undo)).unwrap();
    assert!(authority.snapshot().dependencies().is_none());
    assert_eq!(authority.snapshot().object_count(), 0);
    assert!(
        authority
            .plugin_references(Default::default())
            .unwrap()
            .releases[&provider]
            .history
    );
    authority.submit(envelope(2, SessionCommand::Redo)).unwrap();
    assert_eq!(authority.snapshot().dependencies(), Some(&original));
    assert_eq!(authority.snapshot().object_count(), 1);
}

#[test]
fn catalog_merge_never_overwrites_choices_or_invents_unlocked_roots() {
    let mut authority = authority(Limits::default());
    authority
        .submit(edit(0, vec![create(), Edit::AdoptDependencies(lock(2))]))
        .unwrap();
    authority.adopt_catalog(template_catalog(
        Some(lock(3)),
        component(),
        authority.schemas(),
    ));
    let before = authority.view();
    let counters = authority.experiment().counters();
    let error = authority.submit(instantiate(1)).unwrap_err();
    assert!(error.to_string().contains("conflict"), "{error}");
    assert_eq!(authority.view(), before);
    assert_eq!(authority.experiment().counters(), counters);

    let other = ComponentTypeId::exact(reference(4, "orishu.model.components/v1")).unwrap();
    let mut authority = DocumentAuthority::new(
        authority
            .schemas()
            .clone()
            .with(ComponentSchema::new(other.clone(), SchemaVersion(1))),
        Limits::default(),
    );
    authority
        .submit(edit(
            0,
            vec![Edit::CreateObject(Box::new(
                ObjectSpec::new(DisplayName::new("unlocked").unwrap())
                    .with_component(other, Default::default()),
            ))],
        ))
        .unwrap();
    authority.adopt_catalog(template_catalog(
        Some(lock(2)),
        component(),
        authority.schemas(),
    ));
    let before = authority.view();
    assert!(
        authority.submit(instantiate(1)).is_err(),
        "unknown existing dependency choices must be resolved explicitly"
    );
    assert_eq!(authority.view(), before);
}

#[test]
fn template_lock_obeys_aggregate_retention_and_reuses_unchanged_intent() {
    let original = lock(2);
    let mut limits = Limits::default();
    limits.scientific.retained_bytes = original.canonical_byte_length() - 1;
    let mut refused = authority(limits);
    refused.adopt_catalog(template_catalog(
        Some(original.clone()),
        component(),
        refused.schemas(),
    ));
    let before = refused.view();
    assert!(refused.submit(instantiate(0)).is_err());
    assert_eq!(refused.view(), before);
    assert_eq!(refused.scientific_retained_bytes().unwrap(), 0);
    limits.scientific.retained_bytes += 1;
    let mut accepted = authority(limits);
    accepted.adopt_catalog(template_catalog(
        Some(original),
        component(),
        accepted.schemas(),
    ));
    accepted.submit(instantiate(0)).unwrap();
    let initial = accepted.snapshot().dependencies().unwrap().clone();
    accepted.submit(instantiate(1)).unwrap();
    assert_eq!(accepted.snapshot().object_count(), 2);
    assert!(Arc::ptr_eq(
        &initial,
        accepted.snapshot().dependencies().unwrap()
    ));
    assert_eq!(
        accepted.scientific_retained_bytes().unwrap(),
        initial.canonical_byte_length()
    );
}

#[test]
fn explicit_scoped_removal_preserves_the_remaining_graph_and_is_undoable() {
    let mut authority = authority(Limits::default());
    authority.adopt_catalog(template_catalog(
        Some(lock(2)),
        component(),
        authority.schemas(),
    ));
    authority.submit(instantiate(0)).unwrap();
    let before = authority.snapshot();
    let object = *before.objects().keys().next().unwrap();
    let pruned = Arc::new(
        before
            .dependencies()
            .unwrap()
            .for_roots(&[], LockLimits::default())
            .unwrap(),
    );
    authority
        .submit(edit(
            1,
            vec![Edit::RemoveObject(object), Edit::AdoptDependencies(pruned)],
        ))
        .unwrap();
    assert_eq!(authority.snapshot().object_count(), 0);
    assert!(
        authority
            .snapshot()
            .dependencies()
            .unwrap()
            .selection()
            .contributions
            .is_empty()
    );
    authority.submit(envelope(2, SessionCommand::Undo)).unwrap();
    assert_eq!(authority.snapshot().dependencies(), before.dependencies());
    assert_eq!(authority.snapshot().object_count(), 1);
}

#[test]
fn disjoint_template_roots_merge_and_prune_without_losing_a_shared_provider() {
    let original = lock(2);
    let other = ComponentTypeId::exact(reference(4, "orishu.model.components/v1")).unwrap();
    let mut selection = original.selection().clone();
    selection.roots = vec![other.contribution().unwrap().clone()];
    selection
        .contributions
        .retain(|c| c != component().contribution().unwrap());
    selection
        .contributions
        .push(other.contribution().unwrap().clone());
    selection.contributions.sort();
    selection.bindings[0].requirement.consumer = other.contribution().unwrap().clone();
    let incoming = Arc::new(SelectionLock::new(selection, LockLimits::default()).unwrap());
    let schemas = SchemaRegistry::new()
        .with(ComponentSchema::new(component(), SchemaVersion(1)))
        .with(ComponentSchema::new(other.clone(), SchemaVersion(1)));
    let mut authority = DocumentAuthority::new(schemas, Limits::default());
    authority
        .submit(edit(
            0,
            vec![create(), Edit::AdoptDependencies(original.clone())],
        ))
        .unwrap();
    authority.adopt_catalog(template_catalog(
        Some(incoming.clone()),
        other,
        authority.schemas(),
    ));
    authority.submit(instantiate(1)).unwrap();
    let snapshot = authority.snapshot();
    let merged = snapshot.dependencies().unwrap();
    assert_eq!(
        **merged,
        original.merge(&incoming, LockLimits::default()).unwrap()
    );
    assert_eq!(merged.selection().contributions.len(), 3);
    let prune = Arc::new(
        merged
            .for_roots(original.selection().roots.as_slice(), LockLimits::default())
            .unwrap(),
    );
    assert_eq!(prune, original);
    let object = *snapshot.objects().keys().last().unwrap();
    authority
        .submit(edit(
            2,
            vec![Edit::RemoveObject(object), Edit::AdoptDependencies(prune)],
        ))
        .unwrap();
    assert_eq!(authority.snapshot().dependencies(), Some(&original));
    assert_eq!(authority.snapshot().object_count(), 1);
    let provider = reference(2, "orishu.model.constants/v1").release;
    assert!(
        authority
            .plugin_references(Default::default())
            .unwrap()
            .releases[&provider]
            .current
    );
}
fn root_and_blobs(bytes: &[u8]) -> (serde_json::Value, BTreeMap<ArtifactDigest, Vec<u8>>) {
    let archive = archive::read(bytes, Root::Document, ContainerLimits::default().archive).unwrap();
    (
        serde_json::from_slice(archive.root).unwrap(),
        archive
            .blobs
            .iter()
            .map(|(id, b)| (*id, b.to_vec()))
            .collect(),
    )
}
fn pack(root: &serde_json::Value, blobs: &BTreeMap<ArtifactDigest, Vec<u8>>) -> Vec<u8> {
    let borrowed = blobs.iter().map(|(id, b)| (*id, b.as_slice())).collect();
    archive::pack(
        Root::Document,
        &serde_json::to_vec(root).unwrap(),
        &borrowed,
        ContainerLimits::default().archive,
    )
    .unwrap()
}

#[test]
fn atomic_edit_undo_redo_and_receipts_retain_dependency_only_provider() {
    let mut authority = authority(Limits::default());
    let original = lock(2);
    let request = edit(0, vec![create(), Edit::AdoptDependencies(original.clone())]);
    authority.submit(request.clone()).unwrap();
    assert!(authority.view().dirty);
    assert!(authority.submit(request).unwrap().replayed);
    assert_eq!(authority.view().experiment.dependencies(), Some(&original));
    assert_eq!(
        authority.scientific_retained_bytes().unwrap(),
        original.canonical_byte_length()
    );
    let release = reference(2, "orishu.model.constants/v1").release;
    let uses = authority
        .plugin_references(Default::default())
        .unwrap()
        .releases[&release];
    assert!(uses.current && uses.receipts && !uses.history);
    authority.submit(envelope(1, SessionCommand::Undo)).unwrap();
    assert!(authority.view().experiment.dependencies().is_none());
    let uses = authority
        .plugin_references(Default::default())
        .unwrap()
        .releases[&release];
    assert!(!uses.current && uses.history && uses.receipts);
    authority.submit(envelope(2, SessionCommand::Redo)).unwrap();
    assert_eq!(authority.view().experiment.dependencies(), Some(&original));
    let object = *authority.view().experiment.objects().keys().next().unwrap();
    let before = authority.view();
    assert!(
        authority
            .submit(edit(3, vec![Edit::RemoveObject(object)]))
            .is_err(),
        "component membership changes must carry updated intent"
    );
    assert_eq!(authority.view(), before);
    authority
        .submit(edit(
            4,
            vec![Edit::RemoveObject(object), Edit::AdoptDependencies(empty())],
        ))
        .unwrap();
    let uses = authority
        .plugin_references(Default::default())
        .unwrap()
        .releases[&release];
    assert!(!uses.current && uses.history && uses.receipts);
}

#[test]
fn rejection_preserves_identity_revision_history_and_retention() {
    let original = lock(2);
    let mut limits = Limits::default();
    limits.scientific.retained_bytes = original.canonical_byte_length();
    let mut authority = authority(limits);
    authority
        .submit(edit(
            0,
            vec![create(), Edit::AdoptDependencies(original.clone())],
        ))
        .unwrap();
    let before = authority.view();
    assert!(
        authority
            .submit(edit(1, vec![Edit::AdoptDependencies(lock(3))]))
            .is_err()
    );
    assert_eq!(authority.view(), before);
    assert_eq!(
        authority.scientific_retained_bytes().unwrap(),
        original.canonical_byte_length()
    );
    let count = authority.experiment().counters();
    assert!(
        authority
            .submit(edit(2, vec![create(), Edit::AdoptDependencies(empty())]))
            .is_err()
    );
    assert_eq!(authority.experiment().counters(), count);
    assert_eq!(authority.view(), before);
    let mut tight = Limits::default();
    tight.dependencies.bytes = original.canonical_byte_length() - 1;
    let mut receiver = self::authority(tight);
    assert!(
        receiver
            .submit(envelope(
                0,
                SessionCommand::Open {
                    experiment: Box::new(authority.experiment().clone()),
                    target: None,
                    discard_unsaved: true
                }
            ))
            .is_err()
    );
    assert_eq!(receiver.view().experiment.object_count(), 0);
}

#[test]
fn v5_roundtrip_is_offline_complete_and_never_serializes_as_old_json() {
    let mut authority = authority(Limits::default());
    authority
        .submit(edit(0, vec![create(), Edit::AdoptDependencies(lock(2))]))
        .unwrap();
    let document = captured(&authority);
    assert!(serde_json::to_vec(&document).is_err());
    let bytes = container::encode(&document, ContainerLimits::default()).unwrap();
    let (root, blobs) = root_and_blobs(&bytes);
    assert_eq!(root["formatVersion"], 5);
    assert_eq!(root["experiment"]["setup"]["kind"], "legacy");
    assert_eq!(
        blobs.len(),
        1,
        "no code, scientific fields or installation evidence"
    );
    let restored = kagami_session::decode_document(&bytes).unwrap();
    assert_eq!(restored, document);
    assert_eq!(
        container::encode(&restored, ContainerLimits::default()).unwrap(),
        bytes
    );
    let offline = restored
        .into_experiment(&SchemaRegistry::new(), &Limits::default())
        .unwrap();
    assert_eq!(
        offline.snapshot().dependencies(),
        authority.view().experiment.dependencies()
    );
    assert!(offline.snapshot().setup().scientific().is_none());
    let wire = kagami_document::WireSnapshot::of(&offline.snapshot());
    assert_eq!(wire.version, 4);
    assert_eq!(wire.dependencies.unwrap(), lock(2).describe());
    let mut receiver = DocumentAuthority::new(SchemaRegistry::new(), Limits::default());
    receiver
        .submit(envelope(
            0,
            SessionCommand::Open {
                experiment: Box::new(offline),
                target: None,
                discard_unsaved: true,
            },
        ))
        .unwrap();
    assert!(!receiver.view().dirty);
    assert!(
        receiver
            .plugin_references(Default::default())
            .unwrap()
            .releases
            .contains_key(&reference(2, "orishu.model.constants/v1").release)
    );
}

#[test]
fn hostile_v5_metadata_and_blobs_cannot_drop_or_replace_intent() {
    let mut authority = authority(Limits::default());
    authority
        .submit(edit(0, vec![create(), Edit::AdoptDependencies(lock(2))]))
        .unwrap();
    let bytes = container::encode(&captured(&authority), ContainerLimits::default()).unwrap();
    let (root, blobs) = root_and_blobs(&bytes);
    let mut no_lock = root.clone();
    no_lock.as_object_mut().unwrap().remove("dependencies");
    assert!(kagami_session::decode_document(&pack(&no_lock, &blobs)).is_err());
    let mut null = root.clone();
    null["dependencies"] = serde_json::Value::Null;
    assert!(kagami_session::decode_document(&pack(&null, &blobs)).is_err());
    let mut length = root.clone();
    length["dependencies"]["byteLength"] = serde_json::json!(1);
    assert!(kagami_session::decode_document(&pack(&length, &blobs)).is_err());
    assert!(kagami_session::decode_document(&pack(&root, &BTreeMap::new())).is_err());
    let mut extra = blobs.clone();
    extra.insert(
        ArtifactDigest::sha256_of(b"unrelated"),
        b"unrelated".to_vec(),
    );
    assert!(kagami_session::decode_document(&pack(&root, &extra)).is_err());
    let mut future = root.clone();
    future["formatVersion"] = serde_json::json!(6);
    assert!(
        !kagami_session::decode_document(&pack(&future, &blobs))
            .unwrap_err()
            .is_damage()
    );
    let mut old = root.clone();
    old["formatVersion"] = serde_json::json!(4);
    assert!(kagami_session::decode_document(&pack(&old, &blobs)).is_err());
    let mut tight = ContainerLimits::default();
    tight.dependencies.items = 1;
    assert!(!container::decode(&bytes, tight).unwrap_err().is_damage());
    // A valid replacement lock blob still cannot change the object roots.
    let replacement = empty().to_cbor(LockLimits::default()).unwrap();
    let digest = ArtifactDigest::sha256_of(&replacement);
    let mut changed = root.clone();
    changed["dependencies"] =
        serde_json::json!({"digest": digest, "byteLength": replacement.len()});
    let changed =
        kagami_session::decode_document(&pack(&changed, &BTreeMap::from([(digest, replacement)])))
            .unwrap();
    assert!(
        changed
            .into_experiment(&SchemaRegistry::new(), &Limits::default())
            .is_err()
    );
}
