# 0018 — Catalog template properties are authoring variables captured into workloads

Status: **proposed**  
Decisions: [ADR 0005](0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0007](0007-share-expression-semantics-with-workload-resources.md),
[ADR 0008](0008-catalog-templates-instantiate-self-contained-objects.md), and
[ADR 0010](0010-content-addressed-workload-closure-and-portable-bundles.md)

## Context

Kagami models an experiment as objects with stable identities and named
components. In ECS terms, an object is an entity and simulation plugins
contribute the component/property schemas that may be attached to it. A
catalog is a collection of human-readable object-template files: reusable
component/property compositions similar in purpose to id Tech 4 entity
definitions. A template is data and never selects executable physics.

Field CAD demonstrated this shape with YAML `ObjectTemplate` documents such as
`etc/catalogs/planets.yaml`. It also demonstrated three useful load states:
available, structurally valid but unavailable under the installed component
schemas, and invalid. Its explicit tracking link from an instantiated object
back to a template is not carried forward. ADR 0008 instead requires
instantiation to materialize complete object state without inheritance or
automatic propagation.

Template property values are also useful independently of instantiation. A
researcher may write `new_object.mass = planets.sun.mass / 2`, where the
qualified name denotes the mass property contributed by a component on the
`sun` template in the `planets` catalog. These values belong in the same
dimension-aware variable graph as document variables; a separate catalog
expression engine would create incompatible name, unit, and diagnostic rules.

Catalogs are client-owned file resources. Users can copy them between Kagami
installations, but installations may have different catalogs or simulation
plugins. Orishu never loads a Kagami catalog. Therefore a draft may have an
explicit authoring dependency on catalog files, while every submitted workload
must contain a closed, immutable capture of all catalog values it uses.

## Decision

### Catalog files define generic ECS entity templates

The Kagami catalog system loads a configured collection of bounded, versioned,
human-readable files. A file may contain one or more templates. Each template
has a stable catalog/template identity, metadata, and a composition of
plugin-qualified components and their authored properties. Instance identity,
name, placement, and other invocation-specific state are not template data.

Component and property identities come from simulation-plugin schemas. The
catalog preserves an entry whose plugin, component, or property schema is not
installed, but marks it `Unavailable` with structured reasons and does not
instantiate it. Loading the required plugin causes normal revalidation and may
make the entry available; it never mutates an existing experiment object.
Malformed files, invalid names, duplicate identities, invalid expression
syntax, cycles, dimension errors, and non-finite values are `Invalid`.

One bad or unavailable entry does not prevent other entries or Kagami itself
from loading. Unknown data is never replaced with a fabricated scientific
default.

### Template properties participate in the shared variable environment

Every expression-capable value property of a structurally valid template is
projected into the shared variables subsystem under a catalog/template
namespace. Its canonical identity includes the catalog, template,
plugin-qualified component, and property identity. A concise spelling such as
`planets.sun.mass` is permitted only when it resolves unambiguously; persisted
resolution uses stable canonical identities rather than display names.

There is no separate export list. Value properties are bindings by virtue of
being template properties. Namespace visibility provides the boundary:
properties are public by default for simple shareable catalogs and may be
declared private; private bindings are visible only within their declared
catalog/template scope. Template-local helper variables are private by default.
The catalog format and variable subsystem use the same explicit public/private
rules for UI and MCP callers.

Property expressions may refer to other visible variables, including
qualified values from another loaded catalog. Editable files cannot be assumed
closed or valid. A missing or private dependency leaves the affected binding
and template unavailable with a diagnostic; it does not prevent the file from
being inspected, corrected, copied, or removed. Cycles and expressions that
resolve with the wrong dimension remain invalid. Consequently, copying one
catalog file may also require copying the catalog files on which it declares
dependencies.

Bindings backed by an unknown plugin/property schema remain visible as
unavailable symbols for diagnostics, but cannot supply a typed value until the
schema is installed and the entry revalidates.

### Instantiation materializes an object and creates no template link

