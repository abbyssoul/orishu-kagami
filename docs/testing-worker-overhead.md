# Measure local worker telemetry overhead

Status: **post-diagnostic full curve completed; thirty-worker stability and M4 acceptance open**

## Current formation scaling experiment

Latest execution: the [post-diagnostic 108-cell curve](measurements/formation-post-diagnostic-2026-09-10.md)
completed once with all formation and rate-delivery checks, unchanged artifacts
and clean cleanup. Default sampling CPU/p95 overhead medians are
5.41%/2.83% at three workers and 6.56%/5.12% at ten, meeting the reviewed normal
gates. Thirty-worker medians are low but baseline latency variation fails the
unchanged stability gate, so that size is not performance-qualified. Full
sampling remains high-cost/lossy; do not infer a lossless or normal-compute
recommendation from a completed cell. The report retains all modes, rounds,
noise findings, diagnostic limitations and budget accounting.

The [approved plan](measurements/formation-telemetry-plan.md) measures 3, 10 and
30 ordinary source-built Linux workers, six modes and six rounds per size.
The original v3 allocation uses fixed-rate traffic at 500 requests/s/worker and
20/20/45-minute size slices plus five reserved diagnostic minutes, totalling
90 minutes excluding builds, with no
automatic retries. The subsequent approved total allowance is 120 minutes;
the completed fresh batch used `--fixed-rate --batch-cap-seconds 3300` to
enforce its 55-minute cap, including preflight/cleanup. The cap only shortens
the whole-batch deadline; per-size limits do not grant extra time. Consult the
latest report's remaining debit before any separately approved experiment.
A separate one-round `--smoke` run validates the harness;
it cannot satisfy the full matrix. Do not run either alongside builds/tests.
The [validation record](tasks/cluster-formation-conformance.md#approved-scaling-curve-and-v2-harness-validation--2026-09-09)
retains failed preflights, the completed smoke matrix and its limitations.
The subsequent [quiet-host report](measurements/formation-telemetry-2026-09-09.md)
retains 36 complete three-worker cells and the failed first ten-worker setup.
The newer [fixed-rate report](measurements/formation-fixed-rate-2026-09-10.md)
retains 36/36/11 completed cells at 3/10/30 workers and a failed thirty-worker
unlock setup, without retry. Three/ten-worker normal overhead comparisons are
inconclusive on baseline-p95 variation; thirty-worker comparisons are partial.
Default-sampling CPU medians exceed the normal goal, and full sampling has
high CPU cost and log loss. The separate
[reliability follow-up](measurements/formation-reliability-2026-09-09.md) records
worker/verifier fixes and the original short-gate failures. The operator-approved
[convergence policy](measurements/formation-telemetry-plan.md#shared-setup-convergence-policy--2026-09-09)
now lets post-admission membership, summary and lock/unlock visibility use the
remaining original 60-second setup budget, without resetting it between stages.
Startup/join observations keep their ten-second limits, and all identity,
liveness and policy predicates remain unchanged. The six bounded 3/10/30-worker
checks passed under this policy. A separate
[shutdown follow-up](measurements/formation-telemetry-2026-09-09.md#shutdown-log-follow-up--2026-09-09)
fixes final-event contention and verifies one three-worker zero-sampling
load/shutdown. Review remaining noise, full-sampling cost/loss and checkpoint applicability
and the documented per-size budget feasibility risk before another acceptance
run; do not retry the matrix automatically.
`scripts/check-formation-scaling.py` runs one
formation-only regression with the same exact predicates and writes a new,
private evidence directory; it does not run timed load or grant acceptance.
The [convergence diagnostic](measurements/formation-convergence-diagnostic-2026-09-10.md)
adds a bounded CLI-versus-persistent-client comparison and optional existing
peer-metric snapshots. Its one-second verification cadence and `observe`
probe mode are diagnostic only, not substituted acceptance behavior.
The [adaptive-repair follow-up](measurements/formation-adaptive-repair-2026-09-10.md)
adds a deterministic serialized chain replay and six real-worker setup checks.
Its runtime scheduling fix preserves SWIM and member authentication, but changes
repair traffic and therefore requires a new measurement checkpoint. Existing
curve results do not become passing evidence for the modified worker.
The subsequent [paced-sampling diagnostic](measurements/formation-sampling-diagnostic-2026-09-10.md)
isolates repeated cryptographic-provider cost and verifies a fixed-memory,
non-waiting entropy cache. Three-worker diagnostic CPU overhead improves from
12.05% to 4.41%, but this is not a full scaling acceptance result. Runtime thread
defaults, sampling rates, source authentication and the accepted gates remain
unchanged; do not substitute the rejected two-thread diagnostic configuration.

```sh
make build-formation-telemetry
make measure-formation-telemetry OTELCOL=/absolute/path/to/otelcol FORMATION_TELEMETRY_OUTPUT=/tmp/orishu-telemetry-new-run FORMATION_TELEMETRY_ARGS=--fixed-rate
python3 scripts/summarize-formation-telemetry.py /tmp/orishu-telemetry-new-run
```

Use the existing pinned Collector 0.160.0 binary. The output directory must not
already exist; the runner creates it privately and snapshots the executables.
It retains a versioned source/tool/profile manifest, official-Collector causal
preflight result, and one bounded JSON artifact per cell. A failed cell stops
the run and retains available evidence; do not overwrite or silently resume it.
The summary can inspect a partial matrix but labels missing comparisons.
For a smoke run, use `FORMATION_TELEMETRY_ARGS='--fixed-rate --smoke'` and a
fresh output path. Debit its elapsed seconds rounded up from the subsequent
full curve with `--prior-validation-seconds`; smoke and causal preflight belong
inside the three-worker slice. Omitting `--fixed-rate` retains the historical
saturation profile, never current acceptance.

The Rust probe uses two persistent typed clients per worker and validates every
response. It collects raw latency samples before computing per-worker p95,
excluding post-window completions from timed throughput. Its separately spawned
loopback collector decodes actual OTLP with the final three-operation catalogue.
The runner continuously drains bounded stdout records and samples each process
at 20 Hz. Worker, collector, client, and runner/log-sink CPU/RSS remain separate.
CPU uses a recorded external start/end bracket; scheduling skew is reported,
not silently treated as an exact ten-second CPU interval.
V3 uses a high-resolution per-process CPU clock, retaining ticks as secondary
evidence, and separately reports scheduling delay, scheduled-response latency,
and skipped/tail arrivals. Every worker must complete at least 99% of its 5,000
scheduled arrivals; all counts reconcile without catch-up bursts or queues.
See the [v3 profile](measurements/formation-telemetry-plan.md#fixed-rate-acceptance-profile-v3--2026-09-10)
for bounds and limits of the resulting claim.

Trace queues/batches/flush/attempt/shutdown retain 1024/128/1000 ms/2000 ms/3000 ms;
active capacity is 128, export bytes 1 MiB and response bytes 16 KiB. Logging
uses 256 records and 250 ms shutdown. Zero/default/full modes enable stdout
logging; zero sampling produces lifecycle/final accounting, not operation logs.
The causal preflight uses full sampling and batch size one for exact ID-chain
verification. Timed cells retain bounded receipt counts, not unbounded span-ID
history; their counters are not a durable delivery or exact per-span audit.
Before/after receipt snapshots are observed cumulative counters, not a claim
that every delivered span was produced inside the timed window.

Temporary worker credentials and sockets are removed by fixture cleanup; retained
results contain no credential bytes. Only the fixture's child PIDs are stopped.
No host governor, affinity, service policy or runtime defaults are changed.
Scientific workloads, network scaling across machines, and service/container
overhead remain outside this single-host control-plane experiment.

## Historical single-worker v1 experiment

The current worker no longer prints synchronous final trace statistics to stderr.
V1 requires those exact final counters and therefore now requires an explicit
absolute `ORISHU_BENCH_LEGACY_WORKER` path to a retained, compatible historical
release binary. Without it the harness refuses before creating an artifact or
starting workers. Do not point it at the current binary or substitute a last
scrape for final shutdown accounting. Existing v1 artifacts and their report
reader remain unchanged. The separate v2 curve and budgets are now approved;
v1 observations do not qualify the final profile-5/logging implementation.

This compares one source-built Linux worker's local control-plane request path
with runtime telemetry disabled and enabled. It is not a three-worker formation,
scientific workload, fleet-scale, TLS-collector or production SLO benchmark.
V1 has no retrospectively applied acceptance threshold; report measurements and
limitations rather than deriving a passing budget from the results.

## Reproduction

Run alone on a quiet host, without concurrent builds or test suites. Local TCP
and Unix sockets, readable `/proc`, and the checked-in Rust toolchain are
required. First set `ORISHU_BENCH_LEGACY_WORKER` to the retained compatible
release binary and record its checkpoint/hash. The test refuses debug builds
and is ignored during ordinary tests. These commands are historical-recipe
reproduction, not a supported current-worker benchmark:

```sh
getconf CLK_TCK
uname -srmo
lscpu
measurement_dir=$(mktemp -d)
ORISHU_BENCH_REPORT="$measurement_dir/measurements.jsonl" cargo test --locked --offline --release -p orishu-worker --features observability,otlp-tracing --test standalone measure_local_telemetry_overhead --target-dir target/formation-flow-observability -- --ignored --nocapture --test-threads=1
sha256sum "$ORISHU_BENCH_LEGACY_WORKER"
```

The test prints fifteen `OVERHEAD` JSON records. `ORISHU_BENCH_REPORT` additionally
creates a new JSONL file and flushes each completed cell outside its timed
window; existing paths are refused instead of overwritten. Partial results
remain available on failure. Retain every record, including
run order and final trace accounting, with the executable hash, revision/dirty
checkpoint, toolchain, CPU/kernel, clock-tick frequency and host-load context.
Do not discard slower rounds or present a retry as the original result. Do not
change features in this target while its worker executable is in use.

## Compared modes

All modes use the **same binary with both optional features compiled in**.
They compare runtime settings, not omitted-feature build cost:

| Mode | Metrics and scheduled scrapes | Tracing |
| --- | --- | --- |
| `disabled` | Off | Off |
| `metrics` | On | Off |
| `zero_sample` | On | Enabled, zero sampling |
| `sample_1000ppm` | On | Enabled, default 1000 ppm sampling |
| `sample_all` | On | Enabled, 100% sampling |

The collector is a loopback HTTP responder in the harness process. It decodes
the maintained OTLP protobuf, verifies fixed client-request span names and the
128-span batch bound, and responds successfully. Default queue, batch, flush,
attempt and shutdown settings are retained. Request bodies are capped at 1 MiB,
headers at 4 KiB, and each collector exchange at two seconds. This isolates
local exporter mechanics; collector CPU runs in the harness and is **not**
included in reported worker CPU ticks. Real collector deployment, remote
latency, TLS/mTLS and failure-pressure costs need separate measurements.

## Work and bounds

Three rounds rotate the first mode to reduce simple run-order bias. Every cell
uses fresh private worker state/socket paths, checks startup through the real
client, warms a reused client with 64 summary requests, and measures sequential
typed summary requests for two seconds or 100,000 requests, whichever comes
first. Every measured response must retain formation/node identity, one live
member and the unlocked state. This validates unchanged local summary outcomes,
not numerical equivalence or distributed convergence.

Enabled metrics are scraped after requests, approximately every 250 ms. Scrapes
are interleaved, not a separate concurrent load generator. Request latency
excludes the scrape; end-to-end request throughput includes validation and
scrape pauses. This is a closed-loop single-client experiment and does not
measure latency under independently scheduled arrivals or overload.

Latency storage is capped at 100,000 integer samples (800,000 payload bytes),
and sorting occurs outside the timed window. The output includes median and
nearest-index p95 request latency, request throughput, median scrape duration,
worker CPU ticks during the timed window, and pre-shutdown RSS/high-water RSS.
CPU uses process `utime + stime`; convert ticks using the recorded `CLK_TCK`.
Memory readings come from bounded `/proc/PID/status` reads, and are approximate whole-worker
resident-memory observations, not isolated exporter allocation measurements.
See the [kernel's field definitions](https://docs.kernel.org/filesystems/proc.html).

The worker drains through its normal shutdown path; the existing process guard
allows three seconds and kills/reaps on failure. The collector is cancelled
and awaited on success and cancellation-guarded on failure. The experiment has
a 180-second whole-run deadline, excluding compilation. Final delivery and
queue reports include startup/warm-up as well as timed requests; CPU/RSS timing
excludes shutdown drain. Do not equate their different measurement windows.

## Interpreting results

Compare modes within each round, and publish round-to-round variation rather
than only one percentage. Short windows, CPU frequency scaling, scheduler and
allocator state, ASLR, shared-host load and coarse CPU ticks can dominate small
differences. A faster enabled result is not proof of negative telemetry cost.
Fresh worker identities and sampling entropy are intentionally not fixed.
At low rates, report how many records were actually sampled and delivered;
do not infer a collector workload merely from configured sampling.

Include drops and failures with any throughput claim. This harness reports them
rather than optimizing the benchmark by silently shedding traces. No additional
overhead threshold, runtime optimization or change to scientific semantics is
authorized by a noisy result. Formation instrumentation, cross-peer propagation,
trace-correlated logging, representative concurrent/three-worker operation,
omitted-feature builds, exporter-specific memory and a reviewed budget still
need coverage before the full M4 overhead requirement is accepted.

## Recorded local baseline — 2026-09-07

The [complete retained run](measurements/worker-telemetry-2026-09-07.jsonl)
contains all fifteen cells. It used Rust 1.97.1, Linux 6.17.0-41-generic x86_64,
an Intel Core i7-1370P (20 logical CPUs), and `CLK_TCK=100`. No concurrent build
or test was launched during measurement; host frequency scaling and unrelated
host load were not controlled. The combined-feature optimized worker SHA-256
was `7492acecc4eaed9f22934ee1d78c0ac9c56091c41577ed2c79d730716f4861c8`, at
dirty-worktree base `42d2b2ce10ecd36c03a87da89dcb5de8446c1667`.

Values below are medians of the three per-cell summaries, **not** a pooled
request distribution or confidence interval:

| Mode | Median request µs | Per-cell p95 µs | Requests/s | Whole-worker RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| Disabled | 20.411 | 26.527 | 45,038 | 12,192 |
| Metrics | 20.444 | 27.886 | 44,286 | 12,304 |
| Zero sampling | 20.584 | 26.173 | 44,691 | 13,176 |
| 1000 ppm | 22.262 | 30.531 | 40,505 | 13,572 |
| Full sampling | 22.801 | 45.151 | 35,289 | 13,800 |

Within-round comparisons put 1000-ppm median request latency 8.9–10.0% above
disabled and throughput 8.9–10.9% below it. Full sampling raised median latency
11.3–12.1% and lowered throughput 21.5–22.6%. Metrics-only throughput varied
from 4.7% lower to 2.9% higher; this is noise-sensitive evidence, not a speedup
claim. Whole-worker CPU consumed 97–112 ticks per roughly two-second window.
Every enabled cell made eight diagnostic scrapes. Default sampling delivered
88, 88 and 90 spans; final accounting reported no queue, delivery or shutdown
loss in this retained run.

An earlier measurement also passed, but its tool output was truncated before
complete retention. It is not included in the table or silently treated as a
failed run. Its visible full-sampling round reported 128 shutdown-abandoned
records despite a healthy collector, consistent with the exporter's documented
interrupted-attempt policy; the retained run's zeros do not erase that result.
Create-new per-cell artifact capture was added afterward. The first attempt
with capture was refused at socket creation by the sandbox and produced an
empty artifact; approved execution used a different new filename and completed.

This baseline makes local cost and variability visible. It does not certify a
reviewed performance budget, complete delivery at shutdown or the wider M4
formation workload. Preserve these results when investigating sampling/export
cost rather than selecting only the lowest-cost round.
