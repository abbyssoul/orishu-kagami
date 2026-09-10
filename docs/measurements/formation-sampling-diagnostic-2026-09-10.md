# Paced sampling CPU diagnostic — 2026-09-10

Status: **bounded entropy-cache optimization verified at three workers;
full overhead/scaling and final M4 acceptance remain open**.
Owner: [M4 checklist](../tasks/cluster-formation-m4-checklist.md).
Preceding work: [formation repair](formation-adaptive-repair-2026-09-10.md)
and the [failed fixed-rate curve](formation-fixed-rate-2026-09-10.md).

## Problem, alternatives and evidence

The failed curve reported default-sampling CPU overhead medians of 10.52% and
11.70% at three/ten workers, above the normal below-10% goal. Its baseline-p95
variation also prevents acceptance. Default sampling is only 1,000 ppm (0.1%),
but it draws cryptographic random bytes for every eligible operation. The
investigation separated that producer cost from logging/export and executor
sizing before changing production code. No host policy or accepted workload
was changed, and the failed curve was not resumed.

### Real-worker controls

Two separately named diagnostic sequences each used eight fresh three-worker
formations, the actual fixed 500 requests/s/worker load, unchanged setup and
rate-delivery gates, reversed-order controls and private artifact snapshots.
Each sequence had a 300-second cap and stopped on failure without retry.
Start/end thread inventories covered the measurement adapter, including its
warmup and pre/post checks; these coarse per-thread ticks are not the precise
ten-second process-CPU bracket used for comparisons.

The first sequence used the normal automatic twenty executor threads per
worker. Conditions were runtime-off, zero sampling, default sampling, default
sampling with logging disabled, then the reverse order. All eight operational
paths completed in 104.963 seconds. The two runtime-off CPU values were
2.512/2.625 seconds; default sampling was 2.734/2.732 seconds. Disabling logging
gave 2.805/2.745 seconds, not an improvement. Log-writer thread CPU did not
advance at the 10 ms tick resolution. Only 10–20 spans arrived in the default
windows. This rules out log writing as the dominant cause here, not all log
overhead or full-sampling loss. Logging-disabled cases intentionally do not
satisfy the normal log-delivery contract and are never acceptance cells.

The second sequence set `TOKIO_WORKER_THREADS=2` only for its private worker
children; the recorded inventories verify two threads per worker. It completed
in 104.559 seconds. Runtime-off CPU fell to 2.358/2.320 seconds, while default
sampling stayed at 2.734/2.716 seconds. Relative overhead became worse. Neither
production defaults nor the acceptance profile were changed to this setting.

Artifacts under `/tmp/orishu-curve-check.so7Ibv/`:

- `sampling-cpu-diagnostic-20260910`, runner `/tmp/orishu-sampling-cpu-diagnostic.py`.
- `sampling-cpu-two-threads-20260910`, runner `/tmp/orishu-sampling-cpu-two-threads.py`.

Each retains the complete fixed case list, host observations, source inventory,
runner/executable hashes and cleanup disposition. Both sequences preserved
source/artifact identity and cleaned up all owned children.

### Sampler and instruction profiles

Two ignored, bounded release-only diagnostic tests call the real `SpanQueue`
producer. Neither constructs an exporter or log sink. The tight-loop test runs
200,000 operations per condition, drains its receiver synchronously, and uses
zero/default/full followed by reverse-order controls. Default calls averaged
1.64–1.78 µs, versus 9–14 ns at zero sampling. At 15,000 operations per window,
that alone predicts only about 25 ms of added work. A syscall trace of the same
1.2-million-operation diagnostic recorded only 199 `getrandom` calls, not a
syscall per sampled or unsampled request. The retained provider is already a
stateful cryptographic implementation; no provider or allocator was replaced.

The paced test uses two spawned producers, 1,000 calls each at four-millisecond
intervals with two-millisecond phase separation. It retains per-call duration
summaries and uses the normal skip-missed-ticks policy, with bounded vectors
and task waits. At the same aggregate 500 calls/s, default call medians were
**8.618/8.173 µs**, and p95 **14.429/14.454 µs**. Zero medians were 0.156/0.139 µs.
This shows why the tight-loop estimate understated the paced producer cost.
Durations include scheduling/preemption, not isolated thread CPU: the first
default case also retains a 33.081 ms maximum, rather than discarding it.