Instantiating an available template is one document-authority command. Kagami
resolves the template against one immutable catalog snapshot, validates its
complete transitive variable closure and component schemas, mints an object
identity, and copies the resulting component/property expressions and required
definitions into document/object-local identities. The accepted object is a
materialized ECS entity, not an instance of a runtime template class.

The object may retain historical source provenance and the catalog/template
fingerprint used by the command, but there is no tracking relationship, no
catalog-instance index, and no compare/apply propagation protocol. Editing,
reloading, renaming, or deleting a template never changes a materialized
object. Creating another object from the changed template is a new
instantiation.

### Direct catalog references remain authoring dependencies

An author may deliberately use a public catalog-qualified binding in a
document variable or property expression without instantiating the template.
Unlike instantiation, accepting this expression does not eagerly copy the
catalog binding into the document or pretend the document is catalog-free.
The authored source retains the qualified reference and evaluates against the
current catalog authority projection.

Catalog reload can therefore change the derived preview of such a reference or
make it unavailable without mutating the document or creating a document
revision. Kagami reports the catalog revision, resolved source fingerprints,
and any unavailable dependency so the researcher can see what an eventual
submission would capture. A saved document preserves the expression even when
the referenced catalog is absent, but it cannot be fully validated or compiled
until all public bindings in its dependency graph are available.

This is an ordinary, explicit variable dependency, not Field CAD's special
instance-template tracking link. An instantiated object is catalog-independent;
a document expression that explicitly names `planets.sun.mass` is not.

### Workload compilation captures and rewrites the complete closure

Compiling an exact document revision reads one immutable catalog snapshot and
computes the complete transitive closure of every catalog-qualified binding
reachable from the document. Compilation fails atomically if a binding is
missing, private to another scope, unavailable, invalid, cyclic, dimensionally
wrong, over resource limits, or changes while the snapshot is being acquired.

Kagami copies the source-bearing definitions, dimensions, language/unit
versions, canonical resolved values, stable source identities, and content
fingerprints into workload-local identities. It rewrites document expressions
to those workload-local identities. The immutable workload manifest and its
content-addressed closure therefore contain no catalog path that Orishu must
resolve and no dependency on a Kagami installation.

Orishu independently parses, bounds, dimension-checks, and resolves that
self-contained workload graph under ADR 0007. Workload identity covers both
the captured expression sources and canonical resolved parameter fingerprint,
not only a numeric result. A later catalog edit can affect a later compilation,
but never an accepted or running workload.

## Consequences

- Catalog files provide both reusable entity templates and qualified physical
  values through one shared expression system.
- Catalog availability depends on installed plugin schemas and the complete
  visible variable graph. Missing dependencies are actionable catalog
  diagnostics rather than load-process failures.
- Instantiation is snapshot materialization without inheritance. Explicit
  catalog-qualified expressions remain authoring dependencies until workload
  compilation; the UI must make that distinction visible.
- Sharing a materialized object needs only the experiment document. Sharing a
  draft that retains catalog-qualified expressions also requires the referenced
  catalog files and compatible plugin schemas. Sharing a compiled portable
  workload requires neither.
- Two clients may resolve the same draft differently when their effective
  catalogs differ. Exact workload compilation records the source fingerprints
  it actually captured, making the submitted computation reproducible.
- Kagami needs a catalog-domain crate and adapters to the plugin-schema,
  variables, document-authority, and workload-compilation boundaries. Orishu
  needs no catalog crate or catalog protocol.

## Non-goals

- Field CAD tracking links, automatic template propagation, or an
  instance-to-template synchronization protocol.
- Making catalog identity, template names, or particle species select solver
  behavior.
- Loading, editing, or distributing Kagami catalogs through Orishu.
- Silently accepting unknown components, unresolved variables, private
  references, invalid dimensions, or fabricated defaults.
- Requiring each catalog file to be self-contained; dependencies are allowed
  but must resolve before use and are captured transitively into workloads.

Status: proposed.
