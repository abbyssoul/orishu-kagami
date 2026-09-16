# ADR 0031: Expose bounded identified scientific load over opt-in HTTP

Status: **accepted implementation refinement; experimental load/receipt/current-run routes implemented; public stepping and observations remain open**
Date: **2026-09-16**
Refines: [ADR 0030](0030-retain-durable-worker-load-receipts.md) and
[ADR 0010](0010-content-addressed-workload-closure-and-portable-bundles.md).

## Context and options

The daemon owns identified admission, durable receipt history and retained runs,
but its client server previously exposed only formation operations. A submitter
must deliver exact intent and a complete workload without installing plugins on
workers, buffering unauthenticated payloads or treating response loss as rollback.

- **Reuse the imported `/cluster/workload` authored-resource payload.** Rejected:
  it is not the accepted immutable selected closure and cannot supply all kernels
  and captured inputs. Do not silently change that older resource's meaning.
- **Whole-plugin installation or arbitrary URL fetching at admission.** Rejected:
  distribution/authoring inventory cannot become scientific identity or a new
  worker execution authority.
- **Multipart streaming or mutable upload sessions first.** Deferred: neither is
  necessary for the complete portable-bundle profile; both add parser/lifetime/
  retention rules before resumable transfer and artifact-cache administration.
- **A small identified CBOR prefix followed by the portable bundle.** Chosen:
  authenticate/bound before parsing, then transfer body ownership to the existing
  coordinator. Location and representation remain outside workload identity.

## Decision

The [scientific-load HTTP profile](../protocol-scientific-load-v1.md) defines a
versioned media type and exact length framing, authenticated receipt lookup and
usable retained-run discovery. Every scientific route, including local reads,
requires the worker-local operator token. Existing TLS requirements apply to TCP.
Metadata has a fixed preflight/deadline; the body uses existing lease-bound IO and
independent runtime admission. The HTTP adapter holds one frame at a time and
rejects trailers instead of turning them into body EOF. A shared eight-handler
capacity spans scientific routes/listeners independently of membership mutations;
the coordinator still permits only one admission/run, not a queue.

After complete body EOF, return a Pending receipt with HTTP 202 if admission has
not finished. This acknowledges retained intent, not scientific acceptance.
Compilation and publication remain daemon-owned; dropping the response cannot
dispose a run or create a retry. A final returned receipt uses HTTP 200 whether
Accepted, Refused or Indeterminate: the typed outcome, never transport status,
determines acceptance. The client polls the original identified request. It must
not automatically create a new operation after a lost/unknown response.

Serving is explicitly enabled by `--scientific.enabled true`, its environment
equivalent or configuration. Default formation-only startup is unchanged and does
not create the receipt journal. Enabling opens the private journal/sandbox before
client listeners and retains the coordinator in `RunningWorker`. Unix is required
for the implemented durable backend. The accepted execution profile remains a
locked standalone one-member formation; no hidden lock/unlock, distributed work
advertisement, plugin installation or automatic stepping is introduced.

### Truthful formation occupancy

The old synthetic summary schema v1 could only say `workload: "none"`. Reporting
that after scientific publication is incorrect. Version 2 adds `"scientific"`,
meaning the same formation owner's view contains committed scientific state; it
is not a running/usable-executor guarantee. `RunningWorker::summary` projects both
fields from one owner view. Empty formation summaries remain v1; after publication
the response uses v2, and after release/restart it can return to v1. The shared
reader accepts v1/None and v2 states, but refuses v1/Scientific and unknown versions.

No peer ALPN, lock/join/leave schema, workload identity or run-descriptor format
changes. Older clients continue to work with ordinary formation-only workers;
clients that only understand summary v1 must be upgraded to inspect an occupied
scientific worker. Failing explicitly on v2 is preferable to a false empty state.
Peer distributed-execution capabilities remain unadvertised until their own
coordination/storage gates are delivered.

## Consequences and remaining work

This is an experimental whole-upload admission surface, not completion of
O-RUNTIME, O-CLIENT or X-PLUGIN. Public stepping/stop/unload and observation,
Kagami submission adapters, durable scientific state, distributed
execution, thin/resumable transfer and artifact administration remain. Existing
native JIT interruption/aggregate RSS and reusable-storage hardening limitations
remain explicit in the runtime tasks; opt-in serving does not erase those gates.
Whole workloads still have to fit the implemented single-node profile.

The shared `HttpClusterClient::scientific()` adapter now implements bounded
submission/lookup/discovery with exact request correlation, status/schema checks
and no implicit retry. Its response profile and limits are documented alongside
the wire protocol; real-worker tests upload and recover receipt history through
this path. This does not complete the Kagami product adapter or public controls.
