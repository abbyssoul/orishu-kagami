# Coding style guide

Status: **draft guideline**

This guide is for engineers and coding agents designing and reviewing Rust
code for quality: types that are hard to misuse, functions that are easy to
test, and architecture that stays easy to change. It is intentionally
application-neutral, in the same spirit as the [high-performance Rust
engineering guide](high-performance-rust.md) — general principles, not a
rewrite of any one project's architecture.

This guide covers *why* to shape code a certain way. It does not restate
this repository's concrete conventions (error types, derives, unit types,
naming, crate layout) — those live in the [engineering
guidelines](../GUIDELINES.md). It also does not cover performance trade-offs in
depth — allocation, data layout, and hot-path design are the subject of
[`high-performance-rust.md`](high-performance-rust.md), which this guide
cross-references rather than duplicates. Where the two guides pull in
different directions (for example, immutability vs. an allocation-free hot
path), treat the performance guide as authoritative for code identified as
hot by measurement, and this guide as the default everywhere else.

## Guiding idea

> Make illegal states unrepresentable, prefer functions with no hidden
> inputs or outputs, and defer architectural commitments until the access
> pattern that should drive them is actually known.

A type that cannot be constructed in an invalid state removes a whole class
of runtime checks and tests. A function that only reads its arguments and
returns a value is trivial to test, reorder, cache, or run in parallel. An
architecture that names its real constraints (what must stay decoupled, what
must stay swappable) instead of importing a framework's shape tends to
survive requirement changes better than one chosen up front for its
generality.

None of the practices below are absolute. State the trade-off you are
making and prefer the simplest design that satisfies it.

## Type design

### New-types and value types over bare primitives

A bare `f64` or `String` carries no information about what values are valid
or what unit, scale, or encoding it represents. A new-type
(`struct LengthMetres(f64);`) turns a convention into something the
compiler checks: a function that takes `LengthMetres` cannot be called with
a raw pixel count or a `Duration` by accident, and the type name documents
the unit at every call site without a comment.

Prefer *parsing into a value type at the boundary* over *validating a
primitive on every use*: construct the type once where the untrusted or
ambiguous input arrives (deserialization, parsing, a public API argument),
and let every downstream function receive an already-valid value. This is
the general form of "parse, don't validate" — validation logic that is
correct once and unreachable everywhere else is easier to review than the
same check repeated, or forgotten, at each call site.

Cost: a new-type adds a wrapper and, for `Copy` numeric types, is usually
free at runtime; for types with an invariant that cannot be checked by the
compiler alone (a non-empty vector, a normalized quaternion), the
constructor is the one place that invariant must be proven, so keep its
fields private and offer no way to construct the value that skips it.

### Composition over inheritance

Rust has no struct inheritance, so this principle takes the form of
preferring trait composition, free functions, and delegation over deep
trait-object hierarchies that imitate class inheritance. A type that needs
several independent behaviors should usually implement several small
traits, or hold several component values and expose narrow accessors,
rather than sit at the bottom of a trait hierarchy built to share code. Code
reuse through composition (a struct holding another struct, or a function
calling another function) is easier to trace than reuse through a trait
default method overridden three layers up.

### Prefer static dispatch; reach for boxing and dynamic dispatch deliberately

