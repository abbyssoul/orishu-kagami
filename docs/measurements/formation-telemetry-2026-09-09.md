# Quiet-host formation telemetry curve — 2026-09-09

Status: **incomplete; no overhead or scalability acceptance**.
Plan: [approved 3/10/30-worker experiment](formation-telemetry-plan.md).
Owner: [formation M4](../tasks/cluster-formation-m4-checklist.md).

Follow-up: [reliability investigation and fixes](formation-reliability-2026-09-09.md).
The record below describes the original failed run; later diagnostics do not
replace its cells, change its deadlines or grant overhead acceptance.

## Outcome

The operator provided a quiet host window and explicitly authorized the full
curve. The real run completed all **36 three-worker cells**, then stopped at
the **first ten-worker cell**, with telemetry features omitted, because a
formation observation/setup deadline expired. That cell never entered its
timed request window. There are **36 complete cells, one failed cell and 71
unexecuted cells**. No ten-worker overhead or thirty-worker result exists.

The run stopped after approximately **10 minutes 26 seconds**, derived from
manifest and failed-cell artifact timestamps, not because the 90-minute batch
budget was exhausted. No automatic retry, deadline extension, weaker readiness
condition or replacement formation was used. The failed full run remains intact.

The three-worker results are useful observations but **inconclusive** under
the approved variation rule. They do not establish that instrumentation meets
the normal below-10% cost goal or that thirty workers operate acceptably.

## Three-worker observations

Each row has six paired rounds. Percentage changes use the same round's
`compiled_off` baseline, except `compiled_off` itself, which compares against
`omitted`. The p95 score is the worst worker role's median paired change; CPU
is aggregate worker CPU seconds per validated completed request. No p95s were
pooled into a synthetic request percentile.

| Configuration | p95 change | CPU/request change |
| --- | ---: | ---: |
| Compiled capabilities, runtime off vs omitted | +3.09% | +4.14% |
| Metrics/probes with scrapes | +0.91% | +0.02% |
| Metrics, zero sampling, enabled lifecycle logs | +3.55% | +5.14% |
| Metrics, 1000 ppm tracing and stdout logs | +6.81% | +15.66% |
| Metrics, full sampling and stdout logs | +110.38% | +124.17% |

For absolute context, median aggregate throughput was approximately 75,179
validated requests/s with capabilities compiled but disabled, 75,232 with
metrics, 72,532 with default-sampled tracing/logs, and 42,344 with full sampling.
The median of each cell's worst-worker p95 was respectively 137.21, 137.47,
143.72 and 285.77 microseconds. These are summaries of cell measurements, not
global percentiles or a scientific workload throughput claim.

Important limitations and failures:

- Runtime-disabled aggregate throughput ranged from **71,776 to 127,633
  requests/s** across rounds: max/min **1.778**, above the allowed **1.10**.
  Per-role baseline p95 max/min ratios were **1.923–1.960**. These baseline
  variations alone prevent unqualified acceptance; no faster/slower round
  was discarded. The cause of that variation has not been established.
- Default-sampled CPU's median change lies in the operator's temporary-only
  band, not the normal goal. It is not a recommendation to enable it
  indefinitely or proof that every pair stays inside the temporary tolerance.
- One zero-sampling cell lacked worker role 2's final
  `orishu.worker.stopped` log record. Its other lifecycle records and all 12
  final trace-accounting records were received, and the worker exited cleanly.
  Pre-shutdown log counters had no loss. The missing final lifecycle record
  needs an explicit shutdown-delivery disposition; it is not silently waived.
- Every full-sampling cell reported log loss and log/received-span count
  differences. Across these six cells the resource sampler missed **191
  scheduled intervals**. This is a high-cost diagnostic profile with
  measurement-quality limitations, not acceptable normal compute monitoring.
- Metrics, zero sampling and default sampling had no missed resource samples.
  The metrics and default-sampling cells had no report-reader instrumentation
  issues; that does not override baseline variation or the CPU cost target.

## Bounded formation diagnosis

The original failed ten-worker cell lasted approximately **36.89 seconds**
including cleanup after the preceding cell's artifact. This was before the
60-second whole-setup allowance. Its retained error does not identify which
ten-second observation predicate expired; the harness needs more specific
failure-phase evidence for future runs.

