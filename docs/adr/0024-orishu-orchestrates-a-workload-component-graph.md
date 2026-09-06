# 0024 — Orishu orchestrates a workload component graph

Status: **accepted**  
Date: **2026-09-07**  
Refines: [ADR 0009](0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0010](0010-content-addressed-workload-closure-and-portable-bundles.md),
[ADR 0020](0020-compose-object-behaviour-through-plugin-components.md),
[ADR 0023](0023-fields-are-plugin-modelled-domain-state.md)

## Context

An authored experiment composes capabilities that may come from independently
packaged plugins. A representative experiment can select a Maxwell/Yee field
model, attach electric-charge coupling and Dynamics to particles, and add a
particle emitter. Each selected computational model may contribute different
digest-addressed WebAssembly Component code.

The pre-Field-CAD workload contract assumed one root guest exporting one
`wl_step` operation. That leaves a critical question unanswered: who assembles
and invokes independent components, orders their work, moves state between
them, attributes resource use and failures, checkpoints them, and ensures that
only a complete candidate boundary commits?

The answer also affects distribution. Field-model work, object coupling and
integration need not have identical partitioning or placement. Treating their
code as one opaque executable would unnecessarily prevent Orishu from placing
component partitions independently when the workload contract permits it.

In this record, **workload component** means a sandboxed executable artifact or
instance. A **numerical kernel** remains an implementation detail inside one of
those components; the runtime does not load an uncontracted native kernel.

## Options considered

### Link one executable root during workload compilation

Kagami or another compilation stage could statically link all selected plugin
components into one root guest implementing the existing lifecycle.

This keeps the runtime interface small and may permit efficient internal data
exchange. It was rejected as the universal composition model because it makes
Kagami an executable build/link service, produces a new opaque assembly for
every combination, weakens per-plugin isolation and accounting, and removes
Orishu's ability to place independently partitionable models separately.

A plugin may still internally package tightly coupled numerical kernels into
one component. That is implementation encapsulation, not cross-plugin
composition.

### Generate a thin orchestration component

Kagami could generate a root component whose imports are the selected plugin
components and whose only job is to invoke them in order.

This preserves independently addressed plugin artifacts and a single root
entry point, but still makes authoring produce executable wiring. Arbitrary
component counts make static imports awkward; metering, failure attribution and
placement remain hidden behind the generated guest; and the orchestration
artifact becomes a scientifically significant compatibility layer. This option
was therefore rejected as the product-wide boundary.

### Let components invoke one another directly

Plugins could discover peers and call their exports or share guest memory.

This was rejected because it creates ambient cross-plugin authority, couples
independent schemas and toolchains, obscures deterministic ordering and bounds,
and makes isolation, checkpointing, placement and atomic failure handling
unverifiable.

### Orishu hosts and orchestrates component instances

The immutable workload can declare a bounded component-instance graph and a
deterministic execution plan. Orishu can instantiate, meter and invoke each
guest, mediate typed state exchange, coordinate partition placement, and commit
only after every required invocation succeeds.

This adds a richer host contract and requires careful bulk-data interfaces, but
keeps executable composition aligned with Orishu's existing authority over
time, partitioning, resource limits, checkpointing and commit.

## Decision

Adopt host-orchestrated component instances.

The workload has one immutable root **manifest**, not necessarily one root
executable. Its identity-bearing compute definition contains:

- bounded component instances, each naming a digest-addressed WebAssembly
  Component, plugin/model/schema identities, configuration, lifecycle/profile,
  declared state ownership, capabilities and resource limits;
- versioned typed state and contribution channels, with dimensions, ownership,
  access direction and reduction semantics;
- a bounded deterministic step plan whose invocation nodes name a component
  instance, phase export, input/output channels and dependencies; and
- placement and partition-compatibility constraints that describe what is
  scientifically legal without prescribing the cluster's current placement.

The plan is an admitted directed acyclic graph, or another explicitly versioned
bounded schedule for methods requiring substeps. Plugin installation order,
manifest map order, worker timing and guest completion order never decide the
scientific reduction order. Orishu validates the complete graph before a run
can become ready: every required channel is supplied, every authoritative state
has an allowed writer, model-family exclusivity holds, dimensions and schemas
match, the schedule is complete and bounded, and all component artifacts and
lifecycles are compatible.

Component state ownership is scientific even when the runtime owns storage and
transfer mechanics:

- a field-model component owns the state of its selected field over its domain;
- a coupling/projection phase samples or projects field state onto compatible
  entity properties and emits typed contributions such as forces;
- Dynamics alone combines the admitted force/impulse contributions and writes
  candidate particle velocity and position; and
- emitter components propose bounded spawns while Orishu owns globally safe
  identities, placement and atomic admission of the resulting run-state
  objects.

These are initial roles, not a closed list of physical phenomena. Plugins may
declare other typed state transformations through versioned interfaces, but
cannot obtain another component's state through ambient memory or direct guest
calls.

For each boundary, Orishu supplies read-only committed inputs and isolated
candidate outputs, executes ready plan nodes in deterministic-equivalent order,
performs correctness-bearing transfers where producers and consumers are on
different workers, validates the assembled candidate, and atomically commits
it. Independent plan nodes may run concurrently or on different eligible nodes
when their declared dependencies and placement constraints allow it. Placement
may change without changing workload identity; changing the admitted component
graph, plan, or scientific constraints creates a different workload.

A trap, timeout, invalid output, missing contribution, transfer failure or
limit violation rejects the attempted boundary. No component can publish a
partial scientific commit. Checkpoints aggregate runtime coordination state and
versioned state from every required component instance/partition under one
workload, epoch and boundary identity. Restore rejects missing, duplicate or
incompatible component state.

Cross-component exchange is host-mediated and bulk-oriented. The ABI uses
bounded typed buffers or capability-scoped resource handles; it does not call
the host once per particle or field sample, expose arbitrary host memory, or
permit a guest to retain borrowed access beyond an invocation. Zero-copy or
shared-memory optimizations are admissible only when measurement justifies them
and the same ownership, bounds and isolation remain enforceable.

## Consequences

- `S-WORKLOAD` represents a component graph and deterministic step plan rather
  than a singular root component descriptor.
- `O-WASM` is a multi-component host and phase coordinator, while remaining
  ignorant of model-specific equations.
- The component ABI is role/phase based: initialize, load/restore owned
  partition state, execute an admitted phase over typed channels, checkpoint,
  query observations, and drop state.
- Correctness-bearing component transfers use reliable identified flow and are
  never confused with lossy observer projections.
- Provenance identifies the complete component graph and attributes attempted
  work, failures and produced state to component instance, partition and phase.
- Orishu gains freedom to co-locate or distribute component partitions based on
  capability and cost, subject to admitted scientific dependencies.
- The first implementation may co-locate all instances on one worker and use
  copied bulk buffers. Distribution and zero-copy are optimizations of the same
  contract, not alternate scientific semantics.

## Non-goals

- Defining a universal numerical phase order independent of the selected
  integration method. The admitted step plan records the required schedule.
- Allowing observers, presentation subscriptions or diagnostics into the
  scientific dependency graph.
- Requiring each numerical kernel or source file to become a separate guest.
- Choosing a final buffer representation, Wasm shared-memory feature, placement
  heuristic or distributed load-balancing algorithm in this ADR.

## Implementation

Tracked by [Define composed object execution](../tasks/kagami/define-composed-object-execution.md),
[Define and adopt the shared workload format](../tasks/define-and-adopt-shared-workload-format.md),
and the O-WASM/O-RUNTIME/N-CLUSTER roadmap packages.
