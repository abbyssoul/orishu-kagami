# Implement the operational cluster-formation PoC

Status: **in progress** — public three-worker introducer handoff passes;
recovery, lifecycle/fault conformance and operational handoff remain incomplete

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

## Current gap and next reviewable increment

This summary includes the automatic catch-up increment's worker tests and
public CLI run recorded below. Earlier chronological notes retain superseded
gaps; use this summary for planning. Evidence for one increment does not
revalidate every earlier implementation claim or close the full fault matrix.

- **Recorded integration:** standalone identity/inspection, explicit peer
  listener, bounded owner and QUIC adapters, authenticated lock operations,
  privileged join material, identified join submission/status and public
  bad-pin/replay failure-path coverage. The real CLI harness now proves A
  admitting B, then B admitting C after completed automatic catch-up. All three
  views converge on exact member identities, fingerprints and live state, with
  lock/unlock through different workers and exact join replay. C can then leave
  through the CLI with fresh identities/token, retained certificate and exact
  receipt replay; both survivors eventually mark its old identity dead. Explicit
  readmission retains the certificate but receives a new assigned node ID;
  replaying the old leave receipt after rejoining does not leave again.
  SIGKILL/restart now has public evidence too: survivors observe suspicion/death,
  restart retains the certificate/operator credential but starts standalone,
  and explicit readmission restores three live identities with exact history.
- **Recorded catch-up progress:** bounded baseline transfer, atomic core merge
  and receiving-owner credential installation have named helper/real-wire and
  runtime tests in the integration record. The runtime test withholds readiness
  during incomplete catch-up, cancels a prepared job and then verifies automatic
  retry to `joined`. Production startup now enables the same scheduler.
- **Next unproved boundary:** stale/ejection/catch-up fault conformance,
  then interrupted-admission recovery, public
  certificate-excluded restart/ejection and interrupted leave. Three-worker collision, flood and
  partition cases remain required.
  A listening socket, returned join token or matching member list does not
  prove this barrier complete.
- **Still required:** catch-up failure/lifecycle conformance,
  three-worker reconnect conformance, interrupted-admission
  recovery, remaining leave/ejection lifecycle evidence and the full fault matrix.
- **Companion delivery:** metrics/probes, sampled cross-peer traces and tested
  operator recipes remain P-OBSERVABILITY/P-OBS-DOCS work. Neither recorded
  formation integration nor this review establishes their implementation.

Preserve the public adoption assertions as the integration baseline:
authenticated submission/status, source-formation guards, exact retry and
secret-free output, with matching inserted/adopted identity. The joiner must
remain non-introducing while catch-up is incomplete. Only the complete CLI
journey establishes public handoff; it does not establish N-FORMATION completion.

Next harden catch-up/lifecycle fault evidence against the existing contracts;
follow with recovery, remaining conformance and the combined operational handoff.
Keep protocol maturity and operator manuals synchronized at each checkpoint,
not just at final acceptance.

### Introducer-handoff increment and remaining hardening

The automatic happy path and bounded failure-artifact infrastructure below now
pass. Remaining stale successful-completion and fault tests are still required
before this checkpoint's hardening is complete.
This increment does not close the whole task.
Use the recorded receiver/owner path rather than inventing another catch-up
protocol. Verify the named baseline tests before relying on their evidence.

1. Connect catch-up scheduling to the production runtime. Specify when work
   starts, which eligible authenticated source is selected, and how bounded
   retries resume after failure. Keep one active attempt per worker, reserve
   completion capacity before IO, and supervise cancellation/deadlines. Fence
   completion by attempt, formation generation and current source session;
   shutdown, leave or ejection must not leave detached work or install stale
   credentials. Publish exhausted/unavailable recovery as actionable status,
   without returning to the abandoned standalone formation implicitly.
2. Preserve deterministic negative readiness evidence: pause or cancel a
   transfer through a test-only seam and prove the joiner cannot introduce or
   export target credentials while catch-up is incomplete. Do not depend on
   polling a brief transitional phase before an automatic job finishes.
3. Extend the public CLI harness to three independent processes. Join B through
   A, poll B's identified operation to `joined`, obtain B's privileged join
   material, then join C through B and poll C to `joined`. Check historical
   source identities, newly assigned identities, exact request replay, and
   secret-free ordinary output. Across all three workers, poll the exact
   formation/member identity sets, fingerprints and live state; equal counts
   alone are insufficient. Lock through A and unlock through B, verifying
   convergence from every worker.
4. Preserve bounded cleanup and add redacted failure artifacts. Record the
   exact build/harness commands and observed results; update this summary and
   protocol/operator maturity statements without claiming the remaining fault
   matrix or combined M4 gate has passed.

Exit evidence is the production CLI journey plus deterministic incomplete,
cancelled and stale-completion tests at the owning runtime/wire boundary.
Lost-ACK recovery, public ejection/interrupted leave, the remaining fault matrix and the
observability companion deliveries remain explicit follow-up checkpoints.

## Formation demonstration

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

Treat each area as a separate review gate, not a requirement to redesign all
six before any work can proceed. Existing documented choices and recorded
implementation evidence remain the baseline; close only the unresolved parts.
A gate is closed when its owning protocol section specifies the behavior and
bounds, its tests are identified, and its dependent slice can proceed without
inventing security or lifecycle semantics. A cross-cutting departure from the
accepted ADRs requires an ADR update, not just a task-local decision.

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

