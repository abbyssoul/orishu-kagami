//! Format/recovery gates that need no installed provider or execution engine.
use kagami_session::{
    container::{self, ContainerLimits},
    decode_document,
};
use orishu_plugin::archive::{self, Root};
use std::collections::BTreeMap;

const ROOT: &str = include_str!("fixtures/experiment_scientific_v4.json");
fn packed(root: &[u8]) -> Vec<u8> {
    archive::pack(
        Root::Document,
        root,
        &BTreeMap::new(),
        ContainerLimits::default().archive,
    )
    .unwrap()
}

#[test]
fn empty_scientific_draft_has_a_golden_root_without_a_legacy_domain() {
    let bytes = packed(ROOT.as_bytes());
    let document = decode_document(&bytes).unwrap();
    assert!(document.experiment.setup.legacy().is_none());
    assert!(serde_json::to_vec(&document).is_err());
    let encoded = container::encode(&document, ContainerLimits::default()).unwrap();
    let parsed =
        archive::read(&encoded, Root::Document, ContainerLimits::default().archive).unwrap();
    assert_eq!(std::str::from_utf8(parsed.root).unwrap(), ROOT.trim_end());
    assert!(parsed.blobs.is_empty());
    let experiment = document
        .into_experiment(
            &kagami_catalog::SchemaRegistry::new(),
            &kagami_document::Limits::default(),
        )
        .unwrap();
    assert_eq!(experiment.snapshot().object_count(), 0);
    assert!(experiment.snapshot().setup().scientific().is_some());
    // A root stripped of its required container is not silently imported.
    assert_eq!(
        decode_document(ROOT.as_bytes()).unwrap_err().code(),
        "unsupported_format_version"
    );
}

#[test]
fn container_limits_and_unsupported_framing_are_not_recovery_signals() {
    let bytes = packed(ROOT.as_bytes());
    let limits = ContainerLimits {
        archive: archive::ArchiveLimits {
            max_bytes: bytes.len() - 1,
            ..ContainerLimits::default().archive
        },
        ..Default::default()
    };
    assert!(!container::decode(&bytes, limits).unwrap_err().is_damage());
    let mut compressed = bytes.clone();
    compressed[8..10].copy_from_slice(&8u16.to_le_bytes());
    let directory = compressed
        .windows(4)
        .position(|w| w == b"PK\x01\x02")
        .unwrap();
    compressed[directory + 10..directory + 12].copy_from_slice(&8u16.to_le_bytes());
    assert!(!decode_document(&compressed).unwrap_err().is_damage());
    assert!(
        !decode_document(&bytes[..bytes.len() - 1])
            .unwrap_err()
            .is_damage()
    );
    let mut corrupt = bytes;
    corrupt[43] ^= 1;
    assert!(decode_document(&corrupt).unwrap_err().is_damage());
}

#[test]
fn future_view_remains_nonblocking_but_unknown_scientific_version_does_not() {
    let mut root: serde_json::Value = serde_json::from_str(ROOT).unwrap();
    root["defaultView"] = serde_json::json!({"version": 99, "future": [1, 2, 3]});
    let document = decode_document(&packed(&serde_json::to_vec(&root).unwrap())).unwrap();
    assert_eq!(
        document
            .default_view
            .as_ref()
            .unwrap()
            .decode()
            .unwrap_err()
            .code(),
        "unsupported_view_version"
    );
    let encoded = container::encode(&document, ContainerLimits::default()).unwrap();
    assert_eq!(
        decode_document(&encoded).unwrap().default_view,
        document.default_view
    );
    root["experiment"]["setup"]["apiVersion"] = "kagami.scientific-setup/v99".into();
    assert!(
        !decode_document(&packed(&serde_json::to_vec(&root).unwrap()))
            .unwrap_err()
            .is_damage()
    );
}
