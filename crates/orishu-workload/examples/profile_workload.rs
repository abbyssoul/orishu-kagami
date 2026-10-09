//! Headless allocation profiling for `orishu-workload`: how many allocations
//! (and bytes) does encoding, digesting, decoding, and verifying a workload
//! cost.
//!
//! `criterion` measures wall-clock throughput (see `benches/workload.rs`); it
//! has no notion of allocation counts. This example fills that gap by wrapping
//! each identity-path phase in [`dhat::HeapStats`] snapshots and printing the
//! allocation delta each phase caused. It profiles the **v3** composed
//! scientific workload — the format the runtime admits — and the claim it is
//! here to check is that closure verification *streams*: it must not allocate
//! proportionally to the artifact size.
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

use std::{collections::BTreeMap, time::Instant};

use orishu_workload::{
    ArtifactDescriptor, ArtifactDigest, ComponentInstance, ComputeSpec, InMemoryBlobs, Limits,
    StepInvocation, StepPlan, WorkloadMeta, WorkloadRequirements, v3,
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

fn descriptor(role: &str, content: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor {
        role: role.parse().unwrap(),
        digest: ArtifactDigest::sha256_of(content),
        size_bytes: content.len() as u64,
        media_type: "application/octet-stream".parse().unwrap(),
        schema: None,
    }
}

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

fn v3_manifest(count: usize) -> v3::WorkloadManifest {
    v3::manifest(
        WorkloadMeta::new("profile".parse().unwrap()),
        v3::WorkloadSpec {
            compute: v3_compute(count),
            selection: descriptor(v3::SELECTION_ROLE, b"selection"),
            execution: descriptor(v3::EXECUTION_ROLE, b"execution"),
            artifacts: vec![],
            requirements: WorkloadRequirements::default(),
        },
    )
}

fn v3_closure_case(bytes: usize) -> (v3::WorkloadManifest, InMemoryBlobs) {
    let execution = vec![0xABu8; bytes];
    let manifest = v3::manifest(
        WorkloadMeta::new("profile".parse().unwrap()),
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

fn main() {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::builder().build();

    let limits = profile_limits();
    let manifest = v3_manifest(COMPONENTS);

    report_phase("v3 canonical encode", COMPONENTS, || {
        std::hint::black_box(v3::canonical_bytes(&manifest, &limits).unwrap());
    });

    report_phase("v3 workload digest", COMPONENTS, || {
        std::hint::black_box(v3::workload_digest(&manifest, &limits).unwrap());
    });

    let bytes = v3::canonical_bytes(&manifest, &limits).unwrap();
    report_phase("v3 manifest decode", COMPONENTS, || {
        std::hint::black_box(v3::from_canonical_bytes(&bytes, &limits).unwrap());
    });

    let (closure_manifest, blobs) = v3_closure_case(CLOSURE_BYTES);
    report_phase("v3 closure verify (16 MiB)", CLOSURE_BYTES, || {
        std::hint::black_box(v3::validate_closure(&closure_manifest, &blobs, &limits).unwrap());
    });

    #[cfg(not(feature = "dhat"))]
    eprintln!(
        "note: built without --features dhat, so allocation columns above are always zero \
         and no dhat-heap.json was written"
    );
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