Close admitted-peer connection management as part of this gate: who initiates
connections after adoption/reconciliation, how simultaneous dials converge to
a usable session, and how a lost connection is recovered. Start from the
documented registry's duplicate-connection rule; demonstrate that it cannot
leave both workers repeatedly rejecting each other's surviving connection.
Specify bounded per-peer and process-wide dial work, endpoint selection,
backoff, and cancellation on generation change. Endpoint hints are untrusted;
every reconnect must revalidate the current formation, assigned identity,
certificate and admission restrictions. Reconnecting an admitted peer is not
automatic admission of an unknown worker.

Define what happens to an effect when its route is absent or a connection
fails: which messages expire under core timers, which exchanges may retry,
and which return a correlated failure. Do not accumulate an unbounded offline
send queue or treat a successful transport write as domain acceptance. Record
the supported connection topology and its resource cost for the three-worker
PoC; do not imply that a bounded all-to-all demonstration proves fleet scale.

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

Distinguish a local connection/admission limit from a formation-wide member
limit. Serialization at one introducer prevents local over-admission, not
concurrent admissions at different introducers. Do not promise a globally
reserved final slot without a separate coordination contract; state and test
the limits this PoC actually enforces.

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

### Review checkpoints

Deliver the numbered slices below through these independently reviewable
checkpoints. They organize the existing scope; they do not replace the final
acceptance criteria or authorize adding workload execution.

| Checkpoint | Scope | Evidence required before advancing |
| --- | --- | --- |
| Contract closure | Remaining preflight decisions and slice 1 | Explicit unresolved/closed decisions linked to protocol sections; compatibility consequences and bounded failure behavior specified |
| Two-worker vertical slice | Slices 2–5 through one real join | Production processes, authenticated operator request, peer handshake/admission, validated adoption and observable operation status; no direct core call standing in for a worker route |
| Introducer handoff | Admission-state and credential catch-up, then slice 6 happy path | A admits B, then B admits C; all three views converge, and B cannot introduce while catch-up or credential readiness is incomplete |
| Formation conformance | Remaining lifecycle, policy, authorization and failure cases in slices 3–6 | The required failure/convergence matrix and N-FORMATION acceptance criteria pass, with reproducible commands and bounded process cleanup |
| Operational M4 handoff | Slice 7 with P-OBSERVABILITY and P-OBS-DOCS | Feature matrix, per-worker scrapes/probes, correlated peer trace and executable operator recipes; formation still works without telemetry |

Keep implementation evidence separate from acceptance: name the test or harness
and what production boundary it exercises, record checks actually run, and list
remaining gaps. Documentation review alone does not revalidate implementation
claims. In particular, helper coverage cannot close a process-level checkpoint.

### Implementation evidence and remaining integration

The following is a chronological integration record, not a current checklist.
Later entries supersede earlier statements that a particular adapter or route
is unwired. The current-gap summary above and checkpoint evidence requirements
govern planning; preserve the distinction between reported tests and checks
rerun during the current review.

- Core `MembershipPolicy` now separates the replicated lock from node-local
  admission configuration. Gossip and anti-entropy tests cover lock/unlock,
  ordering, conflicts, version exhaustion and admission re-checks; operator
  removal respects the lock while SWIM remains independent. Hash domains are
  version 2, with updated golden fixtures.
- Per-sender replay tracking now permits unseen requests within a bounded
  128-sequence window across independently reordered streams, while rejecting
  duplicates. Formation adoption drops old policy and replay state.
- Worker `peer::codec` bounds encoded lengths, nesting, collections and work
  before typed allocation, including duplicate-map checks and golden frames.
  `peer::read_frame` checks the prefix before allocation and bounds total read
  time through stream FIN; writes also have a deadline.
- Worker `peer::tls` negotiates `orishu-membership/2`, requires client key
  possession and pins the introducer certificate. Real loopback QUIC tests
  cover mutual authentication, wrong pins, missing client certificates and
  malformed/mismatched private identities. These modules are not yet wired
  into the worker executable; there is no operational cluster claim.
- Worker `credentials` implements bounded, owner-only Unix identity/operator
  token persistence, exclusive instance ownership and fail-closed reload.
  Tests cover restart, corruption, unsafe permissions and symlink rejection.
- `peer::session` now extracts TLS/ALPN facts and distinguishes applicants,
  pinned outbound introducers and admitted members. It checks assigned
  identity/certificate binding against current membership, rejects stale
  generations and rechecks identity blocks/removal. Member dialing supports
  membership-held fingerprint pins; real QUIC tests verify matching and wrong
  pins. The owner-side registry and network checks below now consume those
  bindings; executable listener orchestration is still pending.
- `peer::admission` now produces secret-free credential/source-network verdicts
  from literal-IP/CIDR policy and the transport-observed address. Invalid or
  oversized policy fails closed with an explicit completion, and mapped-address
  tests prevent an IPv4 rule being bypassed by IPv6 representation. The real
  QUIC admission test feeds that evidence back into the core and reaches its
  ID-allocation effect. The owner packet path now executes that bounded check
  for registered sessions, with source revalidation as described below.
- The bounded handshake DTO now binds request/acknowledgement roles over a
  separate bidi stream without an admission token. The real QUIC test drives
  handshake, core-generated JoinReq, credential evidence, ID allocation,
  encoded JoinAccepted and validated adoption by a second core. This remains
  an adapter integration test, not separate production worker drivers or the
  required three-process demonstration; connection orchestration and catch-up
  must still be connected.
