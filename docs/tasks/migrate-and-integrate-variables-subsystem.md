# Migrate and integrate the shared variables subsystem

Status: **partial**; the generic engine, its declared resource bounds,
dimension/unit evaluation, and the experiment integration in
[K2](kagami/integrate-document-variables.md) have landed. Slice 4's workload
integration remains

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

### 1. Migrate the generic engine — **implemented**

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
  evaluation work before the engine is exposed to MCP input. A rejected parse,
  define, or set leaves the system unchanged.
- Preserve idCVar's useful conceptual property—one discoverable registry and
  one evaluation path—without adopting id Tech's global mutable configuration
  semantics or stringly typed values.

### 2. Add shared dimension and unit semantics — **implemented**

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

### 3. Integrate with the experiment authority — **implemented** as [K2](kagami/integrate-document-variables.md)

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

### 5. A prefix-selecting display for dimensioned values — optional backlog

`Display for Quantity` prints a magnitude against its dimension's canonical
unit, so a nanometre reads as `1e-9 m` and an astronomical unit as
`1.495978707e11 m`. Reading such values is already solved — the unit table and
`lookup` cover `nm` through `AU`, and expressions parse — but *presenting* them
is not, and every dimensioned field in Kagami's inspector has the same problem.

Field CAD carried a `format_si_value(magnitude, dimension)` that chose a
readable unit. [K14](kagami/choose-scene-scale.md) can ship with preset labels
and canonical-SI formatting for custom values; this is not its dependency.
Keep any app-only display helper local until a second concrete consumer or a
deep shared interface justifies extraction. A reusable formatter needs its own
bounded scope and round-trip/precision policy before implementation; no slice
is claimed until an owner takes it.

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

## Implementation record for slice 1

The migration itself landed earlier; the declared resource bounds landed in
`crates/orishu-variables/src/limits.rs` as a `Limits` value with a documented
`DEFAULT`, one structured `LimitError` naming the dimension, the value reached,
and the bound configured, and 20 acceptance cases in `tests/limits.rs` that
check every dimension exactly at its bound and one past it.

The six bounds are source bytes (checked before the text is lexed or retained),
parse depth, syntax-tree node count (charged as each node is admitted, so an
expression is refused before its nodes are allocated), variables per system,
dependency-resolution depth, and total evaluation work per top-level
evaluation. Counters use checked arithmetic and refuse before the thing being
counted is committed.

Four decisions are worth carrying forward:

- **Bounds are the caller's, never the process's.** A `VariablesSystem` owns
  the `Limits` it was built with and a parse is handed the limits for the
  source it is about to read; there is no global, thread-local, or otherwise
  ambient limit state. The convenient entry points — `CompiledExpression::parse`,
  `VariablesSystem::default`, `rewrite_symbols` — apply the documented
  `Limits::DEFAULT`; `parse_bounded`, `with_limits`, and
  `rewrite_symbols_bounded` take whatever a caller declares.
- **Depth is counted twice against one number**, because neither kind of depth
  implies the other. `((((1))))` recurses once per parenthesis but builds a tree
  one level deep, so only a parser-recursion counter stops it before the Rust
  call stack does; `a+b+c+…` recurses twice — the Pratt loop folds a
  left-associative chain without recursing — but builds a tree as deep as the
  chain is long, so only a tree counter keeps symbol collection, `is_const`, and
  evaluation, all of which recurse, bounded. One number governing both is what
  makes those walks safe by construction rather than by a second check.
- **A system re-prices an expression parsed elsewhere.** A
  `CompiledExpression` always passed some bound to exist, but not necessarily
  the bounds of the system adopting it, so `define` and `set` re-check the node
  count and depth the parse retained. Otherwise *where* a caller parsed would
  decide *which* bounds applied.
- **Dependency depth is a property of the graph, not of the walk.** Resolution
  memoizes, so a variable reached a second time is not walked again. Charging
  depth only where the walk descends made the bound depend on the order a root
  named its dependencies: with a bound of 2, `a = 1`, `b = a` and a root of
  `a + b` passed while `b + a` was refused, though both contain the same
  three-deep chain. Each memoized result therefore carries the height of the
  chain beneath it, and reaching it charges that height. The height is
  accumulated into the frame as each dependency settles — O(1) per edge, no
  extra map lookup — rather than recomputed when a frame pops.
