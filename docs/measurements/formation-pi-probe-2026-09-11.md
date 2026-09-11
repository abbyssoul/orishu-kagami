# ARM64 probe build and four-Pi warmup — 2026-09-11

Status: **build, typed-client warmup and four-node recovery verified; no timed
window, rate-delivery, CPU/p95 overhead or M4 acceptance result**.
The hardware checkpoint uses input schema 1/output schema 4. The later
input-2/output-5 physical timing revision described below is not staged or
hardware-qualified by this checkpoint.
A [subsequent v2 native build/staging checkpoint](formation-pi-probe-v2-build-2026-09-11.md)
now verifies startup on all four nodes, separately from this v1 warmup evidence.
Owner: [physical-host task](../tasks/implement-physical-formation-experiment.md).
Previous evidence: [three-Pi smoke](formation-pi-smoke-2026-09-11.md).

## Frozen probe artifact

Built on the first Pi 5 using Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6), native
`aarch64-unknown-linux-gnu`. The default toolchain outside the checkout is
1.98.1; the existing installed `1.97` toolchain was correctly selected by the
repository toolchain file. No compiler or package installation was needed.

The build used a separate private source directory, not an edited remote
checkout: HEAD `9c11d9854dc22dd3f999571bed4424ac144a505a` plus the current
`apps/orishu-worker/examples/formation-telemetry-probe.rs`. Inspection confirmed
this was the only changed Rust/build input. The trusted internal archive
appends that changed file after the HEAD version, so extraction selects the
actual updated source. Local private inventories were not overlaid into it.

| Input/output | SHA-256 |
| --- | --- |
| Source archive | `a351d33f0565a7a8e419b946bb0a7efa1c81e0ed23de34d49819cbfe1e3bdd9b` |
| Updated probe source | `e9d918ff31cc6abd81646c77e2f266aa387872f180fcb45b98bb4e4b4baed1bc` |
| New ARM64 `formation-telemetry-probe-node` | `087e61e6af7e79f0afaf1e604742d91f7f1b190fa60f890362a74ff72868cbab` |
| Original probe, preserved | `651c59c31b521cd3371944a782de2af2b5315891306f12aa3b45b82b98965cf4` |

The command was `CARGO_BUILD_JOBS=2 cargo build --locked --offline --release
-p orishu-worker --features observability,otlp-tracing --example
formation-telemetry-probe --target-dir
/home/soultaker/workspace/orishu-kagami/target/pi-enabled`, from the extracted
snapshot. Cargo reported **2m02s**. Its existing build cache was reused; no
worker load experiment ran during the build.

The new binary was staged as `bin/formation-telemetry-probe-node` on all four
Pis, separately from the original probe. Hashes matched after transfer, the
original hashes remained unchanged, and `load-node --help` executed on each
Pi. The local archive/binary are retained under
`/tmp/orishu-pi-probe-source.SzVdQq/`; the private Pi build directory is
`/home/soultaker/orishu-lab/probe-build.fbOI0h` on the build node. These are
local lab artifacts, not published releases or a final acceptance source freeze.

## Four-node preflight

The selected configuration was two Pi 5s with 8 GB RAM, one Pi 4 Rev 1.4 with
8 GB and one Pi 4 Rev 1.1 with 4 GB. The user-owned four-node inventory supplied
the private addresses/aliases; it remains uncommitted.

The `probe-preflight` action ran once with the explicit new probe digest and
compiled-in/disabled worker mode. It used the normal 120-second
formation/recovery deadline and independent node watchdogs. Each probe ran
two public typed clients through 64 warmup summaries per client, against its
own Unix socket, validating four live members and the exact source/formation.
All four reached READY. The coordinator then closed their input **before G**,
so no ten-second load window began. Probe exit 1 is retained: it is the expected
EOF cancellation at the start handshake, not a successful measured load exit.

Evidence: `/tmp/orishu-pi-probe-source.SzVdQq/x4-preflight/result.json`,
run `p-5fd546f95632`.