- Handshake registration now enters the real owner through its bounded peer
  lane. Its owner-side registry caps total/provisional sessions, expires
  applicants, preserves process-unique session IDs and closes removed or
  old-generation bindings. Real QUIC tests cover owner-decided ACK, duplicate
  connection refusal, capacity/expiry, leave fencing and shutdown of registered
  and queued connections. This connects handshake binding to the owner, not
  the executable peer accept loop or post-handshake admission/send dispatcher.
- Registered sessions now recheck active network rules against QUIC's current
  source address during registration and owner sweeps. A shared bounded,
  allocation-free rule iterator supplies both this check and token-verification
  evidence. The real migration test rebinds a client to a blocked loopback IP
  and verifies owner-driven closure, later rule lifting and fail-closed invalid
  policy. The registered-packet owner path below now performs the same check
  immediately before decoding; sweeping sessions alone is insufficient.
- Raw registered-session packets now enter the bounded peer lane with transport
  and generation correlation. The owner revalidates the current registry and
  source, decodes the wire payload, and applies its core input in one turn.
  A session-addressed reliable effect is correlated to that request's reply
  stream. A real QUIC test exercises JoinReq-to-core-to-locked-refusal and rejects
  unknown sessions/malformed frames without membership changes. The executable
  peer listener is not enabled by this intermediate path.
- The runtime now moves its fresh standalone join token into the IO owner.
  Registered requests drive bounded token/source verification and correlated
  core completion inline, followed by core ID allocation and insertion/ACK.
  The real QUIC admission test covers lock/invalid-token rejection, successful
  insertion with independently validated joiner-core adoption, and rejection
  of the previous token after leave. Adoption clears prior token authority;
  target credential installation remains catch-up work. This proves an
  admission exchange; the sustained owner transport evidence is described below.
- Admission now promotes the inserted session; known-member reconnects bind
  against current membership. Member sends use real QUIC datagrams or bounded
  reliable tasks whose responses re-enter the owner. Missing routes and IO
  failures are counted, not fabricated as delivery or made terminal owner
  failures. The real-QUIC integration test runs two owners after validated
  adoption/reconnect, observes SWIM and reliable exchanges, and converges lock
  and unlock in both directions. The second owner is initialized from the
  adopted core by the test. Automatic dialing, the executable accept loop,
  production join commands, recovery and introducer catch-up remain required;
  this is not three independent worker processes starting and joining via CLI.
- `peer::exchange` now bounds concurrent reliable IO without an unbounded
  permit-wait queue, applies an operation-wide deadline including owner-reply
  waiting, and resets/stops cancelled exchanges. Datagram submission refuses
  stream-only/oversized input without fallback. Real QUIC tests exercise
  request/reply, overload, cancellation and permit release; the handshake/join
  integration test uses this pool for its handshake. Live dispatcher and
  owner-completion mailbox wiring remain incomplete.
- `peer::wire` maps all current membership message effects to explicit CBOR
  DTOs, checks session/envelope binding before typed payload allocation,
  separates admission tokens from core inputs, and measures actual snapshot
  bytes. Golden/hostile fixtures cover the envelope, delivery mapping, snapshot
  length and byte-budgeted datagram gossip. A real QUIC join-request test feeds
  the decoded request into the core's credential-verification effect.
- The executable now loads credentials, generates fresh formation/node IDs and
  serves a real versioned `ClusterSummary` through the shared client and CLI.
  Two-process tests verify independent startup, exclusive state ownership and
  restart identity changes. Plain TCP is rejected; remote summary reads require
  the operator token. The runtime truthfully reports introduction unavailable.
- Membership now lives in a serialized owner task; HTTP reads use a bounded
  watch projection and return unavailable if that owner stops. The initial
  standalone driver runs periodic probe/reconciliation commands, owns bounded
  monotonic timers, performs peer selection/ID allocation, rejects stale
  generation input and reserves a shutdown mailbox. Registered-request token
  checks, correlated session replies and registered-member sends now execute.
  This is not yet the complete driver required by
  slice 3. Structured event delivery, steady-state peer IO and end-to-end
  fairness under the live listener remain part of that integration.
- Owner ingress now separates 64 peer messages, 16 operator commands and 64
  effect/timer completions, plus reserved shutdown. Bounded processing quotas
  prevent continuously ready completion/control queues from starving queued
  peer input; due owner timers retain priority. A regression fills peer ingress
  and proves control/completion submission and processing still succeed. The
  live executor must still reserve/correlate completion capacity before issuing
  asynchronous work; callers may not discard an overloaded completion.
- Verification work can now reserve its completion slot before starting. The
  single-use handle fixes request/session/generation correlation and queues a
  negative verdict if dropped or its task is aborted. Tests exhaust all slots
  and exercise completion/cancellation without losing an outcome. The current
  local token check completes inline without asynchronous IO; any future async
  verifier must acquire these handles and own task deadlines. This primitive
  alone is not evidence of the complete admission workflow.
- Remaining work includes join/leave client DTOs and routes,
  session orchestration and admission-state catch-up, interrupted-join
  recovery, completion of driver integration, peer configuration and the
  remaining API/CLI routes,
  observability integration and the complete multi-process failure matrix.
  The remaining preflight choices must be recorded before their adapters land.
- The CLI now explicitly loads a bounded, private worker operator-token file
  and attaches its bearer credential through the shared client. Loader tests
  cover malformed/oversized files, unsafe permissions, links and FIFOs; the
  CLI process test exercises the actual serialized authorization header and
  redacted failure output. This closes credential-file input, not worker
  mutation authorization or the real three-worker operator journey.
