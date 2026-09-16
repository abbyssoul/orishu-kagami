# ADR 0028: Fence worker scientific admission through the formation owner

Status: **accepted implementation refinement; reservation, admission handoff and retained-run publication implemented; allocation refined by ADR 0029 and opt-in load serving by ADR 0031**
Date: **2026-09-16**
Refines: [ADR 0024](0024-orishu-orchestrates-a-workload-component-graph.md)
and [O-RUNTIME](../tasks/implement-single-node-workload-runtime.md).

## Context

The shared runtime independently verifies and executes a complete selected
workload. The worker's existing serialized owner governs formation changes, joins,
membership locking, leave, ejection and shutdown. Scientific validation/JIT must
run outside that owner, or hostile/slow compilation could block membership and
administration. Reading the owner's published standalone summary before launching
admission is not enough: a join can already be reserved but still doing handshake
IO before participation becomes `Joining`.

The product accepts one workload at a time, not a scheduler. Pending admission
also consumes that exclusive slot. Identity changes, shutdown and cancelled callers
must not leave a candidate able to publish into a different formation or leak the
slot forever under mailbox pressure.

## Options considered

- **Check published status before and after compilation.** Rejected as authority:
  reads are not an atomic reservation or publication transition. Multiple uploads
  and a join can all pass the same initial check; a final read can race again.
- **Compile/execute inside the membership owner.** Rejected: synchronous guest/JIT
  work would block probes, operator control and shutdown, violating the existing
  bounded control-plane design.
- **Create an independent worker workload lock.** Rejected as the topology fence:
  a mutex unknown to the formation owner cannot coordinate joins, leave or identity
  replacement. It can still be useful as an implementation detail of a scientific
  executor, but is not formation authority.
- **Owner-issued execution lease plus off-owner effects.** Selected. The existing
  owner serializes eligibility/reservation with topology intent. Expensive work
  holds a lease and returns a fenced result for later owner-side adoption.

## Decision

Use one app-local execution slot in the formation owner. The first single-node
profile requires a **locked standalone formation with exactly its local member**,
no pending outbound join and no reserved join operation (including handshake IO).
It refuses a multi-member/joined formation; it does not silently run a supposedly
distributed workload on whichever entry node received it. Membership lock remains
an explicit operator choice, not a hidden mutation by scientific compilation.
Future distributed coordination must replace this eligibility gate through its
own admitted profile, without weakening identity fencing.

The reservation carries formation and node identity, the existing process-local
generation, and a checked monotonically allocated sequence. The sequence is not
a workload digest, run identity or persisted counter. No mutable label, observed
readiness flag or client-supplied sequence can mint a lease.

Reserve before reading large inputs or starting expensive admission. A current
lease excludes another admission, new outbound joins, membership unlock and
explicit leave. Existing identified join receipt replay/conflict checks remain
available. Shutdown remains independent and revokes before acknowledging stopping.
Unexpected trusted internal/core topology changes, formation replacement,
ejection, owner failure or owner drop also revoke the read-only fence.

Revocation is **not release**: the slot stays occupied until the lease is dropped,
so still-running off-owner work cannot be forgotten and overlapped with another
admission. Dropping the lease immediately invalidates its cloned fences and sends
a generation/sequence-bound release through pre-reserved control capacity. A late
release cannot clear a newer reservation. Dropping a response receiver cannot
strand the slot. Owner shutdown disposes late completion payloads through the
existing reserved-control disposal path.

The reservation uses two control slots initially (request plus completion), then
retains one slot for reliable release. There is at most one admitted lease; no
queue of waiting workloads, artifact payload or Wasm engine enters membership
state. Cloned fences are read-only cancellation hints, not ownership.

**A live fence is not scientific acceptance or permission to commit.** Subsequent
integration must independently validate the closure, bound off-owner execution,
use owner-allocated run/epoch identity and return the candidate to a serialized publication
authority which rechecks the lease. A pre-publication `is_current()` read followed
by an unrelated write is still a race. Cooperative cancellation/guest interruption
connects the fence to operation control as described below; a boolean alone does
not interrupt JIT or a guest. Resource reservations and actual release also remain
necessary, even after revocation.

### Off-owner admission handoff

`driver::scientific::AdmissionService` reserves the lease and a second completion
permit before accepting input. [ADR 0029](0029-bind-run-descriptors-to-owner-allocated-epochs.md)
adds a reserved allocation permit: four slots initially, three after reservation,
and two after allocation (reliable release and owner confirmation). It runs portable
closure verification and the shared `orishu_runtime::admit` on a blocking executor,
never in the formation owner. Only the small fence/control/reply travels back to
that owner, which rechecks eligibility and exact generation/sequence before
confirming once. The admitted runtime and its potentially expensive destruction
remain outside the membership lane. Scope now comes from the formation owner's
immutable run descriptor after closure verification, not from the submitter or
a lease sequence. Confirmation additionally checks the allocated scope.

