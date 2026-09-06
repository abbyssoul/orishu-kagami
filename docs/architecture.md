# Architecture

Orishu Kagami separates editable intent, distributed execution, and
presentation without treating them as separate products.

[Why Orishu exists](why-orishu-exists.md) explains the problem and product
objectives behind these boundaries; this document defines how the parts divide
authority to meet them.

```text
Kagami authoring and presentation
        |
        | compile experiment / send client commands
        v
Orishu client interface and workload contract
        |
        | authenticated control + subscribed observations
        v
Orishu cluster (one workload, many nodes)
        |
        | committed simulation boundaries
        v
result and checkpoint artifacts
```

## Modules

### Deployable applications

- `apps/orishu-worker` runs a node and owns cluster/runtime behaviour.
- `apps/orishu-ctl` provides scriptable operator administration.
- `apps/orishu-monitor` is the more coprehensive TUI interactive terminal operator client.
- `apps/kagami` is the native experiment-authoring and visualization client.

Applications compose library modules; other applications do not depend on an
application crate.

### Shared libraries

- `crates/orishu` owns shared cluster/workload models and the Orishu client
  interface. It is the seam used by every operator or visualization client.
- `crates/kagami-renderer` hides Kagami's GPU pipeline behind Iced's shader-program
  interface. It renders presentation state and must not own simulation state.
- `crates/kagami-document` owns the authoritative experiment model: objects
  composed from plugin-contributed components, the closed set of authoring
  commands, the validated transition, and edit history. It is sans-IO and has
  no UI, transport, or runtime dependency. What the *installed* schemas can
  govern is a separate projection over that model, not part of it, so an
  experiment holding a component whose plugin is absent stays readable and
  editable elsewhere.
- `crates/kagami-session` owns the document server: the current revision,
  guarded and idempotent command envelopes, undo/redo, change events, which
  revision is persisted and where, and the read projections adapters consume.
  It is the sole mechanism by which an experiment is created or modified; UI
  and MCP are adapters over it, converting through the explicit versioned
  representation each crate publishes rather than through its internal types.

The prototype scene tree still lives in `apps/kagami` because it is demo UI
state, not the authoritative experiment model. It is replaced by
`crates/kagami-document` and the document server as the
[Kagami capability programme](./tasks/kagami/README.md) lands; until then, do not
treat it as domain state.

## Client-server from outside, peer-to-peer inside

`orishu` is two different architectures at two different levels, and both
descriptions are simultaneously true:

- **From a client's perspective, `orishu` is client-server.** A client (`orishu-ctl`,
  `orishu-monitor`, Kagami, or any future viewer) connects to the cluster
  through the authenticated [client protocol](./protocol-client.md), issues
  control requests, and consumes streamed observations. The client never
  needs to know, or care, how many nodes the cluster has or how they found
  each other. It sees one addressable service.