Callgrind then profiled role zero in two actual fixed-rate cells (default and
zero sampling), with ordinary peers. Instruction collection covered only the
measurement adapter, excluding formation/startup. Both **failed** the unchanged
99% rate-delivery gate under instrumentation: the profiled worker completed
4,818/5,000 and 4,848/5,000 arrivals. They are retained instruction diagnostics,
not accepted latency/CPU comparisons or silently relaxed passes. Exact
post-window formation and cleanup succeeded; source/artifacts were unchanged.

Default sampling recorded 281,586,083 instructions; `RAND_bytes` accounted for
12,439,402 inclusively (4.42%). The export-loop root accounted for only 237,768
(0.084%), providing no evidence of an exporter busy loop. Zero sampling recorded
271,067,063 instructions and only 159,392 in the sampling entry point. Instruction
counts are not native CPU time, and cryptographic hardware behavior may differ
under instrumentation. Do not add inclusive parent/child counts together.
The export-loop root does not include every separately spawned HTTP connection
task; its percentage is not a measurement of total export-related CPU.
The profile also shows HTTP/CBOR processing and allocation work, consistent
with the earlier allocation diagnostic, but not proof of a peer-memory leak.

Artifacts:

- `/tmp/orishu-sampler-profile.J8dQ8q/`: `native.jsonl`, `syscalls.txt`,
  `paced.jsonl` and the later `paced-cached.jsonl`.
- `/tmp/orishu-curve-check.so7Ibv/sampling-callgrind-default_sample-20260910/`.
- `/tmp/orishu-curve-check.so7Ibv/sampling-callgrind-zero_sample-20260910/`.
- Instruction-profile runner: `/tmp/orishu-profile-sampling-callgrind.py`.

The profile runners have explicit CPU/file/lifetime caps, target only their
private child and retain the failed rate assertions. No profiling-security,
affinity, governor, system-service or allocator change was made.

## Retained optimization and tradeoffs

`SpanQueue` now owns an optional shared cache of **64 fresh 28-byte random
blocks**: 1,792 payload bytes plus index, mutex, reference-counting and allocator
bookkeeping. It is allocated once only when the configured sampling rate is
nonzero. Producer clones share it. Every block is consumed once, including
when sampling excludes the operation. Sampling threshold, root/child identity
construction, remote-parent authorization and error/shedding counters retain
their existing semantics. Cryptographic bytes still come from the same pinned
provider; no deterministic/noncryptographic sampling source was substituted.

Refills amortize provider invocation over 64 operations. Queue-local access uses
`try_lock`; contention or poisoning falls back to the original direct-generation
path rather than waiting for another producer. No lock crosses an await point.
A partially failed refill remains exhausted until a complete fresh fill
succeeds, preventing stale/partial blocks from being consumed. Zero sampling
still acquires no entropy or clock and retains no random cache.

This trades small fixed storage and occasional larger refill work for fewer
provider invocations. It adds no dependency, public setting, wire field,
authority or migration requirement. It is not a claim of zero provider-internal
allocation, zero refill latency or a hard real-time cryptographic primitive.

A failing regression first observed 130 provider calls for 130 operations;
after the change it observes three, with distinct test identities across
producer clones. Additional tests cover partial refill failure/recovery,
non-waiting contention fallback and absence of storage at zero sampling.
Existing tests retain local head-rate limits, parent/collision validation,
zero/closed fast paths, active/queue bounds, cancellation and serialized OTLP.

The paced candidate's default medians are **0.614/0.757 µs**, p95
**1.979/1.908 µs**. Zero medians are 0.366/0.276 µs. The first default case
retains a 30.976 ms maximum; these are not clean average-CPU estimates. Native
end-to-end comparison below, rather than microbenchmark speedup alone, is the
reason to retain the change.

## Fixed native old/candidate comparison

Eight new diagnostic cells ran in fixed order: old-off, old-default,
candidate-default, candidate-off, then reverse order. Both versions include
the preceding formation repair and both telemetry features. Only the candidate
adds the random cache. The normal twenty-thread default and actual fixed-rate
workload are retained. All cells pass the existing reader's workload, scrape,
receipt/loss, resource and window-validity checks, with clean cleanup and
unchanged source/artifacts. No build or other test ran alongside them.

| Condition | Aggregate worker CPU / 10 s, two observations |
| --- | --- |
| Old, runtime off | 2.497 / 2.629 s |
| Old, default sampling | 2.858 / 2.881 s |
| Candidate, runtime off | 2.567 / 2.630 s |
| Candidate, default sampling | 2.746 / 2.679 s |

