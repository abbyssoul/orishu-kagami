# 0007 — Share expression semantics with workload resources

Status: **accepted**  
Date: **2026-09-04**

## Context

ADR 0005 originally placed variables and expressions entirely on Kagami's
authoring side and required submission to contain resolved values only. That is
insufficient once expressions describe relationships intrinsic to workload
intent, such as `bounds.width = bounds.height / 2`.

Kagami is not the only workload author. A manifest may come from
`orishu-ctl`, another client, a generated file, or a future integration. If
only Kagami understands expressions, those producers cannot express the same
model and Orishu must trust client-resolved numbers that it cannot validate
against the authored relationships. Two independent implementations would
also risk accepting different dimensions or producing different values.

Expressions are still declarative workload input, not mutable simulation
state. Re-evaluating them during a run would make an immutable workload change
under execution and would weaken workload identity, checkpoints, and
reproducibility.

## Decision

The variables, expression-language, unit, dimension, and evaluation semantics
are a shared Orishu Kagami resource contract implemented once in a reusable
`orishu-variables` library.

- Kagami uses the shared subsystem to edit an experiment and evaluate candidate
  revisions for immediate feedback. This evaluation is authoritative for the
  editable experiment only.
- A workload manifest retains its variable definitions and expression source.
  Numeric fields participate only where the workload-resource schema marks
  them expression-capable. Expressions may reference named variables and
  other expression-capable resource fields through canonical, unambiguous
  schema paths.
- Orishu parses, bounds, resolves, dimension-checks, and evaluates the complete
  graph when checking or accepting a workload. Kagami's submitted resolved
  preview may be used for diagnostics but is never trusted as the authority.
- Workload acceptance freezes both the source-bearing manifest and a canonical
  resolved parameter set for the workload epoch. All participating workers
  must agree on the expression-language version and resolved fingerprint
  before reporting `Ready`.
- Workload packages receive resolved, dimensioned canonical values. They do not
  parse expression source, resolve variables, or observe variable changes
  while stepping.
- Variables are not runtime control knobs. Changing a definition or expression
  produces a different workload manifest and requires normal workload
  replacement; it never mutates a loaded or running workload.
- Runtime status, observations, checkpoints, results, cluster membership, and
  presentation state do not become expression graphs. Another resource kind
  supports expressions only through an explicit schema and protocol decision.
- The language and unit catalog are explicitly versioned as part of the
  resource contract. Implementations reject unsupported versions rather than
  silently changing parsing, name resolution, units, numerical semantics, or
  canonicalization.
- Run observations remain outside the variable graph. Moving an observed value
  into an experiment or future workload requires the explicit adoption command
  defined by ADR 0004.

## Consequences

- Kagami, Orishu admission, compatibility checks, and workload tooling depend
  on one shared evaluator and produce the same diagnostics for the same input.
- The library has no dependency on Kagami UI/document types, Orishu networking
  or cluster runtime, MCP, solvers, or resource-specific schemas. Those layers
  provide namespaces, field bindings, expected dimensions, and limits through
  its public interface.
- Workload identity and provenance cover the expression-language version,
  source-bearing definitions, and canonical resolved parameter fingerprint.
- `GET` returns the authored workload resource rather than replacing
  expressions with numbers. Runtime code separately consumes the frozen
  resolved representation.
- Compatibility checking includes expression parsing, resource-path
  resolution, cycle detection, dimensional validation, bounded evaluation,
  and language-version support without loading the workload.
- Existing literal unit strings remain the simplest expressions where their
  schema fields opt in; callers need not define a variable to provide a fixed
  quantity.
- ADR 0005's authoring semantics remain valid, but its former resolved-only
  submission boundary is replaced by this decision.
