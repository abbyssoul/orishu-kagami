# Harden the landed document and session boundaries

Status: **implemented**; slices 1–6 landed in `crates/kagami-document` and
`crates/kagami-session`, unblocking
[K4](persist-experiment-documents.md), [K5](instantiate-catalog-templates.md),
and [K6](adopt-document-authority-in-kagami.md)  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0006](../../adr/0006-mcp-ui-equivalence.md),
[ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md),
[ADR 0020](../../adr/0020-compose-object-behaviour-through-plugin-components.md)

## Why this follow-up exists

K1 and K3 correctly established the sans-IO experiment transition and the one
document authority. Reviewing their downstream contracts exposed five seams
that could not be exercised before persistence, plugin inventory changes, and
real adapters existed:

- K4 must preserve a component whose simulation plugin is absent, while the
  current complete-candidate validator requires every component type to be in
  the authority's fixed `SchemaRegistry`.
- installing or removing a plugin must change capability availability without
  mutating the experiment or advancing its revision, but the authority cannot
  replace or re-evaluate its registry today;
- idempotent replay currently identifies only a `CommandId`, so reusing a
  successful identity with a different actor or payload can replay an outcome
  for a request that was never accepted; and
- `mark_persisted()` does not identify the snapshot that was written, so an IO
  completion for revision N could incorrectly mark revision N+1 clean.
- K1 persisted a top-level `pinned`/motion-authority flag. ADR 0020 now makes
  Dynamics the sole opt-in to run-time integration, so retaining that flag
  would leave two conflicting owners of motion.

These are refinements of landed behavior, not reasons to reopen ADR 0019 or
rewrite either crate.

## Outcome

The experiment distinguishes durable authored content from its availability
under the installation's current schemas; the authority can adopt a new schema
snapshot without changing authored intent; replay is bound to the request that
earned the cached outcome; and save completion can acknowledge only the exact
revision and target that were written.

## Implementation slices

### 1. Separate structural validity from capability availability

- Define the invariants that can be checked from persisted authored data alone:
  identities and counters, finite geometry, collection and source bounds,
  referential integrity, property representation, and stored schema identity.
- Treat the installed `SchemaRegistry` as validation context, not part of the
  experiment. A missing component schema produces a structured
  **unavailable** diagnostic and preserves the component; it is not a corrupt
  document and never causes a substitute schema or default value to be used.
- Edits that create, attach, or change content governed by an unavailable or
  incompatible schema are refused. Unrelated edits remain possible without
  silently revalidating the unavailable component under guessed rules.
- Keep workload compilation stricter: any unavailable or incompatible content
  blocks compilation until the exact capability is restored or an explicit
  migration command changes the experiment.

### 2. Refresh capability context without authoring a revision

- Add one authority operation for adopting an immutable schema-registry
  snapshot supplied by the plugin-inventory adapter. It recomputes the
  availability projection and emits a sequenced session event, but does not
  mutate the experiment, dirty it, add history, or advance its revision.
- Recheck restored undo/redo checkpoints against the *current* registry. A
  refused restore leaves both history directions untouched, exercising the
  guard K3 deliberately retained.
- Expose availability and compatibility diagnostics in `SessionView` without
  copying schema definitions into authoritative experiment state.

### 3. Bind idempotency to actor and payload

- Retain enough of every accepted request to compare a repeated `CommandId`
  with the actor, guard, gesture, and command that originally used it.
- Replay only an identical request. Reuse of the identity with any different
  field is a typed `command_identity_conflict` refusal and reveals no cached
  outcome belonging to another actor.
- Keep the existing bounded retention behavior. An identity that has aged out
  is a new request; this limitation is explicit in capability discovery and
  future transport documentation.

### 4. Make persistence completion revision-qualified

- Replace the unqualified `mark_persisted()` hook with an acknowledgement that
  names the captured revision and the successfully written document target.
- Mark the session clean only when its current revision is the acknowledged
  revision. A late completion for an older snapshot updates no clean marker.
- Define Save As target adoption atomically with the successful acknowledgement:
  a failed write changes neither the current target nor dirty state.
- New/Open/Replace remain attributed authority operations over decoded values;
  path selection, byte IO, clocks, and user prompts stay in the imperative
  shell described by K4 and K6.

### 5. Preserve a serializable logical boundary

- Give snapshots, authoring commands, command envelopes, and outcomes explicit
  serializable representations suitable for an in-process MCP adapter and a
  future headless host. A serde derive on an incidental private layout is not
  by itself a versioning policy.
- Keep those representations distinct from both the experiment-file format
  and any future remote collaboration protocol. Publishing a network protocol,
  authentication model, or compatibility promise still requires the decision
  required by ADR 0012.
- Add round-trip fixtures for the representations and conversion tests proving
  malformed values cannot bypass the same constructors and bounds production
  submission uses.

### 6. Remove persisted motion authority

