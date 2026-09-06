# 0019 — The experiment document is a validated model behind one document server

Status: **accepted**  
Date: **2026-09-06**  
Refines: [ADR 0004](0004-separate-authoring-commands-from-run-observations.md),
[ADR 0005](0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0006](0006-mcp-ui-equivalence.md),
[ADR 0008](0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0012](0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0018](0018-catalog-values-are-captured-by-reference-not-linked.md)

Refined by: [ADR 0020](0020-compose-object-behaviour-through-plugin-components.md),
[ADR 0022](0022-persist-default-view-outside-experiment-intent.md),
[ADR 0023](0023-fields-are-plugin-modelled-domain-state.md)

## Context

ADR 0004 established *that* Kagami has a document authority. ADR 0006 requires
UI and MCP to reach it identically. ADR 0012 requires it to be independent of
windows and renderer state so it can later be hosted headlessly. None of them
says what the experiment model *is*, and `apps/kagami` still renders a demo
scene tree that `docs/migration.md` marks as placeholder. Every remaining
Kagami work package — catalog instantiation, MCP authoring parity, workload
compilation, local preview — is blocked on that gap.

Field CAD is the reference implementation. It shipped an atomic validated
`World::commit`, a checkpoint-based undo stack, an interactive-edit bracket, a
versioned scene document with a durable write protocol, and correlated command
receipts. Those mechanics are proven and should be transferred.

Several of its *decisions*, however, do not survive contact with this
repository's accepted ADRs, and transferring the code without recording the
divergences would silently reintroduce them:

- Field CAD ADR 0002 chose a plain object model and deferred composition.
  ADR 0018 here already commits to entities composed from plugin-contributed
  component schemas, and `kagami-catalog` already materialises exactly that
  shape.
- Field CAD ADR 0026 kept authored formulas in an `ExpressionDocument` *beside*
  the world, so the world held only resolved SI numbers. ADR 0005 requires the
  retained expression source to *be* the persisted intent.
- Field CAD's `HeadlessServer` owned the world, the catalog, the simulation
  clock, the solver plugins, the command queue and observation histories in one
  1,900-line type. ADR 0004 splits authoring from execution, and ADR 0008 gives
  the catalog its own authority — which `crates/kagami-catalog` already
  implements.
- Field CAD's `SetObjectVisible` made visibility authored world state. ADR 0012
  lists visibility alongside camera and selection as *not* authoritative
  experiment state.

## Decision

### The experiment model is a pure value, in its own crate

`crates/kagami-document` owns the authoritative experiment model, the closed
set of authoring commands, the pure transition that folds a command batch into
a candidate, and the validation that decides it. It is sans-IO: no filesystem,
clock, randomness, network, UI toolkit, MCP transport, or solver. Its public
inputs are owned values the caller supplies — notably the installed component
schemas — never process-global registries.

An experiment is an immutable state behind a shared pointer plus monotonic
identity counters, so capturing it is a pointer copy rather than a deep clone.
Reads go through an immutable snapshot that carries the revision it describes,
so no caller can observe half of an edit.

### An object is an entity composed from plugin-contributed components

An object is a named pose in space — transform, velocity, and optional shape.
Everything physical is a
component: a plugin-qualified component type, the schema version its values
were checked against, and its authored properties. The document contains no
species enum, no `if electron`, and no branch on a component name; a component
becomes authorable the moment its schema is registered.

ADR 0020 refines motion semantics: Dynamics presence opts an object into
integration; without it the object remains kinematic/static during a run. No
separate motion-authority value is persisted. ADR 0023 separately defines
plugin-owned field state and computational-model selection over the domain.

This adopts Field CAD ADR 0021's composition lesson on top of ADR 0018's
vocabulary. It is not a commitment to an ECS *storage* library: the collection
stays an ordered map until a measured query pattern justifies otherwise, per
`docs/Coding style.md`.

### Expression-capable properties retain their source

A property whose schema declares it expression-capable persists the authored
expression source together with the canonical SI magnitude and the dimension
its schema asked for. There is no separate expression document. Resolved
values elsewhere in the model are a derived projection that may be cached for
interactive speed and must be invalidated from dependency changes; they are
never authoritative and are never the thing a save writes as intent.

Document variables are persisted experiment intent with stable document-local
identities independent of their editable names, an explicit namespace, the
expression source, and optional description. They resolve through the shared
`orishu-variables` engine, which is the only parser and evaluator in the
product (ADR 0007).

### Every change is a validated candidate, adopted atomically

A command expresses intent, not replacement state. A batch is applied to a
candidate; the complete candidate — including the transitive closure of every
affected expression — is validated against the component schemas, dimensions,
referential integrity and declared resource limits; and it is adopted as
exactly one new revision or rejected with a domain reason while the previous
revision remains current. This is Field CAD ADR 0007's discipline, restated by
`GUIDELINES.md`, with schema and dimension validators in place of solvers.

Limits are declared and enforced before externally supplied input is adopted,
because MCP is a first-class caller: object, component and variable counts,
name lengths, expression source length and graph size, batch size, and history
depth.

### Undo restores a captured experiment, forwards

