# Integrate document variables and expressions

Status: **specified**; gated on [K1](implement-experiment-document-model.md)
and [S-VARIABLES](../migrate-and-integrate-variables-subsystem.md) slice 2  
Work package: **K-DOCUMENT** × **S-VARIABLES** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0005](../../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0007](../../adr/0007-share-expression-semantics-with-workload-resources.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md)  
Stories: [Define variables and use expressions](../../user-stories/kagami/authoring.md)

## Outcome

An experiment can define named variables and author any expression-capable
property in terms of them: `mass_of_sun = 1.989e30 kg`, then
`earth.orbit.radius = 1 au`, then a mass authored as `mass_of_sun / 2`. A
plugin-exported constant such as gravitational `G` is visible the same way.
Editing one definition updates every dependant as one atomic revision and one
undo entry, or is rejected whole.

This is **S-VARIABLES slice 3 ("integrate with the experiment authority")
landed against K1's model**. It exists as a separate Kagami task because its
acceptance is document behaviour, not engine behaviour; the engine work stays
in [the variables task](../migrate-and-integrate-variables-subsystem.md) and is
not duplicated here.

## Owning boundary

Owns the document's variable graph, the projection of expression-capable
properties into it, and the atomic closure evaluation that decides an edit.
Does **not** own the parser, the evaluator, the unit catalog, or dimension
inference — those are `orishu-variables`, and a second implementation of any
of them is a defect.

## Consumed contracts

- `orishu_variables::{VariablesSystem, Namespace, Name, FQName, VariableId}`
  plus the dimension-aware value layer from S-VARIABLES slice 2. **This task
  cannot start before that layer exists**: without it, dimensions are
  *declared* rather than derived, and `mass / radius` declared as `kg` would be
  accepted (the limitation `kagami-catalog`'s crate documentation already
  records).
- K1's `Experiment`, `ExperimentCommand`, `update`, `Rejection`, and `Limits`.

## Source assessment

- `../field-cad/crates/fieldcad-expressions` is the behavioural reference for
  retained source, dependency diagnostics, source spans, candidate evaluation
  and property-schema checks. Read it for the problems it solved; do not port
  it, and do not create a second parser
  ([migration.md](../../migration.md)).
- `../field-cad/docs/adr/0026-authored-expressions-evaluate-before-ticks.md`
  is explicitly **superseded** here. Field CAD kept an `ExpressionDocument`
  beside the world and stored only resolved SI numbers in the world; ADR 0005
  makes the retained source the persisted intent, so it lives on the property
  (ADR 0019). Its live-binding half — reading distance probes before each
  tick — is a run concern and is not adopted at all.
- `crates/kagami-catalog/src/{binding,materialize}.rs` shows the projection and
  capture patterns already accepted in this repository: canonical identities,
  public/private visibility, and rewriting references into local identities.

## Implementation slices

### 1. Variable definitions as document intent

- A definition has a stable document-local identity independent of its
  editable name, an explicit namespace, the authored expression source, an
  optional description, and optional provenance.
- Add `DefineVariable`, `SetVariableExpression`, `RenameVariable`,
  `RemoveVariable`, and `SetVariableDescription` to `ExperimentCommand`.
- Persisted identity is the stable handle, never the display name, so a rename
  is presentation work over an unchanged graph.

### 2. Compile and evaluate the affected closure

- Compile the document's definitions and expression-capable property sources
  into one dependency graph over `VariablesSystem`.
- On every edit, evaluate the complete *affected closure* into candidate state
  and accept only if every affected variable and property is valid. On
  rejection, the last valid graph and every resolved value are preserved
  exactly.
- Resolved values are a derived projection, cached for interactive speed and
  invalidated from dependency changes. They are never authoritative and are
  never what a save writes as intent.
- Expose deterministic evaluation-work counters in tests so "affected closure"
  is proved by visited definitions/expressions, not by a wall-clock benchmark.

### 3. Refactoring and referential safety

- One accepted variable edit is one revision and one undo entry, however many
  dependants it repriced.
- Renaming a referenced variable rewrites every referring source as one
  validated refactoring, or is rejected whole.
- Removing a referenced variable is rejected with its dependants identified,
  unless the same batch clears those references.
- Cycles, unknown names, ambiguous names, dimension mismatches, division by
  zero, non-finite results, and limit breaches report with stable source spans
  where the failure belongs to source text.

### 4. Namespaces for plugin and catalog symbols

- Document variables, plugin-exported constants, and catalog-qualified
  bindings share one namespace resolution, so a user writes `G` or
  `planets.sun.mass` in the same expression language.
- A catalog-qualified reference stays a live authoring dependency and is *not*
  eagerly copied into the document (ADR 0018); reporting what an eventual
  submission would capture is
  [K5](instantiate-catalog-templates.md) and
  [capture-catalog-values-in-expressions](../capture-catalog-values-in-expressions.md).
- Visibility rules are the catalog's public/private rules, not a second set.

## Acceptance criteria

- `1e32 kg`, `2.7 g / cm^3`, and `1 / 3 + 0.1` resolve deterministically;
  `1 kg + 1 m` is rejected; a mass expression assigned to a length property is
  rejected; NaN and infinity are never adopted.
- Redefining a variable updates every dependant property in the same revision;
  a rejected edit leaves revision, definitions, resolved values and history
  unchanged.
- A rename preserves stable identities and every dependant's meaning; removing
  a referenced variable is refused with its dependants named.
- A cycle, an unknown name, and an over-limit graph each fail structurally
  without partially changing state.
- Evaluating the affected closure — not the whole document — is what an edit
  costs, demonstrated by deterministic work-count evidence while the
  unaffected part of the graph grows.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- A second expression parser, evaluator, or unit catalog.
- Symbolic algebra, equation solving, or arbitrary user code.
- Binding a variable to a live run observation; adopting a computed value is an
  explicit command with run provenance (ADR 0004).
- Workload-side resolution and freezing — S-VARIABLES slice 4.
- Variable editor widgets or MCP tools; those consume these commands.
