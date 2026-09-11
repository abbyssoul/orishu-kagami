# Implement the single-node workload runtime

Status: **specified; execution gated on shared contracts and sandbox host**.
Owner: **O-RUNTIME**. Target: M2 admission foundations and M3 execution; required
input to M5 distributed execution, not work implicitly completed by M4.

## Outcome and current gap

One worker independently admits an immutable workload closure and owns a run
that advances through accepted fixed simulation steps. Formation readiness does
not mean workload execution is implemented. Consume S-WORKLOAD and the
[component-graph host](implement-wasm-component-graph-host.md), rather than
creating another manifest definition or guest engine.

Contracts: [architecture](../architecture.md), [workloads](../workloads.md),
[workload protocol](../protocol-workload.md),
[ADR 0009](../adr/0009-execute-workloads-as-sandboxed-portable-programs.md) and
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md).

## Prerequisites and bounded slices

1. Review current public types and callers; freeze the run lifecycle, workload/
   run/epoch/boundary identity, command outcomes and bounded failure transitions.
   Coordinate S-IDENTITY/S-OBSERVE, O-CLIENT and O-STORAGE contracts before
   their integration. Record any new authority or persisted/wire choice in an
   ADR; this task does not select an undocumented state machine.
2. Independently validate the complete workload closure, compatible graph/ABI,
   dimensions, limits and execution requirements before admission. Reject
   malformed, incomplete or incompatible submissions atomically; enforce at
   most one workload at a time, without adding a queue or scheduler.
3. Compose the owner with O-WASM. The host executes the admitted component plan;
   the runtime owns time, cancellation and run-level publication. Validate the
   entire candidate before advancing the committed boundary. Traps, exhaustion,
   missing output or cancellation cannot expose partial committed state.
4. Integrate O-STORAGE checkpoint/result identities and S-OBSERVE projections
   through their reviewed interfaces. Only committed state is observable;
   resume validates completeness and compatibility before adoption. Slow
   observers cannot block scientific commit or mutate experiment intent.
5. Prove one supported fixture against its analytic/reference result, including
   multi-component phase failure, deterministic scheduling variation, stop and
   checkpoint/resume. Add bounded workload lifecycle observability and update
   worker/operator/researcher workflows with the actually supported surface.

## Acceptance criteria

- Real host/runtime integration, not only mocked guest calls, demonstrates
  atomic admission, fixed-step advancement and unchanged prior committed state
  after rejected candidates.
- Workload and run identity, dimensions, precision, validity and provenance are
  retained across observations and checkpoints. Wall time is not simulation time.
- Hostile inputs/guests and observer pressure have bounded costs and leave
  membership/administration responsive. Relevant feature and dependency tests pass.
- Reference and restart evidence forms the baseline consumed by
  [N-CLUSTER](implement-distributed-workload-execution.md); shared behavior stays
  under O-RUNTIME/O-WASM ownership rather than a separate distributed runtime.
- Manuals, task index and M3 evidence distinguish delivered from gated features.

## Non-goals

Distributed ownership/commit, implementing the storage or client API programmes,
new numerical models, native guest execution, authoring mutation, or reopening
formation acceptance. Dependencies still need their own bounded implementation.
