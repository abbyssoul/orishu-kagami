# High-performance Rust engineering guide

Status: **draft guideline**

This guide is for engineers and coding agents designing, reviewing, and
optimizing performance-sensitive Rust applications. It is intentionally
application-neutral. Individual projects may impose stronger requirements for
latency, determinism, numerical precision, allocation, or memory use.

## Guiding idea

High-performance Rust begins with the workload:

> Match algorithms, data layout, ownership, and execution strategy to the work
> repeated by the application, then validate the design on representative
> hardware and inputs.

Rust provides memory safety, predictable value semantics, and abstractions that
can compile efficiently. Those properties do not compensate for an unsuitable
algorithm, scattered data, unnecessary work, or allocation hidden inside a hot
API.

Performance is not one number. State which property matters:

- throughput;
- median or tail latency;
- frame or tick time;
- startup time;
- peak or steady-state memory;
- allocation rate;
- energy consumption; or
- scaling with input size or concurrency.

Correctness, safety, and required numerical behaviour remain constraints. A
faster implementation that changes required results is a different
implementation, not an optimization.

## Machine sympathy and data-oriented design

Machine sympathy means understanding the machine well enough to cooperate with
it. Relevant costs include the memory hierarchy, cache and translation misses,
memory bandwidth, prefetching, branch prediction, allocation, synchronization,
SIMD width, and available parallelism.

The CPU transfers memory in cache-line-sized blocks. A useful question is not
only "how many values are read?" but "how many useful bytes arrive with each
cache line?" Dense sequential traversal generally gives hardware prefetchers a
better opportunity than pointer chasing through unrelated allocations.

Data-oriented design starts from transformations over collections:

1. What operation is repeated?
2. Over how many elements and how often?
3. Which fields does each iteration read and write?
4. Which fetched fields are unused?
5. Can the work be streamed or performed in batches?
6. What storage and synchronization does the operation require?

Data layout matters most when many elements undergo the same operation. The
layout of one infrequently accessed object is rarely decisive; the layout of a
million elements traversed every frame often is.

### Array of structures and structure of arrays

An array of structures keeps each logical record together:

```text
[position, velocity, name] [position, velocity, name] ...
```

A structure of arrays keeps fields together:

```text
positions:  [p0 p1 p2 ...]
velocities: [v0 v1 v2 ...]
names:      [n0 n1 n2 ...]
```

Neither is universally faster. Choose from the access pattern:

- prefer records when most operations need most fields of one record;
- prefer columns when bulk operations need a small subset of fields;
- split hot numeric state from cold identity, strings, diagnostics, and
  metadata when they have different access frequencies;
- consider a hybrid layout when kernels naturally operate on small blocks.

Measure hot type sizes and padding with `std::mem::size_of`. Smaller frequently
instantiated types can reduce memory traffic and cache pressure. Rust's default
representation does not promise source-order field layout, so do not build
unsafe assumptions on observed layout. Use an explicit `repr` only when its
guarantees are required, not as a speculative optimization.

### ECS as an example, not a requirement

An archetypal entity-component system can store equal component sets densely.
A query over position and velocity then traverses only the relevant columns.
This can improve locality and avoid testing every object for missing data.

"ECS is cache friendly" is conditional on its storage model, query patterns,
component sizes, iteration order, and rate of structural change. Sparse-set
lookups, entity indirection, archetype movement, and scheduling can introduce
other costs. The reusable lesson is to design storage around the dominant
queries, not that every high-performance application needs an ECS.

It is often appropriate to keep a rich domain representation at an application
boundary and derive a dense computational representation for a hot subsystem.
Authoring, persistence, validation, simulation, rendering, and networking do
not need to share one in-memory layout. Make conversion costs and invalidation
rules explicit, and amortize them outside repeated loops.

## Start with algorithms and necessary work

Record the expected complexity of every important workload in terms meaningful
to the application, such as objects, samples, cells, bytes, or connections.

Prefer this optimization order:

