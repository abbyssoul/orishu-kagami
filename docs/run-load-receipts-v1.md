# Identified run-load receipts v1

Status: **shared typed facts, worker journal, daemon-owned admission coordinator and opt-in HTTP load/lookup/current-run routes implemented; public run control remains open**.
Decision: [ADR 0030](adr/0030-retain-durable-worker-load-receipts.md).

## Shared facts

`orishu::model::run_load` defines strict serde records. IO adapters must bound and
preflight input before typed deserialization; generic serde is not a resource
budget or authentication boundary. No existing workload, WIT, formation-v1
summary or [run-descriptor](run-descriptor-v1.md) encoding changes.

`LoadRequest` is an exact map:

```json
{
  "apiVersion": "orishu.run-load-request/v1",
  "operationId": "load-42",
  "formationId": "formation-a",
  "workloadId": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
}
```

The example digest is illustrative, not an executable workload. `operationId`
uses the existing 1–64 ASCII letters/digits/underscore/hyphen contract. Formation
IDs and workload digests use their existing validating types. Unknown/duplicate
fields and unsupported versions are refused. An operation is scoped by formation
within the worker's journal; reusing it with another workload root conflicts.
Transport body length, portable archive bytes, filesystem paths, URLs, compression,
plugin inventory and mutable names are deliberately absent. Different packaging
of the same workload does not redefine logical intent.

`LoadReceipt` contains exactly `apiVersion: "orishu.run-load-receipt/v1"`, the
original `request`, historical `sourceNodeId`, and `state`. State encodings are:

| Meaning | `state` value |
| --- | --- |
| Reserved, not accepted | `{"phase":"pending"}` |
| Historically accepted | `{"phase":"finished","outcome":{"state":"accepted","descriptor":…}}` |
| Known no initial publication | `{"phase":"finished","outcome":{"state":"refused","reason":"…"}}` |
| Outcome uncertain, never reexecute implicitly | `{"phase":"finished","outcome":{"state":"indeterminate"}}` |

An accepted descriptor must match the request's formation/root and have a nonzero
workload epoch. Constructors **and deserialization** enforce this. `reason` is one
of `formationUnavailable`, `delivery`, `invalidWorkload`, `policy`, `cancelled`,
or `publication`. Receipts do not retain arbitrary guest, network or filesystem
diagnostic strings. These are facts, not content-addressed artifacts or live
status responses; CBOR field order does not define their identity.

## Worker journal

The internal Unix `workload_receipts::ReceiptStore` operates in an existing owned
0700-style worker directory and can coexist with its credential instance lock.
It keeps a separate `.workload-load-receipts.lock`, private
`workload-load-receipts.cbor` snapshot and transient
`.workload-load-receipts.pending` staging file. Names never derive from request
input. A snapshot is exactly `apiVersion: "orishu.worker-load-receipts/v1"` and
`receipts: […]`. At most 256 entries and 512 KiB are accepted. Existing worker
CBOR structural limits additionally apply: no floats/tags/indefinite arrays,
bounded depth/text/items/maps and duplicate-key rejection before owned decoding.
The typed entry reader rejects excess count without deserializing an extra entry.
Duplicate operation keys are invalid even when the receipt bodies are identical.

`begin` returns either a replay or a new completion ticket **after durable intent**.
Only that store incarnation's pending ticket may finish it. Final outcomes cannot
be replaced. Dropping a ticket leaves Pending; reopening converts it durably to
Indeterminate. A full journal still serves exact replay and conflict detection;
it does not evict historical decisions. Failed IO poisons all operations in that
instance until reopen; uncommitted staging bytes are never mistaken for history.

The caller must not interpret a receipt as a run handle. Tests combine actual
Unix-socket delivery and Newtonian/Euler Component execution with a receipt that
stays Pending through admission, becomes Accepted after initial publication and
replays unchanged after unload and reopening the journal.

## Daemon coordinator (implemented internally)

