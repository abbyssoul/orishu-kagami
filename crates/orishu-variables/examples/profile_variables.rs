//! Headless allocation profiling for `orishu-variables`: how many
//! allocations (and bytes) does defining and evaluating variables cost.
//!
//! `criterion` measures wall-clock throughput (see `benches/variables.rs`);
//! it has no notion of allocation counts. This example fills that gap by
//! wrapping phases of the same flat/chain workloads in [`dhat::HeapStats`]
//! snapshots and printing the allocation delta each phase caused.
//!
//! Build and run with the `dhat` feature to also capture a full
//! `dhat-heap.json` call-site profile (viewable at
//! <https://nnethercote.github.io/dh_view/dh_view.html>):
//!
//! ```sh
//! cargo run --release -p orishu-variables --example profile_variables --features dhat
//! ```
//!
//! Without `--features dhat` this still runs and prints wall-clock timings,
//! but the allocation columns report zero (no counting allocator installed).

use std::time::Instant;

use orishu_variables::{CompiledExpression, Namespace, VariableOptions, VariablesSystem};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const FLAT_COUNT: usize = 10_000;
const CHAIN_DEPTH: usize = 10_000;
const EVAL_REPS: usize = 10;

fn main() {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::builder().build();

    report_phase("define flat", FLAT_COUNT, || {
        let mut vars = VariablesSystem::default();
        let namespace = Namespace::new("bench");
        for i in 0..FLAT_COUNT {
            vars.define(
                &namespace,
                format!("v{i}"),
                CompiledExpression::parse(&format!("{}", i as f64)).unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        }
        std::hint::black_box(vars);
    });

    let (vars, ids) = build_flat_system(FLAT_COUNT);
    report_phase("value flat (x10 passes)", FLAT_COUNT * EVAL_REPS, || {
        for _ in 0..EVAL_REPS {
            for &id in &ids {
                std::hint::black_box(vars.value(id).unwrap());
            }
        }
    });

    report_phase("define chain", CHAIN_DEPTH, || {
        std::hint::black_box(build_chain_system(CHAIN_DEPTH));
    });

    let (chain_vars, tail) = build_chain_system(CHAIN_DEPTH);
    report_phase("value chain tail (x10 evals)", EVAL_REPS, || {
        for _ in 0..EVAL_REPS {
            std::hint::black_box(chain_vars.value(tail).unwrap());
        }
    });

    #[cfg(not(feature = "dhat"))]
    eprintln!(
        "note: built without --features dhat, so allocation columns above are always zero \
         and no dhat-heap.json was written"
    );
}

fn build_flat_system(count: usize) -> (VariablesSystem, Vec<orishu_variables::VariableId>) {
    let mut vars = VariablesSystem::default();
    let namespace = Namespace::new("bench");
    let ids = (0..count)
        .map(|i| {
            vars.define(
                &namespace,
                format!("v{i}"),
                CompiledExpression::parse(&format!("{}", i as f64)).unwrap(),
                VariableOptions::default(),
            )
            .unwrap()
        })
        .collect();
    (vars, ids)
}

fn build_chain_system(depth: usize) -> (VariablesSystem, orishu_variables::VariableId) {
    let mut vars = VariablesSystem::default();
    let namespace = Namespace::new("chain");
    let mut previous = vars
        .define(
            &namespace,
            "v0",
            CompiledExpression::parse("0").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
    for i in 1..depth {
        let source = format!("chain.v{} + 1", i - 1);
        previous = vars
            .define(
                &namespace,
                format!("v{i}"),
                CompiledExpression::parse(&source).unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
    }
    (vars, previous)
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
            "{label:<28} {elapsed:>10.2?}  {blocks:>10} blocks  {bytes:>12} bytes  \
             ({:.2} blocks/unit, {:.1} bytes/unit)",
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
