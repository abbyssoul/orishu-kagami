# 0005 — Author numeric values as unit-aware expressions

Status: **accepted**  
Date: **2026-09-04**

The resource-boundary portion of this decision is refined by
[ADR 0007](0007-share-expression-semantics-with-workload-resources.md): workload
manifests retain expressions and Orishu resolves them authoritatively when
checking or accepting a workload.

## Context

Scientific authoring rarely consists only of entering isolated numeric
literals. A user may want to enter `1 / 3 + 0.1`, define
`mass_of_sun = 1e32 kg`, and use `mass_of_sun` in object properties and other
definitions. Retaining only the resulting floating-point value loses the
author's intent, makes related parameters drift apart, and prevents a change to
one definition from being propagated and validated coherently.

Field CAD explored this in two prototypes. `fieldcad-expressions` integrated
retained expression source, physical dimensions, document constants, property
bindings, dependency diagnostics, and resource bounds. The later
`fieldcad-variables` refined a smaller generic core with stable handles,
namespaces, parsed expressions, recursive resolution, and cycle detection, but
deliberately omitted units, documents, and solvers. The prototypes are evidence
for the required seam, not product modules to copy unchanged.

Kagami's document authority must remain the only authority for experiment
intent. Expression evaluation cannot become a side channel that mutates
properties, and a running solver's observations cannot silently become live
variables in an experiment.

## Decision

Kagami will have a first-class **variables and expressions subsystem** within
experiment authoring.

- Numeric fields declared expression-capable by their schema retain authored
  expression source rather than only a resolved number. A plain literal is the
  simplest expression.
- An experiment may define named variables whose definitions are themselves
  expressions. Variables have stable document-local identities independent of
  their editable names, optional descriptive metadata, and explicit namespace
  or scope. Expressions may refer to variables and built-in mathematical or
  physical constants through unambiguous names.
- Expression values carry physical dimensions. Literals may include units;
  arithmetic derives dimensions; and assignment to a property succeeds only
  when the expression's dimension matches the property's schema. Resolved
  physical quantities use canonical SI representation at domain and workload
  boundaries without discarding the authored source or preferred display unit.
- The subsystem compiles references into a dependency graph, evaluates in
  dependency order, and reports syntax errors, unknown or ambiguous names,
  dimension mismatches, cycles, division by zero, and non-finite results with
  stable source spans where applicable.
- Defining, changing, renaming, or removing a variable and setting a property
  expression are experiment commands decided by the document authority. The
  authority evaluates the complete affected dependency closure as a candidate
  and accepts the edit atomically only if the experiment remains valid. A
  rejection preserves the previous variable graph and resolved property state.
- UI and MCP authoring use the same expression parser, variable namespace,
  command path, limits, and diagnostics. Expression length, parse depth, graph
  size, dependency depth, and evaluation work are bounded before externally
  supplied input is adopted.
- Compiling an experiment to a workload preserves the definitions and
  expression source from that exact experiment revision. Kagami resolves them
  for authoring feedback, and Orishu independently validates and resolves the
  same shared language when checking or accepting the immutable workload.
- Run observations are not variables in the authored experiment. Turning an
  observed value into a variable definition or property expression is an
  explicit adoption command under ADR 0004, with source run and simulation
  boundary provenance.

The subsystem is a pure shared-domain component. UI controls, MCP tools,
persistence, resource schemas, catalogs, workload admission, and workload
compilation depend on its public values and decisions; the subsystem depends on
none of those adapters.

## Consequences

- Experiment documents persist variable definitions, expression source,
  identities, namespaces/scopes, display-unit choices where present, and the
  schema/version information needed to compile them reproducibly.
- Editing one variable may update many dependent previews and properties, but
  it remains one atomic document revision and one undo entry.
- Renaming or removing a referenced variable must either update references as
  one validated refactoring or be rejected with its dependants identified.
- Workload identity covers the source-bearing definitions, expression-language
  version, canonical resolved values, and relevant provenance of a specific
  experiment revision, not whichever values the editor happens to resolve
  later.
- Resolved values may be cached as a derived projection for interactive speed,
  but caches are never authoritative and must be invalidated from dependency
  changes.
- The two Field CAD prototypes require a migration evaluation. The generic
  variable engine and the dimension-aware document layer may inform separate
  internal modules, but their historical APIs and names do not define Kagami's
  public model.
