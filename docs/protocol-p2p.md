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

The worker exchange adapter shares a bounded pool across reliable inbound and
outbound operations (configured capacity 1–64). Capacity is acquired without
an unbounded waiter queue; overload is an explicit error. A slot covers the
decoded request and waiting for its owner-produced reply. One five-second
absolute deadline covers stream credit, request write, response read/FIN, and,
on the server, the owner callback and response write. Handshake reads use the
4,096-byte cap before allocation; membership reads use the 1 MiB cap.
Cancellation or failure resets/stops the stream directions and releases the
slot. Transport completion never substitutes for command/admission acceptance.

Datagram submission checks the encoded delivery class, the 1,200-byte profile
ceiling and the connection's negotiated datagram size. Unsupported or oversized
datagrams fail explicitly, with no reliable-stream fallback. Queuing a datagram
is not delivery acknowledgement. The connection dispatcher and owner-completion
mailboxes still need to be integrated before this adapter enables a peer service.

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
| `ComponentChannelData` | Stream (long-lived) | Producer → Consumer | Reliable typed cross-component state/contribution transfer. |
| `StepVote` | Stream (bidi) | Voter → Peers | Step completion signal. |
| `StepCommit` | Stream (bidi) | Peer → Peers | Step advancement confirmation. |
| `CheckpointReq` | Stream (bidi) | Requester → Owner | Checkpoint/catch-up fetch. |
| `CheckpointReply` | Stream (bidi) | Owner → Requester | Checkpoint data transfer on same stream. |
| `FetchChunkReq` | Stream (bidi) | Requester → Holder | Artifact chunk fetch by `ChunkRef`. |
| `FetchChunkReply` | Stream (bidi) | Holder → Requester | Verified block stream on same stream as `FetchChunkReq`. |


## Framing and serialization

### Stream framing

The formation PoC now negotiates ALPN `orishu-membership/5`, adding bounded
optional trace context while retaining admission attempt identity and assignment
replay. Profiles 1/2/3/4 are not accepted or
silently downgraded; rebuild/restart participating PoC workers together. The
policy-aware Merkle hash profile remains version 2: this transport extension
does not change canonical membership hashes or non-admission membership messages.
It caps a stream frame at 1 MiB (before
allocation), CBOR nesting at 24, total values/keys at 32,768, arrays at 4,096
items, maps at 64 fields and UTF-8 strings at 4,096 bytes. Duplicate map keys,
tags, floating-point and indefinite strings/arrays are rejected. Bounded
indefinite maps are supported for serde flattened records. Exactly one CBOR
value occupies a frame; trailing data is rejected. These narrower PoC limits
override the generic maximum below and are enforced by the worker decoder.

Collection preflight additionally caps `gossip` at 10, `deltas` at 1,000,
`nodeHashes` and `buckets` at 256, `accelerators` at 32, `engines` at 16, and
address arrays `peers`, `clients`, and `redirectTo` at 8. These limits apply
before typed allocation, including nested member records. The decoder borrows
encoded envelope fields, checks authenticated session binding, formation and
protocol before constructing a typed payload, and measures the received
`membership` array's byte extent directly (not by re-encoding its values).

### Proposed trace-context extension

