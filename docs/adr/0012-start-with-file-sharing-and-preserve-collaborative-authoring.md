# 0012 — Start with file sharing and preserve collaborative authoring

Status: **accepted**  
Date: **2026-09-04**

## Problem

Scientists need to share experiments while they are being authored and to
inspect the same live or recorded simulation with other people. These are two
different collaboration problems:

- **authoring collaboration** decides which proposed edits become the next
  authoritative experiment revision; and
- **observation collaboration** lets multiple people inspect one immutable run
  while retaining independent cameras, subscriptions, timeline positions, and
  playback rates.

Kagami already places its document authority inside the application process.
That boundary could later be hosted by a stand-alone headless service, but
doing so introduces user identity, permissions, command ordering, conflict
handling, shared undo semantics, asset transfer, persistence ownership, schema
compatibility, and reconnect recovery. The existence of a command boundary
does not make those product decisions free.

Orishu adds a separate constraint: a cluster runs at most one workload at a
time. A shared authoring session therefore cannot let several clients silently
turn their current drafts into competing cluster state. Submission must name an
exact experiment revision and be decided by explicit authority.

## Options considered

### 1. Local authoring and file-based draft sharing

Each person runs a local Kagami document authority and owns an experiment file.
A draft is shared explicitly as a file, through version control, or through an
ordinary file-sharing mechanism. There is no live multi-writer document.

One Kagami instance submits an exact experiment revision as an immutable
workload. After Orishu accepts it, multiple authorized Kagami instances may
connect to the run's observation stream, subject to cluster resource limits.
Each observer can follow the live head or replay persisted results at its own
simulation-time offset and playback rate.

```text
Kagami A ---- share experiment file ---- Kagami B
    |
    | submit immutable revision
    v
Orishu cluster ---- live/persisted run ---- Kagami A (follow head)
          |------------------------------- Kagami B (replay at t1)
          +------------------------------- Kagami C (replay at t2)
```

Implications:

- The initial product needs no collaborative document protocol, account model,
  conflict resolver, or always-on Kagami service.
- File exchange is explicit and understandable, but simultaneous edits require
  human or external version-control reconciliation.
- Submission and observation remain separate: submitting freezes one revision,
  while the local draft may continue to evolve toward a later workload.
- Multiple observers exercise Orishu's normal resumable observation protocol;
  no Kagami process becomes their video relay or availability dependency.

### 2. A shared headless document authority

Several Kagami clients connect to one stand-alone authoring service. UI, MCP,
and remote-client adapters submit typed commands to the same document
authority, which orders them and publishes accepted experiment revisions.

An authorized actor may ask the session to submit an exact revision. The
session records the accepted mapping from that revision to an immutable
workload and Orishu run. Collaborators learn that run reference and normally
open their own observation subscriptions directly to Orishu.

```text
Kagami A ---+
Kagami B ---+--> headless document authority --> experiment revision N
Kagami C ---+                                  |
                                                  | authorized submission
                                                  v
                                             Orishu cluster
                                              /     |     \
                                  independent A     B      C subscriptions
```

Implications:

- All collaborators see one authoritative sequence of accepted edits.
- A central authority is simpler than peer-to-peer document merging, but it
  still requires actor identity, capabilities, stable command IDs, base
  revisions, conflict responses, reconnect snapshots/deltas, persistence, and
  operational ownership.
- Undo cannot mean rewinding a private mutable stack. It must be an authorized,
  attributable command against shared history.
- Submission permission belongs to a role or lease, not inherently to the
  client hosting a window. The session serializes submission requests, and
  Orishu remains authoritative about accepting, rejecting, or replacing its
  single active workload.
- The submitted revision is immutable. Concurrent authoring may continue as a
  successor revision without changing the active run.
- Catalog files remain client-owned unless a separate collaborative catalog
  decision is made. Instantiated catalog objects are already self-contained in
  the experiment and therefore safe to share through the document authority.

Two observation topologies were considered within this option. Relaying all
simulation data through the authoring service would simplify network access,
but would couple document availability to high-volume observations, create a
bottleneck, and imply a single shared playback state. Independent Orishu
subscriptions preserve per-client region, channel, level-of-detail, camera, and
timeline choices. A gateway may be added for constrained networks, but it must
implement the Orishu observation contract and is not document authority.

### 3. Peer-to-peer collaborative documents

Kagami clients could exchange edits without a stable document authority and
merge them through a CRDT or other replicated-data algorithm.

This improves disconnected multi-writer availability, but introduces merge
semantics for structured scientific intent, expressions, object identity,
deletion, validation, and undo before the product has demonstrated that need.
It also conflicts with the existing design in which commands are atomically
accepted or rejected by one document authority. This option is not selected;
it would require a separate architectural decision.

## Decision

Kagami initially implements **local authoring with file-based draft sharing**.
It does not promise live collaborative editing in the first product. Submitting
a workload freezes an exact experiment revision, and multiple Kagami clients
observe the resulting live or persisted run through independent Orishu
subscriptions.

The initial implementation must nevertheless preserve the option to move the
document authority into a stand-alone headless service later:

- The authoritative experiment model and command transition have no dependency
  on windows, widgets, rendering, or local process identity.
- UI, MCP, file, and future remote adapters submit the same typed commands and
  receive the same accepted/rejected outcomes. No adapter mutates the document.
- Commands have stable identity and actor provenance and may name a base
  revision, as required by ADR 0004.
- Experiment snapshots, revision identities, command outcomes, and referenced
  draft assets have explicit serializable domain representations rather than
  pointers into UI or renderer state.
- Submission always names an immutable experiment revision and produces or
  records an explicit run reference: cluster identity, workload identity, and
  workload epoch/run identity as defined by the Orishu protocol.
- Presentation state remains local by default. Camera, selection, visibility,
  subscription, playback cursor, and playback rate are not authoritative
  experiment state.
- Observers connect directly to Orishu and use its snapshot/delta baseline and
  replay semantics. A future headless authoring server announces run identity;
  it does not prescribe what every collaborator sees.

The future service is a deployment of the same document authority, not a
second document model. Adding networking around it will require a new protocol
and security decision covering collaboration-specific identity, permissions,
ordering, conflicts, shared undo, persistence, assets, version compatibility,
and reconnection.

## Consequences

- The first usable collaboration story is deliberately modest: exchange an
  experiment file, submit a chosen revision, and share a run reference.
- Multi-observer live and historical playback is part of the Orishu client
  contract, not deferred with multi-writer authoring.
- Kagami can remain a CAD application while observing a run. Editing a successor
  draft never mutates the already submitted workload.
- Nothing in the initial document core may assume that its only caller is the
  local UI thread, even though it remains in the local Kagami process.
- A future collaborative session shares document revisions and run references,
  not presentation state. “Follow presenter,” shared camera, presence, and
  synchronized playback are optional ephemeral collaboration features.
- Remote authoring, collaborative catalogs, offline multi-writer merging, and a
  simulation-data gateway are not implied by the initial implementation.

Independent historical observation is tracked in
[Implement time-addressable run playback](../tasks/implement-time-addressable-run-playback.md).
