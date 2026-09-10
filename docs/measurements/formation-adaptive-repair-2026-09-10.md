# Sparse-chain formation repair — 2026-09-10

Status: **bounded runtime correction verified in replay and native setup;
instrumentation overhead and final M4 acceptance remain open**.
Owner: [M4 checklist](../tasks/cluster-formation-m4-checklist.md).
Preceding evidence: [observer/peer-metrics diagnostic](formation-convergence-diagnostic-2026-09-10.md)
and [failed fixed-rate curve](formation-fixed-rate-2026-09-10.md).

Follow-up: the [paced-sampling diagnostic](formation-sampling-diagnostic-2026-09-10.md)
now verifies a separate trace-producer optimization and the newer combined
worker. This report retains the formation scheduler's own comparison and
artifact identities; its scoped passes do not certify the full overhead gate.

## Problem and causal evidence

The failed curve exhausted its original 60-second setup deadline during
thirty-worker unlock. Separate native formations repeatedly needed tens of
seconds to acquire complete, all-alive views, even with persistent observers.
Existing metrics showed failed early handshakes/sends and probe deadlines as
the graph became connected, without corresponding TLS or datagram-submit
failures on the sampled nodes.

The bootstrap dependency is legitimate: A cannot bind C's member session until
trusted membership dissemination has introduced C. A sparse introducer chain
therefore needs gossip over existing routes to establish additional routes.
Ordinary SWIM must still probe disconnected Alive/Suspected members; selecting
one of those members cannot deliver its piggybacked news. Failed sends restore
deferred gossip, and five-second anti-entropy on established routes eventually
repairs discovery. Waiting predominantly on that idle repair cadence prolongs
the sparse graph and creates further probe failures/refutations.

A deterministic replay reproduces 25–35 simulated seconds to thirty-member
all-alive convergence without kernel networking, TLS computation, process
startup, thermal effects or allocator stalls. Expediting existing repair
shortens the same seeds to 10–13 seconds. Native old/candidate comparisons then
show the same direction. This establishes a causal scheduling bottleneck,
not a claim that every historical unlock failure has one proven cause or that
the native failure probability is now zero.

This is distinct from the earlier three-worker saturation slowdown. That
[baseline diagnostic](formation-baseline-diagnostic-2026-09-10.md) observed
thermal throttling and reduced CPU clocks, low idle peer CPU, and short-lived
HTTP/serialization allocation churn. The allocations are real optimization
leads, not demonstrated causes of either the saturation cliff or sparse-chain
timer delay. No allocator or host policy was changed here.

## Alternatives tested and retained change

- Historical control: ordinary SWIM, five-second reconciliation.
- Diagnostic counterfactual: restrict probes to connected routes. Thirty-node
  model convergence improves to 15 seconds, but this would suppress meaningful
  failure detection. It is confined to the test fixture and **not deployed**.
- Retained correction: retain ordinary SWIM, but attempt existing anti-entropy
  at most once per second while membership gossip is queued. Return to minimum
  five-second spacing with an empty queue.

The new private `reconciliation.rs` helper owns one timestamp. The owner's
existing skipped-tick one-second probe wakeup supplies time and the core's
queue/active-round state. The helper prevents overlapping rounds and missed-tick
bursts. It does not renew a round deadline. Selection still requires an
existing authorized member route; wire version, authentication, admission,
removal fences, SWIM eligibility and per-round resource limits are unchanged.
There is no new dependency, public configuration key or protocol migration.

This is a latency/bandwidth tradeoff, not a free optimization. Continuous valid
news can keep repair attempts at 1 Hz, up to five times the idle cadence.
Thirty-node replay bytes over formation plus lock/unlock increase by
12.5–32.9% across the three seeds, despite fewer transitions and failed sends.
That is membership traffic, **not measured instrumentation CPU overhead**.
Steady-state and feature-overhead measurements must use the changed worker.

## Replay boundary and results

`peer::session::formation_replay` constructs chain snapshots through real core
admission, then exercises production membership transitions, timer outcomes,
peer serialization/decoding and authenticated session binding. TLS facts,
post-admission installation, instantaneous transport, dial scheduling and random
selection are fixtures. It mirrors canonical dialing with four candidates and
64 scanned members per tick, but not real handshake capacity/delay. Invalidated
sessions are closed in both directions. Missing-route and wire-truncated gossip
use the real deferred-news path. This is not a substitute for real catch-up,
concurrency, partition or transport tests.