| Observation | Coordinator elapsed |
| --- | ---: |
| Exact four-node formation | 12.199 s |
| All four probe warmups and cancellations complete | 13.314 s |
| Lock/unlock convergence complete | 16.225 s |
| Departed node retained as dead on survivors | 63.565 s |
| Exact readmission convergence | 64.480 s |
| All worker cleanup complete | 64.730 s |

Final membership contained five records and four live nodes, including the
retained dead identity of the readmitted Pi. Each worker exited 0, removed its
socket and required no forced kill. Probe termination also required no forced
kill. Private node state/snapshots remain on their nodes; no credentials were
fetched. These timings include setup and polling and are not performance
comparisons against the earlier three-node runs.

The approximately 47-second departure interval is consistent with a
45-second default suspicion timeout at four members, but does not establish
why the immediate announcement path was not observed. Retain this for the
instrumented departure investigation; do not infer telemetry overhead or
silently shorten the failure detector to improve a result.

## Node-local measurement implementation

`pi_lab_measurement.py` now supplies bounded prepare/run/close operations behind
the existing SSH session. It pins the watchdog's actual child PID, parent,
executable and Linux start time; sampling rejects identity changes. Worker,
probe and supervisor CPU are read independently from Linux process CPU clocks,
with their own recorded brackets, RSS/HWM/swap and 50-ms sampling-loss counts.
Unavailable instruments fail rather than becoming zeroes. Partial measurement
stages are retained on cleanup, without exporting arbitrary stderr or secrets.

The local window maps a caller-supplied future Unix start (2–20 seconds ahead)
to a local monotonic deadline and records lateness and wall-clock changes.
It always reports `clock_uncertainty_qualified: false` and
`acceptance_run: false`: this mapping alone cannot prove cross-host alignment.
Schema-4 load receipts have bounded, strict field/type/rate accounting checks;
missing throughput or samples remain explicit issues.

Current scope: omitted/compiled-off modes only. Timed metrics scraping,
secured off-Pi OTLP collection, coordinator-wide clock/overlap qualification,
final multi-node pilot integration/analysis and a reviewed pilot/matrix budget
remain unfinished. No physical execution of the timed sampler is claimed by warmup.

Validation includes 10 sampler tests (real Linux process clocks and watchdog
ownership, PID/executable changes, unavailable clocks, hostile reports, future
start bounds, deterministic window accounting/missed samples), 14 session
tests (including preflight and probe-cleanup failure isolation), 14 readiness
tests and 36 existing formation-harness tests. The existing Rust probe tests
and targeted Clippy apply to the unchanged probe source used for this build.
Final validation passed all 83 focused tests (including nine Rust probe tests),
targeted Clippy, workspace formatting, `git diff --check` and documentation
validation (142 Markdown files). The whole workspace test suite was not rerun
for this tooling change. A final read-only process audit found no remaining
experiment processes on any of the four Pis.

## Next required work

Post-hardware tooling increment: the preflight now retains three authenticated
clock exchanges per role, with run/nonce validation, raw wall/monotonic
brackets, conditional offset ranges and wall-clock-change evidence. Nine
clock tests plus 15 session, ten sampler, 14 readiness and 36 legacy-harness
tests passed (84 Python tests). This includes a real serialized pipe RPC and
cleanup after a mismatched receipt. No new Pi session or timed load ran for
this increment. These source changes therefore are **not** covered by the
hardware bundle hash above; that checkpoint remains immutable. Exchange-time
evidence and explicit-limit assessment do not qualify a future load window.

The subsequent single-node reader increment passed nine reader tests and the
existing suites (93 Python tests in total). It is wired into coordinator
measurement responses and has a digest-checked offline CLI, exact expected
target/role/window checks, separate process accounting and retained unmet
rate/sampling gates. Deterministic sampler-to-reader composition is covered;
hardware execution is still absent. Valid receipts remain explicitly
unqualified. These tooling edits do not change the Rust artifact or add any
Pi sessions to the hardware checkpoint above.