Two explicitly separate formation-only diagnostic invocations then reused the
same frozen binaries, omitted telemetry, join chain and unchanged deadlines.
They did not run timed client load or replace any measurement cell:

1. One ten-worker formation completed in **35.327 seconds**. Exact membership
   convergence after the joins took approximately **6.13 seconds**, followed
   by successful summary and lock/unlock visibility checks.
2. Another invocation reported **`catchUpFailed` for the sixth worker** after
   the preceding four applicants had joined. The failure was observed around
   **9.36 seconds** into the diagnostic, not at the whole-setup deadline.

Thus ten-worker formation can succeed, but the exercised path is not reliably
forming under this profile. The diagnosis establishes an intermittent catch-up
failure as well as the original unclassified observation timeout; it does
**not** establish that both share one root cause. Public join status provides
the phase but not a detailed failure reason. A formation/catch-up investigation
is required before rerunning the curve. Increasing benchmark deadlines alone
would not address the observed `catchUpFailed` state.

## Reproduction and retained evidence

Both release variants were refreshed using the locked offline recipes before
measurement. The 19 measurement-tool Python tests passed. The host check found
no competing build/test, and the runner's guard did not interrupt any cell.
Normal desktop services remained running; no host governor, affinity or service
policy was changed. The host was a Core i7-1370P with 20 logical CPUs, affinity
0–19 and observed `powersave` scaling policy. These are single-host results,
not cross-machine or scientific compute scaling evidence.

```sh
python3 scripts/measure-formation-telemetry.py --worker target/formation-telemetry-enabled/release/orishu-worker --omitted target/formation-telemetry-omitted/release/orishu-worker --ctl target/formation-telemetry-enabled/release/orishuctl --probe target/formation-telemetry-enabled/release/examples/formation-telemetry-probe --otelcol /tmp/orishu-otelcol-check.JlHHuV/otelcol --output /tmp/orishu-curve-check.so7Ibv/full-quiet
python3 scripts/summarize-formation-telemetry.py /tmp/orishu-curve-check.so7Ibv/full-quiet
```

The first command was executed once and exited **2** at the failed cell. The
second reads the preserved partial matrix without completing or accepting it.
The output directory already exists and must never be reused for a new run.

The manifest was written at **2026-09-09 03:06:00 UTC**, and the failed-cell
artifact at **03:16:26 UTC**. HEAD was
`51251791654425566d8392848b347f32c7258cb9` plus the manifest's complete dirty-source
inventory. That inventory was compared after the run, before documentation
edits, and was **unchanged**. The failed run has no `finished.json`; this
separate audit must not be represented as a completed-run marker.

Actual compiler: **rustc 1.97.1**, host `x86_64-unknown-linux-gnu`, LLVM 22.1.6.
Collector: **0.160.0**. The causal preflight passed with **115 matched
spans/logs**, both admission chains, the 172-series catalogue and probes.
Earlier intermittent preflight failures remain historical open findings,
not retroactively erased by this pass.

| Artifact | SHA-256 |
| --- | --- |
| Run manifest | `e812951546d371cee7fe0d7a6ba2c63a22e5600c18869a0a8796d2fcf863f035` |
| Approved plan at execution | `b40dddfa52cd57751c2ac1da2527a600f4a9e42840f580c73dcec0c7297abc5f` |
| Combined-capability worker | `c87c5853d27c723dc836e5d47fd576f666ccea43c747188ac8b0758f5979a4f5` |
| Feature-omitted worker | `9c86f210cac2da34cda96b7600d12bed4d671c483bfb71cb0c76369039f1f296` |
| Operator CLI | `ca2ea5d6837b0c8dc3a6149c47eccacb506a02ebb60f1d34f142a542d53251ff` |
| Measurement probe | `6754e6f159943f4a0352764666b1869eeefbe22b4c53a4639995f2633197b246` |
| Official Collector | `abe338fa33865e54412566db5cea4adc82374594b65b49c1099048cdf8cf2451` |

The run directory retains private executable snapshots, `manifest.json`,
`preflight.json` and `cell-000.json` through `cell-036.json`. Its parent retains
the bounded diagnostic script `diagnose_setup.py`, `formation-diagnostic.json`
and `formation-diagnostic-second.json`, separately labelled as diagnostic
evidence. These `/tmp` artifacts are local and may disappear on reboot; this
document is not a replacement for the raw data or a portable published archive.

