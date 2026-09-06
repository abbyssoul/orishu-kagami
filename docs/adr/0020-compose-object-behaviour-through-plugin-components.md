# 0020 — Compose object behaviour through plugin components

Status: **accepted**  
Date: **2026-09-07**  
Refines: [ADR 0009](0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0019](0019-kagami-experiment-document-model.md)

Refined by: [ADR 0024](0024-orishu-orchestrates-a-workload-component-graph.md)

## Context

ADR 0019 makes an authored object an entity composed from plugin-contributed
components, but does not yet say how that composition becomes executable
behaviour. That omission is dangerous: a workload compiler could infer physics
from a catalog template or species name, or several plugins could each advance
the same object's pose.

Field CAD proved the useful composition. Position and velocity are kinematic
state intrinsic to a modeled object. A dynamics component opts it into inertial
integration. Independent field components contribute forces through explicit
couplings. Dynamics combines those contributions and integrates the object
once. Gravity and electrostatics are examples of the same rule, not privileged
branches.

## Decision

Modeled objects have stable identity and intrinsic pose and velocity. A static
or kinematic object has no dynamics component. Attaching a
plugin-qualified component adds only the capability declared by that component
schema and pinned workload implementation.

The bundled **dynamics** component supplies inertial mass, impulse handling and
force accumulation. It is the single owner of velocity and position integration
for a dynamic object. Impulses change momentum; accumulated force changes
momentum over an accepted fixed step; the resulting velocity advances position.
Acceleration, accumulated force and momentum are run state or derived
observations, not silently persisted as editable initial values.

There is no independent persisted `pinned` or motion-authority flag. During a
run, the presence of a compatible dynamics component is the sole opt-in to
integration. Without dynamics, authored pose and velocity remain kinematic
initial state and the object stays fixed for the run. Authoring-mode
manipulation does not need a run-time ownership flag.

Field plugins declare source and coupling components separately. For example:

- gravity may contribute a gravitational source mass and a gravitational
  coupling mass;
- electrostatics may contribute electric charge as a source and coupling; and
- either coupling contributes a typed force to dynamics rather than advancing
  the object directly.

Inertial mass, gravitational mass and electric charge remain distinct
quantities. A schema may offer an explicit authored relationship such as
"gravitational mass follows inertial mass", but equality is never inferred from
a shared display label.

The versioned workload lifecycle defines deterministic phases: evaluate fields
and other interactions from one committed boundary, gather typed contributions,
combine them in stable order, integrate each dynamic object exactly once,
validate the candidate, and commit it atomically. A plugin cannot acquire
another plugin's state through ambient host access. Cross-component exchange
uses the versioned, bounded workload contract.

Built-in and third-party components use this same contract. Catalog templates
and editable names compose authored data; they never select executable
behaviour. External runtime impulses or forces require a later reliable,
identified intervention protocol and are not implied by the authoring command
surface.

This record fixes the semantic ownership and phase requirements. ADR 0024
resolves the executable topology: Orishu hosts a workload component-instance
graph and mediates its deterministic phase plan.

## Consequences

- Kagami can explain an object's behaviour from its visible components.
- Adding dynamics without a field coupling produces inertial motion; adding a
  coupling without dynamics cannot secretly move an object.
- Gravity and electrodynamics exercise a reusable composition contract rather
  than establishing hard-coded species.
- The workload schema must carry stable component identities, state ownership,
  quantity dimensions, execution phase, and deterministic contribution order.
- Numerical implementations may use an ECS or another hot layout internally;
  that storage choice is not part of the persisted document or workload wire
  format.

## Non-goals

- Selecting a particular integrator, field solver, or distributed
  decomposition.
- Making every visible object simulated.
- How a plugin internally packages tightly coupled numerical kernels.
- Treating a probe, camera, trail, vector glyph or flow line as a physical
  component.

## Implementation

Tracked by [Define composed object execution](../tasks/kagami/define-composed-object-execution.md)
and the X-PLUGIN, S-WORKLOAD, O-WASM and O-RUNTIME roadmap packages.
