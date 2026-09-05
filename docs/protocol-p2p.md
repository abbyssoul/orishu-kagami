# orishu Peer-to-Peer Protocol

This document defines the protocol for communication between `orishu-worker` instances (peers) within a cluster. It covers transport, framing, authentication, message schemas, gossip mechanics, and configuration. An implementer should be able to build a compatible peer node from this document alone, referring to the [Orishu runtime design](./orishu-runtime-design.md) for the conceptual model and [architecture.md](./architecture.md) for product-wide authority boundaries.

For the client-facing protocol (operators and user tools), see [protocol-client.md](./protocol-client.md).

## Reference implementation

The membership decisions this document specifies — admission, join adoption,
SWIM probing and suspicion, gossip merge, and anti-entropy — are implemented as
a deterministic, sans-IO core in `crates/orishu-membership`. That crate owns no
transport: framing, CBOR, QUIC, mTLS, timers, and entropy remain the IO shell's
responsibility, and the crate carries no dependency that could provide them.

Golden JSON fixtures for the wire-visible membership types live in
`crates/orishu-membership/tests/fixtures/`. They pin field names, casing,
nesting, and omission rules, and they are what a change to those shapes has to
update alongside this document. JSON rather than CBOR because the field
contract is identical in both; the encodings differ only in that hashes and
fingerprints are hex strings in JSON and byte strings in CBOR.

`crates/orishu-membership/src/lib.rs` also records the allocation bounds that
remain **mandatory in the decoder**. The core re-validates every logical limit
before adopting state, but by then a collection is already allocated: bounding
allocation while decoding cannot be delegated to it.


## Transport