The concurrent-collection increment adds nine tests (102 Python tests in the
combined focused suites). It checks cohort identity/artifact consistency before
dispatch, preserves per-role results and failures, gathers before/after clock
evidence and prevents measurement resubmission. A local serialized-pipe test
joins the real coordinator and node request paths with prepared-load fixtures;
it is not a hardware timed run. Collection retains the original deadlines,
adds a 45-second ceiling, and explicitly leaves cleanup to its caller. A
complete collection remains unqualified for overlap/performance/acceptance.

The host-environment increment adds seven tests (109 Python tests across the
focused suites). Collection captures pre/post NIC counters, CPU temperature
and firmware flags outside the load/CPU window. The comparison rejects reboot,
interface replacement and counter regression instead of reporting zero cost;
missing thermal/firmware instruments remain explicit. Counters cover all host
traffic over the wider snapshot bracket, not worker-only ten-second throughput.
No continuous temperature peak is measured.

A separate read-only SSH check executed this snapshot path on all four Pis,
without installing files or starting workers/load. All returned exit 0 with
no unavailable instruments, distinct boot identities, selected interface index
2, zero firmware throttle flags and zero lifetime RX/TX error counts. CPU
temperatures by inventory role were 33.650, 35.300, 45.277 and 45.277 °C.
Lifetime RX-drop counters were nonzero on every host; these are not attributed
to an experiment. No timed rate or under-load health claim follows from them.
Source hashes for the executed read-only modules:

- `pi_lab_environment.py`: `cc16895ae8674d7018734e96dd518b48e46ae93f772c2884a7e0ea17ca771678`
- `pi_lab_node.py`: `811bfab0c3f7045f508b65ccd37e30904b831f3ded03346f767b48524f557e99`

Session recovery diagnostic: a deterministic pipe/framing test reproduced
twice that `Remote.stop()` returned a late `info` reply after `info` had timed
out. The cause was connection reuse after losing reply pairing; changing the
deadline does not recover that pairing. The fix marks the session unusable
after an RPC/validation failure and closes it without another request.
Cleanup is explicitly unverified; node cleanup/watchdogs remain responsible
for termination. The regression and four controls now pass, along with the
other focused suites (114 Python tests). No temporary debug instrumentation
was added. The existing coordinator boundary was sufficient for both the fix
and a real serialized regression seam; no worker protocol redesign was needed.
This is a new lab-harness defect, not an explanation of the historical worker
slowdown. No Pi connection or load run was made for this diagnostic increment.

## Subsequent physical timing-profile revision

The supervisor previously knew when it wrote G, but the probe did not report
its own shared arrival-window start. The new input schema 2/output schema 5
(`fixed_500_per_worker_physical_pilot_v2`) captures bracketed wall-clock reads
at the probe's shared start and END marker, the corresponding monotonic
duration, and each client's first request/last timed completion and count.
The const-specialized legacy client path omits physical activity tracking;
legacy local output schemas 2/3 and arrival algorithms are unchanged. This is
not a claim of measured performance equivalence after rebuilding tooling.

Strict readers reject old physical profiles, missing/malformed timing,
inconsistent client counts and a probe window longer than the supervisor's
control bracket. Clock discontinuities remain visible in interval evidence.
Per-client activity endpoints plus offered/completed/skipped counts still do
not prove uninterrupted traffic or qualify cross-host overlap by themselves.

All 132 focused Python tests and 11 targeted Rust probe tests passed, with
targeted Clippy, formatting and documentation checks. The restricted sandbox
initially denied the existing loopback HTTP test's socket creation; the same
test passed with normal-host socket permissions. No Pi connection, ARM64 build,
binary replacement or timed load ran for this revision. The currently staged
ARM64 probe rejects the new input schema; rebuild/stage the matching source
before the next probe warmup or pilot. Worker runtime/CLI binaries were not
changed. Historical evidence and its hashes above remain unchanged.

Qualify clock uncertainty and dispatch an explicitly budgeted capability pilot
with node-local sampling, then implement the remaining telemetry modes and
multi-node reader/analysis. A real timed pilot must test the sampler/reader composition,
not substitute these warmup results. Preserve per-node/per-configuration
baselines and the original 3/10/30 local acceptance evidence. M4 remains open.
