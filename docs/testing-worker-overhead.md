# Measure local worker telemetry overhead

Status: **manual measurement harness; full M4 performance acceptance remains open**

This compares one source-built Linux worker's local control-plane request path
with runtime telemetry disabled and enabled. It is not a three-worker formation,
scientific workload, fleet-scale, TLS-collector or production SLO benchmark.
No reviewed overhead threshold is currently specified; report measurements and
limitations rather than deriving a passing budget from the results.

## Reproduction

Run alone on a quiet host, without concurrent builds or test suites. Local TCP
and Unix sockets, readable `/proc`, and the checked-in Rust toolchain are
required. The test refuses debug builds and is ignored during ordinary tests:

```sh
getconf CLK_TCK
uname -srmo
lscpu
measurement_dir=$(mktemp -d)
ORISHU_BENCH_REPORT="$measurement_dir/measurements.jsonl" cargo test --locked --offline --release -p orishu-worker --features observability,otlp-tracing --test standalone measure_local_telemetry_overhead --target-dir target/formation-flow-observability -- --ignored --nocapture --test-threads=1
sha256sum target/formation-flow-observability/release/orishu-worker
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
