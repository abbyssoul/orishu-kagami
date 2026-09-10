# Post-diagnostic fixed-rate acceptance — 2026-09-10

Status: **108 cells complete; normal instrumentation meets reviewed gates at
three/ten workers; thirty-worker latency stability and combined M4 remain open**.
Owner: [M4 checklist](../tasks/cluster-formation-m4-checklist.md).
Plan: [approved post-diagnostic batch](formation-telemetry-plan.md#approved-post-diagnostic-batch--2026-09-10).

This is a new complete 3/10/30-worker, six-mode, six-round experiment after the
adaptive formation repair and bounded sampling cache. It does not overwrite,
resume, pool with or reclassify the earlier failed curve. The same fixed
500 requests/s/worker workload and all original validity/acceptance gates apply.

The operator approved 120 minutes total excluding builds. Prior debit is
3,802.326 seconds. The new batch is capped at 3,300 seconds including causal
preflight and cleanup, leaving at least 97.674 seconds of the total allowance.
The harness's optional cap only shortens its deadline; a focused regression
first failed because the option was absent, then passed with its validation.
All 36 harness contracts pass. An initial targeted test invocation used the
wrong test-class name and failed during discovery, before any experiment.

## Frozen command and artifacts

```sh
python3 scripts/measure-formation-telemetry.py --fixed-rate --batch-cap-seconds 3300 \
  --worker target/formation-telemetry-enabled/release/orishu-worker \
  --omitted target/formation-telemetry-omitted/release/orishu-worker \
  --ctl /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/ctl \
  --probe /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/probe \
  --otelcol /tmp/orishu-curve-check.so7Ibv/full-quiet/otelcol \
  --output /tmp/orishu-curve-check.so7Ibv/post-diagnostic-full-20260910
```

The output directory must not already exist. Local cached tools are not
portable deployment requirements. The runner snapshots every executable and
all tracked/untracked nonignored source files, records hashes, toolchain,
host/configuration identity and the command, then verifies identity at finish.
No source edit, build, other test or profiler is run concurrently with cells.
No governor, affinity, runtime-thread default or profiling-security setting is
changed. Completed functional/regression evidence remains scoped to the
[correctness checkpoint](../tasks/cluster-formation-m4-checklist.md#post-diagnostic-correctness-checkpoint--2026-09-10).

## Completed execution and integrity

All 108 cells completed once: 36 at each size, with no setup failure, retry,
resume or missing cell. The runner reports **2,765.813 seconds**; the outer
command reports **46m07.200s**, including snapshot/startup overhead. Charge the
larger outer duration. The 55-minute cap was not reached. `finished.json`
confirms source and executable identities unchanged, and every cell records
clean cleanup. A post-run process-name audit found no remaining fixture worker,
Collector, profiler or build process. No unrelated process was stopped.

The official Collector 0.160.0 causal preflight passed A→B→C formation, two
causal chains, **74 matched spans/logs**, probes, policy and 172 series/worker.
Its separate full-sampling/batch-one functional configuration is not a timed
performance cell. Runtime/configuration identities are retained in
`manifest.json`, and each cell binds that manifest's hash.

| Frozen executable | SHA-256 |
| --- | --- |
| Combined release worker | `b3f7db9b2bbb58f571e0e1bcc1f752d8562e5fa725a2c724c19a303fd7a95371` |
| Feature-omitted release worker | `754a4377cfd9f4fb15587f25d30d25bd0c9e8f68f5bbfc3a57adb33297f678b7` |
| CLI | `ca2ea5d6837b0c8dc3a6149c47eccacb506a02ebb60f1d34f142a542d53251ff` |
| Probe | `6ea2f8e96e6cb045afb6d58f85280c94e651a17e3a5d9a7f1eb8c1f1deab3e01` |
| Collector | `abe338fa33865e54412566db5cea4adc82374594b65b49c1099048cdf8cf2451` |

Host: Linux 6.17.0-41-generic, glibc 2.42, 20 available logical CPUs and
unchanged affinity 0–19. Compiler: Rust 1.97.1
(`8bab26f4f`, 2026-07-14), LLVM 22.1.6. HEAD remains
`b52dc1abc62741d91f4b18dcc8c9d2c322bccbd9`; the manifest's complete dirty-source
inventory, rather than HEAD alone, identifies measured implementation.

## Formation and fixed-rate scalability

| Workers | Setup minimum / median / maximum, seconds | Minimum completed arrivals per worker/window | Runtime-off median aggregate requests/s | Runtime-off median worker CPU seconds / 10 s |
| --- | --- | --- | --- | --- |
| 3 | 2.138 / 2.668 / 3.283 | 4,998 / 5,000 | 1,499.8 | 2.597 |
| 10 | 8.500 / 11.396 / 13.614 | 4,997 / 5,000 | 4,999.4 | 8.990 |
| 30 | 24.261 / 27.954 / 31.412 | 4,993 / 5,000 | 14,996.2 | 22.140 |

Setup includes exact membership, all-alive/introducer-ready views and both
policy propagations under the same original 60-second deadline. The original
thirty-worker unlock failure did not recur in these 36 formations. These are
observed distributions, not a guaranteed convergence SLO. Every worker/window
exceeds 99% delivery; missing arrivals remain recorded skips/tail completions.
This measures declared-rate scaling, not saturation capacity or scientific
workload performance.

## Paired instrumentation results

CPU is median aggregate CPU-per-completed-request change over six same-round
pairs. Latency is the worst role's median paired request-p95 change; it is not
a percentile pooled across workers. Every raw pair remains reproducible using:

```sh
python3 scripts/summarize-formation-telemetry.py /tmp/orishu-curve-check.so7Ibv/post-diagnostic-full-20260910
```

| Workers | Mode vs runtime off | CPU change | p95 change | Worst-role median peak-RSS increase | Disposition |
| --- | --- | --- | --- | --- | --- |
| 3 | Metrics | +3.58% | +1.74% | 354 KiB | Normal gates met |
| 3 | Zero sampling | +4.35% | −0.05% | 1,362 KiB | Normal gates met |
| 3 | Default sampling | +5.41% | +2.83% | 1,628 KiB | Normal gates met |
| 10 | Metrics | +3.58% | +4.65% | 442 KiB | Normal gates met |
| 10 | Zero sampling | +6.55% | +4.65% | 1,602 KiB | Normal gates met |
| 10 | Default sampling | +6.56% | +5.12% | 1,844 KiB | Normal gates met |
| 30 | Metrics | +1.99% | +7.99% | 532 KiB | Inconclusive: baseline noise |
| 30 | Zero sampling | +3.68% | +4.32% | 1,552 KiB | Inconclusive: baseline noise |
| 30 | Default sampling | +0.83% | +5.98% | 1,866 KiB | Inconclusive: baseline noise |

All 54 normal-mode cells pass the reader's independent validity checks:
no failed/skipped scrapes, trace/log loss or receipt mismatch, resource sampling
miss, process swap, capped/empty load or invalid CPU window. Scheduled normal
scrapes total 2,160/7,200/21,600 at the three sizes; all completed. Maximum
per-cell/per-worker scrape p95 was 4.151/9.072/24.271 ms. Throughput and RSS
budgets are met. Default-mode timed worker RSS ranges are
15,312–15,796 / 15,932–16,608 / 17,360–18,816 KiB. These are process footprints,
not allocation counts or a long-duration leak test.

Compiled-in but runtime-off versus feature-omitted is separate: CPU medians
−0.11% / +1.94% / +2.76%; p95 +0.44% / +0.14% / +4.89% at 3/10/30 workers.
Only its three-worker comparison passes the noise gate. The ten-worker
omitted baseline has one role over the 1.10 max/min limit: role 4 ranges from
652.830 to 718.895 µs (**1.10120×**). This near-boundary finding is retained,
not rounded into a pass or used to invalidate the separate, stable ten-worker
runtime-off baseline.

### Thirty-worker noise is shared, not a single slow role

All 30 roles exceed the baseline-p95 range limit in both baseline modes.
Worst ratios are **1.16311× omitted** and **1.16588× runtime off**. Runtime-off
role 19, for example, records 952.334 / 868.918 / 946.411 / 835.125 / 824.285 /
961.021 µs in round order. Aggregate baseline delivery remains stable
(max/min 1.000694× omitted, 1.000167× runtime off). No normal thirty-worker
paired p95 or CPU regression exceeds 20%; baseline variation itself triggers
the unchanged inconclusive rule.

Default thirty-worker CPU pairs are −1.84%, +16.20%, −1.88%, +3.50%, +6.61%,
−4.35%. The small median is therefore not proof that instrumentation becomes
cheaper with scale. A shared timing/host effect is consistent with the pattern;
the acceptance artifacts alone do not identify its cause.

### Full sampling remains separate high-cost/lossy evidence

| Workers | CPU median | Worst-role median p95 change | Emitted operation logs / received spans | Log contention counter total before shutdown |
| --- | --- | --- | --- | --- |
| 3 | +40.72% | +7.56% | 92,414 / 92,861 | 447 |
| 10 | +25.74% | +3.83% | 309,826 / 312,417 | 2,591 |
| 30 | +21.48% | +20.45% | 944,338 / 948,348 | 4,010 |

Counts aggregate all six cells and include their setup/lifecycle measurement
scope, not just ten-second traffic. All 18 full-sampling cells have log loss;
the observed operation-log deficit matches the contention counter total.
Other inspected pre-shutdown log-loss counters are zero. Trace accounting and
receipt checks show no trace delivery loss, but that does not restore missing
logs. All full-sampling cells retain domain correctness and clean shutdown.

The six thirty-worker full-sampling cells also miss 164–170 scheduled resource
samples each. Their CPU brackets remain 10.016–10.029 seconds, but the resource
sampling-skew gate fails; do not present their measured RSS or cost medians as
qualified bounds. Full sampling's high-cost pair checks also trigger the
reader's inconclusive status. It is not a normal-compute recommendation or
a lossless log/trace profile, and no automatic optimization campaign or
temporary cost exception is inferred from these measurements.

## Bounded host-sensor follow-up

Three predictions were declared before running: a frequency/thermal cause
should track host clock/throttle readings; scheduling contention should track
CPU pressure; generator timing should track arrival-scheduling delay even when
delivery is stable. This was a separate six-cell, thirty-worker runtime-off
diagnostic, capped at 300 seconds with 15 seconds reserved for cleanup. It
uses the same fixed-rate curve adapter and frozen worker/CLI/probe, plus
read-only host observations every 250 ms during the measurement adapter.
At most 96 observations and 128 sensor paths are allowed per cell. No worker
code or host setting changed. Sensors cover warmup and surrounding checks too,
not precisely the ten-second CPU bracket.

The first wrapper omitted the adapter's `fixed_rate` flag. It was explicitly
interrupted after **30.814 seconds outer time**, before any cell completed;
its incomplete result, original wrapper and source identities remain at
`post-curve-host-diagnostic-20260910`. Its `cleanup_clean` flag is false after
interruption, not a passed normal shutdown assertion; the subsequent process
audit found no remaining fixture processes. It is invalid-profile evidence and
cannot be pooled with either acceptance or the corrected diagnostic.

The corrected wrapper asserts fixed-rate configuration before measurement,
then schema 3, arrival profile and high-resolution CPU identity afterward.
All six cells completed in **246.690 seconds outer time** (246.498 measured
inside the runner), with source, original/corrected wrapper and executable
hashes unchanged, stopped sensor threads and clean cleanup.

| Diagnostic cell | Median role request p95, µs | Aggregate worker CPU seconds | Maximum observed temperature |
| --- | --- | --- | --- |
| 1 | 937.486 | 22.173 | 62°C |
| 2 | 879.118 | 20.321 | 63°C |
| 3 | 960.695 | 23.064 | 59°C |
| 4 | 994.364 | 22.185 | 68°C |
| 5 | 912.744 | 21.976 | 66°C |
| 6 | 918.659 | 21.509 | 62°C |

Variation reproduces: 26/30 roles exceed 1.10×, worst **1.16552×**. Every worker
completes at least 4,997 arrivals. Unlike the earlier saturated diagnostic,
readable throttle counters do not advance and maximum temperature is 68°C.
This does not support thermal throttling as this diagnostic's cause; it does
not retrospectively establish temperatures during acceptance.

Sampled CPU frequencies range about 0.39–3.80 GHz, with per-cell all-CPU medians
1.21–1.44 GHz. Those readings include idle cores, are not effective CPU clocks,
and do not prove frequency scaling caused the shift. CPU-pressure `some` totals
advance 154–169 ms over each observed adapter interval; no unique slow-cell
pressure spike explains the pattern. Median role scheduling-p95 values span
1.777–1.961 ms, but neither correlation nor six windows establish causality.
Sensor collection itself reaches **52.6–61.6 ms per observation** at its maximum;
observer perturbation is a limitation, despite zero resource-sampling misses
and 10.003–10.008-second CPU brackets. Do not call the probe free or use these
cells as replacement acceptance samples.

Artifacts under `/tmp/orishu-curve-check.so7Ibv/`:
`post-curve-host-diagnostic-20260910` (interrupted) and
`post-curve-host-fixed-diagnostic-20260910` (six complete cells).
Retained runners: `/tmp/orishu-post-curve-host-diagnostic.py` and
`/tmp/orishu-post-curve-host-fixed-diagnostic.py`. No automatic acceptance
retry or favorable-sample selection occurred.

## Remaining work and budget disposition

Charge outer times conservatively: prior 3,802.326 + full curve 2,767.200 +
interrupted probe 30.814 + corrected probe 246.690 = **6,847.030 seconds**,
or **114m07s of 120 minutes** excluding builds. **5m53s** remains. Ordinary
contract/documentation checks are recorded separately, not as load experiments.

The formation correction and normal three/ten-worker instrumentation results
are supported. Thirty-worker normal-mode stability and the ten/thirty-worker
compiled-in-cost comparisons remain inconclusive. Neither small medians nor
a near-threshold ratio waive the accepted noise rule. Full sampling remains
explicitly lossy/high-cost and unsuitable as the normal monitoring profile.

Next options require review before another acceptance batch: establish a
controlled performance host/profile (any governor/affinity changes need explicit
permission), or prospectively redesign and validate the statistical experiment
while retaining this batch as inconclusive. A new complete curve cannot fit
the remaining allowance. Do not silently relax the noise gate, restart until
green, change runtime defaults from this evidence, or mark combined M4 complete.

Recommended next diagnostic, **pending host-change approval**: at most five
minutes within the remaining allowance, compare the existing governor with a
temporary performance governor while keeping affinity, worker thread defaults
and workload unchanged, then restore the exact original policy. Validate that
the platform exposes the requested control and that restoration is available
before changing anything. This tests a hypothesis, not a guaranteed fix; it
affects host-wide power/thermal behavior and must not start implicitly. The
alternative is a prospective measurement-method review without host changes.
Neither route authorizes a further full curve or reclassifies this one.

### Governor preflight and physical-host handoff

The operator approved the temporary governor diagnostic, then requested a
real 3/5-Pi setup path if local tuning could not yield acceptable evidence.
Read-only inspection found 20 `intel_pstate` policies, all `powersave`, with
`performance` available. However, the scoped `sudo -n true` privilege check
returned `sudo-rs: interactive authentication is required`. The governor test
was **not run**: no writes, changed policies, restoration operation, load cells
or new performance result. No credentials were requested or alternate
privilege path attempted. The experimental debit remains 114m07s; this was
a setup/authority preflight, not another timed-load experiment.

The [physical Pi setup guide](../testing-worker-pi-cluster.md) and
[implementation task](../tasks/implement-physical-formation-experiment.md)
now record software, SSH trust, per-node preparation, bounded readiness
collection and remaining real-network harness work. Local tuning no longer
blocks that preparation. Actual model/OS/address/access details and hardware
verification remain necessary; three/five Pis do not certify thirty workers.
