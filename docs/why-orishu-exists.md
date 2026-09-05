# Why Orishu exists

Orishu is a distributed simulator: *many nodes co-dreaming a single coherent
world*. Kagami is the authoring and visualization client that makes that world
accessible to researchers.

Together they explore a particular question: how can independent machines
cooperate on the continuous evolution of one scientific state, retain its
large results, and remain understandable to researchers and operators without
a permanent scheduler or centralized management plane?

This page explains the motivation and intended direction. The
[architecture](architecture.md) owns component and authority boundaries, the
[runtime design](orishu-runtime-design.md) owns execution semantics, and the
[roadmap](roadmap/README.md) distinguishes implemented behavior from planned
work.

## The problem

A scientific simulation may exceed one machine in several different ways:

- advancing the state takes too much compute time;
- the live state does not fit in one machine's memory;
- checkpoints and results exceed one machine's storage or serving capacity;
- retaining and retrieving those artifacts needs resilience after a worker is
  lost; or
- several researchers need to observe the same live or recorded run without
  becoming cluster members.

Orishu treats these as one runtime problem. A cluster cooperatively advances
one partitioned workload, commits coherent simulation boundaries, and manages
the resulting artifacts. A single worker is a cluster of one and uses the same
workload and observation model as a multi-worker formation.

Kagami supplies the complementary authoring workflow. Researchers construct an
editable experiment, compile it into an immutable workload, submit it to a
configured Orishu cluster, and inspect live or persisted observations. Editable
intent, authoritative execution, and presentation remain separate even though
they form one product.

## Why not simply use MPI or Ray?

MPI and Ray are capable systems, and Orishu is not intended to replace them for
their normal workloads. They start from different abstractions:

- MPI is an excellent low-level communication model for tightly coordinated
  parallel programs, commonly deployed as explicitly launched jobs on
  provisioned HPC resources.
- Ray is a general distributed application framework centered on tasks,
  actors, and scheduling independent units of work.
- Orishu centers one long-lived, spatially partitioned scientific state. Its
  workers form a peer cluster, negotiate explicit ownership, advance fixed
  simulation steps, commit coherent boundaries, and manage checkpoints and
  results as first-class artifacts.

That focus permits a single operational and scientific model from one worker
to many, ownership transfer after membership change at safe boundaries,
resumable observations, and content-addressed workload distribution. It also
deliberately gives up the generality of a batch scheduler or arbitrary
distributed application runtime.

The implementation uses contemporary mechanisms where they support those
semantics: Rust, authenticated QUIC-based protocols, bounded streaming, and
sandboxed WebAssembly Components for client-supplied physics. A particular
kernel interface or operating-system optimization is an implementation choice,
not the reason the product exists or a standing performance claim.

## Who it is for

The product has three operational user roles and one research audience:

- **Researchers** author physical experiments in Kagami, submit workloads, and
  inspect scientific results without having to operate MPI jobs or understand
  Orishu's peer protocol.
- **Cluster operators** provision workers, establish trust, form clusters, and
  inspect their health with `orishuctl` and `orishu-monitor`.
- **Simulation plugin developers** use ordinary IDEs and build tools to add
  physical models and numerical methods through declarative Kagami schemas and
  sandboxed workload components.
- **Distributed-systems researchers** can use the runtime as a substrate for
  investigating decentralized membership, partition ownership, recovery,
  storage, and scientific coordination under changing topology.

One person may perform several roles, but their interfaces and authority remain
distinct. In particular, Kagami researchers are clients of the cluster; they do
not become Orishu peers merely by submitting or observing a run.

## Design objectives

### Evolve state rather than schedule tasks

The cluster continuously constructs one consistent computational reality. Its
unit of correctness is an accepted simulation boundary for a particular
workload, epoch, partition ownership version, and numerical profile—not whether
an independent task happened to finish.

Orishu therefore runs at most one workload per cluster. Rebalancing or recovery
may move partition ownership, but it does not turn the runtime into an
independent-task scheduler.

### Scale capacity as well as computation

Reduced wall-clock time matters, but it is not the only motivation for a
cluster. Orishu must also let scientific state and durable artifacts exceed one
worker's memory, storage, and serving capacity. Checkpoints, results, verified
chunks, provenance, replication, and retrieval are core runtime concerns rather
than afterthoughts around the solver.

