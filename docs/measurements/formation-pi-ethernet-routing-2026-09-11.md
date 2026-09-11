# Ethernet route correction: sixteen workers on four Pis — 2026-09-11

Status: **six diagnostic windows completed; no request/identity failures;
per-worker target still missed; not telemetry overhead acceptance**.

## Outcome

With the desktop on Ethernet and temporary Ethernet-preferred routes on the
three affected Pis, observed HTTPS summary throughput peaked at **94,280.2
requests/sec**, with **91,099.1/sec** in the repeated 32-client window. This is
48.8% above the previous 63,361.3/sec peak with three wireless return routes,
and 2.57 times the original Wi-Fi-desktop peak. These sequential short runs
support a substantial network-path influence, not a randomized causal estimate
or a sustainable capacity guarantee. See the
[previous comparison](formation-pi-wired-comparison-2026-09-11.md).

Four worker processes ran on each of two Pi 5s and two Pi 4s. The unchanged
authenticated, certificate-verified HTTPS profile used ten-second windows,
32 or 64 clients per worker, one in-flight request per client and a one-second
request timeout. Metrics, traces and logging remained runtime-disabled;
compiled features, worker executors and host CPU policy were unchanged.
Generators ran on the wired desktop, not the Pis. Traffic measures the public
cluster-summary API, **not peer protocol message rate or simulation throughput**.

| Window | Offered rate per worker | Clients/worker | Aggregate requests/sec | Worst-worker p95 |
| --- | ---: | ---: | ---: | ---: |
| First baseline | 5,000/sec | 32 | 59,569.3 | 24.688 ms |
| First unpaced | — | 32 | 94,280.2 | 25.082 ms |
| First unpaced | — | 64 | 92,194.6 | 46.269 ms |
| Repeat baseline | 5,000/sec | 32 | 59,484.7 | 25.779 ms |
| Repeat unpaced | — | 32 | 91,099.1 | 24.734 ms |
| Repeat unpaced | — | 64 | 83,243.6 | 46.031 ms |

There were zero timed request errors, invalid responses or sample-cap hits.
Exact sixteen-member identities and formation state passed between windows.
The baseline offers 80,000/sec total, but minimum individual delivery was only
45.676% and 43.552% in its two windows: **5,000/worker/sec is not met**. Unpaced
aggregate throughput above 80,000 does not satisfy a fixed per-worker target;
the faster Pi 5s account for most of that aggregate. Skipped fixed-rate arrival
slots are retained as missed delivery, not hidden by a growing queue.

## Where the pressure moved

At the highest-throughput window, each row covers four workers:

| Host | Requests/sec | Worker CPU share of four-core host |
| --- | ---: | ---: |
| `pi1` | 36,823.1 | 98.04% |
| `pi2` | 37,789.1 | 98.36% |
| `pi3` | 9,971.8 | 97.26% |
| `pi4` | 9,696.2 | 96.63% |

