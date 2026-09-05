//! Headless allocation profiling for `kagami-catalog`: how many allocations
//! (and bytes) does loading, resolving, and instantiating a catalog cost.
//!
//! `criterion` measures wall-clock throughput (see `benches/catalog.rs`); it
//! has no notion of allocation counts. This example fills that gap by
//! wrapping phases of the same synthetic workloads in [`dhat::HeapStats`]
//! snapshots and printing the allocation delta each phase caused. Catalog
//! loading is not a hot path, but it is on the interactive path for every
//! reload, and a per-template allocation count is the number that tells you
//! whether a change to the format or the projection made that worse.
//!
//! Build and run with the `dhat` feature to also capture a full
//! `dhat-heap.json` call-site profile (viewable at
//! <https://nnethercote.github.io/dh_view/dh_view.html>):
//!
//! ```sh
//! cargo run --release -p kagami-catalog --example profile_catalog --features dhat
//! ```
//!
//! Without `--features dhat` this still runs and prints wall-clock timings,
//! but the allocation columns report zero (no counting allocator installed).

use std::path::Path;
use std::time::Instant;

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, InstantiationRequest, Limits,
    ParsedDocument, PluginId, PropertyKind, PropertyName, PropertySchema, SchemaRegistry,
    SchemaVersion, TemplateIdentity, materialize, parse_stream, resolve,
};
use orishu_variables::Namespace;

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const TEMPLATE_COUNT: usize = 2_000;
const CHAIN_DEPTH: usize = 500;
const EVAL_REPS: usize = 10;

fn main() {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::builder().build();

    let flat = flat_catalog(TEMPLATE_COUNT);
    report_phase("parse flat", TEMPLATE_COUNT, || {
        std::hint::black_box(parse(&flat));
    });

    let registry = registry();
    let documents = parse(&flat);
    report_phase("resolve flat", TEMPLATE_COUNT, || {
        std::hint::black_box(resolve(documents, Vec::new(), &registry, &Limits::DEFAULT));
    });

    let set = resolve(parse(&flat), Vec::new(), &registry, &Limits::DEFAULT);
    let bindings: Vec<_> = set
        .projection()
        .bindings()
        .map(|(reference, _)| reference)
        .collect();
    report_phase(
        "evaluate bindings (x10 passes)",
        bindings.len() * EVAL_REPS,
        || {
            for _ in 0..EVAL_REPS {
                for &binding in &bindings {
                    std::hint::black_box(set.projection().value(binding).unwrap());
                }
            }
        },
    );

    let chain = resolve(
        parse(&chain_catalog(CHAIN_DEPTH)),
        Vec::new(),
        &registry,
        &Limits::DEFAULT,
    );
    let request = InstantiationRequest::new(
        TemplateIdentity::new(
            "bench".try_into().expect("a valid catalog name"),
            format!("body_{}", CHAIN_DEPTH - 1)
                .try_into()
                .expect("a valid template name"),
        ),
        Namespace::new("objects.o1"),
    );
    report_phase("materialize chain tail", CHAIN_DEPTH, || {
        std::hint::black_box(materialize(&chain, &registry, &request).unwrap());
    });

    #[cfg(not(feature = "dhat"))]
    eprintln!(
        "note: built without --features dhat, so allocation columns above are always zero \
         and no dhat-heap.json was written"
    );
}

fn registry() -> SchemaRegistry {
    SchemaRegistry::new().with(
        ComponentSchema::new(
            ComponentTypeId::new(
                PluginId::new("bench.sources").expect("a valid plugin identifier"),
                ComponentName::new("inertial_mass").expect("a valid component name"),
            ),
            SchemaVersion(1),
        )
        .with_property(
            PropertyName::new("mass").expect("a valid property name"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::MASS,
            }),
        ),
    )
}

fn document(name: &str, mass: &str) -> String {
    format!(
        "---
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {{catalog: bench, name: {name}}}
spec:
  components:
  - type: {{plugin: bench.sources, name: inertial_mass}}
    properties:
      mass: {{quantity: \"{mass}\"}}
"
    )
}

fn flat_catalog(count: usize) -> String {
    (0..count)
        .map(|index| document(&format!("body_{index}"), &format!("{}.0e24", index % 9 + 1)))
        .collect()
}

fn chain_catalog(depth: usize) -> String {
    (0..depth)
        .map(|index| {
            let mass = if index == 0 {
                "1.0e24".to_owned()
            } else {
                format!("bench.body_{}.mass * 1.01", index - 1)
            };
            document(&format!("body_{index}"), &mass)
        })
        .collect()
}

fn parse(text: &str) -> Vec<ParsedDocument> {
    parse_stream(Path::new("bench.yaml"), text, &Limits::DEFAULT).documents
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
            "{label:<32} {elapsed:>10.2?}  {blocks:>10} blocks  {bytes:>12} bytes  \
             ({:.2} blocks/unit, {:.1} bytes/unit)",
            blocks as f64 / unit_count as f64,
            bytes as f64 / unit_count as f64,
        );
    }
    #[cfg(not(feature = "dhat"))]
    {
        let _ = unit_count;
        eprintln!("{label:<32} {elapsed:>10.2?}");
    }
}