Pair default with its same-version, same-half runtime-off control. Median
CPU-per-completed-request overhead is **12.05% old / 4.41% candidate**; the
candidate's two comparisons are 6.96% and 1.85%. Worst-role median request-p95
overhead is **5.18% old / 4.46% candidate**. Default-mode aggregate CPU averages
2.870 versus 2.712 seconds (about 5.5% less). Every worker completes at least
4,998/5,000 arrivals. These are two diagnostic comparisons per version at three
workers, **not six acceptance rounds or a 3/10/30-worker qualification**.

Artifacts: `/tmp/orishu-curve-check.so7Ibv/sampling-cache-comparison-20260910/`;
runner `/tmp/orishu-sampling-cache-comparison.py`. Elapsed: 102.930 seconds.
Old combined worker SHA-256:
`957bf25e6162b7fe85176dd2cdd9334870a97e346e26fa7fe82f01d0f79922a9`.
Candidate combined worker SHA-256:
`b3f7db9b2bbb58f571e0e1bcc1f752d8562e5fa725a2c724c19a303fd7a95371`.
Frozen CLI/probe identities match the preceding formation-repair report.

## Validation and remaining M4 work

- Combined-feature library: **229 passed, 0 failed, 5 ignored** (three manual
  profiles, chain diagnostic and subprocess helper), 27.17 seconds.
- Combined-feature all-target Clippy with warnings denied and release builds
  passed. The diagnostic test's initial missing `Duration` import was fixed
  before execution; the intended provider-call regression was red, then green.
- Official Collector 0.160.0 verified A→B→C formation, both causal admission
  chains, **116 matched spans/logs**, 172 series/worker, probes and policy checks
  using the candidate release worker. No trace/log field contract changed.
- The isolated formation-fault all-target suite passed **244 tests / 2 ignored**,
  and Clippy/build passed. It excludes tracing and therefore does not exercise
  the random cache. The separate fault-process results are recorded below.
- All four worker all-target configurations pass tests and Clippy: neither
  feature **245 passed / 2 ignored**, metrics **290 / 2**, tracing **286 / 5**,
  and both **342 / 6**. The combined ignored set includes the separate legacy
  overhead test in addition to the five library helpers/diagnostics. Actual
  held-output/recovery and mixed-feature peer checks ran.
- Workspace all-target Clippy/tests, doctests and the release fault-exclusion
  guard pass. The existing `query_filter!` doctest remains ignored. Formatting,
  whitespace and documentation checks pass (137 Markdown files).
- All ten existing isolated fault-process scenarios passed once, with no timer
  changes or assertion retries: lost join ACK, peer ejection, lost departure,
  issuer loss/ejection, source loss, dead/removed/blocked assignment recovery
  and excluded restart. Source uncertainty and operator-stop outcomes remained
  explicit; no fresh identity or operation could bypass certificate exclusion.
- Plain formation/churn, client-pressure and lost-leave-response process
  journeys passed once. The preceding policy-partition journey uses the
  identical feature-omitted worker. The [post-diagnostic checkpoint](../tasks/cluster-formation-m4-checklist.md#post-diagnostic-correctness-checkpoint--2026-09-10)
  reconciles these results and operator-recipe applicability; it does not
  reclassify prior performance failures or claim all deployment recipes reran.

No full acceptance batch was retried or reclassified. Baseline-p95 variation,
full-sampling cost/loss, ten/thirty-worker overhead and final M4 acceptance
remain open. No runtime thread-count or threshold adjustment is retained.

The preceding diagnostic debit was 3,424.486 seconds. This increment adds
1.43/1.49 seconds for native/syscall sampler tests, 104.963/104.559 seconds for
the two control sequences, 14.326/15.282 seconds for the retained failed
instruction profiles, 16.43 seconds for each paced test and 102.930 seconds
for the native version comparison: **3,802.326 seconds total**, about 63m22s
of the approved 90 minutes excluding builds. About **26m38s** remains.
Correctness regressions and the functional Collector walkthrough are recorded
separately, not as timed-load or profiler experiments.

A fresh full curve is estimated at roughly 40–50 minutes. The checkpoint
therefore proposes explicit approval for a 120-minute total experimental cap
and one new batch capped at 55 minutes. Neither extension nor batch is approved
by this report; the unchanged full-curve gate remains open within the current
remaining allowance.
