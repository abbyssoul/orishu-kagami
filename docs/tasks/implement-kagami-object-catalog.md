# Implement the Kagami object catalog

Status: **ready for catalog core; integration is gated on the experiment authority and shared variables subsystem**
Decisions: [ADR 0004](../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0005](../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0006](../adr/0006-mcp-ui-equivalence.md),
[ADR 0008](../adr/0008-catalog-templates-instantiate-self-contained-objects.md)

## Outcome

Kagami owns an editable catalog of generic object-template files. Users and MCP
clients can inspect and edit the same catalog and instantiate a template through
the document authority. Each instance is complete experiment data with source
provenance, so opening or submitting the experiment never depends on the local
catalog.

## Source assessment

Use `../field-cad/crates/fieldcad-catalog` and
`../field-cad/docs/tasks/user-configurable-object-catalog.md` as behavioral
references. Preserve the valuable ideas: versioned human-readable entries,
per-entry failure isolation, available/unavailable/invalid states, deterministic
loading, bounded files, source-qualified references, content fingerprints,
atomic writes, conflict detection, and explicit propagation.

Do not migrate the crate wholesale. Its types are coupled to Field CAD's core,
its templates currently contain resolved SI values rather than Kagami's shared
expressions, and it supports document-scoped sources and tracking links that
conflict with ADR 0008's client-owned catalog and snapshot-with-provenance
model. Use
`../field-cad/docs/tasks/server-authoritative-catalog.md` as evidence for one
catalog authority, not as a reason to migrate `fieldcad-server`.

## Implementation slices

### 1. Define the catalog domain and format

- Add a Kagami-owned catalog domain module (extract `crates/kagami-catalog`
  only when the migration admission test justifies the seam). It must not
  depend on UI, MCP transport, a solver, or Orishu runtime code.
- Define a bounded, versioned, human-readable object-template format with
  stable catalog/template identity, metadata, generic component composition,
  template parameters, and property expressions. Start with one template per
  file and deterministic ordering unless evidence requires a more complex
  container.
- Keep instance identity and placement outside reusable template content.
  Component and property schemas, rather than template names, determine what
  can be instantiated and compiled.
- Define source-qualified provenance and a canonical content fingerprint that
  excludes machine-specific absolute paths.

### 2. Load, validate, and write safely

- Discover the configured Kagami catalog directory and load files with
  explicit size, count, parse-depth, and expression-work bounds.
- Isolate failures per entry and expose `Available`, `Unavailable`, and
  `Invalid` states with source-located diagnostics. Reject identity collisions;
  never invent fallback physical values.
- Integrate template parameters and expression-capable properties with
  `orishu-variables`, unit/dimension checks, and the component/property schema
  registry. Unsupported schemas make an entry unavailable rather than changing
  its meaning.
- Resolve template-local definitions in an explicit namespace. Treat instance
  bindings as authored document expressions and define a deterministic identity
  rewrite that copies all required definitions and references into the new
  object's document-local scope.
- Write by safe replacement with source revision/fingerprint conflict checks.
  A failed write leaves both the prior file and in-memory authoritative state
  intact.

### 3. Establish one catalog authority

- Define typed list/get/create/update/delete/validate/reload commands and a
  catalog revision. The authority decides commands and produces file effects;
  UI and MCP are adapters over it, never direct filesystem clients.
- Make duplicate command identities idempotent and allow callers to guard
  changes with the catalog revision or source fingerprint they read.
- Publish bounded catalog-change events/read projections so UI and MCP views
  remain coherent. Filesystem reload changes only this projection and never an
  experiment.

### 4. Instantiate through the document authority

- Add a typed `InstantiateObjectTemplate` document command containing the
  catalog/template identity, expected fingerprint, parameter bindings,
  placement and other instance inputs, actor identity, and optional base
  document revision.
- Resolve names and expressions, check dimensions and component schemas, mint
  the object identity, and commit the complete candidate as exactly one
  document revision and undo entry. Reject stale, unavailable, invalid, or
  partially resolvable candidates without changing the document.
- Persist the complete authored object plus catalog/template/schema/fingerprint
  provenance. Verify save/open and workload compilation without catalog access;
  Orishu must receive only self-contained compiled object data.
- Add an explicit compare/apply-newer-template operation. It presents a diff
  and becomes a normal validated document command; catalog changes never invoke
  it automatically.

### 5. Add UI and MCP parity

- Build a catalog browser/editor that shows availability and diagnostics,
  supports CRUD/reload, previews template parameters and properties, and
  invokes instantiation with explicit bindings and placement.
- Expose equivalent MCP discovery, list/get/CRUD/validate/reload and instantiate
  tools through the same authorities. Catalog edits advance only the catalog
  revision; instantiation advances only the experiment revision.
- Add parity tests proving equivalent UI-path and MCP-path commands produce the
  same catalog and document decisions.

## Acceptance criteria

- Two valid catalog entries load deterministically while an adjacent malformed
  or unsupported entry is isolated with an actionable status and diagnostic.
- Oversized files, excessive expression work, identity collisions, stale
  writes, invalid units, dimension mismatches, and non-finite results fail
  within declared bounds and preserve prior state.
- Templates are generic component/property data; no template name or particle
  species selects solver behavior.
- A template can define a dimensioned parameter and use it in property
  expressions; a binding can itself reference an experiment variable. The
  shared evaluator produces the same result and diagnostic semantics as
  experiment authoring, and the persisted instance no longer resolves through
  catalog state.
- UI and MCP catalog CRUD, discovery, validation, reload, and instantiation use
  the same command types, authority, revisions, and results.
- Instantiation is atomic, creates one stable object identity and one undo
  entry, and persists its complete authored state and source fingerprint.
- Renaming, changing, deleting, or losing the source template does not change
  an existing object. Updating from a template is explicit, reviewable, and
  undoable.
- An experiment containing instantiated objects opens, edits, compiles, and is
  accepted as a workload on a machine with no matching catalog.
- Catalog edits do not dirty the experiment; instantiation and explicit update
  do. Orishu has no dependency on Kagami catalog files or APIs.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Reproducing the complete id Tech `idDeclManager` API or declaration types.
- Solver dispatch by template identity, particle species, or filename.
- Managing installed simulation plugins or executable workload components;
  the object catalog contains reusable authored data only.
- Live links, automatic propagation, or silent mutation of instantiated
  objects when a catalog changes.
- Document-scoped catalogs, remote marketplaces, template inheritance, or
  executable template code in the first version.
- Making Orishu load, edit, or distribute Kagami's authoring catalog.