All fixture children exited and were reaped; a host process check found no
remaining worker/collector from these frozen paths. Temporary credentials and
sockets were removed by fixture cleanup. Retained reports and binaries were
not deleted, and no unrelated process was stopped. No production source,
protocol, runtime limit or performance threshold was changed by this run.

## Shutdown-log follow-up — 2026-09-09

Separate from the failed run: `cell-003.json`, zero sampling, worker role 2
retained all twelve final trace-accounting records and `ready`/`stopping`, but
not `stopped`; the reader reported no parse errors. The last scrape preceded
shutdown and therefore cannot identify the final loss counter. The exact
historical scheduling event remains unobservable, not proven retrospectively.

A focused regression reproduced this symptom with a healthy output sink:
write the twelve accounting records, hold the writer's queue-bookkeeping lock,
then execute the worker's final lifecycle/close sequence. The original sequence
cleanly drained but shed `stopped` with `contended = 1`. A real-pipe EOF test of
the measurement reader retained all twelve counters and the final lifecycle
record, providing no evidence for the proposed reader-truncation explanation.

The worker now offers `stopped` inside the same queue-closing transaction already
used by output shutdown, after producers stop. Ordinary producers still use
non-waiting try-lock admission. No queue enlargement, reserved slot, capacity
wait/retry, synchronous fallback or drain-budget change was introduced. The
regression passes with thirteen written records and no contention loss.
Additional tests retain explicit full-queue refusal, held-sink cutoff with
unconfirmed versus never-started records, and terminal failed-output behavior.
The real-worker tracing test now asserts final `stopped` and all twelve
accounting records, including at zero sampling.

One separately declared three-worker zero-sampling diagnostic then exercised
the actual curve `Cell.run`: exact formation/policy checks, the original
ten-second concurrent request window, metrics scrapes, real worker shutdown
and the normal bounded pipe reader. It ran once, exited zero, and received
exactly fifteen records (1,941 bytes) from each worker: three lifecycle records
and twelve accounting records, with no parse errors. Setup took **3.131 s**,
the whole check **13.474 s**; all fixture children exited and were reaped and
artifact hashes remained unchanged. No comparative performance inference is
made from this single diagnostic.

Retained local artifacts:
`/tmp/orishu-curve-check.so7Ibv/shutdown-log-fixed-zero-sample/result.json`
contains the source inventory, profile, artifact hashes and complete cell
results; `/tmp/orishu-shutdown-log-diagnostic.py` is the explicitly labelled
debug fixture. The combined-capability release worker SHA-256 was
`e63717c4bb0d87a0423047b2dd889a32e0614759ffe1ce2a02fbdcabcdd86606`;
CLI and probe are the unchanged frozen artifacts listed above. The fixture
uses the curve's bounded receipt collector, not another official-Collector
causal preflight. Temporary private credentials/sockets were cleaned; the
original curve and its failures were not replaced. These local `/tmp` artifacts
are not a portable published archive.

This closes the demonstrated final-event contention defect at the tested
checkpoint, not a lossless-logging guarantee or final M4 acceptance. Reconcile
its applicability at the next source freeze and retain required shutdown/log
receipt assertions in any approved subsequent curve. Baseline noise,
full-sampling overhead/loss, per-size budget feasibility and combined acceptance
remain open.

Validation at this follow-up: worker all-target suites passed with default
features (**240 passed, 1 ignored**) and both optional capabilities
(**331 passed, 3 ignored**). The ignored entries remain the held-stdout helper
and manual profiling fixtures; the real pressure/exit regressions ran. Worker
both-feature all-target clippy passed with warnings denied; worker doc tests
had zero cases. All **29** curve-tool contract tests, workspace formatting and
`make docs-check` (**132 Markdown files**) passed. This is affected-worker
validation, not a new full-workspace/fault-matrix acceptance checkpoint.

## Baseline variation review — 2026-09-09

Read-only analysis of the retained cells, not another measurement attempt.
The unchanged report reader still returns `inconclusive_noise` for all five
three-worker comparisons. Three explanations were checked against the raw
records: a common early/late effect, one unstable worker role, and distorted
measurement windows. The data narrows the next experiment but does not prove
a thermal, frequency, scheduler or runtime root cause.