- **A generated reference is not an authored one.** The reference a catalog
  binding publishes is built from its template identity, and nothing bounds a
  template name, so `catalog.template.component.property` can be longer than
  the evaluator will read even when every authored expression in the file is
  one character. `kagami-catalog` used to define those aliases through
  `expect`; under the new bounds that was a panic. It now skips the binding,
  reports it as a `RejectedBinding`, and the loader turns that into an ordinary
  diagnostic on the one template that published it — the rest of the set still
  loads, and a reference to the skipped binding resolves as unknown. The
  diagnostic is attributed by *contribution*, not by claim: a document that
  failed to parse still carries the identity it claimed, so two documents can
  name one template while a later one is what was actually projected, and
  matching on the identity alone files the refusal against a document that
  published nothing. `check_values` no longer asserts that a template's own
  binding is projected either — since the evaluator gained bounds that is a
  real state, and an assertion is what stops being true when a new bound is
  added underneath it.
- **Rewriting is bounded at both ends, so it had to become fallible.** A rename
  map from short names to long ones grows what it rewrites, and the growth
  factor belongs to the map rather than to the source. `rewrite_symbols` now
  returns `Result`; `kagami-document` reports the refusal through its existing
  `VariableExpressionTooLong` rejection and `kagami-catalog` through its
  existing `Evaluation` error, so neither acquired new semantics.

Measured cost, from `benches/variables.rs` run alternately against the
pre-bounds engine (four interleaved pairs; the box drifts by up to 25 % between
runs, so only paired readings mean anything and the first pair was discarded as
a warm-up artifact): dependency resolution is unchanged (`value_chain`, −2 % to
+5 %, indistinguishable from noise), defining is about 4 % slower, evaluating an
already-compiled expression about 5 %, and an ad-hoc `eval` — which pays the
parse-side accounting on every call — about 14 %, or roughly 257 ns to 295 ns
per expression. The counters are `#[inline]` with their refusals `#[cold]` and
outlined for that reason: a `LimitError` is wider than a register pair, so an
out-of-line check returns through memory on the path that always succeeds, and
leaving them uninlined cost a further ~15 % on the same benchmark. Carrying
depth through memoized results was measured separately over six pairs and
showed no detectable change (median −5 %, sign varying), though that run was
taken under load and is weaker evidence than the figures above.

The default dependency depth is 4 096 rather than something smaller because a
reference written in a catalog expression resolves through a concise alias
*and* the canonical binding it names — two levels per link. Depth is walked on
the heap, so it costs a vector entry; the bound that makes a runaway graph
affordable is the evaluation budget, which is what a wide shared graph
exhausts. `kagami-catalog`'s own limits carry a test asserting the shared
engine admits every binding a catalog may publish, because that projection
defines its variables through an `expect`.

## Implementation record for slice 2

Landed in `crates/orishu-variables` as the `quantity` module plus a
quantity-valued evaluator, with 12 acceptance tests in `tests/quantities.rs`.
`Dimension`, `Unit` and the unit table moved here from `kagami-catalog`, which
now re-exports them: the table that says what a gram is has to be the one the
evaluator reads, or the two drift.

Four decisions are worth carrying forward:

- **Units are ordinary symbols, not grammar.** `kg` resolves from the shared
  unit table when no variable defines it, so one parser carries units and
  `2.7 g / cm^3` derives a density from the arithmetic. The only syntax added
  is that a magnitude may be juxtaposed with a symbol (`1e32 kg`), parsed at
  the exponent's binding power so `2 m^2` is `2 * (m^2)`. Two adjacent names
  remain a syntax error — far more likely a typo than an intended product.
- **Only a *root* name may not shadow a unit.** A root variable `m` would
  silently retune every expression using metres, so it is refused. A
  *namespaced* one cannot be confused with a unit, because reaching it means
  writing `electricity.K` — so Coulomb's constant keeps its natural name.
- **`is_const` counts units as constants.** A unit is part of the language,
  not a value someone else supplies, so `2.7 g` folds like `2.7`. Without
  this, every unit-bearing literal would look like a computed expression to
  the catalog.
- **Stating a unit twice is refused, not resolved.** Now that an expression
  can carry its own units, `{expression: "2.7 g", unit: kg}` would scale the
  magnitude twice. Both the catalog and the document refuse it rather than
  pick a precedence rule nobody would remember.

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