1. Do not perform unnecessary work.
2. Select an algorithm with suitable scaling.
3. Arrange required data for efficient traversal.
4. Batch operations and amortize setup.
5. Reuse allocations and expensive resources.
6. Reduce branches, indirection, and instruction cost.
7. Parallelize or offload when the workload is large enough.

This is a decision order, not a claim that later items are always smaller.
Profiling may reveal a different immediate bottleneck.

Move invariant work out of repeated loops. Precompute stable coefficients,
indexes, filtered sets, dispatch plans, and converted representations when
inputs change. Do not recompute them for every element or tick.

Do not compute a rich result when the caller uses only one part of it. Separate
field-only from field-plus-derivative kernels, metadata from payloads, and fast
from diagnostic paths when the callers have different needs.

## Allocation is part of API design

Heap allocation inside a high-frequency loop is a major avoidable brake in many
applications. It adds allocator bookkeeping and deallocation, can introduce
thread contention and unpredictable latency, and often accompanies extra
initialization, copying, and memory traffic.

The strongest rule in this guide is:

> Do not hide allocation inside a hot-path API. Decide where storage is owned,
> when capacity is established, and how it is reused.

Allocation behaviour is usually fixed by the function contract before the
loop body is optimized. A convenient owned return value can force allocation
on every call:

```rust
fn transform_all(input: &[Input]) -> Vec<Output> {
    input.iter().map(transform).collect()
}
```

That API is reasonable when the result must escape as an independent owned
value. It is a poor default for a function called per frame, request, tick, or
batch when the caller could retain storage.

### Separate producing, materializing, and allocating

These are different operations:

1. producing a sequence of values;
2. materializing the values in memory; and
3. acquiring the memory that stores them.

Avoid materialization when the next operation can consume a sequence directly:

```rust
fn positive(values: &[i32]) -> impl Iterator<Item = i32> + '_ {
    values.iter().copied().filter(|value| *value > 0)
}

let total: i32 = positive(&values).sum();
```

Standard iterator adapters are lazy. Returning an iterator can therefore avoid
a temporary collection and allow the caller to fuse further operations.
Returning an iterator is not automatically faster: a complex chain may produce
worse code, returning it may complicate lifetimes, and callers that need
ownership or repeated traversal must materialize eventually. Inspect profiles
or generated code when the distinction matters. When ownership is required,
`collect` can use an iterator's size hint to reduce growth reallocations, but it
still creates a fresh owned collection for that call.

When materialization is required repeatedly, let the caller reuse capacity:

```rust
fn transform_all(input: &[Input], output: &mut Vec<Output>) {
    output.clear();
    output.reserve(input.len());
    output.extend(input.iter().map(transform));
}
```

After `clear`, `Vec` retains its allocation. `reserve` does nothing when the
existing capacity is sufficient. If an output size is known and fixed, prefer
a slice contract:

```rust
fn transform_all(input: &[Input], output: &mut [Output]) {
    assert_eq!(input.len(), output.len());

    for (source, destination) in input.iter().zip(output) {
        *destination = transform(source);
    }
}
```

An API should usually choose one of these contracts deliberately:

| Requirement | Useful contract |
| --- | --- |
| Consume results once | `impl Iterator<Item = T> + '_` or a callback |
| Write a known number of results | `&mut [T]` |
| Write a variable number repeatedly | `&mut Vec<T>` with retained capacity |
| Return an independent owned result | `Vec<T>` or `Box<[T]>` |
| Share an immutable result | `Arc<[T]>` or `Arc<T>` |
| Borrow normally, own only on mutation | `Cow<'a, T>` |

The table describes trade-offs, not mandatory type choices. `Arc` uses atomic
reference counting, `Cow` may clone on mutation, and a callback can make
control flow awkward. Choose the simplest contract that satisfies ownership
and measured performance requirements.

### Allocation checklist

Scrutinize these operations when they occur in a repeated path:

