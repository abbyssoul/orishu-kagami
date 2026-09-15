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

- Every selected kernel validates timestep against its configuration/discretization.
  Rejection preserves simulation time; advice never silently rewrites authored `dt`.
- History has explicit cold-start/birth/death semantics with X-EMITTER. Missing
  required history is diagnosed; checkpoint/next-step results are equivalent across
  births and restart. PoC Verlet uses pre-force half-drift and is not automatically
  a conforming first-profile fixture.

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

## Follow-up — Staged integrator capabilities

Status: deferred design review, not first-pass implementation. Trigger: field
kernels and Dynamics execute together under the fixed pipeline with validated
numerical and checkpoint/restore evidence. Owners: X-COMPOSITION/O-WASM with
X-FIELDS; coordinate any resulting contract changes with X-PLUGIN/S-WORKLOAD.

Review integrator hooks before field-force evaluation and after forces are
computed, including multiple force evaluations within one committed tick. Test
the needs of explicit Euler/Verlet variants and RK4 rather than inferring them
from names. Define field-kernel capability declarations and compatibility checks;
not every field solver can evaluate trial states or additional stages safely.
Review trial-state isolation, stage time, deterministic reduction, field-state
advancement, metering and rollback before admitting any expanded profile.

Use Field CAD's reported circular position/velocity histories as a reference.
Specify integrator-required history initialization, bounded retention, updates
and exact restart; distinguish it from trajectory display/recording retention.
No trail setting may change scientific history or numerical results. Verify that
the initial profile rejects unsupported staged combinations with clear diagnostics.

Exit: a reviewed versioned profile/ADR and bounded implementation task, or a
documented decision to retain the initial profile. No speculative hook API is
required to close the first-pass work.

## Non-goals

- Choosing production numerical methods for every field.
- External interactive impulses; they require a reliable intervention contract.
- Mandating an ECS storage library.
