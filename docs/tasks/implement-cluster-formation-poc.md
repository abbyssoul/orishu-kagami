# Implement the operational cluster-formation PoC

Status: **ready after N-MEMBERSHIP acceptance**

Decision: [ADR 0013](../adr/0013-cluster-formation-and-node-identity.md)

Design: [Orishu runtime design](../orishu-runtime-design.md),
[peer protocol](../protocol-p2p.md),
[client protocol](../protocol-client.md), and
[worker/cluster administrator stories](../user-stories/orishu/README.md)

Prerequisite:
[the implemented sans-IO membership core](implement-membership-model.md),
including its open liveness-gossip merge correction and acceptance review

## Outcome

An operator can start three real `orishu-worker` processes, explicitly join
them into one authenticated formation, and inspect and control that formation
with `orishuctl`. The workers exchange real peer traffic and drive
`orishu-membership`; no workload is loaded and no scientific computation is
performed.

This is the **N-FORMATION** work package. It proves the operational control
plane before N-CLUSTER adds partition ownership, halos, distributed step
commit, artifacts, or compute. A successful demonstration is:

```text
worker-a starts as formation A (one member)
worker-b starts as formation B (one member)
worker-c starts as formation C (one member)
        |
        | explicit join through authenticated peer sessions
        v
worker-a, worker-b, worker-c converge on formation A (three assigned node IDs)
        |
        | local or authenticated remote client API
        v
orishuctl cluster info / ls / inspect / cluster lock / cluster unlock / leave
```

The slice should exercise production seams, not a network simulator: separate
OS processes, QUIC connections, the bounded wire codec, the real membership
driver, the worker's client listener, and the existing CLI command path.

## Roadmap placement and dependencies

```text
N-MEMBERSHIP (implemented core; accept after recorded merge correction)
        |
        +--> peer trust/codec preflight
        +--> cluster-policy reconciliation
        +--> membership subset of O-API-SHAPE
                    |
                    v
             N-FORMATION             <- this task
             real three-worker administrative PoC
                    |
                    v
             N-CLUSTER
             distributed scientific execution
```

N-FORMATION does **not** depend on O-RUNTIME, O-WASM, workload schemas,
partitioning, storage, Kagami, or a physics plugin. It may reuse the worker's
existing client listener and `orishuctl` surface, but only after the
membership/status subset of O-API-SHAPE is made explicit. Unresolved workload,
checkpoint, result, log, event, and audit endpoints do not block this slice.

## Required preflight decisions

Close these narrowly before writing the adapter that depends on them. Record
wire changes in `protocol-p2p.md` and client-resource changes in
`protocol-client.md`; do not hide either contract in worker-private structs.

### 1. Trust bootstrap

Specify how a joiner authenticates the introducer before disclosing the join
token. “Accept any self-signed server certificate and then send the token” is
not acceptable because an active intermediary could steal the admission
credential. Choose and test one explicit mechanism, such as join material that
binds the target formation and introducer certificate fingerprint, or an
operator-provisioned CA/trust root.

The handshake must distinguish a provisional applicant from an admitted
member. After admission, the QUIC session is pinned to the formation-assigned
`NodeId` and certificate fingerprint held by membership. A label, DNS name,
endpoint hint, or claimed envelope field never establishes that binding.

Define the PoC credential lifecycle too: certificate creation/loading, secure
local permissions, the formation join token, and what a restart retains. Token
rotation and certificate rotation may remain deferred, but the CLI must not
claim they work if they do not.

Define a post-admission readiness gate. The core's join snapshot contains
members, while cluster policy, blocklist entries, and tombstones converge
through their owning reconciliation paths. A newly admitted worker must not
act as an introducer until it has obtained and validated the admission-relevant
state needed to enforce every gate; otherwise a removed or blocked identity
could exploit the catch-up window.

### 2. Wire adapter and bounds

Define one versioned wire DTO/codec that maps the peer protocol's envelope and
membership payloads to `orishu-membership` semantic inputs/effects. The raw
join token belongs only in the encrypted wire/credential adapter and must not
enter the replayable core, diagnostics, or ordinary logs.

The decoder must enforce frame, datagram, string, byte, collection, snapshot,
gossip, Merkle, and nesting bounds while decoding, before allocating the full
claimed value. It reports the actual encoded snapshot size to the core. Stream
versus datagram mapping follows message semantics in `protocol-p2p.md`; a
datagram that cannot fit is not silently promoted if doing so changes loss or
ordering semantics.

Keep wire DTOs distinct where the transport carries facts the core
deliberately omits. Do not add `Serialize` to every core enum merely to bypass
the adapter or allow secrets into replay fixtures.

### 3. Cluster-wide membership policy

