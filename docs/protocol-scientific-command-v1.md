# Experimental scientific-command HTTP v1

Status: **opt-in worker startup, command/receipt/status routes, shared Rust client and Kagami headless controls implemented; window controls and observations remain work**.
Decision: [ADR 0032](adr/0032-retain-identified-run-command-outcomes.md).
Facts and persistence: [identified command receipts](run-command-receipts-v1.md).

## Authority and enablement

The existing `--scientific.enabled true` option now opens both private load and
command journals and installs both daemon coordinators before listening. Default
startup stays formation-only and creates neither journal. Incompatible/corrupt
history fails enabled startup; it is not deleted or recreated. The Unix backend,
explicit locked standalone execution profile, worker-local operator credential
and TCP TLS requirements are unchanged from [scientific load](protocol-scientific-load-v1.md).
Workers never install authoring plugins to run these commands.

All routes authenticate before reading a body, including status and lookup. They
share the existing eight-handler capacity across scientific listeners. One
non-queuing command owns its journal ticket through execution/final IO; neither
an HTTP future nor a client connection owns the operation or retained run.

## Routes and framing

| Method/path | Exact CBOR body | Successful data |
| --- | --- | --- |
| `POST /api/v1/run-commands` | `RunCommandRequest` | `RunCommandReceipt` |
| `POST /api/v1/run-commands/lookup` | Same original `RunCommandRequest` | Historical `RunCommandReceipt` |
| `POST /api/v1/run/status` | `RunStatusRequest` | Exact live `RunStatus` |

Command requests use the [v1 schema](run-command-receipts-v1.md#request-and-status):
operation ID, complete run identity, expected boundary and `step` or `finish`.
No timestep, kernel choice, timeout, automatic current-run selection or boundary
refresh is accepted. Finish is terminal integration stop, **not pause or unload**.

A status query is read-only despite using POST, so its complete versioned identity
can use the same bounded body decoder instead of query-string parsing:

```json
{
  "apiVersion": "orishu.run-status-request/v1",
  "run": {
    "formationId": "formation-a",
    "workloadId": "sha256:0101010101010101010101010101010101010101010101010101010101010101",
    "workloadEpoch": 1
  }
}
```

Status never substitutes a newer run. Epoch zero/nonexistent identities are
unavailable, not newly allocated by a query. Unknown versions/fields and duplicate
fields are refused. All requests require exactly `application/cbor` and one
canonical decimal Content-Length, at most 4096 bytes. Empty/truncated/trailing or
structurally invalid CBOR is rejected. Complete body EOF is required before command
dispatch. No query, If-Match, compression, Transfer-Encoding or trailer is accepted.
Metadata has a five-second absolute deadline. The HTTP frame adapter and existing
server connection/head/stream limits are shared with scientific load.

## Outcomes and recovery

Replies use the CBOR `ApiResponse` envelope, named `ResponseData` variant and
`Cache-Control: no-store`. Command submission waits up to 65 seconds for a receipt;
daemon execution has a 60-second host control budget. Neither budget is simulation
time or a cancellation guarantee for in-progress filesystem/native work.

HTTP 200 returns a final command receipt regardless of Applied, Refused or
Indeterminate. An exact duplicate submission can return an existing Pending
receipt with HTTP 202; this is durable intent, **not application**. Lookup always
uses 200 for an existing receipt, including Pending. Status uses 200 for exact
committed metadata. A 504/OutcomeUnknown means the response wait expired; the
daemon may still complete execution or final IO. Reconcile the same operation at
the same worker, never invent a new ID or infer attribution from changed status.

History precedes live eligibility: after finish, run loss or worker restart, an
old Applied receipt still replays as historical Applied. Restart does not restore
scientific state, so its live status may be unavailable. Journal IO failure cannot
roll back a committed step; live status remains independently readable. Exact
boundary guards remain inside the executor rather than in a racy HTTP precheck.

The existing bounded framing/authentication errors apply, with these meanings:

| Status/code | Meaning |
| --- | --- |
| 404 / `OperationNotFound` | No historical receipt for this exact request |
| 404 / `RunUnavailable` | Status cannot read this exact live execution |
| 409 / `OperationConflict` | Operation ID reused with different intent |
| 409 / `CommandBusy` | No new ticket dispatched; command lane is occupied |
| 503 / `OperationHistoryFull` | Fixed command history exhausted; lookup/replay still work |
| 503 / `ScientificUnavailable`, `OutcomeUnknown` | Unavailable coordinator or uncertain durable outcome |
| 504 / `OutcomeUnknown` | Response deadline, not rollback |

A recorded `Refused` outcome differs from a pre-dispatch HTTP failure. In
particular, a new identified command for a missing run receives a durable
Refused/RunUnavailable receipt, while the read-only status route returns 404.
The current 256-record no-eviction journal is a manual-control foundation;
sustained retention/scalable persistence are still explicit gates.

## Shared client and compatibility

```rust,ignore
let receipt = client.scientific().command(&intent).await?;
let historical = client.scientific().command_lookup(&intent).await?;
let status = client.scientific().status(&status_request).await?;
```

The configured transport/TLS/operator token is reused. There are no automatic
redirects, retries, polling, locking or ID/boundary substitutions. Full receipt
intent and live-status run identity are checked against the request. Only the exact
404 codes above map to `None` in their corresponding read method; missing APIs,
conflicts and journal uncertainty remain errors.

The existing client 16-KiB streamed body cap, five-second body deadline, 70-second
submission/ten-second read budgets and bounded structural preflight apply. Only
command/status exchanges opt into finite half/single/double CBOR floats for SI
simulation time. NaN/Inf and arbitrary collections remain rejected. Shared typed
validation also rejects negative time and impossible application attribution.
Load/discovery and ordinary peer CBOR retain their float prohibition. Worker
encoding permits finite floats only for typed command/status response variants;
no peer, workload, kernel ABI, load-journal or command-journal format changes.

Kagami's [headless manual controls](../apps/kagami/README.md#headless-manual-run-controls)
now consume these methods with explicit formation/root/epoch, operation, boundary
and receipt action. Their separate `kagami.run-command/v1` JSON reports preserve
historical receipts versus exact live status, domain outcome exit codes and
uncertainty after possible delivery. No plugin inventory or editable document is
opened by the controls, and no client retry or membership mutation is added.

## Evidence and remaining work

Tests cover opt-in/auth-before-body, bounded malformed framing, shared-client
request/response correlation and real retained Newtonian/Euler execution through
HTTP step, replay, stale/conflicting requests, terminal finish and process restart.
Internal coordinator tests separately cover publication/reply loss, final-write
failure and shutdown races. New command routes reuse the existing TLS/listener
stack; command-specific TLS and HTTP/2 journeys are not separate claims yet.

Kagami window controls, continuous execution, pause/resume/unload, observations,
durable scientific restart, distributed execution and sustained history retention
remain open. This profile enables manual stepping; it does not close X-PLUGIN.
