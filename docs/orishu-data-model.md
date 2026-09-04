# Orishu runtime data model

This document classifies Orishu runtime data by authority, durability, and
mutability. Wire shapes belong in the protocol documents; workload identity is
defined by ADR 0010; storage behavior belongs in `storage-spec.md`.

## Classification

| Entity | Authority and lifetime | Mutability |
| --- | --- | --- |
| Workload manifest and closure | Submitted input; retained for a workload epoch | Immutable; replacement creates new identity/epoch |
| Synthetic cluster resource | Derived formation projection; transient | Derived |
| Node record | Cluster membership state; transient to formation | Versioned/merged |
| Workload status | Cluster-owned run state; transient and epoch-scoped | Versioned/merged |
| Membership lock and blocklist | Operator intent; transient to formation | Versioned CRDT state |
| Membership tombstone | Removed-node barrier; transient to formation | Grow-only until explicit clear |
| Partition map and ownership | Cluster authority; workload/epoch/version scoped | Fenced transitions |
| Checkpoint artifact | Durable resumable output | Immutable once committed |
| Result artifact | Durable analytical/observation output | Immutable once committed |
| Local inventory | Node-local durable routing claim | Advisory and mutable |
| Availability view and catalog | Runtime projections | Derived |
| Purge tombstone | Artifact-deletion intent | Grow-only initially |
| Logs, events, diagnostic provenance | Node-local/best-effort trace | Append-only, non-authoritative |

## Identity rules

ADR 0013 defines formation, node, and label identity. A run is identified by
formation, workload identity, and workload epoch; there is no schedulable
collection of independent simulations inside one cluster. `RunReference` is a
shareable reference to this identity, not mutable run authority.

Result sequences are derived client/operator groupings of immutable result
artifacts across stop and resume of the same lineage. They are not stored
resources. Resetting to a different checkpoint, unloading, or replacing a
workload ends that grouping.

## Artifact relationships

Checkpoint and initial-condition state use a compatible underlying state shape
where the declared workload profile permits it. Compatibility includes model,
schema, lifecycle, precision, dimensions, discretization, integration state,
and execution-profile constraints. A checkpoint is never accepted merely
because its bytes can be decoded.

Artifact records describe content and committed provenance. Chunks contain the
partition-aligned bytes. Local inventory claims possession. The catalog indexes
known artifact records. The availability view estimates current retrieval.
Purge tombstones prevent deliberate deletion from being undone by stale
inventory. These are separate entities and must not be collapsed.

## Authority constraints

- A client, peer, inventory entry, cache, filename, URL, and current ring owner
  cannot redefine immutable identity.
- Location and compression are distribution details.
- A partial or invalid transfer never replaces accepted state.
- Runtime status never participates in the canonical workload digest.
- Presentation observations are not checkpoints or simulation inputs without
  an explicit validated authority transition.
