//! Microbenchmarks for `orishu-workload`'s identity path: how fast a manifest
//! turns into its canonical bytes, its digest, and back, and how fast a closure
//! verifies.
//!
//! Deliberately synthetic and IO-free, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: manifests are generated
//! deterministically in memory, then the identity operations are measured on the
//! constructed value, so a result is comparable across commits at a fixed
//! component count. Per-allocation cost (as opposed to throughput) is out of
//! scope for Criterion and is covered by the `dhat`-gated
//! `examples/profile_workload.rs`.
//!
//! **v3 is the primary format.** It is the composed scientific workload the
//! runtime admits: a component graph plus digest-addressed selection, execution,
//! and artifact descriptors, and its identity commits to that whole closure.
//! Each group also benchmarks the legacy **v2** root as a clearly labelled
//! `v2_baseline`, because v2 remains a separate immutable format and its cost is
//! a useful comparison, not the thing admission pays today.
//!
//! Four shapes are benchmarked, because they scale for different reasons:
//!
//! - **canonical**: `canonical_bytes` over a `count`-component graph. The
//!   deterministic CBOR encoder every admission and Kagami compile pays.
//! - **digest**: `workload_digest` — the canonical encode plus one SHA-256 pass,
//!   which is what "identity = digest of canonical bytes" costs.
//! - **decode**: recovering the manifest from canonical bytes, the receiver's
//!   side of the codec.
//! - **closure**: `validate_closure` streaming an artifact of `bytes` bytes
//!   through the SHA-256 verifier — for v3 this covers the selection, execution,
//!   and artifact-closure descriptors the format adds.

use std::collections::BTreeMap;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_workload::{
    ArtifactDescriptor, ArtifactDigest, ComponentInstance, ComputeSpec, InMemoryBlobs, Limits,
    StepInvocation, StepPlan, WorkloadManifest, WorkloadMeta, WorkloadRequirements, authoring,
    canonical::{canonical_bytes, manifest_from_canonical_bytes, workload_digest},
    closure::validate_closure,
    v3,
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

// --- v3: the primary composed scientific workload -------------------------

/// A descriptor naming `content` by digest, exactly as a v3 input is addressed.
fn descriptor(role: &str, content: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor {
        role: role.parse().unwrap(),
        digest: ArtifactDigest::sha256_of(content),
        size_bytes: content.len() as u64,
        media_type: "application/octet-stream".parse().unwrap(),
        schema: None,
    }
}

/// A component instance owning one kernel, with a unique digest per index.
fn v3_component(index: usize) -> ComponentInstance {
    ComponentInstance {
        instance_id: format!("kernel{index}").parse().unwrap(),
        artifact: descriptor("component", format!("code-{index}").as_bytes()),
        plugin_id: "org.example.plugin".parse().unwrap(),
        model_id: "model".parse().unwrap(),
        schema_id: "state/v1".parse().unwrap(),
        engine: "wasm-component".parse().unwrap(),
        lifecycle: "orishu:simulation/field@1".parse().unwrap(),
        roles: vec![],
        state_ownership: vec![],
        config: BTreeMap::new(),
        limits: BTreeMap::new(),
    }
}

fn v3_compute(count: usize) -> ComputeSpec {
    ComputeSpec {
        workload_graph_profile: "orishu.force-then-integrate/v1".parse().unwrap(),
        components: (0..count).map(v3_component).collect(),
        channels: vec![],
        placement_constraints: vec![],
        step_plan: StepPlan {
            profile: "orishu.force-then-integrate/v1".parse().unwrap(),
            invocations: (0..count)
                .map(|index| StepInvocation {
                    invocation_id: format!("advance{index}").parse().unwrap(),
                    instance: format!("kernel{index}").parse().unwrap(),
                    phase_id: "advance".parse().unwrap(),
                    inputs: vec![],
                    outputs: vec![],
                    depends_on: vec![],
                })
                .collect(),
        },
    }
}

/// A v3 manifest with `count` components and small selection/execution inputs.
fn v3_manifest(count: usize) -> v3::WorkloadManifest {
    v3::manifest(
        WorkloadMeta::new("bench".parse().unwrap()),
        v3::WorkloadSpec {
            compute: v3_compute(count),
            selection: descriptor(v3::SELECTION_ROLE, b"selection"),
            execution: descriptor(v3::EXECUTION_ROLE, b"execution"),
            artifacts: vec![],
            requirements: WorkloadRequirements::default(),
        },
    )
}

/// A v3 manifest whose execution input is `bytes` bytes, paired with a blob
/// source holding every referenced blob, so `validate_closure` hashes the whole
/// scientific closure.
fn v3_closure_case(bytes: usize) -> (v3::WorkloadManifest, InMemoryBlobs) {
    let execution = vec![0xABu8; bytes];
    let manifest = v3::manifest(
        WorkloadMeta::new("bench".parse().unwrap()),
        v3::WorkloadSpec {
            compute: v3_compute(1),
            selection: descriptor(v3::SELECTION_ROLE, b"selection"),
            execution: descriptor(v3::EXECUTION_ROLE, &execution),
            artifacts: vec![descriptor("initial-conditions", b"state")],
            requirements: WorkloadRequirements::default(),
        },
    );
    let mut blobs = InMemoryBlobs::new();
    for content in [
        b"code-0".as_slice(),
        b"selection",
        b"state",
        execution.as_slice(),
    ] {
        blobs.insert(content.to_vec());
    }
    (manifest, blobs)
}

// --- v2: the legacy immutable format, kept as a baseline ------------------

/// A structurally valid v2 workload document with `count` field components, each
/// owning one state channel advanced by one invocation. Mirrors the shape of
/// `tests/fixtures/minimal.workload.yaml`, scaled.
fn v2_manifest_doc(count: usize) -> String {
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

fn v2_manifest(count: usize) -> WorkloadManifest {
    authoring::parse_str(&v2_manifest_doc(count), &bench_limits()).unwrap_or_else(|error| {
        panic!("generated {count}-component v2 manifest must parse: {error:?}")
    })
}

/// A single-component v2 manifest whose one artifact is `bytes` bytes, paired
/// with a blob source holding those bytes.
fn v2_closure_case(bytes: usize) -> (WorkloadManifest, InMemoryBlobs) {
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
        .unwrap_or_else(|error| panic!("v2 closure manifest must parse: {error:?}"));
    let mut blobs = InMemoryBlobs::new();
    blobs.insert(blob);
    (manifest, blobs)
}

// --- benchmark groups: v3 primary, v2 baseline ----------------------------

fn bench_canonical(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("canonical");
    for count in component_counts() {
        group.throughput(Throughput::Elements(count as u64));

        let m3 = v3_manifest(count);
        group.bench_with_input(BenchmarkId::new("v3", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(v3::canonical_bytes(&m3, &limits).unwrap()))
        });

        let m2 = v2_manifest(count);
        group.bench_with_input(BenchmarkId::new("v2_baseline", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(canonical_bytes(&m2, &limits).unwrap()))
        });
    }
    group.finish();
}