- Unix socket lifecycle tests now verify active-listener protection, stale
  socket recovery, replacement/link preservation and unsafe-parent refusal.
  The real worker restart test reuses the same socket after both graceful
  shutdown and process loss. Fixtures explicitly create private directories;
  permissive temporary-directory defaults must not weaken production checks.
- The membership owner now accepts acknowledged lock/unlock intents on its
  existing bounded operator lane, checking both generation and formation at
  dequeue time. Tests cover applied-state replies, repeated intents, a queued
  request overtaken by leave, overload and requester cancellation. This is an
  internal authorized-adapter interface used by the identified HTTP path below.
- Versioned `LockRequest`/`LockReceipt` now connect the actual worker POST
  route, shared client's `set_lock`, and CLI lock/unlock commands. All mutations
  require the worker operator credential, including on Unix sockets. Exact
  retries recover historical outcomes without reapplying a superseded intent;
  conflicts and stale formation preconditions are rejected. The explicit PoC
  history limit is 1,024 outcomes per worker's formation lifetime, with no
  eviction/reexecution. Frame/deadline/concurrency limits and expiry semantics
  are recorded in the client protocol; asynchronous join operation tracking
  remains unresolved. Real process tests exercise malformed/unauthorized input
  and the shared client. `python3 scripts/check-formation-cli.py` exercises the
  production CLI lock/unlock/retry journey on one worker, not three-worker
  formation or peer policy convergence.

This evidence does not satisfy the final acceptance checklist. In particular,
two endpoints in one TLS test are not three production worker processes.

The production `peer::server` dispatcher now bounds inbound TLS/connection and
stream work and shares the owner's reliable-exchange pool. Real-QUIC tests
`dispatcher_handshake_and_owner_shutdown_close_all_connections` and
`silent_application_handshake_expires` cover the first-handshake deadline and
shutdown, including unregistered connections. The existing two-owner traffic
test now uses its registered stream/datagram loop. Run these with
`cargo test --locked -p orishu-worker --lib`. This adds adapter evidence only:
executable listener configuration, outbound dialing, operator-led join and
catch-up still prevent closure of the two-worker vertical-slice checkpoint.

Explicit single-address peer listener startup is now connected through the
typed file/env/CLI configuration and runtime supervision. Wildcard binds need
a usable explicit advertisement; concrete port-zero binds publish the actual
allocated port. Runtime tests verify endpoint advertisement and owner-driven
connection closure. Admission remains disabled in this intermediate executable
surface: join operations, dialing and catch-up still prevent vertical-slice
acceptance. This supersedes the listener-configuration gap above, not the
three-process evidence requirement.

Node inspection now reads a single identity through the owner's bounded control
lane and returns a versioned local-view resource through the production route,
shared client and CLI. Tests exercise known/unknown identities, unsupported
source modes, and remote authorization. `scripts/check-formation-cli.py` also
checks the actual `inspect` command. This does not implement membership listing,
direct remote-node queries or prove three-worker convergence.

Membership listing now shares that projection and reads four-record ordered
pages through the production route. The owner checks continuation formation;
the client validates page/source identity and ordering under bounded collection
and time limits. Pure/owner tests exercise multi-page traversal and stale/missing
formation rejection; process tests and the CLI journey exercise real listing,
filtering and remote authorization. This supersedes the listing gap above but
does not make operator paging a catch-up barrier or a frozen snapshot.

Privileged join-material retrieval now reads the owner-held token together with
its formation, introducer identity/fingerprint and peer endpoints. The client
and `token` CLI consume this versioned resource; raw token debug output is
redacted and rotation explicitly rejects. The real CLI journey verifies local
authorization, identity/pin binding, actual port-zero advertisement and token
absence from inspection. Export reports introduction unavailable while the
join/catch-up workflow remains unfinished; no successful join is claimed.

The CLI now prepares a versioned identified join input from a private bounded
JSON material file, with distinct source and target formations. Loader tests
cover privacy, links, size, endpoint validation and redacted errors. The old
raw-token CLI and automatic-leave claim are removed; the real CLI journey
checks that the unfinished executor fails explicitly without transmitting or
printing secret input. Wiring this envelope to the owner, operation tracking,
pinned connection and validated adoption remains the next integration work.

A secret-free bounded outbound bootstrap adapter now completes pinned TLS and
the initial handshake against the real dispatcher. Its target retains the
operator-supplied formation/node/fingerprint binding; ACK validation rejects
a mismatched introducer identity. The real-QUIC dial test covers wrong pins,
capacity and abandoned-connection cleanup. It never receives or sends the
token. Registering the completed handshake through the active owner operation,
then sending JoinReq and driving adoption/recovery/catch-up, remains unfinished.

Pinned introducer ACKs now enter the current owner through the bounded peer
lane, with source-policy checks, owner-assigned session IDs, generation fencing,
and provisional lifetime/capacity enforcement. The real dial test checks owner
registration and wrong-identity/stale-generation closure without changing the
joiner's formation or member count. The binding is JoinReply-only. This closes
session-registration plumbing, not active-operation correlation or JoinReq
execution; the operator join state machine remains the next required work.

