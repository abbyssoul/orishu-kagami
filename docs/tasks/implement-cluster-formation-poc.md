# Implement the operational cluster-formation PoC

Status: **ready for contract slice** — N-MEMBERSHIP is accepted;
dependent adapters start after their preflight contracts land

Decisions: [ADR 0013](../adr/0013-cluster-formation-and-node-identity.md) and
[ADR 0017](../adr/0017-worker-operational-observability.md)

Design: [Orishu runtime design](../orishu-runtime-design.md),
[peer protocol](../protocol-p2p.md),
[client protocol](../protocol-client.md), and
[worker/cluster administrator stories](../user-stories/orishu/README.md)

Prerequisite:
[the implemented sans-IO membership core](implement-membership-model.md),
including its accepted liveness-gossip merge correction. Verify the recorded
regression suite when integrating; completed core work is not a new prerequisite.

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
N-MEMBERSHIP (implemented and accepted, including merge correction)
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

Coordinate the [worker observability task](implement-worker-observability.md)
as a companion M4 delivery: its process probes, formation metrics and sampled
client/peer trace export exercise this task's production driver and transport.
Publish bounded health states and diagnostic outcomes for the worker adapters;
do not put exporter dependencies, secrets or trace context in the membership
core. The transport can develop independently, but the operational M4 demo
includes scrape/probe/trace evidence and
[operator documentation](document-worker-observability.md). This adds no
dependency on workload execution. N-FORMATION owns driver/transport behavior;
P-OBSERVABILITY owns exporters and optional dependencies; P-OBS-DOCS owns
operator recipes. The combined M4 demonstration requires all three deliveries,
but exporters need not be implemented before the transport slice can start.

## Required preflight decisions

Close these narrowly before writing the adapter that depends on them. Record
wire changes in `protocol-p2p.md` and client-resource changes in
`protocol-client.md`; do not hide either contract in worker-private structs.

For each preflight area, land the selected behavior, state transitions,
version/compatibility consequences, exact bounds and golden/hostile fixtures.
Listing alternatives is not completion. Reconcile imported protocol examples
with those choices before writing dependent handlers.

### 1. Trust bootstrap

Specify how a joiner authenticates the introducer before disclosing the join
token. “Accept any self-signed server certificate and then send the token” is
not acceptable because an active intermediary could steal the admission
credential. Choose and test one explicit mechanism, such as join material that
binds the target formation and introducer certificate fingerprint, or an
operator-provisioned CA/trust root.

Specify how the operator securely obtains and transfers that join material.
A redirect is an untrusted endpoint candidate, not permission to disclose a
token to a new certificate. Bound redirects, DNS resolution, connection
attempts and total join duration; every candidate must satisfy the trust rule.

The handshake must distinguish a provisional applicant from an admitted
member. After admission, the QUIC session is pinned to the formation-assigned
`NodeId` and certificate fingerprint held by membership. A label, DNS name,
endpoint hint, or claimed envelope field never establishes that binding.

Define the PoC credential lifecycle too: certificate creation/loading, secure
local permissions, the formation join token, and what a restart retains. Token
rotation and certificate rotation may remain deferred, but the CLI must not
claim they work if they do not.

Specify how an admitted worker obtains the credential material needed to act
as an introducer for the same formation. Tokens are absent from ordinary
gossip, membership snapshots, traces and replayable core state. Public policy
catch-up alone cannot enable introduction if token verification is not ready.
Test a third worker joining through the second worker.

Define a post-admission readiness gate. The core's join snapshot contains
members, while cluster policy, blocklist entries, and tombstones converge
through their owning reconciliation paths. A newly admitted worker must not
act as an introducer until it has obtained and validated the admission-relevant
state needed to enforce every gate; otherwise a removed or blocked identity
could exploit the catch-up window.

Define how completion is proved: a bounded, identified snapshot or equivalent
reconciliation barrier covering policy, blocklist and tombstones, including
concurrent updates, empty collections and expired continuations. Receiving one
page or seeing matching member counts is insufficient. This proves adoption
of a defined admission-state baseline, not globally latest state under a
network partition; normal convergence and admission re-checks remain necessary.

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

