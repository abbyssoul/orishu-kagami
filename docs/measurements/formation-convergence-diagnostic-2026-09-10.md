# Formation convergence diagnostic — 2026-09-10

Status: **slow convergence reproduced with persistent clients; peer-establishment
and failure-detector interaction narrowed, runtime cause not yet proven**.
Owner: [formation M4 checklist](../tasks/cluster-formation-m4-checklist.md).
Preceding evidence: [failed fixed-rate curve](formation-fixed-rate-2026-09-10.md).

Follow-up: [adaptive repair](formation-adaptive-repair-2026-09-10.md) now
reproduces the sparse-chain delay, implements a bounded scheduling correction,
and records six native checks. This report preserves the preceding diagnostic
checkpoint; its then-unproven hypothesis is not the current implementation status.

## Problem and scope

Thirty-worker setup consumed 41–59 seconds in completed acceptance cells and
then failed unlock under the original shared 60-second deadline. Earlier
diagnostics repeatedly spent about 36 seconds observing exact membership, but
issued roughly 1,100 separate CLI processes during that stage. It was unclear
whether observation overhead, membership propagation, or peer establishment
dominated. This diagnostic isolates the observer first, then uses existing
peer metrics. It does not rerun or replace any acceptance cell.

The original source-built worker binaries are unchanged. No worker timers,
thread counts, allocator, peer grammar, authority, handshake security or
acceptance gates change. The same exact membership, fingerprint, liveness,
formation, readiness and policy predicates remain required.

## Method and retained identities

`scripts/diagnose-formation-convergence.py` runs CLI/persistent/persistent/CLI
in a fixed order, with **one verification sweep per second in both variants**.
This controlled cadence is diagnostic, not a silently changed acceptance
profile. Startup and admission still use the original CLI. Only post-join
membership/summary reads use the selected observer; lock/unlock submission
still uses the CLI. Every case keeps the shared original 60-second setup gate,
with an 80-second outer diagnostic deadline and cleanup inside 90 seconds.
The four-case series is capped at 360 seconds. Cases are not repeated until
success; a failed case remains failed. Unsafe cleanup/checkpoint failures stop
the series. No timed load runs.

The release-only probe's `observe` subcommand holds one public typed HTTP client
per worker. It accepts only `summary`/`members` reads, at most 4,096 commands,
with 128-byte command and 1 MiB reply bounds, two-second request deadlines,
and two executor threads. The owning fixture enforces its process lifetime
and closes stdin before cleanup. Python reads replies in bounded chunks rather
than making a syscall per output byte. Typed client page/identity validation
and the curve's final exact predicates are preserved.

Frozen worker/CLI/collector-probe paths come from
`/tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/`.
The new observer executable SHA-256 is
`29f3bb3da353dca0e0556f8e64de5f9cc50eae5b3818f3fcc30c9e7cfbcb1945`.
Each new directory retains a source/artifact manifest, bounded per-case JSON,
and elapsed diagnostic accounting. All cases verify clean cleanup and
unchanged source/artifacts during their respective runs. No acceptance
artifact was modified.

## Observer comparison

Directory: `/tmp/orishu-curve-check.so7Ibv/convergence-observer-20260910`.
Elapsed **165.457 seconds**, all four predeclared cases retained.

| Case | Observer | Outcome | Total elapsed | Exact-members stage | Mean list-read wall time |
| --- | --- | --- | --- | --- | --- |
| 0 | CLI | Complete | 46.796 s | 31.256 s | 29.986 ms |
| 1 | Persistent | Diagnostic adapter failure after exact-members | 41.020 s | 35.158 s | 4.551 ms |
| 2 | Persistent | Diagnostic adapter failure after exact-members | 30.653 s | 25.170 s | 4.272 ms |
| 3 | CLI | Complete | 46.623 s | 32.112 s | 29.690 ms |

Both persistent failures were `KeyError` at the first summary read. The typed
protocol serializes `memberCount`/`aliveCount`/`membershipLocked`; CLI JSON
projects them as `nodes`/`alive`/`locked`. The observer initially omitted that
presentation mapping. Membership lists use the same field names, so their
completed stage timings remain partial evidence, **not completed policy gates**.

A regression through the actual Python observer adapter first reproduced
`KeyError: locked`, then passed after the three explicit renames. Identity,
participation and readiness fields remain unchanged. A separately named
development verification, not a resumed/replacement series, is retained in
`/tmp/orishu-curve-check.so7Ibv/convergence-observer-mapping-check-20260910`.
It completed the full persistent-client formation/policy path in **55.841
seconds** (55.917 seconds including diagnostic bookkeeping). Exact-members
took **41.166 seconds**, with a mean list read of **4.519 ms**; lock/unlock
took 4.048/5.037 seconds.

Persistent reads are much cheaper, but membership convergence remains slow.
The two complete CLI cases also show incomplete membership across many
successive sweeps, not merely one late final read. In case 0, the minimum local
member count progressed from 5 near second 5 to 18 near second 15 and 29 near
second 30. All lists reached 30 members around second 34, and all-alive was
observed around second 36. Temporary non-alive states occurred in 29 of its
31 membership sweeps while every child process remained running. Persistent
cases show the same phenomenon. CLI startup alone therefore does not explain
the tens-of-seconds delay. These few random formations do not estimate a
population failure rate or prove a performance improvement.