### Preserve scientific meaning

Simulation time advances only through accepted fixed steps. Network timing,
duplicate delivery, retries, and membership churn must not change committed
scientific state outside the workload's declared numerical tolerance.

This is not a universal promise of bitwise equality across arbitrary hardware,
topology, precision, or algorithms. Each execution identifies its workload,
model, numerical profile, precision, and provenance, and comparisons use the
declared tolerance appropriate to that profile.

### Use one model from a laptop to a cluster

A standalone worker forms a cluster of one. Moving to several workers should
change capacity and partitioning, not introduce a second workload definition,
a separate simulation lifecycle, or a mandatory central scheduler.

Dynamic membership does not mean unconstrained mid-step mutation. Admission,
loss, ownership transfer, and rebalancing are explicit transitions, and
correctness-bearing ownership changes occur only at safe committed boundaries.

### Treat extension code and networks as hostile

Orishu does not assume a trusted HPC enclave. Clients, peers, manifests,
artifacts, and workload components are authenticated where required and still
treated as untrusted input. Parsers, messages, queues, allocations, transfers,
and guest execution have explicit limits before state is adopted.

Client-supplied physics executes as a capability-limited WebAssembly Component,
not as a native library with ambient access to the worker. The runtime retains
authority over networking, partitioning, time, barriers, storage, and commit.

### Keep the platform extensible without making Kagami an IDE

Kagami is planned to ship with gravity and electrodynamics plugins as useful
starting points. Advanced users can develop other models with normal software
engineering tools and install the resulting packages. Built-in and third-party
plugins use the same public schema, workload, validation, and sandbox contracts.

## How success is measured

The long-term research target is useful operation at 10,000 or more workers
with near-linear scaling where a workload's decomposition and communication
pattern permit it. This is a target, not a claim about the current
implementation.

Evidence is staged at smaller cluster sizes first and includes more than speed:

- comparison with a competent single-worker baseline;
- scientific result equivalence under the declared tolerance;
- state and artifact capacity beyond one worker;
- bounded memory, network, membership, and diagnostic work;
- continued or explicitly failed progress under loss and churn;
- safe ownership transfer and recovery at committed boundaries; and
- restoration and verification of artifact durability targets.

Near-linear scaling is conditional, not universal. A result does not count as
a success if it changes scientific semantics, excludes coordination or storage
cost without disclosure, or compares against an intentionally weak baseline.
The detailed gates and claim discipline live in
[Orishu runtime scaling objectives](orishu-scaling-objectives.md).

## Explicit non-goals

- **Not a general scheduler.** Orishu does not run many unrelated jobs or offer
  Kubernetes-, Slurm-, or Ray-style task placement. One cluster advances one
  workload.
- **Not a comprehensive simulation suite.** Kagami provides an authoring
  environment and starter physical models, while the platform and community
  supply extensibility. It does not promise an out-of-the-box implementation of
  every scientific method.
- **Not performance at any cost.** Correctness, bounded behavior, provenance,
  operator clarity, and recoverability take precedence over an optimization
  that changes or obscures scientific meaning.
- **Not only a compute accelerator.** A cluster is also valuable for memory,
  durable artifact capacity, replication, and serving observations.
- **Not a trusted-network design.** Authentication is necessary but does not
  make input safe; zero-trust validation applies across client, peer, artifact,
  and extension boundaries.
- **Not live collaborative authoring in the initial product.** Researchers
  initially share experiment files and independently observe submitted runs.
  The document-authority boundary preserves a path to a later headless
  collaboration service without putting draft editing into Orishu.

## Continue reading

- [Architecture](architecture.md) — how Kagami, Orishu, plugins, and artifacts
  divide authority.
- [Orishu runtime design](orishu-runtime-design.md) — worker formation,
  deterministic execution, ownership, recovery, and state.
- [What is an Orishu workload?](workloads.md) — the immutable unit submitted to
  a cluster.
- [Simulation plugins](simulation-plugins.md) — how new physical models extend
  the product safely.
- [Scaling objectives](orishu-scaling-objectives.md) — staged evidence and the
  long-term research target.
- [Implementation roadmap](roadmap/README.md) — what exists now and what must be
  demonstrated next.
