# Five-Pi client-error diagnostic — 2026-09-11

Status: **approved single diagnostic completed; earlier failures did not
reproduce; original error cause unresolved; cleanup and ARP rollback verified**.

This follows the [post-M4 five-Pi campaign](formation-post-m4-five-pi-plan.md),
whose corrected twenty-worker run stopped on 57 unclassified client errors.
The operator approved up to ten minutes from the remaining allowance, with
the same temporary ARP/rollback safeguards. Only one 32/64-client pair ran;
there was no retry, executor sweep or persistent host-policy change.

## Frozen scope and instrumentation

- Same five physical hosts and common worker/CLI artifacts as the preceding
  campaign: two Ubuntu Pi 5s, two Ubuntu Pi 4s and one Debian Pi 4, all 8 GB.
  Four workers per host, twenty membership nodes, default four-thread Tokio
  pools. Host roles 0–4 preserve the preceding report's aliases.
- Worker SHA-256 remains
  `da9d189b5ff0a56acdfeaeb218686978ac69f01dcce8dccd59bf4d98c7a285c1`.
  Only the coordinator probe/harness changed; no ARM64 rebuild or worker fix.
- New optimized coordinator probe SHA-256:
  `575d730de0baeff33cdefbc0ca676b7c235a406efba3bd57d667a498adcba08b`.
  It retains one-second requests, authenticated certificate-verified HTTPS,
  ten-second windows, identity validation and stop/accounting rules.
- Network input/output schema **4** adds finite client-error categories/status
  counts and first-error timing. Network schemas 2/3 remain supported with their
  old shapes; current Unix CLI schema 2 is unchanged. The probe additionally
  accepts Unix schema 3 for the same diagnostic shape.
- No error strings, URLs, response bodies or credentials are emitted. Known
  client-owned formatting distinguishes empty HTTP responses and response-body
  read/decode failures. Other transport causes remain grouped: classification
  cannot reconstruct typed causes already discarded by the shared client.
- `--error-diagnostic` selects one unpaced 32/64-client pair. It is exclusive
  with short comparison and retains every safety/timing/cleanup gate.
- New root-owned rollback helpers saved the original ARP settings and armed
  independent fifteen-minute timers before applying the approved per-device
  correction. That safety lease did not extend the ten-minute experiment budget.
  The two-packet UDP check again delivered both packets on Ethernet.

## Observed result

Twenty-worker formation completed at **34.359 seconds** after capacity setup
began. Both windows passed the unchanged conditional timing gate and retained
the expected membership/identity state.

| Unpaced clients/worker | Aggregate requests/sec | Worst-worker p95 | Client errors | Conditional joint skew bound |
| ---: | ---: | ---: | ---: | ---: |
| 32 | 101,792.7 | 25.326 ms | 0 | 59.172 ms |
| 64 | 100,113.7 | 46.059 ms | 0 | 59.151 ms |

All forty worker-window error detail objects have empty counts and null first
error, with zero `transport_errors`, `invalid_responses` and sample-cap hits.
Thus **the earlier 57 errors did not reproduce**. They remain recorded failures,
not reclassified as harmless or repaired by this diagnostic. Increasing
concurrency did not increase aggregate throughput and substantially increased
the worst-worker percentile. Two short windows do not establish sustainable
capacity or a default executor policy.

These are unpaced cluster-summary API requests, not peer-message or simulation
throughput. An aggregate above 100,000/sec does **not** satisfy the fixed
5,000/sec-per-worker target: no fixed-rate baseline ran here, and the Pi 4s
remain far below 20,000/sec per four-worker host.

| Host role | Requests/sec, 32 clients | Requests/sec, 64 clients | Worker CPU share of four-core host, 32 / 64 |
| --- | ---: | ---: | ---: |
| 0 — Pi 5 / Ubuntu | 34,876.5 | 33,045.8 | 98.16% / 97.35% |
| 1 — Pi 5 / Ubuntu | 36,417.4 | 36,139.7 | 98.10% / 97.36% |
| 2 — Pi 4 / Ubuntu | 9,493.2 | 9,727.6 | 97.05% / 96.77% |
| 3 — Pi 4 / Ubuntu | 9,720.9 | 9,689.8 | 97.20% / 97.06% |
| 4 — Pi 4 / Debian | 11,284.7 | 11,510.8 | 97.07% / 97.50% |

