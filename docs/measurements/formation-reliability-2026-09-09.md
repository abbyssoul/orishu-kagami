# Formation/catch-up reliability investigation — 2026-09-09

Status: **demonstrated defects corrected; shared setup convergence policy approved
and six bounded formation checks passed; full overhead/M4 acceptance pending**.
Owner: [formation task](../tasks/implement-cluster-formation-poc.md).
Predecessor: [failed quiet-host curve](formation-telemetry-2026-09-09.md).

## Outcome

Real thirty-worker admission and catch-up now complete in about **five seconds**,
compared with roughly **55 seconds** in an intermediate diagnostic before
event-triggered first preparation. A separate observation found exact thirty-node
membership, matching certificate bindings and all nodes alive at **34.352 seconds**;
the final batched-accounting build repeated late convergence at **37.868 seconds**.
This is diagnostic evidence, not an accepted performance result or convergence SLO.

The investigation's original harness failed its **ten-second per-observation gate** on
some ten-worker and thirty-worker formations, despite the larger **60-second
whole-setup allowance**. Do not describe this investigation as a passing scaling
prerequisite on the strength of those earlier runs. The operator subsequently
approved using the remaining whole-setup budget for convergence. The
[accepted policy](formation-telemetry-plan.md#shared-setup-convergence-policy--2026-09-09)
applies only to new runs and preserves all identity/liveness/policy checks,
earlier failures and worker protocol limits.

The original full curve, all failed diagnostics and the original three-worker
results remain retained. No full curve or timed-load measurement was rerun here.
Baseline noise, shutdown-log disposition and high full-sampling cost remain
separate open findings.

The subsequent [revised-policy verification](#approved-policy-verification--2026-09-09)
passed complete setup at 3, 10 and 30 workers, both feature-omitted and with
metrics enabled. This closes the scoped setup-check prerequisite, not the full
performance curve, a repeatability/SLO qualification or combined M4 acceptance.

## Demonstrated causes and bounded corrections

1. **Prepared gossip was retired without a wire opportunity.** The core charged
   all offered deltas, but the encoder trimmed trailing deltas to the 1,200-byte
   datagram cap. A real owner/registered-connection regression delivered only
   **two of ten** queued records when trimming feedback was withheld. With the
   fix, all ten receive a real datagram opportunity and the queue still retires.
   Missing member routes likewise no longer consume gossip allowance. The
   `GossipDeferred` local effect outcome restores only the omitted attempt and
   only for an exact current authoritative value, under existing queue limits.
   Batched feedback subtracts one charge per omission without undoing another
   send from the same transition that actually reached submission.
   It cannot merge foreign, unknown, stale or removed records. A record too large
   even alone remains available through reliable anti-entropy without permanently
   occupying highest gossip priority. This is not a peer-delivery guarantee.
2. **Reconciliation could occupy a deadline without a route.** Anti-entropy
   selection now considers current registered member routes. SWIM still probes
   disconnected members: this does not hide unreachable peers or weaken failure
   detection. A route lost after selection retains normal timeout behavior.
   Tests distinguish unstarted reconciliation from a real routed round expiring.
3. **The first catch-up waited behind connection scanning and periodic ticks.**
   Adoption retains only the original introducer's node ID and certificate pin
   for one prioritized, freshly authenticated member dial. The hint is checked
   against current membership and consumed once without advancing the normal
   scan cursor. Adoption wakes that first scan; registration of the first current
   admitted route wakes initial catch-up. Coalesced IO notifications confer no
   authority. Failed attempts retain periodic scheduling, one active transfer,
   three prepared attempts and the original 90-second adoption limit.
4. **Connection maintenance underused its existing dial capacity.** It prepared
   one candidate per second despite four shared handshake slots. It now prepares
   at most four candidates per maintenance turn, sharing the existing one-second
   owner-query deadline, active-target exclusion and 64-task cap. No connection,
   exchange, queue or retry budget was increased.
5. **The verifier treated a recoverable phase as terminal.** Public
   `catchUpFailed` can precede an automatic worker retry on the same operation
   and assigned identity. Both the curve and Collector preflight now continue
   observing that operation within their original deadlines. They never submit
   another admission. Identity changes, unresolved admission and pre-admission
   failure remain failures. A permanently failed operation still times out.
6. **The fixture's listing buffer did not accommodate thirty records.** Only
   the operator `ls` capture now permits 64 KiB; other replies retain 16 KiB.
   Both caps are explicit and tested, and the measurement profile records them.
   Failure records now retain the stage, join phases and bounded secret-free
   membership/summary evidence instead of only a generic observation timeout.

These are IO scheduling/bookkeeping and verifier corrections under
[ADR 0013](../adr/0013-cluster-formation-and-node-identity.md), not a new membership
authority, persisted format, peer profile or protocol negotiation. The local
Rust effect enum gains a variant; there is no wire schema change, dependency
addition or supported-release migration. See the reconciled
[peer protocol](../protocol-p2p.md), [client protocol](../protocol-client.md),
[worker manual](../../apps/orishu-worker/README.md) and
[operator stories](../user-stories/orishu/cluster-admin.md).

## Initial corrected diagnostic checkpoint

Every row in the following historical table used the ten-second observation and 60-second setup
gates. These are independent, predeclared diagnostic invocations, not retries
substituted for failed measurement cells. All fixture children were reaped.

| Fixture | Gate outcome | Evidence |
| --- | --- | --- |
| 10 workers, features omitted, A | Passed full setup in 13.496 s | Exact membership, summary, lock and unlock |
| 10 workers, features omitted, B | Failed exact-members gate at 11.894 s | All joins/catch-up completed; no timed load |
| 10 workers, features omitted, C | Passed full setup in 15.345 s | Exact membership, summary, lock and unlock |
| 30 workers, metrics enabled, A | Failed exact-members gate at 15.428 s | Last join completed at 5.271 s |
| 30 workers, metrics enabled, observation | Failed exact-members gate at 16.023 s | Same formation observed exact/all-alive at 34.352 s; no late acceptance or lock/unlock claim |

Worker executable SHA-256:

- Both capabilities: `96896956e5722fc29f9ee8858f689cfbddfd3bb69a58dd528ede27d23a5a5706`.
- Features omitted: `bf151ad091658b5839ec959617fa40fd5903def9eb042f5dbb6fc7bcc7594167`.

Raw artifacts are under `/tmp/orishu-curve-check.so7Ibv/`:
`formation-fixed-event-10-{a,b,c}.json`, `formation-fixed-event-30-a.json` and
`formation-fixed-event-30-observe.json`. Each identifies its actual executable
hashes. The frozen original CLI/probe binaries were reused, not overwritten.
Earlier `formation-diagnostic-*`, `formation-fixed-10-*`,
`formation-fixed-30-*`, `formation-fixed-final-*`, `formation-fixed-batched-*`
and `formation-fixed-owner-*` files retain intermediate outcomes. One intermediate
priority-run diagnostic hit the listing cap and then failed while collecting
evidence; it produced no JSON report and is not counted as a passing run.

An intermediate integration error placed gossip feedback on session replies,
not member sends. Helper-only tests missed it; the owner branch was corrected
and the real registered-datagram regression now fails two-of-ten with feedback
withheld and passes ten-of-ten with it restored. No intermediate passing result
is presented as proof of the final checkpoint. Temporary debug logging and
the one-gossip-per-message experiment were removed; diagnostic scripts remain
explicitly labelled under `/tmp`, not production configuration.

The temporary artifacts may disappear on reboot. For a repository-owned
formation-only reproduction, after building the release artifacts, use a fresh
output directory:

```sh
python3 scripts/check-formation-scaling.py --workers 30 --mode metrics --worker target/formation-telemetry-enabled/release/orishu-worker --ctl target/formation-telemetry-enabled/release/orishuctl --probe target/formation-telemetry-enabled/release/examples/formation-telemetry-probe --output /tmp/orishu-formation-new-check
```

This uses the curve's actual formation routine and gates, returns nonzero on
failure, records source/artifact identities and cleanup, and never runs timed
load or the full curve. It does not overwrite an existing result directory.

## Final batched-accounting checkpoint

Final review added a red/green regression for several sends emitted before their
local feedback returns. Omitting the first and third sends must leave the middle
successful send charged, even if the third selection retired the queue entry.
The feedback now subtracts one charge from the current/retired count instead of
rewinding to an earlier selection's absolute hop count.

After rebuilding both variants, `formation-fixed-batch-accounting-30-observe.json`
retains one thirty-worker metrics diagnostic: final catch-up at **5.323 seconds**,
exact-members gate failure at **15.471 seconds**, and the same formation observed
exact/all-alive at **37.868 seconds**. Cleanup was clean. As before, this records
no late gate acceptance and no lock/unlock or timed-load claim.

Final executable SHA-256:

- Both capabilities: `a58d116921edccdfff3e469cb0d68af3bce2d201b90a324873484aa225d4332f`.
- Features omitted: `d09606dda5f2bd3658680b97860923cdbbcdde6f228db708ec117f23c42d4ed3`.

One predeclared final ten-worker, feature-omitted check using the repository
verifier passed complete setup in **16.634 seconds**, including exact membership,
summary and cross-worker lock/unlock visibility. Its new
`formation-final-repository-10/result.json` records clean cleanup and unchanged
binary identities. It does not replace the earlier failed ten-worker diagnostic
or establish repeatable short-gate passage by itself.

The repository-owned three-worker verifier passed with the earlier corrected
checkpoint and retained source/artifact identities and clean cleanup in
`formation-repository-smoke/result.json`. The official Collector 0.160.0
three-worker journey also passed there with **125 matched spans/log records**,
both admission chains, 172 series per worker, probes and policy checks. These
are functional regressions, not an overhead curve or final M4 freeze.

## Approved policy verification — 2026-09-09

The operator explicitly approved letting convergence use the remaining original
60-second setup budget. The runner now applies
`remaining_whole_setup_v1` to exact membership, formation-summary and lock/unlock
visibility at every worker count. Startup and join-status observations retain
their ten-second limits, capped by the same setup deadline. All identities,
certificate bindings, counts, liveness, readiness and policy checks remain
mandatory. No worker code, protocol timer, build, load-window duration, retry
limit or cell/per-size/batch budget changed for this policy verification.

Six cases were declared in advance and executed once each, sequentially, with
the final executable hashes above and the same frozen CLI/probe. The current
repository-owned verifier returned zero for every case. All artifacts identify
the new policy, retain source/artifact identities and report unchanged binaries
and clean cleanup. No failed case was replaced or reclassified.

| Workers | Features omitted: full setup | Metrics enabled: full setup |
| --- | ---: | ---: |
| 3 | 2.745 s | 3.168 s |
| 10 | 15.375 s | 15.502 s |
| 30 | 46.615 s | 51.287 s |

These durations include complete catch-up, exact membership and summaries, and
cross-worker lock/unlock visibility. They are setup observations, not paired
instrumentation overhead or a scalability performance curve. Raw evidence:
`/tmp/orishu-curve-check.so7Ibv/policy-v1-{3,10,30}-{omitted,metrics}/result.json`.
The command follows the repository-owned reproduction above with each count,
mode, corresponding final worker binary and a fresh output directory.

The policy tests cover all three sizes, sharing the original absolute deadline
across stages, refusing successful replies at/after expiry, retaining ordinary
ten-second limits and refusing convergence without a setup deadline. The Python
curve suite now has **28 passing tests**; the companion formation-receipt and
Collector validator suites retain **8 and 9 passes**. `make docs-check` passes.
Rust was not changed or rebuilt for this policy-only increment; the prior
affected-crate validation remains the recorded code evidence.

Before another full curve, retain the existing noise/shutdown findings and
review per-size budget feasibility. If the observed thirty-worker setup times
repeat, 36 cells plus their ten-second request windows alone imply approximately
**34–37 minutes**, before other overhead, versus the unchanged 30-minute per-size
limit. This is an estimate, not an executed matrix. Any budget revision needs
explicit approval; no longer run, automatic retry or timing acceptance is
inferred from approving the setup-convergence policy.

## Regression validation

The later [post-fix regression checkpoint](../tasks/cluster-formation-m4-checklist.md#post-reliability-regression-checkpoint--2026-09-09)
records workspace/release and process revalidation after the separate shutdown
fix. Counts and scope below describe this report's earlier formation-fix
checkpoint, not the latest combined M4 disposition.

Affected membership/worker all-target suites pass with features omitted and
with both observability/tracing capabilities. The resolved dependency tests
continue enforcing the sans-IO core boundary. New regressions cover locally
unsent gossip, authoritative/bounded feedback, real wire trimming, available
reconciliation routes, first-introducer priority, first-attempt wake-ups and use
of four existing dial slots. Existing generation, ejection, leave, source-loss,
retry-exhaustion and admission-deadline tests remain applicable and passing.

At the final code checkpoint: membership has **208 passing tests** plus its
benchmark smoke cases; worker all-target runs have **237 passed / 1 ignored**
with capabilities omitted and **328 passed / 3 ignored** with both enabled.
The ignored cases are a subprocess-only held-stdout fixture and, when compiled,
manual allocation/overhead profiles, not silently skipped formation regressions.
The three Python validator suites have **25, 8 and 9 passing tests** respectively.

The Python curve/formation-receipt/Collector validator suites pass. Manual
allocation and overhead tests are not performance evidence for this fix; their
normal ignored status remains explicit. Final M4/workspace acceptance and the
large process-fault/deployment matrix are not claimed by these scoped checks.

`cargo fmt --all -- --check`, `make docs-check`, affected-crate doc tests and
both-capability all-target clippy with `-D warnings` passed. The global
`git diff --check` still reports pre-existing trailing whitespace in `TODO.md`
and three Kagami task documents; those unrelated edits were preserved.