fn bench_digest(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("digest");
    for count in component_counts() {
        group.throughput(Throughput::Elements(count as u64));

        let m3 = v3_manifest(count);
        group.bench_with_input(BenchmarkId::new("v3", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(v3::workload_digest(&m3, &limits).unwrap()))
        });

        let m2 = v2_manifest(count);
        group.bench_with_input(BenchmarkId::new("v2_baseline", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(workload_digest(&m2, &limits).unwrap()))
        });
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("decode");
    for count in component_counts() {
        group.throughput(Throughput::Elements(count as u64));

        let v3_bytes = v3::canonical_bytes(&v3_manifest(count), &limits).unwrap();
        group.bench_with_input(BenchmarkId::new("v3", count), &count, |b, _| {
            // The recovered manifest is a large value; drop it outside the timing
            // loop so this measures decode cost, not the teardown.
            b.iter_with_large_drop(|| v3::from_canonical_bytes(&v3_bytes, &limits).unwrap())
        });

        let v2_bytes = canonical_bytes(&v2_manifest(count), &limits).unwrap();
        group.bench_with_input(BenchmarkId::new("v2_baseline", count), &count, |b, _| {
            b.iter_with_large_drop(|| manifest_from_canonical_bytes(&v2_bytes, &limits).unwrap())
        });
    }
    group.finish();
}

fn bench_closure(c: &mut Criterion) {
    let limits = bench_limits();
    let mut group = c.benchmark_group("closure");
    for bytes in closure_bytes() {
        group.throughput(Throughput::Bytes(bytes as u64));

        let (m3, blobs3) = v3_closure_case(bytes);
        group.bench_with_input(BenchmarkId::new("v3", bytes), &bytes, |b, _| {
            b.iter(|| {
                let report = v3::validate_closure(&m3, &blobs3, &limits).unwrap();
                std::hint::black_box(report.root())
            })
        });

        let (m2, blobs2) = v2_closure_case(bytes);
        group.bench_with_input(BenchmarkId::new("v2_baseline", bytes), &bytes, |b, _| {
            b.iter(|| {
                let report = validate_closure(&m2, &blobs2, &limits).unwrap();
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
