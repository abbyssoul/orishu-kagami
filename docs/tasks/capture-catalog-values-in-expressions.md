# Integrate catalog variables and capture them into workloads

Status: **gated on the catalog, shared variables, document, and workload contracts**

Decisions: [ADR 0005](../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0007](../adr/0007-share-expression-semantics-with-workload-resources.md),
[ADR 0008](../adr/0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0010](../adr/0010-content-addressed-workload-closure-and-portable-bundles.md),
and [ADR 0018](../adr/0018-catalog-values-are-captured-by-reference-not-linked.md)

User story: [Reuse a catalog value in an expression](../user-stories/kagami/authoring.md#reuse-a-catalog-value-in-an-expression)

## Outcome

Catalog template properties participate in Kagami's shared authoring variable
environment under stable, visibility-aware namespaces. A document may retain a
qualified reference such as `planets.sun.mass / 2`. When Kagami compiles an
exact document revision, it captures and rewrites the complete transitive
catalog-variable closure into the immutable workload so Orishu needs no catalog
files or catalog-aware evaluator.

This is distinct from template instantiation. Instantiation materializes an
object and its dependencies into document-local state under ADR 0008; this task
handles deliberately retained catalog-qualified expressions.

## Dependencies

- [Implement the Kagami object catalog](implement-kagami-object-catalog.md):
  catalog authority, stable identities, availability states, immutable
  snapshots, fingerprints, and the variable-binding projection.
- [Migrate and integrate the shared variables subsystem](migrate-and-integrate-variables-subsystem.md):
  namespaces, visibility, stable bindings, dimensions, dependency closure,
  limits, and resource-independent resolution hooks.
- K-DOCUMENT: persisted expression source and atomic document commands. This
  task must not make a catalog reload a document mutation.
- [Define and adopt the shared workload format](define-and-adopt-shared-workload-format.md):
  workload-local variable identities, source-bearing definitions, canonical
  resolved values, fingerprints, and closure validation.

Do not add catalog-specific behavior to Orishu admission or runtime. The shared
expression library may expose generic namespace/resolver facilities, but it
must not depend on Kagami catalog types.

## Implementation slices

### 1. Publish catalog bindings

- Project every structurally valid expression-capable template property into
  the variable environment using canonical catalog, template,
  plugin/component, and property identities.
- Apply the format's public/private namespace rules. Properties require no
  separate export list; public is the default for property bindings and
  template-local helper variables are private by default.
- Permit concise source spelling only when unambiguous. Retain enough resolved
  identity information to survive display-name ambiguity without silently
  selecting another binding.
- Represent missing catalogs, private dependencies, and unknown plugin schemas
  as structured unavailable bindings. Parse/cycle/dimension/non-finite failures
  remain invalid. Preserve unaffected bindings and entries.

### 2. Resolve retained document references

- Let document expressions refer to public catalog bindings through the shared
  variables API. Cross-catalog dependencies are allowed when every binding is
  visible and available.
- Preserve catalog-qualified authored source in the document; do not eagerly
  replace it with document variables on acceptance.
- Re-evaluate derived previews against catalog revision changes without
  mutating the document or adding undo history. Surface the effective catalog
  revision, source fingerprints, and unavailable dependency chain.
- Save/open must preserve unresolved references losslessly. Validation and
  compilation remain unavailable until their dependencies return.

### 3. Capture one immutable compilation snapshot

- Compile an exact document revision against one immutable catalog snapshot.
  Detect and reject a mixed-revision/changed-source capture rather than racing
  catalog reload or file replacement.
- Walk the complete transitive closure of every referenced catalog binding,
  across catalog files where applicable, under declared count, depth, source,
  and evaluation-work limits.
- Reject the complete compilation for missing/private/unavailable/invalid
  bindings, cycles, dimension failures, non-finite values, unsupported
  language/unit versions, or exceeded limits.

### 4. Lower into the workload variable graph

- Mint deterministic workload-local identities for captured definitions and
  rewrite all catalog-qualified references to them. No executable workload
  expression may retain a Kagami catalog path.
- Preserve expression source after rewriting, dimensions, language/unit
  versions, canonical resolved values, source identities, and content
  fingerprints as required by ADRs 0007, 0010, and the workload schema.
- Deduplicate identical reachable bindings within one compilation without
  merging distinct source identities accidentally. Canonical ordering must
  make repeated compilation against the same document/catalog snapshots
  produce identical bytes and workload identity.
- Feed the lowered graph through the same Orishu workload validation path used
  by non-Kagami workload producers.

### 5. Add UI and MCP parity

- UI and MCP expression editing use the same catalog namespace resolver,
  visibility decisions, availability diagnostics, and document commands.
- Both surfaces show which catalog revision and fingerprints a successful
  compilation captured, and distinguish a materialized object from a retained
  catalog-variable dependency.
- Neither adapter reads catalog files directly or constructs a workload-only
  rewrite independently.

## Acceptance criteria

- `planets.sun.mass / 2` resolves when its public canonical binding and plugin
  schema are available, with the expected dimension and value.
- Private bindings are usable inside their permitted namespace and rejected
  outside it. Ambiguous concise names require a canonical qualification.
- A missing referenced catalog, cross-catalog dependency, or plugin schema
  makes only affected bindings/templates unavailable and reports the complete
  dependency chain; unrelated catalog entries remain usable.
- Saving and reopening preserves an unavailable catalog-qualified expression
  exactly, without inventing a value. Restoring its dependencies makes it
  resolvable again without rewriting the document.
- Catalog reload changes derived authoring previews but neither the document
  revision nor existing materialized objects. A workload compiled earlier is
  unchanged.
- Compilation captures all and only reachable catalog definitions from one
  immutable snapshot, rewrites every reference to workload-local identities,
  and is deterministic for identical inputs.
- Orishu accepts the compiled manifest with no catalog installed and obtains
  the same canonical resolved parameters through the shared evaluator.
- Missing, private, unavailable, invalid, cyclic, dimensionally wrong,
  non-finite, oversized, or concurrently changed inputs reject compilation
  atomically.
- UI and MCP paths produce equivalent resolution, diagnostics, compiled bytes,
  and captured-source reports.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Field CAD tracking links, compare/apply propagation, or automatic mutation
  of materialized objects.
- Eagerly copying every catalog reference into the document.
- Requiring every catalog file to be independently closed.
- Catalog loading or catalog-path resolution in Orishu.
- Selecting physics from catalog/template names or particle species.
