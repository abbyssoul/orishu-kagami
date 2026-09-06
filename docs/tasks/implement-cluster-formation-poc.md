# Implement the operational cluster-formation PoC

Status: **in progress; readmission observation budget corrected, final conformance open** —
public three-worker introducer handoff has recorded passing evidence;
recovery, lifecycle/fault conformance and operational handoff remain incomplete

Roadmap package: **N-FORMATION**; companion milestone: **M4**

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

## How to use this task

This is an in-progress acceptance plan, not a greenfield implementation brief.
Read the [remaining increments](#remaining-reviewable-increments), then the
[conformance ledger](cluster-formation-conformance.md), before selecting work.
The [numbered implementation slices](#1-close-the-contracts) retain the original
scope; the dated evidence record is historical, not a backlog. The
[acceptance criteria](#acceptance-criteria) determine completion.

This task owns outcomes and acceptance; ADRs own architectural decisions and
the client/peer protocol documents own wire contracts and exact limits. Record
new test results in the conformance ledger rather than extending this task's
historical narrative. A documentation review neither verifies the current
implementation nor authorizes starting the next implementation increment.

For review, use these entry points without reading the historical checkpoints
as new instructions:

- [Remaining work](#remaining-reviewable-increments) and
  [audit selection](#selecting-and-closing-an-audit-increment).
- [Scope and companion ownership](#roadmap-placement-and-dependencies).
- [Preflight contracts](#required-preflight-decisions) and
  [original implementation slices](#1-close-the-contracts).
- [Required failure matrix](#required-failure-and-convergence-evidence),
  [closure gates](#acceptance-criteria) and [non-goals](#non-goals).

## Current gap and next reviewable increment

Use the [current conformance ledger](cluster-formation-conformance.md) for the
row-by-row evidence audit, including the completed scoped pressure scenarios. It
distinguishes tests rerun against the current worktree from historical process
results; partial rows remain open. The detailed checkpoints below preserve
the implementation history, not a claim that every matrix row is complete.

The [baseline audit](cluster-formation-conformance.md#admission-baseline-condition-audit--2026-09-06)
completed the named catch-up condition mapping but exposed an intermittent
post-leave readmission convergence failure in the full process journey. The
[controlled timing follow-up](cluster-formation-conformance.md#controlled-departure-round-and-readmission-budget--2026-09-07)
subsequently disproved the harness's ten-second expectation: the unchanged
round timeout and cadence can delay repair for about fourteen seconds. The
readmission observation budget is now seventeen seconds, with exact identity,
certificate and liveness assertions retained. This is an evidence-based test
budget correction, not a runtime fix or a proven explanation of the historical
process executions. Preserve those failures and the timing regression; final
formation acceptance still requires the complete conformance matrix and final
process reruns, not merely the corrected budget or a passing retry.

The summary distinguishes recorded progress from the checks explicitly rerun
in the linked conformance ledger. The [integration record](cluster-formation-integration-record.md)
preserves earlier reports, including superseded gaps. Do not reopen completed
slices merely because their original implementation instructions remain below.

| Boundary | Recorded evidence | Remaining acceptance gap |
| --- | --- | --- |
| Public formation and introducer handoff | A admits B, B completes automatic catch-up and admits C; exact identities, fingerprints, live views, authenticated status/replay and cross-worker lock/unlock; scoped collision/capacity/policy-ordering, partition/heal, stale-route, datagram, peer/client pressure, stalled-response writes and unfinished-client shutdown evidence in the ledger | Complete limits/authority/recovery matrix audits and final acceptance reruns |
| Catch-up and lifecycle fencing | Real receiver completion fenced after leave/ejection/shutdown/session loss/deadline; bounded attempt exhaustion; expired/truncated/cross-snapshot/bad-digest receiver failures and clean retry; post-adoption wire self-ejection has public process evidence | Final lifecycle-matrix audit and combined process faults; scoped fixture evidence does not establish cluster-wide removal convergence |
| Leave and ordinary restart | CLI leave/readmission, lost departure announcements with SWIM fallback, discarded public leave response with exact receipt recovery, SIGKILL/suspicion/death and restart with retained credentials/fresh identities; separate excluded-restart evidence | Final lifecycle-matrix audit and combined fault cases |
| Interrupted admission | Profile-4 original-assignment replay; public lost-ACK recovery; issuer/source history loss and Dead/removed/certificate-blocked assignments have independent-process stop evidence; certificate-excluded restart refuses fresh admission | Remaining unavailable-history combinations and final recovery-matrix audit; broader fault/convergence evidence |
| Operational handoff | ADR 0017 is accepted; the [observability guide](../orishu-observability.md#implemented-local-surface) documents loopback probes, health gauges, owner counters and lane-slot gauges; the [local Prometheus guide](../testing-worker-prometheus.md) records real scraping, tested example alerts and standalone scraper-outage isolation | Remaining probe/feature/security matrix, broader formation instrumentation, sampled cross-peer traces and deployment/operator handoff; these scoped results do not close either companion task |

### Remaining reviewable increments

Complete these against the existing production seams. Each increment ends with
named regression evidence and synchronized protocol/manual status; none alone
closes N-FORMATION.

Preserve the catch-up lifecycle and receiver-fault regressions in the evidence
ledger, together with deterministic incomplete/cancelled-transfer readiness
and the public A-to-B-to-C handoff. Their bounded wire/runtime coverage does
not replace public ejection/exclusion or process-level fault acceptance.

1. **Retain the corrected readmission timing gate.** The controlled fixture
   and documented seventeen-second process observation budget supersede the
   unsupported ten-second expectation. Preserve the exact-record check and
   historical failures in final process reruns. A failure under the corrected
   budget needs the bounded investigation below; do not repeat unchanged
   diagnostic batches solely to erase historical failures. Remaining matrix
   audits and independent observability work can proceed.
2. **Audit remaining slow-input and ingress pressure.** Use the
   [ledger's pressure checklist and subsequent evidence](cluster-formation-conformance.md#next-implementation-combined-slow-input-and-ingress-pressure).
   First reconcile any existing work with its assertions; add only missing
   evidence or a demonstrated production fix. The original “next” heading is
   not evidence that its cases are still missing. Current documented gaps
   include active incomplete HTTP/2 header/frame timing and the remaining
   flow-control bounds; silent-client expiry is already recorded separately.
   This is an acceptance audit, not a new scheduling or health authority. Preserve the recorded
   collision/recovery, local-capacity, policy-ordering and process partition tests.
   Use the bounded completion contract below rather than repeating the already
   recorded peer saturation and slow-body tests.
3. **Remaining formation conformance and recovery audit.** Map the recorded
   interrupted-admission, excluded-restart and interrupted-leave journeys before
   adding cases. Close outstanding policy races/partition, hostile-input,
   stale-IO and overload rows at their specified boundaries. Use the recovery
   checklist below for preflight area 6: duplicate-certificate rejection alone
   is not recovery, and unverifiable outcomes require a bounded stop condition.
4. **Operational M4 handoff.** Integrate P-OBSERVABILITY slices 1–3 and the
   formation portions of P-OBS-DOCS. This can progress alongside conformance
   once the owner health/outcome contract is fixed. Report exporter and
   documentation status separately from N-FORMATION acceptance.

For each remaining scenario, maintain a current evidence row with checkpoint,
named test/harness case, production boundary, exact command, result and
limitation. Use `not run`, `failed` or `passed` explicitly; historical reports
remain historical until rerun. Derive convergence deadlines from configured
probe/suspicion/reconciliation bounds and record them with the scenario.
Increasing a deadline is not a runtime fix or a measured production SLO.

### Conditional investigation: a new readmission failure

The controlled timing follow-up above has completed the initial investigation's
test-budget correction. The checklist below remains the procedure for any new
failure under that budget, not an instruction to restart the old investigation.

Review one bounded investigation, not a restart of the formation programme.
Use the [recorded failure and diagnostic follow-ups](cluster-formation-conformance.md#admission-baseline-condition-audit--2026-09-06)
as the starting evidence. Do not infer the cause from differing member counts
alone or treat a worker's health probe as proof of membership convergence.

- **Trigger and question:** a new full public journey fails its exact-view
  assertion under the corrected observation budget. Why did surviving views
  fail to converge after voluntary leave and same-certificate readmission?
  Historical ten-second failures alone do not trigger another investigation.
- **Before execution:** record the worktree checkpoint, feature build,
  configured convergence bounds, selected diagnostic command and finite trial
  budget. Reuse existing diagnostic subsets and bounded failure capture before
  adding instrumentation. Preserve identity redaction and process cleanup.
- **Exit artifact:** retain every trial's result and identify either a supported
  cause with a proposed bounded fix/regression test, or an unresolved finding
  with the missing evidence and one proposed next experiment. Exhausting the
  trial budget ends this investigation increment, not the acceptance gap.
- **Fix acceptance, if separately selected:** establish the failing behavior at
  the boundary responsible, verify the correction there, then rerun the full
  public handoff/leave/readmission/crash/restart journey. A shortened diagnostic
  or a larger timeout cannot substitute for that journey.
- **Non-goals:** new recovery authority, automatic restart/readmission,
  unbounded repeated runs, additional public diagnostic APIs, or expanding the
  pressure matrix without evidence linking it to the failure.

This is a conditional procedure, not the default next increment. Reviewing
this task does not start an investigation or authorize implementation.

### Selecting and closing an audit increment

Before implementing another case, turn one remaining ledger gap into a bounded
checklist: required behavior, owning protocol section, existing evidence,
missing assertion, supported transport/feature configuration, and exit command.
Distinguish a missing evidence mapping from a missing test or a demonstrated
implementation defect. An audit may finish with documentation alone when the
existing evidence already establishes the requirement at the right boundary.

The next review should produce one short increment brief containing:

- the exact open ledger row and its owning contract;
- the existing evidence to preserve and the specific missing assertion;
- the bounded implementation/test scope, or an evidence-only mapping if enough;
- the exit command, time/resource budget and required failure artifact; and
- the affected operator instructions and explicit non-goals.

Do not select all remaining increments as one implementation task. An evidence
audit ends with a mapped result or a concrete missing case; implementation of
that case is the next reviewable decision, not an implicit follow-on.

For the next implementation review, present that checklist as one selected
increment with its non-goals and stop condition. Select from the remaining
gaps above after reconciling the latest ledger entries; their numbering does
not require repeating an earlier increment or completing all ingress work
before an independent recovery or observability slice. If the selected case
reveals a new authority or protocol decision, return that decision for review
before expanding the implementation scope.

For ingress, distinguish HTTP/1.1 header/body and socket-write limits from
HTTP/2 header assembly, stream capacity and stream/connection flow control.
A socket-write timeout does not establish a deadline while a peer withholds
HTTP/2 credit. Map the documented Unix and TLS client modes explicitly; tests
over one mode may support a shared handler contract but do not establish the
other mode's handshake or transport-specific behavior.

For recovery and stale IO, enumerate the unresolved outcomes/completion classes
against preflight area 6. Select combined faults that can change an authority,
generation, retained-history or resource-release decision; do not interpret
“combined faults” as an unbounded Cartesian product. Explain equivalent cases
and exclusions in the ledger. Excluding a required acceptance case needs an
explicitly reviewed scope change, not merely a passing nearby test.

Close the increment when its checklist is mapped and its missing assertions
pass, with limitations recorded. Preserve failures and their resolutions;
retries do not erase a failed result. Stop there for review of the next
increment. Final task acceptance still requires the complete matrix and final
validation commands below, not an endlessly expanding set of pressure cases.

### Retained acceptance contract: unfinished client IO

The [shutdown follow-up](cluster-formation-conformance.md#shutdown-with-unfinished-clients--2026-09-06)
now records passing process evidence for the shutdown row below and its
single-owner drain fix. Preserve that regression. There is now scoped evidence
for every case below. The
[header-limit follow-up](cluster-formation-conformance.md#pre-authentication-connection-and-header-limits--2026-09-06)
records exact-capacity, expiry and rejection tests plus public peer/control
progress with sixty incomplete heads. Preserve those regressions too.
The [stalled-write follow-up](cluster-formation-conformance.md#stalled-response-writes--2026-09-06)
adds observed real Unix write backpressure, stopped pipeline processing,
authenticated control progress and timeout-driven reclamation. Audit these
cases at their stated boundaries before closing the ingress increment; do not reimplement
them merely because the original completion contract remains below.

This contract covers the recorded client-facing cases in the ingress increment; it does
not exhaust every supported protocol's ingress audit. Use the PoC's documented
client transport matrix and retain real authenticated peer traffic. Do not add
unsupported transports merely to complete this audit.
Implementation already present in a dirty worktree is not passing evidence:
inspect and validate it before adding another implementation.

Before changing handlers, record the existing header/body read deadlines,
connection/request capacities, response byte/work limits and shutdown budget
in the owning client protocol. Identify which limits apply before authentication;
a mutation semaphore alone does not establish a bound on unfinished headers
or clients that never read a response. If a limit is unspecified, close that
narrow contract first without changing membership or command semantics.

| Case to retain in the audit | Required acceptance evidence |
| --- | --- |
| Unfinished HTTP headers | Hold incomplete requests before authentication. Prove the configured admission/connection bound and read deadline, while legitimate status requests and peer/control work make bounded progress. Release or expire the clients and demonstrate capacity reuse. |
| Slow response reader | Use a valid, bounded response large enough to encounter actual transport backpressure. Keep the reader stalled while proving response buffering/work stays bounded and unrelated control progresses; verify cancellation or deadline releases capacity. A small response fitting in a socket buffer does not prove this case. |
| Shutdown with unfinished clients | Retain incomplete-header and admitted incomplete-body sockets until the workers exit. Trigger the supported shutdown path and assert a single whole-journey deadline, clean process exits and safe client-socket cleanup. Closing test clients before shutdown is not evidence of server-side cancellation. |

Keep the pressure source finite and record evidence that the intended capacity
or backpressure was actually reached. Test cleanup must run on failure too,
but must not manufacture the successful cancellation being asserted. Preserve
the existing full handoff/churn journey after shared harness changes.

The exit artifact is updated conformance rows with exact bounds, commands,
results and limitations, plus any changed client contract and worker manual.
These cases do not require a new public diagnostic endpoint. Process health
routes, stalled-owner probe behavior and telemetry-outage testing remain the
explicit P-OBSERVABILITY companion gate.

### Interrupted-admission recovery completion checklist

The [operator recovery runbook](../cluster-admission-recovery.md) now specifies
the observation budget, identity checks and stop conditions. Source correlation
and issuer inspection are wired. Recoverable ACK loss and issuer-loss stop now
have independent-process evidence; preserve those cases while closing the
remaining retired/excluded and unavailable-history combinations. The next work
is conformance, not another diagnostic endpoint. Preserve the distinction
between existing pending recovery and an exhausted/unverifiable attempt that
must stop.

Subsequent checkpoints below now cover Dead, removed and certificate-blocked
assignments with the original source alive, plus source loss with and without
a surviving issuer. Do not repeat their implementation as missing work. When
resuming this recovery audit, check unverified combinations and the case-to-test table
before closing this slice. Later checkpoints verify certificate exclusion
across fresh admission after restart and receiving-worker post-adoption
self-ejection over authenticated gossip; preserve their stated scope limits.

Start with preflight area 6, not a second implementation of the successful
join path. This slice is complete only when an operator can follow the
documented recovery or stop procedure through supported interfaces.

Treat the recorded correlation, issuer-inspection and replay contracts as the
baseline, not new implementation work. Review their owning protocol sections
before changing behavior. Complete this slice in the following bounded order:

1. Map the existing named tests and commands to the cases below. Reuse evidence
   at its actual boundary; a standalone inspection-response test does not prove
   an interrupted-admission journey. Mark missing combinations `not run`.
2. Add the missing independent-process journeys, arranging faults through
   explicit test-only controls and asserting the public source/issuer views.
   Preserve original operation, attempt, formation and certificate correlation
   throughout; do not reconstruct an attempt from a worker label.

   | Case | Required operator-visible result |
   | --- | --- |
   | Accepted assignment remains usable after ACK loss | The pending attempt recovers the original assigned ID, without another live identity or a reset retry budget; retain the existing regression. |
   | Assignment becomes dead, removed or certificate-restricted before recovery | Exact retry cannot revive the assignment or bypass restrictions. Issuer inspection and source status lead to the runbook's bounded stop/review path; exercise the distinct restriction cases. |
   | Original issuer is unavailable or has restarted | Retain the verified issuer-loss journey: uncertainty survives retry exhaustion, and a restarted issuer cannot substitute fresh history for the original ledger. |
   | Original source restarts with retained credentials | Old operation history is unavailable and the new standalone identity does not recover the old assignment merely by reusing its certificate. Follow the runbook's stop condition without submitting a fresh join as recovery. |
   | A retained source reference has no verifiable issuer record | A missing record, failed lookup or mismatched response is not proof of non-admission. Preserve uncertainty and stop within the documented observation budget. |

3. Re-run the complete lost-ACK/handoff/churn journey after any recovery or
   liveness fix. Admission-only and handoff-only diagnostics remain useful
   regressions but cannot close the full journey. Record repeat counts and
   failures without replacing a failed run with an unexplained successful retry.
4. Reconcile the [worker manual](../../apps/orishu-worker/README.md),
   [CLI manual](../../apps/orishu-ctl/README.md) and
   [cluster administrator stories](../user-stories/orishu/cluster-admin.md)
   with exact inspection targets, bounded waits and safe next actions. These
   formation recovery instructions belong to N-FORMATION; telemetry-based
   troubleshooting remains coordinated with P-OBS-DOCS.

The exit artifact is a case-to-test evidence table plus the tested runbook,
not another status endpoint. Every row above must have a passing journey or
an explicitly reviewed scope change before this recovery slice is closed.
This documentation review does not rerun or upgrade the recorded evidence.

Missing test evidence leaves the acceptance row open. Missing or unverifiable
admission evidence during an operator journey must lead to an explicit
stop/escalation result, not an automatic restart, new attempt, alternate
introducer or exclusion removal.
This slice does not require adding a general administrative removal API,
distributed admission ledger or new recovery service. If a safe path needs
one of those, record the decision and separate scope before implementation.

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

The companion contract is already accepted in ADR 0017; do not substitute an
always-on route in the mutation API or reopen the listener decision here:

| Delivery | M4 obligation |
| --- | --- |
| N-FORMATION | Real owner supervision/progress and bounded diagnostic outcomes; no exporter dependencies in the sans-IO core |
| P-OBSERVABILITY slices 1–2 | Independently feature-gated `observability` capability; configurable dedicated HTTP diagnostics listener for `/metrics`, `/livez`, `/readyz` and `/startupz`; runtime exposure disabled by default |
| P-OBSERVABILITY slice 3 | Independently feature-gated `otlp-tracing`, disabled by default; bounded sampled client/peer traces reaching a test OTLP receiver |
| P-OBS-DOCS formation portions | Tested feature/configuration, secure scrape, probe and collector recipes; updated worker/CLI manuals and worker/cluster operator stories |

Both optional features are excluded from Cargo defaults. Remote metrics require
the ADR's TLS and monitoring-only authorization policy (or its secured proxy
option); a remote probe exemption never opens metrics or mutation routes.
Publish exact settings and the health matrix in the owning observability
documents as the companion contract lands. The table defines the full M4
obligation, not its implementation status. Consult the
[documented local surface](../orishu-observability.md#implemented-local-surface)
for currently reported capabilities; do not infer secured remote metrics,
complete formation instruments or trace export from local probe availability.

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

The imperative wording below preserves the original contract requirements;
it is not a fresh inventory of missing implementation. Resolve each requirement
against the owning protocol and conformance ledger before reopening it. In
particular, recorded trust pinning, policy replication, credential bootstrap
and profile-4 replay remain the baseline unless evidence reveals a gap.

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
gates when applying credential-verification evidence before inserting a member.
Test actual owner ordering; asynchronous verification is not a requirement to
introduce a new pending-job boundary solely to stage this race.

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

Close the recovery contract with an explicit outcome table: no request emitted,
request emitted but acceptance unknown, accepted assignment still usable,
assignment now dead/removed, and introducer or applicant restarted. Bind retry
identity to authenticated credentials and the originating lifecycle, not a
worker label or certificate alone: retaining a certificate on restart must not
restore an old assigned ID. Specify changed-payload conflicts, record capacity,
retention/expiry and behavior when a record is unavailable. Neither eviction
nor reconnect may silently turn an uncertain attempt into a new admission or
reset its total retry budget. Local serialization is not a cluster-wide
exactly-once guarantee across independently partitioned introducers.

If the selected PoC behavior remains explicitly unresolved, the recovery
procedure must name the operator authority and target to inspect, the safe
preconditions for any new admission, bounded waits, and the stop/escalation
condition when those preconditions cannot be established. Demonstrate it through
the supported operator surface; “restart and retry” without checking the old
assignment and exclusions is not an adequate procedure. Record any wire/profile
change and compatibility tests in the owning protocol before implementation.

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

### Evidence record

Previously reported tests and integration results are preserved in the
[historical integration record](cluster-formation-integration-record.md).
They are not fresh verification or a substitute for the acceptance ledger.
Record new results against the checkpoints and failure scenarios below, with
an exact runnable command and the production boundary exercised.

The dated checkpoints below are historical reports too: later entries can
supersede earlier failures or gaps. Use the linked conformance ledger for
current acceptance disposition; append new evidence there rather than another
planning status in this narrative. A documentation-only review does not rerun
these commands or promote a partial row to passed.

#### Catch-up lifecycle conformance — verified 2026-09-05

Command: `cargo test --locked --offline -p orishu-worker --lib runtime::tests -- --nocapture`
passed all 10 runtime tests on Unix with loopback socket access. The following
five cases were added in this increment; names are in `runtime::tests`.

| Named test | Production boundary and assertion | Result / limitation |
| --- | --- | --- |
| `leave_fences_successful_catchup_completion` | Real QUIC baseline/credential fetch, then owner leave before successful completion delivery; fresh standalone IDs/token remain unchanged | Passed; runtime leave path, not a lost HTTP receipt test |
| `ejection_fences_successful_catchup_completion` | Same verified receiver result delivered after self-removal; no token/readiness restoration or further core transition | Passed; ejection injected as an owner baseline command, not a public removal route |
| `shutdown_fences_successful_catchup_completion` | Reserved completion held while runtime/owner shuts down, then released; closed owner cannot revive | Passed; bounded shutdown with held successful result, not every in-flight network shutdown phase |
| `source_session_loss_refuses_successful_catchup_then_retries` | Retire the authorized session without changing formation generation; refuse its late result, then automatically fetch/install through a fresh attempt | Passed; real transport/receiver and owner registry, not a three-process network partition |
| `exhausted_catchup_attempts_remain_non_introducing` | Cancel three reserved preparations; further preparation and automatic scheduling cannot reset the budget or install credentials | Passed; attempt limit, not elapsed 90-second adoption deadline |

The scheduling barrier is compiled only in tests. It waits after the normal
receiver verifies all pages and confirmed credentials, before the normal
reserved completion send; it cannot fabricate a `Completed` value. FIFO
control replies establish that late completion was processed before assertions.
No production protocol, dependency, credential format or runtime behavior was
changed by this increment. It does not close interrupted-admission recovery,
the full process fault matrix or the observability companion tasks.

Additional checks for this increment passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (91 tests: 77 library, 9 binary and 5 integration).
- `cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`.
- `cargo fmt -p orishu-worker -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 dependency-purity/budget tests).
- `make docs-check` (86 Markdown files) and `git diff --check`.

The full workspace, public CLI journey and observability-feature matrix were
not rerun for this test-only increment; earlier results remain historical.

#### Catch-up deadlines and partial transfers — verified 2026-09-05

| Named test | Command filter / production boundary | Result / limitation |
| --- | --- | --- |
| `runtime::tests::adoption_deadline_refuses_successful_catchup_completion` | `adoption_deadline_refuses`; real QUIC fetch and reserved owner completion after adoption expiry | Passed; test-only owner control ages the adoption timestamp to 90 seconds without retiring the valid session or changing generation; this isolates the deadline guard, not wall-clock scheduling latency |
| `peer::catchup::client::tests::expired_continuation_refuses_partial_baseline_and_allows_retry` | `peer::catchup::client::tests`; actual source store/wire handler and receiver over mTLS QUIC | Passed; controlled source time expires the lease after page zero; production continuation returns `unavailable` |
| `peer::catchup::client::tests::truncated_page_frame_releases_exchange_capacity_for_retry` | Same receiver filter; declared frame length exceeds delivered final-page bytes | Passed; transport failure is bounded and a fresh transfer succeeds on the same connection/pool |
| `peer::catchup::client::tests::cross_snapshot_page_refuses_partial_baseline_and_allows_retry` | Same receiver filter; final page names another snapshot | Passed; no complete baseline or credential request escapes |
| `peer::catchup::client::tests::incorrect_digest_never_requests_credential_and_allows_retry` | Same receiver filter; descriptor root disagrees with both complete pages | Passed; digest failure precedes credential confirmation |

Run each filter with
`cargo test --locked --offline -p orishu-worker --lib FILTER -- --nocapture`.
All four receiver cases fetch two pages, fail after partial progress without
requesting a credential, then complete a fresh transfer using the same QUIC
connection and a one-slot exchange pool. The test source uses admitted-member
fixtures and the real retention/resource handler; it does not run the production
handshake registry, receiving owner or CLI. The runtime deadline case separately
checks retained `catchUpFailed`, target identity, no introduction/token, and
refusal to prepare another attempt. These are test-only seams and no production
behavior, dependency or persisted/wire format changed.

Final validation for this increment passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (96 tests: 82 library, 9 binary, 5 integration).
- `cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`
  and `cargo fmt -p orishu-worker -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 tests), `make docs-check` (86 Markdown files), and `git diff --check`.

The full workspace, public process journey and optional observability-feature
matrix were not rerun. Interrupted-admission recovery is the next implementation
increment; the wider process fault matrix and operational handoff remain open.

#### Interrupted admission — bounded core rebinding, verified 2026-09-05

The core now accepts `RebindJoin` only for a matching pending session/target
and a distinct replacement session. It consumes the next existing retry,
cancels the old timer, fences old-session replies and cannot restart an
exhausted attempt. A duplicate `BeginJoin` no longer overwrites a pending
attempt. Early refusal/redirect retries also cancel their superseded timer.

`cargo test --locked --offline -p orishu-membership --test join --quiet`
passes 25 tests, including:

- `reconnect_preserves_join_budget_and_fences_old_timer_and_reply`;
- `stale_or_retargeted_reconnect_and_duplicate_begin_do_not_reset_a_join`;
- `repeated_reconnects_exhaust_the_original_join_budget_without_resurrection`;
- the extended `a_rejection_backs_off_and_retries` timer regression.

These are pure transition tests, not reconnect or lost-ACK acceptance. At this
checkpoint the worker redial integration below had not yet been verified.
Introducer attempt identity, accepted-assignment retention/replay, retired
assignment outcomes and real lost-ACK recovery tests remain required. No wire
profile or persisted format changed; downstream exhaustive matches over the
public `Command` enum must handle the new local variant.

The combined core/worker all-target test command
`cargo test --locked --offline -p orishu-membership -p orishu-worker --all-targets --quiet`
passed 203 core tests plus benchmark smoke cases and 96 worker tests. Scoped
all-target Clippy with `--locked --offline` and warnings denied, scoped format
checks and `git diff --check` passed. Full-workspace, public CLI and optional
observability acceptance have not been rerun for this increment.

After extending the refusal-timer assertion, the 25-test join suite and
`cargo test --locked --offline -p orishu-membership --doc --quiet` (1 doctest)
passed again. `make docs-check` passed for 86 Markdown files.

#### Pending-join redial — runtime integration, verified 2026-09-06

Production peer maintenance now starts one idempotent pending-join redial
supervisor. The owner retains the public operation's original secret-free
route/pin and permits one job at a time, at most eight preparations while
core retries remain. Each preparation reserves completion capacity before IO;
cancellation consumes the reconnect budget and clears the pending reservation.
Generation, previous session, reconnect ordinal, participation and the original
target binding are rechecked before invoking `RebindJoin`. The job contains no
token; the normal owner send path alone attaches it after revalidation.
The scheduler reuses the existing endpoint, dialer, shared IO budget and
handshake deadlines, with one-second polling/preparation waits and a 16-second
outer job deadline. Lifecycle changes and shutdown cancel supervised jobs.

| Named runtime test | Production boundary | Result / limitation |
| --- | --- | --- |
| `pending_join_redials_original_introducer_after_pre_insertion_disconnect` | Drop a real decoded JoinReq connection before owner insertion, then automatic pinned redial, original operation adoption and catch-up | Passed; pre-insertion transport recovery, not accepted-outcome replay |
| `cancelled_join_reconnect_jobs_release_reservation_without_resetting_budget` | Hold/cancel eight real owner preparations; one-active-job guard and automatic scheduling cannot reset the budget | Passed; preparation/cancellation cap, not eight timed-out network dials |
| `shutdown_fences_successful_join_reconnect_handshake` | Complete a real pinned replacement handshake, shut down with its reserved result held, then release it | Passed; no owner revival or remote insertion after shutdown |

Run each named filter with
`cargo test --locked --offline -p orishu-worker --lib FILTER -- --nocapture`.
Fault controls are compiled only in tests; no network fault endpoint or protocol
profile change was added. Introducer attempt identity, retained acceptance and
lost-ACK recovery remain the next required integration boundary.

Final validation passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (99 tests: 85 library, 9 binary, 5 integration).
- `cargo clippy --locked --offline -p orishu-membership -p orishu-worker --all-targets -- -D warnings`
  and `cargo fmt -p orishu-worker -p orishu-membership -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 tests).
- `cargo build --locked --offline -p orishu-worker -p orishuctl`, then
  `python3 scripts/check-formation-cli.py` (public handoff, leave, crash/restart
  and readmission journey). The build retains the existing `proc-macro-error2`
  future-incompatibility warning; it did not fail.
- `make docs-check` (88 Markdown files) and `git diff --check`.

Full-workspace and observability-feature acceptance were not run. The public
journey remains a successful operator flow, not the full fault matrix.

#### Accepted-assignment replay — profile 4, verified 2026-09-06

The shell now generates one fresh attempt ID per BeginJoin and preserves it
across redial/retry. The introducer records at most 1,024 accepted requests per
local formation lifetime, keyed by authenticated fingerprint and attempt ID,
with a canonical public-request digest and assigned node ID. Capacity is checked
before insertion and the acceptance is recorded before its ACK. Records never
evict into reexecution; current authorization and member liveness/exclusion
checks precede replay. No credential/snapshot enters the ledger or core.

`runtime::tests::lost_join_ack_recovers_original_assignment_after_redial`
passes through the real runtime/QUIC path: discard acceptance after insertion
and ledger commit, close the connection, redial automatically, recover the
original ID, and complete catch-up. Exact operator replay remains the same
operation and the introducer keeps exactly two members.

The extended
`peer::registry::tests::real_join_packet_drives_owner_lock_token_and_admission_outcomes`
passes same-connection replay after promotion without another core transition;
wrong token, changed body and a new attempt on an admitted session refuse over
real streams. Wire tests check attempt-ID CBOR, credential/sequence exclusion
from the request digest, and changed public-body detection. The ledger capacity
test fills all 1,024 entries, preserves exact replay and refuses conflict/overflow
without eviction. Run the named tests with
`cargo test --locked --offline -p orishu-worker --lib FILTER -- --nocapture`.

Compatibility: ALPN is now `orishu-membership/4`; required `attemptId` changes
JoinReq, so older PoC binaries must be rebuilt/restarted together. Membership
protocol version 1 and Merkle hash version 2 remain unchanged. The prior-profile
TLS rejection test now offers profile 3. Operator manuals document replay
capacity and distinguish live-assignment recovery from unresolved outcomes.
Retired/excluded assignments, introducer loss/restart, unavailable ledger
recovery and the public process fault matrix remain incomplete acceptance work.

Final checks passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (101 tests: 87 library, 9 binary, 5 integration).
- `cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`
  and `cargo fmt -p orishu-worker -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 tests).
- `cargo build --locked --offline -p orishu-worker -p orishuctl`, followed by
  `python3 scripts/check-formation-cli.py` against profile-4 binaries: public
  handoff, leave, crash/restart and readmission passed. The existing
  `proc-macro-error2` future-incompatibility warning remains.
- `make docs-check` (88 Markdown files) and `git diff --check`.

The full workspace and optional-observability matrix were not rerun. The
ordinary three-worker journey is not the deliberate lost-ACK process fault test;
that fault currently has two-runtime, real-QUIC evidence as described above.

#### Retired/excluded assignment replay — verified 2026-09-06

The extended
`peer::registry::tests::real_join_packet_drives_owner_lock_token_and_admission_outcomes`
now checks the following additional cases through the production owner and
bounded codec:

| Scenario | Required assertion | Result / boundary |
| --- | --- | --- |
| Lock after acceptance | Exact replay retains the assigned ID and lock, without another core transition | Passed over the promoted connection's real stream |
| Remove the accepted member | A fresh applicant handshake cannot replay the removed ID; a fresh attempt still receives `tombstoned` | Passed; owner removal command, then real reconnect/JoinReq streams |
| Block the accepted fingerprint | Exact replay refuses; a fresh attempt still receives the fingerprint `blocklisted` reason | Passed; owner block command, observed old-session retirement, then real reconnect/JoinReq streams |
| Mark the accepted ID dead through self-departure | Exact replay cannot resurrect it; fresh admission assigns another ID and retains old dead history | Passed; serialized owner datagram input for departure, then real reconnect/JoinReq streams |

Command:
`cargo test --locked --offline -p orishu-worker --lib real_join_packet_drives_owner -- --nocapture`.
Fresh applicant handshakes succeed in these cases, so replay refusal cannot be
attributed merely to the already-retired old connection. No new production
behavior, schema or protocol profile changed in this test increment. Public
certificate-excluded restart/ejection, unavailable replay-state diagnostics,
and a bounded operator recovery procedure remain required; these negative
checks do not establish that operator workflow.

Final validation passed: `cargo test --locked --offline -p orishu-worker
--all-targets --quiet` (101 tests: 87 library, 9 binary, 5 integration),
`cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`,
`cargo fmt -p orishu-worker -- --check`, `make docs-check` (88 Markdown files),
and `git diff --check`. The test count is unchanged because the existing
parameterized wire test gained cases. Full-workspace, public CLI journey and
observability-feature checks were not rerun for this test-only increment.

#### Retained admission correlation — verified 2026-09-06

JoinOperation resource version 2 now includes an explicit nullable
`recoveryReference` containing the peer attempt ID, local applicant fingerprint
and original introducer ID/pin. The serialized owner binds it in the admission
start turn and retains it independently of transient IO. It cannot be replaced
by a changed reference. This is correlation only: issuer-side inspection,
unavailable-history outcomes and the operator recovery/stop procedure remain
required. JoinRequest/JoinMaterial version 1 and peer profile 4 are unchanged;
worker and operator clients must be rebuilt together for the resource change.

- `join_operations::tests::recovery_reference_is_immutable_retained_and_required_on_the_wire`
  passes JSON roundtrip, required explicit null, rejected old schema, immutable
  binding, unresolved lifecycle retention and secret exclusion checks.
- `runtime::tests::lost_join_ack_recovers_original_assignment_after_redial`
  now also verifies the original reference and local certificate through real
  QUIC recovery and exact operator replay. Run these filters with
  `cargo test --locked --offline -p orishu-worker --lib FILTER --quiet`.
- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  passed 102 tests (88 library, 9 binary, 5 integration).
- `cargo test --locked --offline -p orishu --lib --quiet` passed 93 tests.
- `cargo build --locked --offline -p orishu-worker -p orishuctl`, then
  `python3 scripts/check-formation-cli.py` passed the public three-process
  handoff/leave/crash/restart/readmission journey. New assertions check explicit
  null in the initial version-2 receipt and the adopted reference's identities
  through CLI JSON; exact replay retains the same status. This is not the
  deliberate lost-ACK process fault or the unresolved operator procedure.
  The build retains the existing `proc-macro-error2` future-compatibility warning.
- Scoped all-target Clippy for `orishu` and `orishu-worker` with `--locked
  --offline` and warnings denied, scoped format checks, `make docs-check`
  (88 Markdown files) and `git diff --check` passed.

Full-workspace and optional-observability checks were not run for this increment.
This does not close interrupted-admission recovery or formation conformance.

#### Issuer-side admission inspection — verified 2026-09-06

The operator-only read-only admission inspection now connects a version-1
shared request/report, bounded owner lookup, HTTP handler, library client and
`orishuctl admission-inspect`. It reports wrong issuer, absent retained record,
current member or retired/restricted assignment. The report echoes correlation
and actual source identities; it never authorizes retry or turns missing
history into proof of non-insertion. Request size is 4 KiB, handler deadline
five seconds, and the handler shares the 16-request inspection budget.
No peer profile or persisted format changed; the new client trait method and
response variant require downstream implementations/matches to be updated.

`runtime::tests::lost_join_ack_recovers_original_assignment_after_redial`
now verifies issuer lookup of the original assignment through the serialized
owner after real QUIC recovery. The existing full-capacity ledger test checks
retained lookup and absent fingerprint/attempt pairs without admitting or
evicting records. These tests do not establish a public unresolved recovery
procedure or cluster-wide absence/exclusion proof.

`cargo test --locked --offline -p orishu-worker -p orishu -p orishuctl
--all-targets --quiet` passed 202 tests: 93 shared-library, 102 worker and
7 CLI tests. Scoped Clippy with the same three packages, all targets and
warnings denied passed, as did scoped format checks, `make docs-check`
(88 Markdown files) and `git diff --check`. The existing `proc-macro-error2`
future-incompatibility warning remains. Full-workspace and observability
acceptance were not run. Operator recovery/stop workflow acceptance remains
open even with diagnostic lookup available.

After `cargo build --locked --offline -p orishu-worker -p orishuctl`,
`python3 scripts/check-formation-cli.py` passed the public three-process
journey. New CLI assertions cover current assignment, absent attempt, wrong
issuer, retired assignment after voluntary leave and wrong issuer after its
restart, with exact echoed identities and unchanged repeated reports. Real
Unix HTTP checks reject missing operator authority, a join token used as the
credential, malformed/oversized bodies, content encoding, query parameters and
unsupported preconditions. This remains the ordinary handoff/lifecycle journey,
not the deliberate lost-ACK process fault or full overload conformance.

#### Recovery runbook and report correlation — verified 2026-09-06

The linked recovery runbook specifies current-versus-historical identity and
participation checks, a finite 300-second operator observation budget, unchanged
pending-attempt recovery and explicit stop conditions. Its budget is grounded
in current 183-second default retry windows, 15-second initial handshake and
90-second catch-up limits; it is a workflow cutoff, not a runtime completion
guarantee. Runbook documentation is not independent-process fault evidence.

The client now rejects a non-`wrongIssuer` report whose actual source
formation/node contradict the requested issuer. The new
`client::http_client::tests::admission_inspection_correlates_request_and_issuer_but_preserves_unknown_outcomes`
test exercises eight actual HTTP/CBOR response cases: valid current member,
changed attempt, changed source formation, changed source node, explicit wrong
issuer after restart, unavailable record, retired assignment and unsupported
schema. The transport fixture does not stand in for a worker process.

`cargo test --locked --offline -p orishu --all-targets --quiet` passed 94 tests;
scoped all-target Clippy with warnings denied, format check, `make docs-check`
(89 Markdown files) and `git diff --check` passed. Worker/CLI public journey,
full workspace and observability matrix were not rerun for this client/runbook
increment. Deliberate interrupted-admission process acceptance remains next.

#### Independent-process ACK loss — partial evidence, failures open 2026-09-06

`formation-fault-test` is an explicitly selected development-only worker
feature, absent by default and compile-rejected without debug assertions. Its
startup-only switch arms the existing one-shot post-insertion/ledger-commit
ACK-loss hook. It requires loopback peer binding, Unix clients and an explicitly
supplied state directory. No network fault-control route is introduced.
`make test-formation-lost-ack` builds into `target/formation-faults` so fault
artifacts do not replace ordinary operator binaries.

The public harness now requires an observed fault marker whose original
assigned ID matches the recovered version-2 status, exact operation replay,
issuer inspection and membership map. Its `--admission-only` diagnostic subset
passed across independent OS processes, including graceful shutdown:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl \
  --lost-join-ack --admission-only
```

**Full scenario failed and remains open.** Two runs without `--admission-only`
passed recovery/handoff/churn assertions but failed final graceful shutdown;
worker A reported an owner failure after SIGTERM (worker C also did on one
run). Two subsequent runs stopped earlier at the unchanged three-worker live
convergence deadline. No deadline was increased and no success criterion was
removed. The diagnostic subset is not a substitute for those failing rows or
the mandatory unresolved/issuer-loss stop journey. A temporary, feature-only
`[DEBUG-formation-shutdown]` classification in the worker supervisor was retained
for diagnosis at this checkpoint; it has since been removed with the supervision
race regression recorded below.

Default and feature-enabled worker suites each passed 103 tests (88 library,
10 binary, 5 integration). Feature-enabled all-target Clippy with warnings
denied passed. `cargo rustc --locked --offline -p orishu-worker --lib --features
formation-fault-test -- -C debug-assertions=no` failed with the intended
compile-time rejection (negative guard check, not a successful build).
Full-workspace and observability acceptance were not run. Preserve these
failures at handoff rather than reporting the new process fault as accepted.

#### Acknowledged-shutdown supervision race — verified 2026-09-06

The error previously printed as an owner failure was reproduced as
`PeerAdapterUnavailable`. Runtime supervision could observe peer-listener exit
after shutdown published `Stopping` and acknowledged the request, but before
the real owner dropped its published view. It incorrectly treated the still
readable view as proof that the listener failed while running, then returned a
synthetic failure even when the owner completed successfully.

`runtime::tests::peer_listener_exit_during_acknowledged_shutdown_preserves_owner_success`
failed before the fix and passes afterward. A test-only gate holds the real
owner after its shutdown acknowledgement, and the production listener-exit
branch is polled in that interval before cleanup is released. Supervision now
recognizes published `Stopping` and preserves the actual owner task result.
`runtime::tests::unexpected_peer_listener_exit_still_fails_and_stops_owner`
verifies the running-state failure path remains intact. No error category is
blanket-ignored and no membership/protocol decision changes.

Default and `formation-fault-test` worker all-target suites passed 105 tests
each (90 library, 10 binary, 5 integration). Default scoped Clippy with warnings
denied, scoped format check, `make docs-check` (89 Markdown files) and
`git diff --check` passed. Temporary diagnostic tags were removed from source
and harness. Full-workspace and observability checks were not run. Earlier
intermittent three-member convergence failures are not explained by this fix
and remain open; do not turn a later passing journey into a claimed fix for them.

After rebuilding fault artifacts with `cargo build --locked --offline
-p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test
--target-dir target/formation-faults`, the original full command passed:
`python3 scripts/check-formation-cli.py --worker
target/formation-faults/debug/orishu-worker --ctl
target/formation-faults/debug/orishuctl --lost-join-ack`. This includes the
original-assignment fault marker, public handoff/churn assertions and clean
termination of all workers. The existing `proc-macro-error2` build warning
remains. No observation deadline was increased.

#### Issuer loss after insertion — process stop path verified 2026-09-06

The development-only `--test-crash-after-join` fault exits the original issuer
with code 86 inside the serialized owner turn, after insertion and accepted
ledger commit but before returning the ACK. It shares the fault feature's
normal-build exclusion, release guard and local/private startup restrictions,
and conflicts with the recoverable ACK-loss switch. Its marker contains only
the original assigned identity and proves the insertion boundary was reached.

After rebuilding isolated fault artifacts, this command passed:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --issuer-loss
```

`make test-formation-issuer-loss` reproduces the build and command. Three real
workers and public CLI/Unix/QUIC paths are used. The source exhausts its real
default retry windows (183 seconds total; 210-second harness cutoff with
one-second status polling), with no clock hook or shortened budget. Assertions
verify retained `unresolved`, unchanged recovery reference, unchanged source
formation/node and one-member state, exact request replay without a new dial,
and refusal of a new join ID and voluntary leave. Original issuer inspection
fails while it is offline. Restarting only that dead issuer produces
`wrongIssuer` for the old reference; the source remains unchanged and is never
restarted or sent to another introducer. Surviving/restarted workers shut down
cleanly with bounded cleanup. This verifies the runbook's mandatory stop path,
not durable recovery, a cluster-wide non-insertion proof or retired-assignment
clearance.

Default and fault-enabled worker all-target suites each passed 106 tests
(90 library, 11 binary, 5 integration), including normal-build rejection and
fault-switch mutual exclusion. Fault-enabled all-target Clippy with warnings
denied, scoped formatting, `make docs-check` (89 Markdown files) and
`git diff --check` passed. The existing `proc-macro-error2` build warning remains.
The full recoverable lost-ACK/churn journey, workspace-wide checks and optional
observability matrix were not rerun in this increment; earlier convergence
failures remain open.

#### Lost suspicion notification — regression verified 2026-09-06

The convergence failure was reproduced with all three expected identities
present: A held B as `Suspected(0)`, while B and C held B as `Alive(0)`.
Fault-build instrumentation showed repeated direct contact from the locally
suspected member. An old-incarnation alive claim correctly cannot clear a
suspicion, but once the original notification/gossip was lost, later contact
did not remind the subject to refute. A shortened `--handoff-only` process
loop reproduced this on its fourth run before the fix.

The core now sends one existing `Announce(Suspect)` directly to a subject when
its authenticated Ping or correlated direct ACK leaves it locally suspected.
This does not clear suspicion, extend its timer, change incarnation ordering,
or resurrect Dead membership. It introduces no timer, queue, IO dependency or
wire/profile change; at most one additional datagram is emitted per validated
direct contact. The peer protocol records the behavior and bounds.

`direct_contact_reissues_a_lost_suspicion_without_clearing_or_extending_it`
failed before the fix and passes afterward for both Ping and ACK. It drops the
original notification, retains the original suspicion timer, delivers the
reminder to a subject core, and adopts only that subject's newer refutation.
`suspicion_reminders_do_not_follow_unknown_acks_dead_records_or_newer_refutations`
covers the negative boundaries. Both are in the membership `swim` integration
suite. Ten consecutive independent-process diagnostic journeys then passed
with the original ten-second convergence deadline:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl \
  --lost-join-ack --handoff-only
```

`--handoff-only` intentionally omits later leave/crash churn and is not the
full acceptance harness. The core all-target suite passed 205 tests plus
benchmark smoke cases, including dependency purity; its doctest passed.
The default worker suite passed 106 tests. Default scoped core/worker Clippy
with warnings denied, scoped formatting, `make docs-check` (89 Markdown files)
and `git diff --check` passed. Temporary diagnostic tags were removed.
Full-workspace and operational observability acceptance were not run.

Follow-up validation recovered the full lost-ACK/handoff/leave/crash/restart
harness result: **passed**, using the command above without `--handoff-only`.
Feature-enabled validation was rerun after the earlier test handles were no
longer available: `cargo test --locked --offline -p orishu-worker --features
formation-fault-test --all-targets --quiet` passed 106 tests, and
`cargo clippy --locked --offline -p orishu-membership -p orishu-worker --features
orishu-worker/formation-fault-test --all-targets -- -D warnings` passed.
This closes that reproduced regression, not the remaining conformance matrix.

#### Source history loss after issuer loss — verified 2026-09-06

`make test-formation-issuer-loss` **passed** after extending the independent-
process journey. It first preserves the existing real retry-exhaustion and
issuer-restart stop assertions, then deliberately kills and restarts the
unresolved source with its same private directory. Public CLI checks prove
retained certificate/operator credentials, fresh standalone formation/node
identities, exactly one live local member, and authenticated `UnknownOperation`
for the old request. The saved reference still yields `wrongIssuer`; inspection
does not change the restarted source. No new join is submitted. The injected
source crash is a conformance fault, not an operator recovery instruction.

Boundary limitation: both original process histories are lost in this case.
At this checkpoint, source loss with a surviving original issuer/retained
assignment remained open; the following checkpoint addresses it. Interrupted
retired/excluded assignment journeys and the rest of the failure matrix remain
open. No durable recovery or new admission authority is introduced.

Current-worktree validation also **passed**:

- `cargo fmt --all -- --check`;
- `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`;
- `cargo test --locked --offline --workspace --all-targets --quiet`;
- `cargo test --locked --offline --workspace --doc --quiet` (one existing
  ignored example in `crates/orishu/src/model/mod.rs`, not verified);
- `python3 scripts/test_formation_evidence.py` (six tests);
- `make docs-check` and `git diff --check`.

Cargo reported the existing `proc-macro-error2 v2.0.1` future-incompatibility
warning. Optional observability acceptance was not run and remains incomplete.

#### Source loss with retained issuer history — verified 2026-09-06

`make test-formation-source-loss` **passed**. The harness's `--source-loss`
scenario uses the existing development-only lost-ACK hook after real insertion,
then briefly pauses the issuer with Unix `SIGSTOP`. It requires public source
status to remain `admitting` with a retained recovery reference; missing that
pre-adoption window fails rather than accepting an ordinary completed join.
It kills/restarts only the source with retained private files and resumes the
same issuer with `SIGCONT`. Failure cleanup kills and reaps paused workers.

Public authenticated CLI assertions establish missing source history
(`UnknownOperation`, not an arbitrary transport/authentication failure), fresh
standalone formation/node IDs, retained certificate/operator credentials and
one local live member. The original issuer must still report the original
assigned ID against its unchanged formation/node identity and the applicant
fingerprint, with no extra member allocated. Inspection does not alter the
restarted source, and no new join is submitted as recovery.

The initial direct run and five consecutive repetitions **passed**:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --source-loss
```

This is an independent-process missing-history/stop journey, not a durable
recovery mechanism. The retained assignment may be current or liveness-retired
when inspected; either requires stopping after source history loss. Deliberate
Dead/removed/certificate-restricted replay while the original source survives
is still a separate outstanding case. No runtime, wire/profile or production
fault-control change was needed. Worker manual, runbook and administrator
story describe the tested boundary and do not prescribe the injected restart.

The full `--lost-join-ack` handoff/leave/crash/restart journey also **passed**
after the harness change. `python3 scripts/test_formation_evidence.py` passed
six tests; `make docs-check` (89 Markdown files) and `git diff --check` passed.
Workspace Rust validation from the preceding checkpoint was not rerun for
this Python/Makefile/documentation-only increment. The fault-build Make target
reported the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.

#### Dead assignment with surviving source — verified 2026-09-06

The independent-process `--dead-assignment` journey **passed**:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --dead-assignment
```

After real insertion/ledger commit and lost acceptance, the harness requires
public source status to remain `admitting`. It pauses that same source with
Unix `SIGSTOP`, lets the surviving issuer's normal SWIM timers mark the original
assignment Dead (60-second observation cutoff), and resumes the source with
`SIGCONT`. No private membership edit, fake clock, removal command or new
production fault hook establishes retirement. The original retry budget is
unchanged; source exhaustion has a 210-second observation cutoff after resume.

During recovery, public polling checks the retained reference, pending or
unresolved phase, exact issuer member-ID set and Dead liveness. At exhaustion,
the source retains its original standalone identities with `joinUnresolved`
participation; exact operator request replay returns the same record, while
new join and leave requests refuse. The issuer reports `retiredOrRestricted`
for the original ID. Inspection does not change source state. Both original
processes survive until bounded graceful cleanup.

The paired real-wire test
`peer::registry::tests::real_join_packet_drives_owner_lock_token_and_admission_outcomes`
also **passed**, using
`cargo test --locked --offline -p orishu-worker --lib real_join_packet_drives_owner -- --nocapture`.
It directly exercises replay rejection on real reconnect/JoinReq streams;
the process journey establishes the operator-visible recovery/stop boundary,
not an exact count of those wire rejections.

This closes the liveness-Dead combination only. Removed/tombstoned and
certificate-blocked assignments, excluded restart/ejection and the other
formation failure rows remain distinct requirements. The worker manual,
recovery runbook and administrator story reflect that limit. There is no
runtime, wire/profile or authority change in this increment.

The second isolated run through `make test-formation-dead-assignment` also
**passed**, as did the shared-setup regression `make test-formation-source-loss`.
`python3 scripts/test_formation_evidence.py` passed six tests; `make docs-check`
(89 Markdown files) and `git diff --check` passed. Full workspace Rust and
combined observability acceptance were not rerun for this harness/docs change.
Fault builds still report the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning.

#### Removed assignment with surviving source — verified 2026-09-06

The independent-process `--removed-assignment` journey **passed**:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl \
  --features orishu-worker/formation-fault-test --target-dir target/formation-faults
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --removed-assignment
```

`make test-formation-removed-assignment` provides the equivalent build/run
shortcut. The development-only `--test-remove-after-join` startup hook discards
acceptance after real insertion and ledger commit, then submits the existing
force-removal command through the serialized owner before publishing its
secret-free assigned-ID marker. It never edits the membership map directly.
The hook is one-shot, shares the loopback/Unix/private-directory guards and
cannot be combined with the other startup faults. Ordinary builds reject it.

Public process checks prove the original pending operation retains its
reference and identities through bounded retry exhaustion, while issuer
inspection reports `retiredOrRestricted` and its exact member set contains
neither the removed assignment nor a replacement. Exact operator replay
returns the same exhausted record; new join/leave requests refuse. Both
original processes survive until graceful cleanup. This is a test-arranged
removal, not an administrative API or a suggested operator recovery action.

Default and `formation-fault-test` worker all-target suites each **passed 107
tests**, including switch availability/conflict checks. Scoped worker Clippy
with warnings denied passed both configurations; `cargo fmt --all -- --check`,
six evidence-helper tests, `make docs-check` (89 Markdown files) and
`git diff --check` passed. No wire/profile, dependency or production authority
change was introduced. Certificate-blocked recovery, excluded restart/ejection,
other formation fault rows and observability remain open. Full-workspace
acceptance was not rerun in this increment.

The full process harness with `--lost-join-ack` also **passed** after the fault
selector changed, preserving original-assignment recovery, three-worker
handoff, leave, crash/restart and readmission. Fault-build Cargo commands still
report the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.

#### Certificate-blocked assignment with surviving source — verified 2026-09-06

`make test-formation-blocked-assignment` **passed**. It builds the isolated
development fault worker and runs the public `--blocked-assignment` journey.
The one-shot `--test-block-after-join` startup hook applies the existing
`UpdateBlocklist` command to the authenticated applicant fingerprint after
real insertion/ledger commit, before delivering acceptance. It uses the
serialized owner, not a private membership-map edit, and publishes the
secret-free assigned-ID marker after the transition. The switch is absent
from ordinary builds, mutually exclusive with all other startup faults, and
subject to the same loopback/Unix/explicit-private-directory guard.

The original source remains pre-adoption and exhausts its unchanged retry
budget within the 210-second observation cutoff. Throughout recovery, public
issuer inspection reports `retiredOrRestricted`, the member record retains
the original fingerprint, and the exact member-ID set has no replacement.
The source retains its original identity/reference, exact request replay
returns the same exhausted record, and new join/leave work refuses. Both
original processes survive until bounded graceful cleanup. No certificate
rotation, exclusion removal, public blocklist API or new recovery authority
is introduced.

Default and fault-enabled worker all-target suites each **passed 108 tests**.
Scoped worker Clippy with warnings denied passed both configurations;
workspace formatting, six evidence-helper tests, `make docs-check` (89 Markdown
files) and `git diff --check` passed. An initial compile check exposed an
incorrect blocklist type import path, corrected before these passing checks.
No full-workspace or observability acceptance was rerun in this increment.
The existing `proc-macro-error2 v2.0.1` future-incompatibility warning remains.

This proves certificate-blocked interrupted recovery with the original source
alive. Excluded restart followed by fresh admission, post-adoption ejection,
remaining unavailable-history combinations and the broader fault matrix still
require their own evidence. The runbook and operator story preserve these
limits; passing this case alone does not close the recovery slice or M4.

The full `--lost-join-ack` independent-process handoff/leave/crash/restart
journey also **passed** against the rebuilt fault worker after this change.

#### Excluded restart and fresh admission — verified 2026-09-06

`make test-formation-excluded-restart` **passed**, including a successful
different-certificate control. The earlier direct `--excluded-restart` run
also passed before that control was added; it is narrower evidence, not a
second run of the complete final scenario.

The journey uses the existing certificate-block-before-ACK hook, requires
the original source to remain pre-adoption, then kills/restarts only that
source with its private directory retained. Public CLI checks establish the
same certificate/operator credentials, fresh standalone formation/node IDs,
one local member and `UnknownOperation` for the original request. The same
surviving issuer still reports the original assignment `retiredOrRestricted`.

As a deliberate negative test—not operator recovery—the harness submits a
fresh operation with current standalone preconditions and original issuer
material. It checks that a new peer-attempt ID uses the same blocked
fingerprint, never adopts or allocates a replacement, and reaches `unresolved`
within the unchanged 210-second observation cutoff. Exact replay cannot reset
that terminal record. The original issuer reports `recordUnavailable` for
the new attempt and retains the old restricted assignment. Missing history
still does not establish general permission to admit a worker.

A third worker with a different certificate then completes admission/catch-up
through the same issuer and join material. Its distinct assigned ID is the
only new member, and the excluded source remains unchanged. This rules out a
dead listener or blanket formation lock as the explanation for refusal.
All fault setup and assertions use independent processes and production
interfaces; no runtime, wire/profile or new fault-control change was needed.

This closes the certificate-excluded restart/fresh-attempt combination, not
post-adoption self-ejection or the entire combined restart/ejection matrix row.
The worker manual, runbook and administrator story preserve the distinction
between a negative bypass test and a safe operator procedure.

`make test-formation-source-loss` also **passed** after the shared restart
setup changed. Six evidence-helper tests, `make docs-check` (89 Markdown files)
and `git diff --check` passed. Workspace Rust tests, full churn and optional
observability acceptance were not rerun for this Python/Makefile/docs-only
increment. The existing fault-build `proc-macro-error2 v2.0.1`
future-incompatibility warning remains.

#### Post-adoption peer-wire self-ejection — verified 2026-09-06

`make test-formation-peer-ejection` **passed** after the HTTP harness regression
described below was fixed. The public journey first verifies completed
adoption/catch-up and current `joined` participation. A guarded development-
only Unix signal fixture on the original introducer then sends one valid
tombstone-bearing Ping datagram through the existing codec and authenticated
member connection to its sole live peer. It rejects absent/ambiguous routes
and deferred gossip, with no retry, extra queue or reliable-stream fallback.

The receiver must report `ejected`, preserve its formation/node IDs, withhold
join material and introduction, and keep its historical joined receipt. The
fixture sender's surviving view eventually marks it Dead, establishing that
the receiver no longer answers live membership probes. The receiver remains
ejected until the harness explicitly leaves using its current formation
precondition, yielding fresh standalone IDs and one live local member. Output
must not contain the old formation token. All observations use the public CLI.

The fixture sends a valid test-peer tombstone without mutating the sender's
own model. As allowed by the failure matrix, this proves receiving-worker
self-ejection, not a new public removal API or cluster-wide removal convergence.
The startup-only `--test-eject-peer-on-signal` switch registers a one-shot
`SIGUSR1` handler only in the explicit fault build, requires the existing
loopback/Unix/private-directory safeguards and conflicts with other startup
faults. The ordinary CLI rejects it; non-Unix invocation is unsupported.

The initial direct journey passed. A later Make run failed **before ejection**
with `BrokenPipeError` in a malformed-inspection check; private evidence is
retained at `/tmp/orishu-formation-failure-91svg76i`. The test client sent headers
and body separately, but the server could reject the headers and close its
read side before the body write. A real-socket, deterministic regression at
the same helper reproduced the failure before the fix. The helper now reads
the response after that specific write error, without retrying the request or
treating transport failure as acceptance. Exact expected status and response
size remain mandatory. Four `scripts/test_formation_http.py` tests pass for
the early response, wrong status, no response and oversized response. The
original Make journey was rerun and passed; no temporary debug logging remains.

Default and fault-enabled worker all-target suites each **passed 109 tests**;
scoped worker Clippy passed both feature configurations. Workspace formatting,
six evidence-helper tests, four HTTP-helper tests, `make docs-check` (89 Markdown
files) and `git diff --check` passed. The full lost-ACK/handoff/leave/crash/restart
journey also passed after adding the owner fixture, before the HTTP helper fix.
Workspace-wide Rust tests and observability acceptance were not rerun; the
existing `proc-macro-error2 v2.0.1` future-incompatibility warning remains.

#### Lost voluntary departure announcements — verified 2026-09-06

The full independent-process `--lost-departure` journey **passed**:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --lost-departure
```

The one-shot, development-only `--test-drop-next-departure` startup fault is
armed on C. On its voluntary leave, the owner encodes the normal departure
effects under the old formation/assigned identity but suppresses delivery at
the existing datagram-send boundary. The public harness requires the exact
old assigned-ID marker and a count of two suppressed announcements. The core
leave transition, receipt, fresh standalone identity, session closure and
generation fencing are unchanged. The flag is absent from normal builds,
conflicts with all other startup faults and uses the existing exposure guards.

Survivors must still report the departed ID Dead through normal SWIM within
the unchanged 60-second observation cutoff. C replays its original receipt,
then rejoins with the same certificate and a fresh assigned ID while old dead
history remains. No tombstone/block is cleared to make readmission succeed.
The full journey continues through ordinary crash, restart and readmission,
exact member/fingerprint/liveness assertions and bounded graceful cleanup.

This closes deliberate announcement loss, not loss of the public leave
response: interrupted operator-request/receipt recovery remains a separate
case. It does not claim a production convergence SLO or prove arbitrary churn.
The worker manual documents the fault boundary and the normal SWIM fallback.

Default and fault-enabled worker all-target suites each **passed 110 tests**,
including startup-switch availability and mutual exclusion. Feature-enabled
scoped Clippy, workspace formatting, six evidence-helper tests, four HTTP-helper
tests and `git diff --check` passed. Full-workspace Rust and observability
acceptance were not rerun in this increment. The existing fault-build
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

The second isolated run through `make test-formation-lost-departure` also
**passed**, as did default scoped worker Clippy with warnings denied,
`make docs-check` (89 Markdown files) and the final `git diff --check`.

#### Lost public leave response — verified 2026-09-06

The full ordinary-build `--lost-leave-response` process journey **passed**:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --lost-leave-response
```

A single-request Unix-socket proxy in the private harness directory forwards
the real CLI's authenticated leave request unchanged to C. It requires HTTP
200 from the worker, then closes both sockets without forwarding response
bytes. The CLI must report failure. Independent public inspection must show
C already standalone with fresh identities before the exact request is retried
directly. The returned receipt must identify that same completed transition;
current formation/node identity must remain unchanged by replay.

The rest of the normal journey remains enabled: stale preconditions and changed
payload reject, exact receipts replay, survivors detect departure, the same
certificate rejoins with a fresh ID, and replay after readmission cannot leave
again. Ordinary crash/restart/readmission and bounded cleanup also pass.

The proxy bounds headers to 8 KiB, request body to 4 KiB, one connection and
finite socket/deadline waits. Raw authenticated bytes remain in memory and
never enter failure artifacts. Two real-socket proxy tests verify unchanged
forwarding with zero response bytes delivered and refusal to count HTTP 401
as lost acceptance. Together with the four early-rejection regressions,
`python3 scripts/test_formation_http.py` **passed six tests**. This introduces
no worker hook, protocol change or new recovery authority. It proves recovery
while the original process retains the receipt, not durable restart recovery.

Six evidence-helper tests, `make docs-check` (89 Markdown files) and
`git diff --check` passed. The worker/CLI build reported the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning. Full-workspace Rust
and observability acceptance were not rerun for this harness/docs-only change.

The second isolated run through `make test-formation-lost-leave-response`
also **passed**, including its HTTP-helper suite and full process journey.

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
  tombstone state is synchronized. A state-changing leave announces departure,
  closes old sessions, generates fresh standalone identities, and reports both
  old and new formation state. Preserve the client protocol's distinct
  standalone-of-one no-op: retain identity and return its recorded unchanged
  receipt. A standalone introducer with other member records leaves normally;
  member count alone does not determine participation or leave eligibility.
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

- Map formation instruments to actual owner/adapter outcomes before wiring
  exporters. Distinguish a newly accepted admission, a rejected admission and
  replay of a retained assignment; an HTTP success or a delivered reply alone
  does not establish any of those outcomes. Document counter reset/lifetime,
  units, finite labels and unavailable instruments in the companion catalogue.
  Membership convergence remains an exact-state assertion, not a metric or
  probe inference.
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
- Limit this handoff to process and formation instruments and their tested
  deployment recipes. Runtime/storage/observation instruments and their
  workload-specific dashboards remain with P-OBSERVABILITY slice 4 and its
  owning milestones; no fabricated workload is needed to close M4.

## Required failure and convergence evidence

Use pure transition tests for state matrices and real process/wire tests for
adapter guarantees. In addition to the three-worker happy path, cover:

Choose and record the minimum sufficient boundary before implementing each
remaining case:

- Independent worker processes plus public operator requests are required for
  the introducer handoff, interrupted-admission operator procedure, process
  loss/restart, excluded restart/ejection, lost departure and partition/heal
  journeys. Test-only fault injection may arrange the fault; assertions must
  inspect production interfaces, not mutate the core to manufacture success.
- Real authenticated wire/runtime tests can establish codec rejection,
  connection collision/reconnect, partial catch-up, stale completion and
  resource-limit behavior. State which process-level row they support rather
  than presenting them as a substitute for that journey.
- Pure tests establish transition orderings, merge conflicts and exact budget
  boundaries. Pair them with wire/runtime evidence when the property depends
  on decoding, session binding, scheduling or effect delivery.

For partition tests, specify which links are interrupted, which remain usable
and when healing occurs. For overload tests, name the configured capacity,
expected refusal/drop behavior and a measurable bound on control/shutdown
progress. Avoid replacing these assertions with merely “did not crash.”

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
| Leave followed by same-certificate readmission | All surviving views converge on the new assigned identity while preserving the old departed record; exact leave-receipt replay after readmission does not leave again. Retain the controlled timing regression and rerun the full journey under the documented corrected budget; preserve historical failures without claiming a proven runtime cause. |
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

Use these closure gates without marking an entire companion programme complete
when only its formation portion ships:

| Gate | Required closure artifact |
| --- | --- |
| N-FORMATION | All formation criteria and required failure rows mapped to passing evidence at the stated boundary, final validation and tested formation/recovery manuals |
| P-OBSERVABILITY for M4 | Slices 1–3 accepted against the same worker contract, including disabled/enabled modes, probe transitions, security, bounded metrics and a received cross-peer trace |
| P-OBS-DOCS for M4 | Tested formation-stage scrape/probe/collector recipes, applicable dashboards/runbooks and updated operator stories/manuals; unsupported release/platform paths explicitly identified |
| Combined M4 | All three gates above pass together; no workload execution or slice-4 telemetry dependency |

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
- A state-changing voluntary leave returns that worker to a fresh standalone
  formation without a tombstone and the old formation converges on its
  departure as liveness state; its membership record need not disappear.
  Leaving an already standalone formation of one is a recorded no-op with
  unchanged identities. Exact receipt replay must not cause a second leave or
  identity change, including after subsequent readmission. Verify both cases
  through the supported authenticated client surface.
- Mailboxes, sessions, connections, frames, datagrams, timers, decoded
  collections, diagnostics, and retry/fan-out work have tested limits.
- Required multi-process journeys use real QUIC/mTLS and real serialized
  client requests; no journey's pass condition calls the membership core
  directly or depends on a mock transport. Pure transition tests and real-wire
  fixtures remain valid evidence for the distinct boundaries assigned in the
  failure matrix; they do not substitute for a required process journey.
- The failure/convergence matrix above passes at the appropriate production
  boundary, including interrupted joins, policy races and stale IO fencing.
- Every row in that matrix maps to a named test/harness case and its runnable
  command. Record platform prerequisites and any remaining manual checks;
  skipped cases are gaps, not passing evidence. Check in the operator journey
  and its expected assertions so another contributor can reproduce acceptance.
- Before closing the task, consolidate the historical integration record into
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
  Run the complete formation multi-process harness separately if it is not
  part of the default suite, including its explicit fault-build cases; prove
  test-only controls are absent from normal release builds. Preserve the
  membership dependency-purity test and record platform-only manual checks.
  For the combined M4 gate, additionally run the observability-feature
  build/test matrix and companion telemetry harness. Default workspace tests
  alone establish neither fault-build nor optional-exporter acceptance.
  Record the revision or dirty-worktree checkpoint, exact feature set and
  executable/target directory for each run. Run different feature builds
  sequentially when they share executable paths, or isolate their target
  directories; one build must not replace a binary used by another live test.

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
