# kagami-document

Kagami's authoritative experiment model: what an experiment *is*, what may
change one, and whether a proposed change is valid.

This is the functional core of Kagami's authoring side. It is sans-IO — no
filesystem, clock, randomness, network, UI toolkit, MCP transport, or solver —
and holds no process-global state. `tests/dependencies.rs` asserts that against
the resolved dependency graph rather than against imports, because a transitive
`tokio` arriving through something innocuous is exactly the failure a review of
`use` statements misses.

```text
apps/kagami        UI, MCP adapter, dialogs      <- imperative shell
     |  command envelope        ^ read projection
kagami-session     revisions, guards, events     <- the document server
     |  update(model, commands) ^ Rejection
kagami-document    this crate                    <- pure, sans-IO
```

## What it owns

| Module | Responsibility |
| --- | --- |
| `id` | stable identities, and the counters that mint them |
| `geometry` | placement value types whose invariants are their constructors |
| `name` | human labels, which are not identities |
| `object` | objects as entities composed from plugin-contributed components |
| `setup` | the numerical domain, time step, and plugin composition |
| `command` | the closed set of authoring intents |
| `update` | the pure transition, and the candidate it produces |
| `validate` | the decision, and precisely why not |
| `history` | undo and redo, as captured experiments |
| `limits` | declared bounds on what one batch may ask for |

It owns none of: revisions as a service, command envelopes, event logs,
persistence, transports, presentation state, or any part of a run.

## The four properties worth knowing

- **Validate a candidate, adopt on success.** `update` applies a batch to a
  clone, validates the whole result, and returns a `Candidate` the caller may
  adopt. A failure anywhere leaves the input untouched, so a batch that fails on
  its last command changes nothing — structurally, not by each command being
  careful.
- **The source is the intent.** An expression-capable property stores what the
  author wrote alongside the canonical SI magnitude *this crate* derived from
  it. A command carries an `AuthoredValue`; the only path from there into an
  experiment runs through `validate`, so no caller can assert a magnitude its
  source does not produce.
- **Undo restores forwards.** A history entry is a captured experiment, not an
  inverse command. Restoring produces a *new, later* revision, and identity
  counters are never rewound — undoing a creation frees no identifier, so no
  later object inherits a removed predecessor's identity.
- **A rejection is data.** Every `Rejection` carries a stable `code()`, the
  affected entity or property path, and a human explanation, because an
  automation client has to repair its input and cannot parse prose to do it.

## Units, and what is deferred

An authored quantity names its unit *beside* the expression rather than inside
it, and a property's dimension is *declared* by its schema rather than inferred
through the expression's arithmetic. Both are the same deliberate limitation
`kagami-catalog` records, for the same reason: the dimension-aware value layer
belongs to the shared variables subsystem, and this crate will delegate to it
rather than grow a second implementation.

Until then an expression must be self-contained — a symbol reference is an
explicit `expression_unresolved` rejection, never a silent zero.

## See also

- [ADR 0019](../../docs/adr/0019-kagami-experiment-document-model.md) — the
  decisions this crate implements, including where Field CAD's prototype is
  deliberately superseded.
- [Kagami document programme](../../docs/tasks/kagami/README.md) — this crate is
  task K1; the document server, persistence, variables, catalog instantiation
  and app adoption follow.
