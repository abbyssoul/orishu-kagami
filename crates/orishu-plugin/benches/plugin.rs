//! Microbenchmarks for `orishu-plugin`'s scientific bulk-IO packet path: how
//! fast a packet of `count` records encodes, validates on read, and iterates.
//!
//! Deliberately synthetic and IO-free, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: records are generated
//! deterministically in memory, so a result is comparable across commits at a
//! fixed record count. Per-allocation cost — and the crate's claim that reading
//! and iterating a packet are *allocation-free* — is out of scope for Criterion
//! and is measured by the `dhat`-gated `examples/profile_plugin.rs`.
//!
//! These packets are the per-step data contract the fixed-run owner and
//! workload admission consume, so `Batch::read`'s "O(records), allocation-free"
//! claim is exactly the kind that has to be measured rather than asserted.
//!
//! Three shapes are benchmarked, over two record types (a small `Force` and a
//! larger `ObjectState` with kinematics and an optional mass):
//!
//! - **encode**: `encode_batch` — validate every record, then serialize. The
//!   producer's cost.
//! - **read**: `Batch::read` — framing and bounds, then validate every record
//!   and strict identity ordering before exposing any. The consumer's admission
//!   cost.
//! - **iterate**: walking an already-validated batch, which decodes each record
//!   without a per-record allocation.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_plugin::{
    FiniteF64,
    execution::{Batch, BulkLimits, BulkRecord, EntityId, Force, Kinematics, ObjectState},
};

/// A record ceiling wide enough for whatever count the parameters below ask for.
fn bench_limits() -> BulkLimits {
    BulkLimits {
        bytes: 1 << 30,
        records: 1 << 21,
    }
}

const DEFAULT_COUNTS: [usize; 4] = [64, 1_024, 16_384, 262_144];

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

fn counts() -> Vec<usize> {
    env_list("ORISHU_PLUGIN_BENCH_COUNTS", &DEFAULT_COUNTS)
}

fn f(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}

fn kinematics() -> Kinematics {
    Kinematics {
        position_metres: [f(1.0), f(2.0), f(3.0)],
        velocity_metres_per_second: [f(0.1), f(0.2), f(0.3)],
    }
}

/// `count` forces with strictly ascending ids, as `encode_batch` requires.
fn force_records(count: usize) -> Vec<Force> {
    (0..count as u64)
        .map(|id| Force {
            id: EntityId(id),
            newtons: [f(id as f64), f(0.0), f(-1.0)],
        })
        .collect()
}

/// `count` object states with strictly ascending ids, alternating between
/// dynamic (with a mass) and static (without), so the optional field is exercised.
fn object_records(count: usize) -> Vec<ObjectState> {
    (0..count as u64)
        .map(|id| ObjectState {
            id: EntityId(id),
            kinematics: kinematics(),
            inertial_mass_kilograms: (id % 2 == 0).then(|| f(1.5)),
        })
        .collect()
}

fn encoded<R: BulkRecord>(records: &[R]) -> Vec<u8> {
    let mut bytes = Vec::new();
    orishu_plugin::execution::encode_batch(records, &mut bytes, bench_limits())
        .expect("records encode");
    bytes
}

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode");
    for count in counts() {
        let forces = force_records(count);
        let objects = object_records(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("force", count), &count, |b, _| {
            b.iter(|| {
                let mut bytes = Vec::new();
                orishu_plugin::execution::encode_batch(&forces, &mut bytes, bench_limits())
                    .unwrap();
                std::hint::black_box(bytes)
            })
        });
        group.bench_with_input(BenchmarkId::new("object", count), &count, |b, _| {
            b.iter(|| {
                let mut bytes = Vec::new();
                orishu_plugin::execution::encode_batch(&objects, &mut bytes, bench_limits())
                    .unwrap();
                std::hint::black_box(bytes)
            })
        });
    }
    group.finish();
}

fn bench_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("read");
    for count in counts() {
        let forces = encoded(&force_records(count));
        let objects = encoded(&object_records(count));
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("force", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(Batch::<Force>::read(&forces, bench_limits()).unwrap()))
        });
        group.bench_with_input(BenchmarkId::new("object", count), &count, |b, _| {
            b.iter(|| {
                std::hint::black_box(Batch::<ObjectState>::read(&objects, bench_limits()).unwrap())
            })
        });
    }
    group.finish();
}

fn bench_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("iterate");
    for count in counts() {
        let forces = encoded(&force_records(count));
        let batch = Batch::<Force>::read(&forces, bench_limits()).unwrap();
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("force", count), &count, |b, _| {
            b.iter(|| {
                let mut sum = 0.0;
                for record in batch.iter() {
                    sum += record.newtons[0].get();
                }
                std::hint::black_box(sum)
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_encode, bench_read, bench_iterate);
criterion_main!(benches);
