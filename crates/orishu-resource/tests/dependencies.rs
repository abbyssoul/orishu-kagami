//! The envelope's dependency budget is part of its contract, so it is a test.
//!
//! This crate is depended on by both an Orishu domain crate and a Kagami
//! authoring crate. Anything it acquires, both acquire — which is exactly how
//! a shared "small" type ends up dragging a runtime across a boundary that was
//! supposed to separate them. Reviewing imports catches a direct dependency;
//! it does not catch a transitive `tokio` or `iced` arriving through something
//! innocuous. Resolving the real graph does.

use std::collections::{BTreeSet, HashMap};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

/// Crates that would give the envelope a capability it must not have.
///
/// Each entry names the capability, not just the crate, because the reason is
/// what a future contributor needs.
const FORBIDDEN: &[(&str, &str)] = &[
    // Domains. The envelope is shared *by* these; depending on one would make
    // the shape belong to whichever domain won.
    ("orishu", "Orishu domain models and client interfaces"),
    ("orishu-identity", "cluster and node identity"),
    ("orishu-membership", "cluster membership"),
    ("orishu-variables", "expressions and units"),
    ("kagami-catalog", "Kagami's object-template catalog"),
    ("kagami-document", "Kagami's experiment model"),
    ("kagami-session", "Kagami's document server"),
    ("kagami-renderer", "Kagami's rendering boundary"),
    // Applications. A library never depends on a deployable.
    ("orishu-worker", "the worker application"),
    ("orishu-ctl", "the operator CLI application"),
    ("orishu-monitor", "the operator TUI application"),
    ("kagami", "the Kagami application"),
    // Capabilities the envelope has no business holding.
    ("iced", "UI toolkit"),
    ("iced_core", "UI toolkit"),
    ("wgpu", "GPU presentation"),
    ("winit", "windowing"),
    ("rfd", "native file dialogs"),
    ("tokio", "async runtime"),
    ("quinn", "QUIC transport"),
    ("rustls", "TLS"),
    ("reqwest", "HTTP client"),
    ("hyper", "HTTP"),
    ("salvo", "HTTP server"),
    ("tempfile", "filesystem access"),
    ("glam", "solver/renderer linear algebra"),
    ("chrono", "wall-clock time"),
    // Codecs. Choosing JSON or YAML is the consuming domain's decision; this
    // crate describes a shape and lets the caller bring the bytes.
    ("serde_json", "a concrete codec"),
    ("serde_yaml", "a concrete codec"),
];

fn metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .exec()
        .expect("cargo metadata must succeed inside the workspace")
}

/// The crates `orishu-resource` actually links against, excluding
/// dev-dependencies, which exist only inside the test and doc-example
/// binaries.
fn runtime_dependency_names(metadata: &Metadata) -> BTreeSet<String> {
    let resolve = metadata
        .resolve
        .as_ref()
        .expect("resolved dependency graph");
    let by_id: HashMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect();
    let nodes: HashMap<_, _> = resolve
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect();

    let root = metadata
        .packages
        .iter()
        .find(|package| package.name.as_ref() == "orishu-resource")
        .expect("this crate is a workspace member");

    let mut seen = BTreeSet::new();
    let mut stack = vec![root.id.clone()];
    let mut names = BTreeSet::new();

    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(node) = nodes.get(&id) else { continue };
        for dependency in &node.deps {
            let is_runtime = dependency
                .dep_kinds
                .iter()
                .any(|kind| matches!(kind.kind, DependencyKind::Normal | DependencyKind::Build));
            if !is_runtime {
                continue;
            }
            if let Some(package) = by_id.get(&dependency.pkg) {
                names.insert(package.name.to_string());
            }
            stack.push(dependency.pkg.clone());
        }
    }

    names
}

/// The dependencies declared directly, rather than reached through the graph.
fn direct_runtime_dependency_names(metadata: &Metadata) -> BTreeSet<String> {
    metadata
        .packages
        .iter()
        .find(|package| package.name.as_ref() == "orishu-resource")
        .expect("this crate is a workspace member")
        .dependencies
        .iter()
        .filter(|dependency| {
            matches!(
                dependency.kind,
                DependencyKind::Normal | DependencyKind::Build
            )
        })
        .map(|dependency| dependency.name.clone())
        .collect()
}

#[test]
fn the_envelope_has_no_application_ui_transport_or_domain_dependency() {
    let resolved = runtime_dependency_names(&metadata());
    let violations: Vec<String> = FORBIDDEN
        .iter()
        .filter(|(name, _)| resolved.contains(*name))
        .map(|(name, capability)| format!("  {name} — {capability}"))
        .collect();

    assert!(
        violations.is_empty(),
        "orishu-resource must stay a dependency-light shape shared by Orishu and Kagami, but \
         its dependency graph now includes:\n{}\n\
         Both domains inherit whatever this crate acquires; the consuming crate should hold \
         the capability instead.",
        violations.join("\n")
    );
}

#[test]
fn serde_is_the_only_direct_runtime_dependency() {
    let direct = direct_runtime_dependency_names(&metadata());
    assert_eq!(
        direct,
        BTreeSet::from(["serde".to_owned()]),
        "the envelope describes a shape; a second runtime dependency means it started doing \
         something else"
    );
}

#[test]
fn the_dependency_graph_stays_small_enough_to_audit() {
    // Everything resolved should be serde itself and the crates its derive
    // macro needs. A jump past this bound means something with a genuinely new
    // capability arrived.
    let resolved = runtime_dependency_names(&metadata());
    assert!(
        resolved.len() <= 12,
        "the envelope resolved {} runtime dependencies, which is more than serde and its derive \
         macro should need: {resolved:?}",
        resolved.len()
    );
}

#[test]
fn the_expected_dependency_is_present() {
    // A guard against every assertion above passing because metadata
    // resolution returned nothing.
    let resolved = runtime_dependency_names(&metadata());
    assert!(
        resolved.contains("serde"),
        "expected `serde` in the resolved graph, got {resolved:?}"
    );
}