The replay is bounded to 200,000 transitions, 50 ms simulated quanta and one
shared 60-second simulated deadline across membership, lock and unlock. Three
sizes, three seeds and three alternatives produce 27 cases. The retained
candidate calls the actual scheduler helper. Results are in the private file
`/tmp/orishu-formation-replay.2QPV2B/results.jsonl` (27 rows, create-new only).

| Workers | Seed | Old membership / lock / unlock | Candidate membership / lock / unlock |
| --- | --- | --- | --- |
| 3 | 11 | 0 / 0.5 / 1.5 s | 0 / 0.5 / 1 s |
| 3 | 29 | 0 / 1 / 2 s | 0 / 1 / 1.5 s |
| 3 | 47 | 0 / 1 / 3 s | 0 / 1.5 / 2 s |
| 10 | 11 | 10 / 12 / 15 s | 5 / 6 / 8 s |
| 10 | 29 | 10 / 13 / 16 s | 3 / 4 / 6 s |
| 10 | 47 | 5 / 8 / 10 s | 3 / 5 / 7 s |
| 30 | 11 | 25 / 30 / 35 s | 10 / 12 / 14 s |
| 30 | 29 | 35 / 39 / 41 s | 13 / 15 / 17 s |
| 30 | 47 | 35 / 40 / 45 s | 11 / 13 / 16 s |

Times are cumulative simulated observations, not separate phase durations or
native startup SLOs. Three-node zero time follows from instantaneous fixture
installation and initial exchanges. Counters in the JSON include all phases.

Reproduce the optional diagnostic with a fresh private report path:

```sh
ORISHU_FORMATION_REPLAY_REPORT=/tmp/private-new-dir/results.jsonl cargo test --locked --offline -p orishu-worker --lib chain_dissemination_diagnostic --target-dir target/formation-telemetry-enabled -- --ignored --nocapture
```

The normal, non-ignored ten-node regression
`sparse_chain_repairs_news_without_suppressing_disconnected_probes` fails when
the helper is restored to five-second spacing and passes with adaptive repair.
It also requires disconnected probes and unknown-member handshake refusals to
remain present, then verifies lock/unlock and all-alive convergence. Three
scheduler tests cover cadence, active-round preservation and missed wakeups.

## Native fixed diagnostic sequence

Six predeclared formation-only cases ran sequentially, without timed load or
concurrent builds/tests: old-30, candidate-30, candidate-30, old-30, candidate-3,
candidate-10. No failed case was retried. All use the curve's exact public
identity, fingerprint, membership, liveness, readiness and lock/unlock checks;
startup/join keep ten-second observations and all setup stages share the
original 60-second deadline. Every case passed and cleaned up, with unchanged
executable identities. Source identity was stable across the sequence.

Directories below are under `/tmp/orishu-curve-check.so7Ibv/`; each contains a
bounded `result.json` with executable/source identities and stage observations.

| Directory | Workers | Setup | Lock / unlock |
| --- | --- | --- | --- |
| `adaptive-repair-old-1-20260910` | 30 | 44.974 s | 4.376 / 3.654 s |
| `adaptive-repair-candidate-1-20260910` | 30 | 25.598 s | 2.379 / 2.539 s |
| `adaptive-repair-candidate-2-20260910` | 30 | 30.357 s | 3.524 / 2.901 s |
| `adaptive-repair-old-2-20260910` | 30 | 46.509 s | 4.407 / 4.934 s |
| `adaptive-repair-candidate-3nodes-20260910` | 3 | 2.237 s | 0.484 / 0.503 s |
| `adaptive-repair-candidate-10nodes-20260910` | 10 | 10.413 s | 2.428 / 2.037 s |

Thirty-worker average setup fell from 45.741 to 27.977 seconds (38.8%). These
are two random native formations per version, not a confidence interval,
worst-case bound or accepted full-curve result. The native control is the frozen
feature-omitted binary from the failed curve; the candidate is also omitted.

SHA-256 identities:

- Old omitted worker: `6ace0838c69c00e7b32872093a19555cd5df9479c7f67aa539ce0dc90fb6d801`.
- Candidate omitted worker: `754a4377cfd9f4fb15587f25d30d25bd0c9e8f68f5bbfc3a57adb33297f678b7`.
- Candidate combined worker: `957bf25e6162b7fe85176dd2cdd9334870a97e346e26fa7fe82f01d0f79922a9`;
  built successfully, not used for the six timing cases.
