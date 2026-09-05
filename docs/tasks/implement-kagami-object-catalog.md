# Implement the Kagami object catalog

Status: **slices 1–3 and the slice 4 pure core are implemented and accepted in
`crates/kagami-catalog`; document, UI, MCP, and workload integration remain
gated**
Decisions: [ADR 0004](../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0005](../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0006](../adr/0006-mcp-ui-equivalence.md),
[ADR 0008](../adr/0008-catalog-templates-instantiate-self-contained-objects.md),
and [ADR 0018](../adr/0018-catalog-values-are-captured-by-reference-not-linked.md)

## Outcome

Kagami owns an editable catalog of generic ECS entity-template files. Users and
MCP clients inspect and edit the same catalog and instantiate a template through
the document authority. Each instance is materialized experiment data without
a tracking link. Template properties also form a visibility-aware projection
into the shared variables subsystem; capturing retained catalog references into
workloads is handled by the separate catalog-variable integration task.

## Source assessment

Use the sibling Field CAD repository's `crates/fieldcad-catalog`,
`etc/catalogs/planets.yaml`, ADR 0019, and catalog tasks as behavioral
references. Preserve the valuable ideas: versioned human-readable entries,
per-entry failure isolation, available/unavailable/invalid states, deterministic
loading, bounded files, source-qualified references, content fingerprints,
atomic writes, and conflict detection.

Do not migrate the crate wholesale. Its types are coupled to Field CAD's core,
its templates currently contain resolved SI values rather than Kagami's shared
expressions, and it supports document-scoped sources and tracking links that
conflict with ADR 0008's client-owned catalog and materialization model. Use
Field CAD's `server-authoritative-catalog.md` as evidence for one catalog
authority, not as a reason to migrate `fieldcad-server` or its propagation
workflow.

## Current state

`crates/kagami-catalog` implements the catalog domain, format, bounded loader,
variable projection, guarded writer, catalog authority, and the pure
materialization core. `etc/catalogs` ships `planets.yaml`, `particles.yaml`,
and `scenarios.yaml`, and the crate's integration tests load those exact files.

Independent review on 2026-09-05 confirmed the architecture and broad test
coverage but found two acceptance blockers: an update could rename onto another
entry's identity before collision detection, and lexical path validation did
not contain writes through symlinked ancestors or a pre-existing temporary
path. The bounded
[authority-boundary correction](fix-kagami-catalog-authority-boundaries.md)
fixed identity ownership and rename events and added the physical-path checks.
A follow-up review reproduced one remaining escape — creating a missing
descendant below a symlink created an outside directory before the command was
rejected — which the same task corrected by proving each path component before
creating anything below it. The core is accepted.

Two things named by the slices below are deliberately not implemented here and
belong to their respective tracked tasks:

- **Dimension inference through expression arithmetic.** Authored quantities
  declare a unit beside the expression (`{expression: "6.9634e5", unit: km}`)
  and the catalog checks that unit against the property schema's declared
  dimension. Unit-bearing literals (`1e32 kg`) and dimensions derived through
  multiplication, division, and powers are slice 2 of
  [the variables subsystem task](migrate-and-integrate-variables-subsystem.md),
  which owns the one dimension-aware layer over the shared grammar. Forking a
  second one into the catalog is explicitly what ADR 0018 warns against. Until
  it lands, an expression that references another binding may only carry a
  canonical unit, because bindings are published in canonical SI and a scaling
  factor would rescale an already-canonical magnitude.
- **The `InstantiateObjectTemplate` document command, UI, and MCP surfaces.**
  `materialize` produces the complete `ObjectCandidate` such a command would
  commit — resolved values, copied definitions, rewritten expressions, and
  provenance — but there is no document authority in this repository yet to
  commit it, and no UI or MCP adapter to submit through.

## Implementation slices

### 1. Define the catalog domain and format

- Add `crates/kagami-catalog` as a Kagami-owned, independently testable catalog
  domain crate. Salvage suitable parsing, validation, availability, source,
  fingerprint, and fixture concepts from `fieldcad-catalog`, but design its
  public types for this repository. It must not depend on UI, MCP transport, a
  solver, Orishu runtime code, Field CAD core types, or tracking links.
- Define a bounded, versioned, human-readable object-template format with
  stable catalog/template identity, metadata, generic component composition,
  template parameters, optional private helpers, visibility, and property
  expressions. Support the Field CAD-proven one-or-more YAML documents per file
  and deterministic ordering; do not assume that one catalog identity equals
  one filesystem file.
- Keep instance identity and placement outside reusable template content.
  Component and property schemas, rather than template names, determine what
  can be instantiated and compiled.
- Define source-qualified provenance and a canonical content fingerprint that
  excludes machine-specific absolute paths.
- Define the read-only variable-binding projection required by ADR 0018: every
  expression-capable property has a canonical catalog/template/component/
  property identity, properties are public by default unless marked private,
  and helper variables are private by default.

### 2. Load, validate, and write safely

- Discover the configured Kagami catalog directory and load files with
  explicit size, count, parse-depth, and expression-work bounds.
- Isolate failures per entry and expose `Available`, `Unavailable`, and
  `Invalid` states with source-located diagnostics. Reject identity collisions;
  never invent fallback physical values.
- Integrate expression-capable properties and helpers with
  `orishu-variables`, unit/dimension checks, and the component/property schema
  registry. Unsupported schemas make an entry unavailable rather than changing
  its meaning.
- Resolve visible cross-catalog dependencies when present. Missing/private
  dependencies make affected entries unavailable; syntax, cycle, dimension,
  and non-finite failures make them invalid. Preserve entries so users can
  inspect and repair copied files or install a missing plugin later.
- Resolve template-local definitions in an explicit namespace and define a
  deterministic identity rewrite that copies all definitions required by an
  instantiation into the new object's document-local scope.
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
- Retain historical source identity/fingerprint provenance, but create no
  tracking link, propagation index, compare/apply operation, or automatic
  update path. A changed template is used only by a later instantiation.

### 5. Add UI and MCP parity

- Build a catalog browser/editor that shows availability, missing
  variable/plugin dependencies, and diagnostics; supports CRUD/reload; previews
  parameters, components, and properties; and invokes instantiation with
  explicit bindings and placement.
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
- A template can define a dimensioned parameter, property expressions, and
  private helpers, and can refer to a visible binding from another loaded
  catalog. An instance binding may itself reference a document variable. The
  shared evaluator produces the same result and diagnostic semantics as
  experiment authoring, and the persisted instance no longer resolves through
  catalog state.
- Each expression-capable property appears in the catalog variable projection
  at its canonical identity. Visibility and ambiguous-short-name tests prevent
  private or unintended bindings from resolving externally.
- UI and MCP catalog CRUD, discovery, validation, reload, and instantiation use
  the same command types, authority, revisions, and results.
- Instantiation is atomic, creates one stable object identity and one undo
  entry, and persists its complete authored state and source fingerprint.
- Renaming, changing, deleting, or losing the source template does not change
  an existing object. There is no template propagation operation.
- An experiment containing instantiated objects opens, edits, compiles, and is
  accepted as a workload on a machine with no matching catalog.
- Catalog edits do not dirty the experiment; instantiation does. Orishu has no
  dependency on Kagami catalog files or APIs.
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
- Capturing catalog-qualified document references into workloads; that belongs
  to [the catalog-variable integration task](capture-catalog-values-in-expressions.md).