- `.collect::<Vec<_>>()`;
- `vec![...]`, `Vec::new`, and `String::new` followed by growth;
- `format!` and owned string conversion;
- `clone` or `to_owned` on collections and strings;
- rebuilding maps, sets, and temporary indexes;
- boxing each element independently;
- creating GPU buffers, command resources, or staging resources repeatedly;
- serialization into an intermediate `String` or `Vec<u8>`; and
- library calls whose ownership contract is unclear.

Ask how often the site runs, how many bytes it allocates, how long the allocation
lives, and whether ownership is necessary. Do not mechanically remove cold or
one-time allocations.

Useful techniques include:

- allocate and reserve before the hot loop;
- keep scratch buffers in the caller or long-lived worker;
- use `clear` plus `extend` rather than collect plus replace;
- use `clone_from` when replacing an existing allocation with cloned content;
- use fixed arrays for small compile-time bounds;
- consider `SmallVec` only when measured length distributions support its
  inline capacity and benchmark the additional branch and larger value size;
- retain persistent GPU and I/O buffers, growing them only when capacity is
  insufficient; and
- use arenas or pools when object lifetimes and bulk reclamation make them a
  natural fit, not merely to avoid calling the allocator.

Changing the global allocator can help some workloads, but it is not a
substitute for eliminating unnecessary allocation or fixing ownership design.

## Borrow, move, and share deliberately

Prefer borrowing when the callee only observes data:

```rust
fn evaluate(samples: &[Sample]) -> Summary;
fn label_text(label: &str) -> ParsedLabel;
```

Accept ownership when the function stores, transforms in place, or transfers
the value. Moving a `Vec<T>` or `String` moves its small header; it does not copy
the heap buffer. Avoid cloning merely to satisfy a short-lived borrow problem;
first reconsider scopes, field decomposition, and function boundaries.

A clone is not one cost category:

- cloning `Vec<T>` or `String` normally allocates and copies elements or bytes;
- cloning `Arc<T>` shares the same allocation but updates an atomic reference
  count;
- cloning a small `Copy`-like value may be trivial;
- `clone_from` may reuse the destination's allocation.

Use shared immutable storage when several consumers need the same large value,
but do not wrap everything in `Arc`. Reference counting introduces indirection,
heap allocation, and—in `Arc`'s case—atomic operations.

Prefer compact numeric handles or indexes in repeated lookups when identity
strings or rich keys are only needed at boundaries. Build indexes when inputs
change, then reuse them. Account for hashing, pointer stability, deletion, and
invalidation rather than assuming a hash map is the fastest lookup.

## Design loops for predictable execution

Hot loops should make their data dependencies and work count obvious:

- traverse contiguous slices when practical;
- keep loop bodies small and move error formatting or logging out of them;
- filter invariant cases before entering the loop;
- hoist stable calculations;
- avoid repeated hash and string lookup;
- batch calls across elements instead of dispatching once per element;
- write independent iterations in a shape the compiler can vectorize; and
- keep validation required for correctness, moving it only when equivalent
  semantics can be demonstrated and tested.

Iterator syntax and explicit loops can both compile well. Choose the clearest
form, benchmark important cases, and examine assembly or optimization reports
only after profiling narrows the question. Iterator adapters do not allocate by
themselves; terminal operations and captured work determine the cost.

Bounds checks are usually cheap and are often eliminated. Prefer safe indexing,
iteration, zipping, and exact chunks that make bounds evident to the compiler.
Use unchecked access only after measurement demonstrates a material remaining
cost and the safety invariant is small, documented, and thoroughly tested.

Branch-free code is not automatically faster. Predictable branches are cheap;
extra arithmetic or memory traffic used to remove them may be worse. Prefer to
remove branches whose outcome is invariant or highly unpredictable in a proven
hot loop.

## Concurrency, SIMD, and accelerators

Parallelism changes the cost model; it does not erase work.

- Batch enough work to amortize scheduling and synchronization.
- Partition contiguous, independent ranges where possible.
- Avoid shared mutation, fine-grained locks, and allocator contention.
- Watch for false sharing when threads write different values on the same cache
  line.