Specify envelope sequence scope, replay-window bounds, request correlation,
connection replacement and overflow behavior. Valid reordered datagrams and
idempotent domain retries must remain valid; a sequence high-water mark must
not discard every older datagram. Disable replayable early data for admission
and mutations. Reject duplicate map keys, trailing frames/bytes where forbidden,
unsupported versions and excessive nesting before state adoption.

Resolve the peer document's datagram-size contradiction: its blanket stream
fallback conflicts with the message-specific SWIM mapping. Define a fitting
base message and byte-budgeted gossip selection; defer excess gossip to later
dissemination/anti-entropy and report an unencodable base message. Do not
silently change a datagram operation into a reliable stream. Golden tests must
cover the actual framing/CBOR representation, not only semantic JSON fixtures.

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

Specify the policy's canonical leaf/tag, hash input, version ownership, equal-
version conflicts and anti-entropy participation. A local lock response means
local acceptance, not a synchronous cluster-wide fence. Concurrent lock/unlock
commands converge through the documented ordering; partitioned or catching-up
nodes must not claim globally current policy. Re-check locally held admission
gates after asynchronous credential verification before inserting a member.

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

Close the local administrator bootstrap: config-free startup permits local
reads but does not authorize mutations or token retrieval. Specify a secure
same-user credential provisioning path usable by both CLI and test harness;
operator, join and monitoring credentials remain non-interchangeable. Resolve
the current `DELETE /membership` Tier 1 table entry against the rule that all
mutations require authenticated authority. Freeze which local/remote client
transports the PoC supports; update protocol maturity and reject unsupported
modes explicitly rather than implicitly promising every documented fallback.

Define asynchronous command outcomes and bounded operation tracking, including
request identity, retry/replay behavior, timeouts and expiry. A successful HTTP
exchange is not proof that join/catch-up or cluster convergence completed.
Specify direct-worker targeting for join/leave and stale formation preconditions
for mutations; delayed requests must not operate on a newly adopted formation.
Distinguish a standalone cluster of one from an already joined member even if
all other members are dead; member count alone cannot decide join eligibility.

### 5. Ejection and restart semantics

Define what the worker shell does when the core learns that its own assigned
identity has been removed: stop participating in that formation, close its
peer sessions, and expose an actionable local state. Do not leave a process
sending as a tombstoned identity.

For this PoC, restart begins a fresh standalone formation and requires explicit
readmission with a new formation-assigned ID. Retain the securely stored
certificate identity where configured so restart does not silently bypass an
active formation's certificate-based exclusion. Do not restore old membership,
policy or assigned IDs. Automatic formation recovery and certificate rotation
remain separate work; document behavior when credentials cannot be loaded.

### 6. Interrupted transitions and stale IO

Specify join attempt identity and recovery when an introducer inserts a member
but its ACK is lost. Bound pending admissions and retry records; reconnecting
the same authenticated applicant must not allocate repeated live identities
or strand it permanently behind duplicate-certificate rejection. Retry may
recover the accepted outcome or return an explicit unresolved state with a
bounded recovery procedure; it must not pretend the insertion never occurred.
Define the case of concurrent attempts through two introducers, or explicitly
serialize supported attempts and reject concurrent operator requests. A lost
client connection alone must not silently undo an accepted domain transition.

Every timer, credential/ID allocation result, queued send and decoded peer
input carries the local lifecycle/attempt generation needed to reject stale
completion after adoption, leave, ejection or shutdown. Serialize formation
transitions; failed pre-adoption join retains the standalone state. After
adoption, catch-up failure is an explicit degraded target-formation state, not
an automatic rollback to the abandoned formation.

Voluntary leave is a self-announced liveness transition, not record deletion
or an operator tombstone. Because its announcement is lossy, immediate receipt
by every peer is not promised: survivors eventually detect departure through
announcement dissemination or normal SWIM. Bound shutdown flushing and close
old sessions; old work must never be re-sent using the new formation identity.

## Implementation slices

### 1. Close the contracts

