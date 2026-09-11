# Implement the physical Raspberry Pi formation experiment

Status: **partial; four-Pi pilot and sixteen-worker Unix/HTTPS capacity diagnostics executed;
host-network findings and paired telemetry/acceptance matrix open**.
Owner: N-FORMATION/P-OBSERVABILITY physical-host evidence, with P-SCALE reuse.
Requested by the operator on 2026-09-10 after the
[local full curve](../measurements/formation-post-diagnostic-2026-09-10.md).
Operator guide: [three-to-five-Pi setup](../testing-worker-pi-cluster.md).

## Outcome and current gap

An operator prepares three to five dedicated ARM64 Pis, gives the coordinator
normal key-authenticated SSH access, and runs a bounded, reproducible real-peer
formation/telemetry experiment that retains sanitized results and all failures.
This is lab orchestration, not a new Orishu management service, membership
authority, package release or simulation benchmark. ADRs 0013/0017 still apply.

Implemented now: `pi_lab_node.py prepare/check`, `pi-lab.py validate/check`,
strict bounded inventory/SSH collection, example inventory and prerequisites.
These commands deliberately cannot report a performance pass. Actual Pis,
their OS/models and SSH aliases have now been supplied and inspected. Two Pi
5s and one Pi 4 (8 GB each, Ubuntu 26.04 ARM64) passed
[bounded formation/recovery and endpoint checks](../measurements/formation-pi-smoke-2026-09-11.md).
The separate `smoke` command starts disposable workers over SSH and preserves
staged binaries; it is not a measured performance experiment.

The existing `measure-formation-telemetry.py` assumes all worker PIDs, sockets,
clock accounting and children are local. The legacy Rust load profile accepts
exactly 3/10/30 local targets. A locally tested `load-node` seam now separates
one local target from explicit 3–5-node membership and role. Its original
input-1/output-4 ARM64 rebuild and four-Pi public-client warmup passed. The
current input-2/output-5 timing revision adds probe-owned clock boundaries and
per-client activity. Its [native ARM64 build/staging](../measurements/formation-pi-probe-v2-build-2026-09-11.md)
and startup/version checks pass on all four nodes. The subsequent
[approved four-Pi pilot](../measurements/formation-pi-pilot-2026-09-11.md) now
verifies v2 warmup, timed delivery, local sampling, conditional overlap and
cleanup. It retains RX-drop findings on all hosts and is not a paired
observability comparison; the enabled-telemetry matrix remains open.
The test collector is loopback-only and also excludes five nodes.
Changing only the Python size list or pointing it at SSH-forwarded sockets is
not a physical-cluster implementation.

## Bounded slices