- Measure scaling at realistic thread counts and input sizes.
- Expect memory-bandwidth-bound workloads to saturate before all cores do.
- Preserve ordering and numerical determinism when the application requires
  them.

Use SIMD-friendly layouts and simple loops before reaching for explicit SIMD.
Use CPU threading, asynchronous I/O, or GPU execution according to the type of
work; they solve different problems. Keep blocking or unbounded-latency work off
latency-sensitive event-loop and executor threads.

## Build configuration is part of performance

Measure optimized artifacts. Cargo's default development and test profiles are
not representative of release runtime performance; the benchmark profile
inherits from the release profile.

Release options such as link-time optimization and fewer code-generation units
can improve runtime performance at the cost of build time. Profile-guided
optimization can use representative execution profiles to guide inlining,
machine-code layout, and other compiler decisions. These options must be
benchmarked for each deployable artifact rather than copied blindly between
projects.

Keep enough debug information to obtain useful profiles when practical. Confirm
the target CPU and deployment portability requirements before enabling
target-specific instructions.

## Measurement workflow

Optimization without evidence easily moves cost elsewhere or improves an
unrepresentative microbenchmark.

### 1. Define the workload and budget

Record:

- representative inputs and adversarial sizes;
- the metric and target;
- expected algorithmic complexity;
- hardware, operating system, toolchain, and build profile; and
- correctness and determinism requirements.

### 2. Establish a baseline

Use release builds, warmups where appropriate, repeated samples, and stable
inputs. Measure end-to-end workloads first. Add microbenchmarks to isolate a
known cost, not to replace application-level evidence.

Use `std::hint::black_box` in microbenchmarks where compiler removal or constant
folding would make the workload unrealistic. It is a best-effort benchmarking
hint, not a correctness or security mechanism.

### 3. Profile the limiting resource

Choose evidence that matches the question:

- sampling profiles and flame graphs for CPU time;
- hardware counters for cache, branch, and instruction behaviour;
- allocation profilers for allocation count, bytes, lifetime, and peak memory;
- resident-set and mapped-memory tools when process memory differs from the
  language heap;
- I/O and scheduler timelines for latency stalls; and
- GPU timestamp queries and vendor tools for accelerator work.

An absent signal is useful evidence. Do not optimize the Rust heap to explain
memory owned by a driver, mapped file, or external library.

### 4. Form and test one hypothesis

State the suspected cause and predicted effect. Prefer the smallest change that
tests it. Preserve a correctness test and benchmark for the affected workload.

### 5. Compare and inspect scaling

Report absolute values as well as percentages. Check variance and relevant tail
latencies. Sweep input size to distinguish fixed costs, linear work, cache
cliffs, and worse complexity. A fitted exponent can be misleading when the
workload is `fixed cost + O(n)`.

### 6. Keep or revert based on evidence

Retain the change when the benefit is meaningful, repeatable, and worth its
complexity. Document negative results and deferred opportunities so they are
not repeatedly rediscovered.

## Review checklist for engineers and agents

Before implementing performance-sensitive code:

- [ ] Name the repeated workload and its frequency.
- [ ] State expected time and space complexity.
- [ ] Identify the fields touched by the hot operation.
- [ ] Choose a layout for those access patterns.
- [ ] Decide who owns scratch and result storage.
- [ ] Decide whether results should stream, borrow, reuse storage, or be owned.
- [ ] Keep invariant construction and conversion outside the repeated loop.
- [ ] Establish a representative correctness test and baseline.

While reviewing a hot path:

- [ ] Look for hidden `collect`, `clone`, `format!`, boxing, and serialization.
- [ ] Check capacity growth and whether buffers can be retained.
- [ ] Check for repeated map, string, or dynamic-dispatch setup.
- [ ] Check for unused outputs and unconditional diagnostics.
- [ ] Check whether the API prevents batching.
- [ ] Check algorithmic scaling before micro-optimizing instructions.
- [ ] Check blocking calls and synchronization on latency-sensitive threads.
- [ ] Verify claims with a profiler or benchmark rather than source inspection
      alone.

