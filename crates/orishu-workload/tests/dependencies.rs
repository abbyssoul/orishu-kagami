//! The workload domain's dependency budget is part of its contract, so it is a
//! test.
//!
//! This crate is the one definition of what a workload is. Kagami's authoring
//! crates compile experiments into it and Orishu's worker admits it, so it sits
//! below both: anything it acquires, both acquire. `crates/orishu` in
//! particular links `reqwest`, `tokio`, `http` and `chrono`, and Kagami's
//! library crates deliberately depend on none of them — which is the whole
//! reason this model does not live there.
//!
//! Reviewing imports catches a direct dependency. It does not catch a
//! transitive `tokio` arriving through something innocuous. Resolving the real
//! graph does.

use std::collections::{BTreeSet, HashMap};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

/// This crate, as `cargo metadata` names it.
const CRATE: &str = "orishu-workload";

/// Crates that would give the workload model a capability it must not have.
///
/// Each entry names the capability, not just the crate, because the reason is
/// what a future contributor needs.
const FORBIDDEN: &[(&str, &str)] = &[
    // The consuming domains. Depending on either would make the shared
    // definition belong to whichever one won, and would re-create the
    // "two models that can disagree" problem this crate exists to remove.
    ("orishu", "Orishu's client interfaces and prototype models"),
    ("orishu-membership", "cluster membership"),
    ("kagami-catalog", "Kagami's object-template catalog"),
    ("kagami-document", "Kagami's experiment model"),
    ("kagami-session", "Kagami's document server"),
    ("kagami-renderer", "Kagami's rendering boundary"),
    // Applications. A library never depends on a deployable.
    ("orishu-worker", "the worker application"),
    ("orishu-ctl", "the operator CLI application"),
    ("orishu-monitor", "the operator TUI application"),
    ("kagami", "the Kagami application"),
    // Capabilities a value-and-parsing library has no business holding. A
    // workload is decided from bytes already in memory; fetching them is the
    // caller's job, which is what `BlobSource` exists to express.
    ("tokio", "async runtime"),
    ("reqwest", "HTTP client"),
    ("hyper", "HTTP"),
    ("quinn", "QUIC transport"),
    ("rustls", "TLS"),
    ("salvo", "HTTP server"),
    ("tempfile", "filesystem access"),
    ("iced", "UI toolkit"),
    ("iced_core", "UI toolkit"),
    ("wgpu", "GPU presentation"),
    ("winit", "windowing"),
    ("rfd", "native file dialogs"),
    ("glam", "solver/renderer linear algebra"),
    // Wall-clock time is not workload identity: a manifest that could carry a
    // timestamp would have a different digest every time it was built.
    ("chrono", "wall-clock time"),
    // `uom` is a deliberate omission rather than an oversight. See
    // `src/domain.rs`: this crate carries canonical SI magnitudes so that
    // Kagami's authoring crates do not inherit a unit system through it.
    ("uom", "unit-typed quantities"),
];

fn metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .exec()
        .expect("cargo metadata must succeed inside the workspace")
}

/// The crates this one actually links against, excluding dev-dependencies,
/// which exist only inside the test and doc-example binaries.
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
        .find(|package| package.name.as_ref() == CRATE)
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
        .find(|package| package.name.as_ref() == CRATE)
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
fn the_workload_model_has_no_application_ui_transport_or_consumer_dependency() {
    let resolved = runtime_dependency_names(&metadata());
    let violations: Vec<String> = FORBIDDEN
        .iter()
        .filter(|(name, _)| resolved.contains(*name))
        .map(|(name, capability)| format!("  {name} — {capability}"))
        .collect();

    assert!(
        violations.is_empty(),
        "orishu-workload is the shared definition of a workload, consumed by both Kagami's \
         authoring crates and Orishu's worker. Its dependency graph now includes:\n{}\n\
         Both consumers inherit whatever this crate acquires; the consuming crate should hold \
         the capability instead.",
        violations.join("\n")
    );
}

#[test]
fn the_direct_dependencies_are_the_declared_ones() {
    let direct = direct_runtime_dependency_names(&metadata());
    assert_eq!(
        direct,
        BTreeSet::from([
            "orishu-resource".to_owned(),
            "serde".to_owned(),
            "serde_json".to_owned(),
            "serde_yaml".to_owned(),
            "sha2".to_owned(),
            "thiserror".to_owned(),
        ]),
        "a new direct runtime dependency means this crate started doing something beyond \
         describing, identifying, and structurally validating a workload. `serde_json` and \
         `serde_yaml` are here for the human authoring path only and must never reach the \
         canonical encoding; see `src/canonical.rs`."
    );
}

#[test]
fn the_dependency_graph_stays_small_enough_to_audit() {
    // serde and its derive macro, two authoring codecs, sha2 and its digest
    // traits, thiserror, and the shared envelope. A jump past this bound means
    // something with a genuinely new capability arrived.
    let resolved = runtime_dependency_names(&metadata());
    assert!(
        resolved.len() <= 32,
        "orishu-workload resolved {} runtime dependencies, more than a value-and-parsing \
         library should need: {resolved:?}",
        resolved.len()
    );
}

#[test]
fn the_expected_dependencies_are_present() {
    // A guard against every assertion above passing because metadata
    // resolution returned nothing.
    let resolved = runtime_dependency_names(&metadata());
    for expected in ["serde", "sha2", "orishu-resource"] {
        assert!(
            resolved.contains(expected),
            "expected `{expected}` in the resolved graph, got {resolved:?}"
        );
    }
}
