# Spatiotemporal foundation of orishu

`orishu` is built around a specific class of computational problem: a shared simulation state that evolves **forward in simulation time** over a domain that is simultaneously structured in **space**.

This is not an implementation detail. It is the foundational property that makes `orishu`'s runtime model, storage model, and decentralized coordination model fit together.

This document expands one specific assumption summarized in [Foundations and assumptions of orishu](./foundations.md): the assumption that `orishu` workloads are spatiotemporal, partitionable, checkpointable, and advanced through ordered simulation boundaries. Read `foundations.md` first for the system-level assumptions and operator-facing constraints. This document exists to explain the architectural why behind one of those assumptions and to normalize the time/space/artifact vocabulary used elsewhere.

## Why this matters

Unlike a generic batch system or job scheduler, `orishu` does not treat computation as an unordered pile of tasks. The cluster cooperatively advances **one coherent world-state** through a sequence of committed simulation boundaries.

Two structural properties make that possible:

1. **Temporal ordering** — simulation state advances in one direction, from earlier simulation time to later simulation time.
2. **Spatial partitionability** — at a given simulation boundary, the global state can be divided into regions or partitions of the simulated domain and assigned across workers.

Together, these properties allow `orishu` to reason about work, correctness, storage, and recovery in a way that would not be valid for arbitrary distributed computation.

## Core design statement

`orishu` assumes workloads whose state evolution can be modeled as an ordered sequence of committed simulation states over a partitioned spatial domain.

At any committed simulation boundary:

- the cluster has a logically coherent view of the simulation state at a specific simulation time;
- that state is distributed across spatial partitions;
- each partition has one authoritative owner for a specific `(workloadEpoch, partitionVersion)`;
- advancing the simulation means deriving the next committed state from the current committed state plus the workload's declared execution rules.

Ownership transfer becomes valid only at fenced simulation boundaries.

This document intentionally uses **committed simulation state** rather than requiring a strict "next frame depends only on the previous frame" rule. Many workloads do behave like frame-to-frame evolution, but the runtime should not over-constrain valid numerical schemes that carry additional bounded solver state or integrator history. What matters to `orishu` is that the workload exposes deterministic, checkpointable progression across ordered simulation boundaries.

## Key terms used in this document

To keep the docs consistent, use the following language:

- **simulation boundary** — a committed point in simulation progression;
- **frame** — an operator-friendly logical view of simulation state at a boundary;
- **partition** — a spatial slice of the simulation domain;
- **checkpoint artifact** — resumable stored state at a simulation boundary;
- **result artifact** — retrievable stored output over a time range;
- **chunk** — the partition-aligned stored piece of a checkpoint or result artifact.

These terms are related but not interchangeable. A **simulation boundary** is the committed runtime point. A **frame** is a human-friendly view of state at such a boundary. A **checkpoint artifact** persists resumable state at one boundary. A **result artifact** persists output spanning one or more boundaries or a simulation-time range. A **chunk** is a stored artifact fragment, not the primary term for live mutable simulation ownership during step execution.

Avoid using "frame", "result", "state", and "chunk" interchangeably.

Not every committed simulation boundary is persisted, and not every persisted artifact is intended for resume.

## Time as an ordered state axis

The simulation progresses forward in **simulation time**, not wall-clock time.

This has several consequences:

- Simulation output is naturally ordered.
- Checkpoints refer to a specific simulation-time boundary.
- Resume restores a prior committed simulation boundary and continues from there.
- Deterministic validation can compare outputs produced for the same workload, epoch, partition, and step range.

For operator intuition, it is often useful to think of simulation output as a sequence of **"frames"** (as in a movie). A "frame" is a logical representation of the simulation state at a particular simulation boundary. Not every workload needs to persist every "frame", and not every internal solver substep needs to be exposed as a "frame". But the runtime does depend on the existence of ordered, checkpointable state boundaries.

## Space as a distribution axis

At any committed simulation boundary, the global state is not treated as one monolithic chunk. It is partitioned across the simulated domain.

This enables:

- assigning different spatial regions to different workers;
- transferring ownership of a region at safe boundaries;
- scaling the computation horizontally without inventing a central scheduler;
- storing resulting data in partition-aligned chunks rather than as one giant object.

The runtime therefore treats **partition ownership** as a first-class concern. A worker does not own "a task" in the scheduler sense; it owns a region of the simulation state for a workload epoch and partition version.

Because ownership is authoritative rather than advisory, fencing must prevent stale owners, delayed messages, and partitioned nodes from continuing to mutate authoritative state or publish authoritative artifacts after authority has moved.

## Why storage can also be spatiotemporal

Because the computation itself is spatiotemporal, storage can follow the same structure.

### Temporal dimension

