# Implement staged runtime scaling evidence

Status: **specified; staged implementation gated on the measured services**.
Owner: **P-SCALE**, with O/N owners supplying correctness fixtures.
Targets: post-M4 lab work in early M5; scientific 1/3/5-worker evidence in M5;
complete 1/3/5/12/32-worker release evidence in M8.

## Outcome and current gap

Deliver reproducible compute, capacity, storage, retrieval and churn evidence
against the [scaling objectives](../orishu-scaling-objectives.md). Existing
formation and Pi summary-API harnesses are reusable infrastructure, not a
scientific benchmark or the complete P-SCALE implementation. Preserve their
measured results and limitations, including generator cost, mixed hardware,
thirty-worker noise and full-sampling cost/loss.

## Bounded slices

1. Inventory the existing [physical experiment](implement-physical-formation-experiment.md)
   and local harnesses. Define a versioned evidence manifest and bounded reader
   covering source/binary/configuration hashes, topology, workload identity,
   worker/generator placement, clocks, offered/achieved load, errors and cleanup.
   Reuse retained evidence without relabelling API requests as simulation steps.
2. Complete the physical worker-placement recheck and consume the
   [executor review](review-worker-executor-performance.md). Before each new
   experiment, select one question, comparison, topology, resource/time budget,
   validity gates and stop condition. No inherited unlimited experiment budget,
   automatic retry or host-policy change follows from this task.
3. Once O-RUNTIME/N-CLUSTER and the selected scientific profile have reference
   fixtures, measure the same immutable workload at 1/3 workers and then 5
   workers with churn. Separate correctness, fixed-rate overhead and saturation
   profiles. Compare three identical workers against a competent single-worker
   baseline; the mixed Pi fleet is not an equivalent-hardware speedup control.
4. Extend evidence as O-STORAGE/N-ARTIFACT and observation services land:
   memory capacity, bounded artifact streaming, replication/repair, retrieval
   and observer pressure. Validate logical large-artifact proxies explicitly;
   do not claim unexecuted physical terabyte tests.
5. Complete 12/32-worker stages in a declared lab/scheduled environment for M8.
   Report useful steps, numerical equivalence, CPU/memory/network, coordination,
   storage and generator costs, telemetry configuration/loss, and recovery.
   Publish a sanitized report and reproducible commands; private inventories,
   credentials and raw sensitive artifacts stay outside the repository.

## Acceptance criteria

- Each stage has exact identities, validated results, bounded resource/reader
  behavior and independent cleanup evidence; failed and inconclusive runs remain
  visible. Missing stages cannot be marked passed.
- The `2 * y < x` three-worker research target is measured and explained if
  missed; it is not a substituted correctness gate or a promised speedup.
- Comparison boundaries and instrumentation cost are explicit. No performance
  improvement is accepted by changing scientific semantics or moving costs out
  of the measured boundary.
- Update scaling reports, operator capacity guidance, the
  [follow-up register](../roadmap/README.md#post-m4-operational-follow-ups) and
  M5/M8 evidence dispositions. Release claims match actual supported profiles.

## Non-goals

Reopening accepted M4 measurements, proving 10,000-worker scaling, choosing an
executor policy without evidence, implementing numerical models, or completing
the runtime/storage services merely to make a benchmark pass.