The internal owner begin-join path now checks source formation/generation,
standalone participation, attempt exclusion and the registered introducer.
It executes core JoinReq effects through the bounded real transport, retaining
the secret only in shell state; replies re-enter the owner and validated
adoption reaches `catchingUp`. The real dial test now proves two-owner admission
and adoption from independent standalone models, plus stale-source rejection
and absence of introduction/credential readiness after adoption. Operator
operation tracking/routes, interrupted-join recovery, reconnect and complete
catch-up still block the public two-worker and three-worker checkpoints.

Lost-ACK classification now has a real-QUIC regression: the introducer inserts
the member, a test-only response-write fault discards its ACK, and retry
exhaustion leaves the joiner `joinUnresolved` rather than eligible for another
join. The source formation and remote insertion are preserved, transient secret
and send state are dropped, and another begin request is refused. This provides
ambiguity evidence, not outcome recovery; the identified recovery protocol and
operator procedure remain required before interrupted-join acceptance closes.

The versioned join-operation DTO and bounded worker-local tracker now distinguish
processing, possible admission, adoption and catch-up. Tests cover exact replay
after adoption, changed-request conflicts (including token changes), exclusive
active work, non-evicting 64-record history and invalid lifecycle transitions.
Only a request digest and secret-free status are retained. The tracker must
still be integrated at the serialized owner boundary with supervised execution,
cancellation, status routes and client polling; unit bookkeeping alone does
not close the operation/recovery or process-level acceptance gates.

Operation preparation/status now run at the serialized owner boundary. A
single-use preparation owns a reserved completion slot, so dropping a job or
its reply receiver records pre-admission failure without losing completion.
Successful pinned IO returns through that slot for lifecycle/ACK revalidation;
the owner records emission and adopted target identity. The real dial test now
checks tracked adoption and exact replay across formation change. Runtime job
supervision/deadlines, authenticated routes and client polling remain unwired;
these tests do not yet establish a public two-process operator join.

Prepared jobs now have a runtime executor with bounded task retention, shared
dial capacity, a 16-second outer deadline, retained outgoing endpoint ownership
and shutdown exclusion/cancellation/drain. The runtime test verifies failed
certificate pinning, retained exact replay, no membership changes and task
cleanup. Public authenticated submission/status routes and client polling still
remain before this can be exercised as a real operator join workflow.

Authenticated identified join submission/status routes and the shared client/
CLI are now connected. The real CLI journey verifies authorization, bad-pin
failure, polling, exact replay, stale-source/changed-request rejection and
secret-free status output. Processing acceptance is explicitly distinct from
adoption and completed catch-up. The executable still withholds introduction,
so this is public failure-path evidence, not the successful two/three-worker
formation gate or interrupted-admission recovery.

Missing owner-held formation credentials now produce a negative token verdict
instead of a terminal owner error. The real-QUIC
`real_join_packet_drives_owner_lock_token_and_admission_outcomes` regression
includes an otherwise eligible introducer without an installed token, verifies
`InvalidToken`, unchanged membership and continued owner availability. This
closes a fail-closed prerequisite, not explicit admission configuration or the
post-adoption catch-up readiness contract.

Explicit `spec.accepts.peers` / `ORISHU_ACCEPTS_PEERS` / `--accepts.peers`
configuration now enables standalone introduction, defaulting to false and
requiring a peer listener when enabled. Owner readiness requires a configured
role, advertised endpoint, safe participation phase and installed formation
token. Transitional phases refuse credential admission. The runtime test
`explicit_introducer_runtime_admits_but_adoption_withholds_readiness` exercises
real QUIC adoption between independently initialized runtimes and checks that
the adopted worker cannot export target credentials or claim introduction
readiness. Run with `cargo test --locked -p orishu-worker --all-targets`.
This runtime evidence alone does not establish public process acceptance or
completed catch-up.

The public two-process adoption checkpoint now has CLI evidence in
`scripts/check-formation-cli.py`. It starts isolated workers with duplicate
labels but distinct initial identities, explicitly enables introduction and
drives authenticated join/status through the real binary and listeners. It
checks target/assigned identities, matching membership ID sets and certificate
bindings, exact replay across adoption, secret-free status, withheld target
credential export, `catchingUp`/non-introducing status and bounded process/socket
cleanup. Build with `cargo build --locked -p orishu-worker -p orishuctl`, then
run `python3 scripts/check-formation-cli.py` on Unix with loopback sockets
available. The command passed for this increment; no stable liveness convergence,
complete catch-up, B-to-C introduction or recovery claim follows from it.

The admitted-member dial path now derives a bounded secret-free routing snapshot
from current membership and sends an assigned-identity handshake using the
recorded certificate pin. It shares bootstrap dial/exchange limits and returns
through owner-fenced member registration. The real-QUIC
`real_join_packet_drives_owner_lock_token_and_admission_outcomes` test now uses
this adapter when reconnecting after adoption, then verifies SWIM and replicated
lock traffic. This replaces manual TLS/request construction in that test, not
test-driven connection scheduling: automatic reconnect/backoff, simultaneous
dials and public steady-state convergence remain required.

Runtime maintenance now schedules canonical admitted-peer connections using a
bounded one-second cursor scan, shared dial budget and per-target exclusion;
generation changes and shutdown cancel old work. The lower assigned ID dials
when both advertise endpoints; unadvertised workers dial reachable peers. The
driver now includes suspected members in peer selection so restored sessions
can carry actual probes/refutations rather than becoming permanently idle.
The public CLI journey verifies lock/unlock convergence after adoption without
manually constructing a session, and verifies that a catching-up worker still
cannot mutate policy. The runtime adoption test additionally injects connection
loss through a `cfg(test)`-only registry close hook, preserving membership and
session-ID allocation, then verifies autonomous reconnect, policy convergence
and two live members. `cargo test --locked -p orishu-worker --all-targets` and
`python3 scripts/check-formation-cli.py` passed for this increment. This does not
close the three-worker simultaneous-dial, long-partition, malformed catch-up,
ejection or overload matrix; it does not grant introduction readiness.

