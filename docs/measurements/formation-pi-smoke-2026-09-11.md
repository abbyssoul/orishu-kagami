# Three-Pi formation/recovery smoke — 2026-09-11

Status: **scoped hardware correctness/endpoint checks passed; no physical
performance or trace-delivery acceptance claimed**.
Owner: [physical experiment task](../tasks/implement-physical-formation-experiment.md).
Operator instructions: [Pi guide](../testing-worker-pi-cluster.md).

## Configuration and provenance

The operator supplied two Raspberry Pi 5 Model B Rev 1.0 machines and one
Raspberry Pi 4 Model B Rev 1.4, each with 8 GB RAM. All report Ubuntu 26.04
ARM64, kernel `7.0.0-1017-raspi`, Python 3.14.4, wired 1000-Mbit/s full-duplex
links and MTU 1500. Private SSH aliases/addresses remain in the operator's
uncommitted inventory, not this report. Each worker used its real LAN address
on UDP 9000; the operator API stayed on a private local Unix socket.

All three remote source checkouts were clean at
`9c11d9854dc22dd3f999571bed4424ac144a505a`, matching the coordinator's
pre-increment HEAD. All four originally staged artifacts have identical hashes
across the three nodes. Executable hashes, not assumed build recipes, identify
what ran; compiler/build-environment provenance still needs a final freeze.

| Artifact | SHA-256 |
| --- | --- |
| Combined worker | `1553cc178bb09edbc53a6663a346ca846459de1f5ba14f95f6c40dd82230369d` |
| Feature-omitted worker | `53dd47079404c48c1ecfb57bb36b6f635101f4d37fa08c826779aa7ca221bfb8` |
| Fixed `orishuctl` from `pi-enabled` | `edf3d53be54586495cbd5311596bafd2c04ded653515935c6965d90811fd2aa5` |
| Originally staged probe, **not executed** | `651c59c31b521cd3371944a782de2af2b5315891306f12aa3b45b82b98965cf4` |
| Cargo.lock | `861336bb203f3c45a23213194734bfea2b7eab1e89c5ed5b781705f34abef7ae` |
| Toolchain file | `9dee632f575fa73074c503286dbf3dda7c078b20c4dd55fa8b853eb6d6d6b210` |

The hosts reported `ondemand` governors and synchronized clocks. Direct
read-only firmware checks returned `throttled=0x0` on all three; temperatures
at that preflight were 34.0°C, 35.6°C and 46.2°C respectively. The readiness
helper previously returned unavailable firmware readings, so neither these
spot checks nor a synchronization boolean certify continuous thermal health
or sufficiently small cross-host clock uncertainty. No governor, firewall,
time service, package, source checkout or staged binary was changed.
Final readiness collection confirms all four staged artifact hashes remain
unchanged on every node and all governors still read `ondemand`. A final
read-only process audit found no remaining experiment workers/watchdogs on
any of the three nodes. Private per-run snapshots/state were retained, not
deleted or exported.

## Executed scope and retained attempts

Each smoke snapshots its worker and CLI into a new private node-owned run
directory. It forms the cluster by A→B→C admission, verifies exact formation,
assigned identities, certificate bindings and live membership, propagates
lock/unlock, then leaves and readmits C. The whole operation has one 120-second
deadline, independent node watchdogs expire at 180 seconds, and cleanup has a
separate bounded allowance. No automatic mutation resubmission or cell retry.

Coordinator evidence is retained under
`/tmp/orishu-pi-experiment.3tRzEb/`; this is private local lab data, not a
published/reproducible artifact store. Each attempt has its own `result.json`.

| Attempt directory | Result | Total elapsed |
| --- | --- | ---: |
| `compiled-off-smoke` | Incomplete before worker startup: bootstrap input bug | 15.703 s |
| `bootstrap-fixed-smoke` | Formed and propagated policy; incorrect departed-member predicate and command cap stopped verification | 43.101 s |
| `history-aware-smoke` | Combined build, telemetry disabled: complete, clean cleanup | 38.935 s |
| `metrics-smoke` | Metrics-enabled: complete, all diagnostics routes pass, clean cleanup | 41.135 s |
| `omitted-smoke` | Feature-omitted worker: complete, clean cleanup | 10.785 s |