- Remove `pinned` from `Object`, `ObjectSpec`, projections and serialized
  command values, and remove `SetMotionAuthority`.
- An object without a compatible Dynamics component is kinematic/static during
  a run. An object with Dynamics is integrated under ADR 0020; editing its
  initial pose or velocity remains an ordinary Authoring-mode command.
- Add conversion coverage for any interim serialized adapter fixture that
  contained `pinned`; K4's first public experiment-file version must never emit
  it.
- Replace `is_inert() == components.is_empty()` with capability classification
  derived from registered component schemas: a static field source or emitter
  participates in physics even without Dynamics.

## Acceptance criteria

- A structurally valid experiment containing an uninstalled component remains
  readable and editable outside that component, while editing or compiling the
  unavailable content is refused structurally.
- Removing and reinstalling a schema changes only availability diagnostics and
  event sequence; experiment revision, bytes, history, counters, and dirty
  state do not change.
- Undo or redo that is incompatible with the current registry is refused with
  both history stacks unchanged.
- Repeating an identical accepted envelope replays its outcome; repeating its
  identity with another actor, guard, gesture, or command is refused and makes
  no state change.
- A save of revision N followed by an edit to N+1 and then N's completion
  leaves N+1 dirty. A failed Save As preserves both the old target and dirty
  state.
- The serializable adapter representations round-trip through golden fixtures,
  remain separate from the experiment-file envelope, and reject malformed or
  over-limit input through production validation.
- No persisted motion-authority flag or command remains. Dynamics presence is
  the only ordinary integration opt-in, while non-dynamic sources and emitters
  remain correctly classified as participating components.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Implementing K4's JSON format or filesystem write/recovery protocol.
- Defining the simulation-plugin package or installation manager (X-PLUGIN);
  this task consumes an owned schema snapshot supplied by that future owner.
- Migrating unavailable content to a newer schema. Migration is an explicit
  authoring operation and needs the plugin compatibility contract first.
- A remote multi-writer protocol, authorization policy, or durable command log.

## Implementation record

Landed across both crates: `kagami-document` grew from 82 to 107 tests and
`kagami-session` from 39 to 59. The new surfaces are
`kagami_document::capability`, `kagami_document::wire`,
`kagami_session::persist` and `kagami_session::wire`.

| Slice | Where it landed |
| --- | --- |
| 1 — structural vs capability | `capability.rs`; `validate.rs` splits into `validate_structure`, `validate_governed` and `validate_restorable`; `tests/capability.rs` |
| 2 — refresh capability context | `DocumentAuthority::adopt_schemas`, `capabilities()`, `SessionView::capabilities` |
| 3 — replay binds actor and payload | `AcceptedRecord`, `SessionRejection::CommandIdentityConflict`, `MAX_REPLAY_COMMANDS` |
| 4 — revision-qualified persistence | `kagami_session::persist`: `DocumentTarget`, `SaveAcknowledgement`, `DocumentAuthority::acknowledge_save` |
| 5 — serializable boundary | `kagami_document::wire`, `kagami_session::wire`, golden fixtures under each crate's `tests/fixtures/` |
| 6 — no persisted motion authority | `pinned` and `SetMotionAuthority` removed; `Object::participation` replaces `is_inert` |

Five decisions were made during implementation and are worth carrying forward:

- **A schema *version* difference is not by itself a capability gap.** Stored
  values are checked against the installed declaration directly — property,
  kind, dimension, requirement — rather than against the version they were
  first accepted under. Treating a bump as a gap would make every plugin
  release break every document, and deciding which version *steps* are
  compatible regardless of shape is X-PLUGIN's contract to define, not
  something to guess here. The recorded `schema_version` remains provenance,
  and an edit re-stamps it with the declaration that actually checked the
  value.
- **Restore is weaker than an edit in one direction and stronger in another.**
  Captured contents whose plugin was uninstalled mid-session still restore —
  refusing would mean uninstalling a plugin silently disables undo, and those
  contents are no different from a document loaded on that machine. Contents an
  *installed* schema refuses do not restore, because making them current would
  assert values that declaration rejects.
- **`DetachComponent` and `RemoveObject` stay possible on unavailable
  content.** Neither needs a schema to be correct, and an author who cannot
  interpret content must still be able to remove it; in-session undo puts it
  back.
- **The replay window is bounded on two axes.** Comparing a resubmission
  against the request that earned the cached outcome means retaining its whole
  batch, so counting submissions alone would let an MCP caller pin
  `MAX_COMMAND_HISTORY × max_commands_per_batch` commands in memory.
  `MAX_REPLAY_COMMANDS` bounds the total.
- **Slice 5 closed two live constructor-bypass holes.** `ObjectShape` and
  `Domain` derived `Deserialize` straight onto their fields, so
  `{"shape":"sphere","radius":-1}` and an inverted domain decoded fine. Both
  now decode through their constructors, and the whole document JSON contract
  was made uniformly camelCase while its shape was still unpublished.