An undo entry is a *captured experiment*, not an inverse command. Writing an
inverse for every command variant would have to restore a removed object under
the identifier it had, would need a new inverse for every command a plugin
later adds, and would produce a plausible-but-wrong state when one is written
incorrectly. Capturing is a pointer copy, so an entry is cheap and unchanged
sub-states are shared between entries.

Restoring moves the revision *forward*: a revision is a point in history, not a
place to return to. Identity counters are never rewound, so undoing a creation
frees no identifier and no later object inherits a predecessor's identity.
A restore is validated like any other edit, because a candidate that was
representable when captured may not be after a plugin change; a refusal leaves
the history untouched. Undo and redo are commands submitted to the document
server, not a client-side stack, so UI and MCP share one history (ADR 0006).

This adopts Field CAD ADR 0024 in full. Its rule that a solver tick clears the
history does not apply: no solver writes this model.

### An interactive edit is one transaction, not a run control

A gesture that spans frames — a viewport drag, a held inspector control — is
bracketed explicitly, and every commit inside the bracket joins one history
entry, so a hundred-frame drag undoes as one step. The bracket is an ordinary
correlated command so MCP can express the same grouping.

This adopts the transaction half of Field CAD ADR 0023 and drops its
run-suspension half: suspending a run belongs to the run authority, which this
crate does not know about.

### The document server is the sole mutation path

`crates/kagami-session` owns the document server. It holds the current
revision, decides submitted command envelopes, and publishes read projections
and change events. It is the only way an experiment is created or modified.
UI actions, MCP tool calls, catalog instantiation, file open, undo and redo all
become the same typed envelope; no adapter receives mutable access to the model
and successful transport never implies acceptance.

The envelope contract mirrors the accepted catalog authority in
`crates/kagami-catalog`, so adapters learn one pattern:

- **Guarded** — an envelope may name the revision it was composed against; a
  mismatch is a refusal, so two adapters editing concurrently cannot silently
  clobber each other.
- **Idempotent** — an envelope carries a caller-chosen command identity;
  resubmitting one that already succeeded replays its recorded outcome. A
  failed command is not recorded, so retrying after a transient error still
  runs.
- **Attributed** — every accepted command records who submitted it.
- **Observable** — every accepted command appends one bounded event, so a view
  can catch up from the revision it last saw.

Separating the server from the model crate keeps the sans-IO boundary
mechanical rather than aspirational: persistence, revision bookkeeping and
event retention live on one side of it, and the transition that decides an edit
lives on the other, testable as `(model, command) -> outcome` triples.

### What the document is not

- **Not run state.** No play, pause, step, clock, tick pacing, tick-boundary
  queueing, solver, or observation history. Field CAD ADR 0011's queueing and
  Field CAD's refusal to undo while running belong to the run authority. A
  submitted workload is frozen at an exact revision, so a document authority
  may own a successor draft while another client observes the run. ADR 0022's
  Kagami window switches explicitly back to Authoring before exposing edits.
- **Not presentation.** Camera, selection, layout, per-object hiding,
  observation subscriptions, playback cursor and rate are client-local. Hiding
  an object neither dirties the experiment nor advances its revision. ADR 0022
  permits a separate default-view section in the saved file; that section is
  not experiment intent.
- **Not a catalog link.** Instantiation copies complete self-contained object
  state plus historical provenance. There is no tracking relationship, no
  instance index, and no propagation protocol (ADR 0008). An author may
  separately write an explicit catalog-qualified expression, which remains a
  live authoring dependency until workload compilation captures it (ADR 0018).
- **Not the catalog's owner.** The catalog has its own authority and its own
  revision; a catalog reload changes that projection and nothing else.

## Consequences

- Kagami gains a testable authoring core that runs without a window, a GPU, a
  network, or a solver, satisfying ADR 0012's requirement that the document
  authority be hostable headlessly later.
- The demo scene tree in `apps/kagami` becomes replaceable: the UI renders a
  read projection and submits envelopes, exactly as the MCP adapter does.
- `kagami-catalog`'s materialised object candidate is committed unchanged, so
  the catalog needs no second object representation.
- Persisting the model must preserve expression source, stable identities,
  namespaces, dimensions and schema versions — not resolved numbers alone — or
  reopening a document silently discards the author's intent.
- Two adapters share one undo history. An MCP client's undo reverses whatever
  the last accepted edit was, including the user's; the UI must show what a
  history operation will restore rather than offering an unlabelled arrow.
- Making visibility client-local means a decluttered arrangement does not
  travel with a shared draft. If that turns out to matter to researchers, it
  becomes a named view or an amendment to ADR 0012 — not a quiet field on the
  object.
- Restoring history forwards means the revision counter never decreases, so
  every projection, cache and event consumer can treat revisions as monotonic.

## Non-goals

- Adopting Field CAD's `fieldcad-core::World`, `fieldcad-scene-document`, or
  `fieldcad-server` types wholesale, or preserving their names as compatibility
  aliases.
- A second expression parser, evaluator, or unit catalog beside
  `orishu-variables`.
- Multi-writer merge semantics, operational transformation, or CRDTs; ADR 0012
  keeps live collaborative editing out of the first product.
- Making the document authority aware of Orishu, cluster membership, or
  workload admission.

## Implementation

Tracked as the [Kagami capability programme](../tasks/kagami/README.md), which
carries K-DOCUMENT in the [implementation roadmap](../roadmap/README.md).
