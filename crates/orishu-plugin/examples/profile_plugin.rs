//! Headless allocation profiling for `orishu-plugin`'s bulk-IO packets: how
//! many allocations (and bytes) does encoding, reading, and iterating a packet
//! cost.
//!
//! `criterion` measures wall-clock throughput (see `benches/plugin.rs`); it has
//! no notion of allocation counts. This example fills that gap and exists mainly
//! to check the crate's load-bearing claim: reading a packet allocates a bounded
//! amount regardless of record count, and iterating a validated packet allocates
//! nothing at all. The `blocks/record` column is what proves it — it should
//! trend to zero as the record count grows.
//!
//! Build and run with the `dhat` feature to also capture a full
//! `dhat-heap.json` call-site profile (viewable at
//! <https://nnethercote.github.io/dh_view/dh_view.html>):
//!
//! ```sh
//! cargo run --release -p orishu-plugin --example profile_plugin --features dhat
//! ```
//!
//! Without `--features dhat` this still runs and prints wall-clock timings, but
//! the allocation columns report zero (no counting allocator installed).

use std::time::Instant;

use orishu_plugin::{
    FiniteF64,
    execution::{Batch, BulkLimits, EntityId, ObjectState, encode_batch},
};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const COUNT: usize = 262_144;

fn limits() -> BulkLimits {
    BulkLimits {
        bytes: 1 << 30,
        records: 1 << 21,
    }
}

fn f(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}

fn object_records(count: usize) -> Vec<ObjectState> {
    (0..count as u64)
        .map(|id| ObjectState {
            id: EntityId(id),
            kinematics: orishu_plugin::execution::Kinematics {
                position_metres: [f(1.0), f(2.0), f(3.0)],
                velocity_metres_per_second: [f(0.1), f(0.2), f(0.3)],
            },
            inertial_mass_kilograms: (id % 2 == 0).then(|| f(1.5)),
        })
        .collect()
}

fn main() {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::builder().build();

    let records = object_records(COUNT);

    report_phase("encode", COUNT, || {
        let mut bytes = Vec::new();
        encode_batch(&records, &mut bytes, limits()).unwrap();
        std::hint::black_box(bytes);
    });

    let mut bytes = Vec::new();
    encode_batch(&records, &mut bytes, limits()).unwrap();

    report_phase("read (validate all)", COUNT, || {
        std::hint::black_box(Batch::<ObjectState>::read(&bytes, limits()).unwrap());
    });

    let batch = Batch::<ObjectState>::read(&bytes, limits()).unwrap();
    report_phase("iterate", COUNT, || {
        let mut sum = 0.0;
        for record in batch.iter() {
            sum += record.kinematics.position_metres[0].get();
        }
        std::hint::black_box(sum);
    });

    #[cfg(not(feature = "dhat"))]
    eprintln!(
        "note: built without --features dhat, so allocation columns above are always zero \
         and no dhat-heap.json was written"
    );
}

/// Runs `work`, printing wall-clock time and (with `--features dhat`) the
/// allocation delta it caused, normalized per record.
fn report_phase(label: &str, records: usize, work: impl FnOnce()) {
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
            "{label:<22} {elapsed:>10.2?}  {blocks:>8} blocks  {bytes:>12} bytes  \
             ({:.5} blocks/record)",
            blocks as f64 / records as f64,
        );
    }
    #[cfg(not(feature = "dhat"))]
    {
        let _ = records;
        eprintln!("{label:<22} {elapsed:>10.2?}");
    }
}
