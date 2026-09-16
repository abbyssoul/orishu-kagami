# ADR 0030: Retain durable worker load receipts before admission

Status: **accepted implementation refinement; shared facts, Unix journal and daemon coordinator implemented; opt-in load/retrieval serving delivered by ADR 0031**
Date: **2026-09-16**
Refines: [ADR 0028](0028-fence-worker-scientific-admission-through-formation-owner.md)
and [ADR 0029](0029-bind-run-descriptors-to-owner-allocated-epochs.md).

## Problem and options

Complete artifact delivery, successful JIT and even owner confirmation are not
proof of initial publication. A lost client connection or process crash can occur
between these stages or after publication but before receipt delivery. Retrying
as a new admission must not silently execute the same identified request twice.

- **Only return the live run handle.** Rejected: connection/process loss erases
  retry history; a handle is neither durable provenance nor a command receipt.
- **Persist acceptance only after publication.** Insufficient: a crash in that
  gap leaves no record that execution may have started.
- **Durable intent followed by a final historical outcome.** Selected: reserve
  intent before accepting a body, then record known acceptance/refusal. An
  interrupted pending attempt is indeterminate, not permission to execute again.
- **Atomically commit the journal and all simulation state.** Deferred: this
  requires durable scientific storage and coordinated commit, neither supplied
  by this journal. It must not be simulated by optimistic status reporting.

## Decision

The shared [load request/receipt facts](../run-load-receipts-v1.md) bind an existing
client-assigned `OperationId`, expected formation and exact workload root. Archive
length/representation and transport details are not logical request identity.
The worker-private key is `(formationId, operationId)`. Exact replay returns the
historical receipt; changing its workload root conflicts. The source node is
historical provenance, not a current routing hint. Accepted descriptors must match
the request formation/root and use the owner profile's nonzero epoch.

The IO shell has four states: Pending, Accepted, Refused and Indeterminate.
Only a durable Pending reservation yields a nonserializable completion ticket.
Only the daemon coordinator, after known initial publication, may supply Accepted.
Known non-publication permits Refused; uncertain publication requires Indeterminate.
Restart durably converts every Pending record to Indeterminate before opening
the store for use. Final states are immutable. Acceptance is a historical fact,
not evidence of a running executor, restored checkpoint or present formation.

Keep the journal outside the formation owner: synchronous filesystem IO must
not block membership or scientific commit. It is not another execution lock or
formation authority. The internal daemon coordinator retains tickets and live runs
independently of request connections, resolves historical retries before current
eligibility checks, and uses the existing owner fence for **new** work. Authenticated
public framing, routing and receipt retrieval are now provided by the opt-in
[ADR 0031 profile](0031-expose-bounded-identified-scientific-load-http.md).
`RunningWorker` installs and retains the coordinator once; enabled startup opens
its journal and sandbox before client serving. Distributed execution remains
unadvertised. A distinct daemon-owner drop
guard shuts down admission/runs even if client clones survive.

For this bounded Unix foundation, retain at most 256 receipts and a 512 KiB
snapshot per worker directory. Never evict to make an old operation executable.
Full history refuses new operations but permits replay/conflict checks. Garbage
collection/tombstone policy requires a later explicit retention contract. Identified
requests are not deduplicated across different worker directories; no cross-worker
retry or exactly-once distributed execution claim follows from this journal.

Use the worker's owned/private directory checks and fd-relative constant names,
private regular single-link files, and a separate nonblocking writer lock. Encode
one versioned CBOR snapshot with the existing bounded structural preflight and
an early count-bounded typed reader. Write a fresh staging file, sync the file,
rename over the committed snapshot, then sync the directory. Only then publish
the new in-memory history or return a completion ticket/receipt. Any write failure
poisons the instance until reopen; it may already have published a snapshot.
Recover only the committed filename; remove only a checked private staging file.
Corrupt, duplicate, incompatible or oversized history fails closed, never resets.

## Consequences and limits

This gives conservative **at-most-once admission permission per retained key**,
not exactly-once execution or a promise of recoverable results. A crash before
publication may leave an indeterminate attempt that actually did nothing. The
client must surface that uncertainty rather than inventing a new ID automatically.
An administrator deleting or rolling back the directory invalidates its history;
filesystem checks are not protection against a malicious same-UID administrator.

The one-snapshot design trades bounded O(receipts + encoded bytes) write/copy work
for a small failure surface; duplicate-key recovery checks are O(receipts log
receipts). This is admission-time work, not a per-tick hot path. Fault injection
tests cover failures around create/write/file-sync/rename/directory-sync, but do
not emulate every filesystem or prove hardware power-loss behavior. Durability
depends on the host filesystem honoring sync/atomic rename. The backend is Unix
only and is not automatically opened by worker bootstrap yet.

### Internal coordinator ownership

One non-queuing admission slot owns the detached task and its journal through
actual completion. A bounded read-only projection serves history without IO;
new requests recheck history under the local gate to avoid a completion race.
The owner execution lease is still required before polling a body or compiling.
Shutdown cancels operation control without abandoning native work or its capacity.
Client response loss never drops the completion ticket or retained run. Exact run
identity is required for lookup, and historical receipt lookup never selects a
newer epoch. No extra step scheduler, storage engine or observer protocol is added.

The coordinator records pre-start failures as known refusals; initial-start
failures conservatively become Indeterminate because the current start error
surface includes lost publication acknowledgement. A known published run is
retained before final receipt IO. If that IO fails, the run remains retrievable,
receipt history becomes unavailable and new admission fails closed. Unexpected
task failure similarly poisons history; restart recovery, not automatic retry,
resolves its durable pending record. HTTP and automatic startup wiring remain
separate work against this implementation, not another ownership design.
