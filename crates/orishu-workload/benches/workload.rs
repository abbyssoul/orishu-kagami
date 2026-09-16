//! Microbenchmarks for `orishu-workload`'s identity path: how fast a manifest
//! turns into its canonical bytes, its digest, and back, and how fast a closure
//! verifies.
//!
//! Deliberately synthetic and IO-free, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: manifests are generated
//! deterministically in memory and parsed once through [`authoring::parse_str`],
//! then the identity operations are measured on the parsed value, so a result is
//! comparable across commits at a fixed component count and does not measure the
//! authoring reader. Per-allocation cost (as opposed to throughput) is out of
//! scope for Criterion and is covered by the `dhat`-gated
//! `examples/profile_workload.rs`.
//!
//! Four shapes are benchmarked, because they scale for different reasons:
//!
//! - **canonical**: `canonical_bytes` over a `count`-component graph. The core
//!   deterministic CBOR encoder; every admission and every Kagami compile pays
//!   it, and it scales with the graph the manifest declares.
//! - **digest**: `workload_digest` over the same manifest — the canonical
//!   encode plus one SHA-256 pass, which is what "identity = digest of canonical
//!   bytes" costs.
//! - **decode**: `manifest_from_canonical_bytes` recovering the manifest, the
//!   receiver's side of the codec.
//! - **closure**: `validate_closure` streaming one artifact of `bytes` bytes
//!   through the SHA-256 verifier, so this is the hashing throughput a real
//!   closure verification is dominated by.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_workload::{
    ArtifactDigest, InMemoryBlobs, Limits, WorkloadManifest, authoring,
    canonical::{canonical_bytes, manifest_from_canonical_bytes, workload_digest},
    closure::validate_closure,
};

/// Bounds wide enough for whatever component count the parameters below ask for.
///
/// A benchmark measures the cost of a shape, so it declares the bounds that
/// shape needs rather than being silently capped by `Limits::DEFAULT`, whose
/// `max_components` (64) is an admission ceiling, not a benchmark ceiling.
fn bench_limits() -> Limits {
    Limits {
        max_components: 1 << 20,
        max_channels: 1 << 20,
        max_step_invocations: 1 << 20,
        max_artifacts: 1 << 20,
        max_manifest_bytes: 1 << 30,
        max_canonical_values: 1 << 24,
        ..Limits::DEFAULT
    }
}

const DEFAULT_COMPONENTS: [usize; 4] = [4, 16, 64, 256];
const DEFAULT_CLOSURE_BYTES: [usize; 3] = [1 << 20, 1 << 22, 1 << 24];

/// Parses a comma-separated list of `usize`s from `name`, falling back to
/// `default` when unset and panicking on a malformed entry rather than silently
/// ignoring it.
fn env_list(name: &str, default: &[usize]) -> Vec<usize> {
    match std::env::var(name) {
        Ok(raw) => raw
            .split(',')
            .map(|entry| {
                entry
                    .trim()
                    .parse()
                    .unwrap_or_else(|error| panic!("invalid {name} entry {entry:?}: {error}"))
            })
            .collect(),
        Err(_) => default.to_vec(),
    }
}

fn component_counts() -> Vec<usize> {
    env_list("ORISHU_WORKLOAD_BENCH_COMPONENTS", &DEFAULT_COMPONENTS)
}

fn closure_bytes() -> Vec<usize> {
    env_list(
        "ORISHU_WORKLOAD_BENCH_CLOSURE_BYTES",
        &DEFAULT_CLOSURE_BYTES,
    )
}

/// A structurally valid workload document with `count` field components, each
/// owning one state channel advanced by one invocation. Mirrors the shape of
/// `tests/fixtures/minimal.workload.yaml`, scaled.
fn manifest_doc(count: usize) -> String {
    let mut components = String::new();
    let mut channels = String::new();
    let mut invocations = String::new();
    for index in 0..count {
        let digest = ArtifactDigest::sha256_of(format!("artifact-{index}").as_bytes());
        components.push_str(&format!(
            "      - instanceId: field{index}\n\
             \x20       artifact:\n\
             \x20         role: component\n\
             \x20         digest: {digest}\n\
             \x20         sizeBytes: 20\n\
             \x20         mediaType: application/wasm\n\
             \x20       pluginId: dev.orishu.electromagnetism\n\
             \x20       modelId: dev.orishu.electromagnetism.yee/v1\n\
             \x20       schemaId: dev.orishu.em.field/v1\n\
             \x20       engine: wasm-component\n\
             \x20       lifecycle: orishu.component/v1\n\
             \x20       roles: [field-model]\n\
             \x20       stateOwnership: [ch{index}]\n"
        ));
        channels.push_str(&format!(
            "      - channelId: ch{index}\n\
             \x20       schema:\n\
             \x20         schemaId: dev.orishu.em.field/v1\n\
             \x20         version: 1\n\
             \x20       shape: [3]\n\
             \x20       owner: field{index}\n\
             \x20       reduction: single\n"
        ));
        invocations.push_str(&format!(
            "        - invocationId: adv{index}\n\
             \x20         instance: field{index}\n\
             \x20         phaseId: update-field\n\
             \x20         outputs: [ch{index}]\n"
        ));
    }
    format!(
        "apiVersion: orishu.dev/v2\n\
         kind: Workload\n\
         metadata:\n\
         \x20 name: bench {count} components\n\
         spec:\n\
         \x20 compute:\n\
         \x20   workloadGraphProfile: orishu.workload-graph/v1\n\
         \x20   components:\n{components}\
         \x20   channels:\n{channels}\
         \x20   stepPlan:\n\
         \x20     profile: orishu.workload-graph/v1\n\
         \x20     invocations:\n{invocations}\
         \x20 domain:\n\
         \x20   dimensions: 3\n\
         \x20   bounds:\n\
         \x20     shape: cube\n\
         \x20     sideMetres: 1.0\n\
         \x20   discretization:\n\
         \x20     spaceMetres: 0.001\n\
         \x20     timeSeconds: 1.5e-11\n"
    )
}

