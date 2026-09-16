# kagami-session

Kagami's document server: the sole mechanism by which an experiment is created
or modified.

A UI gesture, an MCP tool call, an undo, a redo, and (task K5) a catalog
instantiation all become the same `ExperimentCommandEnvelope` before anything
is decided. No adapter receives mutable access to the model, and successful
transport never implies acceptance — that is what makes
[ADR 0006](../../docs/adr/0006-mcp-ui-equivalence.md)'s parity guarantee a
property of this type rather than a promise each adapter has to keep.

```text
apps/kagami        UI, MCP adapter, dialogs      <- imperative shell
     |  ExperimentCommandEnvelope   ^ SessionView / ExperimentEvent
kagami-session     this crate                    <- the document server
     |  update(model, commands)     ^ Rejection
kagami-document    model, transition, history    <- pure, sans-IO
```

## The envelope contract

The document model's wire version is now 3 (scientific setup descriptors); the
session envelope remains version 1. A scientific capture is an in-process edit
with normal revision, history and refusal semantics, but `WireEnvelope::of`
returns `None` for it: bounded blob effects are not ordinary JSON intents. The
future effect adapter must also guard document identity, not just a revision.
The durable file store now selects the self-contained v4 container for scientific
setup; direct bare JSON encoding still refuses it. Reopening restores exact captured
bytes, with no installed executable requirement or initializer invocation.

Deliberately the same four properties `kagami-catalog`'s authority already
established, so an adapter learns one pattern for both:

- **Guarded** — a submission may name the revision it was composed against; a
  mismatch is a refusal, so two adapters editing concurrently cannot silently
  clobber each other.
- **Idempotent** — a submission carries a `CommandId`; resubmitting one that
  already succeeded replays its recorded outcome. A *failed* command is not
  recorded, so a retry after a transient transport error still runs.
  Old receipts are bounded by count, command count and scientific retained bytes;
  a receipt evicted under pressure is outside the replay window. Revision guards
  remain essential for retries beyond that window.
- **Attributed** — every accepted command records its actor on the resulting
  event.
- **Observable** — every accepted command appends one bounded
  `ExperimentEvent`, so a view catches up from where it was rather than
  re-reading everything. `can_catch_up_from` says when a view has fallen
  further behind than the retained window, instead of silently returning a
  truncated one.

Scientific capture and scientific Open are staged transactions. Before publication,
the authority counts unique buffer allocations plus canonical metadata weight
across current state, undo/redo and replay receipts (default 512 MiB). It may evict
the oldest replay prefix but must retain the new receipt. It does not silently
trim undo/redo to make a capture fit: `scientific_retention_limit` leaves revision,
history, dirty state, counters and gesture/events unchanged. Scientific edit
preflight runs this same rule. Open also reapplies the receiving authority's
per-setup byte/count policy rather than trusting the decoder's policy.

The tally pins shared allocations while accounting, so equal digests in independent
allocations count separately and address reuse cannot produce free retention.
Normal timestep edits, undo and redo share captures and add no new opaque buffers.
Only cold capture/Open transactions copy bounded registry/replay metadata; ordinary
gestures do not. Work is proportional to retained capture references and unique
buffers with ordered-map lookup; no opaque bytes are copied or rehashed by the
tally. This bounds authority retention, not allocator overhead, external snapshot
holders, pending initialization, JIT memory or file-I/O staging.

## Three decisions worth knowing

- **Undo and redo are commands, not a client stack.** Only the authority can
  say what the experiment was and validate that it may be restored. One
  consequence, which the UI must show: a user and an agent share one history,
  so an agent's undo reverses whatever the last accepted edit was — including
  the user's.
- **A gesture is explicit, and stale identities are refused.** `BeginInteractiveEdit`
  mints a `GestureId`; submissions naming it join one undo entry, so a
  hundred-frame drag undoes as one step. Naming a gesture that has finished is
  a refusal rather than a silent downgrade, and any submission that does *not*
  name the open gesture closes it — an adapter that drops its
  `EndInteractiveEdit` cannot coalesce the rest of the session into one entry.
- **Dirty state is derived.** From the revision last persisted, not a flag an
  adapter sets — a boolean somebody has to remember to clear is a boolean that
  will be wrong after a rejected save. Undoing back to the persisted revision
  does not clear it: the revision moved forward, so the file on disk is not
  this experiment even though the contents match.

## What is not here

The JSON experiment codec now writes envelope version 3, which supports exact
component contribution pins. Versions 1 and 2 are explicitly converted while
preserving their legacy logical references, not choosing installed providers.
Exact pins mislabeled as v1/v2 are refused. The independently versioned default
view is unchanged. Scientific drafts use the separate
[stored-ZIP v4 container](../../docs/experiment-container-v4.md), with exact retained
declarations/configuration/state/history and no embedded executable code. Both
decoders return the normalized in-memory `ExperimentDocument`; save chooses the
format by its explicit setup variant. Neither format is a workload export.

The authority accepts an owned schema snapshot from the plugin adapter.
`PluginStore::resolve_authoring` can supply exact, verified selected component
schemas, and `adopt_schemas` changes capability reporting without modifying
experiment intent or its revision. Plugin property constraints govern ordinary
commands, variable repricing and capability checks. Application startup/document
selection wiring remains separate integration work.

No run: no play, pause, step, clock, tick pacing, solver, or observation.
[ADR 0004](../../docs/adr/0004-separate-authoring-commands-from-run-observations.md)
puts those behind a separate run authority, and Field CAD's tick-boundary
command queue does not transfer because no solver writes this model.

No transport, no authentication, no tool schemas, no presentation state, and no
catalog writes — the catalog keeps its own authority and its own revision.

## See also

- [ADR 0019](../../docs/adr/0019-kagami-experiment-document-model.md) — the
  decisions this crate implements.
- [Kagami document programme](../../docs/tasks/kagami/README.md) — this crate is
  task K3; persistence (K4), catalog instantiation (K5) and app adoption (K6)
  build on it.
- [`kagami-document`](../kagami-document/README.md) — the model it decides over.
