# orishu Peer-to-Peer Protocol

This document defines the protocol for communication between `orishu-worker` instances (peers) within a cluster. It covers transport, framing, authentication, message schemas, gossip mechanics, and configuration. An implementer should be able to build a compatible peer node from this document alone, referring to [design.md](./design.md) for the conceptual data model and [architecture.md](./architecture.md) for the internal structure of the worker process.

For the client-facing protocol (operators and user tools), see [protocol-client.md](./protocol-client.md).


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

The `Handshake`/`HandshakeAck` exchange binds the QUIC connection to logical node identities. This is necessary because the TLS certificate alone does not carry the human-readable cluster name or protocol version. The handshake cluster name is an admission/isolation label, not durable cluster formation identity. For nodes that have already joined, the handshake also communicates the cluster-assigned node ID; for new nodes that have not yet been admitted, the `nodeId` field is null (see [Node identity](design.md#node-identity)).

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
2. The introducer evaluates [admission criteria](./design.md#member-acceptance) (`accepts.peers`, `membershipLocked`, capacity, blocklist, token validity).
3. The introducer replies with `JoinReply` on the same stream.
4. On `ACK`: the introducer generates a unique node ID (see [Node identity](design.md#node-identity)), creates a `NodeRecord` keyed by this ID, and begins gossiping it. The joining node adopts the `assignedNodeId`, `formationId` and `clusterName` from the `JoinReply`.
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
  "actorId": <string>
}
```

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
  "deltaType":  <string>,               -- "MembershipUpdate" | "BlocklistUpdate" | "WorkloadUpdate" |
                                         --   "CheckpointUpdate" | "ResultUpdate" | "AuditEvent"
  "key":        <string>,               -- Entity identifier (node ID, workload ID, etc.)
  "version":    <VersionTuple>,          -- Version of this delta
  "data":       <map>,                   -- Delta-type-specific payload (entity fields that changed)
  "hops":       <uint>                   -- Times this delta has been piggybacked. Starts at 0.
}
```

### MerkleDigest
```
{
  "rootHash":    <bytes>,                -- SHA-256 of the Merkle tree root
  "depth":       <uint>,                 -- Tree depth
  "nodeHashes":  [<bytes>, ...]          -- Hashes at the requested subtree level (for comparison)
}
```

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

- The introducer must evaluate ALL [admission criteria](./design.md#member-acceptance) before replying: `accepts.peers == true` AND `membershipLocked == false` AND `limits.peers` not exceeded AND joining node not blocklisted AND join token valid.
- On `ACK`, the introducer generates a unique node ID for the joining node (see [Node identity](design.md#node-identity)), creates a `NodeRecord` keyed by this ID, and begins gossiping it. The `assignedNodeId` field contains this newly generated ID. The `membership` field contains the full current membership so the joining node can bootstrap its cluster view.
- On `Redirect`, the `redirectTo` field contains addresses of other introducers believed to have capacity. Only nodes with `accepts.peers == true` are included.
- On `NACK`, the joining node should back off before retrying. Recommended: exponential backoff starting at 1 second, capped at 60 seconds.

---

### Ping / Ack

Sent as QUIC datagrams (unreliable). The primary mechanism for SWIM failure detection and gossip dissemination.

```
Ping payload:
{
  "incarnation": <uint>                  -- Sender's current incarnation number
}
```

```
Ack payload:
{
  "incarnation": <uint>                  -- Responder's current incarnation number
}
```

**Behavioral rules:**

- The `gossip` field of the enclosing `MessageEnvelope` carries piggybacked deltas. This is the primary dissemination path for membership updates, CRDT state changes, and protocol announcements.
- If a `Ping` arrives from an unknown sender (no matching `NodeRecord`), it is silently discarded.
- The `incarnation` field allows the receiver to update its view of the sender's incarnation number.

---

### PingReq / PingReply

Sent as QUIC datagrams. Used for indirect probing when a direct `Ping` times out.

```
PingReq payload:
{
  "targetId": <string>                   -- Node ID of the target to probe
}
```

```
PingReply payload:
{
  "targetId":    <string>,               -- The target that was probed
  "result":      <string>,               -- "Ack" | "NoSuchPeer" | "Timeout"
  "incarnation": <uint | null>           -- Target's incarnation if Ack received
}
```

**Behavioral rules:**

- The intermediary that receives `PingReq` sends a `Ping` to the target and waits for an `Ack`. It then replies with `PingReply` to the requester.
- If the intermediary has no connection to the target, it replies with `"NoSuchPeer"`.
- The intermediary uses the same probe timeout as its own SWIM configuration when waiting for the target's `Ack`.

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
  - `Alive(n)` overrides `Suspect(m)` if `n > m`.
  - `Suspect(n)` overrides `Alive(m)` if `n > m`.
  - `Dead(n)` overrides `Suspect(m)` for any `n >= m`.
  - `Dead(n)` overrides `Alive(m)` for any `n >= m`.
  - `Leave` is authoritative only when `targetId == senderId` (self-announcement).
- A node that detects it has been suspected may refute by broadcasting `Alive` with `incarnation + 1`.
- `Leave` may only be announced by the node itself. Any `Leave` where `targetId != senderId` is discarded.


## State management messages

### PullReq / PullReply

Anti-entropy state synchronization. Sent over a dedicated bidirectional QUIC stream.

```
PullReq payload:
{
  "digest":     <MerkleDigest>,          -- Requester's Merkle tree digest of CRDT state
  "stateTypes": [<string>, ...],         -- State types to sync: "membership", "blocklist",
                                         --   "workload", "checkpoints", "results"
  "subtreeReq": [<uint>, ...]            -- Specific subtree indices to request (after initial
                                         --   digest comparison). Empty = full sync.
}
```

```
PullReply payload:
{
  "digest":    <MerkleDigest>,           -- Responder's own Merkle tree digest
  "deltas":    [<GossipDelta>, ...],     -- State entries that differ between the two digests
  "complete":  <bool>                    -- true if all divergent entries included; false if
                                         --   truncated (use subtreeReq for remaining)
}
```

**Behavioral rules:**

- The anti-entropy exchange proceeds in rounds:
  1. Initiator sends `PullReq` with its Merkle digest.
  2. Responder compares digests, identifies divergent subtrees, and replies with the differing entries.
  3. If `complete` is `false`, the initiator sends a follow-up `PullReq` with `subtreeReq` specifying the remaining subtrees.
- Both sides merge received deltas into their local CRDT state.
- The stream is closed after the exchange completes.
- Maximum entries per reply is bounded by `swim.antiEntropyMaxEntries` (default 1000).


## Workload coordination messages

### PartitionIntent / PartitionAck

Ownership transfer proposals for simulation space partitions. Sent over a dedicated bidirectional stream. See [partition ownership](./design.md#partition-ownership-and-rebalancing) in the design.

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

> **CheckpointReq/Reply vs FetchChunk.** `CheckpointReq`/`CheckpointReply` carries *live, in-progress* partition state during an active run (late-joiner catch-up, reassignment). Retrieval of a **committed, immutable artifact** (checkpoint or result) from storage uses `FetchChunk` (below). Committed-artifact bytes are never moved over a swarm/torrent transport — see [decision-010](../backlog/decisions/decision-010%20-%20Adopt-QUIC-native-chunked-artifact-transfer-supersedes-decision-006.md).


## Artifact transfer

Bulk transfer of committed artifact chunks (checkpoint and result artifacts) is **QUIC-native**: a node fetches a chunk by `ChunkRef` over the existing authenticated peer streams and verifies it end-to-end against the chunk's SHA-256 `contentHash`. There is no separate transport, swarm, or new wire format ([decision-010](../backlog/decisions/decision-010%20-%20Adopt-QUIC-native-chunked-artifact-transfer-supersedes-decision-006.md)).

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
    2. Send Ping to selected peer (as datagram).
    3. Wait PROBE_TIMEOUT for Ack.
    4. If Ack received:
         - Peer is alive. Update membership view.
         - Process any piggybacked gossip in the Ack.
    5. If Ack NOT received within PROBE_TIMEOUT:
         - Select k random peers (k = INDIRECT_PROBE_COUNT).
         - Send PingReq(target) to each of the k peers (as datagrams).
         - Wait INDIRECT_PROBE_TIMEOUT for PingReply from any intermediary.
    6. If any PingReply confirms Ack:
         - Peer is alive. Cancel suspicion.
    7. If no confirmation after INDIRECT_PROBE_TIMEOUT:
         - Emit Announce(Suspect, target, target's current incarnation).
         - Start SUSPICION_TIMEOUT timer.
    8. If SUSPICION_TIMEOUT expires without an Alive refutation:
         - Emit Announce(Dead, target, target's incarnation).
         - Update membership: target.memberState = Dead.
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

All settings follow the [configuration precedence](./config.md): config file < environment variable < command-line argument.

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
    "digest": {
      "rootHash": h'1234ABCD...',
      "depth": 4,
      "nodeHashes": [h'AA...', h'BB...', h'CC...', h'DD...']
    },
    "stateTypes": ["membership", "blocklist", "workload"],
    "subtreeReq": []
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
    "complete": true
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
