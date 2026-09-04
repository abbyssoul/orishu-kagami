# Orishu artifact storage specification

This document owns operational semantics for checkpoint and result storage.
`storage-model.md` explains the architecture and tradeoffs; this specification
defines authority, safety, failure behavior, and maturity. Workload-input
storage is additionally constrained by ADR 0010.

## Maturity

| Area | Status |
| --- | --- |
| Artifact records and SHA-256 chunk identity | Accepted for initial implementation |
| Local inventory as advisory input | Accepted for initial implementation |
| Bounded inventory reconciliation | Accepted contract; wire details pending |
| QUIC-native committed-chunk transfer | Accepted by ADR 0015 |
| Purge tombstones and join-time suppression | Accepted by ADR 0014 |
| Read repair from verified bytes | Accepted contract; policy details pending |
| Re-replication and hinted handoff | Direction accepted; lifecycle pending |
| Created/visible/retrievable/durable transitions | Open |
| Cross-formation purge retention and compaction | Open |

Do not infer open behavior from storage backend conventions.

## Authority layers

### Artifact record

An immutable `CheckpointRecord` or `ResultRecord` is authoritative for artifact
identity, required chunk descriptors, integrity anchors, committed provenance,
and creation-time incompleteness. It never contains a live holder or retrieval
location.

### Local inventory

A worker's durable local inventory states what that worker believes it can
serve. It is node-local, backend-shaped, potentially stale or dishonest, and is
never proof that bytes exist or are correct.

### Availability view

The availability view is a transient routing projection derived from artifact
records, current membership, inventory claims, verified fetch results, and
backend visibility. It may guide reads but cannot change artifact identity or
integrity.

## Chunk identity

A committed output chunk is addressed by:

| Field | Meaning |
| --- | --- |
| `artifactId` | Checkpoint or result artifact identity |
| `artifactKind` | `checkpoint` or `result` |
| `partitionId` | Spatial partition represented by the chunk |
| `originFormationId` | Formation that produced the artifact |
| `contentHash` | Expected SHA-256 digest |
| `sizeBytes` | Exact expected byte length |

Provider-specific keys are derived storage details. They do not enter the
cluster contract.

## Transfer and placement

Per ADR 0015, committed chunks move through bounded `FetchChunk` exchanges over
authenticated QUIC peer streams and are accepted only after exact size and hash
verification. The cluster-managed ring, virtual nodes, and replication target
select intended holders. Transport neither selects placement nor establishes
durability.

The node-local artifact catalog, derived from validated artifact-record gossip,
maps workload, epoch, and artifact kind to candidate artifacts. There is no
public or in-band swarm discovery.

## Inventory reconciliation

A join request never contains an unbounded inventory dump. After admission,
inventory uses explicit bounded batches or streaming continuation. Processing
is idempotent, resumable, and limited by items, bytes, nesting, CPU, memory,
disk work, and concurrency.

The intended order is:

1. Complete admission and bind the connection to a formation and node identity.
2. Advertise bounded inventory batches.
3. Reject malformed, oversized, stale, duplicate-conflicting, or cross-identity
   entries.
4. Check every artifact identity against purge tombstones.
5. Instruct deletion for matches before considering availability.
6. Add non-matches as provisional routing candidates.
7. Confirm or disprove claims only through verified retrieval.

When work must be prioritized, prefer current-formation artifacts, resumable
checkpoints, recently updated imported artifacts, then older imports. This is a
policy default, not permission to process an unbounded backlog.

## Availability states

Operators must be able to distinguish:

- **creation-time incomplete:** required chunks were never durably written;
- **currently unavailable:** chunks may exist but no reachable verified holder
  can currently serve them;
- **under-replicated:** the artifact is retrievable but below its durability
  target; and
- **discovering:** bounded reconciliation is still establishing candidate
  holders.

These states must not be collapsed into one boolean. A committed record is not
proof of present retrievability, and retrievability is not proof that the
durability target is met.

## Retrieval and repair

A coordinator selects candidate holders, fetches bounded data, verifies exact
length and digest, and only then assembles or forwards bytes. Invalid or missing
responses reduce confidence in that holder and produce observable failure.

Read repair may copy only bytes verified against the immutable artifact record.
The current ring primary is a routing preference, not an authority. Repair must
be fenced against stale formation, workload, epoch, artifact, partition, and
content identities.

Re-replication after holder loss is cluster policy separate from read repair.
Its triggers, source selection, concurrency limits, fencing, completion
criteria, and operator-visible state require a dedicated implementation task.

## Backend contracts

- `local` stores worker-owned durable chunks and durable local inventory.
- `memory` is explicitly volatile and must not advertise restart-surviving
  inventory.
- `external` may expose one canonical object namespace to several workers;
  availability then depends on backend reachability and credentials rather than
  distinct node-held copies.

No backend may silently redefine artifact identity, integrity, provenance,
completeness, or cluster control-plane semantics. Backend startup and runtime
failures must be explicit; there is no plausible fallback to another backend.

## Purge behavior

Artifact deletion follows ADR 0014. A purge is not complete merely because the
current catalog entry disappeared. The system retains deletion intent and
suppresses stale inventory before it can affect availability.

## Open operational contracts

Before storage can be called stable, specify and test:

- the exact transitions between created, listed, retrievable, and durable;
- hinted-handoff representation, expiry, replay, crash recovery, and bounds;
- re-replication completion and degraded-state reporting;
- purge persistence and compaction across formations;
- external-backend deletion and consistency behavior; and
- whole-artifact streaming limits and partial-download representation.

## Failure expectations

| Failure | Required meaning |
| --- | --- |
| Writer fails before persist | Artifact may be incomplete from creation |
| Replica propagation fails | Artifact may be retrievable but under-replicated |
| Claimed chunk fails verification | Reject bytes and downgrade holder confidence |
| Inventory exceeds bounds | Stop/reject reconciliation without destabilizing the node |
| Offline holder returns after purge | Delete stale data; do not resurrect artifact |
| All holders are unavailable | Report current unavailability, not non-existence |
| External store is unavailable | Report backend failure distinctly from membership failure |
