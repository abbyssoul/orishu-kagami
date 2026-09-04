# Migrate and integrate the shared variables subsystem

Status: **ready**  
Decisions: [ADR 0004](../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0005](../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0007](../adr/0007-share-expression-semantics-with-workload-resources.md)

## Outcome

Orishu and Kagami share an idCVar-inspired variables and expressions subsystem.
It lets experiments and workload resources define namespaced variables, retain
authored expressions, resolve dependencies and expression-capable field paths,
and use dimensioned quantities such as `mass_of_sun = 1e32 kg`.

The existing `../field-cad/crates/fieldcad-variables` crate is the starting
point for the generic engine. It is migrated into this workspace and renamed
for its shared product role. Dimension and unit awareness is then added above
the generic variables and expression parser and integrated with both Kagami
experiments and Orishu workload resources; it is not pushed into an
application-global mutable registry.

## Source assessment

- Migrate the useful implementation and tests from
  `../field-cad/crates/fieldcad-variables`: parsed arithmetic expressions,
  namespaced names, opaque stable handles, definitions, recursive reference
  resolution, cycle detection, inspection, and comments.
- Use `../field-cad/crates/fieldcad-expressions` as a reference for retained
  source, physical dimensions, unit literals, source spans, dependency
  diagnostics, resource limits, candidate evaluation, and property-schema
  checks. Do not copy that crate wholesale or create a second parser.
- Reassess every dependency and API under the migration admission test in
  [`docs/migration.md`](../migration.md). Remove Field CAD names and assumptions
  rather than preserving compatibility aliases.

## Implementation slices

### 1. Migrate the generic engine

- Add the workspace library crate `crates/orishu-variables`, published as
  `orishu-variables`, for use by Kagami and Orishu resource handling.
- Move the generic parser, compiled-expression representation, namespaces,
  variables, stable handles, evaluator, errors, and their interface-level
  tests from `fieldcad-variables`.
- Keep this layer independent of Kagami UI, MCP, experiment documents, Orishu
  networking/runtime, solvers, and resource-specific schemas. It is an owned
  value passed to pure transitions, never a process-global singleton.
- Retain expression source and source spans. Add explicit bounds for source
  length, parse depth/node count, variable count, dependency depth, and
  evaluation work before the engine is exposed to MCP input.
- Preserve idCVar's useful conceptual property—one discoverable registry and
  one evaluation path—without adopting id Tech's global mutable configuration
  semantics or stringly typed values.

### 2. Add shared dimension and unit semantics

- Build one dimension-aware value layer over the migrated expression grammar
  and name resolver. Do not fork the parser into dimensionless and unit-aware
  implementations.
- Represent a resolved physical value as a finite magnitude plus physical
  dimension, with canonical SI magnitude at domain and workload boundaries.
- Parse unit-bearing literals and derived arithmetic, including at least the
  representative cases `1e32 kg`, `2.7 g / cm^3`, and dimensionless
  `1 / 3 + 0.1`.
- Derive dimensions through multiplication, division, and supported powers;
  require compatible dimensions for addition and subtraction; and validate a
  resolved expression against the target property's expected dimension.
- Keep the authored source and preferred display unit separate from the
  canonical resolved quantity so evaluation does not destroy authoring intent.
- Report syntax, unknown/ambiguous name, dimension mismatch, cycle, division by
  zero, non-finite result, and resource-limit errors with stable source spans
  where the failure belongs to source text.

### 3. Integrate with the experiment authority

- Model variable definitions as persisted experiment intent with stable
  document-local identity, editable name, namespace/scope, expression source,
  and optional description/provenance.
- Make define, set, rename, remove, and property-expression assignment typed
  experiment commands decided through the document authority from ADR 0004.
- Compile an explicit dependency graph. Evaluate the complete affected closure
  into candidate state and atomically accept one new document revision only
  when all affected variables and properties are valid. On rejection, preserve
  the last valid graph and resolved values.
- Make one accepted variable edit one undo entry even when it changes many
  dependent projections. Renaming updates references as one validated
  refactoring; removing a referenced variable is rejected with its dependents.
- Version and bound the persisted representation. A save/open round trip
  preserves expression source, stable identities, namespaces/scopes, units,
  and meaning; derived evaluation caches are not authoritative persisted state.
- Expose the same public command and diagnostic types to eventual UI and MCP
  adapters. This task need not build their presentation or transport surfaces.

### 4. Integrate with Orishu workload resources

- Extend the versioned workload schema so it can retain the shared expression
  language version, variable definitions, and expression source for numeric
  fields explicitly marked expression-capable. Other resource fields remain
  literal unless their schema opts in.
- Support canonical, unambiguous references between expression-capable workload
  fields, including relationships such as a bound width derived from its
  height, without coupling the shared evaluator to `WorkloadSpec` types.
- Make workload compatibility checks and acceptance parse, bound, resolve,
  dimension-check, and evaluate the complete graph through the shared library.
  Structured failures reject the manifest before it changes cluster state.
- Freeze the source-bearing manifest and canonical resolved parameter set for
  the workload epoch. All participating workers verify the language version
  and resolved fingerprint before reporting `Ready`.
- Pass only immutable, validated, dimensioned canonical values to workload
  packages. Expression source is not parsed or reevaluated during simulation
  steps, and changing a definition requires workload replacement.
- Do not bind authored variables implicitly to local or remote run
  observations. Computed-to-authored values enter only through an explicit
  adoption command carrying run and simulation-boundary provenance.

## Acceptance criteria

- The migrated shared crate has no dependency on application, UI, MCP, solver,
  document, networking/runtime, or resource-schema modules and contains no
  Field CAD product names.
- Existing applicable `fieldcad-variables` interface tests pass through the
  migrated crate's public API; tests that encoded obsolete APIs are rewritten
  around behavior rather than copied blindly.
- Namespaced variables can depend on other variables, redefining a dependency
  updates dependents, and cycles and unknown names produce structured errors.
- Dimension-aware integration evaluates the representative expressions above,
  rejects `1 kg + 1 m`, rejects a mass expression assigned to a length
  property, and never adopts NaN or infinity.
- A failed variable edit leaves the experiment revision, definitions, resolved
  properties, and undo history unchanged. A successful edit advances exactly
  one revision and creates exactly one undo entry.
- Stable identities survive rename and document round trip. Referenced removal
  is rejected with dependents identified.
- UI-originated and MCP-originated command envelopes produce identical document
  decisions when given the same command and base revision.
- Kagami experiment validation, Orishu compatibility checking, and Orishu
  workload acceptance produce equivalent values and diagnostics for the same
  language version, bindings, dimensions, and limits.
- Oversized source, excessive nesting, excessive graph/dependency depth, and
  excessive evaluation work fail within declared bounds without partially
  changing state.
- A source-bearing workload round trip preserves expressions. Acceptance is
  deterministic for its language version, freezes resolved canonical
  quantities, and workload stepping does not invoke the evaluator.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Reproducing the complete id Tech `idCVarSystem` API, flags, console commands,
  or global singleton lifetime.
- Building variable editor widgets or MCP transport endpoints; those consume
  the subsystem's shared commands and diagnostics in later tasks.
- Treating presentation state, Orishu configuration, or live observations as
  experiment variables.
- Making every Orishu resource field expression-capable; schemas opt in only
  where expressions are part of declarative resource intent.
- General symbolic algebra, automatic equation solving, or arbitrary user code.
- Migrating `fieldcad-expressions` as a second expression engine.
