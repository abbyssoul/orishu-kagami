//! Microbenchmarks for `orishu-runtime`'s pure per-step numeric path: the
//! deterministic [`ForceReducer`], which folds every selected field's force
//! packet into one net force per dynamic entity.
//!
//! Deliberately synthetic and engine-free, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: the Wasm host is not involved, just
//! deterministically generated coupled/force packets, so a result is comparable
//! across commits. This isolates the part of a step that is pure computation —
//! the reduction is `O(F log F + C log E + E)` and reuses its workspace across
//! steps — where a regression is a design problem rather than an engine one.
//!
//! Two shapes are benchmarked, because the reduction scales with two things:
//!
//! - **entities**: a fixed field count, scaling the entity count `E`. Dominated
//!   by the per-entity accumulate and the coupled-record lookup.
//! - **fields**: a fixed entity count, scaling the field count `F`. Dominated by
//!   the field-order sort and the aggregate record walk.
//!
//! The reducer is reused across iterations, so a steady result also shows the
//! workspace capacity is not being reallocated per step.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_plugin::{
    FiniteF64,
    execution::{
        Batch, BulkLimits, BulkRecord, CoupledEntity, CouplingSlot, DynamicEntity, EntityId, Force,
        Kinematics,
    },
};
use orishu_runtime::{FieldForces, ForceReducer, ReductionLimits};
use orishu_workload::ComponentInstanceId;

/// Aggregate limits wide enough for whatever entity and field counts the
/// parameters below ask for; `ReductionLimits::default` is an admission ceiling.
fn bench_reduction_limits() -> ReductionLimits {
    ReductionLimits {
        fields: 1 << 20,
        entities: 1 << 24,
        field_records: 1 << 30,
    }
}

const DEFAULT_ENTITIES: [usize; 4] = [64, 512, 4_096, 16_384];
const DEFAULT_FIELDS: [usize; 4] = [1, 4, 16, 64];
const FIXED_FIELDS: usize = 3;
const FIXED_ENTITIES: usize = 1_024;

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

fn entity_counts() -> Vec<usize> {
    env_list("ORISHU_RUNTIME_BENCH_ENTITIES", &DEFAULT_ENTITIES)
}

fn field_counts() -> Vec<usize> {
    env_list("ORISHU_RUNTIME_BENCH_FIELDS", &DEFAULT_FIELDS)
}

fn f(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}

fn kinematics() -> Kinematics {
    Kinematics {
        position_metres: [f(0.0); 3],
        velocity_metres_per_second: [f(0.0); 3],
    }
}

fn entity(id: u64) -> DynamicEntity {
    DynamicEntity {
        id: EntityId(id),
        kinematics: kinematics(),
        inertial_mass_kilograms: f(1.0),
    }
}

fn coupled(id: u64) -> CoupledEntity {
    CoupledEntity {
        id: EntityId(id),
        slot: CouplingSlot(0),
        kinematics: kinematics(),
        has_dynamics: true,
        source_si: Some(f(1.0)),
        response_si: Some(f(1.0)),
    }
}

fn force(id: u64) -> Force {
    Force {
        id: EntityId(id),
        newtons: [f(1.0), f(0.0), f(0.0)],
    }
}

fn encode<R: BulkRecord>(records: &[R]) -> Vec<u8> {
    let mut output = Vec::new();
    orishu_plugin::execution::encode_batch(records, &mut output, BulkLimits::default())
        .expect("records encode");
    output
}

/// Owns one field's encoded packets so its borrowed [`FieldForces`] view can be
/// reconstructed cheaply.
struct Field {
    id: ComponentInstanceId,
    coupled: Vec<u8>,
    forces: Vec<u8>,
}

impl Field {
    fn new(index: usize, entities: usize) -> Self {
        let coupled: Vec<CoupledEntity> = (0..entities as u64).map(coupled).collect();
        let forces: Vec<Force> = (0..entities as u64).map(force).collect();
        Self {
            id: ComponentInstanceId::new(format!("field-{index:06}")).unwrap(),
            coupled: encode(&coupled),
            forces: encode(&forces),
        }
    }

    fn view(&self) -> FieldForces<'_> {
        FieldForces {
            instance: &self.id,
            coupling_slots: 1,
            coupled: Batch::read(&self.coupled, BulkLimits::default()).unwrap(),
            forces: Batch::read(&self.forces, BulkLimits::default()).unwrap(),
        }
    }
}

/// The entity packet, owned so a [`Batch`] view can borrow it across iterations.
fn entities_packet(entities: usize) -> Vec<u8> {
    let records: Vec<DynamicEntity> = (0..entities as u64).map(entity).collect();
    encode(&records)
}

fn run_reduce(fields: usize, entities: usize, group_name: &str, key: usize, c: &mut Criterion) {
    let limits = bench_reduction_limits();
    let packet = entities_packet(entities);
    let owned: Vec<Field> = (0..fields)
        .map(|index| Field::new(index, entities))
        .collect();
    // Field ids are generated in ascending order, so this is already the
    // admitted canonical field set the reducer expects.
    let selected: Vec<ComponentInstanceId> = owned.iter().map(|field| field.id.clone()).collect();
    let views: Vec<FieldForces<'_>> = owned.iter().map(Field::view).collect();
    let entity_batch = Batch::<DynamicEntity>::read(&packet, BulkLimits::default()).unwrap();

    let mut reducer = ForceReducer::default();
    // Warm the workspace so the benchmarked steps measure reuse, not first-touch
    // growth.
    reducer
        .reduce(&entity_batch, &selected, &views, limits)
        .expect("reduction succeeds");

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements((fields * entities) as u64));
    group.bench_with_input(BenchmarkId::new("reduce", key), &key, |b, _| {
        b.iter(|| {
            let reduced = reducer
                .reduce(&entity_batch, &selected, &views, limits)
                .unwrap();
            std::hint::black_box(reduced.len())
        })
    });
    group.finish();
}

fn bench_entities(c: &mut Criterion) {
    for entities in entity_counts() {
        run_reduce(FIXED_FIELDS, entities, "reduce_entities", entities, c);
    }
}

fn bench_fields(c: &mut Criterion) {
    for fields in field_counts() {
        run_reduce(fields, FIXED_ENTITIES, "reduce_fields", fields, c);
    }
}

criterion_group!(benches, bench_entities, bench_fields);
criterion_main!(benches);
