# Implement the WebAssembly component-graph host

Status: **specified**; gated on the S-WORKLOAD graph and component ABI  
Work package: **O-WASM** ([roadmap](../roadmap/README.md))  
Decisions: [ADR 0009](../adr/0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md)  
Protocol: [Workload contract](../protocol-workload.md)

## Outcome

Orishu admits and executes a bounded graph of untrusted WebAssembly Component
instances. It mediates typed bulk channels, follows the deterministic step
plan, isolates candidate state, and makes a simulation boundary visible only
after every required component invocation and validation succeeds.

The host understands lifecycle roles, channel contracts and ownership—not
model-specific equations. The same host supports bundled and third-party
components.

## Slices

1. Freeze `orishu:simulation/component@1` WIT from the domain interface in the
   workload protocol. Generate bindings and golden component fixtures for
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
8. Prove a minimal field → coupling/projection → Dynamics graph, including a
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
