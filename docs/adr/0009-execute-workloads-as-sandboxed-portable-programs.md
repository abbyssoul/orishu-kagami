# ADR 0009: Execute workloads as sandboxed portable programs

Status: **accepted**
Date: **2026-09-04**

Refined by: [ADR 0024](0024-orishu-orchestrates-a-workload-component-graph.md)

## Context

An Orishu workload is not merely configuration or input data. Its closure
contains client-supplied executable physics whose assigned component instances
workers load to advance their partitions. The same component artifact must
behave consistently across machines while being unable to corrupt membership,
networking, storage, or host process.

id Tech arrived at the same boundary for distributable game logic. Quake III
compiled game, client-game, and UI modules into portable QVM bytecode and
connected them to the engine through a narrow call/system-call interface. Doom
3 again ran game scripts through an engine-owned interpreter. The useful
principle is not either historical instruction set: extension code is a guest
program, the engine owns its execution and scheduling, and all interaction
crosses an explicit host interface. See Fabien Sanglard's
[Quake III QVM](https://fabiensanglard.net/quake3/qvm.php) and
[Doom 3 scripting VM](https://fabiensanglard.net/doom3/scripting_vm.php)
reviews and id Software's
[Quake III VM implementation](https://github.com/id-Software/Quake-III-Arena/blob/master/code/qcommon/vm.c).

WebAssembly provides the modern portable code and isolated-memory substrate.
The Component Model adds typed, self-describing imports and exports: a
component can interact with its environment only through interfaces supplied
by the host. This maps directly to Orishu's need to expose computation inputs
and bounded diagnostics while withholding ambient filesystem, network, clock,
randomness, process, and threading authority. See the
[WebAssembly security model](https://webassembly.org/docs/security/) and
[Component Model worlds](https://component-model.bytecodealliance.org/design/worlds.html).

Content hashes and signatures establish artifact identity and publisher trust;
they do not make executable code safe. Conversely, memory isolation alone does
not provide deterministic results, bounded execution, or a stable lifecycle.
Orishu needs both a sandbox and a versioned domain ABI.

This record uses the historical term **workload package** for the executable
WebAssembly artifact. User-facing documentation calls it the **workload
component** to distinguish it from a portable workload bundle, which is a
distribution format containing the whole workload closure.

The product may call an installable model capability a **simulation plugin**.
That plugin includes declarative Kagami authoring schemas and a pinned workload
component; it is not a trusted native plugin. This decision applies equally to
components delivered by built-in and third-party simulation plugins. See
[Simulation plugins](../simulation-plugins.md).

## Decision

### Workload packages are untrusted guest programs

Every workload package is treated as hostile executable input, regardless of
its author, signature, source language, or whether Kagami produced it. Orishu
validates the manifest, digest, signature policy, engine, lifecycle identifier,
imports, and declared limits before instantiation. A load failure cannot execute
package initialization or partially change the accepted workload.

The first and only admitted execution format is content-addressed WebAssembly
Components. ADR 0024 refines the original singular lifecycle into graph profile
`orishu.workload-graph/v1`, engine `wasm-component`, component lifecycle
`orishu.component/v1`, and WIT world `orishu:simulation/component@1`. All
participating workers validate the same immutable graph, pinned component bytes
and compatible lifecycles. Architecture-specific JIT/AOT caches are derived
local artifacts and never workload identity.

### The component world is the capability boundary

The workload exports only the versioned lifecycle operations in
[`protocol-workload.md`](../protocol-workload.md). Orishu supplies only the
imports named by that world. The initial world has no general WASI filesystem,
socket, HTTP, environment, wall-clock, host-random, process, or thread
capabilities. A package requiring any undeclared or disallowed import is
rejected before instantiation.

Host calls validate pointer/range or component values, bound allocations and
payload sizes, and rate-limit diagnostic output. Returned state, checkpoints,
metrics, and errors are untrusted until schema, size, finiteness, and semantic
validation succeeds.

### The runtime owns infrastructure and time

Guest components supply their declared transformations for component
partitions and simulation boundaries. Orishu owns the outer loop, component
placement, partition assignment, host-mediated channels, halo exchange,
networking, barriers, committed time, cancellation, checkpoint/result storage,
and provenance. The guest neither joins the cluster nor drives its own event
loop. All durable effects leave the sandbox as validated return values or
explicit host calls.

Deterministic inputs include the frozen workload parameters, current state,
halo data, partition context, simulation time, and a runtime-derived seed.
Packet arrival, worker wall time, host randomness, and undeclared machine state
are not inputs. The runtime enforces memory/table limits, bounded host calls,
and interruptible CPU/fuel or epoch budgets independently of guest cooperation.
A trap, timeout, limit violation, or cancellation fails the attempted step; it
cannot publish partial state as a committed boundary.

### Sandboxing is not an optional backend detail

Native dynamic libraries and in-process language runtimes are not workload
package formats. Merely reproducing function signatures with a native ABI does
not reproduce the isolation, portability, deterministic capability surface, or
machine-independent artifact identity of the contract.

Adding another engine requires a new ADR and threat model proving equivalent
or stronger isolation, resource enforcement, cancellation, portability,
determinism, and lifecycle semantics. It must not weaken existing cluster
policy. Defense-in-depth process isolation for the WebAssembly runtime remains
permitted without changing the guest contract.

Kagami local preview should execute the same pinned graph and lifecycles as
Orishu where the supported profile allows it. Shared native numerical crates
may be source used to build both hosts and components, but a native library is
not the artifact submitted to a cluster and local preview cannot silently grant
capabilities that remote execution denies.

## Consequences

- Workload authors can distribute machine-independent simulation logic without
  being trusted with a worker's ambient authority.
- The lifecycle ABI and allowed imports become durable product interfaces that
  require explicit versioning and compatibility tests.
- Workers need component validation, capability filtering, deterministic host
  functions, resource metering, interruption, output validation, and hostile
  guest tests before they may advertise `wasm-component` support.
- Content digests identify the portable component, while compiled-code caches
  remain disposable and excluded from provenance identity.
- Orishu can keep membership, networking, storage, and scheduling in one binary
  while maintaining a deliberate sandbox boundary around client code.
- Native plugin convenience and unrestricted WASI are rejected because they
  would turn a submitted workload into arbitrary code execution on every node.
- Bundled gravity/electrodynamics and user-installed simulation plugins use the
  same component validation, lifecycle, capability, and resource limits.