Generics and enums are resolved at compile time, can be inlined, and keep
data flow visible to the compiler and the reader. `Box<dyn Trait>` and
similar dynamic-dispatch patterns are the right tool at a genuine
boundary — a plugin contract that must accept implementations unknown at
compile time, or a heterogeneous collection whose element types are not
enumerable — but are a cost, not a default, when a generic parameter or an
enum would express the same set of cases. See
[`high-performance-rust.md`](high-performance-rust.md#borrow-move-and-share-deliberately)
for the runtime cost of indirection and reference counting when the
question is performance rather than design; here the concern is that a
`dyn Trait` boundary hides the concrete type from callers and the compiler,
which is sometimes exactly the point (decoupling) and sometimes just
friction.

### Immutability and pure functions by default

Prefer values that, once constructed, cannot change, and functions that
compute a result from their arguments without reading or writing anything
else. This is a design default, not a rule without exceptions: a
hot loop that must reuse a buffer across iterations, or a cache that must
survive across calls, needs mutation for allocation reasons — see
[`high-performance-rust.md`](high-performance-rust.md#allocation-is-part-of-api-design)
for when a mutable, reused output parameter is the correct contract. Choose
immutability as the starting point and drop to mutation only where a
measured or clearly load-bearing reason (ownership, allocation, identity)
requires it, and keep the mutable region as small as possible.

Immutable values and pure functions compose without surprises: passing an
immutable value to two callers never risks one mutating state the other
depends on, and a pure function's result depends only on its visible
arguments, so nothing about its behavior can hide outside the signature.

### Why this enables test-driven development

A pure function over value types needs no mocks, no fixtures, and no setup
beyond constructing its inputs — the test is "given this value, expect that
value." This is what makes red-green-refactor practical: a test written
before the implementation can assert on values alone, and a refactor that
preserves the function's signature and contract cannot change the test's
expectations. Code built from impure functions, hidden mutable state, or
inheritance hierarchies with overridden behavior usually needs a test
double or an integration harness to exercise the same case, which raises
the cost of writing the test before the code and discourages doing it.

## High-level architecture patterns

The patterns below are common shapes for structuring an application. None
of them is a default: choosing one before the workload or the access
pattern is known picks the answer before the question is asked. State
which constraint a pattern buys you (testability, decoupling, query
performance) before adopting it.

### Separate pure computation from IO

Structure the application as a small "imperative shell" that performs IO
(reading files, drawing frames, sending network messages, mutating shared
state) around a large "functional core" that computes decisions and values
from data already in memory. The shell should be thin enough that most bugs
are provably impossible to hit in the core, and the core should be testable
without ever standing up the IO it will eventually be wired to. This is the
same idea as "ports and adapters" or "hexagonal architecture" under a
different name; the specific label matters less than keeping IO at the
edges and pure computation everywhere else.

### Data-oriented design

Structuring data around the operation that repeats over it, rather than
around an object's conceptual identity, is its own discipline with
significant performance consequences — full treatment, including
array-of-structures vs. structure-of-arrays trade-offs, is in
[`high-performance-rust.md`](high-performance-rust.md#machine-sympathy-and-data-oriented-design).
The design-quality angle here is narrower: even where performance is not
the driver, grouping data by how it is transformed (rather than by an
inheritance-style taxonomy) tends to produce flatter, more composable code,
because the operation's inputs and outputs are visible instead of buried
inside a method on a deep object.

### The Elm Architecture (model-message-update, with or without a view)

A unidirectional data-flow architecture — one immutable model, a closed set
of messages that can change it, and a pure `update` function that folds a
message into a new model — makes state changes traceable and replayable:
every state transition is a value (the message) that can be logged,
replayed, or asserted on in a test without any IO running. The optional
fourth piece, a pure `view` function that renders the model, is what most
people picture when they hear "TEA," but it is not the load-bearing part.

The load-bearing part is narrower and more general than UI: TEA is a
specific application of separating pure computation from IO (see
[above](#separate-pure-computation-from-io)), where the IO boundary is
named explicitly as *messages* rather than left as an unstructured "the
shell calls into the core somehow." Anything that produces a message —
a user clicking a button, a socket delivering a packet, a timer firing,
another node's gossip update arriving — is IO, and it is uniform from the
model's point of view: the model does not know or care whether a message
originated from a human or from the network, only that it is a value
describing what happened. `update` stays pure and testable regardless of
how many different IO sources feed it, because every source is normalized
to the same message type before it reaches the core.

This is why TEA is not a UI-specific pattern in this codebase. A cluster
membership tracker is a worked example with no view at all: each node holds
its own model of the cluster (who the members are, and their state — live,
suspected, departed). That model changes only in response to messages —
a heartbeat received directly, a piggybacked membership update carried on
unrelated traffic, a suspicion timer firing with no message received in
time — each folded through a pure `update(model, message) -> model`. There
is no rendering step; the "view" a caller might build (a status table, a
log line, a metric) is just another pure function of the model, optional
and separate from the update loop. The architecture still buys the same
thing it buys a UI: every membership transition is a loggable, replayable
value, and the transition logic can be tested by asserting on
`(model, message) -> model` triples without a running network.

Reach for named model/message/update whenever a component's state is
mutated from more than one IO source (network, timers, user input, disk)
and the transitions matter enough to want them as inspectable values —
correctness-critical state like cluster membership, workload epoch, or
ownership tracking is the common case in `orishu`. It is overhead for a
small surface with a single caller and little shared state, where a
handful of local mutable fields are easier to read than a message enum
built for a state machine that does not yet need one.

### Entity-component-system and other composition/storage patterns

An ECS densely stores components by type and lets a system query "every
entity with components X and Y" without testing every entity for the
presence of unrelated data. It is one point in a wider space of composition
and storage patterns (plain structs, a `struct-of-arrays` alongside a
lookup table, a graph of typed handles), and "ECS is cache-friendly and
decoupled" is conditional on its storage model, query pattern, and rate of
structural change — see
[`high-performance-rust.md`](high-performance-rust.md#ecs-as-an-example-not-a-requirement)
for the performance side of that claim.

The design-quality trade-off is separate from performance: an ECS's natural
plugin or extension boundary is "a system with a query," which couples
every extension to the host's storage layout. That coupling is sometimes
exactly what is wanted (systems that must interoperate tightly) and
sometimes the opposite of what is wanted (a plugin contract meant to stay
expressible over a serializable snapshot, independent of how the host
happens to store data in memory). The right answer depends on which
property — query performance, or a decoupled plugin boundary — matters
more, and that in turn depends on access patterns that are not always known
up front.

The predecessor Field CAD project is a useful worked example of *deferring*
this choice rather than defaulting either way. It chose a plain object model
because the query pattern was not yet known and an ECS would have leaked host
storage layout into the plugin contract. It revisited the question once the
real requirement turned out to be authoring-time composition rather than a
storage decision. The lesson generalizes: prefer the storage
model that fits a measured access pattern, and be willing to revisit the
choice when a system demonstrates a different one — not to adopt or reject
ECS as a default.

### Dependency injection

Passing a component's collaborators in as constructor or function
parameters — rather than having the component reach for a global,
singleton, or ambient value — is what lets a test substitute a fake
collaborator and lets two systems that would otherwise share global state
run independently. In Rust this rarely needs a framework: a struct field
holding a trait object or generic parameter, or a function taking a
collaborator by reference, achieves the same decoupling as an
injection-container pattern in other languages. Reach for the pattern where
a component's dependency genuinely needs to vary (test doubles, alternate
backends, plugin-provided behavior) — threading a parameter through several
layers purely out of habit adds ceremony without a corresponding gain in
decoupling.

## Low-level implementation practices

### Contract-driven design

Give every non-trivial function an explicit contract: what its arguments
must satisfy on entry (preconditions), what it guarantees on return
(postconditions), and what must remain true throughout its body
(invariants). Where the type system cannot express a precondition (a slice
length relationship, a value range not worth a new-type), state it in a
guard clause at the top of the function that fails fast — via a `Result`
return for a condition a caller can legitimately trigger, or a `debug_assert!`
/ `assert!` for a condition that should be structurally impossible given the
rest of the program's contracts. A guard clause that fails immediately and
close to the violated assumption is far cheaper to diagnose than an
incorrect result discovered several calls later. Validating a proposed world
edit before adopting it is a concrete example of enforcing a contract at the
single point where it can be checked authoritatively, rather than trusting
every caller to have checked it already.

Choose the enforcement mechanism deliberately:

| Condition | Enforcement |
| --- | --- |
| Caller can legitimately violate it (external input, another process) | `Result`/guard clause returning an error |
| Should be structurally impossible if the rest of the program is correct | `debug_assert!`/`assert!` |
| Can be made impossible to violate at all | A new-type or private constructor (see [Type design](#type-design)) |

Preferring a new-type over a repeated runtime check is usually the stronger
contract, since it also blocks the code from compiling in the presence of
the violation rather than only failing at run time.

### Iterators over materialized collections

Prefer expressing a transformation as a chain of iterator adapters over
eagerly building an intermediate `Vec` at each step. This is first a design
clarity point: a lazy iterator chain reads as "this is the sequence of
operations applied to each element," while a sequence of `map` calls each
followed by `.collect()` reads as "build a list, then build another list
from it, then another," obscuring that the whole chain is really one
transformation. It also happens to avoid allocating the intermediate
collections — the performance dimension of that same choice, including when
materializing is unavoidable and how to let a caller reuse a buffer across
calls, is covered in
[`high-performance-rust.md`](high-performance-rust.md#separate-producing-materializing-and-allocating).
Materialize (`collect`, or write into a caller-provided buffer) at the
boundary where a concrete owned value or a fixed-size result is genuinely
required — not by default partway through a chain of transformations.

## Review checklist

Before proposing a new type or module:

- [ ] Can an invalid value of this type be constructed? If so, can a
      new-type or a private constructor make it impossible instead of
      checked at every call site?
- [ ] Does this function have hidden inputs (globals, ambient state) or
      hidden outputs (mutation the caller cannot see from the signature)?
      Can it be made pure, or can the effect be pushed to the shell?
- [ ] Is a trait object or `Box<dyn Trait>` here decoupling a real boundary
      (plugin, heterogeneous collection), or could a generic or enum
      express the same cases with static dispatch?
- [ ] Is an architectural pattern (ECS, TEA, a DI container) being adopted
      because the access pattern or requirement demands it, or because it
      is a familiar default? What access pattern would have to change for
      the choice to be wrong?
- [ ] Does state mutated from more than one IO source (network, timers,
      user input, disk) have its transitions named as an explicit message
      type folded through a pure `update`, or is it being mutated ad hoc
      from each call site? This applies equally to non-UI state such as
      cluster membership.
- [ ] Does every non-trivial function state its contract, and does a guard
      clause enforce the part the type system cannot?
- [ ] Is an intermediate collection being materialized where a lazy
      iterator chain would express the same transformation?

## References

- [Engineering guidelines](../GUIDELINES.md) — this project's concrete conventions
  (errors, serialization, units, derives, naming, test placement).
- [`high-performance-rust.md`](high-performance-rust.md) — performance
  trade-offs for data layout, allocation, and hot-path design.
- [Migration strategy](migration.md) — how predecessor code is evaluated
- [Parse, don't validate (Alexis King)](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/)
- [The Elm Architecture](https://guide.elm-lang.org/architecture/)
- [Functional core, imperative shell](https://www.destroyallsoftware.com/screencasts/catalog/functional-core-imperative-shell)
