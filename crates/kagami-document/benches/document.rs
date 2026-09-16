//! Microbenchmarks for `kagami-document`'s validated transition: how fast
//! `update` applies authoring commands, since every command validates the whole
//! candidate experiment before it is adopted.
//!
//! Deliberately synthetic and IO-free, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: experiments are built
//! deterministically in memory, so a result is comparable across commits at a
//! fixed object count. Authoring is human-paced, so this is not a throughput hot
//! path in production — but `update` clones and re-validates the whole candidate
//! on every batch, so the two shapes below are where an accidental quadratic
//! (re-scanning every object per command) would show up.
//!
//! - **create_batch**: one batch of `count` `CreateObject` commands applied to an
//!   empty experiment. The whole batch is validated as a single candidate.
//! - **incremental**: a single `CreateObject` applied to an experiment that
//!   already holds `existing` objects. This is the realistic per-edit cost, and
//!   a rising curve means one edit's cost grows with document size.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kagami_catalog::SchemaRegistry;
use kagami_document::{DisplayName, Experiment, ExperimentCommand, Limits, ObjectSpec, update};

/// Bounds wide enough for whatever object count the parameters below ask for;
/// `Limits::DEFAULT` caps a batch at 4096, which is an authoring ceiling, not a
/// benchmark one.
fn bench_limits() -> Limits {
    Limits {
        max_objects: 1 << 20,
        max_commands_per_batch: 1 << 20,
        ..Limits::DEFAULT
    }
}

/// No component schemas are needed: the benchmarked objects carry no components,
/// which isolates the transition and validation cost from schema lookup.
fn schemas() -> SchemaRegistry {
    SchemaRegistry::new()
}

const DEFAULT_BATCH: [usize; 4] = [16, 256, 1_024, 4_096];
const DEFAULT_EXISTING: [usize; 4] = [16, 256, 4_096, 16_384];

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

fn batch_counts() -> Vec<usize> {
    env_list("KAGAMI_DOCUMENT_BENCH_BATCH", &DEFAULT_BATCH)
}

fn existing_counts() -> Vec<usize> {
    env_list("KAGAMI_DOCUMENT_BENCH_EXISTING", &DEFAULT_EXISTING)
}

fn create(label: &str) -> ExperimentCommand {
    ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
        DisplayName::new(label).expect("valid label"),
    )))
}

fn create_commands(count: usize) -> Vec<ExperimentCommand> {
    (0..count).map(|i| create(&format!("object-{i}"))).collect()
}

/// An experiment holding `count` objects, built by adopting one batch.
fn grown(count: usize) -> Experiment {
    update(
        &Experiment::new(),
        &create_commands(count),
        &schemas(),
        &bench_limits(),
    )
    .expect("the seed batch is valid")
    .adopt()
    .0
}

fn bench_create_batch(c: &mut Criterion) {
    let limits = bench_limits();
    let schemas = schemas();
    let empty = Experiment::new();
    let mut group = c.benchmark_group("create_batch");
    for count in batch_counts() {
        let commands = create_commands(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("create", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(update(&empty, &commands, &schemas, &limits).unwrap()))
        });
    }
    group.finish();
}

fn bench_incremental(c: &mut Criterion) {
    let limits = bench_limits();
    let schemas = schemas();
    let one = [create("newcomer")];
    let mut group = c.benchmark_group("incremental");
    for existing in existing_counts() {
        let experiment = grown(existing);
        group.bench_with_input(BenchmarkId::new("add_one", existing), &existing, |b, _| {
            b.iter(|| std::hint::black_box(update(&experiment, &one, &schemas, &limits).unwrap()))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_create_batch, bench_incremental);
criterion_main!(benches);
