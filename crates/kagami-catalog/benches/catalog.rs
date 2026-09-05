//! Microbenchmarks for `kagami-catalog` in isolation: how fast a catalog
//! parses, resolves, evaluates, and materialises.
//!
//! Deliberately synthetic and IO-free, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: catalogs are generated
//! deterministically in memory and fed through [`parse_stream`] rather than
//! read from disk, so a result is comparable across commits at a fixed
//! entry count and does not measure the filesystem. Per-allocation cost (as
//! opposed to throughput) is out of scope for Criterion and is covered by the
//! `dhat`-gated `examples/profile_catalog.rs`.
//!
//! Four shapes are benchmarked, because they scale for different reasons:
//!
//! - **parse**: `count` independent templates through the YAML reader and
//!   structural validation. Dominated by the YAML scanner and expression
//!   parsing; no cross-references, so nothing resolves.
//! - **resolve**: the same already-parsed documents through the pure decision
//!   core — projection, reference checks, evaluation, schema checks. This is
//!   what a reload costs once bytes are in memory.
//! - **evaluate**: reading every published binding's value from a resolved
//!   set, the interactive-preview path.
//! - **materialize**: instantiating the tail of a `depth`-long chain of
//!   templates that each read the previous one, so the transitive closure the
//!   candidate must copy and rewrite grows with `depth`. This measures the
//!   part of instantiation that is not bounded by template size.

use std::path::Path;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kagami_catalog::{
    CatalogSet, ComponentName, ComponentSchema, ComponentTypeId, Dimension, InstantiationRequest,
    Limits, ParsedDocument, PluginId, PropertyKind, PropertyName, PropertySchema, SchemaRegistry,
    SchemaVersion, TemplateIdentity, materialize, parse_stream, resolve,
};
use orishu_variables::Namespace;

const DEFAULT_COUNTS: [usize; 4] = [10, 100, 1_000, 5_000];
const DEFAULT_CHAIN_DEPTHS: [usize; 4] = [10, 50, 200, 1_000];

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

/// Template counts for the `parse`, `resolve`, and `evaluate` groups,
/// overridable via `KAGAMI_CATALOG_BENCH_COUNTS`.
fn counts() -> Vec<usize> {
    env_list("KAGAMI_CATALOG_BENCH_COUNTS", &DEFAULT_COUNTS)
}

/// Dependency chain depths for the `materialize` group, overridable via
/// `KAGAMI_CATALOG_BENCH_CHAIN_DEPTHS`.
fn chain_depths() -> Vec<usize> {
    env_list("KAGAMI_CATALOG_BENCH_CHAIN_DEPTHS", &DEFAULT_CHAIN_DEPTHS)
}

fn registry() -> SchemaRegistry {
    SchemaRegistry::new()
        .with(
            ComponentSchema::new(
                ComponentTypeId::new(
                    PluginId::new("bench.sources").unwrap(),
                    ComponentName::new("inertial_mass").unwrap(),
                ),
                SchemaVersion(1),
            )
            .with_property(
                PropertyName::new("mass").unwrap(),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            ),
        )
        .with(
            ComponentSchema::new(
                ComponentTypeId::new(
                    PluginId::new("bench.geometry").unwrap(),
                    ComponentName::new("sphere").unwrap(),
                ),
                SchemaVersion(1),
            )
            .with_property(
                PropertyName::new("radius").unwrap(),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::LENGTH,
                }),
            ),
        )
}

fn document(name: &str, mass: &str, radius: &str) -> String {
    format!(
        "---
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata:
  catalog: bench
  name: {name}
  description: generated benchmark template
spec:
  helpers:
    scale: {{expression: \"1.0\", unit: kg}}
  components:
  - type: {{plugin: bench.sources, name: inertial_mass}}
    properties:
      mass: {{quantity: \"{mass}\"}}
  - type: {{plugin: bench.geometry, name: sphere}}
    properties:
      radius: {{quantity: {{expression: \"{radius}\", unit: km}}}}
"
    )
}

/// `count` independent templates: no cross-references, so every entry
/// resolves on its own.
fn flat_catalog(count: usize) -> String {
    (0..count)
        .map(|index| {
            document(
                &format!("body_{index}"),
                &format!("{}.0e24", index % 9 + 1),
                &format!("{}.0e3", index % 7 + 1),
            )
        })
        .collect()
}

/// A `depth`-long chain: `body_0` is a literal, and each later template reads
/// the previous one's mass, so materialising the tail must capture the whole
/// chain.
fn chain_catalog(depth: usize) -> String {
    (0..depth)
        .map(|index| {
            let mass = if index == 0 {
                "1.0e24".to_owned()
            } else {
                format!("bench.body_{}.mass * 1.01", index - 1)
            };
            document(&format!("body_{index}"), &mass, "1.0e3")
        })
        .collect()
}

fn parse(text: &str) -> Vec<ParsedDocument> {
    parse_stream(Path::new("bench.yaml"), text, &Limits::DEFAULT).documents
}

fn resolved(text: &str) -> CatalogSet {
    resolve(parse(text), Vec::new(), &registry(), &Limits::DEFAULT)
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for count in counts() {
        let text = flat_catalog(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("flat", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(parse(&text)))
        });
    }
    group.finish();
}

fn bench_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve");
    let registry = registry();
    for count in counts() {
        let text = flat_catalog(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("flat", count), &count, |b, _| {
            b.iter_batched(
                || parse(&text),
                |documents| {
                    std::hint::black_box(resolve(
                        documents,
                        Vec::new(),
                        &registry,
                        &Limits::DEFAULT,
                    ))
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_evaluate(c: &mut Criterion) {
    let mut group = c.benchmark_group("evaluate");
    for count in counts() {
        let set = resolved(&flat_catalog(count));
        let bindings: Vec<_> = set
            .projection()
            .bindings()
            .map(|(reference, _)| reference)
            .collect();
        group.throughput(Throughput::Elements(bindings.len() as u64));
        group.bench_with_input(BenchmarkId::new("all_bindings", count), &count, |b, _| {
            b.iter(|| {
                let mut total = 0.0;
                for &binding in &bindings {
                    total += set.projection().value(binding).unwrap();
                }
                std::hint::black_box(total)
            })
        });
    }
    group.finish();
}

fn bench_materialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("materialize");
    let registry = registry();
    for depth in chain_depths() {
        let set = resolved(&chain_catalog(depth));
        let request = InstantiationRequest::new(
            TemplateIdentity::new(
                "bench".try_into().unwrap(),
                format!("body_{}", depth - 1).try_into().unwrap(),
            ),
            Namespace::new("objects.o1"),
        );
        // Guards against silently benchmarking a rejection.
        materialize(&set, &registry, &request).expect("the chain tail is available");
        group.throughput(Throughput::Elements(depth as u64));
        group.bench_with_input(BenchmarkId::new("chain_tail", depth), &depth, |b, _| {
            b.iter(|| std::hint::black_box(materialize(&set, &registry, &request).unwrap()))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_parse,
    bench_resolve,
    bench_evaluate,
    bench_materialize
);
criterion_main!(benches);