Artifacts are anchored to simulation-time boundaries or time ranges:

- **Checkpoint artifacts** capture resumable state at a specific simulation-time boundary.
- **Result artifacts** capture retrievable output over a simulation-time range.

This gives artifacts a natural ordering and lineage.

Checkpoint and result artifacts serve different purposes. A checkpoint artifact is written for correctness and resume from a specific boundary. A result artifact is written for retrieval, analysis, or export over a time range. A workload may produce one, both, or neither at a given boundary depending on operator action and policy.

| Concept | Purpose | Time scope | Resumable? |
|---|---|---|---|
| Checkpoint artifact | restart and resume | one committed boundary | yes |
| Result artifact | retrieval, analysis, export | one or more boundaries / a time range | not necessarily |

### Spatial dimension

Artifacts are composed of partition-aligned chunks rather than requiring a single node to assemble and own the whole state.

This allows:

- workers to persist the partitions they computed;
- replica placement to operate chunk-by-chunk;
- retrieval to assemble a whole artifact from distributed chunk holders;
- a later cluster formation to rediscover imported artifacts only when durable artifact records and surviving compatible chunk sets are made visible again through local inventory and storage mechanisms.

In this sense, the storage layer is not a generic file store bolted onto the side of the runtime. It is an extension of the same spatiotemporal decomposition that drives execution.

Committed artifact record and availability view are not the same thing. An artifact record may survive while present-day retrieval is degraded or unavailable because some chunk holders are offline.

Rediscovering an old artifact does not mean the old cluster formation still exists. A later cluster is a new formation that may discover compatible durable artifacts; it does not inherit the previous formation's live control-plane state.

## Consequences for the runtime model

This foundational decision explains several other `orishu` design choices. These are not the whole foundations of the system; they are the specific consequences of the spatiotemporal workload assumption.

### 1. Single shared workload per cluster

The cluster is advancing one coherent spatiotemporal state machine, not multiplexing unrelated jobs.

### 2. No central scheduler

Work distribution emerges from partition ownership, safe transfer boundaries, and reconciliation, rather than from an external job queue.

### 3. Epoch-scoped ownership

Because time progression and partition ownership must remain coherent, ownership is versioned by workload epoch and partition version, with exactly one authoritative owner per `(workloadEpoch, partitionVersion)`.

### 4. Checkpoints as first-class recovery objects

A checkpoint is not merely a backup file. It is a committed simulation boundary from which the cluster can resume forward evolution.

### 5. Deterministic stepping

The runtime must preserve deterministic progression across ordered simulation boundaries. Packet timing, wall-clock drift, and arbitrary peer arrival order must not change the committed simulation outcome.

### 6. Distributed artifact retrieval

The cluster can present a unified view of a result or checkpoint even though the underlying chunks are held across multiple nodes, because those chunks correspond to stable spatial partitions within an ordered simulation artifact.

Current retrievability depends on available chunk holders and storage backend state, not solely on the existence of an artifact record.

## What this does not mean

This design decision should not be overstated.

It does **not** mean:

- every workload must store every simulation step as a persisted frame;
- every numerical scheme is restricted to a naive one-frame-in / one-frame-out formulation;
- the runtime understands simulation physics directly;
- storage layout is forced to mirror the exact in-memory solver layout.

Instead, it means the runtime is optimized for workloads that can expose:

- ordered simulation progression,
- explicit resumable boundaries,
- spatially partitionable state,
- deterministic advancement rules,
- and artifactization of state in partition-aligned form.

## Relationship to existing design docs

This document is intended to explain the architectural why behind several design elements already present elsewhere:

- `docs/foundations.md` — the primary source for system-level assumptions and constraints;
- [`docs/design.md#partition-ownership-and-rebalancing`](./design.md#partition-ownership-and-rebalancing) — authoritative ownership and fenced transfer semantics;
- [`docs/design.md#deterministic-execution-contract`](./design.md#deterministic-execution-contract) — deterministic stepping expectations;
- [`docs/storage-model.md`](./storage-model.md) — checkpoint/result storage rationale, and [`docs/storage-spec.md`](./storage-spec.md) — operational storage semantics and lifecycle;
- [`docs/design.md#local-inventory-and-availability-view`](./design.md#local-inventory-and-availability-view) — availability view and rediscovery mechanics;
- `docs/workload.md` — spatial domain, time stepping, initial conditions, checkpoints and results;
- `docs/protocol-client.md` and workload user stories — operator-facing lifecycle around start, stop, checkpoint, resume, and result retrieval.

Those documents should remain the source of truth for protocol shape and resource semantics. This document exists to make the spatiotemporal rationale explicit and to keep that rationale separate from the broader assumptions catalog in `foundations.md`.
