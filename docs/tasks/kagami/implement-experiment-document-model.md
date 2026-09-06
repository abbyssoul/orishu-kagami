# Implement the experiment document model

Status: **implemented**; slices 1–8 landed as `crates/kagami-document`, and
the [downstream boundary follow-up](harden-document-boundaries.md) has since
landed on top of them  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0019](../../adr/0019-kagami-experiment-document-model.md),
[ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0005](../../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0018](../../adr/0018-catalog-values-are-captured-by-reference-not-linked.md)  
Stories: [Modify the experiment](../../user-stories/kagami/authoring.md),
[Undo and redo modifications](../../user-stories/kagami/authoring.md)

## Outcome

A new workspace library crate `crates/kagami-document` owns Kagami's
authoritative experiment model: what an experiment *is*, the closed set of
authoring commands that can change it, the pure transition that folds a
command batch into a candidate, the validation that decides that candidate,
and the checkpoint history that undo and redo restore.

The crate is sans-IO. It has no filesystem, clock, randomness, network, UI
toolkit, MCP transport, or solver dependency, and it holds no process-global
state: the installed component schemas are an owned value the caller supplies,
exactly as `kagami-catalog` already does.

The document *server* — revisions, command envelopes, guards, idempotent
replay, events and persistence — is
[K3](implement-document-authority.md)/[K4](persist-experiment-documents.md) and
lives in a separate crate. This task delivers only the value and the
transition, testable as `(experiment, commands) -> outcome` triples with
nothing running.

## Owning boundary

| Owns | Does not own |
| --- | --- |
| `Experiment` state, snapshots, checkpoints, identity counters | Revisions as a *service*, command envelopes, event log (K3) |
| `ExperimentCommand` and the pure `update` transition | Who is allowed to submit one, and in what order (K3) |
| Candidate validation against schemas, dimensions, and limits | Reading or writing a file (K4) |
| Undo/redo history as a value | The UI affordance that drives it (K6) |
| Object composition from plugin-contributed components | Declaring those component schemas (X-PLUGIN) |

## Consumed contracts

- `kagami_catalog::{SchemaRegistry, ComponentSchema, PropertySchema, PropertyKind, SchemaVersion}`
  — the installed component/property declarations to validate against.
- `kagami_catalog::{ComponentTypeId, PluginId, ComponentName, PropertyName, NameError}`
  — already-validated identifier new-types. Reuse them; do not define a second
  set. These conceptually belong to the future X-PLUGIN schema boundary, and
  both crates move together when it exists.
- `kagami_catalog::{Dimension, Unit}` — the SI base-exponent dimension type.
- `orishu_variables::{VariablesSystem, Namespace, Name, ExprParsingError, ExprEvalError}`
  — the one expression parser and evaluator in the product (ADR 0007).

The dependency direction is `kagami-document → kagami-catalog`. The catalog
does not and must not depend on the document: it *produces* candidates the
document consumes.

## Source assessment

- `../field-cad/crates/fieldcad-core/src/world.rs` is the implementation
  reference. Transfer: the `Arc<WorldState>`-plus-counters representation so a
  checkpoint is a pointer copy; per-collection `Arc` so a commit that touches
  one map shares the rest; monotonic identity counters that are never rewound;
  atomic all-or-nothing batch commit; `label()`/`batch_label()` living on the
  command so an undo entry, a log line and a remote client agree on the words.
- `../field-cad/docs/adr/0007-validate-before-adopting-a-world-edit.md` and
  `0024-undo-restores-a-captured-scene.md` are the decisions being adopted; both
  are restated in ADR 0019.
- Do **not** transfer: `SetObjectVisible` (ADR 0012 makes visibility
  presentation state), `CatalogLink`/`ApplyCatalogTemplate`/
  `LinkCatalogTemplate`/`UnlinkCatalogTemplate` (ADR 0008 has no tracking
  link), `RegisterComponentSchema` as a world command (schemas are an owned
  input, not document state), `MotionMode` (ADR 0020 and the boundary follow-up
  removed all persisted motion authority),
  or any probe/plane/region command (deferred to [K8](implement-requested-observations.md)).
- `crates/kagami-catalog` is the in-repo style reference: module layout, doc
  comments that explain *why*, `Limits` as one declared value, and
  `tests/dependencies.rs` asserting the dependency budget against the resolved
  graph.

## Implementation slices

### 1. Create the crate and its dependency guard

- Add `crates/kagami-document` to the workspace and register
  `kagami-document = { path = "crates/kagami-document" }` in
  `[workspace.dependencies]`.
- Dependencies: `kagami-catalog`, `orishu-variables`, `serde`, `thiserror`.
  Document the budget in the manifest, as `kagami-catalog` does.
- Add `tests/dependencies.rs` modelled on
  `crates/kagami-catalog/tests/dependencies.rs`, forbidding `iced`, `wgpu`,
  `winit`, `rfd`, `kagami-renderer`, `orishu`, `orishu-membership`,
  `orishu-identity`, `tokio`, `quinn`, `rustls`, `hyper`, and `glam`.
  `glam` is forbidden for the same reason it is in the catalog: the persisted
  document contract must not depend on a renderer's linear-algebra types.

