# 0016 - Use a narrow Maxwell/Yee profile for first distributed conformance

Status: **proposed for revalidation**

## Context

The historical Orishu/Field CAD integration selected one narrow scientific
profile to test partition loading, halo exchange, stepping, checkpointing, and
rebalancing. The monorepo replaces Field CAD with Kagami and changes submission
ownership, but it has not recorded whether the scientific profile itself is
still desired.

## Proposed retained decision

Use a versioned initial-condition profile for one stateful Maxwell/Yee FDTD
workload component:

- three-dimensional uniform Cartesian grid in SI units;
- periodic physical boundaries only;
- explicit electric/magnetic grid state and profile-defined sources;
- `f32` execution, evaluated against a verified `f64` CPU reference within a
  declared tolerance envelope; and
- global initial-state representation independent of Orishu's temporary
  partition map.

The input artifact uses a canonical deterministic envelope and indexed,
length-delimited binary payload chunks. It identifies profile/schema/component
and model versions, domain and discretization, time step, channel layout and
centring, byte order, dimensions, exact lengths, finite numeric constraints,
and SHA-256 digests.

Kagami compiles supported experiment intent into explicit immutable state and a
closed workload. Package code does not reinterpret mutable scene intent during
admission. Kagami may export or submit the closed workload; `orishuctl` may
submit a prebuilt workload but does not author or assemble Kagami experiments.

## Required validation

Before acceptance, define the concrete schema and limits, then test:

1. golden artifact encoding and hostile parser fixtures;
2. `f64` reference versus one-worker `f32` execution within tolerance;
3. one partition versus multiple partitions;
4. checkpoint/resume equivalence; and
5. ownership transfer and rebalancing equivalence.

Parsing requires bounded CBOR, checked offset/extent arithmetic, exact hashing,
finite-value validation, allocation limits, malformed/truncated/oversized
fixtures, and fuzzing.

## Non-goals

This profile does not establish general scene interchange, arbitrary plugins,
non-periodic boundaries, unstructured meshes, live edits, or a universal
observation schema.

## Review requirement

An agent or maintainer must reconcile this proposal with the current bundled
gravity/electrodynamics plugin plan and ADRs 0009-0010 before changing its status
to accepted. If another first conformance workload is preferred, supersede this
record explicitly rather than silently dropping the historical decision.
