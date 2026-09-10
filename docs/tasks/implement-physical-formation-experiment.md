# Implement the physical Raspberry Pi formation experiment

Status: **planned; node preparation and SSH readiness collection implemented**.
Owner: N-FORMATION/P-OBSERVABILITY physical-host evidence, with P-SCALE reuse.
Requested by the operator on 2026-09-10 after the
[local full curve](../measurements/formation-post-diagnostic-2026-09-10.md).
Operator guide: [three/five-Pi setup](../testing-worker-pi-cluster.md).

## Outcome and current gap

An operator prepares three or five dedicated ARM64 Pis, gives the coordinator
normal key-authenticated SSH access, and runs a bounded, reproducible real-peer
formation/telemetry experiment that retains sanitized results and all failures.
This is lab orchestration, not a new Orishu management service, membership
authority, package release or simulation benchmark. ADRs 0013/0017 still apply.

Implemented now: `pi_lab_node.py prepare/check`, `pi-lab.py validate/check`,
strict bounded inventory/SSH collection, example inventory and prerequisites.
These scripts deliberately cannot report an experiment pass. Actual Pis,
their OS/models and SSH aliases have not yet been supplied or validated.

The existing `measure-formation-telemetry.py` assumes all worker PIDs, sockets,
clock accounting and children are local. The Rust load probe accepts exactly
3/10/30 local targets; its validation confounds local target count with total
membership. Its test collector is loopback-only and also excludes five nodes.
Changing only the Python size list or pointing it at SSH-forwarded sockets is
not a physical-cluster implementation.

## Bounded slices

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
   independently. Support 3 and 5 global nodes without changing the existing
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
   source/profile changes or 3/5 versus 30 workers.
6. **Evidence collection and operator handoff.** Fetch only allowlisted bounded
   files with digest/schema/role/run validation; no tar of the state root.
   Record every attempted cell, worker/tool CPU separately, real network
   throughput/errors, Pi temperature/throttle/undervoltage flags, collector
   losses, measured clock/skew evidence and cleanup. Update the operator guide,
   M4 checklist and roadmap with actual commands/artifacts and explicit scope.

## Acceptance criteria

- Native ARM64 checks plus real 3-node and, when available, 5-node public
  formation journeys; no mock-only or same-host substitution for hardware proof.
- Tests reject malformed/oversized/duplicate inventories and reports, wrong
  run/role/member identity, missing targets, incompatible source/artifacts,
  stale PID reuse and credential-bearing result files. Exercise timeout,
  disconnect, child crash, partial deploy/fetch and bounded cleanup paths.
- The local probe extension tests one local target against 3/5 global members,
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

Actual Pi model/RAM, homogeneous versus mixed roles, pinned 64-bit OS image,
SSH aliases and verified host keys, real lab addresses/interfaces, coordinator
placement, and a separately reviewed physical experiment budget. Decide the
off-Pi Collector TLS/listener profile and synchronization bound before pilot
implementation. Defaults/recommendations are in the operator guide; no secret
or broad root access is needed from the user.

## Non-goals and relationship to M4

No Kubernetes/cloud qualification, public package/image publication, distributed
scientific workload, GPU, overclocking, production benchmark guarantee or
new cluster authority. Three/five real nodes are a distinct target experiment,
not evidence for thirty nodes. This task lets useful implementation/preparation
proceed while local-host performance is inconclusive; it does not silently
remove the existing M4 coverage/stability requirement. Any change to M4's final
acceptance scope needs an explicit operator decision.
