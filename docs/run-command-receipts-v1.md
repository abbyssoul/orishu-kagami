# Identified fixed-profile run commands v1

Status: **shared typed facts, bounded Unix journal, daemon command coordinator, opt-in HTTP routes, shared client and Kagami headless controls implemented; window controls remain work**.
Decision: [ADR 0032](adr/0032-retain-identified-run-command-outcomes.md).
Serving: [scientific-command HTTP v1](protocol-scientific-command-v1.md).

## Request and status

`orishu::model::run_command` defines one exact immutable request:

```json
{
  "apiVersion": "orishu.run-command-request/v1",
  "operationId": "step-001",
  "run": {
    "formationId": "formation-a",
    "workloadId": "sha256:0101010101010101010101010101010101010101010101010101010101010101",
    "workloadEpoch": 1
  },
  "expectedBoundary": 0,
  "command": "step"
}
```

The digest is illustrative. The run tuple must come from its actual immutable
descriptor, not labels or the last "current run" response silently substituted
by a retry. Epoch zero is rejected. `step` requires a non-overflowing next uint64
boundary. `finish` is a terminal stop at the same boundary; it is **not** pause,
unload or step-budget exhaustion. Those actions are not valid v1 command strings.
There is no caller-supplied timestep, kernel choice, simulation time or timeout.

IO adapters must enforce `MAX_RUN_COMMAND_BYTES` (4096) and structural
preflight before serde. All shapes reject unknown/duplicate fields and unknown
versions; typed construction and serde enforce the same semantic constraints.
The request does not make itself authorized or executable merely by decoding.

`RunStatus` is a small `orishu.run-status/v1` committed metadata record:

```json
{
  "apiVersion": "orishu.run-status/v1",
  "descriptor": {
    "apiVersion": "orishu.run-descriptor/v1",
    "run": {
      "formationId": "formation-a",
      "workloadId": "sha256:0101010101010101010101010101010101010101010101010101010101010101",
      "workloadEpoch": 1
    }
  },
  "boundary": 1,
  "timeSeconds": 0.5,
  "phase": "ready"
}
```

`timeSeconds` is finite, nonnegative simulation time in SI seconds. CBOR admits
finite float encodings, not NaN/infinity; JSON uses a number. Boundary is a uint64
committed-step index, not an attempt counter. `ready` means manual stepping is
possible, not that a background scheduler is running. `finished` is terminal
integration stop with retained state. Status is neither an object/field snapshot
nor a durable checkpoint. In a receipt it is historical; a live status query must
obtain it from the exact owner-accepted run projection.

## Receipts and reconciliation

A `RunCommandReceipt` has exact fields `apiVersion:
"orishu.run-command-receipt/v1"`, `request`, `sourceNodeId` and `state`.
`sourceNodeId` is historical provenance, not the worker to select on a fresh retry.

| State | Meaning |
| --- | --- |
| `{"phase":"pending"}` | Durable intent only; not applied |
| `{"phase":"finished","outcome":{"state":"applied","status":...}}` | Known owner acknowledgement of this command |
| `{"phase":"finished","outcome":{"state":"refused","reason":{"code":...}}}` | Known non-application |
| `{"phase":"finished","outcome":{"state":"indeterminate"}}` | Application may have happened; never auto-execute again |

Refusal codes are `busy`, `runUnavailable`, `staleBoundary` (with `actual`),
`cancelled`, `scientific` and `capacity`. A stale refusal cannot claim that actual
equals expected. Reasons have no arbitrary guest/error/path payload. Ambiguous
publication must not be flattened into any of these known refusals.

Applied status must match the request's complete run identity and action:
step yields expected + 1 / Ready; finish yields expected / Finished. Shared
constructors and deserialization reject mismatches. They can validate attribution,
not prove that an untrusted remote worker really executed the computation.

The worker's command key is `(formationId, operationId)` within its command
journal, separate from load operations. Exact replay returns the original record;
any changed request field conflicts even when history is full. Check history
before current-run eligibility. A disappeared run or restarted worker must not
turn a historical applied command into a refusal or fresh execution permission.
Do not silently change operation ID, expected boundary, epoch or worker on retry.
Boundary guards alone do not prove which operation advanced a run.

## Private persistence

`CommandReceiptStore` shares the sealed `ReceiptJournal` implementation with the
existing `ReceiptStore` load alias. Its independent files are
`run-command-receipts.cbor`, `.run-command-receipts.pending` and
`.run-command-receipts.lock`, under the owned/private worker directory. Snapshot
version is `orishu.worker-run-command-receipts/v1`, with one `receipts` array.
Load schema/files remain byte-compatible and are never migrated or combined.

Each profile allows 256 records and 512 KiB. Byte/count/nesting/duplicate/work
checks precede typed allocation. Writer locks and completion capabilities are
store/incarnation specific. Persist Pending before execution; final states are
immutable. Restart durably converts Pending to Indeterminate. No eviction or
implicit garbage collection is supported. Directory/file safety and atomic-write
failure rules are those of [load receipts](run-load-receipts-v1.md).

The current whole-snapshot cost is O(history + encoded bytes) per command write,
not a recommended per-tick continuous scheduler. Bounded retention and scalable
persistence need further work before sustained control. The journal neither
retains a live executor nor provides scientific restart/rollback.

## Internal daemon coordinator

`RunningWorker::install_run_command_coordinator` explicitly binds an already
opened command journal to the worker's installed load/run authority. It does not
open files, enable routes or change formation membership. The daemon retains a
distinct owner; client handles and response futures do not own execution lifetime.
One detached command occupies a non-queuing slot through final receipt IO.
History lookup/replay precedes current-run eligibility; new work persists Pending
before calling `step_at` or `stop_at` for the exact run and expected boundary.

Only an acknowledged, identity-checked committed projection produces Applied.
Busy, stale boundary, pre-computation capacity failures and scientific refusals
remain known non-application. Executor response loss or publication uncertainty
becomes Indeterminate, never inferred success or refusal from a later status.
Final journal failure poisons command history until reopen; it cannot undo a
committed step or discard the retained run. Reopen conservatively reconciles
unfinished intent without executing it again.

Shutdown/drop revokes new commands and cancels uncommitted work even if client
handles survive. A known committed result still completes its final receipt IO.
`wait_idle` covers this command lane, not global native runtime disposal. Closing
only the command lane is not an implicit finish/unload. Exact live `status`
remains independent of journal health; historical lookup remains available after
finish, unload or replacement and must not be confused with live availability.

Real-kernel tests cover response loss, same-operation replay/conflict, competing
boundary advancement, terminal finish, replacement, final-write failure,
interrupted detached work, reopen and shutdown around publication. Opt-in startup
now opens/installs the journal and coordinator before authenticated bounded HTTP
serving. The shared client and Kagami headless controls preserve identity and
uncertainty; Kagami window controls remain a separate implementation gate.
