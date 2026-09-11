# Post-M4 five-Pi placement and executor experiment

Status: **placed formation/recovery passed; capacity campaign stopped on 57
client errors at twenty workers; cleanup/ARP restoration verified; executor
comparisons unexecuted**.

Subsequent approved follow-up: [error-classification diagnostic](formation-pi-client-errors-2026-09-11.md)
completed one 32/64-client pair without reproducing the failures. The earlier
errors and measurements below remain unchanged; no worker fix or error
resolution is claimed.
Owner: [P-SCALE executor review](../tasks/review-worker-executor-performance.md),
with [physical placement qualification](../tasks/implement-physical-formation-experiment.md).

## Question and limits

First verify formation/recovery on the current five-host fleet using worker-owned
Ethernet placement, without temporary host routes. Then compare default and
one/two/four-thread Tokio multi-thread executors at identical offered load,
separating one worker per physical host from four workers per host. This tests
5- and 20-worker formations, not scientific simulation scaling. A one-thread
multi-thread executor is not a Tokio current-thread runtime.

The operator approved a **fresh 60-minute experimental allowance, excluding
builds**. No previous allowance is carried forward. Preserve each attempt,
including failed initialization and clock planning; no repeat-until-green.
Each capacity invocation has a six-minute ceiling including its 90-second
cleanup reserve. Stop the campaign on request/identity failures, unsafe thermal
or swap conditions, unverified cleanup, or exhausted global allowance.
Clock-invalid comparisons are retained and not presented as performance passes.

## Frozen inputs and sequence

- Fleet: two 8 GB Pi 5s and three 8 GB Pi 4s, four logical CPUs each. Use private
  inventory and stable report aliases; no real addresses or hostnames in reports.
  Readiness identifies Ubuntu 26.04.1 on roles 0–3 and Debian 13 on role 4;
  do not treat the three Pi 4s as an identical OS/kernel cohort.
- Worker/CLI: one native ARM64 build of checkpoint
  `cc9f2de7afbf2e94cdb494b1bf56eabd446707d3`, staged identically in new private
  roots. Existing staged binaries, runs and configuration are untouched.
- Worker has both telemetry features compiled; capacity runs disable metrics,
  tracing and logs. Separate formation smokes cover compiled-off and enabled
  metrics/probes with enforced peer placement.
- Harness/probe: record hashes of the working-tree extension. Network input and
  load-report schema 3 explicitly names total membership; Unix schema 2 does
  likewise. Historical network schema 2 / Unix schema 1 retain their exact
  four-host/16-worker meaning. Local targets are bounded to one or four and
  total topology to three through five hosts.
- Authenticate HTTPS from the wired coordinator using disposable per-worker
  certificates and tokens. Bind peers and TCP clients to the inventory device;
  retain local Unix administration and loopback-only diagnostics.
- Start with placement formation/recovery, then the default 5-/20-worker capacity
  profiles. Subject to retained safety gates and remaining allowance, compare
  1/2/4-thread pools and reverse-order repeats. No default policy is changed.
- Each capacity profile runs ten-second windows: 5,000 offered summary requests
  per worker per second at 32 clients, then unpaced 32 and 64 clients, repeated
  once. A fixed-rate miss is a finding, not permission to hide skipped arrivals.
  Aggregate targets are 25,000/sec and 100,000/sec respectively.

## Evidence and interpretation

Retain exact artifact, role, formation and certificate identities; original
load accounting; worst-worker p95 (never averaged percentiles); worker and
generator CPU separately; thread counts, per-thread scheduler/context-switch
brackets, memory/swap, thermal/throttle evidence, kernel socket/device evidence,
interface counters, clock proposals and shutdown receipts. Scheduler reads are
bounded diagnostic work and their brackets are retained. Host interface bytes
include other traffic; they are not per-socket packet attribution.

Stable RSS does not measure allocation rate. A CPU/scheduler result cannot by
itself identify a hot function or prove that allocations cause the remaining
cost. Allocation/flamegraph profiling and any policy recommendation require their
own evidence; unavailable kernel counters stay unavailable. Current-thread
runtime changes, affinity/governor changes, persistent routes, device-down tests
on the SSH path, production rollout and scientific benchmarks are out of scope.