Operator-requested capacity extension, 2026-09-11: distinguish four physical
hosts from sixteen worker membership nodes. The separate
[`pi-lab-capacity.py` diagnostic](../testing-worker-pi-cluster.md#four-hosts--sixteen-workers-capacity-diagnostic)
uses four processes per host, explicit per-worker rates and bounded-concurrency
unpaced windows, with unchanged worker binaries and no host-policy tuning.
This is colocated summary-API capacity/failure-mode work, not a replacement for
the one-worker-per-host observability comparison or thirty-worker M4 evidence.
Keep all attempts, generator CPU, mixed-model results and cleanup evidence.
The [hardware result](../measurements/formation-pi-capacity-2026-09-11.md) now
records seven timed cells, 85.4k/86.2k aggregate local-summary requests/sec at
eight clients per worker, increasing latency at higher concurrency, and clean
independent cleanup. A probe warmup fix separates synchronous client
construction from requests; no worker binary or host setting changed. The
colocated generator consumes nearly half each host's CPU, so this is not an
isolated worker-side ceiling or a paired telemetry acceptance result.

Delivered network profile: retain sixteen workers/four hosts and offer **5,000
summary requests/sec per worker** (80,000 aggregate), configurable with
`--baseline-per-worker`. Run generators on the coordinator using the existing
authenticated HTTPS client path, ephemeral certificate-verified listeners and
worker-local operator credentials. Measure generator CPU separately and retain
actual generator/sampler timing; do not relabel Unix traffic or peer background
traffic as network API throughput. No insecure TLS or production setting change.
The [executor-sizing review](review-worker-executor-performance.md) is separately
planned after the PoC; the current test retains runtime defaults.
The [authenticated HTTPS run](../measurements/formation-pi-network-capacity-2026-09-11.md)
has now completed nine windows with verified cleanup: highest observed
36.75k/sec, confirmation 36.55k/sec, and no timed request/identity failures.
The desktop route crossed Wi-Fi; the 5,000/worker target was not met. An
all-wired comparison remains useful, not a prerequisite for recording these
findings or a replacement for paired observability acceptance.
The [wired-desktop short comparison](../measurements/formation-pi-wired-comparison-2026-09-11.md)
subsequently reached 63.36k/sec but stopped after four of six windows on four
request errors. Exact-source route checks found three Pis still using Wi-Fi
for replies. Verify both directions/interfaces before an all-wired claim;
operator-approved Pi routing and bounded request-error classification remain
next diagnostics. No network or runtime policy was changed by the run.

The subsequently approved [temporary Ethernet route correction](../measurements/formation-pi-ethernet-routing-2026-09-11.md)
completed six windows: peak 94.28k/sec, 32-client repeat 91.10k/sec, no request
errors and independent clean shutdown. Original routes are restored. A rejected
pre-load clock-plan attempt is retained separately; the harness now preserves
clock inputs before policy validation and can dial inventory IPs over SSH
without changing hostname trust. The per-worker rate target remains unmet,
with near-full worker CPU use on the Pi 4s. Earlier request-error causes,
environment findings and paired observability acceptance are not closed.

Bring forward [worker network placement](implement-worker-network-placement.md)
as bounded PoC support: ADR and one enforceable interface per peer/client role
first, full bounded lists/failover later. Include bootstrap outbound sockets
and client replies; multiple listen addresses alone do not solve this problem.
Do not silently make temporary lab routing persistent or require full
multi-homing before recording the existing evidence.

Delivered increment: `pi_lab_session.py` and `pi_lab_experiment.py` provide
120-second formation/recovery smokes in omitted/compiled-off/metrics modes,
private hash-checked worker/CLI snapshots, transient credential handling,
independent node watchdogs, retained membership-history checks, loopback
diagnostics qualification, sanitized results and cleanup receipts. Real SSH
loss and hostile-peer failure injection remain beyond the local EOF cleanup
test. No runtime worker source changed for this increment.

Next delivered increment: the [four-Pi probe checkpoint](../measurements/formation-pi-probe-2026-09-11.md)
records the native pinned-toolchain build and separate probe staging on all
four nodes, real warmup, formation/recovery, and clean worker shutdown. The
node-local sampler now records pinned process identities, high-resolution CPU,
memory, missed samples and clock mapping changes. At that checkpoint its timed
path was locally tested only; the later pilot above supplies scoped hardware evidence.

Clock evidence is now collected by the preflight coordinator: three bounded,
run/nonce-bound exchanges per node with retained wall/monotonic brackets,
round-trip delay and conditional offset envelopes. Strict local tests cover
the serialized RPC, hostile receipts, explicit exchange-only limits and
partial-batch cleanup. This does not qualify clock drift or window overlap,
or select the pilot synchronization policy. A subsequent
[read-only clock-check command](../measurements/formation-pi-clock-2026-09-11.md)
now has twelve hardware exchanges across all four Pis, without worker/root
preparation or load. Their roughly +50 ms Pi-to-coordinator offsets and
3.1–11.3 ms individual round trips inform scheduling; they do not prove future
window alignment or external wall-clock accuracy.

Offset-aware start proposal logic is now implemented and exposed as an
optional read-only `clock-check --start-policy` path. All bounds are explicit;
it rejects stale/inconsistent/replayed evidence, retains every exchange in
the conservative offset envelope and includes age/drift and wakeup allowances.
It does not approve its assumptions, dispatch requests or qualify a load
window. The pilot below now uses fresh prepared-session evidence; hardware
overlap checks still remain. The prior hardware clock checkpoint did not run
this later proposal logic.

The single-node result reader now validates exact profile/role/target (including
the private run socket), separate worker/probe/supervisor accounting, timing
identities, finite resource values and recomputed load/sampler issues. It is
used by the coordinator's measurement response path, which also checks the
requested window, and by a bounded, digest-checked offline CLI. A valid receipt
remains `valid_unqualified`, including when throughput/sampling gates fail;
it cannot claim distributed timing or M4 acceptance. Cross-mode comparison
and repeatable hardware overhead comparisons remain open; the later pilot
verifies one sampler/reader execution, not an acceptance matrix.

An internal `collect_window` seam now dispatches one measurement per prepared
node concurrently (3–5 roles), with before/after clock receipts, original
session deadlines tightened to a 45-second collection ceiling, and no retries.
It checks common run/formation/mode and worker/CLI/probe hashes before dispatch,
retains private per-role files/digests and partial failures, and never pools
mixed-node results. Its caller still owns reviewed clock-derived starts,
experiment allowance and all worker cleanup; collection explicitly leaves
cleanup and distributed timing unverified. Local serialized coordinator/node
tests cover this seam. A public pilot now owns its lifecycle as described
below; its hardware timing evidence is conditional on the recorded policy.

Actual-window overlap analysis is now implemented behind that collection seam
with an optional explicit timing policy. It revalidates raw per-role clock and
measurement receipts, bounds actual probe starts and common duration, removes
END-marker lateness, and checks both clients' first/last activity. Ten pure
contract tests and three collection integration tests cover offsets, delayed
starts/markers, malformed or inconsistent clocks, failed bounds and retained
partial evidence. These are conditional bounds under stated whole-interval
clock assumptions, not proof that those assumptions hold on the Pis, a rate
pass or M4 acceptance. The subsequent approved pilot exercises scheduling and
these bounds on hardware; the telemetry matrix remains open. The guide
defines the separate policy fields and their conditional interpretation.

The `pi-lab.py pilot` command now integrates pristine formation, all-node
warmup, fresh clock-derived starts, one concurrent window, post-window exact
membership and bounded cleanup. It shares formation/cleanup ownership with
the existing non-timed smoke without making smoke run load. An explicit
policy file is required; the proposed example does not grant approval.
Compiled-off/omitted profiles are supported, while enabled telemetry and the
full matrix remain separate work. Twelve local orchestration/CLI tests cover
3–5 nodes, failures, retained findings, unchanged deadlines and no retries.
The subsequently approved hardware pilot now has the evidence linked above.
No further allowance is inferred from the old desktop budget or an automatic
goal continuation. Implementation and one conditional timing result do not
constitute M4 acceptance or prove unsampled clock behavior.

Session recovery now treats any failed/interrupted RPC or rejected receipt as
terminal for that connection. A timed-out reply can arrive later, so resetting
the deadline and sending `stop` cannot restore request/reply pairing. The
coordinator instead closes the connection and records cleanup as unverified;
node EOF/signal cleanup and independent watchdogs retain ownership of worker
termination. Five local regressions cover late/partial replies, uncertain
writes and healthy cleanup. This fixes a lab-harness defect, not the earlier
worker slowdown, and adds no hardware acceptance evidence.

Collection now also captures bounded host-environment receipts before/after
the load, outside its CPU window: selected-interface byte/error/drop counters,
boot/interface identity, CPU temperature and firmware throttle flags. Counter
regression, reboot or interface replacement invalidates the delta; unavailable
temperature/firmware data stays explicit. These are host-level brackets,
including control traffic and waiting time, not worker-only ten-second network
measurements or a continuous temperature peak. The read-only snapshot path
passed on all four supplied Pis without starting workers. Seven local tests
cover that endpoint contract.

Periodic in-window kernel sampling is now implemented: at most ten snapshots
of temperature and selected-interface counters, one per second with explicit
missed slots, bounded read brackets and no catch-up bursts. Firmware commands
remain outside the CPU window; sample work belongs to supervisor accounting.
Measurement envelope version 2 requires the raw series, and the reader and
collection validate run/interface identity, counter continuity and availability.
Interrupted windows retain partial series. Seven additional environment tests
plus sampler/collection tests cover this path. These are sampled host values,
not a continuous peak, worker-only network attribution or hardware acceptance;
the actual sensor overhead and timed Pi behavior remain unverified.

1. **Freeze and deploy source-built ARM64 artifacts.** Capture the actual dirty
   source snapshot and lockfile/toolchain/features, build only worker/CLI/probe,
   verify the supported ARM64 ABI on each Pi and stage content-hashed binaries
   into a new private run directory. No rebuild/update during timing, no
   unverified downloads or assumed published release. Keep private runtime
   state/credentials out of result bundles; do not overwrite prior runs.
2. **Node-owned supervisor and safe control.** Start one worker per physical
   node and node-local tooling through a bounded JSON-over-SSH command surface,
   with allowlisted operations, authenticated local Unix control, explicit
   lifetime deadlines and cleanup independent of SSH survival. Record process
   identity/start time before signaling; no broad `pkill`, remote arbitrary
   shell fragments, passwordless root, automatic admission retry or secrets in
   command lines/results. Private join material stays transient and protected.
3. **Physical formation and telemetry preflight.** Bind advertised QUIC peers
   to real lab addresses, exercise A→B→C(/D/E), exact member/fingerprint/live
   and lock/unlock predicates, and the same-source feature matrix. Keep an
   original whole-setup deadline; review its physical-host value before the
   pilot rather than repeatedly resetting it. Local loopback metrics and an
   explicitly secured, off-Pi Collector retain separate authority. Verify
   actual cross-peer causal trace/log receipts and bounded shutdown.
4. **Distributed measurement seam.** Extend the public typed-client probe to
   accept bounded explicit global membership count, role and local targets
   independently. Support 3 through 5 global nodes without changing the existing
   single-host schemas or accepting an unrelated membership size. Define a
   new versioned physical profile and receiver/reader contract. Schedule a
   future window after all nodes report ready; bound clock uncertainty,
   start/overlap skew, sample vectors, local CPU/RSS brackets, watchdogs and
   report sizes. Do not assume SSH delivery or NTP implies exact simultaneity.
5. **Pilot then reviewed matrix.** Verify architecture and correctness first,
   then fixed-rate deliverability at the proposed 500 requests/s/worker. Retain
   failed capability pilots. Freeze rate, duration, resource limits, six
   modes/six rotated rounds, normal cost/noise gates and total time budget
   before performance acceptance. Separate compiled-in cost, instrumentation
   cost and full-sampling troubleshooting. Do not pool results across models,
   source/profile changes or 3–5 versus 30 workers.
6. **Evidence collection and operator handoff.** Fetch only allowlisted bounded
   files with digest/schema/role/run validation; no tar of the state root.
   Record every attempted cell, worker/tool CPU separately, real network
   throughput/errors, Pi temperature/throttle/undervoltage flags, collector
   losses, measured clock/skew evidence and cleanup. Update the operator guide,
   M4 checklist and roadmap with actual commands/artifacts and explicit scope.

## Acceptance criteria

- Native ARM64 checks plus real 3-node and, when selected and available,
  4/5-node public
  formation journeys; no mock-only or same-host substitution for hardware proof.
- Tests reject malformed/oversized/duplicate inventories and reports, wrong
  run/role/member identity, missing targets, incompatible source/artifacts,
  stale PID reuse and credential-bearing result files. Exercise timeout,
  disconnect, child crash, partial deploy/fetch and bounded cleanup paths.
- The local probe extension tests one local target against 3–5 global members,
  rejects wrong global counts/roles, and preserves existing local-profile tests.
- Every timed window records exact identity/liveness, offered/completed/skipped
  traffic, start uncertainty/overlap and CPU accounting local to the process.
  Missing instruments are unavailable/failure, not zero overhead.
- A hardware pilot and reviewed full matrix produce reproducible retained
  results. Any unmet gate remains explicit; the local noisy curve is preserved.
- The guide distinguishes installed dependencies, runnable readiness tools,
  implemented experiment operations and unverified hardware claims. Operator
  stories retain read-only telemetry and explicit control authority.

## Decisions / inputs still needed

Supplied/verified: three mixed-model 8 GB Pis, Ubuntu 26.04 ARM64, normal SSH
access, Gigabit full-duplex links/MTU 1500, identical staged artifact hashes and
clean source checkouts matching the coordinator's pre-increment HEAD. The
remaining list below applies to acceptance, not to blocking further tooling.
An additional Pi 4 Rev 1.1 with 4 GB RAM subsequently passed read-only
readiness with matching staged artifacts. Four-node inventories, smoke control
and the new probe profile are covered by local tests; four-node formation,
recovery and ARM64 probe warmup subsequently passed on hardware. Its timed
performance profile remains prospective, not a rerun to replace the
three-node evidence.

The operator approved the five-minute, one-window pilot and its explicit clock
policy; it ran once on the supplied four-node inventory with the staged v2
probe. No additional node, passwordless root or system-tuning change was
required. The initial compiled-off pilot had no Collector. Its approval is
consumed by the recorded attempt, not an authorization for automatic reruns.

After the pilot, review its rate delivery, timing, per-node resource and
environment evidence before freezing the physical comparison matrix and its
separate total budget. Enabled trace/log comparisons additionally require the
off-Pi Collector TLS/listener and credential profile. These later choices must
not be inferred from approval of the one-window disabled-telemetry pilot.

Do not replay the old 120-minute local-host allowance as a new physical
acceptance budget. Power/cooling, exact build provenance (not just the recipe),
timed execution of the rebuilt ARM64 probe and measured clock uncertainty
still need qualification.

## Non-goals and relationship to M4

No Kubernetes/cloud qualification, public package/image publication, distributed
scientific workload, GPU, overclocking, production benchmark guarantee or
new cluster authority. Three-to-five real nodes are a distinct target experiment,
not evidence for thirty nodes. This task lets useful implementation/preparation
proceed while local-host performance is inconclusive; it does not silently
remove the existing M4 coverage/stability requirement. Any change to M4's final
acceptance scope needs an explicit operator decision.