The public baseline content contract is now specified under peer connection
maintenance. `peer::catchup` freezes the complete policy/blocklist/tombstone
projection into bounded canonical pages and verifies formation/source/snapshot,
ordering, totals and an aggregate digest before exposing records. Tests cover
explicit empty collections, immutable capture across policy changes, pagination,
missing/reordered/duplicate/mixed pages, digest/size rejection, lifted blocks
and cleared tombstones. Run `cargo test --locked -p orishu-worker --lib
peer::catchup`. This is content validation only: source retention/expiry,
authenticated peer routes and wire negotiation, atomic core merge, credential
release/installation and completion publication remain required. No worker
readiness changes follow from these helper tests.

Atomic domain installation now has the versioned core
`InstallAdmissionBaseline` command and one correlated applied/rejected outcome.
It stages the normal merge rules against a bounded clone, retaining newer local
facts and rolling back all state/gossip changes on malformed records, conflicts
or capacity failure. Self-removal is explicit; local admission settings and
credential readiness are not changed. `baseline::tests` covers replay, late
failure rollback, stale/empty source views, wrong formation, duplicate keys,
union capacity and self-removal. The worker receiver now produces this command
value only after content completeness validation, retaining formation/snapshot
binding. Authenticated transfer, owner outcome handling/ejection and target
credential installation remain required before catch-up can complete.

`peer::catchup::store` now retains at most four immutable baselines for 30
seconds, with monotonic snapshot IDs, requester/generation binding and no live
eviction. Exact retries preserve bytes and expiry; continuation/finish rejects
skipped pages, wrong roots, unrelated requesters, stale generations and expired
snapshots. Identity checks reuse the registered-session member validator, and
new identity exclusions invalidate retention. Tests cover frozen retry across
source policy changes, expiry, capacity, paging completion and revocation.
This is source storage evidence, not an authenticated route or credential
release: owner readiness/source-network checks and network integration remain
required before a newly admitted worker can complete catch-up.

Profile 3 now connects source-side admission-state request/reply handling to
the real bounded peer dispatcher and serialized owner. The owner validates
current member/session/source restrictions and readiness before serving retained
pages or releasing its token after confirmation. The real-QUIC admission/traffic
test now fetches the descriptor/pages, verifies complete content and rejects
premature confirmation before receiving the expected credential. Wire tests
cover readiness refusal, identity mismatch, size and reply correlation. ALPN
changes from `orishu-membership/2` to `/3` with no downgrade; Merkle hash domains
remain version 2. Receiver scheduling, core-outcome handling, atomic credential
installation and the B-introduces-C process checkpoint remain incomplete.

`peer::catchup::client` now performs a complete receiving-side transfer over an
already registered connection: source certificate binding, correlated descriptor,
sequential page validation, then snapshot/root-bound credential retrieval. It
has a 25-second overall deadline, 128 KiB pre-allocation reply cap and shares the
existing exchange budget. The real QUIC admission/traffic test now serves 40
blocklist records over multiple pages and verifies the received baseline/token
and wrong-pin rejection. The resulting completion is private shell data, not
readiness: receiving-owner scheduling, attempt/generation fencing, core-outcome
handling and explicit credential installation remain required. Full malformed,
expired and interrupted transfer conformance is not established by this test.

Receiving-owner preparation/completion now reserves completion capacity, fences
attempt/generation/source-session identity and limits catch-up to three prepared
attempts within 90 seconds of adoption. Cancellation records failure; explicit
retry transitions the existing operation back to catching up. A validated
completion applies the atomic core command and installs its shell token only
after acceptance and local identity/network-policy checks; self-removal closes
sessions and prevents installation. The runtime adoption test drops one job,
observes retained failure, retries through real QUIC and verifies `joined`,
introducer readiness and the same formation token. This is owner/runtime
evidence, not automatic runtime scheduling or the public B-introduces-C gate;
those and the complete stale/ejection/catch-up fault matrix remain required.

Automatic catch-up is now enabled at production startup with one supervised
transfer and one-second eligibility polling, retaining the owner's attempt and
deadline bounds. The runtime adoption test preserves incomplete-readiness and
cancelled-attempt evidence, then starts the scheduler twice to verify idempotent
startup and automatic retry to `joined`. The public CLI harness now starts three
independent workers, drives A-admits-B then B-admits-C, and verifies historical
source identities, assigned IDs, fingerprints, exact replay, live convergence
and lock/unlock through different entry nodes. No manual peer session or catch-up
invocation drives that process journey.

Validation for this increment: `cargo build --locked --offline -p orishu-worker
-p orishuctl`, `cargo test --locked --offline -p orishu-worker --all-targets
--quiet` (81 tests), `cargo clippy --locked --offline -p orishu-worker --all-targets
-- -D warnings`, and `python3 scripts/check-formation-cli.py` passed. The harness's
first run exposed an incorrect expected CLI summary key; it was corrected to
the actual `alive`/`nodes` output and the full journey rerun successfully.
This is happy-path handoff evidence, not lost-ACK recovery, complete lifecycle
or fault conformance. Failure artifacts and stale-completion/ejection cases
remain required; no observability implementation is claimed.