## Why the earlier run is not explained yet

The diagnose workflow exposed an important difference in the retained
coordinator receipts, not a demonstrated root cause:

| Coordinator observation | Earlier twenty-worker unpaced windows | New diagnostic |
| --- | ---: | ---: |
| Aggregate generator CPU cores | 8.54 / 8.66 | 5.09 / 5.90 |
| Host busy CPU fraction | 68.89% / 75.06% | 29.52% / 32.87% |
| Summed generator thread scheduler wait | 8.95 / 8.89 s | 0.81 / 0.98 s |
| Highest sampled CPU temperature | 69°C / 69°C | 88°C / 92°C |

Host busy fraction is derived from the first/last host CPU-counter snapshots,
excluding idle and I/O-wait. It covers **all host work**, not just the generators;
its bracket differs from each process's CPU bracket. Scheduler wait is summed
across generator threads over their explicit pre-start-lead-through-post-window
brackets, not a single thread's wait or exactly ten seconds. These are diagnostic
comparisons, not a paired observability-overhead measurement.

Previously the Pi 5 workers consumed roughly half their hosts; here they were
near full utilization. The changed generator/host pressure is a plausible lead
for throughput variation and failure reproduction. It does not identify the
competing processes, CPU-frequency policy, packet fault or original error kind.
The newly built probe is another changed variable. Do not attribute the gain to
error instrumentation, claim a worker optimization, or conclude thermal
throttling from these temperatures alone. No CPU-policy changes were made.

Highest sampled Pi temperature was **52.095°C**, with zero sampled process swap
and zero pre/post firmware flags. One 100 ms process sample was missed on the
Debian Pi in the 32-client window and one on the first Ubuntu Pi 4 in the
64-client window. Sampled Ethernet brackets retained 4–5 RX drops per Ubuntu
host and zero on Debian, despite zero client errors. These counters do not
identify failed application traffic. Consequently the run remains a diagnostic
with findings, not a paired acceptance result.

## Validation, cleanup and follow-up

Validation passed **164 Python tests**, **seven focused Rust capacity tests**,
targeted probe Clippy with warnings denied, and formatting. A test exercises
the real HTTP client against an empty HTTP 503 response; others verify finite
classification, secret omission, earliest-error selection, exact count totals,
malformed fields/status/timing rejection, legacy profiles and no automatic retry.
An initial Clippy range-pattern warning was corrected before the frozen build.

Capacity elapsed time was **88.802 seconds**. The whole diagnostic, including
ARP setup, UDP check, restoration and independent cleanup, took **101.700
seconds**, below the approved 600 seconds. The cumulative conservative charge
is now **1,105.128 seconds (18.4 minutes)**, leaving about **41.6 minutes** of the
original allowance. Unused time does not authorize additional automatic runs.

All workers exited cleanly without forced termination. Independent inspection
found no owned workers/generators, lab listeners, API sockets or copied
coordinator credentials. Every original ARP setting was restored and all five
new timers were stopped and verified inactive. No route, neighbour-cache,
interface or persistent network change was made. Private evidence is retained
at `/tmp/orishu-x5-errors.k6QsW2`, including the manifest, complete per-cell
receipts/error objects, helper receipts, `finished.json` and `cleanup-audit.json`.

A mode-0600, Git-ignored archive is retained at
`target/pi-lab-evidence/x5-client-errors-k6QsW2.tar.gz`; gzip integrity and the
frozen source hashes were verified. Its private evidence index covers 72 files.
Archive SHA-256:
`35c39a64a2ddaa7916ff0983a67091b530ef599938e9337f3f8c26100fd9fbcb`.
Do not publish the archive: raw lab addressing remains private. This report and
the linked documentation are sanitized; documentation/link and diff checks pass.

The [physical experiment](../tasks/implement-physical-formation-experiment.md)
now has error-classification tooling, but the earlier error's root cause remains
open. A next **separately selected** diagnostic should control and record
coordinator contention/CPU conditions and probe-build identity before trying to
reproduce/classify the failure. The [executor review](../tasks/review-worker-executor-performance.md)
remains pending; no one/two/four-thread comparison ran. No further lab activity
or operator action is required to complete this approved single diagnostic.