`workload_load::LoadCoordinator` composes the journal with `AdmissionService` and
retains the accepted `RunHandle`. `RunningWorker::install_load_coordinator` binds
and installs it once from an already opened private journal, sandbox and host
`LoadPolicy`. The daemon has a distinct retained owner: dropped client handles or
response futures cannot discard work, while daemon shutdown **or owner drop**
cancels admission and revokes all live run loans. Explicit scientific enablement
installs it before main's client listeners; default startup remains formation-only.
See the [HTTP profile](protocol-scientific-load-v1.md). Distributed execution
capability remains unadvertised.

Submission first checks the bounded last-durable-history projection, before
current formation or runtime availability. A known exact request replays without
polling the supplied body, including Pending and historical acceptance after
unload/shutdown. New work claims a non-queuing local admission slot, retains the
ticket independently of the response, persists intent off-owner, then obtains the
existing owner lease before body reads/JIT. The formation owner remains the only
topology/execution authority. The coordinator's gate bounds detached jobs and
journal writers; it cannot grant execution itself. Busy new requests have no new
receipt and can retry the same operation ID; durable refused attempts replay their
recorded refusal unchanged.

Small read-only receipt projections never wait behind filesystem IO or JIT. A
concurrent final write may still read Pending until its durable result is
published. A new request during the brief reservation write can see Busy before
its Pending projection appears. The submission response is emitted after final
IO, projection publication and releasing the local admission slot. Run retrieval
matches the exact formation/root/epoch and cannot redirect a historical receipt
to a replacement run. Releasing all client run loans does not unload the daemon's
run; explicit unload revokes it, after which a new identified execution can use a
fresh owner-allocated epoch.

Pre-publication admission errors become bounded durable refusals. `start`'s
combined failure surface includes lost initial-publication acknowledgement, so a
start failure conservatively records Indeterminate. Successful publication is
retained before writing Accepted. A final write failure leaves the known live
run retrievable but receipt history poisoned, never reports a false refusal, and
refuses new admission. `retained_descriptor` discovers that usable live execution
without requiring the client to guess an epoch after a lost receipt; it is a live
projection, not a repaired historical fact. Shutdown racing with successful publication revokes the
late run but still records historical acceptance. An unexpectedly aborted/panicked
job poisons the projection and cancels its active control; reopening the journal
recovers remaining Pending intent as Indeterminate. The coordinator does not
automatically reopen or retry uncertain work.

The detached job retains the store/capacity until its actual IO finishes; cancelling
its operation is not a claim that native JIT or filesystem sync was interrupted.
`wait_admission` lets a caller wait under its own deadline for admission/receipt
work to drain. It does not prove all retained-runtime disposal has finished.

## Public integration

The [opt-in HTTP profile](protocol-scientific-load-v1.md) implements bounded
authenticated framing, startup installation, historical lookup and current-run
discovery. Its early HTTP 202 is a Pending projection after complete body EOF;
the coordinator's final outcome still waits for durable IO. The original
integration requirements below remain the review checklist; public run control
and exhaustive disconnection fault injection remain follow-up work.

1. Authenticate and bound the small request framing before journal/body work.
   Retain routing to the same worker; no cross-worker retry guarantee exists.
2. Look up historical request identity before treating changed current formation
   or a full journal as a reason to reject an already recorded operation.
3. Install/use the daemon coordinator rather than recreating receipt/lease/run
   ownership in request handlers. Supply a body-bounded reader and host policy;
   no allocation-sized client claims may bypass preflight.
4. Map the existing typed receipt/Busy/conflict/unknown outcomes into an explicitly
   versioned bounded HTTP framing/status contract. Preserve exact request IDs.
5. Expose authenticated receipt and run retrieval/control using the retained
   coordinator. A persistence error after publication is uncertainty, not refusal.
6. Test dropped connections at each boundary, restart, overload, authorization,
   malformed actual framing and run retrieval/control before enabling execution.

Durable scientific state, reset/resume, distributed commit/receipt coordination,
receipt cleanup policy and kernel-cache administration are separate follow-ups.
