# Formation telemetry overhead: approved acceptance experiment

Status: **post-diagnostic batch completed all 108 cells; three/ten-worker normal
gates met, thirty-worker noise prevents full overhead acceptance**.
Owner: [P-OBSERVABILITY for M4](../tasks/implement-worker-observability.md).
This is the finite plan requested by the
[formation handoff](../tasks/implement-cluster-formation-poc.md#open-m4-work-selection),
not accepted performance evidence or permission to change runtime limits.
The [source-built recipe](../testing-worker-overhead.md#current-formation-scaling-experiment)
is implemented separately from the historical local test. Its six-mode smoke
matrix is harness evidence, not an accepted scaling result.
The [quiet-host run report](formation-telemetry-2026-09-09.md) now records all
36 three-worker cells, the first ten-worker setup failure and separate bounded
diagnostics. Thirty-worker performance and final overhead acceptance remain
unverified. The original formation/catch-up setup blocker has the scoped
follow-up below; the separate
[shutdown-log follow-up](formation-telemetry-2026-09-09.md#shutdown-log-follow-up--2026-09-09)
fixes reproduced final-event contention and verifies one real zero-sampling
load/shutdown. Timing noise, full-sampling cost/loss, budget feasibility and
full-curve acceptance remain open.

The [read-only baseline review](formation-telemetry-2026-09-09.md#baseline-variation-review--2026-09-09)
locates the major shift in the first round, across all three roles and before
full sampling first runs. It does not prove the cause or permit excluding that
round. On 2026-09-10 the operator approved the five-minute baseline diagnostic
and exploration of per-size budget redistribution (including up to 45 minutes
for 30 workers), within the next 90-minute total excluding builds. The
no-automatic-retry rule and acceptance thresholds remain unchanged.
The [completed diagnostic](formation-baseline-diagnostic-2026-09-10.md)
reproduces the shared slowdown with observed thermal throttling and lower
sampled clocks, confirms short-lived HTTP/serialization allocation churn, and
finds low idle peer-maintenance cost. Baseline stability remains the execution
gate; redistribution alone is insufficient. V3 implements the 20/20/45-minute
split plus five diagnostic minutes when `--fixed-rate` is selected.
A fixed 90-second load-conditioning control also failed to stabilize three
subsequent fresh baselines (1.325× throughput range). Total diagnostic execution
was 258.991 seconds. The operator subsequently explicitly approved fixed-rate
traffic for acceptance. The following v3 profile supersedes the historical
saturation workload and equal-size budgets below, not the retained results.

## Fixed-rate acceptance profile v3 — 2026-09-10

### Approved post-diagnostic batch — 2026-09-10

Execution is now recorded in the [post-diagnostic report](formation-post-diagnostic-2026-09-10.md):
all 108 cells completed in 46m07s outer time with unchanged source/artifacts
and clean cleanup. Formation succeeds at all sizes. Normal three/ten-worker
instrumentation meets reviewed gates; thirty-worker baseline-p95 variation
keeps its comparisons inconclusive. A separately instrumented host diagnostic
reproduces variation without observed thermal throttling and does not replace
acceptance. Total conservative debit is now 114m07s of 120 minutes, leaving
5m53s. There is no authorization to automatically repeat the completed batch.

Following the [correctness checkpoint](../tasks/cluster-formation-m4-checklist.md#post-diagnostic-correctness-checkpoint--2026-09-10),
the operator explicitly approved extending the total experimental allowance
from 90 to **120 minutes** and running the required activities. Run one fresh
full 108-cell matrix with `--fixed-rate --batch-cap-seconds 3300`, capped at
**55 minutes including causal preflight and cleanup**. Prior experimental
debit is 3,802.326 seconds; at most 3,300 additional seconds totals 7,102.326,
below the 7,200-second allowance. No build time is charged.

This explicit cap can only shorten the harness's existing batch allowance;
the original 20/20/45-minute per-size ceilings remain and do not add time to
the shorter whole-batch deadline. The historical 300-second diagnostic
reservation is already represented in prior accounting, not charged again.
No prior smoke or failed cell is reused. All 3/10/30-worker, six-mode,
six-round requirements, workload, setup and acceptance gates remain unchanged.
Freeze new artifacts/source before running; preserve every failure and do not
automatically retry or resume. No host-policy change is authorized.

### Earlier v3 attempt and diagnostic follow-up

The [v3 execution record](formation-fixed-rate-2026-09-10.md) retains harness
checks, the separate smoke matrix, executable identities and budget debit.
The attempted curve completed 36/36/11 cells at 3/10/30 workers, then failed
thirty-worker unlock under the original 60-second setup deadline. No retry or
resume occurred. Three/ten-worker normal overhead comparisons are inconclusive
under the unchanged baseline-p95 variation gate; default-sampling CPU medians
also exceed the normal below-10% goal. Thirty-worker comparisons are partial.
Separate formation-only timing diagnostics did not reproduce the unlock
failure, and a bounded harness fix now preserves partial timing evidence on
failure. These findings do not change the accepted profile or certify M4.

The subsequent [adaptive-repair diagnostic](formation-adaptive-repair-2026-09-10.md)
reproduces slow sparse-chain dissemination and verifies a bounded runtime
scheduling correction. Two native thirty-worker checks improve setup from
45–47 seconds to 26–30 seconds; three/ten-worker checks also pass. This is
formation-only evidence, not a resumed curve or accepted instrumentation
overhead. Preserve the failed batch and separately resolve normal CPU excess,
baseline-p95 variation, full-sampling loss and final checkpoint applicability.
The [paced-sampling follow-up](formation-sampling-diagnostic-2026-09-10.md)
now verifies a bounded cryptographic entropy cache, with three-worker diagnostic
CPU overhead medians of 12.05% before / 4.41% after. It preserves the original
thread defaults and sampling contract. These two same-version comparisons per
build are not six acceptance rounds, and ten/thirty-worker overhead and the
baseline-p95 gate remain unaccepted.

The accepted question is instrumentation overhead at a declared offered rate,
not maximum throughput. Preserve the 3/10/30-worker, six-mode, six-round matrix,
exact identity/formation/liveness checks, runtime settings, paired comparisons,
cost bands and baseline-variation gates. Saturation v2 remains a separately
labeled stress profile; never pool its cells with v3 or describe a v3 result
as peak capacity.

- Offer **500 summary requests/second per worker** (1,500/5,000/15,000 aggregate).
  Two persistent typed clients per worker each receive 2,500 absolute arrival
  slots over ten seconds: a four-millisecond period. Stagger phases across all
  clients. Keep the 64-request warmup per client, outside the timed window.
- At most one request is in flight per client. Skip expired slots instead of
  building a queue or replaying missed arrivals in bursts. Retain scheduled,
  completed, skipped and post-window-completion counts. All 5,000 arrivals per
  worker must reconcile; require **at least 99% completed within the window**
  for every worker. This is a declared workload-validity condition, not a
  production loss allowance. A missed arrival is never a successful request.
- Retain separate raw request-latency, scheduled-arrival-to-completion and
  scheduling-delay samples. The original request p95 remains the overhead
  score; show scheduling delay and skips alongside it so generator limitations
  cannot silently become a service-latency claim. This bounded-concurrency
  workload is not an unlimited open-loop queue or a general tail-latency SLO.
- Preallocate 2,500 64-bit samples per client for each of those three measures:
  0.36/1.2/3.6 MB total numeric payload at the three sizes. Merge/sort after END.
  No additional benchmark allocation occurs per recorded sample.
- Account CPU with the live fixture process's POSIX CPU clock, selected through
  `clock_getcpuclockid` and read in nanoseconds with `clock_gettime_ns`.
  Retain `/proc` ticks for comparison, but do not round low-load CPU cost to
  ten-millisecond ticks. Missing clocks fail rather than substituting zero.
  This is CPU time, not instructions or effective CPU frequency. Keep the
  external window bracket, 20 Hz RSS sampling and separate tooling accounting.
- Use schema **3** for manifest/cells/load and profile
  `fixed_500_per_worker_v1`; the OTLP collector's independent schema stays 2.
  `--fixed-rate` selects v3; omission retains the historical v2 stress workload.
- Allocate **20/20/45 minutes** to 3/10/30 workers, with **five minutes reserved
  for the completed diagnostics**, totalling 90 minutes excluding builds. The
  three-worker slice includes causal preflight and any separate harness smoke.
  Debit the smoke's elapsed time rounded up via `--prior-validation-seconds`
  (bounded to 180 seconds). The runner reserves ten seconds for cleanup inside
  remaining budgets and cannot borrow from the 90-minute cap.

Validate the new pacing/accounting functions and real serialized load path
before acceptance. Retain failed validation separately; no automatic retry or
favorable-sample selection. No worker semantics, allocator, runtime limits,
machine-wide policy, peer grammar or telemetry thresholds change with v3.

### Approved baseline diagnostic — 2026-09-10

Run `scripts/diagnose-formation-baseline.py` once: six fresh three-worker
`compiled_off` cells, using the real curve's formation, ten-second load,
identity and cleanup gates. The whole pilot is capped at 300 seconds, reserving
15 seconds for cleanup. Freeze executable hashes and source identity; retain
every attempted cell, including failure, without conditioning or exclusions.
Read-only observations at 500 ms cover each fixture process's thread CPU,
scheduling wait, context switches, page faults, RSS/swap, and readable host
temperature/frequency/throttle counters. Bound observations to 48 per cell,
six processes, 128 threads per process and 128 sensors. Unreadable counters are
unavailable, not zero. Sampled cpufreq is not effective-clock measurement and
RSS is not allocation count. Observer cost belongs to the runner.

Use the result to distinguish host placement/frequency, allocation or retained
state, runtime contention, and load-client limits. Allocation profiling is a
separate diagnostic comparison, not overhead acceptance; inspect actual hot
callers before changing storage ownership. No governor or machine-wide
profiling-security changes are authorized by this recipe. Review findings
before executing a revised full curve, keeping total accounting explicit.

The [reliability follow-up](formation-reliability-2026-09-09.md) corrects
demonstrated worker/verifier defects. The operator has now approved the
[shared setup convergence policy](#shared-setup-convergence-policy--2026-09-09).
Earlier short-gate failures remain failed historical results; new checks use
the explicitly revised policy, not retrospective acceptance.

## Shared setup convergence policy — 2026-09-09

Accepted by the operator after reviewing the demonstrated fixes and observed
thirty-worker convergence: post-admission exact membership, formation-summary
and lock/unlock visibility checks may use the **remaining original 60-second
whole-setup budget**, at all three worker counts. They do not each receive a
fresh 60 seconds. Startup/socket and individual join-status observations retain
their ten-second limits, also capped by the same whole-setup deadline.

All exact formation/node/certificate identities, member counts, all-alive
liveness, introducer readiness and policy checks remain mandatory. A reply
completing at or after its observation deadline cannot count as success. No
phase change, partial convergence or worker retry resets the deadline. Worker
protocol timers, admission/catch-up attempt limits, the ten-second load window,
cell/per-size/whole-batch budgets and no-automatic-retry rule are unchanged.

New manifests identify this policy as
`convergence_budget_policy: remaining_whole_setup_v1`, with
`observation_budget_seconds: 10` and `setup_budget_seconds: 60`.
Preserve all earlier manifests and failures under their original policy. This
is an acceptance-harness policy, not a production convergence SLO, worker runtime
change or new peer-protocol/architecture decision.

The [six-case formation-only check](formation-reliability-2026-09-09.md#approved-policy-verification--2026-09-09)
passed at 3/10/30 workers, with features omitted and metrics enabled. Thirty-worker
full setup took 46.615/51.287 seconds. If repeated across 36 cells, setup plus
ten-second load windows would exceed the unchanged 30-minute per-size budget
(approximately 34–37 minutes before other overhead). Review that feasibility
alongside remaining timing noise and the shutdown fix's applicability before another full curve; no
per-size or 90-minute batch-budget increase is authorized by this setup policy.

## Question and existing evidence

What does the final optional M4 telemetry configuration cost as real workers
maintain one authenticated formation and concurrently answer operator
requests? Report latency, throughput, worker CPU, memory, scrape cost and
telemetry loss without changing the domain outcomes or suppressing inconvenient
samples. No scientific workload or fleet-scale claim is involved.

## Operator decision and proposed scaling curve — 2026-09-09

The operator subsequently approved the complete proposed curve and its full
90-minute execution allowance, excluding builds. The heading is retained for
existing decision links; the curve and budget are now accepted, not awaiting
another choice.

The operator accepted proper paired verification and specified these cost bands
for both p95 latency and worker CPU overhead:

- Below 10% is the normal monitoring instrumentation goal, including the
  default-sampled trace/log configuration; the earlier 15% latency / 20% CPU
  proposal must not become an unqualified normal-operation pass.
- Up to 20% is tolerable only temporarily, with an explicit time-bounded
  operational disposition; it does not meet the normal goal.
- Above 20% through 25% is outside that temporary tolerance and requires review.
  Above 25% belongs to a separately scoped troubleshooting exercise, not an
  acceptable compute-operation monitoring profile. It does not authorize an
  automatic optimization or troubleshooting campaign.

Three workers remain the baseline; coverage must also answer the operator's
30-worker question. Accepted measurement sizes are **3, 10 and 30 workers**,
each with the same six modes and six rounds below. Keep two clients per worker
(6, 20 and 60 clients) and compare each enabled mode against its same-size,
same-round disabled baseline. Historically this held closed-loop concurrency
constant; v3 additionally fixes the offered per-worker request rate. Report absolute per-worker and aggregate
CPU, throughput, latency, RSS and formation/convergence timings at every size,
alongside telemetry overhead. A small percentage increase alone is not proof
that the underlying formation scales well.

This is a single-host formation/control-plane curve. Host saturation, swapping,
collector/client contention and background load can make a point inconclusive;
record those conditions rather than attributing all degradation to worker
count. Thirty local processes are not thirty independent machines, nor evidence
of scientific compute speedup or cross-host network behavior. The separate
[scientific scaling stages](../orishu-scaling-objectives.md#staged-evidence)
remain unchanged; their 32-worker stage is not satisfied by this experiment.

The expanded matrix has 108 cells instead of 36. The original v2 limit was 1,800
seconds per size and 5,400 seconds total; v3 uses the redistributed budget above,
excluding compilation, retaining every
partial result and with no automatic retries. This authorizes implementing and
validating the harness, then executing the frozen profile; no new measurements
are claimed by approval itself.

The [retained local baseline](../testing-worker-overhead.md#recorded-local-baseline--2026-09-07)
used one worker, one sequential client, two-second windows and scrapes
interleaved with requests. It compared one combined-capability binary in five
runtime modes. It did **not** measure omitted features, independent scrape
contention, three-worker peer work, the final peer profile or correlated logs.

The [v1 report reader](../../scripts/summarize-worker-overhead.py) now makes its
within-round comparison reproducible. At 1000 ppm the retained local baseline
shows CPU time per completed request approximately 20.0–21.3% above disabled,
and per-cell p95 latency approximately 8.3–17.4% above disabled. Full sampling
shows p95 increases approximately 62.8–96.6%, despite much smaller median-latency
changes. These are derived comparisons of old observations, not new runs or
predictions for any point on the new curve. The cost targets below are not
thresholds already proven by that baseline.

## Required profile before implementation

Use source-built Linux release executables, with the same compiler, lockfile,
optimization settings and accepted peer profile in both build directories.
Record executable hashes and a precise dirty-worktree checkpoint; feature
changes must not replace binaries used by a live cell. The feature-omitted
build must still speak the same accepted wire grammar as the enabled build.

| Mode | Build capabilities | Runtime configuration |
| --- | --- | --- |
| `omitted` | Neither optional feature | No diagnostics or exporter |
| `compiled_off` | Both | Both disabled; final logging disabled |
| `metrics` | Both | Metrics/probes and scheduled scrapes; tracing/logging disabled |
| `zero_sample` | Both | Metrics; trace exporter enabled with zero sampling; final log settings recorded explicitly |
| `default_sample` | Both | Metrics plus 1000 ppm tracing and the final accepted correlated-log configuration |
| `full_sample` | Both | Metrics plus 1,000,000 ppm tracing; same final output/queue policy, reported separately as a high-cost mode |

The [implemented logging contract](../orishu-observability.md#bounded-operational-log-adapter)
now defines zero sampling: enabled lifecycle records remain, but there are no
operation records or invented span IDs; enabled tracing can emit its bounded
final accounting records at shutdown. ADR 0025 is also implemented, and the
[official Collector journey](../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
verifies the client/peer/log configuration. These are functional prerequisites,
not performance evidence. The zero/default/full modes enable logging to a
continuously drained stdout reader, with queue 256 and shutdown 250 ms; the
reader validates bounded records and retains counts, not unbounded log history.
Trace queue/active/batch limits are 1024/128/128, export/response byte caps
1 MiB/16 KiB and flush/attempt/shutdown durations 1000/2000/3000 ms. Record these
exact settings and reader/collector executable identities before each run;
do not invent different runtime semantics in the benchmark or substitute
local-root-only evidence for admission tracing.

Use a healthy bounded loopback OTLP receiver that decodes the actual protobuf
and records finite receipt/accounting summaries, plus the selected local logging
sink. It must accept the final static span catalogue rather than assume every
span is a local client root. Reuse the M4 journey's explicitly named client/peer
operation to prove that the selected configuration really emits correlated
spans/logs before measurement. A configuration flag is not delivery evidence.
Record collector/sink CPU and memory separately from worker totals. Their
same-host resource contention remains a limitation, not worker CPU.

Remote TLS/mTLS, service/container overhead and degraded backend measurements
must be separate named profiles if required for the selected M4 deployment.
Do not combine them into this baseline or claim that this profile qualifies them.

## One measurement cell

The expanded profile below uses the accepted execution budget.

1. Start N isolated ordinary worker processes, for N = 3, 10 or 30. Use public
   authenticated joins in a chain: A admits B, B admits C, and each subsequent
   worker joins through its predecessor. Require the accepted
   exact formation/node/certificate/liveness views and complete introducer
   catch-up. Record join/catch-up durations separately from steady-state request
   latency. Setup has a sixty-second whole-cell budget; post-admission membership
   and summary convergence use its remaining time under the accepted policy
   above. Retain the owning protocol's narrower deadlines and the ten-second
   startup/join observations rather than extending them to sixty seconds.
   Observe `catchUpFailed` as a possible intermediate worker retry outcome under
   the same operation/assigned identity and existing observation deadline; never
   restart admission or reset the deadline on phase changes.
2. Check identified lock/unlock and eventual policy visibility across entry
   workers, then leave the formation unlocked. Record control/convergence
   durations within that same original setup deadline, and record the selected
   trace/log receipt evidence outside the request
   measurement window. Keep background SWIM/gossip/anti-entropy enabled.
3. Warm up 2N persistent typed clients, two directly targeting each worker,
   with 64 summary requests per client. Release all clients through one start
   barrier. Each has one outstanding request at a time for a shared ten-second
   window; v3 issues them only at its predeclared arrival slots. Client-process
   startup is not request latency. Every response retains
   its source, exact formation, N live members and unlocked state. Validate
   exact membership views immediately before and after the timed window.
4. In metrics-enabled modes, run one **independent** scraper per worker on a
   250 ms schedule, one request in flight at most. Do not pause client load to
   scrape and do not issue catch-up bursts for missed scrape intervals. Record
   scheduled/completed/skipped/failed scrapes and their response bytes/latency.
   Each scrape keeps the 32 KiB response cap and a one-second deadline.
5. Stop timed work, collect final samples and use normal bounded worker/sink
   shutdown. Record process-lifetime and timed-window accounting separately.
   No extra retry or flush grace may be added merely to erase shutdown drops.
   A missing exit, invalid state or failed required request makes the cell fail;
   it is not silently excluded from the performance report.

The historical v2 profile is a two-clients-per-worker closed-loop throughput experiment. It is not an
independently scheduled arrival-rate/tail-latency SLO test. Request throughput
counts validated completed requests divided by the shared measured window;
client-side validation and contention remain part of that workload.

## Repetitions, resource bounds and retained artifacts

Run six rounds, rotating the first of the six modes each round so each occupies
every ordinal position once: 36 complete cells per size, 108 for the proposed
curve. Run sizes in ascending order and record that ordering as a possible
thermal/time confound; the disabled comparison remains paired within each size.
Use a fresh formation per cell and stable logical join-order roles for
comparisons, not reused ephemeral identities.
Run alone without concurrent builds/test suites; record CPU/kernel, `CLK_TCK`,
CPU affinity, scaling policy when readable and competing host load. Do not
change governor, host services or affinity policies without separate permission.

For historical v2, preallocate at most 500,000 64-bit integer latency samples per client (24, 80
and 240 MB decimal payload at 3, 10 and 30 workers, respectively); v3 uses the
smaller fixed-rate storage bounds above. Retain at most
128 scrape-latency records per worker. Reaching
a sample cap before the ten-second window ends is an explicitly incomplete
cell, not a shorter conveniently fast measurement. Sample worker CPU/RSS at
20 Hz with bounded `/proc` reads; record setup high-water RSS separately from
timed resident-memory peaks. Do no sorting or artifact writes in client hot
loops. The v3 execution budgets are specified above; the 90-minute total still
excludes compilation. Preserve partial results; there is no automatic
retry loop. A setup deadline or resource failure at a larger size is retained
as a failed/incomplete point, never silently replaced with a smaller formation.

The scaling verifier bounds `orishuctl ls` output separately at 64 KiB so a
complete thirty-member listing is representable; all other operator command
replies retain the 16 KiB cap. Both limits are recorded in the manifest. Exact
membership, unique identity, pinned certificates and Alive checks still apply.
Failure artifacts retain the formation substage, last observed join phases,
bounded membership rows and summary projections before fixture teardown.

The new report format needs explicit versioning; do not append new semantics
to v1. Before running, create a new private output directory and freeze a run
manifest containing this plan revision, approved budgets, build/tool/profile
identities, exact commands, endpoint/security mode, sampling, queues, batching,
flush/shutdown settings, load counts and deadlines. Flush one bounded cell
summary after each cell outside its timed window. Retain all failures and
original run order; never overwrite a previous report.

Per cell, retain per-worker request count, elapsed window, median/p95 latency,
CPU ticks and seconds, CPU per validated request, timed RSS peak and separate
process high-water RSS; scraper counts/bytes/latencies; setup/control timings;
collector and log receipt/loss counts; and domain validation/exit results.
Do not use process-lifetime counters divided by timed request counts as a
fabricated loss rate. Report exporter queue/active/encoding/delivery/shutdown
loss and intentional sampling separately. No missing instrument becomes zero.

## Cost targets and remaining profile review

The operator's p95/CPU cost bands above supersede the earlier normal-operation
latency/CPU proposals. Other numerical profile settings below are retained from
the reviewed recommendation, not measured guarantees or scientific performance
objectives. Compare `metrics`, `zero_sample` and
`default_sample` with the same round's `compiled_off` result. Compare
`compiled_off` with `omitted` separately to expose compiled-in cost.

| Metric | Metrics-only target | Default-sampled final trace/log target |
| --- | --- | --- |
| Per-worker request p95 | Below 10% increase for normal operation | Same |
| Aggregate validated throughput | At most 10% decrease | At most 15% decrease |
| Aggregate worker CPU seconds per validated request | Below 10% increase for normal operation | Same |
| Timed peak RSS increase, per worker | At most 2 MiB | At most 8 MiB |
| Scraping | No failed/skipped scheduled scrapes; report per-worker median/p95 and bytes | Same |

Apply the default-sampled budget also to `zero_sample`; enabling an idle exporter
must not gain an exemption. Report full sampling with all drops and timings,
but do not promise it meets the default-sampling overhead budget. It retains
the same domain correctness, finite-resource and shutdown requirements.
Compiled-off versus omitted is an explicitly reported comparison requiring
review, not a claim of zero overhead from absence of export traffic.

At each worker count separately, for each worker role calculate the six paired
relative changes first, then
their median and range; never pool independently computed p95s into a fake
request percentile. The primary latency/memory score is the worst role's median
paired change; throughput and CPU use aggregate worker totals within each cell.
Report every pair and absolute values. Any correctness failure or default-mode
local/delivery loss prevents an unqualified performance acceptance claim;
shutdown losses are reported separately and need an explicit disposition, not
a lossless-export assumption. A zero baseline denominator is unavailable, not
a zero-cost improvement.

If baseline per-role p95 or aggregate throughput has a max/min ratio above 1.10,
or any individual paired regression exceeds twice its proposed budget, mark
the batch inconclusive pending bounded investigation. Do not automatically
repeat until a favourable batch appears. This noise rule, the numerical
budgets and the selected deployment profiles all need review **before** an
acceptance run. A failed budget may lead to a measured optimization or an
explicitly revised requirement, never retroactive threshold fitting.

## Implementation and review exit

The parameterized scaling measurement harness, versioned run/cell artifacts,
independent scraper/collector/log accounting and report reader now exist.
Their tests cover partial rounds, mismatched
build/profile/window identities, missing instruments, cap exhaustion, failures,
zero denominators and loss preservation. The current v1 reader deliberately
refuses partial matrices and unsupported schemas; it is not the v2 reader.

Review exit: the expanded matrix execution budget is accepted. Freeze its
size-dependent profile and executable/command identities before measurement,
preserving the accepted operator cost bands. Name any
additional deployment performance profile explicitly if required for M4.
ADR 0025 and the stdout logging destination
are accepted and have separate implementation evidence; that does not verify
the cost targets above. Use the
[M4 checklist](../tasks/cluster-formation-m4-checklist.md) to record deployment
applicability without turning functional recipe coverage into measured
service/container overhead. This plan does not close P-OBSERVABILITY/M4. The
approved measurement run must still execute without overlapping builds/tests
and retain all results, including partial or failed cells.
