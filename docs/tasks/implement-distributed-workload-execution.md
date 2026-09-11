# Implement distributed workload execution

Status: **specified; protocol and scientific-profile gates remain open**.
Owner: **N-CLUSTER**, with O-RUNTIME and X-DIST-PROFILE.
Target: **M5**, after a proven single-node reference and M4 formation transport.

## Outcome and current gap

Distribute the same admitted component graph while preserving one coherent,
validated scientific result. Peer membership is implemented; it is not partition
ownership or distributed simulation commit. Extend the
[single-node runtime](implement-single-node-workload-runtime.md), consuming
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md), the
[workload protocol](../protocol-workload.md) and [architecture](../architecture.md).

## Decisions and prerequisites

Before wire implementation, review/version partition ownership fences, epochs,
step votes/commit, deterministic reductions, incomplete-boundary recovery and
ownership transfer. Define finite message/queue/time/resource bounds and the
failure model; record costly decisions in the relevant ADR/protocol. Formation
liveness alone is not authority to commit or transfer ownership.

X-DIST-PROFILE must specify concrete schemas, limits, fixtures, a single-worker
reference and numerical tolerances. [ADR 0016](../adr/0016-first-distributed-scientific-profile.md)
is a proposal for revalidation, not an already accepted numerical profile.
Confirm that profile or explicitly supersede it before scientific acceptance.
O-STORAGE/N-TRANSFER/N-PURGE/N-ARTIFACT remain separately owned dependencies.

## Bounded slices

1. Model ownership, epoch and candidate/commit transitions with deterministic
   replay tests for stale, duplicate, reordered, missing and cross-identity
   events; then exercise the same transitions through real serialized IO.
2. Place component-instance partitions and exchange bounded reliable halo and
   typed channel data through authenticated peers. Guests never call peers or
   one another. Keep these transfers separate from supersedable observations.
3. Execute the admitted plan with deterministic reduction/order and atomic
   boundary acceptance. Show identical semantics with co-located and separated
   component instances; no entry node becomes an independent authority.
4. Implement reviewed cancellation, worker-loss, fenced ownership transfer,
   checkpoint/resume and rebalancing paths. Incomplete/uncertain steps cannot be
   reported committed. Where emitters are included, verify spawn identity and
   checkpoint state against duplicate/lost emission under retry and transfer.
5. Validate the selected scientific profile at 1/3 workers and then 5 workers
   with churn, including checkpoint/resume and partition changes. Feed results
   to [P-SCALE](implement-staged-runtime-scaling-evidence.md), and add bounded
   workload/ownership telemetry and operator failure/recovery instructions.

## Acceptance criteria

- Same workload closure and execution profile preserve the declared numerical
  equivalence across supported partition layouts, completion orders and recovery.
- Real wire/process tests reject stale fences, duplicate commits, conflicting
  identities, malformed/oversized transfers and missing required contributions.
- Failure either follows the reviewed recovery path or stops explicitly at the
  last committed boundary; it never manufactures a partial scientific result.
- Relevant O-RUNTIME/O-WASM, membership, wire and hostile-input checks pass;
  telemetry and observers remain bounded and independent of commit authority.
- M5 disposition links scientific, failure and scaling evidence plus tested
  operator stories/manuals; proposed ADR status changes require that evidence.

## Non-goals

General scheduling, a second workload model, authoring collaboration, an
unverified Maxwell/Yee acceptance, implementing all artifact services inside
N-CLUSTER, production cloud qualification or 10,000-worker scaling claims.
