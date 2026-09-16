//! The identity crate's dependency budget is its whole reason to exist, so it
//! is a test rather than a comment.
//!
//! `orishu-identity` is depended on by the networking client in `orishu` (which
//! links `reqwest`, `tokio`, and `chrono`) and by the sans-IO `orishu-membership`
//! core (which must link none of that). Defining the identity types once here
//! keeps a single definition without forcing the membership core to inherit the
//! client's dependency tree — but only for as long as this crate stays poor.
//! Reviewing imports catches a direct dependency; it does not catch a transitive
//! `tokio` or `getrandom` arriving through something innocuous. Resolving the
//! real graph does.

use std::collections::{BTreeSet, HashMap};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

/// Crates that would give the identity contract a capability it must not have.
///
/// Each entry names the capability, not just the crate, because the reason is
/// what a future contributor needs. The list mirrors the ban the sans-IO
/// membership core lives under: a networking, async-runtime, clock, filesystem,
/// TLS, or RNG dependency here would flow straight into that core.
const FORBIDDEN: &[(&str, &str)] = &[
    // Domains and consumers. This crate is shared *by* these; depending on one
    // would invert the relationship and drag the consumer's tree back in.
    ("orishu", "Orishu domain models and client interfaces"),
    ("orishu-membership", "cluster membership"),
    ("orishu-workload", "the immutable workload"),
    ("orishu-resource", "the resource envelope"),
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
    // The exact capabilities the crate's manifest promises to stay clear of.
    ("tokio", "async runtime"),
    ("async-std", "async runtime"),
    ("quinn", "QUIC transport"),
    ("rustls", "TLS"),
    ("tokio-rustls", "TLS"),
    ("reqwest", "HTTP client"),
    ("hyper", "HTTP"),
    ("salvo", "HTTP server"),
    ("chrono", "wall-clock time"),
    ("time", "wall-clock time"),
    ("tempfile", "filesystem access"),
    ("rand", "randomness"),
    ("getrandom", "randomness"),
    // UI. Nothing here is presentation.
    ("iced", "UI toolkit"),
    ("iced_core", "UI toolkit"),
    ("wgpu", "GPU presentation"),
    ("winit", "windowing"),
    ("rfd", "native file dialogs"),
    // Codecs. Choosing JSON, YAML, or CBOR is the consuming domain's decision;
    // this crate describes types and lets the caller bring the bytes.
    ("serde_json", "a concrete codec"),
    ("serde_yaml", "a concrete codec"),
    ("ciborium", "a concrete codec"),
];

fn metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .exec()
        .expect("cargo metadata must succeed inside the workspace")
}

/// The crates `orishu-identity` actually links against, excluding
/// dev-dependencies, which exist only inside the test and doc-example binaries.
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
        .find(|package| package.name.as_ref() == "orishu-identity")
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
        .find(|package| package.name.as_ref() == "orishu-identity")
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
fn identity_has_no_networking_clock_filesystem_rng_ui_or_domain_dependency() {
    let resolved = runtime_dependency_names(&metadata());
    let violations: Vec<String> = FORBIDDEN
        .iter()
        .filter(|(name, _)| resolved.contains(*name))
        .map(|(name, capability)| format!("  {name} — {capability}"))
        .collect();

    assert!(
        violations.is_empty(),
        "orishu-identity must stay poor enough for the sans-IO membership core to depend on it, \
         but its dependency graph now includes:\n{}\n\
         The consuming crate should hold the capability instead.",
        violations.join("\n")
    );
}

#[test]
fn serde_and_thiserror_are_the_only_direct_runtime_dependencies() {
    let direct = direct_runtime_dependency_names(&metadata());
    assert_eq!(
        direct,
        BTreeSet::from(["serde".to_owned(), "thiserror".to_owned()]),
        "the crate defines identity types and their validating parse errors; a third runtime \
         dependency means it started doing something else"
    );
}

#[test]
fn the_dependency_graph_stays_small_enough_to_audit() {
    // Everything resolved should be serde and thiserror plus the crates their
    // derive macros need. A jump past this bound means something with a
    // genuinely new capability arrived.
    let resolved = runtime_dependency_names(&metadata());
    assert!(
        resolved.len() <= 12,
        "orishu-identity resolved {} runtime dependencies, which is more than serde, thiserror, \
         and their derive macros should need: {resolved:?}",
        resolved.len()
    );
}

#[test]
fn the_expected_dependencies_are_present() {
    // A guard against every assertion above passing because metadata
    // resolution returned nothing.
    let resolved = runtime_dependency_names(&metadata());
    assert!(
        resolved.contains("serde") && resolved.contains("thiserror"),
        "expected `serde` and `thiserror` in the resolved graph, got {resolved:?}"
    );
}
