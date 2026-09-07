# Integrate document variables and expressions

Status: **slices 1–3 implemented** in `crates/kagami-document`; slice 4's
document half (namespaces) is landed, and its plugin and catalog *sources*
wait on X-PLUGIN and [K5](instantiate-catalog-templates.md)  
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
  plus the dimension-aware value layer from S-VARIABLES slice 2, which has
  landed: evaluation is `Quantity`-valued, so `2.7 g / cm^3` derives a density
  and `1 kg + 1 m` is refused. What remains for this task is the *document*
  side — an expression with no units of its own is still read in whatever
  dimension the property's schema declares, so a dimensionless ratio assigned
  to a mass is accepted until document variables give it a real dimension.
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

### 1. Variable definitions as document intent — **implemented**

- A definition has a stable document-local identity independent of its
  editable name, an explicit namespace, the authored expression source, an
  optional description, and optional provenance.
- Add `DefineVariable`, `SetVariableExpression`, `RenameVariable`,
  `RemoveVariable`, and `SetVariableDescription` to `ExperimentCommand`.
- Persisted identity is the stable handle, never the display name, so a rename
  is presentation work over an unchanged graph.

### 2. Compile and evaluate the affected closure — **implemented**

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

### 3. Refactoring and referential safety — **implemented**

- One accepted variable edit is one revision and one undo entry, however many
  dependants it repriced.
- Renaming a referenced variable rewrites every referring source as one
  validated refactoring, or is rejected whole.
- Removing a referenced variable is rejected with its dependants identified,
  unless the same batch clears those references.
- Cycles, unknown names, ambiguous names, dimension mismatches, division by
  zero, non-finite results, and limit breaches report with stable source spans
  where the failure belongs to source text.

### 4. Namespaces for plugin and catalog symbols — **partially implemented**

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

## Implementation record

Landed in `crates/kagami-document` as the `variable` module plus a two-phase
transition, taking the crate from 107 to 134 tests (19 of them K2 acceptance
cases in `tests/variables.rs`).

Slice 4's *resolution rule* is in place — definitions live in namespaces, a
qualified name is what an expression writes, and the root namespace is where an
experiment's own variables go. What is not here is the other two **sources** of
symbols: plugin-exported constants need X-PLUGIN's inventory to exist, and
catalog-qualified bindings are [K5](instantiate-catalog-templates.md)'s to
introduce. Neither needs a second resolution rule when it arrives.

Four decisions are worth carrying forward:

- **The transition runs in two phases.** A batch can define a variable and use
  it in the same breath, and changing a definition reprices every property
  that reads it; neither works if a property is priced when its command is
  applied. Phase 1 applies structure and *collects* authored values, phase 2
  builds one `VariablesSystem` from the candidate and prices everything. This
  replaced pricing-inside-`apply` and is why `CreateObject` no longer resolves
  anything itself.
- **Only the affected closure is evaluated.** Every definition is *declared*
  into the system, because any expression may reference any of them, but only
  those the batch changed — transitively — are *evaluated*. A definition whose
  inputs did not move resolved at the previous revision and resolves
  identically now. `CommitReport::work` reports the counts so this is
  observable rather than asserted: the test grows the graph to 41 definitions
  and 41 objects and still pays for one of each.
- **Repricing a stored value re-resolves its retained source.** A stored
  quantity keeps its expression and display unit, so repricing is resolving
  that same authored value again. There is no second form of the intent that
  could drift from the first.
- **A rename is a validated refactoring, not a rebind.** The identity never
  moves; every expression that named the old one — in other definitions *and*
  in property sources — is rewritten in the same edit through the shared
  `orishu_variables::rewrite_symbols`, or the batch is refused whole. That
  helper moved out of `kagami-catalog`, where its own documentation noted it
  was mirroring the parser's lexer from another crate.

## Non-goals

- A second expression parser, evaluator, or unit catalog.
- Symbolic algebra, equation solving, or arbitrary user code.
- Binding a variable to a live run observation; adopting a computed value is an
  explicit command with run provenance (ADR 0004).
- Workload-side resolution and freezing — S-VARIABLES slice 4.
- Variable editor widgets or MCP tools; those consume these commands.
