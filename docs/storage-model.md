# Artifact Storage Model

This document captures the **architectural decision** for how `orishu` stores, locates,
and retrieves simulation artifacts (checkpoints and results).

This version describes output artifacts. ADR 0010 applies the same fundamental
descriptor-versus-location rule to immutable workload components and inputs,
but workload closure storage, pinning, and garbage collection are tracked in
the
[shared workload-format task](./tasks/define-and-adopt-shared-workload-format.md).
The workload definition never stores a live holder or retrieval location.

It is intentionally a **decision and rationale document**, not the final operational
specification. It explains the storage model, why it fits `orishu`, what invariants it
protects, and what kinds of backend variation are intentionally supported.

For detailed operational behavior, see [storage-spec.md](./storage-spec.md). For the
formal classification of individual entities, see [orishu-data-model.md](./orishu-data-model.md).

> **How to read this document.** This is rationale, not a normative specification. It records
> *why* the storage model is shaped the way it is and which invariants any backend must
> preserve. Do not implement operational behavior from this document — the canonical operational
> contract (failure behavior, reconciliation, repair, maturity classification) is
> [storage-spec.md](./storage-spec.md). If this document and the spec disagree about behavior,
> the spec is authoritative.

---

## The decision

The artifact store borrows the **descriptor-vs-location separation** from BitTorrent/Kademlia —
an artifact record describes what a chunk is, never where it lives — without adopting either
system's swarm or DHT routing. Placement is cluster-managed (a consistent-hash ring, in the
Cassandra sense) and discovery goes through the control-plane catalog, not XOR-metric routing
or peer/tracker gossip; see the "why not BitTorrent" note under
[transport binding](#transport-binding) and
[what was deliberately not adopted](#what-was-deliberately-not-adopted).

This document describes the **internal, node-to-node view** of `orishu` — how chunks are
placed, discovered, and transferred among cluster members. It is the peer-to-peer half of
`orishu`'s architecture. From a client's perspective, none of this is visible: retrieving a
result artifact looks like requesting a range of a stream of simulated states from a single
service. See
[architecture.md](./architecture.md#client-server-from-outside-peer-to-peer-inside) for how
the client-server and peer-to-peer views relate, and
[Result and checkpoint artifacts as stream state](#result-and-checkpoint-artifacts-as-stream-state)
below for how the two vocabularies map onto each other.

The single most important consequence of this decision is:

> **Chunk location is never stored in an artifact record.**

Artifact records (`CheckpointRecord`, `ResultRecord`) describe *what* an artifact is —
content descriptors per partition chunk — but not *where* the chunks live. Location is
always a runtime answer derived from cluster state, storage visibility, and node-local
inventory claims. It changes as nodes join, leave, fail, or as the backend topology
changes. The artifact record does not.

This keeps artifacts portable across cluster reformations and prevents immutable artifact records
from being coupled to transient node membership.

---

## Transport binding

Artifact **chunk transfer** is QUIC-native: chunks are fetched, content-addressed and verified,
over the cluster's existing authenticated peer streams. Per
[ADR 0015](./adr/0015-use-quic-native-artifact-transfer.md)
(which superseded the earlier BitTorrent proposal, decision-006, after the TASK-065 comparison),
the transport reuses what the cluster already has rather than importing a swarm stack:

| Concern | How it is served |
|---|---|
| Chunk identity / integrity | `ChunkRef` + SHA-256 `contentHash` — the single, end-to-end integrity authority |
| Chunk transfer | a `FetchChunk(ChunkRef)` RPC over the existing mTLS/QUIC peer streams |
| Which nodes hold a chunk | local inventory (advisory) → the availability view |
| Parallel fetch ("rarest-first") | fan out across alive holders from the availability view; fetched chunks become eligible holders (opportunistic re-seed) |
| Discovery / search | the control-plane artifact catalog (`workloadId` / `workloadEpoch` / `artifactKind` → artifact) |
| Resume | by offset, with incremental verification via `MerkleDigest` |

Key properties, specified operationally in [storage-spec.md](./storage-spec.md):

1. **Cluster-managed placement.** The consistent-hash ring + vnodes + replication factor decide
   which nodes hold which chunks. Durability is cluster-enforced; the transport only moves
   verified bytes between the chosen holders.
2. **Single integrity authority.** `artifactId` is a UUID; each chunk's SHA-256 `contentHash` is
   the only integrity authority — verified end-to-end, no second hash and no transport-specific
   handle.
3. **Catalog discovery.** The control-plane catalog is the authoritative, node-local index from
   `workloadId` / `workloadEpoch` / `artifactKind` to artifacts; no in-band discovery channel.
4. **No new attack surface.** Transfer rides the existing authenticated, formation-scoped QUIC
   streams and CBOR framing — no new wire parser, no public swarm, no SHA-1.

> **Why not BitTorrent.** A BitTorrent-family transport (decision-006) was proposed and then
> rejected on review: its constraints disabled the very features that justify it (DHT/PEX
> discovery, emergent durability, native integrity, swarm scheduling), leaving a large dependency
> and two new hostile-input parsers to buy piece-scheduling that is unnecessary when the catalog
> already tells every node who holds what. This matches the same reasoning that rejected Kademlia
> below ("bounded, known cluster → prefer the simpler model"). A swarm transport may be
> reconsidered only if a benchmark ever shows a large-artifact fan-out win that matters for
> `orishu`'s one-workload-per-cluster regime ([deferred runtime design](./orishu-runtime-future-work.md)).

---

## Why this fits the problem space

`orishu` workloads are spatiotemporal: the simulation advances forward in simulation
time over a domain that is partitioned in space. This is not incidental — it is the
structural property that makes the storage model coherent.

See [spatiotemporal-foundation.md](./spatiotemporal-foundation.md) for the full rationale.
The storage consequences are:

**Spatially:** Artifacts are exposed to the cluster as partition-aligned chunks. Each
chunk corresponds to a stable partition ID — a first-class runtime concept, not an
arbitrary byte range.

**Temporally:** Artifacts are anchored to simulation-time boundaries or ranges. They are
immutable once committed. A checkpoint captures a specific boundary; a result captures
output over a range.

Together, these properties mean the cluster-managed placement model is not being imposed on
the problem — the problem naturally has the shape that makes this storage model a strong fit.

---

## Result and checkpoint artifacts as stream state

`architecture.md` describes `orishu` from a client's perspective as a producer of a stream of
simulated states, ordered by simulation time, where "live" and "pre-recorded" are just
properties of the stream rather than different contracts (see
[architecture.md#the-server-is-a-producer-of-a-stream-of-states](./architecture.md#the-server-is-a-producer-of-a-stream-of-states)).
This document's vocabulary maps directly onto that framing:

- A **checkpoint artifact** is the stream's resumable state at one point — the equivalent of a
  seek/resume point in a recording.
- A **result artifact** is a recorded segment of the stream over a simulation-time range —
  the equivalent of the recording itself.
- A **chunk** is a partition-aligned fragment of either, which is why large artifacts do not
  need a single node to hold or serve the whole object, exactly as a large media file is split
  into pieces for distributed serving.

Per [ADR 0002](./adr/0002-initial-value-problem-scope.md), `orishu` advances state only forward
from a known initial condition; there is no boundary-value coupling across simulation time.
That is what makes "stream" an exact metaphor rather than an approximation: a checkpoint or
result artifact's position in simulation time is fixed once committed and never revised by
later computation, the same way a recorded stream segment does not change after it is written.

---

## Synthesis from research

Four systems were studied. Each contributed specific ideas that are adopted, adapted,
or deliberately not adopted for `orishu`. See
[the storage-pattern synthesis below](#synthesis-from-research)
for the full analysis.

### From BitTorrent / Kademlia: descriptor-vs-location separation

**Adopted:** The separation of the artifact record (what the artifact is, expressed as
content hashes per chunk) from the location index (which peer or backend currently serves
the bytes) is the defining idea.

In `orishu`, `CheckpointRecord` and `ResultRecord` are the stable artifact records.
Current location is always discovered separately.

**Adapted:** `orishu` does not use Kademlia's open-swarm routing machinery, nor a real
BitTorrent transport. Cluster membership is bounded and known through SWIM-style membership
rather than an unbounded internet-scale DHT, and chunk transfer is QUIC-native over the existing
authenticated peer streams (see the transport binding above and
[ADR 0015](./adr/0015-use-quic-native-artifact-transfer.md)) — only the descriptor-vs-location *idea* is borrowed.

**Adopted:** Content addressing for integrity. Chunk-level hashes allow the cluster to
verify retrieved bytes independently of who served them.

### From Cassandra: consistent-hash placement and coordinator fan-out

**Adopted:** The consistent-hash ring as the reference model for write-time placement and
expected retrieval routing in cluster-managed backends.

**Adopted:** Virtual nodes (vnodes) to smooth placement and reduce large reshuffles on
membership change.

**Adopted:** Coordinator-based retrieval. Any client-facing node may coordinate a read,
fan out internally, assemble a full artifact, and return one logical response.

**Adapted:** `orishu` keeps artifact record portable by not storing live location inside
the immutable artifact record.

### From Redis Cluster: locality-aware routing

**Adopted:** The general idea that placement and routing should preserve locality where
possible.

In `orishu`, partition IDs and workload topology are the natural units for locality.
Related partitions may be placed or routed in ways that reduce cross-node traffic during
computation and artifact access.

### From GFS: separate metadata path from data path

**Adopted:** Metadata and data flow through different mechanisms.

- Artifact records and related storage metadata move through the cluster's control-plane
  mechanisms.
- Chunk bytes move directly between data endpoints.

**Not adopted:** A centralized metadata master. `orishu` remains decentralized.

---

## The three-layer model

### Layer 1 — Artifact records

`CheckpointRecord` and `ResultRecord` describe *what* the artifact is:

- immutable artifact identity
- content descriptors per chunk
- artifact-level integrity anchors
- provenance such as `originFormationId`
- permanent creation-time incompleteness information

These records are intended to remain valid even when the original writers are gone and a
new cluster formation later rediscovers the artifact.

### Layer 2 — Local inventory

Each node may maintain durable local inventory describing what chunks it believes it can
serve.

This layer is intentionally weaker than the artifact record:

- it is node-local rather than global truth
- it is shaped by the configured storage backend
- it may become stale
- it is an **advisory claim**, not proof that bytes are still retrievable or correct

Inventory is therefore useful for routing and historical rediscovery, but it is not
authoritative until actual bytes are retrieved and verified against the artifact record.

### Layer 3 — Runtime routing and availability view

The cluster derives a current answer to questions such as:

- which endpoints should be tried first for a chunk
- whether an artifact appears currently retrievable
- which holders are online, degraded, stale, or missing

This availability view is dynamic. It is not a durable artifact record. It is a runtime projection that
changes with membership, reconciliation progress, and backend state — the same shape as the
cluster-membership model in
[`Coding style.md`](./Coding%20style.md#the-elm-architecture-model-message-update-with-or-without-a-view):
a pure fold of arriving messages (peer heartbeats, fetch results, reconciliation events) over an
immutable prior view, kept separate from the durable, message-independent artifact record.

---

## Backend families and extension boundary

Storage is a planned extension point in `orishu`. The point of variation is the
implementation **below** the cluster-visible artifact contract, not the cluster semantics
above it.

The current backend families are:

- `memory`
- `local`
- `external`

These backends may differ significantly in durability, repair behavior, placement, and
operational dependencies. That variation is intentional.

### External backend is not just "local, but elsewhere"

The `external` backend is especially important because it can change how inventory and
availability behave.

For example, if all workers can read and write the same canonical object store, then the
cluster may not need to reason about distinct node-held chunk copies in the same way it
does for `local` storage. Multiple nodes may report visibility into the same canonical
artifact namespace rather than unique physical holdings.

That makes `external` a useful research configuration, but it also means operator-facing
availability, discovery, and repair semantics must be described separately from purely
node-local storage modes.

The exact operational behavior belongs in [storage-spec.md](./storage-spec.md), not in
this decision note.

---

## Fixed invariants

Any backend or retrieval strategy variation must preserve these invariants:

| Invariant | Why it matters |
|---|---|
| No live location in artifact records | Keeps immutable metadata portable across topology change and cluster reformations. |
| Artifact records are the integrity authority | Retrieval and repair are grounded in artifact records and verified bytes, not in transient ownership alone. |
| `originFormationId` is part of durable artifact identity | Prevents cross-formation ambiguity and addressing collisions. |
| Inventory is advisory, not authoritative | A node's claim to hold data must not be treated as proof until bytes verify. |
| Backend choice does not redefine cluster control-plane semantics | Storage variation is allowed below the contract, not as a way to change cluster meaning. |
| Availability is a runtime view, not durable truth | Operator-visible current state must remain distinguishable from immutable artifact records. |

---

## Reliability and safety implications

This decision carries several important consequences.

### Inventory reconciliation must be bounded and defensive

Join-time or recovery-time inventory sharing must never assume a node can dump an
unbounded full inventory in one admission message. That would create an easy denial of
service path and an operational hotspot at the introducer.

Therefore the operational model must use:

- bounded advertisements,
- batching or streaming,
- explicit continuation,
- resource limits,
- replay/idempotence handling,
- and validation of malformed or oversized input.

Exact mechanics belong in [storage-spec.md](./storage-spec.md).

### Read repair for immutable artifacts is hash-authoritative

For immutable artifacts, repair must be grounded in verified content matching the artifact
record — not in the assumption that the current ring-primary is always the truth source.

Ring placement is a routing hint and a write-time expectation. The immutable artifact
record and successful hash verification are the real authority.

### Re-replication is a separate operational policy

The architectural stance is that cluster-managed backends may restore durability after
holder loss, similar in spirit to Redis/Cassandra-style recovery behavior.

However, the exact trigger conditions, fencing, source selection, and completion rules are
operational semantics and belong in [storage-spec.md](./storage-spec.md), not here.

---

## What was deliberately not adopted

| Pattern | Source | Reason not adopted |
|---|---|---|
| XOR-metric DHT routing | Kademlia | Cluster membership is bounded and known; a simpler cluster-aware routing model is preferred. |
| Fixed hash slots | Redis Cluster | A ring + vnodes better fits heterogeneous nodes and elastic rebalancing experiments. |
| Centralized metadata master | GFS | Conflicts with `orishu`'s decentralized control-plane stance. |
| Mutable location in artifact records | Dynamo/Cassandra-style location-heavy metadata | Would couple durable artifact records to transient topology. |

---

## Failure-mode questions this model should support

This decision doc is also meant to support later failure analysis and operator guidance.
It should make the following questions answerable by the formal spec and operational docs:

- What happens if a node fails before local chunk persistence completes?
- What happens if a node fails after local persist but before replica propagation completes?
- What happens when inventory claims are stale or dishonest?
- How does the cluster distinguish permanent artifact incompleteness from temporary holder loss?
- How should a purge interact with a later-joining node that still has old local chunks?
- How should `external` backend failure differ from `local` backend failure in operator UX?

The detailed answers belong in [storage-spec.md](./storage-spec.md) and later operator-facing
guides, but the architectural model should make those questions natural rather than awkward.

---

## Relationship to other documents

- [project context](../CONTEXT.md) — canonical terminology, ownership, and invariants
- [spatiotemporal-foundation.md](./spatiotemporal-foundation.md) — why the compute and storage model share the same spatial/temporal decomposition
- [architecture.md](./architecture.md#client-server-from-outside-peer-to-peer-inside) — where this document's node-to-node model sits relative to the client-facing stream-of-states view
- [storage-spec.md](./storage-spec.md) — operational storage semantics and failure behavior
- [orishu-data-model.md](./orishu-data-model.md) — runtime entity classification and authority
- [storage-pattern synthesis](#synthesis-from-research) — research synthesis behind these choices
- [ADR 0002 — Scope orishu to initial value problems](./adr/0002-initial-value-problem-scope.md) — why artifact simulation-time anchoring is forward-only and never revised
- [ADR 0003 — Distribute orishu as a single self-sufficient binary](./adr/0003-single-binary-distribution.md) — why chunk serving is a role of every `orishu-worker` node rather than a separate storage service