- Resolve the six preflight areas above and add hostile/golden fixtures.
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
  `orishu-membership`. Start with cohesive, testable modules in the worker, its
  real caller. Extract a crate only when a second consumer or a demonstrated
  deep interface justifies it; tests alone do not require a new crate.
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
- Define fairness and reserved processing capacity for control, actionable
  timer expiries and effect completions under peer/client floods. Coalesce only
  permitted diagnostics or supersedable work; never drop a correctness-bearing
  completion and leave an admission permanently pending. Timer scheduling uses
  monotonic shell time and generations, not wall-clock order.
- Publish bounded health/progress and instrumentable outcomes for
  P-OBSERVABILITY from this real owner. Health requests and scrapes must not
  acquire long-held locks or drive core transitions.
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
- Distinguish bound from advertised addresses, reject unusable peer endpoint
  claims and define port-zero reporting for tests. Wire the accepted core's
  anti-entropy depth/round limits through the shared configuration path.
- Exercise fresh restart, ejection, cancelled join and shutdown with pending
  effects. Secure certificate persistence is in scope; retained formation
  state and scientific artifact storage are not.

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
- Return structured pending/accepted/rejected outcomes and provide the bounded
  completion/status path chosen in preflight. Keep operation IDs and formation
  preconditions in automation output. Secret retrieval must be explicit and
  never printed in ordinary status, logs or error output.

### 6. Multi-process conformance harness

- Start three worker subprocesses with isolated runtime directories and
  dynamically allocated listeners. Drive formation through `orishuctl` or the
  same public client methods it uses.
- Run at least the successful operator journey through the CLI binary; library
  calls alone do not validate command routing, authentication or output. Use
  machine-readable output for assertions and the same production listeners.
- Poll observable state with bounded deadlines; do not use fixed sleeps as
  proof of convergence.
- Capture per-process logs and deterministic test artifacts on failure, while
  redacting join tokens and private-key material.
- Reap every subprocess and bound cleanup on success, failure and timeout.
  Inject faults at real transport/effect boundaries with narrowly scoped test
  hooks when needed; preserve actual QUIC/TLS/codec and client authorization.
- Cover duplicate worker/cluster labels, reordered startup, an invalid token,
  a wrong formation/fingerprint, lock/unlock through different entry nodes,
  voluntary leave, process loss and SWIM visibility, and clean shutdown.

### 7. Observability and operator handoff

- Integrate P-OBSERVABILITY slices 1–3 against the driver and peer adapter:
  scrape each worker, exercise the probe matrix, and collect a sampled
  client-to-peer trace in a test OTLP receiver. Keep this in a feature-enabled
  companion harness; the formation harness must also pass with optional
  telemetry compiled out and with exporters disabled.
- Verify telemetry outage/overload does not change admission, lock, leave or
  SWIM outcomes, and a stalled driver cannot hide behind a responsive HTTP
  exporter. Validate bounded trace context on the serialized peer/client path.
- Deliver P-OBS-DOCS formation recipes with the above features. Update worker
  configuration/manual, operator stories, protocol maturity, task index and
  roadmap to the actual supported command and transport matrix. Distinguish
  N-FORMATION completion from combined M4 observability acceptance.

## Required failure and convergence evidence

Use pure transition tests for state matrices and real process/wire tests for
adapter guarantees. In addition to the three-worker happy path, cover:

