# ADR 0033: Persist Kagami client run intent outside experiment and workload identity

Status: **accepted; Unix journal and window integration implemented**
Date: **2026-10-09**
Related: [ADR 0004](0004-separate-authoring-commands-from-run-observations.md),
[ADR 0030](0030-retain-durable-worker-load-receipts.md) and
[ADR 0032](0032-retain-identified-run-command-outcomes.md).

## Context

A worker records each load and run command durably (ADRs 0030 and 0032). A
client can lose the reply. Kagami kept its pending submission, the frozen
workload bytes and its pending command only in memory. When Kagami closed or
stopped, the user lost the operation ID. Then the user could not find out if
the worker applied the operation. A new operation with a new ID can load a
second run or advance a run two times.

Client state changes independently of worker state. A client can run on a
different host, version or storage. For this reason this record is separate
from ADR 0032.

## Options

- **Keep intent in memory and warn the user.** Rejected. The previous design did
  this. It depends on the user to copy identifiers before a stop.
- **Store intent in the experiment document.** Rejected. Run state must not
  change editable intent or its history (ADR 0004). A run operation is not part
  of a workload identity.
- **Infer the outcome from the current run status after a restart.** Rejected.
  Status is not attribution. Another client can change the same run.
- **Write-ahead client journal outside the document.** Selected.

## Decision

Kagami records each submission and run command in a local journal before it
sends anything. The [format](../kagami-run-intents-v1.md) is
`kagami.run-intents/v1`.

- The journal holds at most one submission and one run command. Each record has
  the exact request, the worker address and formation that received it, and the
  last known receipt. A submission also has its source document incarnation and
  revision, for lineage display only.
- The journal keeps the frozen bytes of an unresolved submission. Kagami
  verifies the bytes against the workload root before it sends them again.
- The journal holds no credential and no credential path. Each request reads
  the credential file when it is sent.
- Kagami adopts a new journal state only after the state is durable. If the
  write fails, Kagami sends nothing.
- After a restart, Kagami shows the recorded operations. It sends nothing
  automatically. The user reconciles or resends the same operation explicitly.
  Kagami does not create a new operation ID for recovery.
- Kagami reconciles a recorded operation only through the same worker address.
  An operation ID is scoped to one worker and formation.
- One Kagami process holds the journal at a time. Other processes can observe
  runs, but they cannot submit workloads or send run commands.
- Without a usable journal, Kagami can observe runs. Submissions and run
  commands are disabled, and each disabled action shows the reason.

## Consequences

- Kagami needs writable local storage to change a remote run. A full disk, a
  read-only file system, a damaged record or a second instance disables these
  actions. Each case has a specific message.
- A damaged or newer journal stays unchanged on disk. The user must move it
  away manually after reconciliation.
- A reply that Kagami cannot record leaves the operation unresolved. A later
  lookup gets the same receipt from the worker.
- The journal is Unix-only. Other platforms disable submissions and run
  commands.
- A future mode for read-only media can add an explicit volatile journal with a
  warning that a restart loses recovery. `Journal::open` marks this point.
- Live multi-writer clients, shared undo and remote recovery stay out of scope.