### 2. Identities and geometry value types

- `ObjectId`: opaque, `Copy`, ordered, minted only by the crate's counters.
- `ExperimentRevision`: monotonic counter with `INITIAL`, `next()`,
  `Display` as `r{n}` — matching `CatalogRevision`'s spelling.
- `Counters`: per-kind monotonic allocation, serialisable, never rewound by a
  history restore.
- `DisplayName`: a bounded, non-empty, trimmed human label. Deliberately *not*
  an `orishu_variables::Name` — an object is called "Earth core", not
  `earth_core`, and names are labels that may collide (Field CAD US-18).
- `Vector3`, `Point3`, `Rotation`, `Transform`, `Velocity`, `ObjectShape`:
  small value types with private fields and validating constructors that
  reject non-finite components. `Rotation` normalises on construction and
  rejects a zero-norm quaternion, so a denormalised rotation is
  unrepresentable rather than checked at every use.

### 3. Objects composed from components

- The landed first pass used `Object { name, transform, velocity, shape,
  pinned, components }`. This is historical implementation detail, superseded
  by ADR 0020 and the boundary follow-up, which removed `pinned`: Dynamics
  presence decides integration, and `Object::participation` classifies the
  rest.
  No visibility field (ADR 0012); no species, kind, or catalog link.
- `ObjectComponent { type_id, schema_version, properties }`, structurally
  identical to `kagami_catalog::ObjectComponent`, so a materialised candidate
  commits without translation.
- `PropertyValue` is the *stored* form: `Quantity { source, si_value, dimension }`,
  `Boolean(bool)`, `Text(String)`. The retained `source` is the authored
  intent (ADR 0005); `si_value` is derived and is *computed by this crate*,
  never accepted from a caller.
- `AuthoredValue` is the *input* form a command carries:
  `Expression(String)`, `Boolean(bool)`, `Text(String)`. Parsing
  `AuthoredValue` into `PropertyValue` during validation is the single place
  the resolution rule is enforced.
- `ObjectSpec` is a builder for a creation command, so `CreateObject` cannot
  be constructed with a half-specified object.

### 4. Experiment setup

- `Domain`: axis-aligned bounds plus per-axis grid resolution and boundary
  condition. `TimeStep`: a positive finite duration in seconds.
- `PluginComposition`: the set of enabled simulation plugins by `PluginId`.
  Configuration property bags are X-PLUGIN's, not this slice's — record that
  as an explicit non-goal in the module documentation rather than inventing a
  placeholder schema.
- Keep this slice deliberately small and say so: the authoritative
  discretization schema is owned by the workload format (S-WORKLOAD) and the
  plugin contract (X-PLUGIN), and this is the minimum an experiment needs to
  be describable before either lands.

### 5. The command set and the pure transition

- `ExperimentCommand`, closed and serialisable:
  - objects — `CreateObject`, `RemoveObject`, `RenameObject`, `SetTransform`,
    `SetVelocity`, `SetShape`; the landed `SetMotionAuthority` was removed by
    the boundary follow-up before K4 persistence
  - components — `AttachComponent`, `DetachComponent`, `SetComponentProperty`
  - setup — `SetDomain`, `SetTimeStep`, `SetPluginEnabled`
- `label()` per variant and `batch_label(&[Self])` naming a batch after its
  first command and counting the rest, so one transaction reads as one step.
- `update(&Experiment, &[ExperimentCommand], &SchemaRegistry, &Limits) -> Result<Candidate, Rejection>`:
  pure, all-or-nothing. It applies onto a cloned candidate, validates the
  complete result, and returns the candidate plus a report of what was created;
  it never mutates its input. Adoption is the caller's decision, which is what
  makes K3's guarded envelope possible without a second transition.
- `Candidate` carries the next state and a `CommitReport` naming created
  identities, so an editor can select what it just made without diffing.

### 6. Validation and rejections

- Validate the complete candidate, not the individual command: names bounded
  and non-empty; transforms, velocities and extents finite; every referenced
  object present; component type registered in the supplied `SchemaRegistry`;
  every required property authored; property kind matching its schema; a
  quantity's expression parsing and evaluating to a finite value; its declared
  dimension matching the schema's.
- `Rejection` is a typed enum carrying a stable machine-readable reason, the
  affected entity or property path, and a human explanation — the bar the
  Field CAD story inventory's API rule 6 sets and `GUIDELINES.md` repeats.
- Note in the module documentation what dimension checking does *and does not*
  buy today, with the same honesty as `kagami-catalog`'s crate docs: the
  dimension is *declared* by the schema and checked against a literal
  expression, and derived-dimension inference through arithmetic arrives with
  [K2](integrate-document-variables.md) and S-VARIABLES slice 2. Do not build a
  second unit layer here.

### 7. Limits

