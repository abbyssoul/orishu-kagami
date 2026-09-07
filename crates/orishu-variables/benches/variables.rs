//! Microbenchmarks for `orishu-variables`'s throughput: how fast variables
//! can be defined and how fast they can be evaluated.
//!
//! Deliberately synthetic, mirroring the shape of
//! `fieldcad-dynamics/benches/dynamics.rs`: no real CAD document behind
//! this, just deterministically generated variables, so results are
//! comparable across commits at a fixed count/depth. Per-allocation memory
//! cost (as opposed to eval throughput) is out of scope for Criterion and is
//! covered instead by the `dhat`-gated `examples/profile_variables.rs`.
//!
//! Two shapes are benchmarked:
//! - **flat**: `count` independent constant-expression variables in one
//!   namespace, no cross-references. Isolates per-variable definition/lookup
//!   cost with no recursive resolution.
//! - **chain**: a `depth`-long dependency chain (`v1 = v0 + 1`, `v2 = v1 +
//!   1`, ...). Evaluating the tail variable walks the whole chain, so this
//!   measures how [`VariablesSystem::value`]'s iterative resolution — an
//!   explicit stack over pooled scratch buffers — scales with depth.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_variables::{
    CompiledExpression, Namespace, VariableId, VariableOptions, VariablesSystem,
};

const DEFAULT_COUNTS: [usize; 4] = [10, 100, 1_000, 10_000];
const DEFAULT_CHAIN_DEPTHS: [usize; 4] = [10, 100, 1_000, 10_000];

/// Parses a comma-separated list of `usize`s from `name`, falling back to
/// `default` when the variable is unset and panicking on a malformed entry
/// rather than silently ignoring it.
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

/// Variable counts for the `define` and `value_flat` groups, overridable via
/// `ORISHU_VARIABLES_BENCH_COUNTS` as a comma-separated list.
fn counts() -> Vec<usize> {
    env_list("ORISHU_VARIABLES_BENCH_COUNTS", &DEFAULT_COUNTS)
}

/// Dependency chain depths for the `value_chain` group, overridable via
/// `FIELDCAD_VARIABLES_BENCH_CHAIN_DEPTHS`.
fn chain_depths() -> Vec<usize> {
    env_list(
        "FIELDCAD_VARIABLES_BENCH_CHAIN_DEPTHS",
        &DEFAULT_CHAIN_DEPTHS,
    )
}

fn define_flat(vars: &mut VariablesSystem, namespace: &Namespace, count: usize) -> Vec<VariableId> {
    (0..count)
        .map(|i| {
            vars.define(
                namespace,
                format!("v{i}"),
                CompiledExpression::parse(&format!("{}", i as f64)).unwrap(),
                VariableOptions::default(),
            )
            .unwrap()
        })
        .collect()
}

fn build_flat_system(count: usize) -> (VariablesSystem, Vec<VariableId>) {
    let mut vars = VariablesSystem::default();
    let namespace = Namespace::new("bench");
    let ids = define_flat(&mut vars, &namespace, count);
    (vars, ids)
}

/// Builds a `depth`-long dependency chain in namespace `chain`:
/// `v0 = 0`, `v1 = chain.v0 + 1`, ..., `v(depth-1) = chain.v(depth-2) + 1`.
/// Returns the system and the handle of the tail variable, whose evaluation
/// walks the entire chain.
fn build_chain_system(depth: usize) -> (VariablesSystem, VariableId) {
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

fn bench_define(c: &mut Criterion) {
    let mut group = c.benchmark_group("define");
    for count in counts() {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("flat", count), &count, |b, &count| {
            b.iter_batched(
                || (VariablesSystem::default(), Namespace::new("bench")),
                |(mut vars, namespace)| define_flat(&mut vars, &namespace, count),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_value_flat(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_flat");
    for count in counts() {
        let (vars, ids) = build_flat_system(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("value", count), &count, |b, _| {
            b.iter(|| {
                let mut sum = 0.0;
                for &id in &ids {
                    sum += vars.value(id).unwrap().magnitude();
                }
                std::hint::black_box(sum)
            })
        });
    }
    group.finish();
}

/// Ad-hoc `eval()` of a fresh expression string referencing an existing
/// variable, run `count` times per iteration — this bundles the parse cost
/// (unlike `value_flat`, which evaluates an already-compiled expression) so
/// it reflects the REPL's actual per-line cost.
fn bench_eval_adhoc(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_adhoc");
    for count in counts() {
        let (vars, _ids) = build_flat_system(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("eval", count), &count, |b, &count| {
            b.iter(|| {
                let mut sum = 0.0;
                for i in 0..count {
                    sum += vars.eval(&format!("bench.v{i} * 2")).unwrap().magnitude();
                }
                std::hint::black_box(sum)
            })
        });
    }
    group.finish();
}

fn bench_value_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_chain");
    for depth in chain_depths() {
        let (vars, tail) = build_chain_system(depth);
        group.throughput(Throughput::Elements(depth as u64));
        group.bench_with_input(BenchmarkId::new("value", depth), &depth, |b, _| {
            b.iter(|| std::hint::black_box(vars.value(tail).unwrap()))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_define,
    bench_value_flat,
    bench_eval_adhoc,
    bench_value_chain
);
criterion_main!(benches);
