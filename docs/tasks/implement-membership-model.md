# Implement the sans-IO cluster membership core

Status: **implemented and accepted**; the
[liveness-gossip merge correction](fix-membership-liveness-gossip-merge.md)
that acceptance waited on has landed

Decision: [ADR 0013](../adr/0013-cluster-formation-and-node-identity.md)

Design: [Orishu runtime design](../orishu-runtime-design.md),
[peer protocol](../protocol-p2p.md), and
[functional-core/IO-shell guidance](../Coding%20style.md#the-elm-architecture-model-message-update-with-or-without-a-view)

## Outcome

Each `orishu-worker` can eventually own one local, replicated view of cluster
membership—who the admitted members are and their liveness state—by driving a
deterministic sans-IO core. Network delivery, authenticated connection facts,
credential checks, clocks, timers, entropy, persistence, and process lifecycle
enter as typed inputs or effect outcomes. The core returns a new model and
typed effects; it never opens a socket, sleeps, reads a clock, generates random
identity, or hides work in an async task.

The core covers formation-scoped admission, SWIM probing and suspicion,
membership announcements, membership gossip, and bounded anti-entropy. It does
not integrate with `apps/orishu-worker` in this task.

## Roadmap placement and dependencies

This is the **N-MEMBERSHIP-CORE** work package: an early slice of the Orishu
network/cluster lane that may run in late Milestone 1 or in parallel with
Milestone 2. It is not the N-FORMATION transport/runtime integration package.

```text
S-IDENTITY
formation ID + formation-assigned node ID + membership tombstone identity
        |
        v
N-MEMBERSHIP-CORE                 <- this task
pure membership transitions + effects
        |
        v
N-FORMATION                       <- next operational slice
QUIC, mTLS, codecs, timer wheel, worker adapter and multi-process admin tests
        |
        v
N-CLUSTER
partition ownership, halos, step votes and commits
```

The task consumes the membership portion of S-IDENTITY. If the explicit shared
`FormationId`, formation-assigned `NodeId`, worker-name/cluster-name label,
and membership-tombstone types have not landed, add only those shared types as
a separate first commit coordinated with the S-IDENTITY owner. Do not create a
membership-private `FormationId`, use `Option<String>` as identity, or expand
this task into run/observation identity.

No part of this task depends on O-RUNTIME, workload schemas, partitioning,
artifact transfer, or Kagami.

## What landed

`crates/orishu-identity` carries the shared identity contract — `FormationId`,
formation-assigned `NodeId`, the `ClusterName`/`WorkerName` labels,
`CertFingerprint`, `VersionTuple`, `Incarnation`, `RemovalMode`, and
`MembershipTombstone`. It exists as its own crate because `crates/orishu`
carries an HTTP client (and with it `reqwest`, `tokio`, and `chrono`) while the
membership core must have none of those; `orishu` re-exports `NodeId` and
`RemovalMode` from it, so there is one definition rather than two.

`crates/orishu-membership` implements the deterministic core: admission, join
adoption, SWIM direct and indirect probing, announce and refutation, gossip
merge, and bounded resumable anti-entropy, behind
`update(model, message) -> Transition`. `tests/dependencies.rs` resolves the
real dependency graph and fails if a networking, async-runtime, clock,
filesystem, TLS, or RNG crate appears.

The implementation was reviewed on 2026-09-05. Package tests, formatting,
strict Clippy, dependency-purity tests, wire fixtures, and documentation checks
all pass, and the identity/effect/admission/probe/removal boundaries match this
task. That review found one merge-path defect not covered by the suite at the
time: a SWIM announcement changes liveness/incarnation while retaining the
member's descriptive `VersionTuple`, but `merge_member` rejected any
same-version non-identical record as a conflict before evaluating the
independent SWIM ordering, so a direct `Announce` worked while the full record
subsequently carried through gossip or anti-entropy could be rejected by a
third node instead of propagating suspicion or death.

The [liveness-propagation follow-up](fix-membership-liveness-gossip-merge.md)
corrected that: `merge_member` now orders the descriptive projection and the
SWIM liveness pair independently, and one merge may adopt liveness while
reporting a descriptive conflict. Its three-node gossip and anti-entropy
regressions, the independent-ordering matrix, and the safety regressions live
in `tests/convergence.rs`. Cluster-wide policy and the IO shell remain separate
work tracked by [N-FORMATION](implement-cluster-formation-poc.md).

The preflight decisions were recorded in `docs/protocol-p2p.md` before the code
depended on them: probe correlation IDs on `Ping`/`Ack`/`PingReq`/`PingReply`
plus timer generations; a total order and explicit conflict rule for
`VersionTuple`; the removal/liveness split including versioned tombstone
clearing; the full canonical anti-entropy algorithm (leaf keys, leaf and bucket
hashing, tree layout, subtree addressing, continuation cursors, and round
bounds); and the reconciled admission gate list with its evaluation order and
post-verification re-check. Two corrections to the existing contract came out of
implementing it: `Suspect(n)` must override `Alive(m)` at `n >= m` rather than
`n > m` — the documented rule made suspicion unreachable, since a probe times
out against the target's current incarnation — and a `GossipDelta`'s `data`
must carry the complete record rather than a field diff, because a partial diff
can be neither canonically hashed nor idempotently merged.

Golden JSON fixtures for the wire-visible types live in
`crates/orishu-membership/tests/fixtures/`, and `benches/membership.rs` profiles
the core in isolation.

### Deliberately not closed

- **The worker lock adapter is not wired.** N-FORMATION now adds a versioned
  `MembershipPolicy` to core gossip and anti-entropy, separate from node-local
  `AdmissionPolicy`. Core convergence, conflict, overflow and admission tests
  cover it; the worker/API and policy-aware transport/catch-up contract remain
  [N-FORMATION](implement-cluster-formation-poc.md#3-cluster-wide-membership-policy)
  work. Hash domains advance to version 2; mixed-profile transport is unsupported.
- **`swim.antiEntropyRounds` and `swim.antiEntropyDepth`** are new
  configuration keys documented in the peer protocol but not yet wired into the
  worker's configuration loader, which is an N-FORMATION concern.
- Client DTOs still carry `cert_fingerprint: Vec<u8>` rather than
  `CertFingerprint`. Migrating them is a client-protocol change with its own
  compatibility surface, and nothing in the membership core depends on it.

## Original gap

`crates/orishu/src/model/node.rs` and `cluster.rs` contain imported client
DTOs such as `NodeId`, `MemberState`, `NodeCapabilities`, and
`Version { epoch, counter }`. They do not form a membership transition model:

- formation identity is still represented indirectly by a generic optional
  manifest ID rather than an explicit shared type;
- no model tracks probes, suspicion timers, gossip retirement, join progress,
  or anti-entropy progress;
- no pure update function decides membership transitions; and
- some peer-protocol prose is not precise enough to implement safely, notably
  probe correlation, version-conflict semantics, and canonical Merkle leaves.

The imported DTOs are evidence and compatibility inputs, not permission to
freeze their current shapes into the new core.

## Required preflight decisions

Resolve these narrowly before implementing the transition that relies on
them. Amend the peer protocol when a wire field or algorithm is missing; do
not invent crate-private behavior that a future worker adapter cannot encode.

1. **Identity:** use explicit `FormationId` and formation-assigned `NodeId`.
   Names remain non-unique labels. Define the one-node standalone formation
   constructor and the atomic transition that abandons its formation-scoped
   state when a joiner adopts another formation and its newly assigned ID.
2. **Probe correlation:** add or confirm correlation identities for direct
   `Ping`/`Ack`, indirect `PingReq`/`PingReply`, and their timeout events.
   Target ID alone is insufficient when messages are duplicated, reordered, or
   several probes overlap.
3. **Version merge:** define one shared formation-scoped version type and its
   total ordering. Exact replay of the same version and payload is idempotent;
   the same version with different payload is a structured conflict, not a
   silent tie.
4. **Removal:** keep SWIM liveness (`Alive`, `Suspected`, `Dead`) distinct
   from operator removal. A membership tombstone cannot be cleared or
   overridden by a later `Alive` announcement. Voluntary self-`Leave` does
   not create the operator-removal tombstone described by the client protocol.
5. **Anti-entropy:** specify canonical leaf keys/bytes, hash-tree layout,
   subtree addressing, continuation, and batch limits before implementing a
   Merkle comparison. A `MerkleDigest` struct alone is not an algorithm.
6. **Admission:** reconcile the shorter peer-protocol checklist with the
   complete runtime-design gates: authenticated transport, formation/join
   intent, current join token, membership lock, introducer peer-admission flag,
   capacity, blocklist, membership tombstone, and protocol compatibility.

## Implementation slices

### 1. Shared identity and wire preflight

- Complete only the membership-related S-IDENTITY prerequisites above and add
  JSON/CBOR fixtures where they are wire-visible.
- Replace brittle line-number assumptions with protocol headings and golden
  fixtures.
- Record the probe, merge, removal, anti-entropy, and admission answers in the
  peer protocol before core code depends on them.
- Keep protocol framing, CBOR decoding, QUIC, and mTLS implementation out of
  this task.

### 2. Crate scaffold: `crates/orishu-membership`

- Add a workspace library with cohesive modules such as `model`, `message`,
  `effect`, `update`, `admission`, and `limits`. Prefer fewer modules if
  the implementation remains small; the filenames are not an acceptance
  criterion.
- Reuse explicit shared identity and public protocol-domain types from
  `crates/orishu`. Do not duplicate them merely to avoid coordinating a
  shared contract change.
- Allow only deterministic domain dependencies. Do not add Tokio, async traits,
  QUIC, socket, TLS, filesystem, wall-clock, random-number-generator, or global
  executor dependencies.
- Do not serialize or expose raw join tokens, private keys, or other secrets as
  part of the replayable membership model.

### 3. Model

Represent at least:

- the current non-optional `FormationId`, cluster-name label, local `NodeId`,
  worker-name label, local incarnation, and outbound sequence state;
- admitted members keyed only by `NodeId`, with separately typed labels,
  certificate fingerprint, liveness/incarnation, version, advertised endpoint
  metadata, accepts/limits, and capabilities;
- membership tombstones separately from SWIM liveness;
- the admission-policy view required by the pure decision, including
  membership lock, capacity, blocklist/tombstone matches, and protocol range,
  without retaining credential secrets;
- pending direct/indirect probes keyed by a correlation ID, including the exact
  timer generation/token whose expiry remains actionable;
- pending join/admission transitions, anti-entropy continuation state, and a
  bounded gossip-retirement queue; and
- typed limits/configuration validated at construction.

Formation replacement must not merge the old standalone formation's members,
locks, tombstones, probe state, or versions into the target formation. A
bootstrap membership snapshot is bounded, validated input that seeds a local
view and then converges normally; it is not permanent introducer authority.

Use owned values and ordinary local mutation inside `update` to avoid cloning
the whole membership map. Observable purity means the result depends only on
the input model and message, not that every internal operation must copy.

### 4. Messages and effect outcomes

Use one closed input enum for domain facts. Its exact Rust spelling may evolve,
but it must distinguish:

- authenticated inbound context: opaque peer/session handle, certificate
  fingerprint, claimed sender, formation, protocol version, and sequence;
- join request/reply and validated admission evidence;
- `Ping`, `Ack`, `PingReq`, `PingReply`, and `Announce` bodies;
- bounded membership/blocklist gossip and `PullReq`/`PullReply` anti-entropy;
- local commands such as begin join or voluntary leave;
- correlated direct-probe, indirect-probe, suspicion, anti-entropy, and retry
  timer expiries; and
- correlated outcomes for requested entropy, generated IDs, credential
  verification, or other effects.

Piggybacked gossip should enter one shared merge path regardless of its carrier.
Non-membership gossip is returned or ignored through an explicit boundary; it
must not be decoded into membership-owned state.

The core must recheck formation and sender binding for every post-admission
input. A future decoder should reject mismatches early, but correctness must not
depend on every adapter remembering an undocumented precondition. Pre-admission
join traffic uses the authenticated worker label/certificate/session context,
not a fabricated target-formation `NodeId`.

### 5. Effects

Return typed effects for the imperative shell, including:

- send a semantic peer message to an admitted `NodeId` or reply on an opaque
  pre-admission session handle;
- arm or cancel a timer identified by a stable timer token/generation;
- request bounded peer selection/entropy and receive the selection as a later
  correlated input;
- request a new opaque node ID for successful admission and receive it as a
  later correlated input;
- request credential verification without placing the secret in the model or
  a diagnostic effect; and
- publish a structured membership/admission change for future audit, metrics,
  or owner adapters without performing that IO.

An effect is requested work, not evidence that it succeeded. `update` adopts
an effect result only through its corresponding input. The core must never call
a random API to choose a probe peer or generate a node ID; doing so would make
replay nondeterministic.

### 6. Update, admission, and SWIM transitions

Expose a function equivalent to:

```rust,ignore
pub fn update(model: Membership, message: Message) -> Transition {
    // Transition contains the next model plus bounded effects/diagnostics.
}
```

- **Admission:** decide all gates from the runtime design. Authentication,
  cryptographic token comparison, and network/CIDR matching may arrive as
  correlated verified facts from the shell; the core still owns the atomic
  accept/reject/redirect decision. Successful admission requests a fresh node
  ID, rejects collision, inserts exactly one member, and only then replies and
  gossips. Redirects are bounded untrusted hints, not authority.
- **Join adoption:** validate ACK identity, formation, assigned-ID uniqueness,
  self entry/certificate consistency, protocol compatibility, and bounded
  snapshot contents before atomically adopting the target formation. Pass
  non-membership bootstrap data to its eventual owner rather than storing it
  here.
- **SWIM:** implement the complete correlated direct-probe → indirect-probe →
  suspicion → dead sequence. Stale timer events and late replies cannot affect
  a newer probe. Unknown senders, spoofed self-leaves, cross-formation input,
  and invalid state/incarnation transitions are rejected or ignored with a
  structured reason.
- **Refutation:** a local node that learns it is suspected requests/broadcasts
  `Alive` at the next valid incarnation without allowing overflow.
- **Gossip:** merge through one deterministic helper. Exact replay is a no-op;
  older/out-of-formation data cannot regress state; same-version conflicting
  payloads produce a diagnostic conflict; membership tombstones fence removed
  IDs against liveness resurrection.
- **Anti-entropy:** operate in bounded, resumable rounds using the preflight's
  canonical tree/continuation rules. Do not claim complete convergence from a
  truncated reply.

### 7. Bounds and hostile-input behavior

- Define limits for members, snapshot bytes/items, gossip items/bytes/hops,
  anti-entropy tree/deltas/rounds, addresses, labels, fingerprints,
  capabilities, redirects, pending probes, pending joins, timers, effects, and
  diagnostics.
- Validate checked arithmetic and bounds before state adoption. The future wire
  decoder must also bound allocation while decoding; core validation is
  defense-in-depth and cannot retroactively make an already allocated `Vec`
  safe.
- Cap effects produced by one update. One hostile message must not cause
  unbounded fan-out, timers, cloning, hashing, or diagnostics.
- Treat malformed identity, sequence, formation, version, self-announcement,
  and correlation data as structured rejection rather than panic.

### 8. Tests

- Pure transition tests feed message/effect-outcome sequences and assert the
  next model, effects, and structured diagnostics without sockets, sleeps,
  clocks, entropy, or an async runtime.
- Table-driven tests cover the complete SWIM incarnation/state matrix plus
  tombstone fencing and voluntary-leave rules.
- Admission tests cover every gate independently and in combination, including
  unauthenticated, stale token, incompatible protocol, tombstoned identity,
  blocklisted network/identity, capacity, lock, ID collision, reject, and
  bounded redirect behavior.
- Probe tests cover duplicate/reordered replies, overlapping probes, stale
  timers, indirect success, indirect failure, suspicion refutation, and
  checked-incarnation overflow.
- Gossip/property tests permute duplicate and out-of-order delivery and prove
  idempotence/convergence for non-conflicting updates; conflicting equal
  versions remain explicit.
- Bootstrap tests reject wrong formation, missing/mismatched self entry,
  duplicate IDs, inconsistent certificate identity, oversized snapshots, and
  leakage from the abandoned standalone formation.
- Anti-entropy golden tests cover canonical digest stability, continuation,
  truncation, malformed trees, and bounded convergence.
- Dependency tests or manifest inspection ensure the crate has no networking,
  async-runtime, clock, filesystem, or RNG dependency.

## Acceptance criteria

- The membership-related S-IDENTITY and peer-wire preflight is reviewed and
  recorded; the crate contains no private substitute for formation identity or
  version ordering.
- `crates/orishu-membership` builds with no networking, async-runtime, clock,
  filesystem, TLS, or RNG dependency and contains no raw credential secret.
- The deterministic update API covers admission, join adoption, SWIM direct and
  indirect probing, announce/refutation, membership gossip, anti-entropy, and
  all correlated timer/effect outcomes required to drive them.
- A fresh worker has an explicit one-node standalone formation. Joining another
  formation atomically adopts its generated formation ID and newly assigned
  node ID without importing old formation-scoped state.
- Labels never satisfy identity checks. Post-admission messages are scoped to
  the formation and authenticated sender in both core tests and the documented
  future decoder boundary.
- Admission covers every runtime-design gate and never reports acceptance
  before ID generation/collision checking and member insertion succeed.
- SWIM transitions are correlation-safe under loss, duplication, reordering,
  stale timers, overlapping probes, and checked-incarnation limits.
- Membership removal/tombstones cannot be undone by SWIM. Voluntary leave and
  operator removal retain their distinct meanings.
- Merge and anti-entropy behavior is canonical, bounded, resumable, idempotent
  for exact replay, convergent for the specified non-conflicting CRDT inputs,
  and explicit about equal-version conflicts. Member description and SWIM
  liveness are ordered independently, so a liveness change still propagates
  through a full record at an unchanged descriptive version.
- The core revalidates all logical limits and caps emitted work; the task also
  documents which allocation bounds remain mandatory in the future decoder.
- `cargo test -p orishu-membership`, `cargo fmt --all -- --check`, and
  `cargo clippy --locked -p orishu-membership --all-targets -- -D warnings`
  pass.

## Non-goals

- Integration with `apps/orishu-worker`, including its event loop, task
  supervision, configuration loader, persistence, metrics, or shutdown path.
- Wire framing/CBOR codecs, QUIC transport, mTLS, socket/address resolution, a
  timer wheel, or OS entropy. This task may correct the protocol contract and
  add golden wire fixtures, but it does not implement the IO shell.
- Partition ownership, halo exchange, step votes/commits, workload state,
  checkpoints, `FetchChunk`, artifact availability/replication, or purge
  inventory reconciliation.
- Treating non-membership gossip (`WorkloadUpdate`, checkpoint/result records,
  audit data) as membership-owned state.
- A durable authored cluster manifest. Formation membership is transient;
  restart/persistence semantics require their owning task.
- Automatic discovery or admission, including mDNS. MVP admission remains
  explicit and operator-led.
- Choosing persistent/structural-sharing collections without measurements.
  Ordinary owned mutation inside a pure transition is sufficient initially.