The owner now handles self-removal from ordinary gossip as well as an atomic
baseline through one ejection fence. It suppresses the transition's entire
effect batch, advances generation, clears credentials/pending catch-up, closes
sessions and stops timers, sends, dialing and periodic membership work. The
real-QUIC `real_join_packet_drives_owner_lock_token_and_admission_outcomes`
regression now sends a self-tombstone in an authenticated datagram and checks
ejection, connection closure, unavailable introduction/catch-up and no further
core transitions from old/new-generation probe submissions. The runtime
`ejection_fences_prepared_catchup_and_its_late_cancellation` test applies a
baseline at the owner boundary while a prepared job is held, then drops its
late completion and verifies the owner remains available and ejected with a
retained catch-up failure. Both focused tests passed. This is not yet public
three-process ejection/restart exclusion or a late successful-transfer fault;
those lifecycle cases remain required.

Validation after the ejection fence: the worker all-target suite passed 82
tests, worker all-target Clippy passed with warnings denied, the rebuilt
three-worker CLI handoff passed, and scoped formatting/whitespace checks passed.
`make docs-check` still fails on the unrelated existing `TODO.md` link
`./docs/adr/0013`; it is not recorded as passing. The public leave route is
still unsupported. Review found that its prerequisite owner path replaced the
model and fenced sessions before executing departure effects; the following
increment corrects that identity boundary.

The owner now captures the bounded old-generation notification routes before
the core leave transition, then submits only validated old-sender `Leave`
datagrams before retiring those sessions. Departure effects are consumed there,
never encoded/routed as replacement-formation work. Missing routes or failed
submission remain non-fatal bounded diagnostics, with no promise of receipt or
flush before closure; SWIM remains the fallback. The production encoding helper
has a regression using real core leave effects and CBOR field decoding, proving
old formation/sender identity after model replacement and rejecting a mismatched
target or unrelated message. The worker all-target suite passed 83 tests and
Clippy passed with warnings denied. This is internal owner/codec evidence, not
public leave acceptance. Identified/authenticated leave routes, retained retry
outcomes, CLI wiring and real departure/loss convergence remain required.

Identified voluntary leave is now connected through typed request/receipt,
serialized owner, authenticated bounded POST route, shared client and CLI.
Process-lifetime history retains 64 successful receipts including no-ops,
checks exact replay before current-formation preconditions and never evicts
into reexecution. New stale/conflicting requests refuse; client cancellation
does not revoke acceptance. Leave from joined/catching-up/ejected state creates
fresh random identities/token and fences old work; joining/unresolved admission
refuses pending its separate recovery contract. The current display label and
private certificate/operator credential are retained. No compute drain or
artifact transfer is claimed.

The owner tests verify receipt replay across identity change, lost caller,
lock-independent departure, no-op, stale/conflicting input and full history.
Real API tests exercise local authorization, exact no-op replay, duplicate
credentials, oversized/malformed bodies and unsupported content/preconditions.
The CLI journey now leaves C and checks fresh identities/token, certificate
retention, exact replay, stale/conflict/no-op and eventual dead state of its old
identity at both survivors. Its first ten-second convergence deadline was
shorter than the default size-scaled suspicion timeout (30 seconds at three
members); the corrected 60-second bounded poll passed. This does not prove
that the lossy announcement arrived, nor does it close deliberately dropped
announcement or interrupted-leave races. Those remain required. The journey
also explicitly readmits C after departure visibility, verifies new assigned
identity with its retained certificate and old dead history, and replays the
old leave request after readmission without changing current membership.

Validation: worker all-target tests passed (85), shared `orishu` tests passed
(93), CLI tests passed (7), scoped all-target Clippy and formatting passed,
and `make docs-check` passed. The rebuilt public CLI journey passed with leave
and readmission. This changes the early client API from parameterless `leave`
to `leave(&LeaveRequest)` and the CLI requires explicit formation/operation
IDs. No full-workspace or observability-feature acceptance is claimed.

The process harness now keeps bounded combined worker-log tails in memory and
retains only a bounded recent non-token CLI journal. Failure writes private
redacted per-worker logs and a versioned report with scenario, exception type,
process IDs and observed exit status; success writes no bundle. State directories,
credential files, join material and command arguments are never archived.
Invalid credential parsing withholds raw evidence rather than risking export.
Tail truncation discards a partial first line, token/key encodings and long
opaque strings are scrubbed, and files/directories have tested private modes.
Identity hex values in raw output are deliberately redacted too.

`python3 scripts/test_formation_evidence.py` passed four tests, including real
subprocess output flood, seeded secrets/encodings, limits, permissions and
fail-closed invalid identities. The real harness's
`--inject-failure-after-startup` mode produced the expected nonzero exit and
private bundle; all three reported worker PIDs were absent after cleanup.
The normal CLI leave/readmission journey then passed with capture enabled.
`make test-formation` now provides one build/evidence-tests/process-journey
entry point. The deliberate failure tests evidence plumbing only, not lost
ACKs, partitions, ejection or other protocol faults. Those matrix rows remain
required; this does not provide production observability exporters.

The CLI harness now exercises joined-worker SIGKILL, observes suspicion then
death at survivors, restarts the killed worker with the same private directory
and Unix socket, and verifies fresh standalone IDs/token with retained
certificate/operator credentials. It explicitly readmits the restarted worker
and checks all three views against the exact five-record identity/fingerprint/
liveness map: two dead historical IDs and three live current IDs. This uses
ordinary process signals and public CLI routes, not a worker fault endpoint.

