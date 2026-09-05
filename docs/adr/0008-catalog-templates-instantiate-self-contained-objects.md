# ADR 0008: Catalog templates instantiate self-contained experiment objects

Status: **accepted**

Direct use of catalog-qualified values is refined by
[ADR 0018](0018-catalog-values-are-captured-by-reference-not-linked.md):
instantiation still materializes self-contained object state, while an author
may separately retain an explicit catalog-variable dependency that is captured
when compiling a workload.

## Context

Scientists repeatedly create objects with the same useful composition and
properties. Kagami therefore needs an editable object catalog: a collection of
human-readable template files that both the UI and MCP clients can inspect and
change. Instantiating a template creates an object in the current experiment.

This resembles id Tech 4's declaration manager and entity definitions: named,
reloadable data declarations supply reusable authoring vocabulary without
hard-coding every kind in the editor. Scientific authoring adds stronger
requirements for dimensions, reproducibility, invalid-data handling, and
document portability.

Field CAD's `docs/adr/0019-generic-particle-catalog-is-data.md` established the
central lesson: a catalog entry describes generic component data, not a
particle class or hidden solver dispatch. Its later catalog work also
demonstrated isolated load failures, explicit availability, content
fingerprints, and provenance. Kagami retains those lessons while deliberately
dropping Field CAD's tracking/propagation relationship and integrating its
shared expression language and command-authority boundaries.

## Decision

### The catalog is client-owned data with one authority

Kagami owns an editable, versioned collection of object-template files outside
the experiment document. A catalog authority owns the loaded registry,
revision, validation diagnostics, and file effects. UI and MCP adapters submit
the same typed catalog commands to that authority and consume its read model;
neither adapter writes catalog files or maintains a competing registry.

Catalog edits do not dirty the experiment. Catalog files are a property of the
Kagami installation/profile, not Orishu workload resources and not embedded
catalog entries in an experiment document.

### Templates describe composition and authored properties

A template is a named, schema-versioned composition of components and authored
properties. Expression-capable properties and template parameters use the
shared variables and expressions subsystem from ADRs 0005 and 0007, including
units, dimensions, retained source, limits, and diagnostics.

Template-local definitions live in an explicit template namespace. Instance
bindings may themselves be document expressions. On acceptance, Kagami rewrites
and copies every definition and property expression needed by the instance into
stable document/object-local identities, so retained intent does not continue
to resolve through mutable catalog state.

Templates do not introduce solver-facing species enums, executable behavior,
or branches such as “if electron”. Physics and workload compilation inspect
the resulting components and properties through their schemas. Instance
identity, placement, and other invocation-specific inputs are supplied by the
instantiation command rather than hidden in the reusable template.

### Instantiation is an ordinary document command

An `InstantiateObjectTemplate` proposal identifies a template and content
fingerprint, supplies parameter bindings and instance inputs, and enters the
document authority from ADR 0004. The authority resolves and dimension-checks
the complete candidate, mints an object identity, and either adds the object as
one experiment revision and undo entry or rejects the proposal atomically.

The accepted object persists all authored component/property state and the
transitive template definitions needed to use that object without the source
catalog. It also
persists provenance sufficient to identify the source catalog, template,
schema version, and content fingerprint. That provenance is not a live pointer.
Editing, reloading, removing, or losing a template never changes existing
objects. Kagami does not retain Field CAD's template-tracking or propagation
relationship; creating from a changed template is a new instantiation.

Orishu receives the self-contained object state produced during workload
compilation. It does not load or interpret Kagami's catalog.

The catalog is not Kagami's simulation-plugin inventory. Object templates
reuse authored component/property data and cannot select executable physics;
simulation plugins explicitly provide physical-model schemas and pinned
workload components. See [Simulation plugins](../simulation-plugins.md).

### Failure is explicit and isolated

Catalog parsing and expression validation are bounded. Each entry is reported
as available, unavailable because a dependency or schema is unsupported, or
invalid with structured diagnostics. One malformed entry does not prevent
other entries or Kagami itself from loading. Duplicate identities and source
write conflicts are errors. Kagami never substitutes fabricated scientific
defaults for an invalid template.

Catalog writes use safe replacement and conflict detection. Reload replaces
only the catalog projection and never mutates an experiment.

## Consequences

- Scientists and agents gain one discoverable, editable vocabulary for making
  objects, with UI/MCP parity.
- Saved experiments and submitted workloads remain reproducible when moved to
  a machine with a different or missing catalog.
- Generic component schemas, not template names, determine simulation
  behavior.
- Catalog and document revisions are distinct; an instantiation command uses
  one immutable catalog snapshot and creates one document revision.
- Kagami needs a catalog domain module, file loader/writer, authority, UI, MCP
  tools, and an explicit instantiation bridge to the document authority.
- Live inheritance, silent propagation, and treating the catalog as an Orishu
  runtime registry are intentionally excluded.
- Installing a simulation plugin may make new component schemas available to
  templates, but a template remains data and never becomes an executable
  extension.
