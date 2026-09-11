# Four-Pi fixed-rate pilot — 2026-09-11

Status: **completed with environment findings; rate, conditional timing and
cleanup gates met; not an overhead comparison or M4 acceptance**.
Owner: [physical experiment task](../tasks/implement-physical-formation-experiment.md).
Predecessor: [v2 ARM64 build/staging](formation-pi-probe-v2-build-2026-09-11.md).

## Approved execution and scope

The operator explicitly approved one four-Pi pilot using the proposed policy
and a separate five-minute allowance. Executed once in `compiled_off` mode:
observability/tracing capabilities compiled in but disabled, including logs.
No Collector, metrics scraping, builds, installs, tuning, acceptance matrix or
automatic retries. Real QUIC peers used the lab network; typed client load
used each worker's private local Unix socket, not a remote client/TLS endpoint.

Run `p-df9794ecba0c`; coordinator reference start
**2026-09-10 22:39:49.125529 UTC** (2026-09-11 in the operator's timezone).
The pilot finished in **28.925 seconds**, below its 300-second allowance.
The separate read-only artifact/port precheck took 2.866 seconds and the final
cleanup audit 4.078 seconds. Those are command durations, not the full elapsed
time of the interactive analysis. Unused allowance did not trigger a rerun.

Formation converged at 9.402 seconds. All four probes completed warmup by
10.020 seconds; fresh clock exchanges followed. The post-window exact
formation/node/fingerprint/live check passed at 28.631 seconds.

## Fixed-rate delivery and resources

One ten-second window, 500 offered requests/s/worker, two clients per worker.
The delivery criterion was at least 4,950 timed completions of 5,000 offered.
CPU is **absolute utilization of one core**, not telemetry overhead or a
percentage increase versus another build. Pi models are not pooled.

| Role / hardware | Timed completions | Request p95 (µs) | Scheduled-arrival p95 (µs) | Worker CPU | Probe CPU | Supervisor CPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 — Pi 5, 8 GB | 5,000 / 5,000 | 121.982 | 1,932.622 | 2.312% | 3.950% | 1.604% |
| 1 — Pi 5, 8 GB | 5,000 / 5,000 | 120.870 | 1,823.774 | 2.283% | 3.918% | 1.621% |
| 2 — Pi 4, 8 GB | 4,999 / 5,000 | 654.086 | 2,240.557 | 10.465% | 16.447% | 7.500% |
| 3 — Pi 4 Rev 1.1, 4 GB | 4,998 / 5,000 | 632.431 | 2,016.864 | 10.743% | 17.640% | 7.641% |

Role 2 skipped one arrival. Role 3 skipped one and had one tail request outside
the timed-completion window. No sample caps were hit. All four meet the 99%
delivery criterion; this is not 100% delivery on the Pi 4s. Scheduled-arrival
latency includes generator scheduling delay and must not be confused with
observed request latency. First client requests occurred 1.11–4.10 ms after
their probe starts; every client's last timed completion was beyond 9.996 s.

Every node retained all 200 resource samples and ten kernel/environment
snapshots, with no missed or failed samples. Worker RSS peaks were
21,116 / 21,132 / 21,048 / 21,040 KiB, respectively; sampled worker swap was
zero. Worker lifetime high-water marks equalled their setup marks. This short
observation does not prove allocation-free hot paths or long-term stability.
Probe and supervisor costs are material on the Pi 4s and remain separate from
worker CPU; a later comparison must keep this measurement profile fixed.

## Timing and environment findings

Actual-window review: `within_conditional_bounds`, with conservative pairwise
start skew **59.357 ms** (limit 80 ms) and common-window lower bound
**9.935643 s** (minimum 9.9 s). These are bounds under the approved clock
assumptions, not exact measured skew or proof against unobserved clock steps.
The saved review deliberately retains `clock_uncertainty_qualified: false`
and `acceptance_run: false`.

| Role | Sampled temperature peak | RX-drop increase in periodic read bracket | RX-drop increase in wider pre/post bracket |
| --- | ---: | ---: | ---: |
| 0 | 33.100 °C | 5 | 9 |
| 1 | 34.200 °C | 5 | 9 |
| 2 | 44.303 °C | 4 | 9 |
| 3 | 44.790 °C | 5 | 9 |

Firmware flags were zero before and after on all nodes; they were not sampled
continuously. Temperatures are one-second samples, not continuous maxima.
RX/TX error counters and TX-drop deltas were zero. The nonzero host RX-drop
deltas caused `role_0_environment` through `role_3_environment` and CLI exit 2
(`complete_with_findings`). They were retained, not waived or subtracted.

These are whole-interface counters, not Orishu packet-loss measurements.
The periodic first/last read bracket is approximately nine seconds. Three
nodes reported zero TX-byte delta in that bracket; counter freshness and
traffic attribution need separate checking before interpreting network rates.
No cause is established for the RX drops, and this pilot does not attribute
them to saturation, worker activity or a particular background protocol.

## Cleanup and evidence

All four workers and probes exited 0, without forced termination; client
sockets were removed. Worker stdout and probe stderr were empty as expected
for this disabled-logging run. A subsequent independent SSH audit read each
node's cleanup receipt and found **no remaining process executing from this
run's binary directory and no client socket**. No originals were overwritten.

Private evidence root: `/tmp/orishu-pi-pilot.L6a6Jm/`.
It contains `precheck.json`, `cleanup-audit.json`, and `run/result.json` plus
the window manifest, raw per-node measurements and reviews. The collection
digest and all four measurement digests were independently checked through
the strict reader. Inventories and private raw evidence were not committed.

| Identity | SHA-256 |
| --- | --- |
| Approved policy | `df7b2201164f39e94f4a89df284bbb01a70f864e7e18ae8835d9c2406efbee45` |
| Worker | `1553cc178bb09edbc53a6663a346ca846459de1f5ba14f95f6c40dd82230369d` |
| Fixed CLI | `edf3d53be54586495cbd5311596bafd2c04ded653515935c6965d90811fd2aa5` |
| ARM64 v2 probe | `9e44762e7ae766881677fec865b23f7f76ed5025a3dc0e0a7913a16e0af1c9d3` |
| Streamed node bundle | `3226c8cf6597360be7465582d9fdb1624df94c1520ebe187d4e8795cf85f954b` |
| Coordinator | `c3f6162f00a86644deff8a2584011e6897d569c89fc8cf385b2bf9a7f246693b` |
| Result reader | `4e63d1e6fceba8bc758500eb0735bc6d190252c785bea7a611587bf46905761d` |
| `run/result.json` | `f098529e6300d79f635303e1e86da242566a1ea6a5128709b765e6eb1ee6e51b` |
| `window/collection.json` | `070fbfd6074cee014cda151b83e64d4718efe5af7069162434511710c5952609` |
| `window/manifest.json` | `3c9a357b232baa4edf3c9e338d921b838295b5a3842396ab915e7aeb31366ff5` |

## Disposition and next work

The 4 GB Pi participates successfully in this bounded formation/load profile.
This run does not reproduce a capacity collapse, but a single fixed-rate,
disabled-telemetry pilot cannot establish a scaling curve, instrumentation
overhead, or the cause of earlier desktop slowdown. M4 remains open, including
the separate thirty-worker/noise acceptance disposition.

Next: qualify the host network counters with an idle/control-traffic check,
then review a paired physical comparison with the same profile and per-node
baselines. Enabled metrics/trace modes, off-Pi Collector security and the full
matrix/budget still need their planned implementation/review. Do not change
gates retrospectively or infer authorization for another workload run from
this completed pilot.
