# Formation telemetry overhead: approved acceptance experiment

Status: **3/10/30-worker curve and execution budget accepted on 2026-09-09;
revised setup policy verified; full overhead run remains incomplete**.
Owner: [P-OBSERVABILITY for M4](../tasks/implement-worker-observability.md).
This is the finite plan requested by the
[formation handoff](../tasks/implement-cluster-formation-poc.md#open-m4-work-selection),
not accepted performance evidence or permission to change runtime limits.
The [v2 source-built recipe](../testing-worker-overhead.md#current-formation-scaling-experiment)
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
round. A five-minute baseline-only diagnostic and a 45-minute allowance for the
30-worker point have been proposed for operator review, both within the next
reviewed 90-minute total. Neither change is approved; current budgets and the
no-automatic-retry rule remain unchanged.

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
same-round disabled baseline. This holds per-worker closed-loop concurrency
constant, not total request rate. Report absolute per-worker and aggregate
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

The expanded matrix has 108 cells instead of 36. The accepted limit is 1,800
seconds per size and 5,400 seconds total, excluding compilation, retaining every
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
   window; client-process startup is not request latency. Every response retains
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

This is a two-clients-per-worker closed-loop throughput experiment. It is not an
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

Preallocate at most 500,000 64-bit integer latency samples per client (24, 80
and 240 MB decimal payload at 3, 10 and 30 workers, respectively) and at most
128 scrape-latency records per worker. Reaching
a sample cap before the ten-second window ends is an explicitly incomplete
cell, not a shorter conveniently fast measurement. Sample worker CPU/RSS at
20 Hz with bounded `/proc` reads; record setup high-water RSS separately from
timed resident-memory peaks. Do no sorting or artifact writes in client hot
loops. The accepted limit is 1,800 seconds per size / 5,400 seconds total,
excluding compilation. Preserve partial results; there is no automatic
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