- CLI: `ca2ea5d6837b0c8dc3a6149c47eccacb506a02ebb60f1d34f142a542d53251ff`.
- Probe: `6ea2f8e96e6cb045afb6d58f85280c94e651a17e3a5d9a7f1eb8c1f1deab3e01`.

## Validation, accounting and remaining work

### Idle-cost control

A separately predeclared three-case sequence ran old-30, candidate-30 and
candidate-3, each with 30 seconds of no client traffic after exact formation.
It used high-resolution process CPU clocks, start/end RSS, the original setup
deadline and exact post-window identity/liveness/readiness checks. All passed
with clean shutdown and unchanged source/artifacts. No case was retried.
The sequence was capped at 330 seconds and completed in 170.447 seconds.

| Case | Setup | Aggregate CPU / 30 s | Equivalent share of one core | Aggregate RSS, before → after |
| --- | --- | --- | --- | --- |
| Old, 30 workers | 46.227 s | 7.917 s | 26.4% | 460,852 → 466,124 KiB |
| Candidate, 30 workers | 27.100 s | 8.108 s | 27.0% | 460,504 → 467,084 KiB |
| Candidate, 3 workers | 3.119 s | 0.203 s | 0.68% | 38,896 → 38,928 KiB |

The thirty-worker candidate uses 2.4% more CPU in this single comparison.
This finds no large persistent idle-cost increase, but does not establish a
small-overhead bound or bandwidth result. Do not confuse aggregate CPU with
per-worker CPU, or this version comparison with telemetry overhead. Three idle
workers remain inexpensive, consistent with the preceding diagnostic.

Artifacts: `/tmp/orishu-curve-check.so7Ibv/adaptive-repair-idle-20260910/`.
The private, explicitly diagnostic runner is `/tmp/orishu-adaptive-repair-idle.py`,
SHA-256 `8667e2551bd20e5a4c15f88e3bd68e7c2175b3045f9eae90c2e8ca23f9c786c5`.
It retains each case separately and stops on failure; it is not an operator
entry point or an added acceptance mode.

### Regression checkpoint

- Regression red/green confirmed; final optional 27-case replay completed in
  25.11 seconds, with all three stages present in every case.
- Combined-feature worker library: 226 passed, none failed, three intentionally
  ignored (manual allocation, replay diagnostic and subprocess helper), 27.14 s.
- Worker combined-feature library/tests/examples Clippy with `-D warnings`
  passed. Combined and omitted release workers built successfully.
- Native six-case sequence: 161.462 seconds including bookkeeping.
- Omitted release worker: encrypted-UDP partition/heal with competing policy
  updates and full handoff/leave/crash/restart/readmission journey passed.
- Combined release worker with observability enabled: full formation churn,
  per-worker admission/activity/reliable/datagram/outbound/deadline/catch-up
  scrapes, locked/peer-loss health, crash expiry, leave retention and restart
  counter reset passed. This is not the separate official-Collector walkthrough.
- All 82 formation Python harness tests passed. An initial sandboxed run had
  eleven socket-permission errors; the scoped local-socket rerun passed without
  test or runtime changes.
- Formatting, whitespace and documentation checks passed (136 Markdown files).

The two native lifecycle commands used the worker hashes above and the frozen
CLI, without rebuilding or substituting artifacts:

```sh
python3 scripts/check-formation-cli.py --worker target/formation-telemetry-omitted/release/orishu-worker --ctl /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/ctl --policy-partition
python3 scripts/check-formation-cli.py --worker target/formation-telemetry-enabled/release/orishu-worker --ctl /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/ctl --observability
```

### Remaining acceptance

Conservative performance/diagnostic debit: preceding 3,030.007 seconds plus
12.06 and 25.40 seconds for development replays, 25.11 seconds for the retained
replay, 161.462 seconds native verification and 170.447 seconds idle control =
**3,424.486 seconds**, about 57m04s of the approved 90 minutes, excluding builds.
About 32m56s remains.
Ordinary correctness regressions are recorded separately, not timed-load cells.
No acceptance batch was resumed, overwritten or silently replaced.

Next: complete the affected fault/feature matrix and final build applicability
review beyond the two lifecycle journeys above; isolate default-sampling CPU
cost and baseline-p95 variation;
retain full-sampling loss/cost as troubleshooting evidence. The original curve
still has 83 completed cells, one failure and 24 unexecuted cells. Faster setup
does not waive overhead goals or prove the original unlock failure impossible.
No user action is required for these authorized diagnostic/regression steps.
A new full acceptance batch, threshold exception or host-policy change needs
separate review; this report grants none of them.