[ADR 0025](adr/0025-version-peer-trace-context-propagation.md) accepts profile 5
only, with coordinated rebuild/restart and no profile-4 fallback. It is now
active, with [three-worker and compatibility evidence](tasks/cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09).
The preceding [codec increment](tasks/cluster-formation-conformance.md#staged-profile-5-wire-codec--2026-09-09)
established the grammar and packet-fit rules. Historical profile 4 rejects
`traceParent` and is no longer negotiated.

The membership envelope keeps its seven required fields and permits
one optional `traceParent` CBOR text field. It uses the
[client trace-context grammar](protocol-client.md#planned-client-trace-context),
with a 128-byte context-processing cap. Invalid/missing context is discarded
independently of an otherwise valid membership message. A non-text field is
ignored after bounded structural validation. Duplicate keys, malformed CBOR,
unknown domain fields and existing frame/value/string limits still reject the
packet normally; optional telemetry does not relax hostile-input validation.

Only authenticated and correctly bound session traffic may supply a remote
parent. Context is absent from admission identity/digests, replay keys, hashes,
core messages and catch-up records. A sender omits context when it would exceed
the existing packet budget, preserving the already selected payload/gossip
and delivery class. New-profile builds without tracing support the same grammar
but need not retain metadata. The existing handshake and baseline shapes remain
unchanged; exporter implementation must not imply context support there.

The encoder first obtains the ordinary bounded domain packet, including
its selected gossip. Its golden-tested indefinite envelope permits appending a
69-byte canonical context field without re-encoding payloads. If that append
would violate the byte or structural-work cap, the exact original bytes and
deferred-gossip count are retained. The active owner now attaches its locally
sampled join-exchange span context; the receiver creates an `orishu.admission`
server span after registry/session/generation checks. Adopting the remote parent
also requires the current join credential; admission policy and credential
decisions still run normally. Invalid credentials cannot select diagnostic
ancestry. Missing/invalid context yields a fresh locally sampled trace.
Responses, handshakes, catch-up DTOs and periodic gossip currently emit no
context. There is no operator-selectable dual-profile mode or fallback.

### Formation profile 2 membership payloads

The implemented `peer::wire` adapter uses the envelope below with all seven
fields required and unknown envelope/payload fields rejected. `proto` remains
1 for the semantic membership protocol; ALPN selects the incompatible framing
and policy-aware hash profile. The following narrower payloads override the
historical broad examples later in this document for the formation PoC:

| Type | Required payload fields |
| --- | --- |
| `JoinReq` | `attemptId`, `joinToken`, `nodeName`, `certFingerprint`, `endpoints`, `accepts`, `capacity`, `capabilities` |
| `JoinReply` ACK | `result: "ACK"`, `formationId`, `clusterName`, `assignedNodeId`, `membership` |
| `JoinReply` NACK | `result: "NACK"`, `reason` (structured `RejectReason`, not free text) |
| `JoinReply` redirect | `result: "Redirect"`, `redirectTo` |
| `Ping`, `Ack` | `probeId`, `incarnation` |
| `PingReq` | `probeId`, `targetId` |
| `PingReply` | `probeId`, `targetId`, `result` (`{"ack": incarnation}`, `"noSuchPeer"`, or `"timeout"`) |
| `Announce` | `announcement` (`"suspect"`, `"alive"`, `"dead"`, or `"leave"`), `targetId`, `incarnation` |
| `PullReq` | `round`, `digest`, `buckets`, `cursor` |
| `PullReply` | `round`, `digest`, `deltas`, `complete`, `cursor` |

`endpoints` has `peers` and `clients` address arrays; `capacity` has numeric
`peers` and `clients` limits. Capabilities, members, fingerprints, digests and
cursors use the canonical membership types and their golden fixtures. Null
`cursor` denotes no continuation. There is no workload or `stateTypes` field:
this adapter reconciles membership, policy, tombstones and blocklist only.
Variant-inapplicable join reply fields are absent, not null. Admission tokens
are exactly 64 lower-case hexadecimal characters (256 random bits) and leave
the decoded transport DTO separately from the secret-free core input.

The 1,200-byte PoC datagram ceiling includes the entire CBOR envelope. Encoding
removes trailing piggyback deltas until it fits and reports how many were
deferred. The owner returns locally trimmed deltas that could fit by themselves,
or all offered gossip when no member route exists, through a bounded
`GossipDeferred` effect outcome. The core accepts only exact
current records and undoes only the omitted transmission charge, including
for a just-retired entry. Other sends emitted by the same transition remain
charged if submitted; feedback never merges into membership or policy.
The existing queue capacity still applies. A record too large for even a bare
datagram is not continually requeued: its authoritative state remains available
through reliable anti-entropy. Submission and receipt are still distinct;
ordinary transport loss has no delivery guarantee. An
unencodable base message is an error, never a stream fallback. The receiver
rejects stream-only bodies on datagrams and SWIM bodies on streams. Applicant
sessions carry only `JoinReq`; a label matching a member ID cannot upgrade a
provisional session. The worker integration described below now supplies session
orchestration, admission-state catch-up and live-assignment retry recovery;
lifecycle/process conformance is recorded in the
[accepted formation ledger](tasks/cluster-formation-conformance.md).

### Sequence and credential lifecycle

Per-sender sequence replay tracking retains a 128-number sliding bitmap per
admitted identity, across its connections within the formation. A previously
unseen stream request inside that window remains valid even if a later
sequence arrived first on another independent QUIC stream. Duplicate or
out-of-window stream requests are rejected; bounded operation retry uses a
fresh envelope sequence and the same operation identity. Datagram probe
correlation and incarnation checks remain authoritative for reordered replies.
Formation adoption clears the old window; a reconnect does not reset it.

The initial PoC trust mechanism is explicit certificate pinning. Join material
obtained through an authenticated operator path carries the target formation,
introducer endpoint, complete public certificate and its SHA-256 fingerprint,
plus the secret join token. The dialer verifies the pin and TLS proof before
sending application credentials. The server requires a valid client certificate
and proof of its key, but unknown certificates establish only provisional
sessions: membership admission still evaluates every gate. Unknown applicants
cannot exchange admitted-member traffic. Redirects cannot establish new pins;
the PoC reports them for an operator to supply trusted material rather than
automatically forwarding the token. No QUIC early data is accepted.

Workers generate a self-signed peer certificate for `orishu-worker` when no
identity is provisioned, store its key/certificate securely, and retain it
across process restarts. New processes create fresh standalone formation and
node IDs. Certificate or credential load failures are explicit startup errors;
they never trigger silent replacement. Admitted-session binding additionally
checks the formation-assigned node ID and currently held fingerprint.

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
  "formationId": <string>,            -- Immutable cluster formation ID (`orishu_identity::FormationId`, ADR 0013). Serves as a strict security boundary; receiver rejects if mismatch.
  "seq":       <uint64>,              -- Monotonic per-sender sequence number
  "payload":   <map>,                 -- Type-specific payload (defined per message)
  "gossip":    [<GossipDelta>, ...]   -- Optional piggybacked gossip deltas (may be empty)
}
```

**`type` values:** `"Handshake"`, `"HandshakeAck"`, `"JoinReq"`, `"JoinReply"`, `"Ping"`, `"Ack"`, `"PingReq"`, `"PingReply"`, `"Announce"`, `"PullReq"`, `"PullReply"`, `"PartitionIntent"`, `"PartitionAck"`, `"HaloDelta"`, `"ComponentChannelData"`, `"StepVote"`, `"StepCommit"`, `"CheckpointReq"`, `"CheckpointReply"`, `"FetchChunkReq"`, `"FetchChunkReply"`.

**`formationId` guard:** A node receiving a message with a `formationId` that does not match its own immutable formation ID must silently discard the message. This acts as a strict security boundary preventing cross-cluster interference. The legacy human-readable `clusterName` is no longer used as a wire guard.

**`seq` deduplication:** The `seq` field is scoped to one sender identity in one
formation. Use the 128-sequence replay window above, not a high-water mark:
independent streams may arrive out of order. Datagram correlation remains
governed by probe identity and incarnation.


## Authentication

All peer connections use mutual TLS (mTLS). Both sides present certificates during the QUIC handshake. QUIC provides TLS 1.3 encryption natively.

### Certificate verification on connection

The formation session adapter distinguishes three roles. TLS extraction checks
the negotiated membership ALPN and obtains the fingerprint from Quinn's peer
certificate, never from an envelope. Each connection is stamped with the local
owner's formation generation; adoption or leave invalidates the old binding.

| Binding | Authority and allowed input |
| --- | --- |
| Provisional applicant | TLS key possession plus a claimed label; only `JoinReq`, with every admission gate still required |
| Pinned outbound introducer | Operator-authenticated target formation/certificate pin plus handshake node claim; only `JoinReply` while joining, not arbitrary admitted traffic |
| Admitted member | Assigned node ID and matching certificate in the current model; rechecked before each decoded message |

A provisional applicant can upgrade only to a member record with the same
certificate and label. An outbound introducer binding is not upgraded in place:
formation adoption invalidates that generation and requires an admitted binding
against the adopted model. Self-connections, dead/missing members, uncleared
certificate tombstones and active node/name/fingerprint blocks are refused.
Live network-block enforcement and complete handshake/session orchestration remain
integration work; this binding module alone is not a running peer service.

For known-member dialing, the client can pin the certificate fingerprint held
by membership without retrieving a complete certificate through gossip. The
TLS verifier checks the presented certificate's SHA-256 against that trusted
pin before treating it as an anchor, then validates name, usage, validity and
TLS key possession. It still rejects chains or certificates outside the bounded
single-certificate profile. Operator join material continues to support an
exact full-certificate pin. Loading a private peer identity validates both
client/server usage and the `orishu-worker` name at startup, not only key matching.

1. The QUIC handshake completes with mTLS — both peers present certificates.
2. The receiving node extracts the SHA-256 fingerprint from the peer's TLS certificate.
3. If the fingerprint matches a known `NodeRecord.certFingerprint` for a node in `Alive` or `Suspected` state, the connection is accepted as a reconnection from that known peer.
4. If the fingerprint is unknown, the connection is accepted provisionally. The peer must initiate a `JoinReq` within the handshake grace period (default 5 seconds, configurable via `peer.handshakeTimeout`). If no `JoinReq` arrives, the connection is closed.
5. If the fingerprint matches a `Removed` or blocklisted node, the connection is rejected immediately.

### Join token validation

The join token is presented inside the `JoinReq` message payload, not at the TLS layer. The introducer verifies the token after the mTLS handshake succeeds. This separation allows the transport to be established before the admission decision.

The PoC source-network evidence adapter accepts literal IPv4/IPv6 addresses or
CIDRs, at most 1,024 rules and 253 bytes per rule. Host bits in a CIDR are
masked; DNS names, ports, bracketed addresses, zone identifiers, whitespace,
signed prefixes and prefixes beyond the address width are rejected. IPv4 rules
also match IPv4-mapped IPv6 sources. IPv6 rules match IPv4 sources using their
mapped representation, so a broad IPv6 rule covering that space blocks them too.
Matching is bounded O(rules), with no DNS or advertised-address lookup.

The shell must supply the current QUIC-observed source address, including after
connection migration, not an endpoint claim or proxy header. A malformed or
oversized rule set returns a blocked-source verdict plus a bounded policy
diagnostic; it never becomes an empty allow-list and never causes the adapter
to abandon a pending credential completion. Token comparison, authenticated
transport and source blocking remain independent evidence fields. The core
still rechecks its locally held admission gates before insertion. This evidence
adapter is connected through the owner and tested over the real QUIC request
path. Gate checks run before verification, after its completion, and again
after asynchronous node-ID allocation immediately before insertion.

The introducer refuses `alreadyAdmitted` when its current membership contains
an `Alive` or `Suspected` record for the applicant certificate. This includes
a second concurrent allocation completing after the first admission inserted
that certificate; it must not create another live ID. The final gate check
also catches intervening capacity, lock and identity-exclusion changes. Dead
history alone does not forbid a fresh assigned identity, but active certificate
tombstones/blocklist entries still do. This is a locally enforced admission
gate, not a globally serialized certificate reservation across partitioned
introducers; the source worker still serializes its supported join attempts.

`alreadyAdmitted` is a new bounded `JoinReply` NACK reason in the in-progress
profile implementation; this reason itself does not change the envelope or
Merkle hashes. Its CBOR value and complete worker-envelope round trip have a
golden regression, and the real QUIC owner test verifies refusal without a
second insertion. Older PoC decoders may reject this unfamiliar NACK; rebuild
participating binaries together, as rolling profile upgrades are unsupported.
The refusal does not itself recover a lost ACK or identify an accepted attempt.
An unresolved join remains unresolved until the separate bounded recovery
workflow establishes its outcome; it must not be described as never admitted.

### Certificate rotation

Certificate rotation is deferred from the formation PoC. The worker must not
advertise an implemented rotation command. A future signed rotation exchange
needs a versioned protocol and security decision before it can change a pinned
member identity.

### Local credential storage (formation PoC)

The Unix worker credential adapter uses an exclusively owned state directory
with no group/other access. `identity.json` is a version-1 private record with
`version`, DER `certificate` bytes, and PKCS#8 DER `private_key` bytes;
`operator.token` contains exactly the 64-character canonical operator token
with no newline. Both files are owner-only regular files with one hard link.
Reads are descriptor-relative with no final-component symlink following;
identity input is capped at 65,536 bytes and token input at 65 bytes (the extra
byte detects an oversized token). The root directory also rejects symlinks.
The adapter holds an exclusive non-blocking `instance.lock` for its lifetime
so two workers cannot share one identity directory accidentally.

New files are created exclusively with mode `0600`, synced with their
directory, and never overwrite existing contents. Interrupted or corrupt
initialization fails explicitly for operator repair; it does not regenerate
an existing identity silently. Credential diagnostics never include key or
token contents. Platforms without this private-file implementation return an
explicit unsupported-platform error rather than creating insecure files.

The operator credential is worker-local and distinct from the formation join
token. The CLI/harness provisioning path will explicitly read the private
operator token; merely reaching a Unix socket does not authorize mutation or
token retrieval. Formation IDs, node IDs and join tokens are not restored from
these files on restart. Startup now loads these credentials and the summary
API uses the operator token for remote reads. CLI credential-file provisioning,
mutations and join-material retrieval remain integration work.


## Connection lifecycle

### Establishment

The profile-2 handshake now has a bounded DTO/adapter. Its frame payload is
capped at 4,096 bytes before receive-buffer allocation and uses the existing
five-second frame/FIN deadline. The final connection loop must additionally
enforce the connection-wide first-handshake deadline and one initial bidi stream;
those session-orchestration pieces are not enabled yet.

The handshake uses the normal seven envelope fields, with `proto: 1`, `seq: 0`
and an empty `gossip` array. It does not enter the core's per-sender message
sequence window. The only payload fields for `Handshake` are required `nodeId`
(explicit null for an applicant), `nodeName`, and `certFingerprint`. `senderId`
must equal the assigned ID when present, otherwise the applicant label.
`formationId` is the target formation, not the applicant's old standalone one.
No endpoints, capabilities, software version or secrets appear in this exchange;
admission carries the bounded advertisement later.

`HandshakeAck` uses the same identity fields with a non-null responder node ID,
plus `accepted`. Success has no `reason`; refusal requires one of
`incompatibleProtocol`, `formationMismatch`, `identityMismatch`, or `overloaded`.
Inapplicable optional fields are omitted. Unknown fields, incompatible versions,
nonzero sequences, gossip, inconsistent identities and repeated binding attempts
are rejected. Both request and reply must agree with the TLS certificate, and
outbound replies must also satisfy the trusted target pin. These narrower shapes
override the historical broad payload examples below for profile 2. A successful
handshake does not imply admission: `JoinReq`/`JoinReply` use a subsequent stream.

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

The worker now also provides a secret-free outbound bootstrap dial adapter.
`IntroducerTarget` copies only the validated formation, assigned introducer ID,
certificate pin and one to eight literal socket candidates from join material.
One shared `Dialer` permits four concurrent attempts without a waiting queue;
candidates are tried sequentially under a 15-second total budget, with the
existing five-second TLS/frame phase limits and shared reliable-exchange pool.
It uses the caller's endpoint, verifies the pin during TLS, and sends only the
initial handshake—not a JoinReq or token. DNS and redirects are not supported
by this profile. A dropped pending result closes its connection even if clones
exist. Dial success returns raw ACK bytes for current-owner validation, not
membership or command acceptance.

Optional [outbound dial metrics](orishu-observability.md#outbound-dial-and-tls-metrics)
observe whole attempts, individual TLS candidates and the shared capacity gate.
Candidate fallback, initial stream exchange and subsequent owner validation
remain distinct stages; instrumentation changes neither this wire profile nor
the dial/retry budgets. Metrics do not carry endpoint or credential metadata.

The pinned-introducer ACK validator checks the supplied assigned node ID in
addition to formation and TLS fingerprint before granting the limited
JoinReply role. A real dispatcher test covers correct/wrong pins, mismatched
introducer IDs, dial saturation and dropped-result cleanup. This adapter is
now registered through the owner's bounded peer lane using a distinct pinned-
introducer reply mode. The owner rejects stale generations and mismatched
formation/node/fingerprint bindings, rechecks source policy, and assigns its
own session ID. The introducer binding has the same ten-second provisional
lifetime and shared 16-provisional-session budget as applicants; it permits
only JoinReply decoding while the worker retains its source formation. A real
dial test verifies accepted owner registration, wrong-node rejection and stale
generation closure without a membership change. Registration alone does not
begin a join; the production operation, adoption, replay and catch-up paths
described below supply those separate boundaries.

The owner now exposes an internal begin-join seam that checks source formation,
generation, standalone participation, absence of another core join attempt and
the current registered introducer binding before applying `BeginJoin`. Its
shell-only outbound join state retains the target and token while the core owns
attempt timers. JoinReq effects use the target formation and applicant identity;
bounded reliable replies re-enter the same owner packet path. Missing routes
or send capacity count as IO failure and leave recovery to bounded core timers.
Generation changes clear the secret and abort old sends. Validated adoption
reports `catchingUp`, closes old-generation sessions and leaves target credential
installation/introduction unavailable. A real two-owner test exercises the
complete handshake/JoinReq/admission/ACK/adoption path without initializing the
joiner from a pre-adopted model. That test alone does not establish identified
operator API, lost-ACK recovery, target-state catch-up or three-process acceptance;
those paths have their separate evidence below.

The owner now remembers whether any JoinReq was submitted to reliable IO. If
core retries end without adoption after such a submission, local participation
becomes `joinUnresolved`, not ordinary standalone join eligibility. Old sends
are aborted and transient token state is dropped; another begin-join is refused.
This is deliberately conservative even when a transport error does not prove
delivery. A real-QUIC fault test completes introducer insertion and discards its
ACK at the response-write boundary, verifying retained remote membership,
unchanged local formation and explicit unresolved status. The hook exists only
in the test harness; production transport/authentication/core paths are used.
That original fixture proves uncertainty only. The later identified replay and
[recovery process evidence](tasks/cluster-formation-conformance.md#recovery-outcome-and-history-loss-audit--2026-09-07)
separately establish usable-assignment recovery and bounded stop outcomes; an
unresolved result itself is not recovery.

The sans-IO core now provides `RebindJoin` for an IO shell that has established
a fresh authenticated session to the same pinned introducer. It checks the
expected old session and target formation, refuses identical-session or stale
replacement commands, and consumes the next retry from the original budget.
The old timer is cancelled, old-session replies cannot adopt, and exhaustion
abandons the attempt rather than resetting its count. `BeginJoin` cannot
overwrite a pending attempt. All retry paths now cancel their previous timer,
including early NACK/redirect retries and exhaustion. These are local transition
semantics; they add no wire field or new authentication authority.

Worker startup now supervises pending-join redial alongside admitted-peer
maintenance. Only an explicit public join supplies its retained secret-free
introducer route: target formation, assigned introducer ID, certificate pin
and bounded literal endpoints. The scheduler never substitutes a redirect,
discovers another cluster or starts a new operator attempt. It polls once per
second with missed ticks skipped, prepares only when the old route is absent,
and permits one active reconnect job. At most eight preparations are allowed
per pending join; failed/cancelled preparations consume that budget. They do
not reset the independent core retry count or timers. Preparation stops once
there is no remaining core retry. Core exhaustion retains the existing
conservative unresolved outcome if a request may have been delivered.

Preparation reserves a completion slot before IO and carries no join token.
Redial uses the existing process dialer, literal endpoint selection, 15-second
handshake budget and shared exchange pool, under a 16-second supervisory
deadline. Owner-query waiting is bounded to one second. The owner accepts the
result only for the matching pending generation, old session and reconnect
ordinal, revalidates the original retained pin/formation/introducer against
the real handshake, and invokes `RebindJoin`. Only then can normal owner
effects attach the retained token to another JoinReq. A dropped job releases
its reserved outcome, while a stale successful handshake is discarded and
closed. Lifecycle/participation changes cancel jobs; shutdown aborts supervision
before stopping the owner. The internal token-only begin-join testing seam
does not supply routing material and is not automatically redialled.

Real runtime tests drop a decoded JoinReq connection before insertion and
verify redial, adoption and catch-up under the original operation. Separate
tests cancel eight prepared jobs without resetting the budget and release a
successful replacement handshake after owner shutdown. This proves bounded
pre-insertion reconnect. Profile 4 adds accepted-assignment replay below.

#### Accepted admission replay (profile 4)

JoinReq requires `attemptId`, a bounded node-ID-shaped value generated from
fresh 256-bit randomness by the joining shell at BeginJoin. This is correlation,
not an assigned membership ID or authorization. The same pending operation
retains it across timers/redials; a new lifecycle generates another value even
with the same certificate. It stays outside the replayable core. JoinReply
remains correlated by the current pinned session and pending target.

Each introducer retains at most 1,024 accepted assignments for its local
formation lifetime, keyed by authenticated fingerprint and attempt ID. A
domain-separated SHA-256 digest covers canonical typed CBOR of protocol,
formation, attempt ID, name, certificate, endpoints, role flags, capacity and
capabilities. Sequence, gossip and token bytes are excluded; current credential
validation remains mandatory. No tokens or snapshots are retained in the ledger.
Capacity is checked before admission. Insertion and recording the assigned ID
and original reply sequence occur in the same serialized owner turn, before
returning the ACK. Credential verification and ID allocation are currently
inline; future asynchronous work must reserve ledger capacity before insertion.

Records never expire or evict into new execution: exact replay works at
capacity, but new attempts refuse. Local formation change, ejection and process
exit discard the ledger; restart does not restore it. An exact accepted retry
rechecks source-network/session binding, introducer readiness/current token,
the assigned member's certificate and alive/suspected state, blocks and
tombstones. It returns the original assigned ID with a bounded current snapshot,
without a core transition, new identity or liveness resurrection. Recovery of
an existing assignment is not a new admission; a later lock does not revoke an
existing member. An already-promoted connection may only replay its own recorded
assignment, never use applicant syntax for another admission.

Changed request bodies, wrong credentials, retired assignments, unavailable
snapshots and unknown attempts on admitted sessions refuse the exchange. A new
applicant connection with an unknown attempt follows normal admission gates,
including duplicate-live-certificate refusal. Refusal does not mean the original
request was never inserted; retry exhaustion remains unresolved. There is no
cross-introducer lookup, global exactly-once guarantee or durable replay.

Real runtime evidence discards an ACK after insertion/ledger commit, closes the
connection, and verifies redial recovers exactly that assigned ID through
catch-up. Wire tests cover same-connection replay after promotion and rejection
of wrong tokens, changed bodies and unknown attempts without another core
transition. Replay while membership is locked also returns the existing
assignment without changing the lock; the lock gates new admissions, not
continued membership.

The same wire/owner test now removes an accepted member, blocks its fingerprint,
or applies its serialized self-departure before opening a fresh mTLS applicant
connection. Exact replay refuses without another core transition or insertion.
A fresh attempt ID still receives `tombstoned` or fingerprint `blocklisted`
refusal when excluded; dead history alone permits a new assigned identity while
the old record remains dead. Removal/block setup uses owner commands and
departure uses the owner's datagram decoder; the reconnect handshake and
replay/fresh-attempt requests use real QUIC streams. This is not a public
restart/ejection workflow. Ledger tests cover capacity/non-eviction. Operator
recovery after retirement, introducer loss/restart and the wider process fault
matrix remain required.

After a successful `Handshake`/`HandshakeAck` exchange where both sides recognize each other's node ID and certificate fingerprint, the connection is established without a `JoinReq`. Both sides immediately begin gossip exchange and workload coordination.

### Graceful shutdown

1. The departing node sends an `Announce(Leave)` to all connected peers (piggybacked on the next outgoing message or as a standalone datagram).
2. If a simulation is running, the node completes its current step, writes a checkpoint, and transfers partition ownership.
3. The node closes all QUIC connections.

### Connection maintenance

#### Admission-state catch-up contract

Before introduction is enabled after adoption, fetch one identified, immutable
admission-state baseline from a ready admitted peer in the same formation.
The source freezes policy (including explicit absence/default unlocked), all
blocklist entries including lifted entries, and all membership tombstones in
one serialized owner turn. This is separate from membership listing and from
scientific checkpoint transfer. The snapshot contains no join token. It proves
the source's complete state at that boundary, not globally latest state during
a partition. Concurrent newer deltas continue through normal versioned merge;
installing a baseline must not discard newer locally learned restrictions.

The baseline uses ordered, typed records: one policy record first, then
blocklist keys in domain order, then tombstone node IDs in domain order. Each
page has at most 32 records and 64 KiB of canonical CBOR. There are at most
161 pages (one policy, 1,024 blocklist entries and 4,096 tombstones), with an
8 MiB aggregate encoded-page cap. Page metadata binds formation, source node,
source-assigned snapshot ID, page index and total page count. An SHA-256 root
over domain-separated, length-prefixed canonical page encodings identifies the
complete baseline. An explicit policy-only page proves empty collections;
absence of pages does not. Reject mixed identities, changed totals, missing,
duplicate, reordered, oversized or noncanonical pages before installation.

The source will retain at most four baselines for 30 seconds each, bound to
the requesting admitted identity and owner generation, with no eviction of a
live transfer to admit a new one. Expired continuations fail explicitly; the
joiner retries a new identified baseline within a bounded overall catch-up
attempt. Every request and credential release rechecks current admitted session,
source restrictions and generation. A full valid baseline is merged atomically
through the pure core; rejection, capacity failure, self-removal or invalid
policy keeps the worker non-introducing. Public baseline bytes remain separate
from target credential installation in the worker shell.

Credential release follows proof of complete baseline receipt, on an admitted
authenticated stream bound to that snapshot, requester and formation. It is
never gossip, snapshot content, trace context or replayable core state. Only
after full baseline validation/merge and target credential installation can the
owner publish completed catch-up and enable its configured introducer role.
Source lock changes and normal admission gates remain authoritative locally;
the barrier is not a global membership fence. Network routes, explicit transfer
errors and wire negotiation must be integrated and fixture-tested before these
new exchanges are advertised as a supported peer profile.

The source retention adapter now implements four 30-second leases (at most
32 MiB retained encoded page bytes, plus bounded metadata). A begin request is
identified by requester node and operation ID. An exact live retry returns the
same descriptor without refreshing expiry or recapturing changed source state.
After expiry, a new begin may capture another baseline under a new monotonic
snapshot ID; old continuations remain unavailable. Snapshot IDs never wrap or
reset during an owner-generation sweep. A full store rejects a new request
without evicting existing leases. New or existing access rechecks admitted
identity, certificate, liveness and identity exclusions; current transport and
source-network checks remain mandatory at the owning session boundary.

Profile 4 retains `AdmissionStateReq` / `AdmissionStateReply` over registered
member bidirectional streams. They are separate shell resource envelopes with
`schemaVersion: 1`, `type`, `formationId`, `senderId`, `requestId` and an
`action` / `outcome` object. Actions are `begin`, `page` (snapshot/index) and
`confirm` (snapshot/root); outcomes are baseline descriptor, nested canonical
page, confirmed credential, or a finite rejection reason. Request bodies are
capped at 4 KiB, reply decoding at 128 KiB; the existing five-second shared
exchange deadline and 64-exchange capacity still apply. These idempotent read
resources use request/snapshot correlation rather than consuming core message
sequences. They never enter gossip or the membership replay window.

The serialized owner rechecks registered-session generation, current member
identity and observed source-network policy before typed request handling.
Applicants, cross-formation introducer bindings and datagrams cannot access
this surface. Every action additionally requires current source introducer
readiness and its installed formation token; otherwise it returns `notReady`.
Malformed identity/schema input yields no credential response. Finite resource
rejections distinguish `unavailable`, `overloaded`, `invalid` and `unauthorized`.
The owner sweeps retained baselines on its normal transitions/ticks. Confirm
rechecks the retained page barrier before copying the current token into the
encrypted response; neither token nor request enters core state or ordinary
debug formatting. Receiver owner scheduling/installation is described below.

The receiving IO client now checks the selected source's profile/certificate
against the actual connection, then runs begin, sequential page requests and
confirm under one 25-second deadline. Every exchange shares the worker's
five-second exchange deadline and permit pool; reply length is capped at
128 KiB before allocating its receive buffer. It accepts at most 161 pages
and 8 MiB of canonical page content. Descriptor formation/source, reply
request/source identities and page snapshot/order are validated; the credential
response must match the completed descriptor's snapshot and digest. Confirm is
never sent until the public content proof succeeds. Source rejection, malformed
content, timeout or cancellation returns no completed credential installation.
There is no internal retry loop or newly opened peer connection.

The private completion carries the verified atomic-baseline command value and
the separately held target token. It has no Debug/Clone/serialization interface.
The owner must still correlate its attempt/session/generation, revalidate current
authority, atomically apply the public baseline and explicitly install the token
before updating readiness or join-operation completion. Receiving bytes alone
does none of these. The real admitted-session test now exercises two baseline
pages and the receiver. Malformed/expired transfer faults and public process
catch-up completion have separate evidence described below; the
[conformance ledger](tasks/cluster-formation-conformance.md) records their
current acceptance boundaries and remaining gaps.

The receiving owner now prepares at most one catch-up job at a time and reserves
a control-lane completion slot before returning it. A dropped job or preparation
reply queues failure, not an orphaned operation. Preparation requires catching-up
participation and a currently registered, source-authorized member route with
an advertised introducer role. Up to three prepared attempts are allowed within
90 seconds of adoption, including time waiting for a route; no route does not
consume an attempt. Successive attempts rotate across available candidate
routes. Production runtime scheduling checks eligibility once per second with
missed ticks skipped. Registration of the first current admitted route after
adoption also wakes initial preparation once, without waiting for that tick.
The coalesced IO notification carries no authority or credentials; preparation
still revalidates the current generation, route and eligibility. Failed attempts
retain the periodic retry cadence. Scheduling permits one active transfer and
bounds owner preparation waiting to one second. Generation/participation changes
cancel obsolete jobs;
shutdown stops scheduling and aborts outstanding work before stopping the owner.
The receiver's existing overall transfer deadline still applies.

Completion matches the owner's process-unique attempt ID and generation,
revalidates its source session/formation and checks the adoption deadline. Only
then does it apply the atomic core baseline command. An accepted baseline with
the local identity intact, no local identity exclusion and valid bounded network
rules permits installing the separately held token and publishing `joined`.
Role configuration remains independent: a joined non-introducer does not gain
that role. Merge rejection or unavailable/stale source keeps introduction
disabled and records `catchUpFailed`; retry preparation explicitly returns the
operation to `catchingUp`. Baseline self-removal publishes local ejection and
closes held sessions without installing a token. The same owner fence now
handles self-removal learned through ordinary gossip/reconciliation: it changes
participation to `ejected`, advances the local generation, drops credentials,
pending catch-up and bootstrap state, cancels timers/sends and closes sessions.
It suppresses the entire removal transition's outgoing effect batch, including
a probe response that would otherwise use the removed identity. No automatic
return to standalone or readmission occurs. Local inspection and shutdown
remain available; peer admission, dialing and periodic membership work stop.

The real-QUIC registry regression delivers a valid tombstone in a peer datagram
and verifies ejection, connection closure and suppression of further owner
transitions. The runtime regression separately applies a baseline self-removal
while a catch-up preparation is held, then drops its old-generation completion
and verifies retained `catchUpFailed`/ejected state. This latter test is an owner
boundary test, not a serialized baseline-transfer fault test. Three additional
runtime regressions fetch and verify a complete baseline and credential over
real QUIC, then hold the reserved successful completion behind a test-only
scheduling barrier. Releasing it after leave preserves the replacement
standalone identities/token; after ejection it preserves non-introducing state
without another core transition; after shutdown it cannot revive the closed
owner. These tests exercise the normal receiver and completion handler, not
fabricated credentials. Ejection itself is supplied at the owner boundary;
public ejection/restart exclusion and the wider lifecycle fault matrix have
separate passing process evidence in the formation ledger. The owner path is exercised
with real receiver IO by the runtime test, including cancellation followed by
automatic retry. The public CLI harness now proves completed catch-up and
A-admits-B, B-admits-C handoff; final lifecycle/fault validation is recorded in
the formation ledger, without widening these receiver fixtures' scope.

A further runtime regression retires the source session after successful fetch
without changing the receiver's generation. The old completion is refused;
automatic reconnection and a fresh catch-up attempt can then reach `joined`.
Another cancels three prepared jobs and verifies that subsequent preparation
and runtime scheduling remain non-introducing with `catchUpFailed`. This proves
the attempt cap. The separate deadline regression holds a verified real
transfer, ages only the receiving owner's adoption timestamp to 90 seconds
through a test-only control, and delivers success with the generation and
session unchanged. The owner retains `catchUpFailed` and target identity,
installs no credential and refuses another preparation. This isolates the
deadline decision rather than measuring wall-clock timer latency.

Receiver fault tests use the real source retention/resource handler and two-page
transfers over mTLS QUIC. They expire a continuation using controlled source
time, truncate the final page's framed bytes, substitute a cross-snapshot page,
or corrupt the declared content digest. Each fails without requesting credential
confirmation, then completes a fresh transfer on the same connection and
single-slot exchange pool. These tests use admitted-member source fixtures;
they are wire/receiver evidence, not public CLI or full owner lifecycle tests.

The changing-source wire regression updates source policy and adds a blocklist
entry after the first page has crossed the authenticated connection. The
in-progress transfer completes with its original policy/content; a fresh begin
returns a newer snapshot with a different root and the changed state. Together
with atomic core merge tests that retain newer local restrictions, this verifies
snapshot consistency without claiming globally latest policy or a global lock.

Pages may be fetched in order or re-fetched after a lost response, but a request
cannot skip ahead. Finish acknowledgement requires that every page was issued
and that the pinned root matches. This acknowledgement is not cryptographic
proof of remote installation: the receiver's own completeness validation and
atomic core merge remain the readiness authority. The source must separately
recheck its own readiness/current credential before releasing a token. Source
retention has no credential fields; the source route reads credential authority
separately from the owner only after these checks.

The core now accepts the version-1 `InstallAdmissionBaseline` local command
value after shell completeness/authentication checks. It rechecks formation,
collection limits, strict key ordering and every record through the normal
domain merge path, staging one bounded model copy before commit. Stale source
records and absent source policy cannot erase newer local state; equal-version
conflicts, malformed records or union-capacity failures reject the entire
baseline. One correlated `AdmissionBaselineApplied` or
`AdmissionBaselineRejected` publication prevents a large baseline from exhausting
per-transition effects before reporting its outcome. Applied state is queued
for bounded gossip and remains available to anti-entropy; observers refresh
their projection from the aggregate publication rather than expecting one event
per record. An applied baseline reports `self_removed` if its merged state
excludes the local identity. The shell must handle this as ejection, never as
permission to install credentials or mark catch-up complete. No raw token,
transport authorization, IO-network rule parser or introducer-readiness flag
is added to the pure core. Owner integration applies the readiness and
self-ejection checks described above; this does not imply full formation
conformance.

The worker's outbound IO adapter now has a distinct admitted-member handshake
path. Its secret-free target snapshot comes from the current member record,
not join material: same formation, non-self assigned node, Alive/Suspected
liveness, certificate pin, and one to eight literal unicast IP endpoints of at
most 128 bytes each with nonzero ports. Invalid endpoint claims fail closed;
DNS and redirects remain unsupported. It sends the assigned local identity,
not an applicant label, and never sends a join token. It shares the existing
four-attempt dial budget, 15-second total deadline, five-second TLS/frame
deadlines and bounded exchange pool with bootstrap IO. Cancelled/dropped
unregistered results close their connections.

The owner must still validate the returned member ACK against its current
generation, formation, membership, certificate and source restrictions before
registering it; a routing snapshot is not current authorization. The real QUIC
admission/traffic test now uses this adapter after adoption and confirms SWIM
and policy exchanges through registered sessions. The runtime scheduling
contract below connects automatic member repair for the supported PoC topology;
an IO helper alone is not evidence of autonomous worker reconnection.

#### PoC reconnect scheduling

Ordinarily, when both members advertise usable peer endpoints, only the lower assigned
`NodeId` initiates a missing member connection. A local worker without an
advertised endpoint initiates toward the remote advertised endpoint regardless
of ID ordering. No route is invented when neither side advertises one. This
uses immutable assigned identities, not labels or connection arrival order;
it leaves the registry's first-live-binding rule intact. It is not automatic
admission of an unknown worker. Stable advertisements and all-to-all reachability
are the supported three-worker profile; asymmetric reachability and live
advertisement/topology changes require further collision/fallback evidence.

After formation adoption, one exception prioritizes a fresh admitted handshake
to the original introducer, which already knows the assigned joiner. The hint
retains only its node ID/certificate pin, is checked against current membership,
and is consumed by one maintenance query without advancing the ordinary cursor.
It may dial against the usual ID direction; subsequent attempts use the normal
scan. It does not retain the old session, send a join token, renew admission or
catch-up budgets, or bypass current-owner handshake validation. Leave/ejection
and successful catch-up clear the hint. Publishing adoption wakes this first
scan through a coalesced IO notification instead of waiting for the next tick;
the owner still constructs and validates the plan. The real-QUIC regression exercises
adoption with the introducer sorting before the assigned joiner and proves both
the first preference and return to the canonical rule.

One worker-local maintenance loop ticks every second, skipping missed ticks.
It makes at most four owner queries per tick to use the existing four shared
handshake slots, stopping on an empty plan, generation mismatch or unavailable
owner. The queries share one one-second preparation deadline. Each owner query
scans at most 64 ordered member records, returning at most one
missing canonical route and a continuation cursor; a completed scan wraps to
the beginning. Selection is permitted while standalone, catching up or joined,
but does not establish introducer readiness. The runtime retains at most 64
connection/attempt tasks, with at most one task per target node. These share
the four concurrent handshake permits with operator bootstrap, not a new dial
pool. A failed attempt releases its target slot and retries only on a later
cursor visit: there is no immediate retry loop or offline send queue. The
one-second scheduler cadence is this PoC's fixed minimum retry spacing, not an
exponential fleet-scale retry policy. Active target IDs remain held throughout
the batch, so a quickly failed job cannot be retried within that batch.
Owner-query batch wait is bounded to one second;
member ACK registration adds at most five seconds to the 15-second dial budget.

Generation changes cancel/drain old tasks and reset cursors. Owner registration
still rejects late completions; session retirement closes connections held by
IO tasks. Shutdown excludes and aborts maintenance before stopping the owner.
Established outgoing connections use the same bounded stream/datagram pump as
incoming ones. Transport loss only retires a session: it cannot write a
membership tombstone or mark a member alive. Periodic peer selection includes
Alive and Suspected members so a restored route can carry SWIM/refutation and
anti-entropy; Dead and Left members remain excluded. Catch-up and ejection
authority are unchanged.

For anti-entropy specifically, peer selection uses only currently registered,
authorized member routes. Choosing an absent route would start an unsent round
and unnecessarily occupy its deadline. No route yields no round; a later
periodic selection can use a recovered connection. SWIM selection still includes
disconnected Alive/Suspected members, so this does not suppress failure detection
or manufacture an Alive result. A connection lost after selection still follows
the ordinary bounded exchange failure/timeout path.

The current PoC owner schedules repair on its skipped-tick one-second probe
wakeup. While membership gossip is queued, it attempts anti-entropy at most
once per second; with an empty queue, the minimum spacing is five seconds.
An active core round is never overlapped or given a renewed deadline. Missed
wakeups do not produce catch-up bursts. This fixed IO-shell policy is independent
of telemetry features and uses the existing authorized-route selection, round
correlation, byte/count limits and timeout. Continuous valid membership news
can keep the one-second rate active; bounded per-round work does not make that
extra traffic free. The later configurable defaults table is a design contract,
not an implemented override of this PoC scheduler.
The [sparse-chain repair evidence](measurements/formation-adaptive-repair-2026-09-10.md)
records its regression, real-worker timing comparison and bandwidth tradeoff.

The real-runtime regression
`simultaneous_admitted_dials_recover_crossed_connections_without_readmission`
holds both outgoing handshake replies until both incoming sessions register.
Both outgoing registrations are then rejected as duplicates and both crossed
connections close. Normal maintenance restores policy and reliable exchanges,
then repairs a second deliberate transport loss, with unchanged formation,
assigned IDs and certificates and no tombstones. The fixture uses two real
QUIC workers in one process after adoption, while the joiner is catching up;
it is not partition/heal or arbitrary-topology evidence. Ordinary connection
loss is also exercised by the catch-up lifecycle fixture.

The formation owner has a bounded inbound/outbound application-session registry:
64 total retained entries, at most 16 provisional bindings shared by incoming
applicants and outgoing introducers, and a 10-second provisional lifetime.
Optional [registry gauges](orishu-observability.md#registered-session-capacity-gauges)
observe those budgets without changing them or establishing membership.
Session IDs increase for the process lifetime
and are not reset by formation adoption/leave. A second live connection using
the same certificate is refused without replacing the first; a reconnect may
register after the old connection has closed and been pruned. This is not
interrupted-join outcome recovery, which remains a separate admission contract.

Handshake registration uses the 64-entry peer lane, not reserved operator or
completion capacity. Its payload is capped at 4,096 bytes before enqueueing.
The serialized owner validates TLS facts and the handshake against its current
generation/model. After transitions and periodic wakeups it prunes closed,
expired or invalid bindings; removal and formation changes close held QUIC
connections even if an IO task retains a clone. Shutdown also closes queued
handshake connections that were never registered. The worker's `peer::server`
dispatcher additionally caps inbound connection tasks at 64 and concurrent TLS
handshakes at 16, refusing excess arrivals without a permit-wait queue. TLS
and the first application handshake each have a five-second deadline; the
first accepted bidi stream must have index zero. Each registered connection
has at most 16 incoming stream tasks, sharing the owner's 64-exchange budget
with outbound reliable work. Shutdown/abort closes the endpoint and its
connections even if another caller retains an endpoint clone. These adapter
limits are implemented but not yet exposed through worker startup configuration.

Voluntary departure effects belong to the old formation even though the pure
leave transition returns a replacement standalone model. The owner captures
only the bounded live notification candidates and their authorized routes
before that transition, then encodes/submits its `Leave` datagrams with the old
formation and sender before retiring sessions. It does not queue these effects
for delivery through the replacement formation or retry them after retirement.
The dedicated encoding path refuses other message kinds or a leave target
different from the old sender. Submission is non-blocking best effort: missing
routes, rejected/oversized datagrams and transport failure increment the bounded
send-failure diagnostic. Session closure may also discard a queued datagram;
no receipt or network flush is promised. Survivors must therefore converge via
announcement dissemination or ordinary SWIM. The internal codec regression
checks actual CBOR identity after model replacement. The public three-worker
CLI journey now verifies departure, survivor dead-state convergence, readmission
with the retained certificate/new assigned ID, and historical leave replay after
readmission. Deliberately dropped-announcement and interrupted-transition fault
evidence remain required; successful submission alone does not prove receipt.

Real QUIC tests now exercise this owner path, including post-handshake packets
and standalone credential verification. The executable peer listener and
automatic introducer catch-up are connected as described above; handshake registration alone grants no membership
or introducer readiness. The registry now checks active network blocks against
QUIC's current transport-observed IP before registration and on each owner
sweep. Lifted (`Allow`) entries do not block; malformed or oversized active
network policy fails closed. A real endpoint-rebind test verifies closure after
migration to a blocked source IP, plus rule lifting and invalid-policy closure.
Registered packets now enter a bounded owner RPC carrying the IO-owned session
ID, lifecycle generation, transport class and raw frame. The stream/datagram
byte cap is checked before enqueueing on the shared 64-entry peer lane. At
dequeue time the registry rechecks the current source, session lifetime and
membership binding before the wire decoder constructs a core input; validation
and the core transition occur in the same owner turn. Unknown/stale sessions
and malformed packets cannot reuse a captured context to bypass current policy.

The owner correlates at most one reliable session response with that active
request and returns its encoded bytes to the originating stream adapter.
An absent response is not an acceptance receipt. The owner now holds the fresh
standalone formation's join token in its IO shell, separate from the replayable
core and per-request presented token. For `VerifyCredential`, it rechecks the
correlated registry session, source, name and fingerprint, performs the bounded
constant-time token comparison/network checks, and supplies an explicit core
outcome in the same owner turn. This local check performs no asynchronous IO;
future asynchronous checks must use the reserved completion mechanism.

Real QUIC tests now cover the core's lock refusal, invalid-token refusal, and
successful owner-driven insertion/ACK followed by independent validation and
adoption in the joiner's core. Leave generates a fresh standalone token; a
test proves the previous token cannot admit into that replacement formation.
Adoption clears the abandoned token and requires target credential catch-up;
it cannot retain the previous formation's admission authority.

Admission now upgrades the introducer's session to the inserted member identity
and removes its provisional expiry. Known-member reconnect ACKs bind against
the current formation's certificate records. Member-destination effects resolve
only those live, source-authorized sessions: datagrams use QUIC datagrams with
no stream fallback; reliable requests use at most 64 in-flight tasks and the
existing operation-wide deadline. Reliable responses re-enter the owner for
current-generation/session validation. PullReply effects return on the active
request stream, not an unrelated connection stream.

Missing routes, send-capacity exhaustion and transport errors increment bounded
local failure counters; they do not claim delivery or kill the membership owner.
Core probe/reconciliation timeouts continue to govern recovery. There is no
additional immediate reconciliation-round cancellation solely because its peer
departs or its transport fails: its existing ten-second round timer can remain
pending. The PoC owner's idle reconciliation spacing is five seconds (one
second while news is queued), so repair can still wait for the next maintenance
wakeup after that timeout. The historical idle-cadence real-wire test
`departed_reconciliation_round_can_delay_learning_a_readmitted_identity`
establishes three live original records, holds a pull to the departing peer,
delivers its authenticated departure, answers survivor probes without gossip,
and then serves the missing new identity through the next pull. It observes
the record absent after ten seconds and learned at about fourteen seconds.
This proves the timing counterexample, not that every historical process
failure had that cause. The process harness therefore uses a seventeen-second
readmission observation budget (ten plus five plus two seconds of scheduling
margin), not a production SLO or a new transport timeout.

Bounded automatic outbound dialing follows the reconnect scheduling contract
above. Formation changes abort outstanding reliable
tasks, and registry retirement closes their connections. IO completions share
the owner's completion processing quota; shutdown remains reserved.

The real-QUIC integration test now reconnects after admission and runs two
owners through SWIM traffic, reliable reconciliation replies and lock/unlock
convergence in both directions. The second owner is initialized from the
validated adopted core in the test, not joined through the production client
API. Its sustained exchanges now use the production registered-connection
dispatcher, not test-only receive pumps. Executable listener, dialing and
introducer catch-up progress is recorded in the
[formation conformance ledger](tasks/cluster-formation-conformance.md);
this two-owner fixture alone is not the three-worker process gate.
The dispatcher uses this owner RPC rather than constructing trusted peer
contexts itself or relying on periodic cleanup as a per-message gate.

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

`engines` advertises the graph profiles, runtime engines and component
lifecycles the node can actually enforce. The admitted design uses graph
profile `orishu.workload-graph/v1`, engine `wasm-component`, and lifecycle
`orishu.component/v1`; another engine requires a new architectural decision.
See [protocol-workload.md](protocol-workload.md#runtime-engine).

### EngineCapability
```
{
  "engine":             <string>,     -- "wasm-component"
  "componentLifecycles": [<string>],  -- includes "orishu.component/v1"
  "workloadGraphProfiles": [<string>] -- includes "orishu.workload-graph/v1"
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
entry, `key` is its rendered blocklist key), and `MembershipPolicyUpdate`
(`data` is the singleton policy, `key` is `membership`). These converge through the
membership merge rules above.

**Every other delta type is not membership's.** A membership implementation
relays `WorkloadUpdate`, `CheckpointUpdate`, `ResultUpdate`, and `AuditEvent`
to their owning subsystem without decoding `data`, and must not store any part
of them in membership state. An unrecognized `deltaType` is relayed the same
way rather than rejected, so a subsystem can add one without a membership
change.

### Replicated membership policy (formation PoC)

`MembershipPolicyUpdate` carries the singleton `{ "locked": bool, "version":
VersionTuple }` under key `membership`. Absence means the initial unlocked
policy; it is distinct from an explicit versioned unlock. Local peer admission,
capacity and protocol compatibility remain node-local configuration. An
authenticated operator command advances the last observed policy version with
the local actor, refusing counter overflow. Exact replay is idempotent,
equal-version differing payloads are conflicts, and newer versions win.
Admission and operator removal consult the replicated lock. Adoption/leave
discard the previous formation's policy; introduction remains gated by the
shell until target admission-state catch-up is complete.

The policy uses canonical leaf key `0x04 || UTF-8("membership")`; its value is
`0x04 || version(epoch, counter, actor) || u8(locked)`, with the existing
length-prefixed actor encoding. This extension uses membership hash domains
`orishu.membership.{leaf,bucket,node}/2`. The formation transport must negotiate
this policy-aware profile and reject older profiles before exchanging digests.
There is no implicit hash fallback or mixed-profile cluster. Existing entity
value layouts are unchanged. Policy is included in gossip and bounded
anti-entropy, never credentials. Lock success means local acceptance, with
cluster-wide effect after convergence, not a synchronous partition-proof fence.

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
| `0x04` | membership policy | literal `membership` |

The tag keeps the namespaces disjoint, so a member and a tombstone for the same
node cannot collide onto one leaf.

**Leaf hash.** `SHA-256("orishu.membership.leaf/2" || u32(len(key)) || key ||
canonical-value-encoding)`.

**Bucket assignment.** `SHA-256("orishu.membership.bucket/2" || key)`, of which
the top `depth` bits of the first two bytes give the bucket index. Hashing
rather than taking the key directly spreads sequentially named nodes evenly, so
one divergent entry lands in one bucket rather than smearing across all of them.

**Bucket hash.**
`SHA-256("orishu.membership.bucket/2" || u32(count) || leaf hashes in ascending
leaf-key order)`. Sorting is what makes the result independent of insertion
history. An empty bucket hashes its zero count and is not skipped.

**Tree.** A complete binary tree over exactly `2^depth` buckets, folded
pairwise as `SHA-256("orishu.membership.node/2" || left || right)` up to the
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
  "componentGraphHash": <bytes>,         -- digest of admitted instances/channels/plan
  "componentInstanceId": <string>,
  "phaseId":            <string>,
  "componentCodeHash":  <bytes>,         -- digest of the invoked WASM Component
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
  "attemptId":        <string>,          -- Stable shell correlation across retries; not assigned membership identity
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
- If an authenticated subject's `Ping` or correctly correlated direct `Ack`
  leaves that subject locally `Suspected`, send one bounded `Announce(Suspect)`
  datagram directly back to the subject with the held suspicion incarnation.
  The original notification/gossip may have been lost while its route was
  unavailable. This reminder neither clears suspicion nor renews its timer;
  only a newer subject refutation changes the state. Do not generate reminders
  from unmatched ACKs, indirect third-party reports or terminal `Dead` records.
  This uses the existing announcement wire contract and adds at most one
  datagram per validated direct contact, with no new queue or timer.
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

Ownership transfer proposals for a component-instance partition. Different
components may use different compatible decompositions and placements. Sent
over a dedicated bidirectional stream. See [partition ownership](./orishu-runtime-design.md#partition-ownership-and-rebalancing).

```
PartitionIntent payload:
{
  "workloadEpoch":    <uint64>,
  "componentInstanceId": <string>,
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
  "componentInstanceId": <string>,
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
  "componentInstanceId": <string>,     -- owner of this field/state channel
  "channelId":       <string>,
  "step":            <uint64>,           -- Step number this halo data is for
  "sourcePartition": <string>,           -- Partition ID that produced this halo data
  "targetPartition": <string>,           -- Partition ID that needs this halo data
  "face":            <HaloFace>,         -- Which face of the target partition this covers
  "haloDepth":       <uint>,             -- Number of cell layers in this halo
  "data":            <bytes>             -- Serialized boundary cell values
}
```

**Behavioral rules:**

- The `(workloadEpoch, componentInstanceId, channelId, step,
  sourcePartition, targetPartition, face)` tuple uniquely identifies a halo exchange.
- Both directions of a partition boundary use the same bidirectional stream.
- If the stream is reset or the connection is lost, the receiver cannot compute the next step for the affected partition. The SWIM failure detector will eventually mark the peer as failed.
- Halo data must be received for ALL faces before a partition can compute its next step.
- Workload messages also carry piggybacked gossip in the `MessageEnvelope.gossip` field. If a peer accepted a workload message, it was alive at that instant, making a separate `Ping` unnecessary.

### ComponentChannelData

Reliable transfer of an admitted typed channel when a producer and consumer
component invocation are placed on different workers.

```
ComponentChannelData payload:
{
  "workloadEpoch":       <uint64>,
  "step":                <uint64>,
  "invocationId":        <string>,
  "producerInstanceId":  <string>,
  "producerPartitionId": <string>,
  "consumerInstanceId":  <string>,
  "consumerPartitionId": <string>,
  "channelId":           <string>,
  "schemaId":            <string>,
  "coverage":            <map>,
  "chunkIndex":          <uint>,
  "chunkCount":          <uint>,
  "data":                <bytes>,
  "contentHash":         <bytes>
}
```

The receiver accepts data only for the exact admitted graph, boundary,
invocation dependency, channel schema and producer/consumer partitions. Chunks
are bounded, ordered, digest-verified and complete before the dependent phase
may run. They are correctness-bearing workload data: never coalesced, skipped
or treated as observer projections.

---

### StepVote / StepCommit

Step barrier coordination. Sent over bidirectional QUIC streams.

```
StepVote payload:
{
  "workloadEpoch": <uint64>,
  "step":          <uint64>,             -- Step number that this node has completed
  "executions": [{
    "componentInstanceId": <string>,
    "partitionId": <string>,
    "invocations": [<string>, ...],
    "stateHashes": { <channelId>: <bytes>, ... }
  }]
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
  1. Received every required halo and cross-component channel for its assigned
     invocations at step `n`.
  2. Completed and validated all assigned plan nodes/component partitions.
  3. Sent every required outgoing halo and component channel to admitted
     consumers.
- `StepCommit` is disseminated only after every required invocation/partition
  in the admitted plan is covered by compatible votes and the complete
  candidate validates. This confirmation is epidemic.
- If an owner disappears before voting, the step cannot commit. The cluster
  retries from the last committed boundary using complete component/runtime
  checkpoint state or replicated partition state.
- `stateHashes` enable verification of each component-owned committed channel.
- For speculative execution, the first valid result for an exact `(epoch,
  step, invocationId, componentInstanceId, partitionId)` wins.

---

### CheckpointReq / CheckpointReply

Checkpoint and catch-up state transfer. Sent over a dedicated bidirectional stream.

```
CheckpointReq payload:
{
  "workloadEpoch": <uint64>,
  "componentInstanceId": <string>,
  "componentPartitionId": <string>,
  "checkpointId":  <string | null>,      -- Specific checkpoint to fetch (null = latest)
  "step":          <uint64 | null>       -- Specific step boundary (null = latest available)
}
```

```
CheckpointReply payload:
{
  "workloadEpoch":   <uint64>,
  "componentInstanceId": <string>,
  "componentPartitionId": <string>,
  "checkpointId":    <string>,
  "step":            <uint64>,           -- Step number at which this checkpoint was taken
  "simulationTime":  <float>,
  "partitionMap":    <map>,              -- Partition ownership at checkpoint time
  "stateSchema":     <string>,
  "data":            <bytes>,            -- one component-partition checkpoint part
  "contentHash":     <bytes>,
  "provenance":      [<StepProvenance>, ...]  -- Provenance chain up to this checkpoint
}
```

**Behavioral rules:**

- Used by late-joining nodes to catch up with the current simulation state before they can own future steps.
- Used during partition reassignment when a new owner needs the current state.
- The stream supports flow control — a large component part may span multiple QUIC dataframes.
- The receiver verifies identity/schema/content before staging the part. A run
  restores or reassigns only after every required component and runtime part at
  the boundary is present; no individual reply is a complete checkpoint.
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
- **Cluster isolation:** The `formationId` field in `MessageEnvelope` carries the immutable formation ID under ADR 0013, not a resource's `metadata.uid`. Messages with mismatched formation IDs are silently discarded. This acts as a strict security boundary preventing cross-cluster interference. The legacy human-readable cluster name is not a security boundary and is no longer used as a wire guard.
- **Join token scoping:** Join tokens authorize cluster admission only. They must not authorize any other operation (operator APIs, workload submission, result access).
- **Certificate pinning:** Certificate fingerprints are pinned in `NodeRecord.certFingerprint` on admission and verified on every reconnection.
- **Partition integrity:** `PartitionIntent` and `PartitionAck` carry cryptographic signatures to prevent spoofed ownership claims that could lead to duplicate writers.
- **Admission control:** The blocklist provides targeted exclusion (by node ID, name, or certificate fingerprint; by host IP or CIDR range; or intersections of both); the membership lock provides global admission control. Both compose correctly and both must be clear for admission to succeed.
- **Rate limiting:** Nodes should rate-limit incoming `JoinReq` messages to prevent admission flooding. Recommended: 10 `JoinReq` per second per source address.
