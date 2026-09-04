# Project context

Orishu Kagami provides one product for authoring, executing, and observing
scientific simulations. Orishu and Kagami are roles within that product, not
independent platforms.

## Product language

| Term | Meaning |
| --- | --- |
| **Orishu** | The distributed execution engine, cluster runtime, shared models, and authenticated client protocol. |
| **Kagami** | The native Orishu client for authoring experiments and inspecting live or recorded observations. |
| **Cluster operator** | A person or agent that provisions workers, forms and secures clusters, and operates them through `orishuctl` or `orishu-monitor`. |
| **Researcher** | A person or agent that uses Kagami to author experiments, submit workloads, and inspect runs without operating cluster membership. |
| **Simulation plugin developer** | An advanced user who implements and tests physical models or numerical methods with external development tools and packages them for Kagami and Orishu. |
| **Cluster** | A running set of nodes cooperating to execute one workload and manage its artifacts. |
| **Cluster formation** | One ephemeral instantiation of a cluster, identified independently of its reusable human-readable name. |
| **Node** | One running `orishu-worker` process participating in a cluster. |
| **Experiment** | Editable intent: a world, physical models, parameters, initial conditions, run controls, and requested observations. |
| **Document authority** | The owner that serializes proposed authoring commands and atomically accepts or rejects experiment revisions. It initially runs inside each Kagami process and may later be hosted by a headless collaboration service. |
| **Variable** | A named, dimensioned value in expression-capable experiment or workload intent, defined by an authored expression and available through an explicit namespace or scope. |
| **Expression** | Retained authored source for a schema-declared numeric value; it may contain literals, units, arithmetic, variables, supported constants, and references to other expression-capable fields. |
| **Object catalog** | Kagami's client-owned, editable collection of versioned object-template files. It is authoring vocabulary, not experiment or workload state. |
| **Object template** | A named, reusable composition of components, parameters, and authored properties that can be instantiated as a self-contained experiment object. |
| **Simulation plugin** | An installable, versioned capability containing declarative Kagami authoring schemas and a pinned sandboxed workload component for one or more physical models. It is not a native host plugin. |
| **Numerical kernel** | A reusable computational algorithm or library used inside a workload component; it is implementation, not the installed product package or workload identity. |
| **World** | The objects and physical properties authored as part of an experiment. |
| **Workload** | The immutable logical bundle required to execute one simulation: a root manifest and the complete closure of digest-addressed compute code, initial conditions, and other required inputs. It is independent of distribution format. A cluster runs at most one workload at a time. |
| **Workload manifest** | The immutable root definition of a workload, including its compute definition, execution requirements, and digest-pinned references to required artifacts. |
| **Workload component** | A content-addressed, machine-independent WebAssembly Component containing untrusted executable physics behind Orishu's versioned, capability-limited workload lifecycle. Historically called the workload package; it is not a distribution bundle. |
| **Workload closure** | The root manifest plus every content-addressed component, initial condition, geometry, and other artifact reachable from it and required to execute the workload. |
| **Workload bundle** | A portable import/export representation carrying a workload manifest and its complete artifact closure; it is transport, not workload identity. |
| **Distribution format** | A physical file layout, archive encoding, or transfer protocol that carries a workload manifest and artifact bytes without changing their logical identities. Multiple formats may carry the same workload. |
| **Run** | One execution of a simulation problem: state advanced through accepted fixed steps, published as an ordered stream of computed states. A run is a local Kagami preview or a workload executing on a cluster. |
| **Run reference** | Shareable identification of one cluster run by cluster formation, workload identity, and workload epoch, plus optional non-authoritative connection hints. It contains no credentials or presentation state. |
| **Simulation boundary** | A committed simulation time at which workload state is consistent and safe to observe, checkpoint, or transfer. |
| **Observation** | Versioned output carrying workload, simulation-time, model, numerical, and execution provenance. |
| **Observation baseline** | A complete observation that a consumer has validated and adopted and against which a later observation delta is explicitly encoded. |
| **Result artifact** | Durable workload output intended for analysis, export, or visualization. It is not resumable. |
| **Checkpoint artifact** | Durable state captured at a simulation boundary and suitable for resuming a workload. |
| **Artifact record** | Immutable authority for a checkpoint or result artifact's identity, chunks, integrity, completeness at creation, and committed provenance; it contains no live location. |
| **Result sequence** | A derived operator/client grouping of immutable result artifacts from one run lineage across stop/resume; it is not a stored resource. |
| **Local inventory** | A node-local, potentially stale claim about artifact chunks a worker can serve; it is advisory until bytes verify. |
| **Availability view** | A transient projection of current artifact retrievability derived from records, membership, inventory, verified reads, and backend state. |
| **Membership tombstone** | A formation-scoped barrier recording removal of a node and preventing readmission until explicitly cleared. |
| **Purge tombstone** | Persisted artifact-deletion intent that prevents stale inventory from resurrecting a deleted artifact; its cross-formation retention remains an open storage contract. |
| **Durability target** | The accepted number and placement policy for verified artifact copies; current retrievability does not prove it is satisfied. |
| **Subscription** | The observations, channels, spatial region, and level of detail Kagami currently requests. It never changes simulated values. |
| **Presentation state** | Camera, selection, window layout, visibility, and display density owned locally by Kagami. |