Total recorded smoke execution is approximately **149.660 seconds**. These
are implementation-verification smokes, not a new full acceptance matrix or
reuse of the previous local-host 120-minute allowance. Builds, read-only
preflights and local harness tests are separate from those elapsed values.

The first failed bootstrap used a buffered Python read that could consume the
following configuration. An exact-length unbuffered bootstrap and regression
test now preserve the code/config boundary. The second attempt incorrectly
expected departed membership to disappear; Orishu retains dead history, as
the public source/API tests specify. The verifier now expects four records,
three live identities after readmission, and the unchanged certificate binding
for C's newly assigned identity. Its command bound was raised from 256 to
1024 without extending the original time deadline.

The initial watchdog wrapper also returned timeout status 124 for intentional
TERM under the Pi's uutils implementation. `--preserve-status` now preserves
the worker's graceful exit status. The failed attempt's original cleanup
flags remain false; subsequent read-only inspection found all three node-local
cleanup receipts, no forced kills and all three sockets removed. Successful
runs report worker exit 0, no forced kill and socket removal on every node.
Local tests verify actual child termination, including coordinator EOF, rather
than treating a timeout utility's version-dependent status as proof of cleanup.

## Hardware observations, not overhead comparisons

| Mode | Initial exact formation observed | Leave/rejoin complete |
| --- | ---: | ---: |
| Compiled in, disabled | 5.320 s | 38.643 s |
| Metrics enabled | 7.383 s | 40.878 s |
| Feature omitted | 6.904 s | 10.524 s |

Times start at coordinator setup and include SSH, snapshots and CLI polling.
They are single trials with different fresh identities, not paired latency or
CPU measurements. Do **not** infer instrumentation overhead from this table.

In the metrics run each Pi returned HTTP 200 for `/livez`, `/readyz`,
`/startupz` and `/metrics`. Each exposed 151 metric series, approximately
25.7 KiB per metrics response. The listener was loopback-only. This proves
bounded endpoint availability, not Prometheus ingestion, alert behavior,
counter accuracy under load, or OTLP causal delivery on physical hosts.

Survivors recognized C's departure about 31 seconds after unlock in two runs,
versus under a second in the omitted run. The long interval is consistent
with the default three-node suspicion timeout (5 seconds × ceil(log2(3+1)) ×
3 = 30 seconds), but no packet/trace evidence was collected to establish why
the immediate departure path was not observed. This is a follow-up diagnostic
observation, **not evidence of a telemetry regression**. All three runs reached
the exact dead-history and readmission predicates inside the deadline.

## Tooling changes and validation

No worker runtime or public protocol changed. The new Python session and
coordinator implement the allowlisted smoke workflow, private snapshots,
transient join files, independent watchdogs, bounded IO and sanitized evidence.
The Rust probe adds a separate locally tested `load-node` configuration for one
local target and 3/5 global members, preserving the existing local profiles.
It emits schema 4 and is explicitly not accepted by the old local summarizer.
The originally staged ARM64 probe has not been replaced or executed.

Passed: 12 experiment-harness tests, 14 readiness tests, 36 existing formation
harness tests, nine Rust probe tests, and targeted probe Clippy with warnings
denied. The real Unix-socket lifecycle test fails under the filesystem/network
sandbox and passes on the normal host with socket permission; no test was
silently skipped. No whole-workspace regression run was needed for the
tooling-only Rust change; the targeted example and existing harness were used.
Formatting, diff checks and documentation validation (141 Markdown files)
also pass.

## Next increment

1. Freeze/build the updated ARM64 probe into a new artifact location; verify
   public typed-client behavior against the actual three-node formation.
2. Add bounded node-local load/resource sampling and coordinator scheduling,
   including measured clock uncertainty. Keep worker and tooling CPU separate.
3. Run a prospectively bounded fixed-rate capability pilot before choosing
   acceptance rate/budget. Record each Pi's results separately; the Pi 4 is not
   an interchangeable repeat of a Pi 5.
4. Secure and qualify off-Pi trace collection, review the timed matrix/budget,
   then execute it once with retained failures. Investigate the observed
   immediate-departure versus suspicion fallback if it recurs under instruments.

Five-node execution, physical performance/noise gates, trace delivery and M4
acceptance remain open. The old thirty-worker local result is not waived.