## Existing peer metrics

Directory: `/tmp/orishu-curve-check.so7Ibv/convergence-peer-metrics-20260910`.
One separately labeled persistent-client diagnostic used the frozen combined
worker with metrics enabled, tracing/logging disabled. After each sweep, it
read 24 existing, fixed-name metrics on roles 0/15/29. Retention is bounded;
missing instruments are errors, not zeros. No new worker instrumentation was
added. This is an instrumented observation, not an overhead comparison with
the omitted build.

The case completed in **46.037 seconds** (46.129 including bookkeeping).
Exact-members took 30.161 seconds; all policy checks stayed inside the original
deadline. Selected cumulative readings at its final observation:

| Metric | Role 0 | Role 15 | Role 29 |
| --- | --- | --- | --- |
| Initial inbound peer exchanges failed | 126 | 4 | 0 |
| Outbound attempts failed | 0 | 40 | 64 |
| Owner send failures | 7 | 42 | 93 |
| Direct/relay probe deadlines consumed | 7 | 14 | 28 |
| Indirect probe deadlines consumed | 0 | 8 | 15 |
| Retained registry slots | 29 | 29 | 29 |

Role 29 retained only one registry slot near second 6, five near second 20,
15 near second 30 and 29 near second 40. Its observed datagram receive counter
was still zero near second 10. Role 0 accumulated handshake failures early;
these and the sampled send/probe-failure counters stopped increasing as
connectivity developed. The sampled outbound TLS-failure, outbound-attempt
timeout, local datagram-submit refusal/failure and anti-entropy-deadline
counters stayed zero. These are three local projections, not fleet-wide
absence-of-failure or delivery proofs. A registry slot is retained binding
budget, **not** an exact live-connection count; a completed handshake metric
is not domain admission.

Source inspection provides a concrete mechanism to investigate:

- `peer/handshake.rs::accept_request` rejects a claimed member identity not
  yet held by the receiving membership model. Binding also checks authenticated
  identity. This is a security requirement, not a check to bypass for speed.
- Admission queues the new member record for gossip at the introducer.
  Other nodes must learn it through trusted gossip/reconciliation before they
  can accept that member's connection.
- `driver.rs` selects SWIM peers among known alive/suspected members even
  without an established route; anti-entropy selects existing reliable routes.
  The owner already restores gossip allowance when no route exists, so an
  attempted send is not mistaken for transmission.
- `runtime.rs` establishes peer routes through its bounded maintenance loop;
  canonical dial direction also depends on assigned node-ID ordering.

The observations are consistent with a bootstrap feedback loop: incomplete
member knowledge prevents some connections, incomplete connectivity delays
gossip, and probing known-but-not-yet-reachable peers adds transient suspicion
and refutation work. **This is the leading hypothesis, not yet the verified
cause of cell 83's unlock timeout.** Current aggregate metrics do not identify
the reason for each failed handshake or show each member's first-arrival path.
No local network-saturation or allocation root-cause claim follows from them.

## Validation, budget and next action

A deterministic replay in `peer/session.rs` now exercises the application
binding dependency through the real serialized handshake and gossip paths:
A knows B but not C; C's handshake fails without inserting a member; a bound
B session carries C's member record through wire decoding and the production
membership core; the same C handshake then succeeds. Removing C invalidates
the binding and rejects its handshake again. Authenticated TLS facts are
fixture inputs, not a real network handshake. This passing test confirms the
asymmetric-knowledge dependency and security fences, **not** the thirty-worker
timing cause or a runtime fix.

Eight Rust probe tests (including real HTTP), worker library/test/probe Clippy
with warnings denied, four convergence-diagnostic Python contracts, the existing
35 curve contracts, the serialized binding replay, formatting and whitespace
checks pass. Initial compile errors in the diagnostic client adapter were
corrected before execution; the separate runtime mapping failures above remain
retained. These checks do not replace the broader worker regression checkpoint.
The binding test is a post-measurement, test-only source addition;
production worker logic and the frozen binaries are unchanged. Documentation
checks pass across 135 Markdown files.

This continuation used **267.503 seconds** of timed diagnostic execution.
Adding it to the preceding conservative 2,762.504-second accounting uses
**3,030.007 seconds** (50 minutes 30 seconds) of the approved 90-minute allowance
excluding builds. Remaining time is not permission to repeat acceptance or
relax its gates.

Next: extend the deterministic asymmetric-knowledge replay into a bounded
chain-formation model that exposes delayed member dissemination and route
establishment. Test the leading hypothesis by changing one dissemination action in that reproducer,
not by changing timeouts or accepting unknown members. A runtime fix needs a
demonstrated cause and a regression over the real owner/wire path before a
new performance checkpoint can be considered.

No user decision is currently required for this diagnosis. The original
unlock failure, baseline-p95 variation, default-sampling CPU excess and final
M4 acceptance remain open. Any proposed new protocol/security contract,
acceptance relaxation, temporary exception, host-policy change or new full
curve must be presented separately with consequences and trade-offs.
