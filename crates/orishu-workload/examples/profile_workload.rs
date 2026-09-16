//! Headless allocation profiling for `orishu-workload`: how many allocations
//! (and bytes) does encoding, digesting, decoding, and verifying a workload
//! cost.
//!
//! `criterion` measures wall-clock throughput (see `benches/workload.rs`); it
//! has no notion of allocation counts. This example fills that gap by wrapping
//! each identity-path phase in [`dhat::HeapStats`] snapshots and printing the
//! allocation delta each phase caused. The claim it is here to check is that
//! closure verification *streams* — it must not allocate proportionally to the
//! artifact size.
//!
//! Build and run with the `dhat` feature to also capture a full
//! `dhat-heap.json` call-site profile (viewable at
//! <https://nnethercote.github.io/dh_view/dh_view.html>):
//!
//! ```sh
//! cargo run --release -p orishu-workload --example profile_workload --features dhat
//! ```
//!
//! Without `--features dhat` this still runs and prints wall-clock timings, but
//! the allocation columns report zero (no counting allocator installed).

use std::time::Instant;

use orishu_workload::{
    ArtifactDigest, InMemoryBlobs, Limits, WorkloadManifest, authoring,
    canonical::{canonical_bytes, manifest_from_canonical_bytes, workload_digest},
    closure::validate_closure,
};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const COMPONENTS: usize = 256;
const CLOSURE_BYTES: usize = 16 * 1024 * 1024;

fn profile_limits() -> Limits {
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

fn main() {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::builder().build();

    let limits = profile_limits();
    let manifest = manifest(COMPONENTS, &limits);

    report_phase("canonical encode", COMPONENTS, || {
        std::hint::black_box(canonical_bytes(&manifest, &limits).unwrap());
    });

    report_phase("workload digest", COMPONENTS, || {
        std::hint::black_box(workload_digest(&manifest, &limits).unwrap());
    });

    let bytes = canonical_bytes(&manifest, &limits).unwrap();
    report_phase("manifest decode", COMPONENTS, || {
        std::hint::black_box(manifest_from_canonical_bytes(&bytes, &limits).unwrap());
    });

    let (closure_manifest, blobs) = closure_case(CLOSURE_BYTES, &limits);
    report_phase("closure verify (16 MiB)", CLOSURE_BYTES, || {
        std::hint::black_box(validate_closure(&closure_manifest, &blobs, &limits).unwrap());
    });

    #[cfg(not(feature = "dhat"))]
    eprintln!(
        "note: built without --features dhat, so allocation columns above are always zero \
         and no dhat-heap.json was written"
    );
}

/// A structurally valid workload document with `count` field components, each
/// owning one state channel advanced by one invocation.
fn manifest(count: usize, limits: &Limits) -> WorkloadManifest {
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
    let doc = format!(
        "apiVersion: orishu.dev/v2\n\
         kind: Workload\n\
         metadata:\n\
         \x20 name: profile {count} components\n\
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
    );
    authoring::parse_str(&doc, limits).expect("generated manifest must parse")
}

/// A single-component manifest whose artifact is `bytes` bytes, paired with a
/// blob source holding exactly those bytes.
fn closure_case(bytes: usize, limits: &Limits) -> (WorkloadManifest, InMemoryBlobs) {
    let blob = vec![0xABu8; bytes];
    let digest = ArtifactDigest::sha256_of(&blob);
    let doc = format!(
        "apiVersion: orishu.dev/v2\n\
         kind: Workload\n\
         metadata:\n\
         \x20 name: profile closure\n\
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
    let manifest = authoring::parse_str(&doc, limits).expect("closure manifest must parse");
    let mut blobs = InMemoryBlobs::new();
    blobs.insert(blob);
    (manifest, blobs)
}

/// Runs `work`, printing wall-clock time and (with `--features dhat`) the
/// allocation delta it caused, normalized per `unit_count` unit of work.
fn report_phase(label: &str, unit_count: usize, work: impl FnOnce()) {
    #[cfg(feature = "dhat")]
    let before = dhat::HeapStats::get();

    let started = Instant::now();
    work();
    let elapsed = started.elapsed();

    #[cfg(feature = "dhat")]
    {
        let after = dhat::HeapStats::get();
        let blocks = after.total_blocks - before.total_blocks;
        let bytes = after.total_bytes - before.total_bytes;
        eprintln!(
            "{label:<28} {elapsed:>10.2?}  {blocks:>10} blocks  {bytes:>14} bytes  \
             ({:.3} blocks/unit, {:.2} bytes/unit)",
            blocks as f64 / unit_count as f64,
            bytes as f64 / unit_count as f64,
        );
    }
    #[cfg(not(feature = "dhat"))]
    {
        let _ = unit_count;
        eprintln!("{label:<28} {elapsed:>10.2?}");
    }
}
