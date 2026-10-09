# Kagami run intents v1

Status: **implemented on Unix** (`apps/kagami/src/run/intents.rs` and
`apps/kagami/src/run/journal.rs`). Decision: [ADR 0033](adr/0033-persist-kagami-client-run-intent.md).

This document describes the local journal that Kagami uses to recover its
submissions and run commands after a restart. The journal is client state. It
is not part of an experiment, a workload or a worker protocol.

## Location

Kagami opens the journal at startup when `--operator-token-file` is set. The
directory is the first available of:

1. `KAGAMI_RUN_STATE_DIR`;
2. `$XDG_STATE_HOME/kagami/runs`;
3. `$HOME/.local/state/kagami/runs`.

Kagami creates the directory with mode `0700`. The directory contains:

| Name | Content |
|---|---|
| `intents.lock` | Exclusive advisory lock. One Kagami process holds it. |
| `intents.json` | The ledger, described below. |
| `bundles/sha256-<hex>.okw` | Frozen bytes of the unresolved submission. |

## Write rules

- Kagami writes a staging file, flushes it, renames it and flushes the
  directory. A reader never sees a partial file.
- Kagami writes the frozen bytes first and the ledger second. It sends a
  request only after both writes are durable.
- A failure before the rename keeps the previous ledger. Kagami can try again.
- A failure after the rename makes the state on disk unknown. The journal then
  refuses all writes until Kagami opens it again.
- When Kagami opens the journal, it removes staging files and bundle files that
  no unresolved submission needs.
- Kagami never overwrites a damaged ledger or a ledger from another version.

## Ledger format

`intents.json` is JSON with this structure. The maximum size is 64 KiB.

```json
{
  "apiVersion": "kagami.run-intents/v1",
  "load": {
    "target": {
      "address": "unix:/run/user/1000/orishu/worker.sock",
      "formationId": "formation-a"
    },
    "request": { "apiVersion": "orishu.run-load-request/v1", "...": "..." },
    "source": {
      "incarnation": "8c0f6e9a-3f25-4b8e-9d0e-6a1f2b3c4d5e",
      "revision": 7
    },
    "receipt": null
  },
  "command": null
}
```

- `load` and `command` are `null` or one record each.
- `target.address` is the worker address in a form that parses back to the same
  address. A Unix socket always has the `unix:` prefix.
- `request` is the exact shared [load request](run-load-receipts-v1.md) or
  [run-command request](run-command-receipts-v1.md).
- `receipt` is `null` or the last receipt that Kagami recorded for the request.
- `source` identifies the document incarnation and revision of a submission.
  Kagami uses it only to show lineage.
- A `command` record has `target`, `request` and `receipt`. Kagami removes it
  when its outcome is final (applied or refused).
- A `load` record stays until it is final and the user clears it.

The reader refuses:

- unknown fields and duplicate fields;
- a different `apiVersion` (reported as an unsupported version, not damage);
- an address or incarnation in a non-canonical form;
- a receipt for a different request;
- a target formation that is not the request formation;
- a command record with a final receipt.

## Behavior in the window

- After a restart, the Remote run panel shows the recorded operations. Kagami
  sends nothing automatically.
- **Reconcile** looks up the recorded operation. **Resubmit** sends the same
  operation with the same identity. For a submission, Kagami reads the stored
  bytes and verifies them against the workload root first.
- A recorded operation for a different worker address is shown, but Kagami does
  not reconcile it. The reason names the `--host` value to use.
- A disabled action shows its reason in a tooltip. Typical reasons are a full or
  read-only storage, a damaged record, another Kagami instance, or a write that
  did not complete.

## Limits

- One submission and one run command at a time.
- The ledger holds no credential and no credential path.
- Only Unix is implemented. Other platforms disable submissions and run
  commands.
- The journal does not protect against a rollback of the storage by an
  administrator.
