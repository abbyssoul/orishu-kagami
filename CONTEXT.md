# Project context

Orishu Kagami provides one product for authoring, executing, and observing
scientific simulations. Orishu and Kagami are roles within that product, not
independent platforms.

## Product language

| Term | Meaning |
| --- | --- |
| **Orishu** | The distributed execution engine, cluster runtime, shared models, and authenticated client protocol. |
| **Kagami** | The native Orishu client for authoring experiments and inspecting live or recorded observations. |
| **Cluster** | A running set of nodes cooperating to execute one workload and manage its artifacts. |
| **Node** | One running `orishu-worker` process participating in a cluster. |
| **Experiment** | Editable intent: a world, physical models, parameters, initial conditions, run controls, and requested observations. |
| **World** | The objects and physical properties authored as part of an experiment. |
| **Workload** | The immutable simulation problem submitted to an Orishu cluster. A cluster runs at most one workload at a time. |
| **Workload package** | Content-addressed executable physics implementing Orishu's workload lifecycle. |
| **Simulation boundary** | A committed simulation time at which workload state is consistent and safe to observe, checkpoint, or transfer. |
| **Observation** | Versioned output carrying workload, simulation-time, model, numerical, and execution provenance. |
| **Result artifact** | Durable workload output intended for analysis, export, or visualization. It is not resumable. |
| **Checkpoint artifact** | Durable state captured at a simulation boundary and suitable for resuming a workload. |
| **Subscription** | The observations, channels, spatial region, and level of detail Kagami currently requests. It never changes simulated values. |
| **Presentation state** | Camera, selection, window layout, visibility, and display density owned locally by Kagami. |

## Ownership

- Kagami owns editable experiment intent and presentation state.
- Submission compiles supported experiment intent into an immutable workload
  manifest, workload package reference, and input artifacts.
- Orishu owns the accepted workload, workload epoch, simulation boundaries,
  partition ownership, checkpoints, result artifacts, and execution provenance.
- Kagami uses Orishu's client protocol. It never joins cluster membership or
  participates in peer coordination.
- Local preview and remote execution must publish the same observation
  semantics; neither renderer nor UI may read solver-owned memory.

## Invariants

- Simulation time advances only through accepted fixed steps.
- A rendered value identifies the workload, simulation boundary, model,
  precision, domain, and validity that produced it.
- Editable scene intent never mutates an already loaded workload.
- Presentation state never changes physical execution.
- Network input is untrusted, bounded, and validated before allocation or state
  adoption.
- The runtime is decentralized and runs one workload per cluster; it is not a
  general scheduler or job queue.
- Shared numerical kernels do not depend on Kagami UI code or Orishu peer
  runtime code.
