# Instantiate catalog templates into the document

Status: **specified**; gated on [K2](integrate-document-variables.md) and the
accepted [K-CATALOG](../implement-kagami-object-catalog.md) core. The
[K1/K3 boundary follow-up](harden-document-boundaries.md) this task also needed
has landed  
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

### 1. The instantiation command

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

### 2. No tracking link, and prove it

- There is no catalog-instance index, no propagation preview, no compare/apply,
  and no command that refreshes an object from its template.
- Editing, reloading, renaming, or deleting a template — or removing the
  catalog entirely — leaves every materialised object byte-identical. This is a
  test, not a comment.

### 3. Make the distinction visible

- The read projection distinguishes an object materialised from a template
  (catalog-independent, carrying historical provenance) from an expression that
  still names a catalog binding (a live dependency supplied by K2). ADR 0018
  requires the UI to make that distinction visible, and it cannot if the
  projection collapses them.

Catalog-qualified expression resolution is owned by K2; capture into the
workload is owned by
[capture-catalog-values-in-expressions](../capture-catalog-values-in-expressions.md).
Instantiation must not become a prerequisite for either behavior.

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
