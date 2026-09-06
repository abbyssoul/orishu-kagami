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

Deliberately the same four properties `kagami-catalog`'s authority already
established, so an adapter learns one pattern for both:

- **Guarded** — a submission may name the revision it was composed against; a
  mismatch is a refusal, so two adapters editing concurrently cannot silently
  clobber each other.
- **Idempotent** — a submission carries a `CommandId`; resubmitting one that
  already succeeded replays its recorded outcome. A *failed* command is not
  recorded, so a retry after a transient transport error still runs.
- **Attributed** — every accepted command records its actor on the resulting
  event.
- **Observable** — every accepted command appends one bounded
  `ExperimentEvent`, so a view catches up from where it was rather than
  re-reading everything. `can_catch_up_from` says when a view has fallen
  further behind than the retained window, instead of silently returning a
  truncated one.

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