fn manifest(count: usize) -> WorkloadManifest {
    authoring::parse_str(&manifest_doc(count), &bench_limits()).unwrap_or_else(|error| {
        panic!("generated {count}-component manifest must parse: {error:?}")
    })
}

fn bench_canonical(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("canonical");
    for count in component_counts() {
        let manifest = manifest(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("encode", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(canonical_bytes(&manifest, &limits).unwrap()))
        });
    }
    group.finish();
}

fn bench_digest(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("digest");
    for count in component_counts() {
        let manifest = manifest(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("digest", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(workload_digest(&manifest, &limits).unwrap()))
        });
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("decode");
    for count in component_counts() {
        let bytes = canonical_bytes(&manifest(count), &limits).unwrap();
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("recover", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(manifest_from_canonical_bytes(&bytes, &limits).unwrap()))
        });
    }
    group.finish();
}

/// A single-component manifest whose one artifact is `bytes` bytes long,
/// paired with a blob source holding exactly those bytes under their digest, so
/// `validate_closure` hashes the whole artifact.
fn closure_case(bytes: usize) -> (WorkloadManifest, InMemoryBlobs) {
    let blob = vec![0xABu8; bytes];
    let digest = ArtifactDigest::sha256_of(&blob);
    let doc = format!(
        "apiVersion: orishu.dev/v2\n\
         kind: Workload\n\
         metadata:\n\
         \x20 name: bench closure\n\
         spec:\n\
         \x20 compute:\n\
         \x20   workloadGraphProfile: orishu.workload-graph/v1\n\
         \x20   components:\n\
         \x20     - instanceId: field\n\
         \x20       artifact:\n\
         \x20         role: component\n\
         \x20         digest: {digest}\n\
         \x20         sizeBytes: {bytes}\n\
         \x20         mediaType: application/wasm\n\
         \x20       pluginId: dev.orishu.electromagnetism\n\
         \x20       modelId: dev.orishu.electromagnetism.yee/v1\n\
         \x20       schemaId: dev.orishu.em.field/v1\n\
         \x20       engine: wasm-component\n\
         \x20       lifecycle: orishu.component/v1\n\
         \x20       roles: [field-model]\n\
         \x20       stateOwnership: [e-field]\n\
         \x20   channels:\n\
         \x20     - channelId: e-field\n\
         \x20       schema:\n\
         \x20         schemaId: dev.orishu.em.field/v1\n\
         \x20         version: 1\n\
         \x20       shape: [3]\n\
         \x20       owner: field\n\
         \x20       reduction: single\n\
         \x20   stepPlan:\n\
         \x20     profile: orishu.workload-graph/v1\n\
         \x20     invocations:\n\
         \x20       - invocationId: advance\n\
         \x20         instance: field\n\
         \x20         phaseId: update-field\n\
         \x20         outputs: [e-field]\n\
         \x20 domain:\n\
         \x20   dimensions: 3\n\
         \x20   bounds:\n\
         \x20     shape: cube\n\
         \x20     sideMetres: 1.0\n\
         \x20   discretization:\n\
         \x20     spaceMetres: 0.001\n\
         \x20     timeSeconds: 1.5e-11\n"
    );
    let manifest = authoring::parse_str(&doc, &bench_limits())
        .unwrap_or_else(|error| panic!("closure manifest must parse: {error:?}"));
    let mut blobs = InMemoryBlobs::new();
    let stored = blobs.insert(blob);
    assert_eq!(
        stored, digest,
        "the stored blob must key by the declared digest"
    );
    (manifest, blobs)
}

fn bench_closure(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("closure");
    for bytes in closure_bytes() {
        let (manifest, blobs) = closure_case(bytes);
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(BenchmarkId::new("verify", bytes), &bytes, |b, _| {
            b.iter(|| {
                let report = validate_closure(&manifest, &blobs, &limits).unwrap();
                std::hint::black_box(report.root())
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_canonical,
    bench_digest,
    bench_decode,
    bench_closure
);
criterion_main!(benches);