- `Limits` as one declared value with a `DEFAULT` constant: objects per
  experiment, components per object, properties per component, commands per
  batch, name and text byte lengths, expression source bytes, expression
  symbol references, undo depth.
- Sized for an interactive installation and justified in the doc comment, as
  `kagami_catalog::Limits` is. A caller may tighten them for an MCP-facing
  authority; a test may drive a limit failure without generating a huge input.

### 8. Checkpoint history

- `EditHistory`: a bounded stack of `(ExperimentCheckpoint, label)` entries
  plus a redo stack, with `DEFAULT_UNDO_DEPTH`.
- `record(checkpoint, label)`, `undo()`, `redo()`, and read accessors naming
  what each side would restore, so an adapter can label the affordance rather
  than offering an unlabelled arrow.
- A gesture key coalesces every commit inside one interactive edit into the
  entry the gesture opened at (ADR 0019). The history stores the key; opening
  and closing a gesture is K3's envelope concern.
- Restoring returns a checkpoint for the caller to validate and adopt as a
  *new* revision. The history never rewinds a revision or an identity counter,
  and a caller's refusal to adopt leaves the history exactly as it was.

## Acceptance criteria

- `crates/kagami-document` builds with no filesystem, clock, randomness,
  network, UI, transport, or solver dependency, and `tests/dependencies.rs`
  asserts that against the resolved graph rather than against imports.
- Constructing a non-finite transform, velocity, extent, or rotation is
  impossible through the public API; a zero-norm rotation is refused.
- An object can be created with no components and then composed by attaching
  independently declared components; detaching one leaves the object valid.
  No code branches on a component's name.
- A batch that would fail on its last command changes nothing: the returned
  experiment, identity counters, and history are identical to the input.
- A command naming an absent object, an unregistered component type, a missing
  required property, a wrong property kind, or an expression whose declared
  dimension differs from its schema's is rejected with a typed reason naming
  the affected entity or property path.
- A quantity property retains its authored source verbatim and reports an
  `si_value` this crate computed. `1 / 3 + 0.1` resolves; an expression
  evaluating to NaN or infinity is rejected, never stored.
- Exceeding any declared limit fails within its bound without partially
  changing state.
- Undo restores previously captured contents as a *new* checkpoint; identity
  counters do not move backwards, so a re-created object never inherits a
  removed object's identity. Redo after an unrelated edit is discarded rather
  than replaying a stale branch.
- Every commit inside one gesture key produces exactly one history entry.
- The demo scene tree in `apps/kagami/src/scene_model.rs` is untouched by this
  task; replacing it is [K6](adopt-document-authority-in-kagami.md).
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Implementation record

Landed as `crates/kagami-document` with 82 tests: 60 unit, 18 authoring
integration tests through the public API (`tests/authoring.rs`), 3 dependency
budget assertions (`tests/dependencies.rs`), and one crate-level doctest.

Two decisions were made during implementation and are worth carrying forward:

- **`ObjectId` has no public constructor and no `Default`.** The first draft
  derived `Default`, and the "a batch that fails on its last command changes
  nothing" test silently passed for the wrong reason — `ObjectId::default()` was
  the identity of the object the test had just created, so its "invalid"
  command succeeded. An adapter still needs to turn a wire integer or a
  persisted value into an identity, so that is
  `ExperimentSnapshot::resolve_object(raw) -> Option<ObjectId>`: it can only
  succeed for an object that exists, and a stale identity is still refused at
  commit because the object may be removed between the read and the submission.
- **`Rejection` is size-bounded by a test.** It is the `Err` half of every
  fallible function here, so its size is paid on the success path too; a parse
  failure's payload is boxed and `a_rejection_stays_small_enough_to_return_by_value`
  fails if a future variant grows past 128 bytes unboxed.

The first persistence/plugin-inventory review subsequently found that
structural document validity must be separated from availability under the
currently installed schemas. That refinement was tracked and landed separately
as the [K1/K3 boundary follow-up](harden-document-boundaries.md), rather than
being folded back into this task's recorded implementation status.

## Non-goals

- Revisions as a service, command envelopes, guards, idempotent replay, event
  logs, or dirty state — [K3](implement-document-authority.md).
- Saving, opening, or any file format — [K4](persist-experiment-documents.md).
- The document variable graph, cross-property references, rename refactoring,
  or derived-dimension inference — [K2](integrate-document-variables.md).
- Catalog instantiation — [K5](instantiate-catalog-templates.md).
- Probes, distance probes, aggregate probes, slice planes and sampling regions
  — [K8](implement-requested-observations.md). Particle emitters are tracked by
  [K12](implement-particle-emitters.md).
- Run state of any kind: play, pause, step, clocks, tick pacing, tick-boundary
  queueing, solvers, or observations (ADR 0004).
- Presentation state: camera, selection, layout, per-object hiding (ADR 0012).
- A second expression parser, evaluator, or unit catalog beside
  `orishu-variables`.
- Adopting Field CAD type names or compatibility aliases.