| Round | Disabled-baseline cell | Aggregate requests/s | Worst role's p95, µs | Aggregate worker CPU, s | CPU/request, µs |
| --- | --- | ---: | ---: | ---: | ---: |
| 0 | 001 | 127,633.2 | 73.542 | 18.50 | 14.495 |
| 1 | 006 | 75,764.1 | 133.504 | 18.10 | 23.890 |
| 2 | 017 | 71,775.8 | 141.972 | 18.04 | 25.134 |
| 3 | 022 | 75,250.3 | 138.281 | 18.36 | 24.399 |
| 4 | 027 | 75,108.1 | 136.133 | 18.21 | 24.245 |
| 5 | 032 | 74,287.2 | 140.810 | 18.20 | 24.500 |

All three roles shift together: round 0 per-role p95 values are
72.265/72.319/73.542 µs; round 1 values are 131.801/130.792/133.504 µs.
This is not variation confined to one role. The six baseline CPU brackets are
10.001439–10.002606 seconds, with no missed resource samples, capped request
buffers or process swap. Those recorded defects therefore do not explain the
large throughput/p95 shift. Similar CPU time with less completed work does not
by itself identify why each request consumed more CPU time.

The onset precedes the first full-sampling cell. Cells 0–2 (omitted, disabled,
metrics) delivered 122,662–129,640 requests/s. Cell 3 (zero sampling) delivered
110,414 requests/s; cell 4 (default sampling) delivered 70,213 requests/s and
already had a worst-role p95 of 144.403 µs. Full sampling first occurs in cell 5.
It cannot be the sole cause of the initial slowdown, though later carry-over
effects are not ruled out.

For diagnosis only, the *later* five baselines have throughput max/min **1.056**
and per-role p95 max/min **1.054–1.084**. This describes concentration of the
variation in the first round; it is **not permission to discard round 0**, call
the remainder an accepted five-round experiment, or relax the 1.10 rule.
The first default-sampling pair also has CPU/request **+100.55%**, while later
pairs have **+15.02%, +8.31%, +16.30%, +13.87%, +17.67%**. Four of those five
later CPU changes still exceed the normal below-10% goal. Even successful
baseline stabilization would not establish acceptable default instrumentation
cost without a valid new comparison.

The manifest records a Core i7-1370P, affinity 0–19 and 20 logical CPUs. Cell
host snapshots include aggregate CPU/pressure/memory counters and an unchanged
`powersave` governor name, but not effective clock frequency, thermal state,
or per-thread CPU placement. These data cannot discriminate those causes.
No governor, affinity, process policy, runtime setting, report classification
or original artifact was changed in this review.

**Proposed, awaiting operator approval:** one baseline-only diagnostic, six
fresh three-worker cells, at most five minutes, with bounded read-only host
measurements. Preserve the existing cell/load/identity checks and record every
cell; do not change host policy, retry failures or start the full curve
automatically. Count this allowance within the next reviewed 90-minute budget.
Its result must be reviewed before proposing conditioning, affinity or other
measurement-policy changes. The separate proposal to allow up to 45 minutes
for the 30-worker point while retaining 90 minutes total also remains
unapproved. Neither proposal is implemented or authorized by this analysis.

Full-sampling overhead/loss remains a high-cost troubleshooting-profile
finding, not permission for an optimization campaign or acceptable normal
monitoring. The functional/runtime and operator-recipe gates are verified at
the [current checkpoint](../tasks/cluster-formation-m4-checklist.md#current-build-operator-recipe-verification--2026-09-09);
measurement-policy direction is now required before further performance work.

## Next bounded work (at the original failed-run checkpoint)

Investigate reliable formation/catch-up at ten workers, retaining explicit
phase/role/timing and bounded failure reasons before teardown. Establish a
focused failing regression and fix the demonstrated cause without relaxing
identity, liveness, admission or catch-up correctness. Separately review the
unstable timing baseline and shutdown-log disposition before a new acceptance
run. Preserve this failed run and the existing three-worker evidence; neither
a successful diagnostic nor a later run may replace them. Thirty-worker
behavior and final M4 acceptance remain unverified.