## Ownership

- Kagami owns editable experiment intent and presentation state.
- Every authored change, whether initiated by the UI, an MCP client, undo, or
  redo, is a proposed command decided by Kagami's document authority. No input
  adapter mutates the experiment directly.
- Kagami's document authority owns variable definitions, expressions, their
  dependency graph, and their resolved values as part of experiment intent.
- Kagami's separate catalog authority owns object-template files, validation,
  and catalog revision. UI and MCP adapters command that same authority.
- Kagami owns the installed simulation-plugin inventory and exposes its
  declarative model vocabulary to authoring. Plugin installation never grants
  executable code ambient authority inside Kagami.
- Submission compiles supported experiment intent into an immutable workload
  manifest, workload component reference, and input artifacts.
- Orishu owns the accepted workload, workload epoch, simulation boundaries,
  partition ownership, checkpoints, result artifacts, and execution provenance.
- Orishu authoritatively validates and freezes the variable graph and resolved
  values of an accepted workload; Kagami's evaluation is authoritative only
  for its editable experiment revision.
- Kagami uses Orishu's client protocol. It never joins cluster membership or
  participates in peer coordination.
- Local preview and remote execution must publish the same observation
  semantics; neither renderer nor UI may read solver-owned memory.
- Draft sharing is initially file-based. A future headless collaboration
  service must host the same document authority and command semantics rather
  than introduce a second experiment model.
- The Orishu client interface presents one authoritative service, but its run
  authority is the state collectively committed for a workload identity,
  workload epoch, and simulation boundary—not the incidental node serving a
  client connection.

## Invariants

- Simulation time advances only through accepted fixed steps.
- A rendered value identifies the workload, simulation boundary, model,
  precision, domain, and validity that produced it.
- Editable scene intent never mutates an already loaded workload.
- Run observations never mutate editable experiment intent. Adopting computed
  state into an experiment is an explicit, validated authoring command that
  creates a new revision and retains source provenance.
- Expression-capable values retain their authored source and are adopted only
  when their complete affected dependency closure resolves to finite values of
  the dimensions required by the experiment schema.
- Instantiating a catalog template persists complete authored object state and
  source provenance in the experiment. Catalog changes never silently mutate
  existing objects, and neither opening nor submitting the experiment depends
  on the source catalog being present.
- Accepted workload expressions are immutable for their workload epoch and are
  never reevaluated as runtime control state. Changing one requires a new
  workload resource and normal replacement.
- Workload components execute as untrusted guests. They have no ambient host
  authority, interact only through their declared lifecycle imports/exports,
  and cannot commit partial output after a trap, timeout, or limit violation.
- Workload identity contains no artifact location. Changing a peer, mirror,
  cache, repository, archive encoding, or other distribution detail never
  creates a different workload when the root and artifact digests are unchanged.
- Presentation state never changes physical execution.
- Multiple clients observing one run own independent subscriptions, cameras,
  selections, playback cursors, and playback rates. Sharing a document or run
  reference does not implicitly synchronize presentation state.
- Observation projections may be coalesced or skipped under backpressure, but
  every delta names a compatible complete baseline and an unusable baseline
  recovers through a full snapshot. Halo data, step decisions, commands,
  checkpoints, and workload artifacts are never placed on that lossy path.
- Interpolated or extrapolated observations are presentation-only and cannot
  become results, checkpoints, simulation inputs, or authored state without an
  explicit authority transition.
- Network input is untrusted, bounded, and validated before allocation or state
  adoption.
- The runtime is decentralized and runs one workload per cluster; it is not a
  general scheduler or job queue.
- Cluster and node names are labels, not identity. Formation and membership
  identities follow ADR 0013.
- Artifact records, local inventory, availability views, and purge tombstones
  retain their separate authority levels as defined by the storage spec.
- Shared numerical kernels do not depend on Kagami UI code or Orishu peer
  runtime code.
- Built-in and third-party simulation plugins use the same public authoring,
  validation, workload-compilation, and sandbox contracts.
- Selecting a simulation plugin pins its model/schema identities and executable
  artifacts into the compiled workload closure. A plugin name, installation
  path, source URL, or mutable version tag never selects executable physics at
  runtime.
- Removing or updating an installed plugin never mutates an experiment,
  accepted workload, or run. Missing capabilities are explicit and are never
  replaced with a plausible default model.