Replace the core's documented node-local `membershipLocked` limitation before
exposing `orishuctl cluster lock` as a cluster operation. Split cluster-scoped,
versioned membership policy from node-local admission facts such as
`accepts.peers`, local connection capacity, and supported protocol range. The
cluster lock must gossip and reconcile through bounded anti-entropy, and its
version/conflict behavior needs fixtures.

Joining and administrative removal must consult the same converged lock. A
lock accepted through one worker becomes visible from every worker and blocks
admission through every introducer after convergence. Unlock is a newer
cluster-policy transition, not deletion of local state. The raw join token is
secret material and is not part of this gossiped policy.

### 4. Minimal client resource surface

Freeze only the resource shapes needed by this PoC:

- cluster/formation summary, including immutable `formationId`, display
  `clusterName`, member count, membership lock, and explicit “no workload”;
- membership list and one-member inspection, including `NodeId`, worker label,
  liveness/incarnation, role/admission flags, advertised peer endpoints, and
  the source/freshness of a cached view;
- current join material through an appropriately privileged local/admin path;
- begin join on the worker being moved, voluntary leave, lock, and unlock; and
- structured rejection/diagnostic responses for failed joins and stale or
  unauthorized mutations.

Use noun-shaped versioned resources and the authority rules in the runtime
design. The synthetic cluster summary is a projection of the current
formation, not a persisted authored cluster manifest. The client adapter may
reuse compatible imported DTOs, but it must convert explicitly to the shared
identity types instead of keeping a second `NodeId`, removal mode, or
formation representation.

### 5. Ejection and restart semantics

Define what the worker shell does when the core learns that its own assigned
identity has been removed: stop participating in that formation, close its
peer sessions, and expose an actionable local state. Do not leave a process
sending as a tombstoned identity.

For this PoC, a restarted worker may begin a fresh standalone formation and
require explicit readmission with a new formation-assigned ID. If formation
membership or assigned identity is retained across restart instead, persistence
and stale-session fencing become part of this task and require crash/restart
tests. State the chosen behavior in operator output and documentation.

## Implementation slices

### 1. Close the contracts

- Resolve the five preflight areas above and add hostile/golden fixtures.
- Add a versioned, cluster-scoped membership-lock entity to its owning pure
  model and merge path; preserve local admission configuration separately.
- Reconcile the membership subset of the imported `orishu` client DTOs with
  `orishu-identity` rather than translating identity through arbitrary strings.
- Mark unsupported imported CLI operations honestly. In particular, do not
  advertise token/certificate rotation, graceful compute drain, or historical
  audit as implemented by this PoC.

### 2. Peer codec and transport adapter

- Implement bounded CBOR envelope encoding/decoding and length-prefixed stream
  framing, with golden bytes and malformed/truncated/oversized input tests.
- Implement raw QUIC with mTLS, provisional join sessions, admitted session
  pinning, formation guards, stream/datagram routing, connection limits, and
  orderly shutdown.
- Keep QUIC, TLS, sockets, DNS, and codec dependencies outside
  `orishu-membership`. A cohesive `orishu-peer-io` crate is preferred if it
  prevents the worker binary from becoming the only testable owner; the exact
  crate name is not a contract.
- Reject unknown, cross-formation, certificate-mismatched, replayed stream, and
  over-budget input before it can fan out work.

### 3. Membership driver

- Give each worker one serialized owner of `Membership`. All peer input,
  operator commands, timer expiries, and effect outcomes enter its mailbox;
  no handler mutates membership behind the transition function.
- Execute every current effect: send, arm/cancel timer, select eligible peers,
  allocate a cryptographically random collision-checked ID, verify credentials
  and source networks, and publish structured changes/diagnostics.
- Schedule periodic probe and anti-entropy commands. Bound mailbox capacity,
  concurrent streams/sessions, timers, queued sends, and published events;
  define overload behavior instead of relying on unbounded channels.
- Keep foreign gossip explicitly handed off. With no workload owner in this
  milestone it is ignored or reported according to the protocol, never parsed
  into membership state.

### 4. Worker lifecycle and configuration

- Extend the typed configuration path for worker/cluster labels, peer
  listeners and advertised endpoints, `accepts.peers`, limits, trust material,
  and optional explicit join inputs. Preserve file < environment < CLI
  precedence and config-free local startup.
- A fresh worker generates explicit standalone `FormationId` and `NodeId`
  values distinct from labels and immediately exposes truthful one-member
  status.
- Join atomically abandons the old standalone formation only after a validated
  acceptance. It remains in bounded catch-up/not-ready state and cannot
  introduce another worker until admission-relevant policy, blocklist, and
  tombstone state is synchronized. Leave announces departure, closes old
  sessions, generates fresh standalone identities, and reports both old and
  new formation state.
- Keep peer listeners separate from client listeners and never open a remote
  port under the safe default configuration.