This round does not reopen accepted M4 measurements or certify multi-interface
failover, container/cloud deployment or the desktop's thirty-worker noise cause.
Publish sanitized results and link outstanding follow-ups after cleanup.

## Preparation checkpoint and first attempt

All five hosts passed read-only infrastructure checks with synchronized clocks,
wired links and no reported firmware warnings. Initial staged worker/CLI hashes
differed across hosts; one worker also lacked placement flags. A clean native
build on the first Pi 5 used Rust 1.97.1 and two build jobs. The corrected build
completed in 2m45s; the initial command used the wrong CLI package name and
stopped before building. New private roots on all five Pis then passed readiness
and identical artifact-hash verification. Original binaries were not replaced.

| Used artifact | SHA-256 |
| --- | --- |
| ARM64 worker, both telemetry features compiled | `da9d189b5ff0a56acdfeaeb218686978ac69f01dcce8dccd59bf4d98c7a285c1` |
| ARM64 CLI | `f7cf45c5d22697ed8ba87c078d4cf7a38c59eca1908eb2696f30f1dd1ed7986e` |
| Updated optimized coordinator probe | `898a80841f0b976c3b45e70fc7f52647f47599926192095d21b53aa9afec5ac6` |

The first placed, compiled-off smoke stopped after **10.018 seconds**, while
initializing the first SSH session, before any `start` RPC or load. The failure
was a broken SSH input pipe. A minimal SSH check subsequently reported no route
to that Pi's inventory address; its hostname no longer resolved. A bounded
five-host check found both access paths working on the other four nodes, but
neither working on the first Pi 5. The coordinator's Ethernet link remained up;
its neighbour entry for the unreachable address was failed. These observations
do not identify whether power, cabling, addressing or another network condition
caused the loss. No worker correctness or performance failure was observed.

No automatic retry or capacity load followed. Restore/confirm that node's
connectivity before a fresh preflight. Metrics smoke, 5-/20-worker capacity,
executor comparisons and physical placement qualification remain **unexecuted**.
The historical M4 acceptance is unchanged. Private readiness, staging, build,
smoke and connectivity receipts are retained under
`/tmp/orishu-post-m4-x5.GoWF8O`; these are not publication artifacts.

Local validation: 157 Python harness tests passed with local socket access;
five focused Rust capacity tests and targeted probe Clippy passed. The first
sandboxed Python run hit socket restrictions; its failed receipts remain
retained, separate from the successful socket-enabled run. Hardware support is
not inferred from local tests. The original example-test command also omitted
required telemetry features and was corrected before tests executed.

## Restored node and placed-admission diagnostic

The operator confirmed an accidentally disconnected cable/power loss and
restored the first Pi 5. Its staged ownership marker was then present but empty;
readiness rejected that directory. A fresh private directory was prepared and
all copied artifact hashes reverified, leaving the interrupted directory intact.

The subsequent compiled-off smoke started five workers. The first two joined,
but admission of the first Pi 4 stalled until the unchanged 120-second deadline.
The setup/recovery receipt records 120.039 seconds; total wrapper elapsed was
120.385 seconds. All five workers exited 0, with no forced termination or
remaining Unix socket. Metrics smoke and capacity were not started.

Read-only inspection found the second Pi 5's Ethernet neighbour entry for the
first Pi 4's Ethernet address mapped to that Pi 4's **Wi-Fi MAC**, not its
Ethernet MAC. Ethernet and Wi-Fi share one IPv4 subnet; inspected ARP reply,
announcement and filter settings were zero, with loose reverse-path filtering.
This exposed a hypothesis test independent of Orishu:

| Two-packet UDP differential check | Result |
| --- | --- |
| Sender pinned to Ethernet, receiver unplaced, destination is the receiver's Ethernet IP | 32-byte nonce received; kernel `IP_PKTINFO` identified **`wlan0`** ingress |
| Same source/destination, receiver pinned to Ethernet | 32-byte send completed; receiver timed out after four seconds |

Both short SSH fixtures exited 0 and closed their sockets. No route, ARP cache,
sysctl, firewall or interface state was changed. This reproduces the relevant
network delivery failure without membership, TLS or a worker executor. It
supports the ARP mismatch as the cause of the placed admission stall, rather
than a compute-capacity problem. It does not claim to eliminate every possible
formation defect or provide successful five-node placement qualification.

