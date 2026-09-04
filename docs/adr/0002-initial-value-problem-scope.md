# 0002 — Scope orishu to initial value problems

Status: **accepted**  
Date: **2026-09-04**

## Context

`orishu` advances one coherent spatiotemporal world-state forward through
ordered simulation boundaries, as described in
[`docs/spatiotemporal-foundation.md`](../spatiotemporal-foundation.md). That
document explains the architectural *why* of the spatiotemporal model; this
record captures the specific, costly-to-reverse scoping decision it rests on:
which class of problem the runtime, storage model, and coordination model are
built to solve first.

Numerical simulation broadly splits into initial value problems (IVP), where
state is advanced forward from a known starting condition, and boundary value
problems (BVP), where a solution must satisfy constraints imposed across the
whole domain or across multiple time boundaries simultaneously. BVP solvers
generally require globally coupled solves (e.g. relaxation, shooting methods,
or whole-domain linear systems) that do not decompose into a strict forward
march through spatial partitions the way IVP stepping does.

Designing the initial runtime to support both would mean building ownership,
fencing, and checkpointing semantics general enough for globally coupled
solves before any product ships. That generality is unproven and would slow
delivery of the IVP product the spatiotemporal foundation already targets.

## Decision

`orishu` solves initial value problems only. The runtime, storage, and
coordination model assume forward-in-simulation-time advancement from a known
initial condition over a partitioned spatial domain, per
`spatiotemporal-foundation.md`.

Boundary value problem support is an explicit non-goal for the current
product. It may be considered only after an IVP-capable product has shipped,
and only as a deliberate extension evaluated against the coordination model at
that time, not retrofitted into it opportunistically.

## Consequences

- Ownership, fencing, checkpoint, and result-artifact semantics in
  `spatiotemporal-foundation.md` and `docs/design.md` are IVP semantics; they
  are not required to anticipate globally coupled BVP solves.
- Workload authors targeting BVP-style problems (steady-state fields, global
  constraint satisfaction) are out of scope until a future decision revisits
  this one.
- Future BVP support, if pursued, is a new architectural evaluation, not an
  incremental change to the IVP-scoped ownership and stepping model.