| Scenario | Required evidence |
| --- | --- |
| A introduces B; B introduces C | Trust, token-verification readiness and complete admission-state catch-up work beyond the initial introducer |
| ACK lost after insertion; operator retries | Documented recovery without duplicate live IDs or false failure/rollback claims |
| Concurrent joins and final capacity slot | Requests are serialized/rejected as specified; gate re-checks prevent local over-admission |
| Lock during credential verification | Acceptance re-checks the updated policy before insertion |
| Concurrent lock/unlock; temporary peer partition | Deterministic policy convergence after reconnect, with no claim of a global lock before convergence |
| Empty, paginated, changing or truncated admission-state baseline | Introduction is enabled only after validated completion; malformed/expired catch-up remains bounded and non-introducing |
| Old session, timer, verification or send completes after leave/adoption | Generation fencing prevents mutation or emission into either abandoned or unrelated formation state |
| Voluntary leave announcement dropped | Old views converge through SWIM without requiring record erasure or creating a tombstone |
| Restart with prior certificate; tombstoned self learns ejection | New standalone identity; no old-session participation; target formation still enforces certificate exclusion |
| Datagram reordering/replay and oversized gossip | Valid probe correlations survive bounded replay handling; oversized piggybacking never changes transport semantics |
| Peer/client flood, slow reads and incomplete frames | Admission work, allocation, queues and timers stay bounded; control and health remain meaningfully supervised |
| Missing/wrong operator credential or monitoring/join credential used for mutation | No mutation, secret disclosure or misleading success |

Removal administration need not be exposed to exercise self-ejection: a
test peer may supply the valid tombstone through the production wire path.
Record any test-only fault controls explicitly and keep them unavailable in
normal releases. Avoid claiming arbitrary churn/scaling correctness from this
bounded three-process test.

## Administrator stories trialled

This PoC provides executable evidence for the membership-only portions of:

- config-free local startup, worker and cluster labels, peer listeners, peer
  capacity, and peer-admission flags;
- list and inspect cluster nodes, and inspect cluster status;
- retrieve current join material and explicitly join a worker;
- lock and unlock cluster membership; and
- voluntarily leave and observe liveness after a worker becomes unreachable.

It does not complete stories whose acceptance requires workload drain,
artifact transfer, durable audit history, runtime/storage telemetry, packaging,
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
  blocklist, tombstone and admission-credential catch-up has completed; timeout or malformed
  catch-up leaves it non-introducing with an actionable status.
- Killing one process causes surviving views to progress through the specified
  SWIM liveness states without writing an operator-removal tombstone. A late
  packet or stale timer cannot restore it incorrectly.
- Voluntary leave returns that worker to a fresh standalone formation without
  a tombstone and the old formation converges on its departure as liveness
  state; its membership record need not disappear.
- Mailboxes, sessions, connections, frames, datagrams, timers, decoded
  collections, diagnostics, and retry/fan-out work have tested limits.
- Multi-process tests use real QUIC/mTLS and real serialized client requests;
  no pass condition calls the membership core directly or depends on a mock
  transport.
- The failure/convergence matrix above passes at the appropriate production
  boundary, including interrupted joins, policy races and stale IO fencing.
- Local administrator bootstrap and direct-worker targeting are documented and
  tested. No Tier 1 mutation exception or default-success placeholder remains
  in the supported surface.
- The task's core formation evidence passes independently of telemetry.
  Combined M4 acceptance additionally requires the feature-enabled scrape,
  probe and cross-peer trace tests and operator recipes from slice 7; report
  outstanding companion work rather than declaring the whole milestone done.
- `cargo fmt --all -- --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace --all-targets`,
  `cargo test --locked --workspace --doc`, and `make docs-check` pass.
  Also run the explicit observability-feature build/test matrix and the
  multi-process harness if it is not part of the default suite. Preserve the
  membership dependency-purity test and record platform-only manual checks.

## Non-goals

- Workload admission, WebAssembly execution, partition assignment, halo
  exchange, step voting/commit, scientific results, checkpoints, or reset.
- Artifact chunk transfer, placement, replication, purge reconciliation, or
  availability repair.
- Kagami connectivity or observation streaming.
- Automatic discovery/admission, DNS-defined cluster membership, or mDNS.
- A durable authored cluster manifest or a permanent leader/scheduler.
- Token rotation, certificate rotation, rolling protocol upgrades, or retained
  formation recovery after restart. Changing these non-goals requires a new
  bounded task and any necessary architecture decision, not an incidental
  expansion of the IO adapter.
- Full `orishuctl` parity. Workload, result, checkpoint, storage, historical
  event/audit/log, graceful compute-drain, and monitor UI paths remain owned by
  later work packages.
- Performance or scientific scaling claims. This task proves a bounded
  operational control plane at three workers; distributed computation is the
  following milestone.