Linux's documented defaults permit cross-interface ARP responses; device-bound
sockets cannot correct a peer's link-layer destination. See the
[kernel ARP controls](https://docs.kernel.org/6.18/networking/ip-sysctl.html).
The operator approved a bounded temporary ARP correction with saved settings,
automatic rollback and refresh of only affected lab neighbour entries. Both
interfaces remain active; no persistent network policy change is authorized.

Administrator preflight succeeded non-interactively on four Pis, but the fifth
Pi requires interactive authentication. The coordinator also requires it. No
host network settings or neighbour entries have been changed. A private helper
is staged on the fifth Pi for a one-time operator invocation. It sets only
`eth0`/`wlan0` `arp_ignore=1` and `arp_announce=2`, after saving the original
values and arming a 75-minute transient systemd rollback timer. Later privileged
rollback executes a root-owned copy from `/run`, not the mutable staging file.
It preserves conflicting subsequent administrator changes and reports them
instead of silently overwriting them. No routes, interfaces or persistent
configuration are modified; the helper itself does not clear neighbour entries.
The 75-minute safety lease is not an increase to the 60-minute experiment budget.

Six isolated helper tests passed against fake sysctl files, covering timer-before-
write ordering, restoration/idempotency, inactive timer refusal, external-change
conflicts, duplicate-lease refusal, unknown global policy and failure after writes.
These are local safety tests, not proof that rollback has executed on a Pi.
Helper SHA-256:
`81aa1e62ebe20fd7085f586273a5aecafef42c3e29d73060a740ef887e443792`.
The operator subsequently applied the helper on the fifth Pi and enabled
passwordless sudo there. Its originals, applied values and timer were verified;
the same hash-checked helper was then applied on the other four Pis. All five
original-setting receipts and active rollback timers were verified. The repeated
UDP differential delivered both packets on **Ethernet**, including the
device-bound receiver that previously timed out. No manual neighbour refresh
was required for that check. This is a successful minimal reproduction recheck,
not yet a successful formation or performance result. Placed formation smokes
are running before load. The campaign is bounded by both the remaining
experiment budget and the earliest rollback deadline, with cleanup margin.

Private receipts: `smoke-restored-compiled_off/result.json`,
`placement-neighbour-summary.json` and `udp-placement-diagnostic.json` under the
same evidence directory. At that checkpoint no capacity or executor comparison
had run; subsequent attempts are recorded below.

## Successful correction and initial capacity attempts

The placed compiled-off smoke completed in **63.626 seconds** and the metrics
smoke in **65.300 seconds**. Both verified five-member convergence, lock/unlock
propagation, leave/rejoin with fresh identity and clean unforced shutdown on
every host. The latter also verified metrics and health endpoints. Formation
itself completed well before the full smoke; leave/tombstone convergence accounts
for much of its elapsed time. These are scoped recovery checks, not physical
link-loss/failover or paired telemetry-overhead qualification.

The original campaign then completed six default-executor five-worker windows
in **146.999 seconds**. Both 5,000/worker/sec baselines met the 99% per-worker
delivery gate. The default twenty-worker run formed correctly but stopped in
its third window after **127.457 seconds** on the unchanged 100 ms conditional
alignment gate. Every retained timed response in both runs had zero transport
or identity errors, and cleanup was verified. Its first two windows remain
qualified only under their original conditional timing assumptions; the third
has a **134.299 ms** bound and must not be used as a passing comparison.

The delayed Pi's sampler started **88.256 ms** late. Before-snapshot completion
timestamps span **74.986 ms** of per-thread collection on that host, whereas
its after-window clock-offset envelope was only **2.383 ms** wide. The harness
collected the new scheduler receipts *after* the scheduled start and stamped
the sampler start after that work. This directly exposed diagnostic overhead
at a timing-sensitive boundary; it is not evidence of a membership failure.

Four regression tests exercise the actual node/generator measurement entry
points with delayed scheduler reads. They failed before the correction and
pass after moving those reads into the existing pre-start lead, with refusal if
collection overruns that lead. All 161 Python harness tests passed. No worker,
probe binary, host CPU policy, offered load or timing limit changed. Scheduler
receipts now identify their own **pre-start-lead-through-post-window** scope and
read brackets; they include idle lead time and are not exact ten-second counters.
Normalize by their own elapsed brackets and do not substitute them for the
separately measured worker CPU/resource window.

The pre-fix harness and campaign driver are archived privately; original results
remain untouched. A newly frozen series begins with the twenty-worker default
to recheck the original timing failure, then a fresh five-worker default and
the planned one/two/four-thread comparisons. Its remaining allowance deducts
the first campaign's **274.516 seconds** plus a conservative **600-second**
reserve for preceding preparation/diagnostic experiments; that reserve is not
claimed as measured duration. The earliest ARP rollback deadline independently
bounds the series, with cleanup margin. Any further failing run stops it.

## Capacity outcome and stop condition

All rates below are authenticated HTTPS **cluster-summary requests**, not peer
messages or scientific simulation steps. Windows are ten seconds. The table
retains every attempted window; a rejected window is not repaired retroactively.
Worst p95 is the maximum worker percentile, never an average of percentiles.

| Series / workers | Window | Offered per worker | Clients per worker | Aggregate requests/sec | Worst p95 | Outcome |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Original / 5 | 0 | 5,000 | 32 | 24,927.7 | 1.593 ms | Delivery gate met |
| Original / 5 | 1 | Unpaced | 32 | 62,439.3 | 5.507 ms | No client errors |
| Original / 5 | 2 | Unpaced | 64 | 57,969.5 | 10.791 ms | No client errors |
| Original / 5 | 3 | 5,000 | 32 | 24,930.1 | 1.545 ms | Delivery gate met |
| Original / 5 | 4 | Unpaced | 32 | 63,865.3 | 4.925 ms | No client errors |
| Original / 5 | 5 | Unpaced | 64 | 58,637.7 | 10.849 ms | No client errors |
| Original / 20 | 0 | 5,000 | 32 | 59,488.6 | 25.659 ms | Delivery missed |
| Original / 20 | 1 | Unpaced | 32 | 58,014.0 | 26.609 ms | No client errors; sampler finding |
| Original / 20 | 2 | Unpaced | 64 | Not qualified | Not qualified | 134.299 ms timing bound; stopped |
| Timing fix / 20 | 0 | 5,000 | 32 | 57,347.7 | 27.408 ms | Delivery missed |
| Timing fix / 20 | 1 | Unpaced | 32 | 58,957.4 | 26.441 ms | No client errors |
| Timing fix / 20 | 2 | Unpaced | 64 | 55,092.5 | 54.417 ms | **57 client errors; stopped** |

The five-worker baseline's worst individual delivery was **99.466% / 99.604%**,
above the explicit 99% gate, not perfect delivery. The twenty-worker baselines
offer 100,000/sec total; minimum worker delivery was **44.092% / 43.702%**.
Four workers per physical host therefore do not meet 5,000/worker/sec in this
profile. The original five-worker timing bounds were 60.456–86.777 ms. The
corrected twenty-worker bounds were **58.933 / 61.678 / 63.601 ms**: the original
timing failure no longer reproduced, without relaxing the gate.

The final run stopped after **128.889 seconds**. Errors were confined to worker
roles **13 and 14**, two processes on host role **3** (the second Ubuntu Pi 4):
34 and 23 errors respectively. Each client loop exits on its first error, so
the reduced offered concurrency after failure affects the reported failed
window. All decoded-success identity checks and the post-window twenty-member
check passed; no sample-cap hits or `invalid_responses` were recorded.

Important classification limit: the probe's `transport_errors` field increments
for **any** `ClientError`, then discards the variant. It does not distinguish
connection/TLS/timeout failures from authentication, API-status or unexpected-
response errors. Therefore these 57 failures cannot yet be called proven packet
loss, CPU-induced timeouts or a worker protocol defect. Successful-response p95
also excludes failures and must not be used to dismiss them. A bounded kernel
journal check on the affected host, spanning the window plus margins, returned
no entries; this does not establish an absence of networking/application faults.
No more load ran after this stop. **One/two/four-thread executor comparisons did
not run**, and no executor policy has been selected.

## Resource and placement evidence

At the corrected twenty-worker 32-client unpaced window:

| Host role | Hardware / OS | Four-worker requests/sec | Worker share of four-core host |
| --- | --- | ---: | ---: |
| 0 | Pi 5 / Ubuntu | 15,899.4 | 54.56% |
| 1 | Pi 5 / Ubuntu | 14,691.7 | 49.17% |
| 2 | Pi 4 / Ubuntu | 9,209.2 | 93.21% |
| 3 | Pi 4 / Ubuntu | 9,061.3 | 94.01% |
| 4 | Pi 4 / Debian | 10,095.8 | 90.19% |

Pi 4 worker CPU pressure is clear; it does **not** identify the cause of the
57 errors. The five coordinator generators together used about **8.54 CPU
cores** in that window (8.44–8.66 across the corrected run), and Pi 5 worker
CPU was well below host capacity. The generator/client path and per-worker
serialization remain possible limits. Neither an isolated worker ceiling nor a
causal comparison with the older four-host 94k/sec run is established: fleet,
membership size, binaries and diagnostic instrumentation differ.

The corrected run's highest sampled temperature was **51.608°C**. Worker and
supervisor sampled swap was zero; pre/post firmware flags were zero. The affected
Ubuntu Pi 4 missed one 100 ms process sample in the failing window. Sampled
Ethernet brackets recorded 4–5 RX drops on the Ubuntu hosts and zero on the
Debian host, including successful windows; no RX/TX errors or TX drops were
recorded. These host counters do not attribute drops to failed requests.

Kernel socket receipts showed the selected Ethernet device for each worker
before/after the corrected windows. Over the wider preparation-to-post-window
brackets, per-host Ethernet transmit deltas were about **42–74 MB** while Wi-Fi
transmit was **0–4.9 kB**. Together with the UDP ingress differential and real
formation/HTTPS checks, this supports physical Ethernet placement with both
interfaces active. It is not per-packet capture, exhaustive socket isolation or
physical link-loss/failover qualification. The workers made no host-network
changes; the ARP correction was a separately approved deployment prerequisite.

## Cleanup, reproducibility and next work

All three capacity attempts retained clean, unforced worker exits and removed
their API sockets. The independent final audit found no owned worker/generator
processes, lab listeners, API sockets or copied coordinator credentials. Every
original ARP setting was restored and verified on all five Pis, then the exact
now-unneeded rollback timers were stopped and verified inactive. No neighbour
cache entries, routes, interfaces or persistent network configuration were
changed by this round. Original lab binaries remain untouched; staged common
worker hashes still match. The operator's separately enabled sudo policy was
not modified by the experiment.

Capacity-driver elapsed time was **403.428 seconds**, including the rejected
timing attempt and final error run. Adding the conservative 600-second prior
reserve charges **1,003.428 seconds (16.7 minutes)** against the 60-minute
allowance, leaving about **43.3 minutes**; this is budget accounting, not a claim
that every preparation step was stopwatch-measured. Remaining time does not
override the stop-on-error policy or authorize an automatic retry.

Private evidence remains under `/tmp/orishu-post-m4-x5.GoWF8O`: both campaign
manifests/finished receipts, all three `capacity-*/result.json` trees and per-cell
reports, clock inputs, smoke/UDP/ARP receipts, `cleanup-audit.json`, and source
snapshots. Raw inventories, identities, interface addresses and disposable
worker credentials are private, not publication artifacts.

A mode-0600, Git-ignored archive also retains the raw receipts, indexed source
snapshots and common binaries at
`target/pi-lab-evidence/post-m4-x5-GoWF8O.tar.gz` (about 26 MB). Its gzip integrity
check passed; the private index covers 287 files. Archive SHA-256:
`9b82368dfc9e370445d3b9531fabb40205459299cec8c33124cf13b0548ea721`.
Do not publish this archive: it contains real lab addressing and private evidence.
The report above is sanitized. Final validation passed 161 Python harness tests,
five focused Rust capacity tests, formatting, documentation links and diff
whitespace checks. Full-workspace tests were not rerun for this lab-only change.

Next bounded diagnostic belongs to the [physical experiment task](../tasks/implement-physical-formation-experiment.md):
add capped, secret-free **error-variant/status counts and first-error timing**,
preserve the existing aggregate accounting/stop rules, and classify one approved
32/64-client comparison before any executor sweep. Retain worker and generator
pressure plus network counters to distinguish competing causes; do not increase
timeouts, widen timing gates or change CPU/network policy to obtain a pass.
The [executor review](../tasks/review-worker-executor-performance.md) remains open;
allocation profiling, paired observability overhead and production placement
qualification remain separate follow-ups. Historical M4 acceptance is unchanged.
