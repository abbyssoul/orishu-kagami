# Formation telemetry overhead: proposed acceptance experiment

Status: **proposed — measurement profile and budgets require operator review**.
Owner: [P-OBSERVABILITY for M4](../tasks/implement-worker-observability.md).
This is the finite plan requested by the
[formation handoff](../tasks/implement-cluster-formation-poc.md#open-m4-work-selection),
not accepted performance evidence or permission to change runtime limits.
The experiment described below is not implemented by the existing local test.

## Question and existing evidence

What does the final optional M4 telemetry configuration cost while three real
workers maintain one authenticated formation and concurrently answer operator
requests? Report latency, throughput, worker CPU, memory, scrape cost and
telemetry loss without changing the domain outcomes or suppressing inconvenient
samples. No scientific workload or fleet-scale claim is involved.

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
predictions for three workers. The budgets below are proposals to review, not
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

The final logging contract must specify what zero sampling means for operational
records; do not invent that rule in the benchmark. Until the accepted stdout
logging adapter and ADR 0025 are implemented, current-capability experiments can only
be labelled exploratory. They cannot silently substitute local root spans for
the final client/peer/log configuration.

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

1. Start three isolated ordinary worker processes A, B and C. Use public
   authenticated joins so A admits B and B admits C. Require the accepted
   exact formation/node/certificate/liveness views and complete introducer
   catch-up. Record join/catch-up durations separately from steady-state request
   latency. Setup has a sixty-second whole-cell budget; retain the owning
   protocol's narrower deadlines rather than extending them to sixty seconds.
2. Check identified lock/unlock and eventual policy visibility across entry
   workers, then leave the formation unlocked. Record control/convergence
   durations and the selected trace/log receipt evidence outside the request
   measurement window. Keep background SWIM/gossip/anti-entropy enabled.
3. Warm up six persistent typed clients, two directly targeting each worker,
   with 64 summary requests per client. Release all six through one start
   barrier. Each has one outstanding request at a time for a shared ten-second
   window; client-process startup is not request latency. Every response retains
   its source, exact formation, three live members and unlocked state. Validate
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

This is a six-client closed-loop throughput experiment. It is not an
independently scheduled arrival-rate/tail-latency SLO test. Request throughput
counts validated completed requests divided by the shared measured window;
client-side validation and contention remain part of that workload.

## Repetitions, resource bounds and retained artifacts

Run six rounds, rotating the first of the six modes each round so each occupies
every ordinal position once: 36 complete cells. Use a fresh formation per cell
and stable logical roles A/B/C for comparisons, not reused ephemeral identities.
Run alone without concurrent builds/test suites; record CPU/kernel, `CLK_TCK`,
CPU affinity, scaling policy when readable and competing host load. Do not
change governor, host services or affinity policies without separate permission.

Preallocate at most 500,000 integer latency samples per client (24 MiB payload
across six clients) and at most 128 scrape-latency records per worker. Reaching
a sample cap before the ten-second window ends is an explicitly incomplete
cell, not a shorter conveniently fast measurement. Sample worker CPU/RSS at
20 Hz with bounded `/proc` reads; record setup high-water RSS separately from
timed resident-memory peaks. Do no sorting or artifact writes in client hot
loops. Stop the complete batch after 1,800 seconds excluding compilation,
preserving partial results; there is no automatic retry loop.

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

## Proposed budgets — not accepted

These candidate budgets express a default operational cost target for review,
not a scientific performance objective. Compare `metrics`, `zero_sample` and
`default_sample` with the same round's `compiled_off` result. Compare
`compiled_off` with `omitted` separately to expose compiled-in cost.

| Metric | Metrics-only proposal | Default-sampled final trace/log proposal |
| --- | --- | --- |
| Per-worker request p95 | At most 10% increase | At most 15% increase |
| Aggregate validated throughput | At most 10% decrease | At most 15% decrease |
| Aggregate worker CPU seconds per validated request | At most 10% increase | At most 20% increase |
| Timed peak RSS increase, per worker | At most 2 MiB | At most 8 MiB |
| Scraping | No failed/skipped scheduled scrapes; report per-worker median/p95 and bytes | Same |

Apply the default-sampled budget also to `zero_sample`; enabling an idle exporter
must not gain an exemption. Report full sampling with all drops and timings,
but do not promise it meets the default-sampling overhead budget. It retains
the same domain correctness, finite-resource and shutdown requirements.
Compiled-off versus omitted is an explicitly reported comparison requiring
review, not a claim of zero overhead from absence of export traffic.

For each worker role, calculate the six paired relative changes first, then
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

Remaining implementation is the three-worker measurement harness, versioned
run/cell artifacts, independent scraper/collector/log accounting and a reader
for that new format. Their tests must cover partial rounds, mismatched
build/profile/window identities, missing instruments, cap exhaustion, failures,
zero denominators and loss preservation. The current v1 reader deliberately
refuses partial matrices and unsupported schemas; it is not that future reader.

Review exit: approve or revise the profile/budgets above and name any additional
deployment profile required for M4. ADR 0025 and the stdout logging destination
are now accepted separately; their implementation and the numerical budgets
above are not approved or verified by that choice. This plan does not start
the benchmark or close P-OBSERVABILITY/M4.
