# Implement the single-node workload runtime

Status: **fixed-profile library owner, selected-workload admission and worker
formation reservation, off-owner admission and retained single-node execution/
publication, owner-issued run identity, lease-bound complete-body input and internal
durable receipt journal/daemon-owned admission coordinator and opt-in public
load/receipt/current-run routes implemented; public run control remains gated**.
Owner: **O-RUNTIME**. Target: M2 admission foundations and M3 execution; required
input to M5 distributed execution, not work implicitly completed by M4.

## Outcome and current gap

One worker independently admits an immutable workload closure and owns a run
that advances through accepted fixed simulation steps. Formation readiness does
not mean workload execution is implemented. Consume S-WORKLOAD and the
[component-graph host](implement-wasm-component-graph-host.md), rather than
creating another manifest definition or guest engine.

Current evidence: [FixedRun](../runtime-fixed-run.md) validates captured numeric
objects/fields/history, owns atomic single-partition fixed steps, preserves whole
state on late kernel/validation failures, checkpoints/restores complete portable
in-memory state and grants bounded detached field-snapshot leases. It uses the
actual Newtonian/Euler Components. Shared `orishu_runtime::admit` now independently
checks canonical v3 roots, exact selected releases, fixed scientific graph and
captured inputs, then compiles/validates real Components without plugin installation
or reinitialization. The admission suite proves selected export and fresh-runtime
execution, including rejection of inconsistent captures and denied code. This does
not itself implement worker endpoints, delivery or formation-owned run allocation. The worker now
has an [owner-issued exclusive execution reservation](../adr/0028-fence-worker-scientific-admission-through-formation-owner.md),
which fences pending admission against topology changes and releases reliably on
caller loss. `driver::scientific::AdmissionService` now consumes that lease in
off-owner portable-closure verification and actual shared sandbox admission, with
parent-linked guest cancellation and owner-confirmed handoff. The formation owner
now assigns [canonical run-descriptor scope](../run-descriptor-v1.md) after closure
verification and refuses submitter rebinding. Caller cancellation retains capacity until blocking work actually
exits; native JIT itself remains non-interruptible. Five real-Component worker tests
exercise this path and its failure/race cases. `ValidatedAdmission::start` now
retains that executor across explicit steps, terminal stop and field-lease requests,
with internal boundary publication serialized by the formation owner. Three more
real-Component tests cover this path, including complete late candidates after
shutdown/owner loss. Real WorkerRuntime bootstrap now binds the service and allocator.
The prepared admission's bounded async body receiver now checks positive exact
length/EOF and expected workload identity under the lease before epoch allocation
or JIT. Tests include a real socket stream through admission/retained execution,
stalled-input cancellation/exclusivity and absolute deadlines. The
[opt-in HTTP load profile](../protocol-scientific-load-v1.md) now consumes it.
The [durable load-receipt journal](../run-load-receipts-v1.md)
now provides versioned intent/history, bounded exact replay/conflict handling and
restart recovery to Indeterminate. A real-Component socket-body test retains its
acceptance after unload/reopen. `RunningWorker` now explicitly installs and retains
the internal coordinator once, with detached admission, exact-identity run lookup,
independent bounded receipt projections and shutdown/drop revocation. Real
Components prove lost-response survival, new epochs after unload and a retained run
after final receipt IO failure. Explicit scientific enablement now installs the
coordinator before serving and exposes authenticated complete-upload, receipt
lookup and retained-run discovery. The shared client now submits and retrieves
bounded correlated facts, with actual worker/restart evidence. Reusable hot-path
storage, runtime membership, durable formats, public run control/observations
and Kagami adapters remain open.

Contracts: [architecture](../architecture.md), [workloads](../workloads.md),
[workload protocol](../protocol-workload.md),
[ADR 0009](../adr/0009-execute-workloads-as-sandboxed-portable-programs.md) and
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md).

## Prerequisites and bounded slices

Apply [ADR 0027](../adr/0027-plugin-contributions-and-immutable-releases.md): the
engine must also be embeddable behind Kagami's local runtime interface without
formation or a separate Kagami solver. Coordinate this interface with K-PREVIEW/
K-RUN's cluster proxy. Authoring initialization/reinitialization uses an isolated
local invocation even with a cluster target; return state to the document
authority, never mutate a document or active run inside the engine. Prove
authoring initialization without a cluster connection.

1. Review current public types and callers; freeze the run lifecycle, workload/
   run/epoch/boundary identity, command outcomes and bounded failure transitions.
   Coordinate S-IDENTITY/S-OBSERVE, O-CLIENT and O-STORAGE contracts before
   their integration. Record any new authority or persisted/wire choice in an
   ADR; this task does not select an undocumented state machine.
   **Implemented prerequisite:** the existing formation owner now issues one
   generation/sequence-bound execution lease only for a locked, single-member
   standalone formation with no pending join. It prevents conflicting topology
   intents and revokes before shutdown acknowledgement. Off-owner work retains
   the slot until actual release; cancellation is not permission to overlap jobs.
   See ADR 0028 for rejected alternatives, scope and real-owner race tests.
   Scientific adoption/commit must still recheck that lease inside its serialized
   transition; a boolean liveness check followed by a separate write is insufficient.
2. Independently validate the complete workload closure, compatible graph/ABI,
   dimensions, limits and execution requirements before admission. Reject
   malformed, incomplete or incompatible submissions atomically; enforce at
   most one workload at a time, without adding a queue or scheduler.
   **Internal adapter implemented:** byte-bounded portable input, complete shared
   closure/Component validation, frozen digest denial policy, off-owner execution
   and a reserved-capacity confirmation checked by the formation owner. Public
   input IO budgets/delivery and distributed/reset allocation remain; do not expose the staged
   admission handoff itself as a completed load operation.
3. Compose the owner with O-WASM. The host executes the admitted component plan;
   the runtime owns time, cancellation and run-level publication. Validate the
   entire candidate before advancing the committed boundary. Traps, exhaustion,
   missing output or cancellation cannot expose partial committed state.
   **Internal retained executor implemented:** one non-queuing operation slot,
   reserved-capacity boundary-zero/step/stop publication through the formation
   owner, terminal unload and isolated bounded field leases. Lost publication
   coordination terminates the execution lifetime. Shared `advance_with_commit`
   keeps private state unchanged on gate refusal and cannot roll back after gate
   acceptance. A bounded Unix durable identified receipt journal is implemented
   separately from this owner; it does not persist scientific state or retain runs.
   Internal daemon admission/receipt/run ownership is now composed and retained
   by `RunningWorker`; request loss does not discard it. Continuous control,
   distributed/reset allocation and aggregate/hot-path hardening remain open.
   Opt-in startup and public load/retrieval are implemented; public stepping,
   stop/unload and observations are not. The shared load/retrieval client is
   implemented; Kagami adapters remain open.
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
