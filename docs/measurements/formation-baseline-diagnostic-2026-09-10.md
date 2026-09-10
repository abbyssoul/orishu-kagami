# Three-worker slowdown diagnostic — 2026-09-10

Status: **slowdown reproduced; thermal throttling observed; no overhead acceptance**.
Plan: [approved diagnostic](formation-telemetry-plan.md#approved-baseline-diagnostic--2026-09-10).
Owner: [formation M4 checklist](../tasks/cluster-formation-m4-checklist.md).

## Problem being solved

The previous curve's three-worker baseline fell from about 128k to 75k
requests/second, making paired instrumentation comparisons unreliable. Determine
whether merely maintaining a small formation is expensive, or whether the
continuous API workload, host conditions, allocation churn, or scheduling cause
the change. Preserve correctness and the original failed experiment.

The timed workload is **not three idle workers**. Six clients continuously
request the public cluster summary over Unix sockets, without rate limiting.
The workers also maintain authenticated QUIC peer membership. Each worker uses
its default 20-thread Tokio executor on this machine; the separate load
generator uses two executor threads. This is a colocated control-plane stress
test, not a simulation workload, isolated network benchmark, or fleet SLO.

## Options tried and evidence

### Six fresh baseline cells

Ran the real formation/load/cleanup path once through six `compiled_off` cells,
with 500 ms read-only host/thread observations. All six completed, with exact
formation/membership/policy checks, clean shutdown, unchanged source and frozen
binaries. No cell was retried, discarded, or promoted to overhead acceptance.

| Cell | Requests/s | Worst-role p95 (µs) | Worker CPU/request (µs) | CPU0 median sampled GHz | Peak coretemp (°C) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 126,695.6 | 74.765 | 15.08 | 3.80 | 85 |
| 1 | 128,893.0 | 72.348 | 15.09 | 3.80 | 92 |
| 2 | 127,265.2 | 73.167 | 14.65 | 3.80 | 100 |
| 3 | 124,743.1 | 75.501 | 15.28 | 3.80 | 100 |
| 4 | 104,494.8 | 125.739 | 18.49 | 3.66 | 100 |
| 5 | 62,615.6 | 156.992 | 29.40 | 1.85 | 92 |

Throughput max/min is **2.058**; per-role p95 max/min is **2.167–2.239**,
well outside the unchanged 1.10 baseline-variation limit. The slow phase occurs
despite fresh worker processes and disabled telemetry in every cell.

Evidence distinguishing the hypotheses:

- **Thermal/frequency:** CPU0's package-throttle counter increments by
  0, 0, 129, 543, 444, 0 across the six observation windows. Its core-throttle
  increments are 0, 0, 3, 10, 19, 0. Do not sum package counters exposed through
  different CPUs: they can represent the same package events. CPU12's median
  sampled frequency also falls from about 2.6 to 1.4 GHz. Throttling precedes
  the slow phase; temperature then drops while frequency remains lower.
- **Memory:** worker RSS peaks remain 13,780–14,084 KiB. No swap or major
  faults; only 126–184 aggregate worker minor faults per observation window.
  Fresh processes and stable RSS contradict cross-cell retained-worker-state
  growth, but RSS alone cannot rule out short-lived allocation churn.
- **Scheduling:** aggregate worker runnable-queue wait is about 0.09–0.15 s
  per window, compared with 18.17–19.15 s of scheduled execution in the sampled
  brackets. There is no large growth in absolute runnable wait. Work occurs
  across many CPUs; last-CPU snapshots are not migration counts or exact
  time-on-core attribution. Voluntary switches decline with throughput rather
  than accumulating across cells.
- **Measurement:** zero missed 50 ms resource samples; CPU brackets are
  10.0007–10.0026 s. The load generator consumes 19.26–19.37 CPU-seconds per
  ten-second window, roughly two cores, in addition to roughly two cores of
  worker execution. It is a significant co-located heat/load source and can
  itself limit the scalability experiment.
- **Observer cost/limits:** runner CPU is 0.35–0.64 s/window. Each additional
  snapshot takes about 46–57 ms wall time on average, mostly readable sensor
  access, at 500 ms intervals. This diagnostic is not an uninstrumented timing
  comparison. `scaling_cur_freq` is sampled kernel reporting, not an APERF/MPERF
  effective-clock measurement. `perf` was unavailable with
  `perf_event_paranoid=4`; no security, governor, affinity or host-service policy
  was changed.

**Interpretation:** an observed thermal episode followed by reduced sampled
frequency is the best-supported explanation for the common throughput drop.
This is stronger than inference from temperature alone. It does not establish
which firmware/OS power-control mechanism sustains the lower clocks, nor
isolate thermal effects from every scheduler or allocation cost.

### Allocation profile of the actual worker path

Used installed Valgrind 3.25.1 DHAT on role 0 only, with two ordinary peers,
the same formation predicates and real ten-second typed-client load. One
attempt passed with clean cleanup and unchanged source/artifacts. Limits:
180 s outer budget including a 15 s cleanup reserve, 120 s profiler CPU,
32 MiB per output file, 24-frame allocation stacks. No production allocator,
dependencies, wire format or runtime code changed.

The whole-process profile includes startup, admission, warmup, 22,035 timed
summary requests, post-window checks and shutdown. It records **334,912,740
allocated bytes in 963,815 blocks**, a **3,606,251-byte peak live heap**, and
27,144 live bytes at exit. This is churn, not evidence that a 335 MB heap is
retained. DHAT allocation/resize accounting is not an exact malloc-only count;
its instruction-based lifetimes are not native wall-clock timings.

Dominant allocation stacks, aggregated by top frame:

| Stack / operation | Allocated bytes | Blocks | Interpretation |
| --- | ---: | ---: | --- |
| `BytesMut::reserve_inner` under Hyper HTTP/1 reads | 181,805,056 | 22,193 | Repeated 8 KiB read-buffer allocations; about 54% of whole-profile bytes. |
| Salvo `HyperHandler::call` | 74,568,480 | 22,193 | Per-request HTTP service/future storage; about 22% of bytes. |
| All stacks containing `ClusterSummaryHandler` | 47,309,940 | 465,780 | Includes the following rows; do not add them again. |
| Summary-path buffer growth | 23,954,400 | 177,440 | Eight allocation/resize events per 22,180 observed handler calls, including CBOR output growth. |
| Summary-path string clones | 6,121,680 | 133,080 | Six per observed handler call, including view cloning and serialization. |

These are genuine targets for subsequent allocation-reduction work. The
profile does **not** prove allocator contention causes the time-dependent
cliff, measure native allocation CPU overhead, or establish a peer-network
allocation bottleneck. Most measured allocation volume is in client HTTP
handling. The low retained heap agrees with the native RSS observations.
Connection churn must not be inferred merely from the read-buffer count.

### Idle-cluster control

One additional fresh formation, same identity/readiness/policy gates, followed
by a 20.063 s observation without load clients. CPU used by the three workers:
**0.06, 0.04, 0.04 seconds** — 0.14 s total, approximately **0.7% of one core**.
RSS ends at 13,816/13,916/13,832 KiB. Package temperature is 54°C at the first
sample and 53°C at the last. Exact membership/readiness remained valid; shutdown
was clean. This supports inexpensive idle peer maintenance at three workers,
not a general scalability or idle-telemetry-overhead claim.

### Fixed preconditioning was also tried

Used the remaining diagnostic allowance for nine predeclared ten-second load
windows on one formation, then three fresh observed baseline cells. Whole
control cap: 170 seconds, reserving 15 for cleanup. All warmup windows were
retained, with no assertion retry, host-policy change or exclusion of the
original six cells. Warmup gates use the actual curve path; the brief identity
checks between windows remain part of the elapsed experiment time.

Conditioning completed in 95.956 s; its last four windows sustained about
59k–63k requests/s. The following fresh baselines were:

| Cell | Requests/s | Worst-role p95 (µs) | Worker CPU/request (µs) | CPU0 median sampled GHz |
| --- | ---: | ---: | ---: | ---: |
| 1 | 70,990.7 | 145.222 | 26.92 | 2.00 |
| 2 | 69,344.1 | 150.739 | 27.70 | 1.92 |
| 3 | 91,904.3 | 126.519 | 20.38 | 2.50 |

Throughput max/min **1.325** and per-role p95 max/min **1.153–1.195** still
fail the unchanged 1.10 stability threshold. Frequency and throughput recover
together rather than settling at one lower level. Correctness, cleanup and
source/artifact identity all pass, but a fixed 90-second conditioning allowance
has **not** solved measurement stability. It is not enough to add this warmup
to the full-curve recipe and declare the host controlled.

## Options to try next and trade-offs

1. **A fixed request-rate workload (recommended)** could measure instrumentation overhead
   under realistic monitoring traffic with less heat. It changes the measured
   workload and cannot substitute for maximum-throughput/scalability evidence;
   make that distinction explicit before adopting it for acceptance.
   The profile should declare offered rate per worker, keep the 3/10/30 sweep,
   and verify that the generator actually sustains the rate without building
   an unbounded queue. CPU accounting must resolve small changes at the lower
   load; the current 10 ms `/proc` tick quantum needs explicit consideration.
2. **A thermally stable dedicated host or external load generator** better
   separates worker scaling from shared-host limits, but requires external
   resources/access and a newly recorded environment. It preserves the
   saturation question, subject to a fresh stability check.
3. **A deliberately constrained host power/frequency profile** might make
   this machine repeatable at lower sustained speed. It affects machine-wide
   behavior, needs an exact reviewed configuration and privilege, and changes
   the conditions of the claim. Simply selecting a performance governor is
   not demonstrated to solve thermal instability. No such change was tried.
4. **Targeted allocation reduction**, starting with request/response storage
   ownership and identity serialization, can reduce necessary work. It needs
   a production-call-site regression and a controlled before/after profile;
   avoid changing allocators or peer protocols speculatively. It does not
   remove the requirement for a stable host comparison. With an unpaced load,
   more efficient requests can raise throughput while clients remain busy;
   allocation reduction alone need not reduce sustained host heat.

Budget redistribution is feasible as **20 minutes for 3 workers (including
causal preflight), 20 for 10, 45 for 30, and 5 reserved for diagnostics**. This
totals 90 minutes; builds remain excluded. The old 30-worker setup observations
already predict roughly 34–37 minutes for 36 cells before other overhead, so
45 minutes provides useful headroom but is not a completion guarantee. The
current full-curve runner still implements equal 30-minute slices; a revised
profile and its deadline tests must be implemented before using the new split.

No full curve was started: budget redistribution alone cannot repair a 2×
baseline shift. Normal <10%, temporary ≤20%, and >25% troubleshooting bands
remain unchanged. No thermal-unstable cells have been relabeled accepted.

## Actions required from the operator

**Resolved on 2026-09-10:** the operator approved fixed-rate traffic for the
acceptance curve. The [v3 profile](formation-telemetry-plan.md#fixed-rate-acceptance-profile-v3--2026-09-10)
records the implementation parameters. The decision request below is historical,
not a request to repeat that approval.

The approved diagnosis and budget exploration are complete. **The remaining
decision is about the experiment, not another timeout extension:** approve
rate-controlled instrumentation acceptance on this host (recommended), or
retain saturation as the acceptance workload and arrange controlled execution
conditions. A fixed-rate result cannot be labeled peak-throughput or capacity
evidence. If host-policy changes are preferred, review the exact proposed
settings and their system-wide impact separately before applying them.
No additional approval is needed for the already-granted 90-minute budget
redistribution, and no threshold relaxation is requested.

## Reproduction, accounting and retained evidence

Runtime source/dependency inputs match the previously verified release worker;
the checkout HEAD is `b52dc1a` (the unrelated Kagami changes are not a worker
rebuild). Worker SHA-256:
`e63717c4bb0d87a0423047b2dd889a32e0614759ffe1ce2a02fbdcabcdd86606`.
Full source inventories and executable hashes are retained in each result.

```sh
python3 scripts/diagnose-formation-baseline.py \
  --worker target/formation-telemetry-enabled/release/orishu-worker \
  --ctl /tmp/orishu-curve-check.so7Ibv/full-quiet/ctl \
  --probe /tmp/orishu-curve-check.so7Ibv/full-quiet/probe \
  --output /tmp/orishu-curve-check.so7Ibv/baseline-diagnostic-20260910
python3 /tmp/orishu-profile-formation-allocations.py
python3 /tmp/orishu-idle-formation-diagnostic.py
python3 /tmp/orishu-conditioned-baseline-diagnostic.py
```

The three `/tmp` scripts are explicitly temporary diagnostic helpers, retained
with the local evidence, not installed product tooling. Output directories are
create-new; reproducing requires new explicit paths, never overwriting results.
Raw `/tmp` artifacts are machine-local and not durable release evidence.

Under `/tmp/orishu-curve-check.so7Ibv/`:

- `baseline-diagnostic-20260910/`: manifest, six cell results, 21 observation
  files per cell, completion ledger and frozen executables. Elapsed **80.573 s**.
  Manifest SHA-256:
  `6e6727cd047bc677c63135de63f4e7ea7d633517f0a95683f69199231e805ad1`.
- `allocation-diagnostic-20260910/`: result, bounded Valgrind log and DHAT heap
  profile (1,245,908 bytes). Elapsed **16.070 s**. Heap SHA-256:
  `e44c3d2848d0db9dd5dc21c519c248e9c7d0cd0aec182747911354738ea78f65`.
- `idle-diagnostic-20260910/`: result and 41 observations. Elapsed **24.422 s**.
- `conditioned-diagnostic-20260910/`: manifest, nine warmup-window records,
  conditioning result, three fresh observed-cell results and completion ledger.
  Elapsed **137.927 s**.

Total measured diagnostic execution **258.991 s**, within the approved
five-minute allowance. Preparation, analysis and documentation are not load
execution. Conservatively keep the full five minutes reserved, leaving 85
minutes for a future curve rather than silently spending the unused remainder.
All fixture children were reaped; no worker/profiler processes remain. Original
failed-curve artifacts were not modified.

Validation: four new observer contracts, 29 existing curve contracts and
`make docs-check` pass. No runtime code changed, so full Rust builds/tests and
already-verified operator recipes were not repeated. The performance-stability
gate fails observationally; neither allocation improvement nor M4 completion
is claimed.
