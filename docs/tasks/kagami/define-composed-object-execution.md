# Define composed object execution

Status: **specified**; executable topology accepted, implementation gated on shared contracts  
Work package: **X-COMPOSITION** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0020](../../adr/0020-compose-object-behaviour-through-plugin-components.md),
[ADR 0024](../../adr/0024-orishu-orchestrates-a-workload-component-graph.md)

## Outcome

Plugin schemas, workload compilation and the sandbox lifecycle share one
versioned contract for composed modeled objects. Dynamics integrates intrinsic
pose/velocity exactly once, while field plugins contribute typed forces through
explicit couplings.

Orishu hosts the selected component instances. The shared workload graph and
step plan define isolation, typed state exchange, deterministic dependencies,
metering, failure, checkpoint, placement and version-compatibility semantics.

## Slices

1. Extend X-PLUGIN schemas with stable component identities, state ownership,
   quantity dimensions, compatibility constraints and execution phases.
2. Define S-WORKLOAD component instances, typed channels, placement constraints
   and a bounded deterministic step plan, without inferring Rust memory layout.
3. Define the O-WASM component lifecycle and bulk host-mediated exchange for
   field updates, field-to-entity projection, force/impulse contribution,
   stable reduction, integration, validation, checkpoint and observation.
4. Validate complete graphs before `Ready`: channel producers/consumers,
   authoritative writers, dimensions, dependencies, bounded schedules, model
   exclusivity, per-instance/aggregate limits and placement compatibility.
5. Implement the bundled Dynamics component plus gravity and electrostatic
   coupling fixtures through the public contract; keep inertial mass,
   gravitational mass and charge distinct.
6. Prove analytic motion, stable contribution ordering, co-located/distributed
   semantic equivalence, determinism, atomic failure, checkpoint/restart,
   malformed graph rejection and CPU/runtime parity.

## Acceptance criteria

- An inertial object with dynamics and no field coupling advances analytically;
  a coupling without dynamics cannot move it.
- Gravity and electrostatics contribute forces without either plugin writing
  pose or velocity, and each dynamic object is integrated once per accepted
  fixed step.
- Object/template/species names cannot select executable code; exact plugin,
  schema and component identities are pinned in the workload closure.
- Public values are finite, dimensioned, bounded and versioned; traps or invalid
  contributions cannot partially commit a boundary.
- Components cannot call one another or access undeclared channels. Equal
  admitted graphs produce equivalent results whether eligible instances are
  co-located or distributed under the execution profile.
- A checkpoint is complete only with every required component-instance/
  partition part and runtime coordination state at the same committed boundary.
- Built-in and fixture third-party components pass the same conformance suite.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Choosing production numerical methods for every field.
- External interactive impulses; they require a reliable intervention contract.
- Mandating an ECS storage library.