### 5. Operator API and `orishuctl`

- Implement the production worker routes and library client methods for the
  minimal resource surface above; remove the placeholder success path for
  those routes.
- Wire the existing CLI concepts for `cluster info`, `ls`, `inspect`, `token`,
  `join`, `cluster lock`, `cluster unlock`, and `leave` to those real routes.
- Make JSON/YAML output stable enough for automation and table output useful
  to a human. Always display formation identity separately from cluster name
  and node identity separately from worker name.
- Local same-user reads may follow the documented Tier 1 policy. Remote reads
  and every mutation require the applicable authenticated authority; peer join
  credentials never authorize the client API.

### 6. Multi-process conformance harness

- Start three worker subprocesses with isolated runtime directories and
  dynamically allocated listeners. Drive formation through `orishuctl` or the
  same public client methods it uses.
- Poll observable state with bounded deadlines; do not use fixed sleeps as
  proof of convergence.
- Capture per-process logs and deterministic test artifacts on failure, while
  redacting join tokens and private-key material.
- Cover duplicate worker/cluster labels, reordered startup, an invalid token,
  a wrong formation/fingerprint, lock/unlock through different entry nodes,
  voluntary leave, process loss and SWIM visibility, and clean shutdown.

## Administrator stories trialled

This PoC provides executable evidence for the membership-only portions of:

- config-free local startup, worker and cluster labels, peer listeners, peer
  capacity, and peer-admission flags;
- list and inspect cluster nodes, and inspect cluster status;
- retrieve current join material and explicitly join a worker;
- lock and unlock cluster membership; and
- voluntarily leave and observe liveness after a worker becomes unreachable.

It does not complete stories whose acceptance requires workload drain,
artifact transfer, durable audit history, rich resource telemetry, packaging,
or distributed computation. Story/CLI documentation must say which fields or
modes remain unavailable rather than filling them with plausible placeholders.

## Acceptance criteria

- The membership core's liveness-gossip merge correction is accepted, the core
  remains sans-IO, and its complete test suite stays green.
- The trust-bootstrap decision prevents disclosure of the join token to an
  unauthenticated introducer and pins admitted sessions to formation, node ID,
  and certificate fingerprint.
- Three real worker processes starting as three formations can become one
  three-member formation through explicit operator-led joins. All views
  converge on the exact same `FormationId`, assigned `NodeId` set, labels,
  fingerprints, and liveness state.
- Duplicate worker names and reused cluster names do not merge identities or
  satisfy a formation guard.
- `orishuctl cluster info`, `ls`, and `inspect` return real state through the
  worker's production client listener and clearly distinguish identity,
  labels, liveness, source, and freshness.
- Locking through one worker converges to the others; a join attempted through
  another introducer is rejected as locked. Unlocking through a different
  worker converges and permits a valid join. Administrative removal is also
  refused while locked if it is exposed by this slice.
- Wrong token, target formation, certificate binding, sender binding,
  protocol version, replayed stream message, and oversized/truncated input
  produce bounded structured failures and no partial admission.
- A newly admitted worker cannot introduce another node until cluster policy,
  blocklist, and tombstone catch-up has completed; timeout or malformed
  catch-up leaves it non-introducing with an actionable status.
- Killing one process causes surviving views to progress through the specified
  SWIM liveness states without writing an operator-removal tombstone. A late
  packet or stale timer cannot restore it incorrectly.
- Voluntary leave returns that worker to a fresh standalone formation without
  a tombstone and the old formation converges on its departure.
- Mailboxes, sessions, connections, frames, datagrams, timers, decoded
  collections, diagnostics, and retry/fan-out work have tested limits.
- Multi-process tests use real QUIC/mTLS and real serialized client requests;
  no pass condition calls the membership core directly or depends on a mock
  transport.
- `cargo fmt --all -- --check`, strict Clippy for every changed target,
  relevant unit/integration/doc tests, and `make docs-check` pass.

## Non-goals

- Workload admission, WebAssembly execution, partition assignment, halo
  exchange, step voting/commit, scientific results, checkpoints, or reset.
- Artifact chunk transfer, placement, replication, purge reconciliation, or
  availability repair.
- Kagami connectivity or observation streaming.
- Automatic discovery/admission, DNS-defined cluster membership, or mDNS.
- A durable authored cluster manifest or a permanent leader/scheduler.
- Token rotation, certificate rotation, rolling protocol upgrades, or full
  formation restart recovery unless explicitly pulled in by the preflight.
- Full `orishuctl` parity. Workload, result, checkpoint, storage, historical
  event/audit/log, graceful compute-drain, and monitor UI paths remain owned by
  later work packages.
- Performance or scientific scaling claims. This task proves a bounded
  operational control plane at three workers; distributed computation is the
  following milestone.