Host labels are anonymized consistently with the
[wired-comparison report](formation-pi-wired-comparison-2026-09-11.md#outcome);
they are not SSH hostnames. Measurements are unchanged.

CPU is the strongest current bottleneck evidence. The Pi 4s remain around
10,000/sec per host even as concurrency doubles; worst-worker p95 nearly
doubles. At baseline the Pi 5 workers use about 74% of their hosts while the
Pi 4 workers already use about 97%. This is consistent with CPU-limited Pi 4
delivery, not evidence that four gigabytes of RAM causes the lower rate.

The coordinator used about 4.14 aggregate CPU cores at the peak and 6.05 in the
last 64-client window. The latter also reduced Pi 5 CPU use to 85–88%, so worker
CPU alone does not explain every window. More concurrency did not help; do not
claim an isolated server ceiling. The exact hot function, allocation cost,
scheduler cost, and cause of the previous four request errors remain unknown.
Keep measured profiling/executor comparisons in the
[post-PoC performance review](../tasks/review-worker-executor-performance.md).

## Routing, control path and rollback evidence

Three Pis preferred their metric-600 wireless subnet route over metric-1024
Ethernet, despite binding Ethernet IPs. With explicit operator approval, each
received one temporary metric-50 Ethernet subnet route, after arming an
independent twelve-minute deletion timer. Existing routes were not replaced;
no persistent network configuration, default route, Wi-Fi state or worker
setting was changed. The fourth Pi already preferred Ethernet.

Before the attempts, all sixteen exact-source Pi-to-coordinator/other-Pi route
lookups selected `eth0`; coordinator routes selected wired Ethernet. Hostname
SSH was separately observed selecting the Pis' Wi-Fi addresses through mDNS.
The successful run therefore used the new `--ssh-via-peer-address` option,
preserving hostname-based host-key trust and strict verification while dialing
the inventory's literal Ethernet IP. This is a harness destination selector,
not device enforcement in the worker.

The post-run route snapshot was collected **after automatic rollback**, so it
correctly shows the original wireless preference on three hosts; it is not
claimed as an Ethernet post-check. Retained systemd journal entries and the
same-boot monotonic sampler timestamps place rollback about 200 seconds after
the final sample on every affected Pi. Together with pre-run exact-source
routes and substantial measured `eth0` counters, this supports Ethernet load
placement throughout the windows. It is not continuous packet capture.

Across the wider before/after bracket (including setup, aborted attempt,
control checks and post-run delay), Ethernet transmit deltas were approximately
826/850/291/282 MB; Wi-Fi transmit deltas were only 3.4/10.1/10.2/10.7 kB.
Wireless receive/control/background traffic remained, so do not claim zero
Wi-Fi packets. Both NICs' counters and route/timer evidence are retained.
Explicit restoration checks confirmed every original route remained and the
temporary routes were absent. The timers deactivated successfully.

This finding motivates the prioritized
[worker network-placement task](../tasks/implement-worker-network-placement.md):
separate peer/client interfaces, including join sockets and reply egress.
Address binding alone is not that capability.

## Rejected attempt and retained limitations

The first attempt formed sixteen workers but rejected cell 0's clock plan
before measured load. It cleaned up in 35.471 seconds. Its original clock
exchange inputs were not persisted, so the precise violating exchange and
cause cannot be reconstructed. Do not attribute it conclusively to Wi-Fi.
Subsequent bounded clock-only checks passed using both hostname and direct-IP
SSH. The diagnose workflow led to preserving clock evidence **before** policy
validation and testing rejection retention; no timing limit was relaxed.
Both attempts remain in the evidence, not overwritten by a retry.

The successful attempt completed in 173.622 seconds, making 209.092 seconds
across both worker experiments, plus about five seconds of read-only clock
checks; no build was needed. All six conditional generator/sampler skew bounds
were below 59 ms under the existing 20 ms offset-change allowance. They are
conditional bounds, not independently qualified clock synchronization.

Each 64-client window missed one 100 ms process sample on the 4 GB Pi 4;
other process windows and one-second environment series had no missed samples.
Every measured host network bracket retained 4–5 RX drops; no RX/TX errors or
TX drops were observed in those brackets. Drops are host-interface counters,
not identified failed HTTPS requests. Sampled temperatures stayed below 54°C;
before/after firmware throttle flags were zero and sampled process swap zero.
The sampler does not establish a continuous temperature maximum. These
findings, short duration and absent telemetry pairing prevent an acceptance
claim. M4 and the physical observability comparison remain open.

## Reproducibility and cleanup

Private evidence root: `/tmp/orishu-pi-ethernet.k8fGfR` (not a durable archive).

- Aborted attempt: `run/result.json`, run `q-3a50fac8f8`.
- Completed attempt: `run-direct/result.json`, run `q-10374cdeb7`.
- `routes-before.json`, `routes-after.json`, `route-timer-journal.json`,
  `apply-*.json`, `restore-*.json`, `ssh-hostname-paths.json` and clock checks.
- `cleanup-audit-run.json`, `cleanup-audit-run-direct.json` independently
  verify no owned workers/generators, TCP listeners, Unix sockets or copied
  coordinator credentials; all worker exits were clean and unforced.
- `evidence-index.json` hashes result, per-cell, clock, routing and audit files;
  `harness-direct.tar` retains the revised coordinator scripts.

Unchanged SHA-256 identities:

- Worker: `1553cc178bb09edbc53a6663a346ca846459de1f5ba14f95f6c40dd82230369d`.
- CLI: `edf3d53be54586495cbd5311596bafd2c04ded653515935c6965d90811fd2aa5`.
- Native release probe: `97c07b640328839fb22d7d8f7be73680bc75af479e3e18aed8d0d77e64abb694`.
- Revised harness archive: `9e4236ad29d4c313c07deb43f13a6aa9252be8efd6314e78e478c19c02110fe4`.

Private inventories were not staged or committed. No production Rust source
or binary changed for this comparison. The control-path and clock-evidence
changes are covered by focused harness regression tests; no supported release
compatibility or migration behavior changed.

Validation: all 153 Pi-harness tests and 36 telemetry tests passed, as did
`make docs-check` and `git diff --check`. The initial sandboxed Pi test run
failed three socket-dependent cases; rerunning with local socket access passed
the complete suite. No Rust build or broad workspace Rust test rerun was needed
for these coordinator/documentation changes; tested worker artifacts were
unchanged.
