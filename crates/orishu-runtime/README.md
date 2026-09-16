# Shared scientific runtime

WIT/grant/isolated field and Dynamics lifecycles, atomic fixed-profile execution
and selected workload-v3 admission are implemented. This crate is the one deep execution boundary for
Kagami's local runtime and an Orishu worker, not another plugin installer or
authoring authority. Neither application is integrated yet.

The [accepted X-PLUGIN contract](../../docs/plugin-contract-v1-draft.md) and
[O-WASM task](../../docs/tasks/implement-wasm-component-graph-host.md) define the
required end state. Shared [WIT](../orishu-plugin/wit/simulation.wit) lives with the
dependency-light plugin contract; only this host crate depends on an engine.

Wasmtime is pinned to 48.0.2 (Rust requirement 1.95, compatible with this workspace's
1.97 toolchain), with only runtime/Cranelift/Component Model/std/error support.
WASI is not linked. Do not enable threads, async guest execution, filesystem code
caches or plugin-native loading implicitly. Runtime dependencies do not flow back
into workload, plugin, variables or authoring contract crates. Review security
advisories when updating the engine; immutable workload identity names component
bytes and execution provenance, not a JIT cache.

See the [ABI checkpoint and explicit security gaps](../../docs/runtime-component-abi.md)
before using this host. Fixed write frames bound canonical lifting, and operations
support exact grants as well as explicit byte/value ceilings for opaque natural
field/history initialization; returned state carries actual extents, not padding.
Operations have independent deadlines/cancellation. Loaded-state validation, field advance,
isolated sampling, field checkpoint/restore, Dynamics integration and explicit
history/membership transitions are exercised with separate actual Component
artifacts. The [fixed-profile owner](../../docs/runtime-fixed-run.md) now composes
them into atomic object/field/history boundaries, complete in-memory portable
checkpoints and bounded detached field-snapshot leases. `admit` constructs that owner
from an independently verified v3 scientific closure, checking actual Component
contracts and captured state without initialization. Aggregate/JIT budgets,
application integration and hot-path store/arena reuse remain.
The original lifecycle fixtures are not physics. A separate
[classical symplectic Euler and Newtonian pair](../../plugins/reference/README.md)
now has real coupled numerical and restart-continuation evidence through shared
instance/validation envelopes and `invoke_*_bound`. Selected admission tests use
real release evidence; the worker now has an off-owner validated admission handoff,
with parent-linked lease cancellation. Its retained execution adapter now uses
`advance_with_commit` to fence publication through the formation owner; run
allocation and public endpoints remain. Generic requested-channel
sampling now uses checked shared packets, including exact state/context identity,
channel subsets/order, unavailable/invalid cells, precision and quality. Run-owned
snapshot leases are implemented; observer/cache/product adapters remain work.
The [scientific bulk IO](../../docs/scientific-bulk-io.md) packets and reusable
`ForceReducer` now check exact field/response coverage, slot bounds and stable-order
finite sums; the reference kernels, fixed owner and independent workload admission
use them. Raw `invoke_*` methods are low-level ABI
test surfaces; even `invoke_*_bound` does not prove selected closure admission.

Primary references checked while implementing:
[engine releases](https://github.com/bytecodealliance/wasmtime/releases/),
[fuel and configuration](https://docs.rs/wasmtime/latest/wasmtime/struct.Config.html),
[component encoding](https://docs.rs/wit-component/latest/wit_component/struct.ComponentEncoder.html).
These are tooling references, not independent proof of Orishu conformance.

## Benchmarks

```sh
cargo bench -p orishu-runtime --bench runtime
```

The benchmark isolates the pure, engine-free per-step numeric path — the
deterministic `ForceReducer` — over synthetic coupled/force packets, scaling the
entity count (fixed fields) and the field count (fixed entities). Sizes are
overridable via `ORISHU_RUNTIME_BENCH_ENTITIES` and `ORISHU_RUNTIME_BENCH_FIELDS`.
The reducer's workspace is reused across iterations, so a steady result also
shows capacity is not being reallocated per step.