- **Internally, that service is a peer-to-peer cluster.** Nodes self-organize
  using the [peer-to-peer protocol](./protocol-p2p.md): they discover each
  other, form and maintain membership (see the cluster-membership example in
  [`Coding style.md`](./Coding%20style.md#the-elm-architecture-model-message-update-with-or-without-a-view)),
  negotiate partition ownership, and coordinate step execution with no
  central coordinator. No node is a permanent server; any node can accept a
  client connection and act as that client's entry point into the cluster.

These are not competing descriptions that need to be reconciled — they are
answers to different questions ("how does a consumer reach the system" vs.
"how do the nodes providing the system agree with each other"), and `orishu`
answers each with the architecture that fits it. A single-node deployment is
the degenerate case: the peer-to-peer layer has one member, and the
client-server boundary is unchanged.

The client-facing server is therefore a **virtual authority**, not the worker
that happens to terminate a connection. Authoritative run state is identified
by the workload, epoch, and simulation boundary collectively committed by the
responsible workers. An entry node may relay that state, but cannot manufacture
another authoritative version of it.

### The server is a producer of a stream of states

Fundamentally, what a client is connecting to is a **producer of a stream of
simulated states**, ordered by simulation time (see
[`spatiotemporal-foundation.md`](./spatiotemporal-foundation.md)). That
framing is deliberately close to how a media server serves a video stream,
and the analogy is exact enough to reuse, not just illustrative:

- A **live** stream is a simulation currently advancing; the client
  subscribes to committed simulation boundaries as the cluster produces
  them, the way a viewer subscribes to a live broadcast.
- A **pre-recorded** stream is a previously computed result artifact; serving
  it is conceptually no different from serving a stored video file. The
  client requests a simulation-time range and receives it, live or not.
- A large pre-recorded artifact does not need to be served as one object.
  Exactly as a large file is split into pieces in a BitTorrent-style
  transfer, a result or checkpoint artifact is split into partition-aligned
  chunks (see [`storage-model.md`](./storage-model.md)) and reassembled at
  retrieval. The chunking exists for the same reason in both cases: no
  single node needs to hold, or serve, the whole object.

A client is symmetrically a **consumer of a stream of states**: it connects
to the cluster, initiates streaming from a chosen simulation-time point
(live or historical), and can pause, resume, or disconnect and later resume
from the last observation it held — the same operations a media client
performs against a stream, applied to simulation state instead of video
frames. Whether the underlying frames are being computed as the client
watches or replayed from storage is a property of the stream, not a
difference in the client-server contract.

Live observations follow an explicit snapshot/delta contract. A complete
snapshot establishes an observation baseline; each subsequent delta names both
that baseline and its target observation. The consumer resumes from the newest
complete observation it has adopted. If the producer no longer retains a
compatible baseline, it sends a new full snapshot. Baselines are bounded and
shared where possible—keyframes, partition chunks, and encoded deltas—not a
complete copy of the universe per observer.

Observation delivery is deliberately independent from simulation progress. A
slow observer may have intermediate presentation updates coalesced or skipped,
then recover from a newer full snapshot. This latest-state-wins rule applies
only to observation projections. Commands and decisions, halo exchange, step
coordination, workload artifacts, checkpoints, and other correctness-bearing
flows require reliable, identified, validated transfer; omitting one cannot be
repaired merely by receiving a newer message.

A committed observation logically describes the complete requested experiment
state, including selected fields over the authored domain. A simple consumer
may receive a complete field snapshot. Region, channel, and level-of-detail
subscriptions are bounded delivery optimizations; their queues and sampling
work are isolated so no observer can affect scientific state or block commit.

Kagami may interpolate or extrapolate between authoritative observations for a
smooth display, but that result is labelled presentation-only. It cannot be
used as a scientific result, checkpoint, halo value, or simulation input, and
can enter an experiment only through the explicit adoption command described
below. See [ADR 0011](./adr/0011-classify-network-flows-and-baseline-observation-deltas.md)
and the [resumable observation-streaming task](./tasks/implement-resumable-observation-streaming.md).

### Scientific capability is extended through simulation plugins

Orishu provides the compute platform while researchers choose the physical
models that define an experiment. Kagami ships with an initial model vocabulary
and lets advanced users install simulation plugins developed with ordinary
IDEs, compilers, and test tools. Kagami consumes completed packages; source-code
authoring is outside its CAD boundary.

```text
plugin developer              researcher                  cluster operator
IDE + tests                   Kagami                      Orishu tools
     |                           |                             |
     v                           v                             v
plugin manifest ----------> experiment revision ------> immutable workload
authoring schemas                    |                 pinned closure
workload component                   +-------------> Orishu cluster
```

A simulation plugin combines declarative authoring schemas with digest-pinned
sandboxed workload code. Schemas describe fields, parameters, dimensions,
constraints, initial conditions, and observations; they allow Kagami to expose
generic, validated authoring controls without loading arbitrary extension code
into its process. That code implements the executable state transition
through the workload lifecycle described below.

Fields are plugin-owned domain state: conceptually defined at every point in
the authored domain, though their numerical representation is model-specific.
An experiment selects one computational model per field family. Coulomb and
Maxwell/Yee are mutually exclusive electromagnetic alternatives sharing stable
charge coupling; classical gravity and GEM are analogous gravitational
alternatives. Hydrodynamics may model a real medium and its flow as field
state. Each plugin bundles both the authoring vocabulary and executable update
method; see [ADR 0023](./adr/0023-fields-are-plugin-modelled-domain-state.md).

The distinction from the object catalog is strict. A catalog template is
reusable authored data and never selects executable behaviour. A simulation
plugin explicitly adds physical-model vocabulary and executable physics.
Built-in gravity and electrodynamics support must use the same public contract
as third-party plugins. See [Simulation plugins](./simulation-plugins.md).

Objects compose those schemas rather than selecting a species implementation.
Pose and velocity are intrinsic kinematic state. A dynamics component owns
inertial integration; without it an object remains kinematic/static during a
run and no separate persisted motion authority exists. Gravity, electrostatic, and other coupling
components contribute typed forces at a committed boundary. The lifecycle
combines contributions deterministically and integrates each object once; see
[ADR 0020](./adr/0020-compose-object-behaviour-through-plugin-components.md).

Particle emission uses the same composition. An emitter is an ordinary object
whose component names catalog templates during authoring. The authoring command
atomically persists their materialized spawn blueprints; compilation revalidates
and copies those blueprints into the workload closure, and runtime spawns
are run state. Orishu never reads the authoring catalog; see
[ADR 0021](./adr/0021-capture-particle-emitter-recipes-in-workloads.md).

### Orishu orchestrates sandboxed workload components

Clients can submit new simulation logic, not just new parameters. Orishu treats
that logic as a graph of untrusted, digest-pinned WebAssembly Component
instances behind a versioned lifecycle. This is the modern form of the VM
boundary used by id Tech for distributable game code—portable program bytes on
one side, engine-owned scheduling and a narrow system-call surface on the
other.

```text
immutable workload manifest
  component instances + typed channels + deterministic step plan
                |
                v
       Orishu phase coordinator
     /              |                \
field-model guest  coupling guest  Dynamics guest
     \              |                /
      host-mediated candidate state/contributions
                |
                v
 validate complete candidate -> distributed commit
```

The worker owns the outer time loop, cluster membership, networking, component
placement, committed simulation boundaries, storage, and provenance. Each guest
can see only values supplied through `orishu:simulation/component@1`; it receives
no ambient filesystem, network, clock, randomness, process, or threading
capability. Guests cannot call one another; Orishu mediates bulk typed channels.
Memory, execution, host calls, and outputs are bounded and validated. A failed
required phase rejects the complete candidate boundary.

The workload graph and portable component digests identify executed physics
across heterogeneous nodes. Orishu may distribute independently partitionable
components while preserving the admitted dependencies and reduction order. A
local JIT/AOT cache is disposable implementation detail. Native shared
libraries are not a workload format, and any future runtime engine requires a
new security decision rather than claiming that the ABI alone makes it
equivalent. See
[ADR 0009](./adr/0009-execute-workloads-as-sandboxed-portable-programs.md) and
[ADR 0024](./adr/0024-orishu-orchestrates-a-workload-component-graph.md), plus
the [workload contract](./protocol-workload.md).

Logically, a workload is the immutable root manifest plus the complete closure
of digest-addressed code and input artifacts needed to execute it. Distribution
is a separate layer: thin upload, authenticated peer transfer, an optional
repository, and a self-contained archive may all deliver the same workload.
Source locations and archive bytes are not workload identity. See
[What is an Orishu workload?](./workloads.md) and
[ADR 0010](./adr/0010-content-addressed-workload-closure-and-portable-bundles.md).

## Kagami and Orishu

Before submission, Kagami owns an editable experiment. Submission will compile
a supported profile into an immutable Orishu workload manifest, package
reference, and input artifacts. After acceptance, Orishu is authoritative for
that workload and its run; Kagami remains authoritative for its editable
experiment.

Kagami communicates only through the authenticated client interface. It does
not join the peer network, own partitions, commit simulation steps, or directly
access worker GPU buffers. Live visualization consumes complete, versioned
observations associated with committed simulation boundaries.

The long-term local-preview adapter and remote-Orishu adapter must satisfy one
observation interface. Two adapters make that seam real; until both exist, the
repository should avoid inventing abstractions around hypothetical variation.

### Commands into documents, observations into runs

Kagami has two distinct authority boundaries. Its document authority owns the
editable experiment, while a local solver or Orishu owns a run. The former
decides proposed authoring commands; the latter publishes computed facts as
observations. Neither authority silently writes the other's state.

```text
UI ---------+
             +-- experiment command --> document authority --> revision
MCP adapter -+                              |
                                            | compile and submit
                                            v
                                     immutable workload
                                            |
local solver / Orishu -- observation --> run projection --> presentation
```

UI actions, MCP tool calls, undo, and redo all enter the same typed experiment
command path. The authoritative document transition validates a command and
either accepts it atomically as a new revision or rejects it with a domain
reason. Command envelopes carry stable identity and actor provenance; callers
that need optimistic concurrency may also name the base revision. Adapters do
not receive mutable access to the experiment, and successful transport does not
imply command acceptance.

Local and remote compute updates enter a separate observation path. Kagami
validates their workload, run, simulation-boundary, schema, and provenance
identity before updating a run projection. Rendering may compose an experiment
revision, a selected observation, and local presentation state, but this does
not mutate any of them. Turning a computed state into authored intent requires
an explicit command, such as adopting a selected boundary as new initial
conditions; if accepted, it creates a normal experiment revision and undo
entry with source provenance.

The UI, MCP, network, filesystem, and solver integrations form the imperative
shell. They translate IO into messages for pure document, run-projection, and
presentation transitions, execute returned effects, and feed effect outcomes
back as messages. See
[ADR 0004](./adr/0004-separate-authoring-commands-from-run-observations.md).

### Collaboration starts with files, not a shared document server

The first Kagami collaboration model is intentionally asymmetric. Draft
experiments are shared as files; there is no live multi-writer document. Once
an exact revision is submitted, however, multiple authorized Kagami clients
can observe the resulting run, subject to cluster resource limits. Each
connects independently to Orishu, follows the live head or replays persisted
results at its own position and speed, and owns its own camera, visibility,
selection, and observation subscription.

The document authority remains independent of UI and renderer state so it can
later run in a stand-alone headless collaboration service. Such a service would
order the same typed commands, publish accepted revisions, authorize submission
of an exact revision, and announce the resulting run reference. It would not
normally relay observations or impose one collaborator's playback state on the
others. Remote multi-writer editing needs an explicit protocol and security
decision before it is implemented. See
[ADR 0012](./adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md)
and [Implement time-addressable run playback](./tasks/implement-time-addressable-run-playback.md).

A saved file may also carry a separately versioned client-owned default view,
including projection and camera pose. Authoring view edits advance that view
revision and dirty the file, but are not part of the experiment revision, undo,
or workload. Observation/replay view edits are ephemeral. Within a window, Kagami switches
explicitly between Authoring and Observation/replay; returning to initial
conditions stops a local preview or detaches from a remote run, but never
silently stops remote execution. See
[ADR 0022](./adr/0022-persist-default-view-outside-experiment-intent.md).

### Variables and expressions cross the workload boundary

Kagami treats expression-capable numeric properties and named variables as
part of the experiment document. For example, a user can define
`mass_of_sun = 1e32 kg` and author an object's mass as `mass_of_sun / 2`, while
a dimensionless field can contain `1 / 3 + 0.1`. The document retains those
expressions; evaluated numbers alone are not a reproducible representation of
the author's intent.

```text
variable definitions + property expressions + property schemas
                              |
                              v
          parse -> resolve names -> check dependency graph and dimensions
                              |
                     candidate evaluation
                         /           \
                  accepted         rejected
                     |                 |
              new revision     previous revision
                     |
        source-bearing workload manifest
                     + optional resolved preview
                              |
                              v
               Orishu validate and resolve
                         /           \
                  accepted         rejected
                     |
       frozen canonical values -> component-instance graph
```

The variables and expressions subsystem is a shared pure-domain library. It
owns the versioned language, stable variable identities, explicit
namespaces/scopes, retained source, unit-aware evaluation, dependency ordering,
cycle detection, resource bounds, and source-located diagnostics. Kagami uses
it to evaluate candidate document revisions; defining or changing a variable
and assigning an expression remain ordinary experiment commands, so UI and MCP
edits share atomic validation, revisioning, and undo.

Workload compilation preserves that source-bearing graph in the manifest.
Orishu uses the same library to validate and resolve it authoritatively during
compatibility checking and workload acceptance, then freezes a canonical
resolved parameter set for the workload epoch. Workload code sees only that
resolved set; expressions are not reevaluated while simulation advances.
Kagami's prior evaluation is useful feedback, not trusted workload authority.

Only workload fields explicitly marked expression-capable by their schema join
the graph, including canonical references between such fields. Operational
resources and run observations remain ordinary resolved data. Importing an
observation into authored intent requires ADR 0004's explicit adoption path.
See [ADR 0005](./adr/0005-author-numeric-values-as-unit-aware-expressions.md)
and [ADR 0007](./adr/0007-share-expression-semantics-with-workload-resources.md).

### Catalog templates compile into experiment objects

Kagami owns an editable object catalog outside the experiment document. Its
versioned files define named, generic compositions of components, parameters,
and authored properties. Template expressions use the same variables, units,
dimensions, and diagnostics as experiment expressions; template identity never
selects solver behavior.

```text
UI catalog actions ----+
                       +--> catalog authority --> catalog files + revision
MCP catalog tools -----+             |
                                     | resolve template + fingerprint
UI/MCP instantiate ------------------+
                                     v
                              document authority
                                     |
                    self-contained object + provenance
                                     |
                              experiment revision
                                     |
                         compile immutable workload
```

The catalog authority is distinct from the document authority because editing
a local template is not editing the open experiment. Instantiation bridges the
two as an ordinary typed document command: it binds template parameters,
validates the complete candidate, mints an object identity, and atomically
persists the resulting authored component state plus source provenance.

An instance is a snapshot, not a live catalog link. Reloading, changing, or
losing a catalog entry cannot change an existing experiment. Applying a newer
template is an explicit, reviewable, undoable document command. Consequently a
saved experiment and the workload compiled from it never require Kagami's
catalog to be present, and Orishu has no catalog subsystem. Invalid templates
are isolated and diagnosed rather than replaced by plausible scientific
defaults. See
[ADR 0008](./adr/0008-catalog-templates-instantiate-self-contained-objects.md).

### External control through MCP

Kagami can embed an MCP server that lets authenticated external clients — AI
agents, scripts, alternative front-ends — command the same live session the UI
uses. The server is a transport over Kagami's authorities, not a second
authority: authoring commands enter the document authority, run controls use
the same run authority the UI uses, and presentation state stays local to the
Kagami window. The server is off by default, enabled by a startup flag or in
the UI, requires a fresh credential on every request, and binds only local
transports. See
[ADR 0006](./adr/0006-mcp-ui-equivalence.md) and the
[MCP user stories](./user-stories/kagami/mcp.md).

## Trust and data flow

### Operational observability

Worker IO adapters own operational metrics, traces and health projections.
[ADR 0017](./adr/0017-worker-operational-observability.md) plans a feature-gated,
separately configured HTTP diagnostics listener for Prometheus and process
probes, plus independently enabled OTLP trace export. Both remain off at
runtime by default. Telemetry consumes domain outcomes and never determines
membership, simulation commit or scientific validity. The sans-IO membership
core acquires no exporter, clock or network dependency. See the
[observability guide](./orishu-observability.md) for delivery and operator scope.

### Untrusted payloads

All network payloads are untrusted. Control messages and streamed observation
chunks require size limits, checked offsets, schema/version validation,
integrity checks, and bounded buffering. Kagami may retain the last complete
observation while disconnected, but it must label it stale and must never mix
chunks from different observation identities.
