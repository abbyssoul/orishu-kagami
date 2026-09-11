# Orishu runtime scaling objectives

Orishu is a runtime platform for one decentralized, data-parallel scientific
workload per cluster. Scaling is not merely an application benchmark: it is a
platform property that must be demonstrated across membership, coordination,
storage, retrieval, recovery, and hostile-input handling.

This document turns the capacity and performance motivation in
[Why Orishu exists](why-orishu-exists.md) into measurable evidence gates.

These objectives are targets, not claims about the current implementation.
They complement the scientific targets in `target-experiments.md` and the
delivery gates in `roadmap/README.md`.

Implementation is tracked in [P-SCALE](tasks/implement-staged-runtime-scaling-evidence.md):
scientific 1/3/5-worker stages target M5, with complete 1/3/5/12/32-worker evidence
for M8. The [post-M4 register](roadmap/README.md#post-m4-operational-follow-ups)
separately retains executor and physical capacity diagnostics; summary-API
measurements are not substitutes for the scientific stages below.

## Why Orishu scales

Orishu primarily exists to let a workload exceed one machine's compute, memory,
storage, and serving capacity while preserving one coherent execution model.
Reduced wall-clock time matters, but a cluster is also valuable when the state
or artifacts are too large for one node to compute, retain, replicate, or
serve. A valid design must therefore scale both computation and durable data.

A single worker is the degenerate case of the same runtime. Moving from one
node to many must not introduce a scheduler, a separate control plane, or a
different workload model.

## Long-term target

The research target is useful operation at 10,000 or more workers with
near-linear scaling where the workload's partitionability and communication
pattern permit it. At that scale:

- membership and failure-detection work must remain bounded per node;
- no permanent coordinator, metadata leader, or central scheduler may become a
  throughput or availability bottleneck;
- partition ownership, halo exchange, and step commitment must remain correct
  under delay, duplication, reordering, loss, and node churn;
- artifact placement, replication, discovery, and retrieval must not require
  one node to hold or serve the complete dataset; and
- malformed or hostile traffic must have bounded cost and must not disrupt the
  cluster.

Near-linear scaling is conditional, not universal. It is evaluated only for a
fixed scientific problem whose spatial decomposition, boundary exchange,
checkpoint cost, and numerical method can use the added capacity.

## Staged evidence

Scaling evidence should be collected at 1, 3, 5, 12, and 32 workers before
claims about larger clusters. Each stage must use the same accepted workload
closure, numerical profile, and result validation.

The first compute-efficiency gate compares three identical workers with one
worker. If `x` is the validated single-worker runtime and `y` is the validated
three-worker runtime for the same problem, the target is `2 * y < x`. This is a
research target rather than a release guarantee, but failure must be explained
with measured coordination, communication, storage, and fixed-cost overheads.

Measurements must report at least:

- workload identity, input size, partition count, precision, and execution
  profile;
- worker hardware and topology;
- elapsed time and useful simulated steps per second;
- CPU, memory, and network use per worker;
- halo, barrier, checkpoint, replication, and retrieval costs;
- result equivalence under the workload's declared tolerance;
- steady-state and churn/recovery behavior; and
- comparison with a competent single-worker baseline, not an intentionally
  weak implementation.

## Capacity and resilience gates

Compute speedup alone is insufficient. Tests must separately demonstrate:

- state capacity beyond one worker's practical memory;
- artifact creation and retrieval without whole-artifact buffering;
- terabyte-scale logical artifacts through bounded-memory streaming tests or
  representative validated proxies before production-scale runs are practical;
- continued or explicitly failed progress under worker loss;
- safe late join and ownership transfer at committed boundaries;
- restoration of the documented replication target after holder loss; and
- bounded membership, gossip, inventory, observer, and diagnostic work during
  churn.

## Claim discipline

The planned [worker observability instruments](orishu-observability.md) provide
bounded operational measurements for these harnesses. Record feature/runtime
enablement, trace sampling, dropped telemetry and measured instrumentation
overhead with each run; missing telemetry is not a zero-cost measurement.
Compare enabled and disabled configurations while holding scientific inputs
and validation fixed. Metrics/traces complement, not replace, result evidence.

Documentation and release material must distinguish architectural targets,
laboratory results, and supported guarantees. A result is not a scaling success
if scientific semantics changed, invalid output was discarded, durability was
disabled without disclosure, or the comparison moved work outside the measured
boundary.

The installable-release milestone requires representative scaling evidence.
The 10,000-worker target and near-linear behavior remain a post-release research
horizon until demonstrated with reproducible fixtures and published results.
