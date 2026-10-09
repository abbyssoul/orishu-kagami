//! Scoped provider intent is authored content, not a catalog/inventory link.
use kagami_catalog::{document::*, *};
use orishu_plugin::{
    ContributionRef,
    authoring_lock::{LockLimits, SelectionLock},
    resolution::{ProviderBinding, RequirementKey, Selection},
};
use std::{path::Path, sync::Arc};

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
fn document(provider: u8) -> TemplateDocument {
    let root = component().contribution().unwrap().clone();
    let provider = reference(provider, "orishu.model.constants/v1");
    let mut contributions = vec![root.clone(), provider.clone()];
    contributions.sort();
    new(
        MetadataDocument::new("test".try_into().unwrap(), "object".try_into().unwrap()),
        SpecDocument {
            dependencies: Some(Arc::new(
                SelectionLock::new(
                    Selection {
                        roots: vec![root.clone()],
                        contributions,
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
            )),
            components: vec![ComponentDocument {
                component_type: component(),
                name: None,
                properties: Default::default(),
            }],
            ..Default::default()
        },
    )
}
fn registry() -> SchemaRegistry {
    SchemaRegistry::new().with(ComponentSchema::new(component(), SchemaVersion(1)))
}
fn loaded(text: &str, registry: &SchemaRegistry, limits: Limits) -> CatalogSet {
    resolve(
        parse_stream(Path::new("test.yaml"), text, &limits).documents,
        vec![],
        registry,
        &limits,
    )
}

#[test]
fn locked_template_roundtrips_fingerprints_and_materializes_without_inventory() {
    let doc = document(2);
    assert_eq!(doc.api_version().as_str(), LOCKED_API_VERSION);
    let template = Template::from_document(&doc, &Limits::default()).unwrap();
    let text = String::from_utf8(template.canonical_bytes()).unwrap();
    let roundtrip: TemplateDocument = serde_yaml::from_str(&text).unwrap();
    assert_eq!(roundtrip, doc);
    assert_eq!(
        Template::from_document(&roundtrip, &Limits::default()).unwrap(),
        template
    );
    let set = loaded(&text, &registry(), Limits::default());
    let entry = set.get(&template.identity).unwrap();
    assert!(entry.result.is_available());
    let candidate = materialize(
        &set,
        &registry(),
        &InstantiationRequest::new(
            template.identity.clone(),
            orishu_variables::Namespace::new("objects.test"),
        ),
    )
    .unwrap();
    assert_eq!(candidate.dependencies, template.dependencies);
    assert_eq!(candidate.provenance.api_version, LOCKED_API_VERSION);
    let different = loaded(
        &serde_yaml::to_string(&document(3)).unwrap(),
        &registry(),
        Limits::default(),
    );
    assert_ne!(
        entry.fingerprint,
        different.get(&template.identity).unwrap().fingerprint
    );
    let mut stale = InstantiationRequest::new(
        template.identity.clone(),
        orishu_variables::Namespace::new("objects.test"),
    );
    stale.expected_fingerprint = entry.fingerprint;
    assert!(materialize(&different, &registry(), &stale).is_err());
    let offline = loaded(&text, &SchemaRegistry::new(), Limits::default());
    let entry = offline.get(&template.identity).unwrap();
    assert!(!entry.result.is_available());
    assert_eq!(
        entry.result.template().unwrap().dependencies,
        template.dependencies
    );
    assert_eq!(candidate.resolve_standalone().unwrap().len(), 0);
    let mut unlocked = doc.spec.clone();
    unlocked.dependencies = None;
    let unlocked = new(doc.metadata.clone(), unlocked);
    let unlocked = loaded(
        &serde_yaml::to_string(&unlocked).unwrap(),
        &registry(),
        Limits::default(),
    );
    assert_eq!(
        unlocked
            .get(&template.identity)
            .unwrap()
            .provenance()
            .unwrap()
            .api_version,
        PINNED_API_VERSION
    );
}

#[test]
fn writer_persists_choices_and_stale_updates_cannot_replace_them() {
    let directory = tempfile::tempdir().unwrap();
    let root = CatalogRoot::new(directory.path());
    let file = Path::new("test.yaml");
    let initial = Template::from_document(&document(2), &Limits::default()).unwrap();
    write::create_entry(&root, file, &initial, None).unwrap();
    let set = load_directory(directory.path(), &registry(), &Limits::default());
    assert_eq!(
        set.get(&initial.identity).unwrap().result.template(),
        Some(&initial)
    );
    let digest = write::file_digest(&root, file).unwrap().unwrap();
    let updated = Template::from_document(&document(3), &Limits::default()).unwrap();
    let target = WriteTarget {
        file: file.into(),
        document: DocumentOrdinal::from_index(0),
        identity: initial.identity.clone(),
    };
    write::update_entry(&root, &target, &updated, digest).unwrap();
    assert!(write::update_entry(&root, &target, &initial, digest).is_err());
    let set = load_directory(directory.path(), &registry(), &Limits::default());
    assert_eq!(
        set.get(&initial.identity).unwrap().result.template(),
        Some(&updated)
    );
}

#[test]
fn direct_typed_templates_cannot_exceed_the_embedded_readers_ceiling() {
    let mut doc = document(2);
    let mut members = vec![component().contribution().unwrap().clone()];
    members.extend((2..=33).map(|n| reference(n, "orishu.model.constants/v1")));
    let bindings = members
        .windows(2)
        .map(|pair| ProviderBinding {
            requirement: RequirementKey {
                consumer: pair[0].clone(),
                slot: "next".parse().unwrap(),
            },
            provider: pair[1].clone(),
        })
        .collect();
    let generous = LockLimits {
        dependency_depth: 64,
        ..LockLimits::default()
    };
    doc.spec.dependencies = Some(Arc::new(
        SelectionLock::new(
            Selection {
                roots: vec![members[0].clone()],
                contributions: members,
                bindings,
            },
            generous,
        )
        .unwrap(),
    ));
    let limits = Limits {
        dependencies: generous,
        ..Limits::default()
    };
    assert!(Template::from_document(&doc, &limits).is_err());
    let text = serde_yaml::to_string(&doc).unwrap();
    assert!(serde_yaml::from_str::<TemplateDocument>(&text).is_err());
    assert!(matches!(
        loaded(&text, &registry(), limits).entries()[0].result,
        LoadResult::Invalid { .. }
    ));
}

#[test]
fn versions_roots_null_and_receiving_limits_cannot_discard_intent() {
    let doc = document(2);
    let text = serde_yaml::to_string(&doc).unwrap();
    for version in [API_VERSION, PINNED_API_VERSION, "kagami.catalog/v99"] {
        let old: TemplateDocument =
            serde_yaml::from_str(&text.replace(LOCKED_API_VERSION, version)).unwrap();
        assert!(Template::from_document(&old, &Limits::default()).is_err());
    }
    let mut no_roots = doc.clone();
    no_roots.spec.components.clear();
    assert!(Template::from_document(&no_roots, &Limits::default()).is_err());
    let mut missing = doc.clone();
    missing.spec.dependencies = None;
    assert!(Template::from_document(&missing, &Limits::default()).is_err());
    assert!(serde_yaml::from_str::<TemplateDocument>("apiVersion: kagami.catalog/v3\nkind: ObjectTemplate\nmetadata: {catalog: test, name: object}\nspec: {dependencies: null}").is_err());
    for bounds in [
        LockLimits {
            items: 1,
            ..LockLimits::default()
        },
        LockLimits {
            bytes: 1,
            ..LockLimits::default()
        },
        LockLimits {
            work: 1,
            ..LockLimits::default()
        },
        LockLimits {
            dependency_depth: 1,
            ..LockLimits::default()
        },
    ] {
        let limits = Limits {
            dependencies: bounds,
            ..Limits::default()
        };
        assert!(Template::from_document(&doc, &limits).is_err());
        let set = loaded(&text, &registry(), limits);
        assert!(matches!(
            set.entries()[0].result,
            LoadResult::Invalid { .. }
        ));
    }
    let too_small = Limits {
        max_file_bytes: 1,
        ..Limits::default()
    };
    let parsed = parse_stream(Path::new("test.yaml"), &text, &too_small);
    assert!(parsed.truncated);
    assert_eq!(parsed.documents.len(), 1);
}
