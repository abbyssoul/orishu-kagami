# 0013 - Keep cluster formation and node membership identity distinct from labels

Status: **accepted**

## Context

Workers may start from identical configuration and therefore share a display
name. A standalone worker also needs to form a cluster of one without requiring
an operator-authored cluster resource. Human labels cannot safely establish
membership or artifact provenance.

## Decision

A cluster is one ephemeral **formation** with a generated `formationId`.
`clusterName` is a non-unique operator label. The client-facing cluster resource
is a synthetic projection of current formation state, not a durable
operator-authored manifest or CRUD-managed `cluster.yaml` object.

A worker name is likewise a non-unique operator label. Before admission, a
worker identifies itself for the join flow by its name and certificate
fingerprint. On successful admission, the formation assigns a unique node ID.
Membership, ownership, administrative targeting, tombstones, and producing-node
provenance use that node ID, never the name alone.

Leaving a formation drops the assigned node ID. Admission to another formation
assigns an identity in that formation. A node belongs to at most one formation
at a time.

Joiners adopt the target formation ID and label, but treat an introducer's
membership and workload snapshots as bootstrap input that must converge through
normal peer reconciliation. Every post-admission peer message is scoped to the
immutable formation ID. Operators must be able to inspect that ID instead of
trusting a reused cluster name.

## Consequences

- Multiple workers may have the same name; searches by name may return several
  nodes.
- A new formation does not inherit membership, locks, active workload state, or
  other transient control-plane state from an earlier formation.
- Durable artifacts may outlive their producing formation and record its ID and
  display name separately.
- No feature may introduce a durable authored cluster manifest without a new
  decision defining its authority and reconciliation semantics.
- Join replies, identity fields, and bootstrap snapshots are hostile input and
  require explicit size and consistency validation.

## Related documents

- `CONTEXT.md`
- `docs/protocol-client.md`
- `docs/protocol-p2p.md`
- `docs/orishu-data-model.md`
