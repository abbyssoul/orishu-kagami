# 0014 - Prevent purged artifacts from returning through stale inventory

Status: **accepted**

## Context

Artifact records and chunks can outlive a cluster formation. A worker may be
offline when an operator deletes an artifact and later advertise a stale local
copy. Without persisted deletion intent, normal inventory reconciliation would
make the deleted artifact visible again.

Membership tombstones do not solve this problem. They describe removed nodes
and are scoped to one formation; artifact deletion concerns immutable data that
may survive that formation.

## Decision

A deleted stored artifact must not reappear merely because an offline worker
later rejoins with stale inventory.

Artifact deletion creates a **purge tombstone** keyed by
`(artifactId, originFormationId)`. It records the artifact kind, deletion time,
actor, and an optional reason. It is transport-independent and is propagated as
grow-only deletion intent alongside artifact-record state. The first
implementation persists it within the active formation; cross-formation
retention remains explicitly open below.

After admission, a worker advertises local inventory in bounded batches. The
receiving cluster checks each artifact identity against purge tombstones before
allowing it into the availability view. A match produces an idempotent deletion
instruction; the worker deletes its chunks and local inventory record. A
non-match is only a routing claim until fetched bytes verify against the
authoritative artifact record.

There is no routine un-purge operation in the first implementation. Clearing a
membership tombstone does not affect purge tombstones, and removing a purge
tombstone must never be confused with readmission of a node.

## Security and limits

Inventory count, bytes, nesting, continuation state, CPU, memory, disk work, and
concurrency are bounded per peer. Advertisements are streamed or batched,
replay-safe, resumable, and rejected when malformed, oversized, stale, or
cross-formation. A dishonest claim cannot make purged data visible.

## Consequences and open questions

The first implementation accepts grow-only purge state. Before claiming durable
deletion across cluster death, the project must decide:

- how purge intent is persisted across formations;
- when compaction is provably safe;
- how imported artifacts carry or discover deletion intent; and
- whether an external backend deletes canonical objects or suppresses their
  visibility.

These questions do not weaken the in-formation resurrection invariant.

## Related documents

- `docs/storage-spec.md`
- `docs/storage-model.md`
- `docs/protocol-p2p.md`
- `docs/user-stories/orishu/workload.md`
