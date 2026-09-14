# Define composed object execution

Status: **specified**; executable topology accepted, implementation gated on shared contracts  
Work package: **X-COMPOSITION** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0020](../../adr/0020-compose-object-behaviour-through-plugin-components.md),
[ADR 0024](../../adr/0024-orishu-orchestrates-a-workload-component-graph.md)

## Outcome

Shared plugin identities, typed payloads and selected-closure resolution follow
[ADR 0027](../../adr/0027-plugin-contributions-and-immutable-releases.md) and
[X-PLUGIN's design gates](../define-and-implement-plugin-contract.md).
Coordinate their ownership with S-WORKLOAD; do not duplicate contracts or export
an entire plugin bundle when only some contributions are selected.

Plugin schemas, workload compilation and the sandbox lifecycle share one
versioned contract for composed modeled objects. Dynamics integrates intrinsic
pose/velocity exactly once, while field plugins contribute typed forces through
explicit couplings.

The initial [field-to-entity profile](../../simulation-plugins.md#field-to-entity-execution)
places field evolution and entity-force production in the same field kernel;
it does not require a separate coupling executable. Each field sees its own
previous state and read-only matching entity data, not other fields' partial
outputs. The first pass fixes the pipeline to all field/force computation followed
by Dynamics reduction/integration; configurable pipelines are future work.
The existing graph represents this fixed profile, not arbitrary initial scheduling.
Dynamics reduces contributions and integrates motion. Force evaluation
stage remains a contract gate, not an implied choice of numerical method.
Field initialization constructs kernel-defined natural defaults and bounded
setup without coupled entities; completed-experiment validation is separate.
Default construction runs in Kagami's local sandbox on field creation; its
scientific output is captured in the experiment and preserved on reopen/export.
Runtime-only setup is reconstructed separately. Exact exports and state storage
remain gated with X-PLUGIN/O-WASM.

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