The peer protocol uses raw QUIC ([RFC 9000](https://www.rfc-editor.org/rfc/rfc9000)) without the HTTP/3 framing layer. This eliminates header compression and HTTP semantics overhead for high-frequency protocol messages (SWIM probes, gossip, halo exchanges, step coordination).

Two transport modes are used within QUIC:

- **QUIC bidirectional streams** — for request/response exchanges and sustained data flows. One stream per independent operation (join handshake, gossip exchange, halo transfer, checkpoint fetch). Long-lived streams are used for sustained flows such as halo exchanges during simulation stepping. QUIC streams provide reliability and flow control natively.
- **QUIC datagrams** (unreliable) — for fire-and-forget messages: SWIM `Ping`/`Ack` probes, indirect probes, and `Announce` dissemination. No delivery guarantee, no stream setup overhead. Loss is handled at the protocol level through the SWIM suspicion mechanism.

Transport follows message semantics, not message size alone. Halo data, step
votes and commits, partition transfer, workload artifacts, checkpoints, and
other inputs to authoritative execution remain on reliable streams with
identity, validation, and idempotent retry where applicable; a later message
does not make an omitted prerequisite safe. Datagrams are admitted only when
loss has a defined protocol response, as it does for SWIM retry and suspicion.
Observation snapshot/delta coalescing belongs exclusively to the client-facing
projection path and never weakens these peer correctness requirements. See
[ADR 0011](./adr/0011-classify-network-flows-and-baseline-observation-deltas.md).

### Transport mapping

| Message | Transport | Direction | Notes |
|---|---|---|---|
| `Handshake` | Stream (bidi) | Initiator → Responder | Identity exchange. One stream, once per QUIC connection. |
| `HandshakeAck` | Stream (bidi) | Responder → Initiator | Reply on same stream as `Handshake`. |
| `JoinReq` | Stream (bidi) | Joiner → Introducer | Admission request. One stream per attempt. |
| `JoinReply` | Stream (bidi) | Introducer → Joiner | Reply on same stream as `JoinReq`. |
| `Ping` | Datagram | Unidirectional | Direct probe. Carries piggybacked gossip. |
| `Ack` | Datagram | Unidirectional | Probe response. Carries piggybacked gossip. |
| `PingReq` | Datagram | Unidirectional | Indirect probe request. |
| `PingReply` | Datagram | Unidirectional | Indirect probe response. |
| `Announce` | Datagram | Unidirectional | Protocol announcements (Suspect/Alive/Dead/Leave). |
| `PullReq` | Stream (bidi) | Initiator → Peer | Anti-entropy digest exchange. |
| `PullReply` | Stream (bidi) | Peer → Initiator | State transfer on same stream as `PullReq`. |
| `PartitionIntent` | Stream (bidi) | Proposer → Target | Ownership transfer proposal. |
| `PartitionAck` | Stream (bidi) | Target → Proposer | Reply on same stream as `PartitionIntent`. |
| `HaloDelta` | Stream (long-lived) | Bidirectional | Boundary data exchange. One stream per neighbor pair per simulation run. |
| `StepVote` | Stream (bidi) | Voter → Peers | Step completion signal. |
| `StepCommit` | Stream (bidi) | Peer → Peers | Step advancement confirmation. |
| `CheckpointReq` | Stream (bidi) | Requester → Owner | Checkpoint/catch-up fetch. |
| `CheckpointReply` | Stream (bidi) | Owner → Requester | Checkpoint data transfer on same stream. |
| `FetchChunkReq` | Stream (bidi) | Requester → Holder | Artifact chunk fetch by `ChunkRef`. |
| `FetchChunkReply` | Stream (bidi) | Holder → Requester | Verified block stream on same stream as `FetchChunkReq`. |


## Framing and serialization

### Stream framing

Messages sent over QUIC streams use length-prefixed CBOR dataframes:

```
[4 bytes: big-endian u32 payload length][payload: CBOR-encoded MessageEnvelope]
```

Maximum frame size: 16 MiB (configurable via `peer.maxFrameSize`). Dataframes exceeding the maximum are rejected and the stream is reset with `FRAME_TOO_LARGE`.

### Datagram framing

Each QUIC datagram carries a single atomic message. The entire datagram payload is a CBOR-encoded `MessageEnvelope`. No length prefix is needed — QUIC datagrams are self-delimiting.

Maximum datagram size is constrained by the QUIC path MTU (typically ~1200 bytes after QUIC overhead). Messages that exceed the datagram MTU must use streams instead.

### Serialization

All payloads use CBOR ([RFC 8949](https://www.rfc-editor.org/rfc/rfc8949)) via `ciborium`, the same serialization format as the [client protocol](./protocol-client.md#payload-format). Map keys are text strings. Fields with null or default values may be omitted to reduce payload size. Binary fields (hashes, fingerprints) are CBOR byte strings (major type 2).


## MessageEnvelope

All peer protocol messages are wrapped in a `MessageEnvelope` — a discriminated union that identifies the message type and carries the payload plus optional piggybacked gossip.

```
{
  "proto":     <uint>,                -- Protocol version. Current version: 1
  "type":      <string>,              -- Message type discriminator (see message catalog below)
  "senderId":  <string>,              -- Node ID of the sender. For pre-admission messages (Handshake, JoinReq from a new node), this is the node's name since no cluster ID has been assigned yet.
  "formationId": <string>,            -- Immutable cluster formation ID (`metadata.id`). Serves as a strict security boundary; receiver rejects if mismatch.
  "seq":       <uint64>,              -- Monotonic per-sender sequence number
  "payload":   <map>,                 -- Type-specific payload (defined per message)
  "gossip":    [<GossipDelta>, ...]   -- Optional piggybacked gossip deltas (may be empty)
}
```

**`type` values:** `"Handshake"`, `"HandshakeAck"`, `"JoinReq"`, `"JoinReply"`, `"Ping"`, `"Ack"`, `"PingReq"`, `"PingReply"`, `"Announce"`, `"PullReq"`, `"PullReply"`, `"PartitionIntent"`, `"PartitionAck"`, `"HaloDelta"`, `"StepVote"`, `"StepCommit"`, `"CheckpointReq"`, `"CheckpointReply"`, `"FetchChunkReq"`, `"FetchChunkReply"`.

**`formationId` guard:** A node receiving a message with a `formationId` that does not match its own immutable `metadata.id` must silently discard the message. This acts as a strict security boundary preventing cross-cluster interference. The legacy human-readable `clusterName` is no longer used as a wire guard.

**`seq` deduplication:** The `seq` field provides duplicate detection and ordering within a single sender. Receivers may discard messages with a `seq` less than or equal to the last processed `seq` for that sender.


## Authentication

All peer connections use mutual TLS (mTLS). Both sides present certificates during the QUIC handshake. QUIC provides TLS 1.3 encryption natively.

### Certificate verification on connection

1. The QUIC handshake completes with mTLS — both peers present certificates.
2. The receiving node extracts the SHA-256 fingerprint from the peer's TLS certificate.
3. If the fingerprint matches a known `NodeRecord.certFingerprint` for a node in `Alive` or `Suspected` state, the connection is accepted as a reconnection from that known peer.
4. If the fingerprint is unknown, the connection is accepted provisionally. The peer must initiate a `JoinReq` within the handshake grace period (default 5 seconds, configurable via `peer.handshakeTimeout`). If no `JoinReq` arrives, the connection is closed.
5. If the fingerprint matches a `Removed` or blocklisted node, the connection is rejected immediately.

### Join token validation

The join token is presented inside the `JoinReq` message payload, not at the TLS layer. The introducer verifies the token after the mTLS handshake succeeds. This separation allows the transport to be established before the admission decision.

### Certificate rotation

A node may rotate its certificate by sending a signed rotation request over an established connection. The request contains the new certificate fingerprint and is signed by the old key. Upon verification, the cluster updates `NodeRecord.certFingerprint` and propagates the change via gossip.


## Connection lifecycle

### Establishment

1. Initiator opens a QUIC connection to the target's `listen.peers` address with mTLS.
2. QUIC handshake completes — both sides present and verify certificates.
3. The initiator opens a bidirectional stream and sends a `Handshake` message.
4. The responder replies with a `HandshakeAck` on the same stream.
5. If `HandshakeAck.accepted` is `false`, the initiator must close the connection.
6. After a successful handshake, both sides may open streams and send datagrams.

The `Handshake`/`HandshakeAck` exchange binds the QUIC connection to logical node identities. This is necessary because the TLS certificate alone does not carry the human-readable cluster name or protocol version. The handshake cluster name is an admission/isolation label, not durable cluster formation identity. For nodes that have already joined, the handshake also communicates the cluster-assigned node ID; for new nodes that have not yet been admitted, the `nodeId` field is null (see [Node identity](orishu-runtime-design.md#node-identity)).

```
Handshake payload:
{
  "nodeId":           <string | null>,   -- Cluster-assigned node ID, or null if the node has not yet joined a cluster
  "nodeName":         <string>,
  "clusterName":      <string>,
  "protocolVersion":  <uint>,            -- Current: 1
  "certFingerprint":  <bytes>,           -- SHA-256 of this node's certificate
  "host":             <string>,          -- Advertised peer address
  "capabilities":     <NodeCapabilities>,
  "softwareVersion":  <string>
}
```

```
HandshakeAck payload:
{
  "nodeId":           <string | null>,   -- Cluster-assigned node ID, or null if the node has not yet joined a cluster
  "nodeName":         <string>,
  "clusterName":      <string>,
  "protocolVersion":  <uint>,
  "certFingerprint":  <bytes>,
  "host":             <string>,
  "capabilities":     <NodeCapabilities>,
  "softwareVersion":  <string>,
  "accepted":         <bool>,            -- false if protocol version incompatible or cluster mismatch
  "reason":           <string | null>    -- Rejection reason if accepted=false
}
```

The handshake stream is closed after the `HandshakeAck` is sent.

### Joining a cluster (new node)

After a successful `Handshake`/`HandshakeAck` exchange:

1. The joining node opens a new bidirectional stream and sends `JoinReq`.
2. The introducer evaluates [admission criteria](./orishu-runtime-design.md#member-acceptance) (`accepts.peers`, `membershipLocked`, capacity, blocklist, token validity).
3. The introducer replies with `JoinReply` on the same stream.
4. On `ACK`: the introducer generates a unique node ID (see [Node identity](orishu-runtime-design.md#node-identity)), creates a `NodeRecord` keyed by this ID, and begins gossiping it. The joining node adopts the `assignedNodeId`, `formationId` and `clusterName` from the `JoinReply`.
5. On `NACK` or `Redirect`: the joining node may retry with another introducer after backoff.
6. The stream is closed after the `JoinReply`.

### Reconnection (known node)

After a successful `Handshake`/`HandshakeAck` exchange where both sides recognize each other's node ID and certificate fingerprint, the connection is established without a `JoinReq`. Both sides immediately begin gossip exchange and workload coordination.

### Graceful shutdown

1. The departing node sends an `Announce(Leave)` to all connected peers (piggybacked on the next outgoing message or as a standalone datagram).
2. If a simulation is running, the node completes its current step, writes a checkpoint, and transfers partition ownership.
3. The node closes all QUIC connections.

### Connection maintenance

Each QUIC connection is maintained as long as both peers are alive. QUIC's built-in keepalive (configurable via `peer.quicKeepAlive`, default 10s) detects connection loss at the transport layer. The SWIM probe loop operates at a higher layer and independently detects node liveness. The QUIC idle timeout (configurable via `peer.quicIdleTimeout`, default 30s) closes connections that have no traffic and no keepalive.


## Common types

### VersionTuple
```
{
  "epoch":   <uint>,
  "counter": <uint>,
  "actorId": <string>                    -- Node ID of the member that produced this version
}
```

One formation-scoped version type, shared by every replicated entity.

**Total ordering.** Versions compare lexicographically over
`(epoch, counter, actorId)`, with `actorId` compared as raw bytes. The order is
total: any two versions compare, and only identical triples compare equal.
Including `actorId` breaks ties between two members that independently reached
the same `(epoch, counter)`, which is what makes merge deterministic rather
than dependent on delivery order.

**Merge semantics.** For one entity key:

- A strictly greater incoming version replaces the held value.
- A strictly lesser incoming version is discarded as stale. It can never
  regress state.
- An equal version carrying a **byte-identical payload** is an idempotent
  replay and changes nothing — not even a hop counter or a dissemination
  deadline.
- An equal version carrying a **different payload** is a structured conflict,
  not a tie to break silently. The receiver keeps its local value and reports
  the conflict; choosing a winner by arrival order would make converged state
  depend on packet timing. Two nodes each holding one of the payloads both
  report it, so the condition is observable rather than absorbed.

These rules order the **version-owned projection** of an entity: the fields
whose writer is whoever last described it. An entity may carry a second
projection with a writer and an ordering of its own, and that projection is
merged separately. `NodeRecord` is the case in point: its `state` and
`incarnation` are ordered by the [override rules](#announce) — whatever carries
them — and are written by the failure detector, not by the describing member.
So an equal version carrying an equal description is **not** a conflict merely
because liveness differs, a liveness change does **not** advance the version,
and a receiver may adopt a newer liveness from a record whose description it
rejects as conflicting — reporting the conflict and re-disseminating the record
it now holds, never the one it refused.

A version from another formation is meaningless and must be rejected by the
`formationId` guard before any comparison.

Versions are scoped to a formation and are never carried across one. A node
that adopts a new formation starts from that formation's versions and imports
none of its own.

### NodeCapabilities
```
{
  "cpuCores":       <uint>,
  "memoryBytes":    <uint>,
  "architecture":   <string>,
  "storage":        <StorageType>,
  "accelerators":   [<string>, ...],
  "engines":        [<EngineCapability>, ...]   -- runtime engines this node can execute
}
```

`engines` advertises the workload runtime engines and lifecycle identifiers the node can actually execute, used for workload compatibility checks. A node advertises only engines whose complete security contract it implements. The admitted profile supports `wasm-component` with `orishu.workload/v1`; another engine requires a new architectural decision. See [protocol-workload.md](protocol-workload.md#runtime-engine).

### EngineCapability
```
{
  "engine":           <string>,   -- "wasm-component" in the admitted profile
  "runtimeLifecycle": <string>    -- e.g. "orishu.workload/v1"; opaque compatibility identifier
}
```

### StorageType
```
{
  "replicas":  <uint>,
  "backend":            <string>    -- "local" | "memory" | "external"
}
```

### GossipDelta
```
{
  "deltaType":  <string>,               -- "MembershipUpdate" | "TombstoneUpdate" | "BlocklistUpdate" |
                                         --   "WorkloadUpdate" | "CheckpointUpdate" | "ResultUpdate" |
                                         --   "AuditEvent"
  "key":        <string>,               -- Entity identifier (node ID, workload ID, etc.)
  "version":    <VersionTuple>,          -- Version of this delta
  "data":       <map>,                   -- Delta-type-specific payload (the complete entity record)
  "hops":       <uint>                   -- Times this delta has been piggybacked. Starts at 0.
}
```

`data` carries the **complete** record for the entity at that version, not a
field-level diff. A partial diff cannot be hashed canonically for anti-entropy
and cannot be merged idempotently, because two receivers holding different
prior states would reach different results from the same delta.

**Membership-owned delta types.** `MembershipUpdate` (`data` is a `NodeRecord`,
`key` is its node ID), `TombstoneUpdate` (`data` is a membership tombstone,
`key` is the removed node ID), and `BlocklistUpdate` (`data` is a blocklist
entry, `key` is its rendered blocklist key). These three converge through the
membership merge rules above.

**Every other delta type is not membership's.** A membership implementation
relays `WorkloadUpdate`, `CheckpointUpdate`, `ResultUpdate`, and `AuditEvent`
to their owning subsystem without decoding `data`, and must not store any part
of them in membership state. An unrecognized `deltaType` is relayed the same
way rather than rejected, so a subsystem can add one without a membership
change.

### MerkleDigest
```
{
  "rootHash":    <bytes>,                -- SHA-256 of the Merkle tree root
  "depth":       <uint>,                 -- Tree depth, 1..=8; bucket count is 2^depth
  "nodeHashes":  [<bytes>, ...]          -- Exactly 2^depth bucket hashes, ascending by index
}
```

A digest is only comparable if both sides compute identical bytes from
identical state, so every degree of freedom is fixed below. Changing any of it
is a wire-breaking change and requires bumping the domain-separator versions.

**Canonical value encoding.** Fields are written in a fixed order with every
variable-length field length-prefixed by a big-endian `u32`, and every integer
written big-endian. Length prefixes are what stop `("ab", "c")` and
`("a", "bc")` encoding identically.

**Leaf key.** A one-byte namespace tag followed by the entity identifier:

| Tag | Entity | Identifier |
|---|---|---|
| `0x01` | member record | node ID |
| `0x02` | membership tombstone | node ID |
| `0x03` | blocklist entry | rendered blocklist key |

The tag keeps the namespaces disjoint, so a member and a tombstone for the same
node cannot collide onto one leaf.

**Leaf hash.** `SHA-256("orishu.membership.leaf/1" || u32(len(key)) || key ||
canonical-value-encoding)`.

**Bucket assignment.** `SHA-256("orishu.membership.bucket/1" || key)`, of which
the top `depth` bits of the first two bytes give the bucket index. Hashing
rather than taking the key directly spreads sequentially named nodes evenly, so
one divergent entry lands in one bucket rather than smearing across all of them.

**Bucket hash.**
`SHA-256("orishu.membership.bucket/1" || u32(count) || leaf hashes in ascending
leaf-key order)`. Sorting is what makes the result independent of insertion
history. An empty bucket hashes its zero count and is not skipped.

**Tree.** A complete binary tree over exactly `2^depth` buckets, folded
pairwise as `SHA-256("orishu.membership.node/1" || left || right)` up to the
root. Because the bucket count is fixed by configuration there is no ambiguous
padding rule.

**Subtree addressing.** A bucket index in `0..2^depth`. An index outside that
range is skipped, not indexed.

**Comparison.** Two digests are comparable only when their `depth` values match
and the responder's digest is internally consistent — `nodeHashes` has the
length its `depth` implies, and `rootHash` folds from it. A digest failing
either check is refused rather than compared, and produces no reply.

### PartitionRef
```
{
  "partitionId":      <string>,
  "workloadEpoch":    <uint64>,
  "partitionVersion": <uint64>
}
```

### HaloFace
```
<string>                                 -- "+x" | "-x" | "+y" | "-y" | "+z" | "-z"
```

### StepProvenance
```
{
  "nodeId":             <string>,
  "nodeName":           <string>,
  "nodeVersion":        <string>,
  "workloadId":         <string>,
  "workloadEpoch":      <uint64>,
  "simulationCodeHash": <bytes>,         -- SHA-256 of the executed WASM artifact
  "partitionId":        <string>,
  "stepRange":          [<uint64>, <uint64>],  -- [from, to] step numbers
  "timestamp":          <string>         -- RFC 3339, diagnostic only
}
```

### ChunkRef
```
{
  "artifactId":        <string>,         -- UUID; references CheckpointRecord.id or ResultRecord.id
  "artifactKind":      <string>,         -- "checkpoint" | "result"
  "partitionId":       <string>,
  "originFormationId": <string>,
  "contentHash":       <bytes>,          -- expected SHA-256 of the chunk (the integrity authority)
  "sizeBytes":         <uint64>
}
```

Storage-facing identity of a chunk; see [storage-spec.md](storage-spec.md#addressing-and-identity). Used by `FetchChunk` to request and verify artifact bytes.


## Cluster messages

### JoinReq / JoinReply

Sent over a dedicated bidirectional QUIC stream. The stream is closed after the reply.

```
JoinReq payload:
{
  "joinToken":        <string>,          -- Mandatory MVP join token
  "nodeName":         <string>,          -- Human-readable name; serves as primary identifier until cluster assigns an ID
  "certFingerprint":  <bytes>,           -- SHA-256 of the joining node's TLS certificate
  "softwareVersion":  <string>,
  "host":             <string>,          -- Advertised peer listen address
  "clientHost":       <string | null>,   -- Advertised client listen address (null if not accepting)
  "accepts": {
    "clients": <bool>,
    "peers":   <bool>,
    "work":    <bool>
  },
  "limits": {
    "clients": <uint>,
    "peers":   <uint>
  }
  "capabilities":     <NodeCapabilities>,
}
```

```
JoinReply payload:
{
  "result":          <string>,           -- "ACK" | "NACK" | "Redirect"
  "formationId":     <string | null>,    -- Immutable cluster formation ID (on ACK only)
  "clusterName":     <string | null>,    -- Human-readable cluster name (on ACK only)
  "assignedNodeId":  <string | null>,    -- Cluster-assigned node ID (present on ACK only, null otherwise)
  "reason":          <string | null>,    -- Human-readable reason for NACK
  "redirectTo":      [<string>, ...],    -- Addresses of alternative introducers (for Redirect)
  "membership":      [<NodeRecord>, ...],-- Current membership snapshot (on ACK only)
  "workload":        <Workload | null>,  -- Current WorkloadManifest, if any (on ACK only)
  "workloadStatus":  <map | null>        -- Current WorkloadStatus (on ACK only)
}
```

**Behavioral rules:**

- The introducer must evaluate ALL [admission criteria](./orishu-runtime-design.md#member-acceptance) before replying. The complete gate set, in cheapest-first order, is:

  1. **Authenticated transport** — the mTLS handshake completed.
  2. **Formation/join intent** — the request's `formationId` is this formation.
  3. **Protocol compatibility** — the offered `proto` is within this node's accepted range.
  4. **Introducer flag** — `accepts.peers == true` on this node.
  5. **Membership lock** — `membershipLocked == false`.
  6. **Blocklist, by identity** — neither the applicant's `nodeName` nor its `certFingerprint` matches a blocking entry.
  7. **Membership tombstone** — no uncleared tombstone pins the applicant's `certFingerprint`. An applicant has no assigned ID yet, so the fingerprint is what fences a removed node returning under a fresh label.
  8. **Capacity** — `limits.peers` is not exceeded.
  9. **Request bounds** — every advertised address, label, and capability list is within limits.
  10. **Join token** — the presented token matches the formation's current one.
  11. **Blocklist, by network** — the applicant's source address does not fall in a blocked CIDR range.

  Gates 1–9 are decidable from replicated state alone and must be evaluated first, so an unauthenticated or blocklisted flood is refused without ever costing a token comparison. Gates 10–11 need cryptographic comparison and address parsing.

- Gates 1–9 must be **re-evaluated** after gates 10–11 complete. Token verification is not instantaneous, and membership may have locked or filled while it ran; an applicant must not slip through on a decision made before the lock.
- On `ACK`, the introducer generates a unique node ID for the joining node (see [Node identity](orishu-runtime-design.md#node-identity)), rejects the attempt if that ID collides with an existing member or any tombstone, creates a `NodeRecord` keyed by it, inserts that record into membership, and only **then** replies and begins gossiping. Acceptance is never reported before insertion succeeds, or a collision would produce an admitted node no one holds a record for. The `assignedNodeId` field contains the newly generated ID. The `membership` field contains the current membership, including the joiner's own new record, so the joining node can bootstrap its cluster view.
- A joiner validates the `ACK` before adopting it: the `formationId` matches the formation it asked to join, `membership` is within the snapshot item and byte limits, node IDs within it are unique, every entry passes bounds validation and protocol compatibility, and the entry for `assignedNodeId` is *this node* — same certificate fingerprint and same label. Without the self-entry check, an introducer could admit one node and hand its identity to another.
- Adopting a formation is atomic and imports nothing: the joiner drops its previous formation's members, tombstones, blocklist, locks, probe state, and versions rather than merging them.
- On `Redirect`, the `redirectTo` field contains addresses of other introducers believed to have capacity. Only nodes with `accepts.peers == true` are included.
- On `NACK`, the joining node should back off before retrying. Recommended: exponential backoff starting at 1 second, capped at 60 seconds.

---

### Ping / Ack

Sent as QUIC datagrams (unreliable). The primary mechanism for SWIM failure detection and gossip dissemination.

```
Ping payload:
{
  "probeId":     <uint64>,               -- Sender-scoped probe correlation ID
  "incarnation": <uint>                  -- Sender's current incarnation number
}
```

```
Ack payload:
{
  "probeId":     <uint64>,               -- Copied verbatim from the Ping being answered
  "incarnation": <uint>                  -- Responder's current incarnation number
}
```

**Behavioral rules:**

- The `gossip` field of the enclosing `MessageEnvelope` carries piggybacked deltas. This is the primary dissemination path for membership updates, CRDT state changes, and protocol announcements.
- If a `Ping` arrives from an unknown sender (no matching `NodeRecord`), it is silently discarded.
- The `incarnation` field allows the receiver to update its view of the sender's incarnation number.
- **Probe correlation.** `probeId` is unique within the sending node and is echoed verbatim in the `Ack`. A probe is identified by the pair `(senderId, probeId)`; the target ID alone is not sufficient, because datagrams may be duplicated or reordered and several probes of the same target may overlap after a retry.
- A requester accepts an `Ack` only when it matches an in-flight probe *and* the envelope `senderId` equals that probe's target. An `Ack` naming a live `probeId` from any other node is discarded: without that check, any member could keep a failing node alive by answering probes addressed to it.
- A second `Ack` for a probe already completed is discarded. `probeId` values are never reused within a node's lifetime in one formation.
- An intermediary serving a `PingReq` allocates its **own** `probeId` for the `Ping` it sends to the target, and maps the target's `Ack` back to the requester's `probeId`. The two ID spaces are per-node and never shared.

---

### PingReq / PingReply

Sent as QUIC datagrams. Used for indirect probing when a direct `Ping` times out.

```
PingReq payload:
{
  "probeId":  <uint64>,                  -- Requester-scoped probe correlation ID
  "targetId": <string>                   -- Node ID of the target to probe
}
```

```
PingReply payload:
{
  "probeId":     <uint64>,               -- Copied verbatim from the PingReq
  "targetId":    <string>,               -- The target that was probed
  "result":      <string>,               -- "Ack" | "NoSuchPeer" | "Timeout"
  "incarnation": <uint | null>           -- Target's incarnation if Ack received
}
```

**Behavioral rules:**

- The intermediary that receives `PingReq` sends a `Ping` to the target and waits for an `Ack`. It then replies with `PingReply` to the requester.
- If the intermediary has no connection to the target, it replies with `"NoSuchPeer"`.
- The intermediary uses the same probe timeout as its own SWIM configuration when waiting for the target's `Ack`.
- **Probe correlation.** `probeId` is the *requester's* ID for the probe that timed out directly, shared with every intermediary asked about the same target, and echoed verbatim in each `PingReply`.
- A requester accepts a `PingReply` only when all three hold: the `probeId` names a probe currently in its indirect phase, the `targetId` matches that probe's target, and the envelope `senderId` is one of the intermediaries it actually asked. A reply failing any of these is discarded.
- Repeated non-`Ack` replies from the same intermediary count once. Suspicion is raised only when *every* asked intermediary has reported a non-`Ack` result, or when the indirect timeout expires.
- Timer expiries are correlated the same way: an implementation must record, per probe, the generation of the one timer whose expiry is still actionable, and discard an expiry from a superseded generation. Without this, a timer armed for a probe that has since been answered will suspect a healthy member.

---

### Announce

Protocol announcements disseminated epidemically via piggybacking on other messages. May also be sent as standalone datagrams for urgent dissemination.

```
Announce payload:
{
  "announcement": <string>,             -- "Suspect" | "Alive" | "Dead" | "Leave"
  "targetId":     <string>,             -- Node ID this announcement is about
  "incarnation":  <uint>                -- Incarnation number associated with the announcement
}
```

**Behavioral rules:**

- Typically piggybacked on `Ping`/`Ack` messages via the `gossip` field (as a `GossipDelta` with `deltaType: "MembershipUpdate"`) rather than sent as standalone `Announce` messages.
- May also be sent as standalone datagrams for urgent dissemination (e.g., a node broadcasting `Alive` to refute suspicion).
- **Override rules** (higher incarnation wins):
  - `Alive(n)` overrides `Alive(m)` or `Suspect(m)` if `n > m`. A refutation must
    carry a *strictly* newer incarnation, or a replayed old `Alive` would clear
    a fresh suspicion.
  - `Suspect(n)` overrides `Alive(m)` if `n >= m`, and overrides `Suspect(m)` if
    `n > m`. The `>=` is load-bearing: a probe times out against the target's
    *current* incarnation, so requiring `n > m` would make suspicion
    unreachable.
  - `Dead(n)` overrides `Suspect(m)` for any `n >= m`.
  - `Dead(n)` overrides `Alive(m)` for any `n >= m`.
  - Nothing overrides `Dead`. Within one formation, death is terminal:
    readmission means admission to a new formation-assigned identity, not a
    resurrected record.
  - `Leave` is authoritative only when `targetId == senderId` (self-announcement).
- A node that detects it has been suspected may refute by broadcasting `Alive` with `incarnation + 1`. The increment is checked: a node whose incarnation is exhausted reports the condition rather than wrapping, because an incarnation of `0` would rank below every stale `Suspect` still in flight and leave the node permanently unable to defend itself.
- `Leave` may only be announced by the node itself. Any `Leave` where `targetId != senderId` is discarded — otherwise any member would hold a one-datagram eviction primitive.
- A self-announced `Leave` takes effect as `Dead(incarnation + 1)`, so it outranks the departing node's own latest `Alive` and cannot be undone by a replay of one.

**Removal is not liveness.** `Alive`, `Suspect`, and `Dead` are failure-detector
state. Removing a node from a formation is an operator or policy action,
recorded as a membership tombstone (`TombstoneUpdate`), and the two must not be
conflated:

- A membership tombstone fences the removed node's assigned ID **and** its
  pinned certificate fingerprint. No `Announce`, at any incarnation, clears one.
- A voluntary self-`Leave` does **not** create a tombstone. Leaving is the
  node's own decision to stop participating, not the formation barring it.
- The failure detector never writes a tombstone. Declaring a node `Dead` on a
  timeout must not make an unreachable-but-healthy node permanently unwelcome.
- Clearing a tombstone is an explicit operator action expressed as a *versioned
  update* — the record is retained with `cleared: true` at a newer version, not
  deleted. A deletion is not itself a versioned fact, so a peer that had not yet
  heard about the clear would re-gossip the tombstone and re-fence the node
  forever.


## State management messages

### PullReq / PullReply

Anti-entropy state synchronization. Sent over a dedicated bidirectional QUIC stream.

```
PullReq payload:
{
  "round":      <uint64>,                -- Initiator-scoped round ID, echoed in the reply
  "digest":     <MerkleDigest>,          -- Requester's Merkle tree digest of CRDT state
  "stateTypes": [<string>, ...],         -- State types to sync: "membership", "blocklist",
                                         --   "workload", "checkpoints", "results"
  "subtreeReq": [<uint>, ...],           -- Specific bucket indices to request (after initial
                                         --   digest comparison). Empty = compare digests.
  "cursor":     <Cursor | null>          -- Where to resume a truncated exchange
}
```

```
PullReply payload:
{
  "round":     <uint64>,                 -- Copied verbatim from the PullReq
  "digest":    <MerkleDigest>,           -- Responder's own Merkle tree digest
  "deltas":    [<GossipDelta>, ...],     -- State entries that differ between the two digests
  "complete":  <bool>,                   -- true if all divergent entries included; false if
                                         --   truncated (continue from cursor)
  "cursor":    <Cursor | null>           -- Where the initiator should resume when complete=false
}
```

```
Cursor:
{
  "bucket":   <uint>,                    -- Bucket to resume in
  "afterKey": <bytes>                    -- Resume strictly after this canonical leaf key
}
```

**Behavioral rules:**

- The anti-entropy exchange proceeds in rounds:
  1. Initiator sends `PullReq` with a fresh `round` and its Merkle digest, and arms a deadline.
  2. Responder compares digests, identifies divergent buckets, and replies with the differing entries, echoing `round`.
  3. If `complete` is `false`, the initiator sends a follow-up `PullReq` with the same `round` and the returned `cursor`.
- **Round correlation.** `round` is unique within the initiating node. A reply whose `round` or `senderId` does not match the round in progress is discarded; without this, a late reply from an abandoned round would be merged into a newer one.
- **Traversal order is canonical**: ascending bucket index, then ascending leaf key within each bucket. Both sides therefore agree on what "resume after this key" means without exchanging any additional state.
- **Truncation is not convergence.** A reply with `complete: false` means the responder stopped at `swim.antiEntropyMaxEntries`; the initiator must continue from `cursor` and must not conclude that the two nodes agree.
- **Rounds are bounded.** An exchange is abandoned, with a diagnostic, after `swim.antiEntropyRounds` continuations, so a peer that always answers `complete: false` cannot hold a round open indefinitely.
- Both sides merge received deltas into their local CRDT state, through the same merge rules as piggybacked gossip — a delta arriving by anti-entropy has no special standing.
- The stream is closed after the exchange completes.
- Maximum entries per reply is bounded by `swim.antiEntropyMaxEntries` (default 1000).


## Workload coordination messages

### PartitionIntent / PartitionAck

Ownership transfer proposals for simulation space partitions. Sent over a dedicated bidirectional stream. See [partition ownership](./orishu-runtime-design.md#partition-ownership-and-rebalancing) in the runtime design.

```
PartitionIntent payload:
{
  "workloadEpoch":    <uint64>,
  "partitionId":      <string>,
  "currentOwner":     <string>,          -- Node ID of current partition owner
  "proposedOwner":    <string>,          -- Node ID of proposed new owner
  "partitionVersion": <uint64>,          -- Monotonic version of partition ownership
  "transferBoundary": {
    "type":         <string>,            -- "StepCommit" | "CheckpointBarrier" | "LoadComplete"
    "step":         <uint64 | null>,     -- Step number at which transfer becomes valid
    "checkpointId": <string | null>      -- Checkpoint from which new owner should restore
  },
  "signature":        <bytes>            -- Proposer's signature over the intent fields
}
```

```
PartitionAck payload:
{
  "workloadEpoch":    <uint64>,
  "partitionId":      <string>,
  "partitionVersion": <uint64>,
  "accepted":         <bool>,
  "reason":           <string | null>,   -- Reason for rejection
  "signature":        <bytes>            -- Responder's signature over the ack fields
}
```

**Behavioral rules:**

- A transfer becomes active only after BOTH old and new owner acknowledge the same intent, OR after the cluster declares the old owner `Dead` and a replacement claim is committed.
- Transfers happen only at safe boundaries: after step commit, at checkpoint barrier, or after load completion. Mid-step transfers are forbidden.
- The `partitionVersion` must be strictly greater than the last known version for this partition. Stale versions are rejected.
- Both `PartitionIntent` and `PartitionAck` carry signatures to prevent spoofed ownership claims.
- The stream is closed after the ack.

---

### HaloDelta

Boundary data exchange between partition neighbors. Sent over long-lived bidirectional QUIC streams — one stream per partition-neighbor pair, maintained for the duration of the simulation run.

```
HaloDelta payload:
{
  "workloadEpoch":   <uint64>,
  "step":            <uint64>,           -- Step number this halo data is for
  "sourcePartition": <string>,           -- Partition ID that produced this halo data
  "targetPartition": <string>,           -- Partition ID that needs this halo data
  "face":            <HaloFace>,         -- Which face of the target partition this covers
  "haloDepth":       <uint>,             -- Number of cell layers in this halo
  "data":            <bytes>             -- Serialized boundary cell values
}
```

**Behavioral rules:**

- The `(workloadEpoch, step, sourcePartition, targetPartition, face)` tuple uniquely identifies a halo exchange.
- Both directions of a partition boundary use the same bidirectional stream.
- If the stream is reset or the connection is lost, the receiver cannot compute the next step for the affected partition. The SWIM failure detector will eventually mark the peer as failed.
- Halo data must be received for ALL faces before a partition can compute its next step.
- Workload messages also carry piggybacked gossip in the `MessageEnvelope.gossip` field. If a peer accepted a workload message, it was alive at that instant, making a separate `Ping` unnecessary.

---

### StepVote / StepCommit

Step barrier coordination. Sent over bidirectional QUIC streams.

```
StepVote payload:
{
  "workloadEpoch": <uint64>,
  "step":          <uint64>,             -- Step number that this node has completed
  "partitions":    [<string>, ...],      -- Partition IDs this vote covers
  "stateHashes":   {                     -- Per-partition state hash after completing this step
    <partitionId>: <bytes>,              --   SHA-256 of the partition state
    ...
  }
}
```

```
StepCommit payload:
{
  "workloadEpoch": <uint64>,
  "step":          <uint64>,             -- Step number being committed
  "committed":     <bool>,               -- true = step committed; false = must retry
  "reason":        <string | null>       -- Reason for retry if committed=false
}
```

**Behavioral rules:**

- A node emits `StepVote` after it has:
  1. Received all required halo data for step `n` on all owned partitions.
  2. Computed step `n` on all owned partitions.
  3. Sent its outgoing halo data for step `n` to all neighbors.
- `StepCommit` is disseminated after every partition owner for step `n` has emitted a valid `StepVote`. This confirmation is epidemic — each node that has received all votes for step `n` may emit `StepCommit` to its peers.
- If a partition owner disappears before emitting `StepVote`, the step cannot commit. The cluster retries from the last committed step using the most recent checkpoint or replicated partition state.
- `stateHashes` enable verification that all nodes agree on the simulation state after a step (consistency check).
- For speculative execution: if multiple nodes compute the same partition, the first valid `StepVote` for that `(epoch, step, partitionId)` wins. See [protocol-workload.md](./protocol-workload.md) for speculative execution semantics.

---

### CheckpointReq / CheckpointReply

Checkpoint and catch-up state transfer. Sent over a dedicated bidirectional stream.

```
CheckpointReq payload:
{
  "workloadEpoch": <uint64>,
  "partitionId":   <string>,
  "checkpointId":  <string | null>,      -- Specific checkpoint to fetch (null = latest)
  "step":          <uint64 | null>       -- Specific step boundary (null = latest available)
}
```

```
CheckpointReply payload:
{
  "workloadEpoch":   <uint64>,
  "partitionId":     <string>,
  "checkpointId":    <string>,
  "step":            <uint64>,           -- Step number at which this checkpoint was taken
  "simulationTime":  <float>,
  "partitionMap":    <map>,              -- Partition ownership at checkpoint time
  "data":            <bytes>,            -- Serialized checkpoint payload
  "contentHash":     <bytes>,            -- SHA-256 of the data field
  "provenance":      [<StepProvenance>, ...]  -- Provenance chain up to this checkpoint
}
```

**Behavioral rules:**

- Used by late-joining nodes to catch up with the current simulation state before they can own future steps.
- Used during partition reassignment when a new owner needs the current state.
- The stream supports flow control — large checkpoints may span multiple QUIC dataframes.
- The receiver must verify `contentHash` against the received `data` before accepting the checkpoint.
- The stream is closed after the reply.

> **CheckpointReq/Reply vs FetchChunk.** `CheckpointReq`/`CheckpointReply` carries *live, in-progress* partition state during an active run (late-joiner catch-up, reassignment). Retrieval of a **committed, immutable artifact** (checkpoint or result) from storage uses `FetchChunk` (below). Committed-artifact bytes are never moved over a swarm/torrent transport — see [ADR 0015](./adr/0015-use-quic-native-artifact-transfer.md).


## Artifact transfer

Bulk transfer of committed artifact chunks (checkpoint and result artifacts) is **QUIC-native**: a node fetches a chunk by `ChunkRef` over the existing authenticated peer streams and verifies it end-to-end against the chunk's SHA-256 `contentHash`. There is no separate transport, swarm, or new wire format ([ADR 0015](./adr/0015-use-quic-native-artifact-transfer.md)).

### FetchChunkReq / FetchChunkReply

Sent over a dedicated bidirectional QUIC stream. The reply streams fixed-size blocks of one chunk.

```
FetchChunkReq payload:
{
  "chunk":   <ChunkRef>,                  -- which chunk to fetch (carries expected contentHash + size)
  "offset":  <uint64>                     -- resume from this byte offset (0 = from start)
}
```

```
FetchChunkReply payload (framed as a sequence of blocks on the stream):
{
  "blockOffset": <uint64>,                -- byte offset of this block within the chunk
  "block":       <bytes>,                 -- a fixed-size block (last block may be shorter)
  "merklePath":  [<bytes>, ...]           -- optional: sibling hashes to verify this block incrementally
}
```

**Behavioral rules:**

- The requester resolves candidate holders for the chunk from the **availability view** (alive nodes whose advertised inventory claims the chunk) and may fan out across multiple holders in parallel; a single large chunk's blocks may be striped across holders.
- Holder claims are advisory. Bytes are accepted only after they verify against the `ChunkRef.contentHash` (SHA-256), incrementally via the per-chunk `MerkleDigest` where available. A holder that returns bytes failing verification loses holder confidence and may trigger repair — exactly as for any other unverified source.
- Transfer is resumable: on stream reset the requester re-issues `FetchChunkReq` with `offset` set past the last verified byte.
- A node that successfully fetches and verifies a chunk becomes an eligible holder for it (opportunistic re-seed under the cluster's placement policy), so the holder set grows under load.
- The transport does not own placement: which nodes are *required* to hold a chunk is decided by cluster-managed placement (ring + vnodes + replication factor), not by demand.
- The `FetchChunk` parser is bounded and validated like every other peer message; block sizes are bounded. No new wire format is introduced beyond the existing CBOR framing.

### Artifact catalog discovery

Search/filter for artifacts (by `workloadId` / `workloadEpoch` / `artifactKind`) resolves through the **artifact catalog**, a node-local index every node builds by merging gossiped `CheckpointUpdate` / `ResultUpdate` deltas (see [GossipDelta](#gossipdelta)). For these delta types the `data` map carries `artifactId`, `artifactKind`, `workloadId`, `workloadEpoch`, and `originFormationId`. The catalog is authoritative for discovery; there is no in-band/swarm discovery channel.


## Gossip mechanics

### SWIM probe cycle

The following probe cycle runs on every node at a configurable interval:

```
every PROBE_INTERVAL:
    1. Select one random peer from the alive membership list.
    2. Allocate a fresh probeId; send Ping(probeId) to the peer (as datagram).
    3. Arm PROBE_TIMEOUT, recording its generation as the probe's only
       actionable expiry.
    4. If Ack(probeId) arrives from that exact target:
         - Peer is alive. Update membership view. Cancel the timer.
         - Process any piggybacked gossip in the Ack.
    5. If PROBE_TIMEOUT expires (with the recorded generation):
         - Re-arm as INDIRECT_PROBE_TIMEOUT, recording a new generation, so a
           late direct expiry can no longer act on this probe.
         - Select k random peers (k = INDIRECT_PROBE_COUNT).
         - Send PingReq(probeId, target) to each of the k peers (as datagrams).
    6. If any PingReply(probeId, target, Ack) arrives from an asked intermediary:
         - Peer is alive. Cancel suspicion.
    7. If every asked intermediary reports a non-Ack result, or
       INDIRECT_PROBE_TIMEOUT expires with the recorded generation:
         - Emit Announce(Suspect, target, target's current incarnation).
         - Start SUSPICION_TIMEOUT timer, recording its generation.
    8. If SUSPICION_TIMEOUT expires with the recorded generation and without an
       Alive refutation:
         - Emit Announce(Dead, target, target's incarnation).
         - Update membership: target.memberState = Dead. No tombstone is written.

Every expiry above is checked against the generation recorded for that probe or
suspicion. An expiry from a superseded generation is discarded: without that
check, a timer armed for a probe that has since been answered would suspect a
healthy member.
```

This produces O(1) messages per node per period — O(n) total across the cluster — regardless of cluster size. Membership updates are disseminated infection-style by piggybacking on probe messages and workload traffic, reaching all nodes in O(log n) rounds with high probability.

### SWIM protocol parameters

| Parameter | Config key | Default | Description |
|---|---|---|---|
| `PROBE_INTERVAL` | `swim.probeInterval` | `1s` | Time between probe cycles. |
| `PROBE_TIMEOUT` | `swim.probeTimeout` | `500ms` | Time to wait for a direct Ping Ack. |
| `INDIRECT_PROBE_COUNT` | `swim.indirectProbeCount` | `3` | Number of intermediaries for indirect probing. |
| `INDIRECT_PROBE_TIMEOUT` | `swim.indirectProbeTimeout` | `1s` | Time to wait for indirect probe results. |
| `SUSPICION_TIMEOUT` | `swim.suspicionTimeout` | `5s` | Base suspicion timeout (scales with cluster size). |
| `SUSPICION_MULT` | `swim.suspicionMultiplier` | `3` | Suspicion timeout multiplier. |
| `MAX_GOSSIP_PER_MESSAGE` | `swim.maxGossipPerMessage` | `10` | Maximum GossipDelta entries piggybacked per message. |
| `MAX_GOSSIP_HOPS` | `swim.maxGossipHops` | `0` (auto) | Maximum hops before delta retirement. `0` = auto: `ceil(3 * log2(N))`. |

**Suspicion timeout scaling:** The effective suspicion timeout scales logarithmically with cluster size: `swim.suspicionTimeout * ceil(log2(N + 1)) * swim.suspicionMultiplier`, where `N` is the current number of alive members. This prevents false positives in large clusters where gossip propagation takes more rounds.

### Piggybacking mechanics

- Each node maintains a queue of pending `GossipDelta` entries.
- When sending any message (Ping, Ack, PingReq, PingReply, or any workload message), the sender attaches up to `MAX_GOSSIP_PER_MESSAGE` deltas from the queue to the `gossip` field of the `MessageEnvelope`.
- Each delta has a `hops` counter. Every time a delta is piggybacked onto a message, the counter is incremented.
- A delta is retired (removed from the queue) when `hops >= MAX_GOSSIP_HOPS`. If `MAX_GOSSIP_HOPS` is `0`, the auto value `ceil(3 * log2(N))` is used, where `N` is the number of alive members.
- Deltas are prioritized for piggybacking: lower hop count first, then higher version.
- Duplicate deltas (same key and version) received from gossip are silently merged — CRDT merge is idempotent.
- Workload messages (`HaloDelta`, `StepVote`, etc.) also carry piggybacked gossip. During active simulation, workload traffic dominates the network, so piggybacking on it ensures gossip propagation continues without requiring dedicated Ping traffic.

### Anti-entropy repair cycle

In addition to continuous gossip, nodes periodically run a full anti-entropy cycle to bound convergence time for newly joined or previously partitioned nodes:

```
every ANTI_ENTROPY_INTERVAL:
    1. Select one random peer from the alive membership list.
    2. Open a bidirectional stream and send PullReq with local Merkle digest.
    3. Receive PullReply with the peer's digest and divergent entries.
    4. Merge received entries into local CRDT state.
    5. If PullReply.complete is false, send follow-up PullReq for remaining subtrees.
    6. Close the stream.
```

| Parameter | Config key | Default | Description |
|---|---|---|---|
| `ANTI_ENTROPY_INTERVAL` | `swim.antiEntropyInterval` | `30s` | Time between full anti-entropy cycles. |
| `ANTI_ENTROPY_MAX_ENTRIES` | `swim.antiEntropyMaxEntries` | `1000` | Maximum entries per `PullReply` before truncation. |
| `ANTI_ENTROPY_ROUNDS` | `swim.antiEntropyRounds` | `16` | Maximum continuations in one exchange before it is abandoned. |
| `ANTI_ENTROPY_DEPTH` | `swim.antiEntropyDepth` | `4` | Hash-tree depth; bucket count is `2^depth`. Must be `1..=8`. |


## Error handling

Protocol errors are communicated via QUIC stream reset codes:

| Error code | Reset code | Meaning |
|---|---|---|
| `PROTO_ERR` | `0x01` | Malformed message or unknown message type. |
| `VERSION_MISMATCH` | `0x02` | Incompatible protocol version. |
| `CLUSTER_MISMATCH` | `0x03` | Cluster name does not match. |
| `AUTH_FAILED` | `0x04` | Certificate fingerprint verification failed. |
| `ADMISSION_DENIED` | `0x05` | Join request denied (capacity, blocklist, lock, token). |
| `FRAME_TOO_LARGE` | `0x06` | Frame exceeds maximum allowed size. |
| `TIMEOUT` | `0x07` | Operation timed out. |
| `INTERNAL` | `0xFF` | Internal error. |

For stream-based messages, an error is signaled by resetting the stream with the appropriate code. The peer receiving a stream reset should log the error and take appropriate action (e.g., retry with backoff, mark the peer as suspect).

For datagram-based messages, errors are not acknowledged (fire-and-forget). A persistent pattern of unacknowledged `Ping`s is detected by the SWIM probe loop and handled through the suspicion mechanism.


## Configuration parameters

All settings follow the [configuration precedence](./orishu-configuration.md): config file < environment variable < command-line argument.

| Setting | CLI flag | Env var | Default | Description |
|---|---|---|---|---|
| `listen.peers` | `--listen.peers` | `orishu_LISTEN_PEERS` | `["::/128"]` | Addresses for peer connections. |
| `accepts.peers` | `--accepts.peers` | `orishu_ACCEPTS_PEERS` | `true` | Whether node accepts join requests. |
| `limits.peers` | `--limits.peers` | `orishu_LIMITS_PEERS` | `32768` | Maximum peer connections. |
| `limits.seeds` | `--limits.seeds` | `orishu_LIMITS_SEEDS` | `1024` | Maximum seeds to maintain when joining. |
| `swim.probeInterval` | `--swim.probeInterval` | `orishu_SWIM_PROBEINTERVAL` | `1s` | Time between SWIM probe cycles. |
| `swim.probeTimeout` | `--swim.probeTimeout` | `orishu_SWIM_PROBETIMEOUT` | `500ms` | Direct probe Ack timeout. |
| `swim.indirectProbeCount` | `--swim.indirectProbeCount` | `orishu_SWIM_INDIRECTPROBECOUNT` | `3` | Intermediaries for indirect probing. |
| `swim.indirectProbeTimeout` | `--swim.indirectProbeTimeout` | `orishu_SWIM_INDIRECTPROBETIMEOUT` | `1s` | Indirect probe timeout. |
| `swim.suspicionTimeout` | `--swim.suspicionTimeout` | `orishu_SWIM_SUSPICIONTIMEOUT` | `5s` | Base suspicion timeout (scales with `log(N)`). |
| `swim.suspicionMultiplier` | `--swim.suspicionMultiplier` | `orishu_SWIM_SUSPICIONMULTIPLIER` | `3` | Suspicion timeout multiplier. |
| `swim.maxGossipPerMessage` | `--swim.maxGossipPerMessage` | `orishu_SWIM_MAXGOSSIPMESSAGE` | `10` | Max piggybacked gossip deltas per message. |
| `swim.maxGossipHops` | `--swim.maxGossipHops` | `orishu_SWIM_MAXGOSSIPHOPS` | `0` (auto) | Max hops before delta retirement. |
| `swim.antiEntropyInterval` | `--swim.antiEntropyInterval` | `orishu_SWIM_ANTIENTROPYINTERVAL` | `30s` | Time between anti-entropy cycles. |
| `swim.antiEntropyMaxEntries` | `--swim.antiEntropyMaxEntries` | `orishu_SWIM_ANTIENTROPYMAXENTRIES` | `1000` | Max entries per `PullReply`. |
| `swim.antiEntropyRounds` | `--swim.antiEntropyRounds` | `orishu_SWIM_ANTIENTROPYROUNDS` | `16` | Max continuations per exchange. |
| `swim.antiEntropyDepth` | `--swim.antiEntropyDepth` | `orishu_SWIM_ANTIENTROPYDEPTH` | `4` | Hash-tree depth (`1..=8`). |
| `peer.handshakeTimeout` | `--peer.handshakeTimeout` | `orishu_PEER_HANDSHAKETIMEOUT` | `5s` | Timeout for `Handshake`/`HandshakeAck` exchange. |
| `peer.maxFrameSize` | `--peer.maxFrameSize` | `orishu_PEER_MAXFRAMESIZE` | `16777216` (16 MiB) | Maximum length-prefixed frame size. |
| `peer.quicIdleTimeout` | `--peer.quicIdleTimeout` | `orishu_PEER_QUICIDLETIMEOUT` | `30s` | QUIC connection idle timeout. |
| `peer.quicKeepAlive` | `--peer.quicKeepAlive` | `orishu_PEER_QUICKEEPALIVE` | `10s` | QUIC keepalive interval. |


## Wire examples

The following examples use CBOR diagnostic notation ([RFC 8949 §8](https://www.rfc-editor.org/rfc/rfc8949#section-8)). These are illustrative, not normative.

### Example 1: Successful join sequence

**Step 1 — Handshake (initiator → seed):**
```
{
  "proto": 1,
  "type": "Handshake",
  "senderId": "worker-alpha",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 1,
  "payload": {
    "nodeId": null,
    "nodeName": "worker-alpha",
    "clusterName": "my-cluster",
    "protocolVersion": 1,
    "certFingerprint": h'A1B2C3...',
    "host": "192.168.1.10:6655",
    "capabilities": {
      "cpuCores": 8,
      "memoryBytes": 16384,
      "architecture": "x86_64",
      "storage": {
        "backend": "local",
        "replicas": 3
      },
      "accelerators": []
    },
    "softwareVersion": "0.1.0"
  },
  "gossip": []
}
```

**Step 2 — HandshakeAck (seed → initiator):**
```
{
  "proto": 1,
  "type": "HandshakeAck",
  "senderId": "node-seed-001",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 1,
  "payload": {
    "nodeId": "node-seed-001",
    "nodeName": "seed-one",
    "clusterName": "my-cluster",
    "protocolVersion": 1,
    "certFingerprint": h'D4E5F6...',
    "host": "192.168.1.1:6655",
    "capabilities": {
      "cpuCores": 16,
      "memoryBytes": 32768,
      "architecture": "x86_64",
      "storage": {
        "backend": "local",
        "replicas": 3
      },
      "accelerators": ["cuda"]
    },
    "softwareVersion": "0.1.0",
    "accepted": true,
    "reason": null
  },
  "gossip": []
}
```

**Step 3 — JoinReq (joiner → seed, new stream):**
```
{
  "proto": 1,
  "type": "JoinReq",
  "senderId": "worker-alpha",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 2,
  "payload": {
    "joinToken": "tok_29fka8s...",
    "nodeName": "worker-alpha",
    "certFingerprint": h'A1B2C3...',
    "softwareVersion": "0.1.0",
    "host": "192.168.1.10:6655",
    "clientHost": "192.168.1.10:8080",
    "capabilities": { "cpuCores": 8, "memoryBytes": 16384, "architecture": "x86_64",
                       "storage": { "backend": "local", "replicas": 3 }, "accelerators": [] },
    "accepts": { "peers": false, "work": true, "clients": true },
    "limits": { "peers": 32768, "clients": 1024 }
  },
  "gossip": []
}
```

**Step 4 — JoinReply ACK (seed → joiner, same stream):**
```
{
  "proto": 1,
  "type": "JoinReply",
  "senderId": "node-seed-001",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 2,
  "payload": {
    "result": "ACK",
    "formationId": "123e4567-e89b-12d3-a456-426614174000",
    "clusterName": "my-cluster",
    "assignedNodeId": "node-abc-123",
    "reason": null,
    "redirectTo": [],
    "membership": [ ... ],
    "workload": null,
    "workloadStatus": null
  },
  "gossip": []
}
```

### Example 2: Probe cycle with piggybacked gossip

**Ping (node A → node B):**
```
{
  "proto": 1,
  "type": "Ping",
  "senderId": "node-a",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 42,
  "payload": {
    "probeId": 17,
    "incarnation": 5
  },
  "gossip": [
    {
      "deltaType": "MembershipUpdate",
      "key": "node-c",
      "version": { "epoch": 1, "counter": 3, "actorId": "node-c" },
      "data": { "memberState": "Alive", "incarnation": 2 },
      "hops": 1
    }
  ]
}
```

**Ack (node B → node A):**
```
{
  "proto": 1,
  "type": "Ack",
  "senderId": "node-b",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 37,
  "payload": {
    "probeId": 17,
    "incarnation": 3
  },
  "gossip": [
    {
      "deltaType": "BlocklistUpdate",
      "key": "10.0.0.0/24",
      "version": { "epoch": 1, "counter": 1, "actorId": "node-seed-001" },
      "data": { "action": "add", "addedBy": "operator-1" },
      "hops": 0
    }
  ]
}
```

### Example 3: Indirect probe

**PingReq (node A → intermediary node B):**
```
{
  "proto": 1,
  "type": "PingReq",
  "senderId": "node-a",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 43,
  "payload": {
    "probeId": 18,
    "targetId": "node-c"
  },
  "gossip": []
}
```

**PingReply (intermediary node B → node A):**
```
{
  "proto": 1,
  "type": "PingReply",
  "senderId": "node-b",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 38,
  "payload": {
    "probeId": 18,
    "targetId": "node-c",
    "result": "Ack",
    "incarnation": 2
  },
  "gossip": []
}
```

### Example 4: Anti-entropy exchange

**PullReq (node A → node B):**
```
{
  "proto": 1,
  "type": "PullReq",
  "senderId": "node-a",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 44,
  "payload": {
    "round": 91,
    "digest": {
      "rootHash": h'1234ABCD...',
      "depth": 4,
      "nodeHashes": [h'AA...', h'BB...', h'CC...', h'DD...']
    },
    "stateTypes": ["membership", "blocklist", "workload"],
    "subtreeReq": [],
    "cursor": null
  },
  "gossip": []
}
```

**PullReply (node B → node A):**
```
{
  "proto": 1,
  "type": "PullReply",
  "senderId": "node-b",
  "formationId": "123e4567-e89b-12d3-a456-426614174000",
  "seq": 39,
  "payload": {
    "round": 91,
    "digest": {
      "rootHash": h'5678EFGH...',
      "depth": 4,
      "nodeHashes": [h'AA...', h'BB...', h'EE...', h'DD...']
    },
    "deltas": [
      {
        "deltaType": "MembershipUpdate",
        "key": "node-d",
        "version": { "epoch": 1, "counter": 1, "actorId": "node-d" },
        "data": { "memberState": "Alive", "host": "192.168.1.20:6655" },
        "hops": 0
      }
    ],
    "complete": true,
    "cursor": null
  },
  "gossip": []
}
```


## Security considerations

- **Transport encryption:** All peer traffic is encrypted via QUIC's built-in TLS 1.3. mTLS is mandatory — both sides present certificates.
- **Cluster isolation:** The `formationId` field in `MessageEnvelope` carries the immutable `metadata.id` of the cluster formation. Messages with mismatched formation IDs are silently discarded. This acts as a strict security boundary preventing cross-cluster interference. The legacy human-readable cluster name is not a security boundary and is no longer used as a wire guard.
- **Join token scoping:** Join tokens authorize cluster admission only. They must not authorize any other operation (operator APIs, workload submission, result access).
- **Certificate pinning:** Certificate fingerprints are pinned in `NodeRecord.certFingerprint` on admission and verified on every reconnection.
- **Partition integrity:** `PartitionIntent` and `PartitionAck` carry cryptographic signatures to prevent spoofed ownership claims that could lead to duplicate writers.
- **Admission control:** The blocklist provides targeted exclusion (by node ID, name, or certificate fingerprint; by host IP or CIDR range; or intersections of both); the membership lock provides global admission control. Both compose correctly and both must be clear for admission to succeed.
- **Rate limiting:** Nodes should rate-limit incoming `JoinReq` messages to prevent admission flooding. Recommended: 10 `JoinReq` per second per source address.
