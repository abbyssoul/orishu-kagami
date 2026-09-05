//! The catalog's dependency budget is part of its contract, so it is a test.
//!
//! ADR 0008 requires the catalog domain to stay independent of UI, MCP
//! transport, a solver, and Orishu runtime code, and requires Orishu to have
//! no dependency on Kagami catalog files or APIs at all. Reviewing imports
//! catches a direct dependency; it does not catch a transitive `iced` or
//! `quinn` arriving through something innocuous. Resolving the real graph
//! does.

use std::collections::{BTreeSet, HashMap};

use cargo_metadata::MetadataCommand;

/// Crates that would give the catalog domain a capability it must not have.
///
/// Each entry names the capability, not just the crate, because the reason is
/// what a future contributor needs.
const FORBIDDEN: &[(&str, &str)] = &[
    ("iced", "UI toolkit"),
    ("iced_core", "UI toolkit"),
    ("wgpu", "GPU presentation"),
    ("winit", "windowing"),
    ("rfd", "native file dialogs"),
    ("kagami-renderer", "Kagami's rendering boundary"),
    ("orishu", "Orishu domain and client interfaces"),
    ("orishu-membership", "cluster membership"),
    ("orishu-identity", "cluster identity"),
    ("tokio", "async runtime"),
    ("quinn", "QUIC transport"),
    ("rustls", "TLS"),
    ("reqwest", "HTTP client"),
    ("hyper", "HTTP"),
    ("salvo", "HTTP server"),
    ("glam", "solver/renderer linear algebra"),
];

/// Resolves the crates `kagami-catalog` actually links against, excluding
/// dev-dependencies, which exist only inside the test binary.
fn runtime_dependency_names() -> BTreeSet<String> {
    let metadata = MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .exec()
        .expect("cargo metadata must succeed inside the workspace");

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
        .find(|package| package.name.as_ref() == "kagami-catalog")
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
            let is_runtime = dependency.dep_kinds.iter().any(|kind| {
                matches!(
                    kind.kind,
                    cargo_metadata::DependencyKind::Normal | cargo_metadata::DependencyKind::Build
                )
            });
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

#[test]
fn the_catalog_domain_has_no_ui_transport_or_runtime_dependency() {
    let resolved = runtime_dependency_names();
    let violations: Vec<String> = FORBIDDEN
        .iter()
        .filter(|(name, _)| resolved.contains(*name))
        .map(|(name, capability)| format!("  {name} — {capability}"))
        .collect();

    assert!(
        violations.is_empty(),
        "kagami-catalog must stay a UI-, transport-, and runtime-free authoring domain, but its \
         dependency graph now includes:\n{}\n\
         UI and MCP are adapters over `CatalogAuthority`; they depend on it, never the reverse.",
        violations.join("\n")
    );
}

#[test]
fn the_dependency_graph_stays_small_enough_to_audit() {
    // Larger than the sans-IO membership core's budget because a
    // hand-authored file format needs a YAML parser, and `serde_yaml` brings
    // its own scanner plus `indexmap`. Everything in the graph should still
    // be a serde, hashing, or YAML crate; a jump past this bound means
    // something with a genuinely new capability arrived.
    let resolved = runtime_dependency_names();
    assert!(
        resolved.len() <= 32,
        "the catalog domain resolved {} runtime dependencies, which is more than can be reviewed \
         by hand: {resolved:?}",
        resolved.len()
    );
}

#[test]
fn the_expected_direct_dependencies_are_present() {
    // A guard against this test silently passing because metadata resolution
    // returned nothing.
    let resolved = runtime_dependency_names();
    for expected in [
        "orishu-variables",
        "serde",
        "serde_yaml",
        "serde_path_to_error",
        "sha2",
        "thiserror",
    ] {
        assert!(
            resolved.contains(expected),
            "expected `{expected}` in the resolved graph, got {resolved:?}"
        );
    }
}
