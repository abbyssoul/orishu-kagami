//! Audit the real normal/build graph, not only source imports.
use cargo_metadata::{DependencyKind, MetadataCommand};
use std::collections::{BTreeSet, HashMap};

#[test]
fn contract_cannot_acquire_consumer_or_io_dependencies() {
    let metadata = MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .other_options(vec!["--locked".into(), "--offline".into()])
        .exec()
        .unwrap();
    let packages: HashMap<_, _> = metadata.packages.iter().map(|p| (&p.id, p)).collect();
    let nodes: HashMap<_, _> = metadata
        .resolve
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .map(|n| (&n.id, n))
        .collect();
    let root = metadata
        .packages
        .iter()
        .find(|p| p.name.as_ref() == "orishu-plugin")
        .unwrap();
    let direct: BTreeSet<_> = root
        .dependencies
        .iter()
        .filter(|d| d.kind != DependencyKind::Development)
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(
        direct,
        BTreeSet::from([
            "orishu-resource",
            "orishu-variables",
            "orishu-workload",
            "serde",
            "serde_json",
            "thiserror"
        ])
    );
    let mut seen = BTreeSet::new();
    let mut stack = vec![&root.id];
    let mut names = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        for dep in &nodes[id].deps {
            if dep
                .dep_kinds
                .iter()
                .any(|k| matches!(k.kind, DependencyKind::Normal | DependencyKind::Build))
            {
                names.insert(packages[&dep.pkg].name.as_str());
                stack.push(&dep.pkg);
            }
        }
    }
    for forbidden in [
        "orishu",
        "orishu-membership",
        "orishu-identity",
        "kagami-catalog",
        "kagami-document",
        "kagami-session",
        "kagami-renderer",
        "orishu-worker",
        "tokio",
        "reqwest",
        "hyper",
        "quinn",
        "rustls",
        "salvo",
        "tempfile",
        "iced",
        "wgpu",
        "winit",
        "chrono",
        "wasmtime",
        "zip",
        "rand",
        "dhat",
    ] {
        assert!(
            !names.contains(forbidden),
            "forbidden dependency {forbidden}"
        );
    }
    for required in [
        "orishu-workload",
        "orishu-resource",
        "orishu-variables",
        "sha2",
        "serde",
    ] {
        assert!(names.contains(required));
    }
    assert!(
        names.len() <= 36,
        "review expanded dependency graph: {names:?}"
    );
}