Before accepting an optimization:

- [ ] Run correctness and regression tests.
- [ ] Compare the same workload and build configuration before and after.
- [ ] Report hardware, inputs, absolute measurements, variance, and allocation
      changes.
- [ ] Test relevant input-size and concurrency scaling.
- [ ] Explain any safety, readability, memory, portability, or determinism cost.
- [ ] Re-check comments and older performance findings against current code.

## Common mistakes

- Optimizing code that is not hot.
- Treating every allocation as equally harmful.
- Returning an iterator when callers inevitably need an owned collection.
- Assuming iterator chains are always faster than loops, or vice versa.
- Replacing `Vec` with `SmallVec`, an arena, or a custom allocator without a
  measured workload.
- Adding `Arc` to avoid copies when atomic reference counting and indirection
  cost more.
- Parallelizing work too small to amortize scheduling.
- Using `unsafe` as a performance technique without evidence.
- Reading only average timing and missing tail latency or cache cliffs.
- Reporting a percentage without absolute time or workload size.
- Changing output order, floating-point reduction order, or precision without
  treating it as a semantic change.
- Keeping performance findings as permanent truth after the code changes.

## Documenting a performance change

A performance report should contain:

1. **Goal:** workload and user-visible reason.
2. **Baseline:** command, inputs, build, hardware, and measurements.
3. **Expected complexity:** variables and anticipated scaling.
4. **Evidence:** profile or allocation data locating the cost.
5. **Root cause:** algorithm, layout, ownership, allocation, synchronization,
   I/O, or code generation.
6. **Change:** what was changed and which contracts were affected.
7. **Correctness:** tests and semantic equivalence, including numerical or
   ordering effects.
8. **Result:** before and after absolute values, percentages, variance, memory,
   and scaling.
9. **Trade-offs:** complexity, compile time, memory, portability, and
   maintainability.
10. **Reproduction:** exact commands and known limitations.

## The Field CAD proof of concept as a case study

Performance investigations in the predecessor Field CAD project illustrate
several general lessons:

- sampling and unconditional diagnostics can cost more than the central
  simulation step, so measure the entire pipeline;
- a cache-capacity transition can appear as a performance cliff rather than a
  smooth slope;
- fixed work can make a linear algorithm appear sublinear over small inputs;
- full-grid clones, grid-sized scratch buffers, and per-dispatch GPU resource
  creation make allocation and ownership architectural concerns;
- removing a lookup from a pairwise force calculation does not change the
  algorithm's pairwise complexity;
- an API that evaluates one body at a time prevents batch SIMD, threading, or
  GPU execution;
- computing derivatives that a force path discards is unnecessary work; and
- profiling process, Rust heap, and GPU memory separately can disprove an
  attractive but incorrect explanation.

Those reports remain historical evidence in the predecessor repository, not a
permanent backlog. Revalidate every finding against current monorepo code before
using it to justify a change; see the [migration strategy](migration.md).

## References

- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Heap allocations — The Rust Performance Book](https://nnethercote.github.io/perf-book/heap-allocations.html)
- [Iterators — The Rust Performance Book](https://nnethercote.github.io/perf-book/iterators.html)
- [Type sizes — The Rust Performance Book](https://nnethercote.github.io/perf-book/type-sizes.html)
- [Benchmarking — The Rust Performance Book](https://nnethercote.github.io/perf-book/benchmarking.html)
- [`Vec<T>` — Rust standard library](https://doc.rust-lang.org/std/vec/struct.Vec.html)
- [Iterators — Rust standard library](https://doc.rust-lang.org/stable/core/iter/)
- [`Cow` — Rust standard library](https://doc.rust-lang.org/std/borrow/enum.Cow.html)
- [`Arc<T>` — Rust standard library](https://doc.rust-lang.org/std/sync/struct.Arc.html)
- [Type layout — The Rust Reference](https://doc.rust-lang.org/stable/reference/type-layout.html)
- [Cargo build profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Profile-guided optimization — The rustc book](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
- [`std::hint::black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html)
