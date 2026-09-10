# Fixed-rate formation telemetry — 2026-09-10

Status: **83 complete cells; stopped at thirty-worker unlock; overhead and M4
acceptance remain open**.
Plan: [approved fixed-rate profile](formation-telemetry-plan.md#fixed-rate-acceptance-profile-v3--2026-09-10).
Owner: [M4 checklist](../tasks/cluster-formation-m4-checklist.md).

The operator explicitly approved fixed-rate traffic after the
[saturation diagnostic](formation-baseline-diagnostic-2026-09-10.md). This
experiment measures instrumentation overhead at 500 requests/s/worker, not
maximum capacity. Saturation artifacts remain separate and unchanged.

## Implementation and validation

- The release-only typed-client probe schedules absolute, phase-staggered
  arrivals. It bounds concurrency, skips expired slots without a queue, and
  retains request latency, scheduling delay, scheduled-response latency,
  skips and post-window completions. Every worker's 5,000 arrivals reconcile;
  at least 99% must complete inside the window.
- V3 CPU uses the per-process POSIX CPU clock in nanoseconds; `/proc` ticks
  remain secondary evidence. Native clock lookup failures remain errors.
- Manifest, cell, workload and CPU-accounting identities distinguish v3 from
  historical v2. The report reader rejects mixed workload pairs and mismatched
  high-resolution CPU values while retaining all loss/variation findings.
- Budget accounting implements 20/20/45-minute size slices, five reserved
  diagnostic minutes, preflight inside the first slice, an explicit previous
  validation debit, and cleanup inside the remaining budget.
- No worker runtime source, allocator, dependency, peer protocol, host policy,
  or overhead threshold was changed. Both release feature builds and the
  probe were rebuilt before measurement.

Checks: 35 curve/accounting Python contracts, four diagnostic observer
contracts, six Rust probe tests, targeted probe Clippy with warnings denied,
formatting, documentation and whitespace checks pass. Initial CPU-clock test
development caught that Python lacks `clock_getcpuclockid`; the implementation
now uses the native POSIX function with explicit argument/error handling.
The real HTTP probe test initially encountered sandbox-denied sockets and
passed with scoped loopback permission. These were development/test failures,
not retried measurement cells. Builds report the existing dependency
`proc-macro-error2` future-compatibility warning.

## Six-mode smoke matrix

Artifact directory:
`/tmp/orishu-curve-check.so7Ibv/fixed-rate-smoke-20260910`.
All six formation/load/shutdown paths completed once, with unchanged source
and frozen executables, in **82.026 seconds**. Each worker completed
4,998–5,000 of 5,000 scheduled arrivals (at least 99.96%). All missing arrivals
are explicitly recorded as skipped or post-window completions, not successes.
The official Collector causal preflight also passed.

The report reader flags log loss and resulting log/trace count mismatches in
all three full-sampling workers. Other smoke modes have no reported validity
issues. Full sampling remains a separate high-cost profile, not lossless
telemetry acceptance. This smoke has only one round at three workers and cannot
establish overhead bands, baseline stability or scalability.

Executable SHA-256 values:

- Combined worker:
  `b1b333c09d9cc6f9b11ab1d5d5db5ee96d08f6be9c61605d412190e2db208d7a`.
- Feature-omitted worker:
  `6ace0838c69c00e7b32872093a19555cd5df9479c7f67aa539ce0dc90fb6d801`.
- Probe:
  `6ea2f8e96e6cb045afb6d58f85280c94e651a17e3a5d9a7f1eb8c1f1deab3e01`.

The full curve debits **83 seconds** (rounded up) from the three-worker and
whole-batch budgets. Its remaining slices are 1,117/1,200/2,700 seconds, totalling
5,017 seconds. Together with the 83-second debit and 300-second diagnostic
reservation, this stays within the approved 5,400 seconds excluding builds.
No smoke cell is reused as an acceptance round.

## Full-curve execution

Retained directory (do not overwrite or resume):
`/tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910`.

```sh
python3 scripts/measure-formation-telemetry.py --fixed-rate \
  --prior-validation-seconds 83 \
  --worker target/formation-telemetry-enabled/release/orishu-worker \
  --omitted target/formation-telemetry-omitted/release/orishu-worker \
  --ctl target/formation-telemetry-enabled/release/orishuctl \
  --probe target/formation-telemetry-enabled/release/examples/formation-telemetry-probe \
  --otelcol /tmp/orishu-curve-check.so7Ibv/full-quiet/otelcol \
  --output /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910
```

The run stopped once at zero-based **cell 83**, thirty workers, second round,
feature-omitted mode. It retained 36 complete three-worker cells, 36 complete
ten-worker cells, 11 complete thirty-worker cells and one failed setup;
24 cells were not executed. Elapsed execution from retained artifact timestamps
was **2,228.041 seconds** (37 minutes 8 seconds). No failed cell was retried,
discarded or replaced, and no timed load ran in the failed cell.

All owned children cleaned up. The separate `post-failure-integrity.json`
verifies unchanged source and frozen artifacts immediately after failure,
before subsequent harness/documentation edits. Its manifest SHA-256 is
`5067295dda6622a9f9af96fa9234fd466e31e3a671f9746933d066af8ee2492a`.
There is deliberately no `finished.json`: the reader's
`source_stability_verified: false` means no completed-curve certification;
the sidecar is narrower post-failure integrity evidence, not completion.

### Rate delivery and descriptive scaling

Every completed normal-profile cell passes the reader's workload, identity,
window, receipt/loss and resource validity checks. All completed thirty-worker
cells deliver at least 4,993/5,000 arrivals per worker (99.86%). That demonstrates
the declared offered rate in those cells, not full scalability acceptance.

The following uses runtime-disabled, features-compiled-in baselines only.
Latency columns are the worst role's median p95 across available rounds; CPU
is median aggregate worker CPU time per ten-second window, not host utilization.
The thirty-worker values have **two baselines, not six**, and are descriptive.

| Workers | Baselines | Completed requests/s | Request p95 | Scheduled-response p95 | Scheduling-delay p95 | Worker CPU seconds/window |
| --- | --- | --- | --- | --- | --- | --- |
| 3 | 6 | 1,499.85 | 0.708 ms | 2.405 ms | 1.955 ms | 2.590 |
| 10 | 6 | 4,999.05 | 0.712 ms | 2.397 ms | 1.904 ms | 8.897 |
| 30 | 2 (partial) | 14,995.75 | 0.913 ms | 2.782 ms | 2.079 ms | 21.871 |

Scheduling delay is material and is not included in the request-latency score;
the scheduled-response column prevents confusing that score with end-to-end
arrival latency. The frozen phase formula is `4 ms * client_index / (2 * N)`,
with adjacent client indices assigned to each worker. Per-worker pair spacing
therefore varies with cluster size even though the offered rate and client
count per worker remain fixed. This single-host control-plane profile is not a
maximum-capacity, production tail-latency or scientific-workload claim.

### Paired overhead: measured, not accepted

Values below are medians of six within-round comparisons against compiled-in,
runtime-disabled baselines. **Every normal metrics/zero/default comparison is
`inconclusive_noise`**: the worst baseline p95 max/min ratio is 1.125 at three
workers and 1.119 at ten, above the unchanged 1.10 variation limit. Stable
fixed-rate throughput does not imply stable latency. No rounds were removed.

| Mode | 3-worker p95 change | 3-worker CPU change | 10-worker p95 change | 10-worker CPU change |
| --- | --- | --- | --- | --- |
| Metrics | +0.91% | +2.35% | +2.54% | +3.69% |
| Zero sampling, logging enabled | +2.26% | +4.03% | +3.31% | +6.58% |
| Default sampling, logging enabled | +5.14% | +10.52% | +5.83% | +11.70% |
| Full sampling (troubleshooting) | +8.08% | +42.78% | +6.57% | +32.63% |

Default sampling's CPU medians also exceed the normal below-10% goal. They
fall in the temporary-only band; no time-limited exception is granted by this
report. Full sampling exceeds 25% CPU and all 14 completed full-sampling cells
have log loss/count mismatch flags; two also have measurement-scheduling skew.
It is neither normal monitoring nor lossless troubleshooting acceptance.
The remaining invalid artifact is the failed setup (15 invalid artifacts total).

Compiled-in/runtime-off versus omitted-feature comparisons have median p95/CPU
changes of +0.22%/−0.93% at three workers and +0.16%/−1.69% at ten. The reader
labels these `measured_requires_gate_review`, not automatic acceptance.
Thirty-worker pairing is incomplete; do not extrapolate six-round overhead
medians from the available one/two pairs.

### Thirty-worker setup failure and bounded diagnostics

The failure is `formation observation/setup deadline: unlock`. All 29 joins
completed with correct identities; every final view reports 30 alive members
and introducer readiness. The last sequential observations still show locked
policy on zero-based roles 10, 12 and 19. Other roles show unlocked policy.
Successful thirty-worker setup times were 41.096–59.108 seconds, close to the
unchanged shared 60-second limit in the slowest cell.

The failed artifact originally retained final views but not intermediate stage
timings or the unlock-acceptance timestamp. It cannot distinguish insufficient
remaining budget from a propagation stall, nor establish simultaneous policy
state across sequential reads. There is no evidence here of an incorrect join
identity or permanent unlock failure.

Four separately labeled, formation-only diagnostics used the frozen omitted
worker and the original setup predicates/deadline. These are not acceptance
retries or replacement cells. Each had a 90-second bound including cleanup;
at most 20 seconds of post-failure policy observation could explain a timeout
without reclassifying it. No such extension was used.

| Artifact directory under `/tmp/orishu-curve-check.so7Ibv/` | Outcome | Elapsed |
| --- | --- | --- |
| `policy-setup-diagnostic-20260910` | Complete; exact-members stage 35.602 s; lock/unlock observations 4.298/3.748 s | 49.636 s |
| `policy-stage-series-20260910-1` | Failed before formation: diagnostic ephemeral-port reservation collision; no retry | 0.314 s |
| `policy-stage-series-20260910-2` | Complete; exact-members stage 36.496 s; lock/unlock observations 4.036/4.946 s | 51.276 s |
| `policy-stage-series-20260910-3` | Complete; exact-members stage 36.884 s; lock/unlock observations 3.511/3.830 s | 50.236 s |

All retained `result.json` files verify clean cleanup and unchanged source and
artifacts during their own runs. The temporary timing helper
`/tmp/orishu-policy-setup-diagnostic.py` and its hash are diagnostic artifacts,
not a supported acceptance command. The series ran exactly three attempts
regardless of outcome. The first diagnostic issued 1,110 sequential list
commands during exact-members verification, taking 33.736 seconds of command
wall time. This shows the cost of convergence **and observation together**;
it does not prove that parallelizing verification removes propagation time.

The original unlock failure did not recur. The leading unresolved questions
are how much setup budget unlock had left, whether policy propagation stalls
independently of membership, and whether a slow verification sweep crosses
the gate after nodes converge. A deterministic or higher-frequency reproducer
with per-node policy timestamps is needed before selecting a runtime fix.

The demonstrated harness evidence-loss defect was fixed separately: bounded
per-stage start/deadline/end/check/acceptance records and partial join/policy
timings now survive exceptions, with policy submission/acceptance timestamps.
A regression first failed on missing timeout evidence, then passed after the
fix. The record count is capped at 128. No polling interval, concurrency,
deadline or worker behavior changed; late observations still fail. These edits
postdate the frozen acceptance source and do not retroactively enrich cell 83.

Charging the full 300-second diagnostic reservation, rounded 83-second smoke,
2,228.041-second failed curve and all 151.463 seconds of subsequent diagnostics
uses **2,762.504 seconds** (46 minutes 3 seconds) of the approved 90-minute
allowance excluding builds. Unused time is not permission to retry the matrix,
change its acceptance limits or discard failures.

## Disposition and next work

The subsequent [convergence diagnostic](formation-convergence-diagnostic-2026-09-10.md)
reproduces slow membership with persistent clients and narrows the investigation
to peer establishment/dissemination, with retained diagnostic failures and
updated cumulative time accounting. It does not complete or replace this curve.

- **Problem:** the offered-rate experiment executes at 30 workers, but setup
  reliability and baseline latency variation prevent full overhead acceptance;
  default sampling CPU is also outside the normal target in current medians.
- **Tried:** fixed-rate pacing/high-resolution CPU accounting; one complete
  attempted curve; bounded timed-stage diagnostics. Earlier saturation,
  conditioning and allocation evidence remain in the linked diagnostic report.
- **Next:** establish a repeatable policy/convergence failure with the retained
  stage evidence; separate observer cost from peer propagation; only then
  change the demonstrated cause and regression-test the real path. Investigate
  default-sampling CPU and baseline p95 variation separately from full sampling.
- **User action:** none to accept or reinterpret these results. M4 remains
  open. Any proposed relaxation of the shared setup/noise gates, temporary
  overhead exception, host-policy change, or new full acceptance batch needs
  an explicit decision with its trade-offs. No such change is made here.
