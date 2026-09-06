# Implement the document authority

Status: **implemented**; slices 1–5 landed as `crates/kagami-session`, and
the [downstream boundary follow-up](harden-document-boundaries.md) has since
landed on top of them  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0006](../../adr/0006-mcp-ui-equivalence.md),
[ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md)  
Stories: [Modify the experiment through MCP](../../user-stories/kagami/authoring.md),
[Author an experiment through MCP with UI parity](../../user-stories/kagami/mcp.md)

## Outcome

A new workspace library crate `crates/kagami-session` owns the **document
server**: the sole mechanism by which an experiment is created or modified.
UI actions, MCP tool calls, catalog instantiation, file open, undo and redo all
become the same typed command envelope submitted to it; no adapter receives
mutable access to the model, and successful transport never implies acceptance.

This is what releases [the MCP server task's slice 8](../kagami-mcp-server.md)
and gives [ADR 0006](../../adr/0006-mcp-ui-equivalence.md)'s parity guarantee
something to be a property *of*.

## Owning boundary

| Owns | Does not own |
| --- | --- |
| The current experiment revision and its history | What a valid experiment is, or how a command changes one (K1) |
| Command envelopes: guards, idempotency, attribution | Transport, authentication, or tool schemas (`apps/kagami`, MCP task) |
| Bounded change events and read projections | Rendering, camera, selection, layout (K6) |
| Dirty state and the open-document identity | The file format and the write protocol (K4, same crate, separate module) |
| Undo/redo as commands, and gesture bracketing | Any run: play, pause, step, clocks, observations (ADR 0004) |

The catalog keeps its own authority and its own revision
(`kagami_catalog::CatalogAuthority`). This crate never writes a catalog file
and never advances a catalog revision; it consumes one immutable catalog
snapshot when a command needs one.

## Source assessment

- `crates/kagami-catalog/src/authority.rs` is the in-repo contract shape to
  mirror, so UI and MCP adapters learn one pattern: guarded by an expected
  revision, idempotent through a caller-chosen command identity, attributed to
  an actor, and observable through a bounded event log with `MAX_COMMAND_HISTORY`
  / `MAX_EVENT_HISTORY` retention and the `bounded_identity!` new-types.
- `../field-cad/crates/fieldcad-simulation/src/source.rs` supplies the
  correlated-command vocabulary worth transferring: a client-issued command
  identity echoed in its acknowledgement, an explicit lifecycle
  (`Applied`/`Rejected`/`Cancelled`), and receipts that report the authority's
  decision rather than delivery.
- `../field-cad/crates/fieldcad-server/src/lib.rs` is a **cautionary**
  reference only: a 1,900-line type that merged the document, the catalog, the
  simulation clock, solver plugins, the command queue, observation histories,
  run records and recording. ADR 0004 and ADR 0008 split those authorities;
  do not reassemble them here.
- Field CAD's tick-boundary command queue (`../field-cad/docs/adr/0011-…`) is
  **not** transferred: it exists because edits raced a running solver, and no
  solver writes this model.
- Field CAD's lesson that two command sources sharing one session can steal or
  cross-deliver completions (recorded in
  [the MCP task](../kagami-mcp-server.md)'s source assessment) is the reason
  command identity is minted by one owner and outcomes are recorded per
  identity.

## Implementation slices

### 1. The crate and the authority skeleton

- Add `crates/kagami-session`, depending on `kagami-document`,
  `kagami-catalog`, `orishu-variables`, `serde`, `thiserror`.
- Add `tests/dependencies.rs` forbidding UI, renderer, transport and Orishu
  runtime crates. This crate may touch the filesystem — but only inside
  [K4](persist-experiment-documents.md)'s `persist` module, and the test
  asserts no async runtime or network capability arrives with it.
- `DocumentAuthority` holds the current `Experiment`, its `ExperimentRevision`,
  the `SchemaRegistry` it validates against, the `Limits` in force, and the
  `EditHistory`.

### 2. The envelope contract

- `ExperimentCommandEnvelope { command_id, actor, expected_revision, gesture, commands }`.
  `command_id` and `actor` are bounded identities; `commands` is a batch
  committed atomically; `gesture` optionally names the interactive edit this
  submission joins.
- `submit(envelope) -> ExperimentOutcome`, where the outcome is the accepted
  revision plus a commit report, or a typed rejection.
- **Guarded**: a mismatched `expected_revision` is a refusal that changes
  nothing, so two adapters editing concurrently cannot silently clobber each
  other. `None` opts out.
- **Idempotent**: resubmitting a `command_id` that already succeeded replays
  its recorded outcome. A *failed* command is not recorded, so a retry after a
  transient transport error still runs.
- **Attributed**: every accepted command records its actor on the resulting
  event.
- **Observable**: every accepted command appends exactly one bounded
  `ExperimentEvent`, so a view catches up from the revision it last saw rather
  than re-reading everything.

### 3. History, gestures, and dirty state

- `Undo` and `Redo` are envelope commands, not a client-side stack, so UI and
  MCP share one history (ADR 0006). Each reports what it restored and advances
  the revision forward (ADR 0019).
- A restore is re-validated like any other edit; a refusal leaves the history
  untouched.
- `BeginInteractiveEdit` / `EndInteractiveEdit` bracket a gesture. Every commit
  carrying the open gesture key joins the entry the gesture opened at, so a
  hundred-frame drag is one undo step. A gesture that commits nothing costs
  nothing. An unclosed gesture is closed by the next non-gesture submission
  rather than blocking the authority indefinitely.
- Dirty state is derived from the revision last persisted, not from a boolean
  an adapter sets.
- Loading an experiment authors through the same command path and then clears
  the history: the first undo of a session must not empty the workspace.

### 4. Read projections

- `ExperimentView` at the current revision: objects, components, resolved
  values, setup, and the revision itself — serialisable, with no pointer into
  UI or renderer state (ADR 0012).
- `HistoryStatus`: whether undo/redo are available and the label of what each
  would restore.
- `validate(envelope)` — a preflight that runs the same validation and returns
  the same rejection type without mutating anything. Advisory only; the
  authority remains the final decider. This is Field CAD's US-26 and is what
  lets an agent repair input before submitting.
- Capability discovery: the registered component schemas, their properties,
  kinds, dimensions and required flags, so an automation client can construct
  a valid command instead of guessing.

### 5. Parity evidence

- One test drives the same command script through a "UI-shaped" caller and an
  "MCP-shaped" caller and asserts identical accepted revisions, commit reports,
  rejections and resulting views. Keep it where the submission methods are
  visible so it cannot drift into testing a mock.
- Hostile-input tests: oversized identities, oversized batches, unknown
  revisions, replayed identities, conflicting concurrent submissions, and a
  gesture left open. Each produces a bounded typed error and no partial state
  change.

## Acceptance criteria

- Every mutation of the experiment goes through `submit`; no public API
  returns mutable access to the model, and the crate exposes no other write
  path.
- A rejected envelope leaves the revision, model, history and event log
  byte-identical to their prior state.
- Resubmitting a succeeded `command_id` returns the recorded outcome and does
  not apply the change twice; resubmitting a failed one runs normally.
- A stale `expected_revision` is refused; an absent one applies to the latest.
- Undo and redo submitted through either adapter operate on one shared
  history, advance the revision forward, and never rewind an identity counter.
- Every commit inside one gesture is one history entry; the entry's label names
  the edit.
- `validate` returns the same decision the equivalent `submit` would, and
  changes nothing.
- The parity test passes: identical scripts through both callers produce
  identical revisions, reports and views.
- No run control, clock, solver, observation, catalog write, or cluster
  dependency exists in the crate.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Implementation record

Landed as `crates/kagami-session` with 39 tests: 11 unit, 19 authority and
hostile-input tests, 5 parity tests, 3 dependency-budget assertions, and one
crate-level doctest.

Four decisions were made during implementation:

- **A stale gesture identity is refused, not downgraded.** Treating it as
  ungestured would be defensible, but a finished drag's identity would then
  keep coalescing later edits into that drag's undo entry. Refusing surfaces
  the adapter bug instead. Symmetrically, any submission that does *not* name
  the open gesture closes it and publishes an
  `ExperimentChange::GestureClosed { implicit: true }`, so an adapter that
  drops its `EndInteractiveEdit` cannot coalesce the rest of the session into
  one entry.
- **Events carry their own sequence, not just a revision.** Opening and
  closing a gesture are observable session changes that advance no revision; a
  view catching up by revision alone would miss them and could not tell whether
  a gesture is holding. `can_catch_up_from` reports when a view has fallen
  further behind than `MAX_EVENT_HISTORY`, rather than silently returning a
  truncated window.
- **`preflight` declines undo, redo and gesture brackets** with
  `not_preflightable` rather than answering. Whether undo is available is a
  read (`history_status`), and answering it as a validation result would invite
  callers to treat preflight as a dry run of a whole submission.
- **`CommandId`/`ActorId` are this crate's own types**, not
  `kagami-catalog`'s, despite the identical shape. A command identity is only
  meaningful against the authority that recorded it; interchangeable types
  would let a catalog identity replay through the document authority.

One guard is deliberately not covered by a test: `apply_undo`/`apply_redo`
operate on a cloned history so a restore refused by validation costs nothing.
That refusal was unreachable through the public API as this task left it,
because the schema registry was fixed at construction. The
[K1/K3 boundary follow-up](harden-document-boundaries.md) added
`DocumentAuthority::adopt_schemas`, which makes it reachable and exercises the
guard — kept here rather than deferred precisely because adding it later would
have meant finding this again.

That same follow-up landed the other refinements the persistence and
real-adapter review found: cached replay bound to the original actor and
payload, save completion naming the written revision, and explicit
serializable representations for the logical adapter values. They are recorded
there rather than claimed as part of this task's implementation record.

## Non-goals

- MCP transport, authentication, or tool schemas — [the MCP task](../kagami-mcp-server.md).
- Run authority of any kind: local preview or cluster submission (K-RUN,
  K-PREVIEW).
- A headless multi-user document service: identity, permissions, shared undo
  semantics across users, reconnection, or a remote protocol (ADR 0012).
- Cancelling a submitted command; there is no queue behind a tick boundary to
  cancel it from.
- Presentation state of any kind.