The lease now carries monotonic cancellation linked as the operation's parent.
Revocation cancels guest execution through the existing epoch checks; cancelling
one operation does not cancel that parent. A dropped admission future cancels its
child operation, while the blocking job retains the lease until it actually exits.
Native JIT already in progress cannot be interrupted. Success, rejection, timeout
and future cancellation dispose the admitted runtime before releasing its slot.

The result is a **validated admission handoff**, not a published loaded run. The
receiving execution adapter must retain the lease and serialize initial/step
publication with revocation. Neither owner confirmation nor a subsequent live-fence
read grants permission for an unrelated scientific publication. Buffer byte/count
limits and a frozen deny set apply independently of installed plugins; whole-process
RSS and interruptible JIT remain separate hardening work. The input receiver
described below now uses this reservation before reading its body.

### Bounded complete-body delivery

The prepared admission owns input IO as well as subsequent validation. Its
`receive` adapter accepts only a positive exact body length within host/archive
policy, a finite absolute delivery budget and an expected workload root. It
checks policy before allocation/read, bounds read quanta, observes cancellation
even at stalled EOF, and moves one owned buffer into off-owner verification.
Wrong-root input is rejected after complete closure verification but before
epoch allocation or JIT. Existing in-memory internal validation remains available;
public submit adapters must carry the caller's expected identity explicitly.

The IO reader must end at the body boundary; this does not authorize reading an
arbitrary path or reusable socket to EOF. No authentication or wire endpoint is
introduced. Authenticate/authorize and reserve identified-command receipt capacity
before calling it from a public adapter. Complete-body buffering is the current
portable format implementation, not a resumable/thin transfer or an RSS guarantee.

## Consequences and current evidence

### Long-lived execution integration refinement

The worker execution adapter retains the admitted runtime and lease together on
an off-owner executor. FixedRun gains a trusted host commit gate: it computes and
validates a whole candidate first, calls the gate exactly once, and replaces its
private state only after acceptance, with no subsequent fallible/cancellation
check. Existing local `advance` uses an always-accepting gate. This is not a guest
hook or observer callback; it is part of the runtime's publication authority.

For the worker the gate sends only bounded run identity/boundary/time metadata to
the formation owner through capacity reserved before computation. That owner
serializes acceptance with topology/shutdown, checks the original lease, and
publishes an internal run projection. Scientific buffers stay with the executor;
its observer requests are serviced only after gate acceptance and local commit.
The public formation-v1 wire schema is unchanged until the workload API is wired.

Rejected scientific computation preserves the prior state and permits an explicit
retry. Lost publication coordination instead terminates that execution lifetime:
the caller may not assume that transport failure proves non-commit, or retry against
a potentially divergent executor. The lease is revoked and retained until actual
runtime disposal. A disconnected initiating caller does not silently cancel an
already accepted operation. This internal adapter is not the durable identified
command/receipt protocol required by public workload APIs.

One bounded operation is accepted at a time, with no workload or step backlog.
Stop is terminal but retains the latest state for observation; unload revokes the
lifetime and disposes it off-owner. Idle executors observe lifetime revocation
within a bounded poll interval; active guest calls use linked cancellation.
Distributed/reset epoch allocation and durable checkpoint/recovery remain separate work.

`apps/orishu-worker/src/driver/execution.rs` implements the lease, its read-only
fence and owner request. Existing formation adapters reject conflicting intents
only while a lease exists; normal formation behavior is unchanged otherwise.
Tests use the real serialized driver: exclusive/stale reservations, locked and
multi-member gates, pending-handshake races, late release, cancelled receivers,
full mailbox release, topology replacement and shutdown/owner-abort revocation.
Five additional real-Component admission tests cover a portable Newtonian/Euler
closure, off-owner responsiveness on a single-thread executor, malformed/oversized/
denied/wrong-scope input, abandoned jobs retaining capacity until actual disposal,
shutdown and late confirmation through a full mailbox. No plugin inventory or
field initialization is consulted during admission.

`ValidatedAdmission::start` now owns that admitted runtime until unload and uses
the publication gate for boundary zero, each explicit step and terminal stop.
Three additional real-Component tests exercise repeated steps, isolated bounded
snapshots before/after advancement and unload, refusal without a backlog, full-lane
publication after caller loss, and complete late candidates after shutdown/owner
abort. A shared runtime test checks gate refusal preserves every prior buffer and
accepted publication is not rolled back by subsequent cancellation.

This internal refinement introduced no HTTP route, wire field, persisted format
or advertised execution capability. Its internal `RunView` reports the
owner-accepted scope/boundary/time/stop state while the execution lease is valid.
Durable command receipts/daemon ownership were subsequently implemented by
[ADR 0030](0030-retain-durable-worker-load-receipts.md), and opt-in public artifact
delivery, retrieval and summary-v2 occupancy by
[ADR 0031](0031-expose-bounded-identified-scientific-load-http.md).
Distributed/reset allocation, continuous/public run control and checkpoint/
observation wire adapters remain O-RUNTIME/O-CLIENT work. Internal retained
execution is not public API parity.