One run reached the initial ten-second final reconciliation deadline with the
third observer still missing the restarted node's record; all three processes
remained responsive and the private failure bundle captured the mismatch.
Another fresh run passed the exact assertions. The final wait now permits 60
seconds to cover multiple five-second reconciliation rounds and ten-second
round timeouts; the repeat passed. No runtime convergence fix or ten-second
SLO is claimed. The shared evidence helper supports restart only after exit
with the same state directory, reuses a bounded tail, and reaps a child if its
log-reader thread cannot start. Its six tests and `make docs-check` passed.
Certificate-excluded restart, deliberate partitions, interrupted admissions
and the remaining matrix are still required; ordinary restart/readmission
does not establish those guarantees.

Interrupted-admission review found that certificate duplication and policy/
capacity races could pass the final node-ID allocation boundary. The core now
refuses `AlreadyAdmitted` for a certificate already held by an alive/suspected
member in its local view and rechecks every locally held gate immediately
before insertion, after allocation. Dead records remain valid history and do
not by themselves prevent a new assigned identity; active exclusions still do.
This prevents duplicate local insertion, not partition-wide reservation or
lost-ACK outcome recovery.

Core admission regressions cover two outstanding allocations racing on one
certificate, the final local capacity slot, a new lock and a new fingerprint
block; only the first valid admission inserts. Alive/suspected versus dead
history is also covered. Worker codec tests freeze `alreadyAdmitted` CBOR and
the complete NACK envelope, and the real QUIC owner admission test verifies
the same refusal with unchanged membership. The core all-target suite passed
200 tests plus benchmark smoke cases, including dependency-purity checks;
the final worker suite passed 86 tests including the additional real-wire case.
The membership doctest, scoped Clippy/formatting, docs validation and rebuilt
public CLI leave/crash/restart/readmission journey also passed. Interrupted-join
recovery remains required.

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
- Implement bounded admitted-peer dialing/reconnection under the preflight
  contract. Verify that members learned through another introducer can exchange
  traffic without a test harness manually creating their sessions.
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
- Define and test peer-admission configuration independently of binding a peer
  listener. Reject contradictory enabled-role/listener settings at startup;
  merely advertising `accepts.peers` must never bypass the owner-held readiness
  gate. Missing target credentials or incomplete catch-up refuse introduction
  without terminating the membership owner or silently restoring old tokens.
- Keep advertised role, `introducerReady`, join-operation completion and process
  `/readyz` distinct. Coordinate their state matrix with P-OBSERVABILITY: a
  healthy worker deliberately configured not to introduce can be process-ready;
  a locked formation can safely enforce refusal without being unhealthy. A
  successful probe never authorizes admission or proves cluster convergence.
- Distinguish bound from advertised addresses, reject unusable peer endpoint
  claims and define port-zero reporting for tests. Wire the accepted core's
  anti-entropy depth/round limits through the shared configuration path.
- Exercise fresh restart, ejection, cancelled join and shutdown with pending
  effects. Secure certificate persistence is in scope; retained formation
  state and scientific artifact storage are not.
- Test reuse of the same configured Unix socket after graceful shutdown and
  process loss. Cleanup/recovery must preserve active listeners, symlinks and
  unrelated files, reject unsafe ownership/permissions, and report conflicts
  without deleting another process's endpoint.

### 5. Operator API and `orishuctl`

- Implement the production worker routes and library client methods for the
  minimal resource surface above; remove the placeholder success path for
  those routes.
- Wire the existing CLI concepts for `cluster info`, `ls`, `inspect`, `token`,
  `join`, `join-status`, `cluster lock`, `cluster unlock`, and `leave` to those
  real routes. Document bounded polling, pending versus terminal phases and
  exit-status meaning; command exit success alone must not mean join completion.
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
| Simultaneous member dials; connection loss with both processes alive | Session collision handling and bounded reconnect restore real peer exchanges without a new join or duplicate membership; transport loss alone does not write removal state |
| Unreachable or stale advertised endpoints | Dial work/backoff and pending sends remain bounded; recovered routes resume reconciliation, while control/shutdown remain responsive |
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

The criteria below close **N-FORMATION**, except for the explicitly marked
combined M4 gate. Exporter delivery remains in P-OBSERVABILITY and operator
monitoring recipes in P-OBS-DOCS; neither is silently dropped when formation
passes, nor does unfinished companion work make completed formation evidence
disappear. Report the status of all three packages separately at handoff.

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
  produce bounded failures without applying invalid input. Distinguish
  rejection before insertion from a lost or invalid reply after remote
  insertion: the latter must retain an unresolved outcome and use the specified
  recovery procedure, not claim that no admission occurred. Where safe, expose
  structured operator diagnostics; unauthenticated malformed peer input need
  not receive a detailed response.
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
- Every row in that matrix maps to a named test/harness case and its runnable
  command. Record platform prerequisites and any remaining manual checks;
  skipped cases are gaps, not passing evidence. Check in the operator journey
  and its expected assertions so another contributor can reproduce acceptance.
- Before closing the task, consolidate the chronological integration notes into
  a current evidence ledger: checkpoint/scenario, named test, production
  boundary, exact command, result and remaining limitation. Reconcile superseded
  protocol maturity statements and task/roadmap statuses; do not leave both
  “unwired” and “implemented” as current descriptions of the same route.
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
