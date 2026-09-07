# Instantiate catalog templates into the document

Status: **slices 1 and 2 implemented**; slice 3's catalog-independence half is
covered by recorded provenance, and its live-dependency half waits on K2's
catalog symbol source  
Work package: **K-CATALOG** × **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0008](../../adr/0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0018](../../adr/0018-catalog-values-are-captured-by-reference-not-linked.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md)  
Stories: [Create an object from the catalog](../../user-stories/kagami/authoring.md),
[Reuse a catalog value in an expression](../../user-stories/kagami/authoring.md)

## Outcome

The accepted `crates/kagami-catalog` gains the consumer it was built for. A
researcher picks a template, supplies parameter bindings, and gets a complete
object in the current experiment — one document revision, one undo entry — that
resolves without the catalog that produced it.

This closes [the catalog task](../implement-kagami-object-catalog.md)'s
remaining integration gate, which reads "document/workload/UI/MCP integration
gated" in the roadmap registry today.

## Owning boundary

The instantiation bridge only. `kagami-catalog` already owns loading, validation,
availability, the catalog authority, and `materialize`'s pure production of an
`ObjectCandidate`; `kagami-document` owns what a valid object is. This task
owns the seam that resolves a request against one catalog snapshot and commits
the resulting self-contained object through the document authority.

## Consumed contracts

- `kagami_catalog::{CatalogAuthority, CatalogRevision, CatalogSet}` — the
  immutable snapshot an instantiation resolves against.
- `kagami_catalog::materialize::{InstantiationRequest, ObjectCandidate, materialize, InstantiationError}`
  — implemented and accepted; call it, do not reimplement it.
- `kagami_catalog::{TemplateIdentity, ContentFingerprint, TemplateProvenance}`.
- K2's document definitions and expression graph; K3's envelope contract.

`ObjectCandidate` contains component values, copied definitions, an object-local
scope, and historical provenance. K1 currently stores only the first of those;
K2 is required before the bridge can preserve the complete self-contained
candidate rather than dropping its definition closure.

## Source assessment

- `../field-cad/crates/fieldcad-catalog/src/instantiate.rs` and its
  `CatalogLink` machinery show the shape that is **explicitly rejected** here:
  Field CAD kept a tracking link from an instance back to its template, plus
  `preview_catalog_propagation` / `resolve_catalog_propagation` /
  `ApplyCatalogTemplate` compare-and-apply. ADR 0008 drops all of it.
  Instantiation is snapshot materialisation; creating from a changed template
  is a new instantiation.
- `../field-cad/docs/adr/0019-generic-particle-catalog-is-data.md` supplies the
  lesson that *does* transfer: a catalog entry is generic component data and
  never selects executable physics.

## Implementation slices

### 1. The instantiation command — **implemented**

- Add `InstantiateObjectTemplate` to `SessionCommand`, carrying the template
  identity, the content fingerprint the caller believes it is instantiating,
  the parameter bindings as authored expressions, and the instance inputs the
  template does not own: name, transform, or velocity. There is no persisted
  motion-authority/pinned state; Dynamics presence controls integration.
- The authority resolves it against **one** immutable catalog snapshot, mints
  the object identity, translates the materialised candidate into one pure
  document transition, and validates it as any other edit. Keeping resolution
  at the session boundary prevents the sans-IO model transition from acquiring
  a catalog authority as hidden input.
  A fingerprint mismatch is a refusal, not a silent upgrade to newer content
  the caller never saw.
- The accepted object persists the materialised components, the copied
  definition closure rewritten to object-local identities, and the provenance
  (catalog, template, schema version, fingerprint) as **historical evidence,
  not a pointer**.

### 2. No tracking link, and prove it — **implemented**

- There is no catalog-instance index, no propagation preview, no compare/apply,
  and no command that refreshes an object from its template.
- Editing, reloading, renaming, or deleting a template — or removing the
  catalog entirely — leaves every materialised object byte-identical. This is a
  test, not a comment.

### 3. Make the distinction visible — **partially implemented**

- The read projection distinguishes an object materialised from a template
  (catalog-independent, carrying historical provenance) from an expression that
  still names a catalog binding (a live dependency supplied by K2). ADR 0018
  requires the UI to make that distinction visible, and it cannot if the
  projection collapses them.

Catalog-qualified expression resolution is owned by K2; capture into the
workload is owned by
[capture-catalog-values-in-expressions](../capture-catalog-values-in-expressions.md).
Instantiation must not become a prerequisite for either behavior.

## Implementation record

`SessionCommand::InstantiateObjectTemplate` carries an `InstantiationSpec`; the
authority resolves it against one adopted `CatalogSet` and commits the result
through the ordinary command path. 10 acceptance tests in
`crates/kagami-session/tests/instantiate.rs`.

Slice 3's two halves separated in practice. `Object::provenance` records the
template, fingerprint and source location an object was materialised from, so a
projection can say which objects are catalog-independent — that half is done and
survives save/open. The other half, reporting which *expressions* are live
catalog dependencies, needs expressions to be able to name a catalog binding at
all, which is K2 slice 4's remaining symbol source. There is nothing to report
until there is something to depend on.

Four decisions are worth carrying forward:

- **Resolution happens at the session boundary, never in the transition.**
  `update` stays sans-IO with no catalog as a hidden input (ADR 0019). The
  authority materialises against one immutable snapshot and hands the model an
  ordinary command batch, so an instantiated object is validated, undone and
  revisioned exactly like a hand-authored one.
- **The object's scope is the identity it is about to be given.** Copied
  definitions are rewritten into `objects.object_N` before the object exists,
  using the next identity the counters will mint. Two instantiations of the
  same template therefore cannot collide.
- **The document re-derives every magnitude.** The catalog already resolved the
  candidate, but the bridge submits the *rewritten expressions* and lets the
  model price them. A value entering an experiment without validation would
  have broken the one invariant the whole model rests on, and instantiation is
  not an exception to it.
- **Document variables an override reads are captured as literals.**
  `resolve_variables` supplies their current magnitudes to the request, because
  instantiation materialises rather than links (ADR 0018).

Adopting a catalog publishes no event, unlike adopting schemas: a materialised
object carries a copy, so the catalog moving cannot change any projection over
the experiment. `changing_the_template_afterwards_leaves_the_object_identical`
is the test that would fail if a tracking link ever came back.

## Acceptance criteria

- Instantiating an available template produces exactly one revision and one
  undo entry, and the object resolves using only its own copied definitions.
- The accepted object can be resolved from its copied definitions with no
  catalog in scope. Once K4 is present, a cross-task integration test also
  proves the same property after save/open on a machine with no catalog.
- Instantiating an unavailable or invalid entry is refused with the catalog's
  own structured reason; nothing partial is committed.
- A stale content fingerprint is refused.
- Mutating or deleting the source template leaves materialised objects
  identical; no propagation path exists to invoke.
- The projection reports which objects are catalog-independent and which
  expressions are live catalog dependencies.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Template tracking, propagation, or a compare/apply protocol.
- Catalog file loading, writing, or validation — `kagami-catalog` owns those.
- Document-scoped catalog entries: Field CAD embedded templates in its scene
  document; ADR 0008 makes the catalog a property of the installation, not of
  an experiment.
- Workload compilation and closure capture.
- Catalog UI or MCP tools; they command the catalog authority and this command.
- Persisting an emitter as live template references. K12 reuses this task's
  bounded materialization path to snapshot self-contained spawn blueprints into
  an emitter command.
