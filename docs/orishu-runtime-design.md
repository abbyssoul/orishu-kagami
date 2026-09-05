# Orishu runtime design

This document owns the conceptual design assumed by Orishu's client and peer
protocols. `architecture.md` describes the combined product and authority
boundaries; this document focuses on the decentralized worker runtime.

## Design constraints

Orishu runs one immutable workload at a time across one or more workers. A
single worker is a cluster of one, not another execution mode. Workers
self-organize without a permanent leader, scheduler, or external metadata
service. The system favors deterministic scientific correctness, explicit
authority, and bounded recovery over hidden throughput optimizations.

Cluster mode exists for compute capacity and for states and artifacts too large
for one machine. Membership may change during a run, but ownership changes only
at safe committed boundaries and may temporarily reduce performance.

Every peer, client, workload, artifact, and inventory message is untrusted even
when authenticated. Validation and cost bounds precede state adoption.

## Data model

The canonical entity classification is `orishu-data-model.md`. Persisted and
wire types are versioned. Cluster-replicated intent and node-local operational
state remain distinct; a runtime projection is not silently serialized as
durable truth.

## Node identity

ADR 0013 defines identity. Worker and cluster names are non-unique display
labels. A formation ID scopes peer traffic and durable origin provenance. A
formation assigns each admitted member a unique node ID. Before admission, the
join flow uses the worker name and certificate identity.

## Member acceptance

Admission succeeds only after transport authentication and every applicable
gate passes: formation/join intent, current join token, membership lock,
introducer peer-admission flag, capacity, blocklist, membership tombstone, and
protocol compatibility. `protocol-p2p.md` fixes their evaluation order and the
re-check that follows credential verification. A rejection is explicit and
bounded; redirect hints are untrusted candidates, not authority.

Removal is distinct from liveness. A membership tombstone is an operator or
policy decision that fences a node's assigned ID and pinned certificate; the
failure detector may declare a node dead but never writes one, and a voluntary
leave does not create one. Clearing a tombstone is an explicit versioned
update, not a deletion.

An introducer's bootstrap snapshot does not make that node a permanent leader.
The joining worker validates it and then converges through gossip and
anti-entropy. Automatic discovery and admission are not MVP behavior.

## Deterministic execution contract

Simulation time advances through accepted fixed steps. A step is scoped by
formation, workload, epoch, partition, ownership version, and boundary. Packet
timing, duplicate delivery, retry order, membership churn, and wall-clock time
must not change accepted scientific state outside the workload's declared
numerical tolerance.

The runtime owns the outer time loop, partitioning, halo exchange, barriers,
commit, storage, provenance, cancellation, and resource enforcement. Untrusted
components propose outputs through the lifecycle; a trap, timeout, invalid
buffer, non-finite value, or incompatible identity rejects the transition and
cannot publish a partial boundary.

## Partition ownership and rebalancing

Ownership is explicit for `(workload, epoch, partition, ownershipVersion)`.
Only the current fenced owner may propose authoritative output. Transfer is a
versioned transition coordinated between old and new owners, or recovered after
the old owner is declared unavailable according to protocol. State and required
halos are verified before the new owner contributes.

Rebalancing and work stealing choose candidates; they do not bypass ownership
or introduce an independent task scheduler. Transfer occurs at a safe boundary.
Stale owners and delayed messages cannot commit after authority moves.

## Cluster state and reconciliation

Workers converge replicated membership and intent through versioned merge
semantics, gossip, and periodic anti-entropy. Implementations should express
correctness-relevant changes as typed transitions in a functional core and keep
network, storage, timers, and process control in IO adapters. The historical
shared-mutable-state/internal-event-bus design is not retained as a requirement.

Node-local state includes connections, in-flight buffers, local inventory,
metrics, and diagnostic logs. Cluster-visible summaries derived from that state
may be stale and must identify their source and freshness.

## Local inventory and availability view

`storage-spec.md` defines these authority levels. Inventory is a node-local
claim; availability is a derived routing view; immutable records and verified
bytes establish artifact truth. Join-time reconciliation is separate from
admission and bounded independently.

## Result sequences versus stored result artifacts

Each stored result artifact is immutable. A result sequence is a derived
operator/client grouping across graceful stop and resume of one lineage. It is
not a mutable server resource. Rewind to a different checkpoint, unload, or
workload replacement ends the current sequence without rewriting prior
artifacts.

## Access tiers

The initial client protocol distinguishes local read-oriented access from
privileged or remote access. All mutations and remote operations require an
authenticated administrative authority. Worker join tokens, worker
certificates, and operator credentials are separate and non-interchangeable.

The MVP uses shared cluster-administrator authority: an authenticated
administrator may reverse or supersede another administrator's cluster-scoped
change. Fine-grained RBAC and multi-party approval require a later decision.

## Resource-oriented API design

Client URIs identify nouns; HTTP methods express actions; state transitions are
resource updates. Singleton resources are acceptable where presence has useful
semantics. The remaining imported endpoint-shape concerns are recorded in
`orishu-runtime-future-work.md` and must be resolved before compatibility
commitments.

## Events, audit, and logs

Logs are free-form node diagnostics. Events are structured observations of
runtime changes. Audit records describe security- or operator-relevant actions,
including admission, removal, lock, blocklist, workload lifecycle, token
rotation, tombstone clearing, and artifact purge.

Audit dissemination is best-effort unless a later durable audit design says
otherwise. It supports traceability, not total ordering, artifact-content
authority, or ownership of cluster state.
