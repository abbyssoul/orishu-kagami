# Implement the WebAssembly component-graph host

Status: **initial WIT/grant/isolated field and Dynamics lifecycle foundation implemented;
fixed-profile graph execution/admission implemented; security/product gates remain**.
Work package: **O-WASM** ([roadmap](../roadmap/README.md))  
Decisions: [ADR 0009](../adr/0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md)  
Protocol: [Workload contract](../protocol-workload.md)

See the [current ABI checkpoint](../runtime-component-abi.md) for actual Component
execution evidence and remaining gates, notably aggregate/JIT budgets, structured
attribution, scientific projections/validation, reusable storage and graph commit.
Fixed-frame pre-lift bounds and per-operation deadline/cancellation are implemented.
Shared [scientific bulk packets and force reduction](../scientific-bulk-io.md)
are implemented, including exact instance/validation envelopes. Real classical
Euler and Newtonian Components consume them through the bound host API with
coupled numerical and restart-continuation evidence. The [fixed run owner](../runtime-fixed-run.md)
now commits single-partition fields/objects/history atomically, restores complete
portable in-memory checkpoints and grants bounded detached field-snapshot leases.
Shared selected workload-v3 admission now verifies exact releases, scientific
inputs and graph before compiling and validating actual Components. Reusable
stores/arenas, variable output sizing,
membership scheduling and product/recording adapters remain work.
This foundation does not imply a runnable worker or close O-WASM.

## Outcome

Orishu admits and executes a bounded graph of untrusted WebAssembly Component
instances. It mediates typed bulk channels, follows the deterministic step
plan, isolates candidate state, and makes a simulation boundary visible only
after every required component invocation and validation succeeds.

The host understands lifecycle roles, channel contracts and ownership—not
model-specific equations. The same host supports bundled and third-party
components.

## Slices

Before freezing the ABI below, apply
[ADR 0027](../adr/0027-plugin-contributions-and-immutable-releases.md): each
independently compiled kernel implements one Orishu-owned execution contract.
Specify field-update and dynamics-integrator role interfaces with X-PLUGIN,
including how common lifecycle operations and multiple phases fit one contract.
Map existing component/role names to kernel/kernel-instance semantics without
silently changing WIT identifiers or persisted workload bytes. ABI/contract
conformance is not proof of a guest's scientific correctness.

1. Reconcile legacy `orishu:simulation/component@1` metadata with the concrete
   role-specific `field`/`dynamics` worlds in `orishu:simulation@1.0.0`, without
   reinterpreting existing workload bytes. Generate bindings and golden component fixtures for
   initialization, load/restore, phase execution, checkpoint, observation and
   drop.
2. Validate `orishu.workload-graph/v1` against the shared workload domain:
   artifact/lifecycle identities, closed imports, instance/node/edge/channel
   counts, phase exports, ownership, dimensions, complete dependencies,
   deterministic reductions, bounded schedules and aggregate limits.
3. Build the instance supervisor. Apply independent memory/table/stack/fuel/
   deadline/diagnostic limits, cancellation and structured attribution by
   workload, epoch, component instance, phase and partition.
4. Implement invocation-scoped input/output channel resources. Enforce
   direction, schema, coverage, offset and byte/value bounds before access;
   support chunked bulk transfer without exposing arbitrary host memory or
   retaining borrowed handles.
5. Execute ready step-plan nodes over committed inputs and isolated candidate
   outputs. Parallelize only dependency-independent nodes, apply stable
   reductions, validate coverage/finite values, and atomically publish or
   discard the complete boundary candidate.
6. Aggregate component checkpoint parts with runtime-owned coordination state;
   reject incomplete, duplicate, wrong-boundary, wrong-graph and incompatible
   restores. Assemble observations only from committed state outside the
   scientific commit path.
7. Exercise malicious guests: traps, hangs, forbidden imports, oversized host
   calls, invalid handles/ranges, partial outputs, duplicate finish, undeclared
   channels, stale invocation handles and checkpoint bombs. No case may commit
   partial state or damage the runtime owner.
8. Prove the accepted first-pass field/force → Dynamics graph (coupling force
   production belongs to the field kernel, not an extra kernel contract), including a
   failure in each phase, deterministic ordering under varied completion order,
   and equivalent results when instances are co-located. Distributed channel
   transport and placement evidence follow in N-CLUSTER.

## Acceptance criteria

- A valid multi-component fixture reaches `Ready` and advances only through the
  manifest's admitted step plan.
- Guests cannot call one another, inspect undeclared state, retain invocation
  handles, acquire filesystem/network/clock/random/process/thread authority, or
  select another guest by name.
- A trap, timeout, cancellation, invalid output or missing contribution in any
  required invocation leaves the previous committed boundary authoritative.
- Equal inputs and execution profile produce the same accepted result under
  different valid host scheduling orders.
- Checkpoint/restore reproduces continued execution and refuses any incomplete
  or cross-graph collection of component parts.
- Per-instance and aggregate resource limits are enforced before unbounded
  allocation or work. Diagnostics identify the responsible instance and phase
  without becoming scientific provenance.
- The runtime performs no per-particle or per-field-sample host-call loop; a
  representative fixture measures bulk-channel copies, allocations and phase
  overhead.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Implementing gravity, Maxwell/Yee, hydrodynamics or another numerical model.
- Distributed placement, cross-node component-channel transport, ownership
  transfer or work stealing; N-CLUSTER owns those adaptations.
- Choosing shared-memory/zero-copy as a requirement before measurement.
- Loading native libraries, Python modules, arbitrary WASI, or plugin UI code.
