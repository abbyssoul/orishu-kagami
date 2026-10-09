# Catalog template v3: scoped component provider intent

Status: implemented format and authority bridge. The [Unix existing-component
choice form](component-provider-choices.md) also provides guarded lock adoption.
Native removal now uses explicit pruning batches; first unavailable attachment
and new locked-document roots use guarded resolution/adoption. Native exact Add
also captures choices in previously unlocked uncaptured documents. Explicit
uncaptured replacement now accepts detach/new values/scoped lock atomically.
General compound migration adapters and
choice MCP remain X-PLUGIN work.

This extends [catalog templates](../crates/kagami-catalog/README.md) and
[ADR 0008](adr/0008-catalog-templates-instantiate-self-contained-objects.md).
The [shared authoring lock](plugin-contract-v1-draft.md#standalone-authoring-dependency-lock)
is provider intent, not executable bytes or evidence that installed declarations
are available and compatible.

## Format and compatibility

The existing YAML resource envelope has `apiVersion: kagami.catalog/v3`,
`kind: ObjectTemplate`, unchanged `metadata`, and the existing `spec` fields
`parameters`, `helpers`, and `components`. V3 additionally requires
`spec.dependencies`: the structured `orishu.plugin-authoring-lock/v1` envelope,
containing `apiVersion` and `selection` (`roots`, `contributions`, `bindings`).
The shared lock's camelCase spelling is retained inside that field. It is not
an opaque string, external filename, catalog reference or installed inventory ID.

- Roots are exactly the sorted unique exact component contributions in this
  template. Legacy logical components are not silently resolved or added as roots.
- Members and bindings contain the complete transitive dependency closure for
  those roots. Every member is reachable; endpoints, sort order, duplicate slots,
  acyclicity, depth, counts, graph work and canonical byte weight are checked.
- V3 requires the field even when the graph is explicitly empty. Null, missing,
  unknown-version and malformed locks refuse; none becomes an unlocked template.
- V1/v2 reject dependency metadata. V1 remains the logical-reference format;
  v2 remains the exact-pin/explicit-alias format without standalone dependencies.
  Their canonical template bytes and fingerprints are unchanged.
- Writers choose v3 when a lock is present, including an empty lock. The lock
  participates in canonical template bytes and therefore the content fingerprint.
  Provider changes invalidate a caller's expected template fingerprint.
- New instantiation provenance records the canonical template version, including
  v2 (previously incorrectly labeled v1). Existing provenance is historical data
  and is not rewritten or used to select a provider.

## Bounds and availability

Embedded lock deserialization uses the shared bounded seed, with the default
`LockLimits` as an absolute serde ceiling. Catalog receiving limits may tighten
it. The catalog loader preflights the lock under those receiving limits before
the full typed document is constructed. `parse_stream` also checks the file-byte
ceiling for in-memory callers before invoking the YAML parser.

The enclosing catalog parser still creates a YAML value tree before the lock
preflight; this is not a claim of streaming, allocation-free YAML ingestion.
The shared seed bounds subsequent collection reads, depth and value work and
refuses a rejected next entry without invoking its value deserializer.

Structural validity and schema availability remain separate. Missing component
schemas preserve a structurally valid template and its choices as unavailable.
The catalog's schema-only availability result does **not** verify the dependency
closure against an installed plugin inventory. Before computational use, the
plugin adapter must strictly revalidate through the inventory resolver, and
workload compilation/admission must independently verify selected declarations.
No code is executed merely by opening or instantiating a template.

## Materialization and document composition

`ObjectCandidate` retains the immutable template-scoped lock alongside the
self-contained component data and copied expression definitions. The document
authority combines it with existing intent and submits object creation and
`AdoptDependencies` in one ordinary validated batch. Revision, identities,
undo/history, events, receipts and aggregate lock retention are admitted atomically.

`SelectionLock::merge` unions roots, members and bindings. For every exact
consumer common to both graphs, the complete outgoing bindings must agree.
Different providers **or an edge missing on one side** produce a structured
conflict with the consumer/slot and both optional provider choices. There is no
last-writer-wins replacement and no silent completion of an incomplete graph.
An unchanged union reuses the existing document lock handle and retention weight.

Final document validation checks all resulting component roots and agreement
with any captured scientific selection. A locked template cannot invent choices
for unrelated pre-existing unlocked exact components. Resolve that document
explicitly first. Likewise, adding a new exact root from an unlocked v2 template
to a locked document refuses unless a complete matching lock is supplied through
an explicit authoring batch; it does not discard the existing lock.

`SelectionLock::for_roots` explicitly extracts reachable closure from sorted,
unique member roots, for template extraction or root removal. It preserves shared
providers still reachable from retained roots. Unknown roots refuse; empty roots
produce an explicit empty lock. This helper does not perform automatic migration
or change a document: adapters submit its result with the root edit in the same
command batch. Native object/component removal now does this, including required
history regeneration. New locked roots use guarded resolution/adoption; explicit
uncaptured replacement restricts surviving intent before atomic replacement and
lock adoption, without rebinding shared providers or inferring value conversion.
Ordinary native component attachment after capture now uses consented guarded
evidence/history extension. Captured catalog/compound addition preparation now
uses that guarded backend with the shared session materializer, exact fingerprints
and complete provider locks; fields survive and copied definitions/history adopt
atomically. The native single-template picker now supplies bounded instance inputs,
explicit provider choices and history consent. General migration planning, catalog
file editing and MCP/headless adapters remain pending; template instantiation is
not ordinary attachment.

Accepted objects need no live catalog link. The merged lock persists through
[experiment container v5](experiment-container-v5.md), undo/redo and dependency-only
release-reference tracking. No workload, kernel ABI or worker protocol version
changes here; workers never load these catalog files.
