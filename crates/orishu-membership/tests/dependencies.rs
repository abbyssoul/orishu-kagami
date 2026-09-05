//! The dependency budget is part of the crate's contract, so it is a test.
//!
//! "Sans-IO" is a claim about what the code *cannot* do, and a claim like that
//! decays the moment someone adds a convenient crate. Reviewing imports catches
//! a direct dependency; it does not catch a transitive `tokio` arriving through
//! something innocuous. Resolving the real dependency graph does.

use std::collections::BTreeSet;

use cargo_metadata::MetadataCommand;

/// Crates that would give the core a capability it must not have.
///
/// Each entry names the capability, not just the crate, because the reason is
/// what a future contributor needs: a clock or an RNG makes `update`
/// non-deterministic and therefore unreplayable; a socket, a runtime, or a
/// filesystem handle makes it impossible to test without standing up IO.
const FORBIDDEN: &[(&str, &str)] = &[
    ("tokio", "async runtime"),
    ("async-std", "async runtime"),
    ("smol", "async runtime"),
    ("futures", "async runtime"),
    ("futures-util", "async runtime"),
    ("futures-executor", "async runtime"),
    ("async-trait", "async runtime"),
    ("mio", "event loop"),
    ("socket2", "sockets"),
    ("quinn", "QUIC transport"),
    ("quinn-proto", "QUIC transport"),
    ("reqwest", "HTTP client"),
    ("hyper", "HTTP"),
    ("h2", "HTTP"),
    ("rustls", "TLS"),
    ("native-tls", "TLS"),
    ("openssl", "TLS"),
    ("chrono", "wall clock"),
    ("time", "wall clock"),
    ("rand", "random-number generator"),
    ("rand_core", "random-number generator"),
    ("getrandom", "random-number generator"),
    ("uuid", "identifier generation, which needs entropy"),
    ("tempfile", "filesystem"),
    ("fs-err", "filesystem"),
    ("memmap2", "filesystem"),
];

/// Resolves the crates `orishu-membership` actually links against, excluding
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
    let by_id: std::collections::HashMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect();
    let nodes: std::collections::HashMap<_, _> = resolve
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect();

    let root = metadata
        .packages
        .iter()
        .find(|package| package.name.as_ref() == "orishu-membership")
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
            // Dev-dependencies are not linked into the library, so a test-only
            // `serde_json` or `criterion` is not a contract violation.
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
fn the_core_has_no_io_async_clock_or_entropy_dependency() {
    let resolved = runtime_dependency_names();
    let violations: Vec<String> = FORBIDDEN
        .iter()
        .filter(|(name, _)| resolved.contains(*name))
        .map(|(name, capability)| format!("  {name} — {capability}"))
        .collect();

    assert!(
        violations.is_empty(),
        "orishu-membership must stay sans-IO, but its dependency graph now includes:\n{}\n\
         If a capability is genuinely needed, it belongs in the IO shell, expressed as an \
         Effect and returned as an EffectOutcome.",
        violations.join("\n")
    );
}

#[test]
fn the_dependency_graph_stays_small_enough_to_audit() {
    let resolved = runtime_dependency_names();
    assert!(
        resolved.len() <= 24,
        "the sans-IO core resolved {} runtime dependencies, which is more than can be \
         reviewed by hand: {resolved:?}",
        resolved.len()
    );
}

#[test]
fn the_expected_direct_dependencies_are_present() {
    // A guard against the test silently passing because metadata resolution
    // returned nothing.
    let resolved = runtime_dependency_names();
    for expected in ["orishu-identity", "serde", "sha2", "thiserror"] {
        assert!(
            resolved.contains(expected),
            "expected `{expected}` in the resolved graph, got {resolved:?}"
        );
    }
}
