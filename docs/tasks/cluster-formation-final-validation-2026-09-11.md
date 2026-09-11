# Final M4 validation checkpoint — 2026-09-11

Status: **combined M4 accepted for the documented source-built Linux scope**.
Owner: [formation task](implement-cluster-formation-poc.md).
Acceptance contract: [M4 checklist](cluster-formation-m4-checklist.md).

## Accepted measurement disposition

The operator accepts the recorded measurements for the scoped formation PoC.
Retain the [108-cell curve](../measurements/formation-post-diagnostic-2026-09-10.md)
and [route-corrected Pi results](../measurements/formation-pi-ethernet-routing-2026-09-11.md)
with their original numbers, failed/inconclusive gates and hardware limitations.
This is explicit scoped PoC acceptance, not proof that every original numerical
threshold passed. Thirty-worker baseline noise, inconclusive compiled-in cost,
expensive/lossy full sampling, mixed-model capacity and generator costs remain
documented limitations. CPU/executor contention is a hypothesis to investigate,
not an established cause of the desktop noise.

No additional performance curve is required for this closeout. The
[post-M4 register](../roadmap/README.md#post-m4-operational-follow-ups) owns further
experiments and production work. Final correctness, feature independence,
operator-evidence applicability and source validation are not waived.

## Frozen source and applicability

Base HEAD: `370b0ead7449be7096dd0096933632f1e31ff794`, plus the inventoried
dirty worktree. Compiler: Rust 1.97.1 / LLVM 22.1.6; host:
`x86_64-unknown-linux-gnu`, Linux `6.17.0-41-generic`.

Local evidence directory: `/tmp/orishu-m4-closeout.CvAYre` (private, not a
portable prerequisite or permanent archive). `manifest.json` records every
tracked/nonignored file hash, HEAD, toolchain, kernel, command and two-job build
limit. The workspace batch's `finished.json` confirms **no source changes**
during that batch. Subsequent documentation updates must be distinguished from
runtime drift in the feature batch's finish manifest.

Comparison with the actual saved performance manifest, rather than its base
commit alone, finds these production differences: `Cargo.lock`, worker
`Cargo.toml`, `config.rs`, `diagnostics.rs`, `lib.rs`, `main.rs`, `runtime.rs`
and the new `net_placement.rs`. Membership, peer protocol, sampling and exporter
implementation match the measured snapshot. Placement changes socket creation
and configuration, so current feature and real-process regressions remain
required. The [seven-case namespace proof](../measurements/worker-network-placement-2026-09-11.md)
supplies explicit-interface evidence but does not replace the full formation
fault matrix or telemetry capability tests.

## Completed checks

| Check | Result / retained evidence |
| --- | --- |
| `cargo fmt --all -- --check` | Passed; `00.log` |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Passed; `01.log` |
| `cargo test --locked --workspace --all-targets` | 1,594 passed, 2 ignored, 0 failed across 57 suites; `02.log` |
| `cargo test --locked --workspace --doc` | 10 passed, 1 ignored, 0 failed; `03.log` |
| `make test-formation-release-guard` | Four helper tests passed; normal release check succeeded and fault-enabled release failed for exactly the intended compiler guard; `04.log` |
| `make docs-check` at frozen checkpoint | Passed, 157 Markdown files; `05.log` |
| Membership runtime dependency graph with neither/either/both worker telemetry capabilities | Four explicit `cargo metadata --locked --offline` resolutions passed purity, nonempty closure and 24-dependency budget assertions; `feature-graphs.json` |
| Telemetry-free worker all-target tests and Clippy | 256 passed / 2 ignored; `features/01.log`, `features/02.log` |
| Metrics-only worker all-target tests and Clippy | 301 passed / 2 ignored; `features/04.log`, `features/05.log` |
| Tracing-only worker all-target tests and Clippy | 298 passed / 5 ignored; `features-remaining/00.log`, `features-remaining/01.log` |
| Combined worker all-target tests and Clippy | 361 passed / 6 ignored; `features-remaining/03.log`, `features-remaining/04.log`; final executable build/capture passed |
| Isolated fault-build all-target tests and Clippy | 255 passed / 2 ignored; `faults/00.log`, `faults/01.log`; all ten named fault-process cases passed |
| Telemetry-free public formation/churn journey | Passed once; `normal/03.json` and log, 87.414 seconds |
| Telemetry-free client-pressure journey | Passed once; `normal/04.json` and log |
| Telemetry-free partition/heal/full churn | Passed once; `normal/05.json` and log, 91.232 seconds |
| Telemetry-free lost-leave-response/full churn | Passed once; `normal/06.json` and log, 88.798 seconds; normal batch `finished.json` confirms all seven helper/process commands passed |
| Lost join ACK recovery/full churn | Passed once; `faults/03.json` and log, 89.912 seconds; original assignment retained |
| Peer self-ejection | Passed once; `faults/04.json` and log, 35.117 seconds; learned removal over real gossip, stopped participation and required explicit leave |
| Lost departure/full churn | Passed once; `faults/05.json` and log, 88.164 seconds; original process assertions retained |
| Issuer loss/retry exhaustion | Passed once; `faults/06.json` and log; uncertainty retained through the real retry window and source restart did not restore assignment/history |
| Issuer ejection/retry exhaustion | Passed once; `faults/07.json` and log; ejected issuer lost history without changing identity and original source required bounded operator stop |
| Source loss | Passed once; `faults/08.json` and log; source history lost, issuer retained original assignment, inspection did not authorize readmission |
| Dead assignment/retry exhaustion | Passed once; `faults/09.json` and log; no revival or duplicate identity, unchanged source identity and required operator stop |
| Removed assignment/retry exhaustion | Passed once; `faults/10.json` and log; no readmission, unchanged source identity and required operator stop |
| Blocked assignment/retry exhaustion | Passed once; `faults/11.json` and log, 107.617 seconds; no replacement identity or bypass of certificate restriction |
| Excluded restart | Passed once; `faults/12.json` and log, 2.372 seconds; retained-certificate restart/fresh admission could not bypass exclusion; different-certificate control retained |
| Combined observable formation/full churn | Passed once; `observable/00.json` and log; per-worker activity/catch-up scrapes, lock/peer-loss health, leave retention and restart counter reset |
| Official Collector causal/log receipt | Passed; `observable/01.log`: Collector 0.160.0, both A→B→C admission chains, 50 matched spans/logs, 172 series/worker, probes and cross-worker policy |
| Actual Prometheus ingestion and alerts | Passed; `observable/02.log`: Prometheus 3.5.0, 172 combined series, 12 live-evaluated expressions, Collector failure/recovery and fresh re-scrape after outage |
| Official Collector mTLS | Passed; `observable/03.log`: disabled/zero/enabled modes, outage/recovery, redaction, trust/name controls and cleanup; batch confirms artifacts unchanged |
| Final-binary network placement | Passed in 83.666 seconds; `placement.json`: seven cases, 12 exit-0 workers without forced kill or leftover Unix sockets, topology cleanup verified |

The two ignored workspace tests are the subprocess-only `held_stdout_child`
helper and manual `chain_dissemination_diagnostic`; the actual parent
held-output tests ran. The ignored doctest is `model::query_filter`. The upstream
`proc-macro-error2 2.0.1` future-incompatibility warning is retained, not a
Clippy failure. No non-Linux runtime or graphics-window qualification is claimed.

Tracing adds three ignored manual sampling/allocation profiles; the combined
build also ignores the manual optimized overhead profile. Those are not rerun
as part of this functional batch. The accepted performance reports retain
their separate evidence and limitations.

One matrix-driver command failed before executing a test: it selected only
`orishu-membership` while requesting `orishu-worker/observability`. Cargo
correctly rejected that package/feature selection (`features/06.log`). Selecting
both packages corrected the invocation and all three dependency tests passed:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked -p orishu-membership -p orishu-worker --test dependencies --features orishu-worker/observability --target-dir target/formation-flow-observability
```

The original failed batch remains recorded. `features-remaining` runs only the
previously unexecuted tracing/combined rows with corrected package selection;
no failed runtime assertion was retried or weakened. The separate four explicit
metadata resolutions also verify the actual enabled feature graphs (the Rust
dependency test alone resolves its own default metadata graph).

`features-remaining/finished.json` records successful completion and only this
checkpoint document changing during that batch. The earlier feature batch's
source changes likewise contain only this document and the M4 checklist.
Normal process fixtures were copied before any feature-changing builds; no
running fixture executable was overwritten. Captured binary SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| Telemetry-free worker, `features/minimal-worker` | `915140ae09d0890e698f78ed78e29f2570c0b99f4c89e2f1ffd473c69f3bd8ee` |
| Combined worker, `features-remaining/combined-worker` | `73f99bf7caeff3b78c65ba910234a0ed801a105e3901134f59f5a0a0e2a4664c` |
| CLI, `features/orishuctl` | `3bd7cf4d8029c187f989f5a585becb1790aa6df90a66de736256bc4e854720f0` |
| Development-only fault worker, `faults/worker` | `4b4218d38e6aa38967e21190e5c774dbe683de733a71eef4b8830b3ee76dfed3` |

## Operator-recipe applicability

The [selected recipe checkpoint](cluster-formation-m4-checklist.md#current-build-operator-recipe-verification--2026-09-09)
and [post-diagnostic applicability review](cluster-formation-m4-checklist.md#operator-recipe-applicability-and-remaining-authority)
remain evidence at their recorded boundaries, not newly executed supervisor or
proxy deployments. Comparison against the measured source manifest finds no
changes to the service/container/Collector/Prometheus/proxy/dashboard recipes or
their supporting `worker_*` modules. The only changed non-lab configuration is
the worker example's empty optional `interface` block and explanatory comments;
its parsing is covered by the current configuration suite and selects no device.

Diagnostics production code, metric catalogue, log/sink/exporter behavior,
credential checks, supervisor wiring and dashboard queries are unchanged.
The `diagnostics.rs` difference is a test call's new `None` placement argument.
Unconfigured client/peer socket construction did change; current standalone
TLS/HTTP pressure, four-feature, normal/fault formation, backend and namespace
tests revalidate that seam. The new explicit placement flags have separate
namespace proof, not a new systemd/container/Pi qualification claim.

Retain earlier secured-proxy authority/pressure and dashboard/query evidence,
and both selected enabled journal/container collection passes, with this source
applicability justification. The current official Collector walkthrough and
Prometheus/mTLS runs above refresh integration without requiring a second
implementation of already verified recipes. No claim is made about formation
inside those supervisors, proxy co-deployment, published artifacts or optional
Kubernetes/cloud profiles.

## Final integrity and disposition

All ten fault-process cases completed once, with the original assertions and
deadlines. Their elapsed seconds in manifest order were 89.912, 35.117, 88.164,
187.136, 186.352, 4.177, 136.639, 107.270, 107.617 and 2.372. Shorter cases are
observed durations, not shortened runtime retry policies. `faults/finished.json`
confirms all 13 build/test/lint/process commands succeeded and the final fault
worker/CLI hashes equal their initial captured hashes.

Final comparison with the initial source inventory found only Markdown edits;
no production code, test, script, dependency, configuration or private inventory
changed during validation. The feature continuation and observable receipts
confirm their completed checks; observable artifact hashes stayed unchanged.
A host process-name audit found no remaining worker, Collector or Prometheus
fixture process. The placement report independently verifies all worker/socket
and network-topology cleanup. Private retained evidence remains under the
directory above; `placement.evidence` contains disposable credentials and must
not be committed or published. No unrelated resource was removed.

| Closure gate | Final disposition |
| --- | --- |
| N-FORMATION | **Accepted**: current workspace/core/wire tests, four normal process journeys and all ten fault journeys preserve the required identity, authority, convergence, failure and bounded-IO contracts |
| P-OBSERVABILITY for M4 | **Accepted for formation scope**: four capability configurations, live probes/metrics, received causal spans/logs, backend/outage/security checks and explicitly accepted measurements with limitations |
| P-OBS-DOCS for M4 | **Accepted for formation scope**: tested current backend journey plus retained, source-reviewed operator recipes, both selected deployment collection styles, dashboards/runbooks and documented constraints |
| Combined M4 | **Accepted** for this source-built Linux checkpoint; no required formation-stage implementation or verification remains |

This is not completion of workload-specific observability, published releases,
physical qualification of new placement flags, multi-interface failover,
Kubernetes/cloud support or larger scientific scaling. Those have owners,
tasks and target milestones in the [post-M4 register](../roadmap/README.md#post-m4-operational-follow-ups).
The scoped measurement acceptance does not certify lossless full sampling or
turn the thirty-worker noisy comparisons into numerical threshold passes.

Retained receipt SHA-256 (relative to the local evidence directory):

| Receipt | SHA-256 |
| --- | --- |
| `manifest.json` | `9cc0286b9f4b8ff72eb02e6ee8580f86818535b640ccf380971e3d2edc04d55f` |
| `finished.json` (workspace) | `08c5033dac6f9cfefd1e843ed6e7c4fe963b6c93f90ee5b94181bc26acca859e` |
| `features-remaining/finished.json` | `f0f68157eaf23b7d424b774df6a1f4f8612a07bfc9a31575d33c8ace2b036948` |
| `normal/finished.json` | `937035c2f8dccba158b1f88be255d70a7c4e397398c4c7b7036d2693a54ee1a3` |
| `faults/finished.json` | `923027df54299b483a3301ce36991e3287f3ee1a64b9308a11ee19c1ebb7b546` |
| `observable/finished.json` | `365258da448ba7bf6e3e050eccd67b9d6a6c6dda3e0f7b6dba1345bd4205328c` |
| `placement.json` | `f30c9b8c07a057378b87e380feba93476fa8c94129f29337c1c3e2cb20175b85` |

## Requirement audit

This maps the formation task's acceptance bullets, fifteen-row failure matrix,
six-step operator journey and capability/runtime matrix to the current evidence.
The [detailed failure ledger](cluster-formation-conformance.md#required-failure-matrix)
retains the named tests and their narrow boundaries; the
[historical formation disposition](cluster-formation-conformance.md#final-formation-acceptance-disposition--2026-09-07)
retains earlier applicability and failure history. Current test logs confirm the
ledger's named core/wire tests actually executed, not merely that they still
exist in source. `check_public_adoption` and `exercise_mutation_credentials`
are helpers exercised by the recorded process journeys and four executable
credential matrices respectively, not missing standalone tests.

| Required boundary | Current evidence / disposition |
| --- | --- |
| Core merge correction, pure bounded dependency graph | Workspace membership tests plus four actual feature-graph resolutions pass; no IO/clock/RNG dependency introduced |
| Bootstrap, formation/node/certificate binding, hostile wire input | Worker TLS/wire/admission suites pass; current public A→B→C and Collector journeys preserve exact identities and catch-up |
| Labels, config-free startup, private credentials/socket lifecycle, direct targeting and real inspection | Current configuration/standalone and CLI tests plus full public journey pass; no workload/HTTP3 support is inferred |
| Concurrent admissions/final slot and lock-before-insertion | Current `concurrent_allocations_recheck_certificate_capacity_and_lock_before_insertion`, `concurrent_authenticated_joins_share_one_local_capacity_slot` and `queued_policy_changes_order_admission_after_authentication` pass at core/real wire-owner boundaries |
| Collision/reconnect, stale routes and bounded cancellation | Current `simultaneous_admitted_dials_recover_crossed_connections_without_readmission`, dial/registry suites and `runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown` pass |
| Policy propagation and concurrent updates under partition | Current full normal and partition/heal journeys pass; no instantaneous global lock or arbitrary partition-topology guarantee |
| Empty/paged/changing/invalid catch-up, readiness and stale completion fences | Current catch-up client/server, runtime/driver and observable HTTP suites pass; retained receiver-fault mapping has unchanged source; completed public introduction remains separate proof |
| Lost ACK and interrupted history/restriction recovery | Current lost-ACK, issuer-loss/ejection, source-loss and dead/removed/blocked-assignment cases all pass with unchanged public identity/recovery assertions |
| Leave/no-op/replay, dropped departure, SWIM loss, crash/restart and readmission | Current normal, lost-departure and lost-public-response full journeys pass with original identity/liveness assertions and readmission budget |
| Self-ejection and retained-certificate exclusion | Current self-ejection and excluded-restart cases pass; retained credentials do not bypass restriction or authorize a replacement assignment |
| Datagram correlation/replay and size limits | Current `authenticated_reordered_datagrams_preserve_probe_ids_under_replay`, `authenticated_acks_complete_only_their_outstanding_probe`, codec/core and piggyback-budget suites pass |
| Peer/client pressure, unfinished IO, queues/frames/timers, meaningful control and shutdown | Current default/feature/fault suites and real client-pressure journey pass; namespace run separately proves 12 clean worker exits |
| Operator authority and credential-class separation | Four current Unix/TLS × HTTP/1.1/HTTP/2 mutation matrices pass; monitoring-only authority/pressure retained through the unchanged secured-proxy contract and recipe applicability above |
| Telemetry-free and neither/either/both runtime configurations, probes and supervision | Four feature test/lint configurations, configuration refusals, owner/required-role/HTTP suites and current omitted/combined public journeys pass |
| Received client→peer traces, bounded stdout correlation, telemetry outage/loss and recovery | Current Collector two-chain/log receipt, held-output/exporter suites, Prometheus ingestion/recovery and Collector mTLS pass; drops remain accounted for rather than promised absent |
| Operator stories/manuals, scrape/probe/backend recipes, dashboards, incident controls and both selected deployment styles | Current recipes above plus explicitly retained scoped systemd/container/proxy/dashboard evidence; later workload, release and optional deployment obligations stay with their owners |
| Reviewed formation-stage overhead | Explicit operator acceptance with recorded limitations above; original noisy/failed thresholds are not relabelled as passes |
| Workspace, feature/fault builds, production exclusion of fault controls, final source/artifact integrity and current documentation | Workspace/four-feature/fault build checks and release guard pass; final integrity confirms only documentation changes and unchanged fixture bytes; current task/roadmap/companion statuses link this disposition |

No new runtime/wire authority, protocol migration or supported-release claim is
introduced by this acceptance review. Profile 5 and coordinated source rebuild/
restart remain the existing contract; retained formations and profile-4 fallback
are not supported.

The temporary drivers retain command manifests, individual logs and exit/time
receipts, stop on the first failure, and never overwrite previous evidence.
Normal journeys use an immutable telemetry-free worker/CLI copy, independent of
feature-changing builds. These are functional checks, not timed performance
cells, and make no Pi, host routing, affinity or governor changes.
