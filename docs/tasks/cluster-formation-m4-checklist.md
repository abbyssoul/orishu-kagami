# Combined M4 acceptance checklist

Status: **deployment/correctness checkpoints verified; full curve executed;
thirty-worker performance stability and final acceptance pending**.
Owner: [cluster-formation task](implement-cluster-formation-poc.md#acceptance-criteria).
Companions: [P-OBSERVABILITY](implement-worker-observability.md) and
[P-OBS-DOCS](document-worker-observability.md).

This is the finite review artifact requested by the formation task, not a new
milestone or a replacement acceptance contract. N-FORMATION remains accepted
at its historical source-built Linux checkpoint. M4 requires the companion
gates to hold together for the delivered profile-5/logging contract. Historical
passes do not certify arbitrary later dirty-worktree changes.

## Decisions and execution gates

- **Route-corrected Pi HTTPS comparison, 2026-09-11:** the
  [six-window diagnostic](../measurements/formation-pi-ethernet-routing-2026-09-11.md)
  peaks at 94.28k/sec with a 91.10k/sec 32-client repeat, no request errors and
  independently clean shutdown. Temporary routes are restored. The per-worker
  target remains unmet; this is not paired overhead acceptance. The
  [interface-placement task](implement-worker-network-placement.md) is
  prioritized PoC support, with ADR and one-interface-per-role scope first;
  full multi-homing does not reopen historical formation acceptance.

- **Sixteen workers on four Pis, 2026-09-11:** the operator-requested
  [capacity diagnostic](../measurements/formation-pi-capacity-2026-09-11.md)
  completed seven timed cells with unchanged workers, exact membership and
  independently verified cleanup. Eight clients/worker produced 85.4k and
  86.2k aggregate local-summary requests/sec; 32 clients reduced throughput.
  Colocated probes consumed nearly half each host's CPU. This is a distinct
  capacity/failure-mode result, not worker-only capacity, telemetry overhead,
  a thirty-worker substitute or final M4 acceptance.

- **Four-Pi timed pilot, 2026-09-11:** the operator-approved
  [one-window pilot](../measurements/formation-pi-pilot-2026-09-11.md) completed
  in 28.925 seconds with exact formation, 99.96–100% rate delivery, conditional
  timing bounds met and independently verified cleanup. RX-drop counters rose
  on all four hosts, so environment findings remain. This is one compiled-off
  capacity/timing result, not a paired overhead comparison or M4 closure.

- **Four-Pi probe checkpoint, 2026-09-11:** the
  [native build and warmup](../measurements/formation-pi-probe-2026-09-11.md)
  verify the separately staged updated probe, four-node formation/recovery and
  exact local-target/global-membership validation on every node. No timed
  window ran. Node-local sampling is implemented; distributed timing,
  telemetry qualification, the physical reader and final M4 acceptance remain.

- **Three-Pi correctness checkpoint, 2026-09-11:** the
  [physical smoke](../measurements/formation-pi-smoke-2026-09-11.md) verifies
  source-built ARM64 formation, lock/unlock, leave/rejoin and clean shutdown in
  omitted/compiled-off/metrics modes. All loopback diagnostic routes pass in
  metrics mode. This is not a timed overhead/trace-delivery result or M4 closure.

- **Physical-host follow-up requested:** the operator permits moving to a real
  3/5-Raspberry-Pi experiment without waiting for local tuning to succeed.
  The approved governor experiment was **not run**: `sudo -n true` required
  interactive authentication. All 20 observed policies remain `powersave`;
  no governor/affinity/runtime setting was changed. The experimental debit
  remains 114m07s. The [physical experiment task](implement-physical-formation-experiment.md)
  and [setup/readiness guide](../testing-worker-pi-cluster.md) provide an
  actionable next increment, not a waived performance gate or completed
  cross-host benchmark.

- **Latest execution:** the [post-diagnostic full curve](../measurements/formation-post-diagnostic-2026-09-10.md)
  completed all 108 cells, including all 36 thirty-worker formations, with
  unchanged source/artifacts and clean cleanup. Normal metrics/zero/default
  sampling meets reviewed gates at three/ten workers. Thirty-worker normal
  medians are below 10%, but baseline-p95 noise prevents acceptance; the
  ten/thirty-worker compiled-in-cost comparisons are also inconclusive.
  Full sampling remains high-cost/lossy, with resource-sampling skew at thirty
  workers. A separate bounded sensor diagnostic reproduces variation without
  observed thermal throttling but does not establish a cause or qualify the
  failed noise gate. Total debit is **114m07s / 120 minutes**, leaving **5m53s**.
  No further full batch, host-policy change or retrospective threshold change
  is authorized. Historical decisions/results below remain chronological.

- **Diagnostic and redistribution exploration approved on 2026-09-10:** the
  [baseline review](../measurements/formation-telemetry-2026-09-09.md#baseline-variation-review--2026-09-09)
  finds a shared first-round shift, not an isolated worker or recorded
  sample/window failure. The operator approved the five-minute baseline-only
  diagnostic and exploration of per-size budget redistribution, including a
  45-minute 30-worker allowance, within the next 90-minute total excluding
  builds. Allocation profiling is explicitly in diagnostic scope. Follow the
  [bounded pilot recipe](../measurements/formation-telemetry-plan.md#approved-baseline-diagnostic--2026-09-10);
  no machine-wide policy change or acceptance-threshold adjustment is implied.
  The [completed diagnostic](../measurements/formation-baseline-diagnostic-2026-09-10.md)
  reproduces a 2.058× baseline throughput range with observed thermal throttling
  and reduced sampled clocks; idle peers remain inexpensive and DHAT confirms
  short-lived HTTP/serialization churn. All fixtures pass correctness/cleanup,
  but performance stability does not. No full curve or M4 acceptance is claimed;
  fixed 90-second conditioning also leaves 1.325× baseline throughput variation.
  The operator subsequently approved fixed-rate acceptance; follow the
  [v3 profile](../measurements/formation-telemetry-plan.md#fixed-rate-acceptance-profile-v3--2026-09-10).
  Saturation remains separate stress evidence. No further workload/budget
  approval is pending for the recorded attempt; acceptance remains open.
  The [v3 execution record](../measurements/formation-fixed-rate-2026-09-10.md)
  retains the smoke/debit and the attempted curve: 83 complete cells, one
  thirty-worker unlock setup failure and 24 unexecuted cells, without retry.
  Three/ten-worker normal comparisons remain inconclusive on baseline-p95
  variation; default-sampling CPU medians are +10.52%/+11.70%, and full sampling
  remains high-cost/lossy. Thirty-worker rate delivery is observed but paired
  acceptance is incomplete. Bounded formation-only timing diagnostics did not
  reproduce the unlock failure; partial-stage evidence retention is now fixed
  in the harness, without changing runtime or the 60-second deadline. Next:
  obtain a repeatable convergence/policy failure and isolate observer versus
  propagation cost; investigate normal CPU/noise separately. No acceptance
  threshold relaxation, temporary exception or new full batch is authorized
  by these findings. The record accounts for all diagnostic time and failures.
  The subsequent [observer/peer-metrics diagnostic](../measurements/formation-convergence-diagnostic-2026-09-10.md)
  rules out repeated CLI startup as the sole explanation: persistent-client
  exact-members verification still takes 25–41 seconds. Existing peer metrics
  show early handshake/send failures and probe deadlines as connectivity
  develops. A deterministic serialized three-node replay confirms that a new
  member's connection must wait for trusted member discovery, preserving
  removal fences. The subsequent [adaptive-repair follow-up](../measurements/formation-adaptive-repair-2026-09-10.md)
  reproduces the delay in a bounded serialized chain and implements 1 Hz
  existing-route repair while news is queued, returning to five-second spacing
  when empty. SWIM, authentication and active-round deadlines are unchanged.
  Six native setup checks pass; the thirty-worker old/candidate means are
  45.741/27.977 seconds. Retain the bandwidth tradeoff and reassess affected
  lifecycle/feature evidence at the new checkpoint. No overhead or M4 pass is
  implied; default CPU, baseline-p95 noise and full-sampling cost/loss remain.
  The [paced-sampling follow-up](../measurements/formation-sampling-diagnostic-2026-09-10.md)
  subsequently identifies expensive repeated cryptographic-provider work under
  paced traffic and verifies a bounded non-waiting cache. Three-worker diagnostic
  median CPU overhead falls from 12.05% to 4.41%; the official Collector confirms
  both causal admission chains with 116 matched spans/logs. No thread-default,
  sampling-rate, authentication or acceptance-gate change is retained. Full
  scaling and baseline-p95 stability remain open. The
  [post-diagnostic correctness review](#post-diagnostic-correctness-checkpoint--2026-09-10)
  below is complete; these diagnostics do not authorize a new acceptance batch.

- Accepted and [scoped setup checks verified](../measurements/formation-reliability-2026-09-09.md#approved-policy-verification--2026-09-09):
  post-admission convergence uses only the remaining original 60-second setup
  budget, without renewing it between membership/summary/lock/unlock stages.
  Startup/join observations keep ten-second limits; every identity/liveness
  predicate remains mandatory. Six cases passed at 3/10/30 workers, with features
  omitted and metrics enabled. Earlier short-gate failures remain retained.
  Full overhead/M4 acceptance remains open; review per-size feasibility because
  observed thirty-worker setup plus 36 load windows may exceed 30 minutes.
- Accepted: peer profile 5 only, coordinated rebuild/restart, no profile-4
  fallback or retained formation recovery (ADR 0025).
- Accepted: bounded asynchronous structured stdout with independent runtime
  enablement and replaceable worker-owned sink (ADR 0017). No vendor sink,
  Unix-datagram adapter or logging-framework migration is required for M4.
- Accepted on 2026-09-09: normal instrumentation p95/CPU overhead goal below
  10%; up to 20% only temporarily; above 25% is troubleshooting, not normal
  compute operation. Three workers are the baseline and 30-worker coverage is
  required. The [expanded overhead experiment](../measurements/formation-telemetry-plan.md#operator-decision-and-proposed-scaling-curve--2026-09-09)
  now has explicit approval for the full 3/10/30-worker curve: six modes and six
  rounds at each size, at most 30 minutes per size / 90 minutes total excluding
  builds, retained partial results and no automatic retries. Implement and test
  the harness; freeze executable/configuration identities before measurement.
  The [v2 harness validation record](cluster-formation-conformance.md#approved-scaling-curve-and-v2-harness-validation--2026-09-09)
  covers a six-mode smoke run. The subsequent [quiet-host run](../measurements/formation-telemetry-2026-09-09.md)
  completed 36 three-worker cells but failed during the first ten-worker setup;
  a separate diagnostic observed `catchUpFailed` on the sixth worker. The fixes
  and revised-policy setup checks are recorded above. The separate
  [shutdown-log follow-up](../measurements/formation-telemetry-2026-09-09.md#shutdown-log-follow-up--2026-09-09)
  fixes a reproduced final-event contention defect and passes one real
  three-worker zero-sampling load/shutdown check. Original failures remain
  retained; timing variation, full-sampling cost/loss, per-size feasibility and
  final checkpoint/preflight applicability remain open.
- Accepted on 2026-09-09: **Option A — both existing deployment examples** must
  demonstrate enabled trace/log collection: systemd user services and rootless
  containers. The operator explicitly permits using installed Podman. Preserve
  ordinary-process three-worker proof and all existing recipe assertions.
- **Awaiting freeze:** final source/build/tool/configuration identities, exact
  command manifest and any affected regression reruns. Do not begin a final
  acceptance batch merely because independent local checks pass.

## Selected deployment verification

The ordinary-process three-worker journey proves causal admission traces and
per-worker stdout content. The systemd and rootless-container base recipes
separately prove five capability/runtime modes, private access, stop/restart
and probes with logging/tracing disabled. The selected
[enabled collection extensions](cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09)
now verify actual journal/container records against received Collector spans
across explicit restarts. Their scoped passes do not freeze the final M4
checkpoint or approve overhead budgets. Alternatives below preserve the review
rationale; both deployment extensions are selected, not still awaiting choice.

| Candidate combined boundary | Existing evidence to retain | Additional consequence if selected |
| --- | --- | --- |
| Ordinary source-built Linux processes | Official Collector admission/log journey, Prometheus log ingestion, mTLS Collector/proxy security and the separate service/container recipes | Reconcile the companion evidence and manuals at the final checkpoint. Do not claim service/container enabled-record collection. Confirm explicitly that ordinary-process collection satisfies the chosen M4 walkthrough scope. |
| Existing systemd user-service example | All of the ordinary-process and existing recipe evidence | Enable the delivered stdout/tracing contract in a bounded service profile; verify actual structured records through the journal and match received span IDs, with correct stop/identity and secret handling. No new system-wide install, boot policy or automatic restart. |
| Existing rootless-container example | All of the ordinary-process and existing recipe evidence | Verify actual structured records through the container runtime and matching received span IDs with explicit collector/network access, private mounts and bounded termination. No published image, Kubernetes or new remote-management exposure. |

Selecting a service/container extension does not automatically require repeating
all formation faults inside that supervisor. Preserve the real three-worker
causal proof separately; name any additional formation-in-supervisor assertion
explicitly. Conversely, choosing ordinary processes must not erase an existing
requirement: record the P-OBS-DOCS slice-4 applicability decision and retain
service/container collection limitations. Multiple profiles require explicit
selection, not an inferred Cartesian product.

Selected on 2026-09-09: verify enabled record collection in **both** existing
deployment examples while retaining ordinary-process three-worker causal proof.
The operator chose proper verification across both deployment styles. This
authorizes bounded journal/container collection checks and tested manuals;
it adds no service/container performance profile to the separate overhead
proposal, published artifacts, Kubernetes or cloud qualification.

## Requirement-to-evidence review

Each row must receive a final disposition: applicable passing evidence with
checkpoint justification, a named rerun, or an unresolved gap. None is a final
pass merely because it has a link. Failed required assertions stay open.

| Gate | Existing authoritative evidence / commands | Final review obligation |
| --- | --- | --- |
| Formation identity, authority, convergence and bounded failure matrix | [Formation disposition](cluster-formation-conformance.md#final-formation-acceptance-disposition--2026-09-07), [required matrix](cluster-formation-conformance.md#required-failure-matrix), [profile-5 activation](cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09); `make test-formation` | Preserve exact public A→B→C, policy, leave/readmission and restart identities; assess changed wire/owner/IO paths against the recorded fault matrix. Do not reopen every historical brief or omit affected regressions. |
| Feature/runtime independence and core purity | [Capability matrix](implement-cluster-formation-poc.md#m4-capability-and-runtime-matrix), activation's feature-omitted peer, configuration/standalone suites; worker matrix below | Cover neither/either/both capabilities, runtime-off/probes-only/export-only, omitted-capability startup refusal and zero sampling. Verify the resolved membership dependency test under the relevant feature graphs. |
| Metrics, truthful probes and supervision | [Finite inventory](cluster-formation-conformance.md#formation-instrumentation-coverage-inventory--2026-09-07), [phase audit](cluster-formation-conformance.md#pre-adoption-http-probes-and-phase-coverage-audit--2026-09-07); `make test-formation-observability`, standalone HTTP suites | Preserve measured owner/adapter activity, exact health phase semantics, driver stall, required-role failure and shutdown; a responsive exporter cannot substitute for owner progress. |
| Monitoring security and pressure | [Proxy recipe](../testing-worker-monitoring-proxy.md), [foundation audit](cluster-formation-conformance.md#m4-foundation-evidence-reconciliation--2026-09-07); `make test-worker-monitoring-proxy` | Confirm loopback/remote boundary, monitoring-only authority, probe-only route separation, bind/config refusal, slow/concurrent/header/body limits, expiry and recovery. No new cross-host qualification is inferred. |
| Received causal spans and redacted logs | [Official Collector formation receipt](cluster-formation-conformance.md#official-collector-formation-and-log-correlation--2026-09-09); `make test-worker-formation-otelcol` | Preserve both client→peer→admission chains, worker ownership, actual IDs/outcomes/timestamps, bounded records and secret exclusion. Apply the chosen deployment boundary above. |
| Export/log failure, recovery and shutdown | [Runtime log evidence](cluster-formation-conformance.md#runtime-logging-and-local-span-receipt--2026-09-09), [reader recovery](cluster-formation-conformance.md#real-worker-stdout-reader-recovery--2026-09-09), HTTP/mTLS Collector recipes and worker pressure tests | Keep collector failure separate from log failure; retain slow/closed output, recovery without restarting, bounded exit and loss accounting. No durable-delivery or lossless-shutdown assumption. |
| Real parser/backend ingestion | [Logging-counter ingestion](cluster-formation-conformance.md#logging-counter-prometheus-ingestion--2026-09-09); `make test-worker-log-prometheus` plus base/trace recipes | Preserve 151/160/163/172-series applicability, target-only labels, absence versus zero, fresh counters after actual scraper outage, and parser byte limits. Direct scrapes are not ingestion. |
| Operator manuals, stories and examples | [Recipe map](cluster-formation-conformance.md#m4-operator-recipe-applicability--2026-09-08), worker/CLI manuals, both operator story sets; dashboard, alerts and incident evidence | Reconcile selected build/catalogue and deployment commands; retain tested queries, failure/idle controls and safe read-only incident procedures. Do not claim all P-OBS-DOCS release/workload criteria are complete. |
| Formation-stage overhead | [Approved experiment](../measurements/formation-telemetry-plan.md), not the historical v1 result | Implement and test the versioned 3/10/30-worker scaling harness, freeze the measurement manifest, retain every cell/failure/loss within the accepted 90-minute limit and compare against the accepted cost bands. No accepted result exists yet. |
| Final validation and release fault exclusion | Workspace commands below, `make test-formation-release-guard`, applicable fault-process commands in the formation ledger | Run required validation at the final checkpoint; record ignored/skipped/platform-dependent cases and investigate failures. A green default build is not optional-feature or fault-build acceptance. |

## Command manifest to freeze

The [2026-09-10 adaptive-repair checkpoint](../measurements/formation-adaptive-repair-2026-09-10.md#regression-checkpoint)
changes only the production owner's repair scheduling, not its authority,
wire schema, exporter, diagnostics access or deployment configuration. It
passes the combined worker library (226 tests), serialized chain regression,
omitted partition/heal/full churn and combined observable full churn. Idle
three-worker maintenance uses 0.203 aggregate CPU-seconds per 30 seconds;
the single thirty-worker old/candidate comparison is 7.917/8.108 seconds.
These scoped checks do not re-certify the entire historical checkpoint.

| Changed-boundary applicability | Current disposition |
| --- | --- |
| Sparse dissemination, authorized-route repair, normal SWIM and active-round timing | Replay red/green, scheduler controls, six setup cases and idle controls pass; no deadline relaxation |
| Partition/heal, normal leave/readmission and restart, observable owner lifecycle | Two full native journeys and combined library regressions pass |
| Admission fault recovery, exclusion/ejection timing and optional-feature combinations | Subsequently verified in the post-diagnostic correctness checkpoint below; historical artifacts remain separately identified |
| Collector, proxy, systemd/container and operator stories/manuals | No schema, access, sink or deployment contract changed; preserve historical scoped recipes, but review final executable applicability and causal preflight before acceptance |
| CPU/p95 overhead, loss and complete scaling matrix | Fresh 108-cell matrix completed; normal three/ten-worker gates met, thirty-worker noise remains inconclusive. See the latest execution report above |

Before execution, record HEAD plus a reproducible dirty-source inventory,
lockfile/toolchain and executable hashes, feature sets, ordinary/fault builds,
target directories, OS/kernel and exact external tool/configuration identities.
The current toolchain file selects Rust 1.97; final acceptance must record what
actually runs. Collector 0.160.0 and Prometheus/promtool 3.5.0 are the existing
pinned fixture versions, not unreviewed production-version recommendations.

The table's Make commands identify existing recipes. Expand them with `make -n`,
resolve tool paths and record the resulting commands before freezing. Existing
targets use different build directories; run feature-changing builds sequentially
and never replace binaries used by a live process. Do not run performance cells
alongside builds or other test suites. Record bounded script deadlines and
prerequisites from the linked recipes; an occupied port is not permission to
stop another process. Reused evidence needs a written source/configuration
applicability assessment, not a fabricated rerun date.

Required workspace validation remains:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
make test-formation-release-guard
make docs-check
```

The worker optional-feature matrix must additionally run tests and lint for
each of `''`, `observability`, `otlp-tracing`, and
`observability,otlp-tracing`, with `--no-default-features --all-targets` and a
recorded target directory. This does not replace explicit real-process recipes
or the resolved dependency test. Freeze the exact affected fault-process rerun
list from the existing matrix rather than generating new combinations. Final
overhead commands now have a [source-built recipe](../testing-worker-overhead.md#current-formation-scaling-experiment).
Its harness validation and exact pre-run source/configuration manifest are
required before treating a run as acceptance evidence. Approval of the curve
and time limit does not freeze the complete M4 manifest automatically.

## Post-reliability regression checkpoint — 2026-09-09

This is correctness revalidation after the formation and final-log fixes, not
a final M4 freeze or another overhead attempt. The production source, tests,
scripts, manifests and lockfile were compared with the complete source inventory
in the [shutdown diagnostic](../measurements/formation-telemetry-2026-09-09.md#shutdown-log-follow-up--2026-09-09):
only six documentation files differed before this batch. Base HEAD remains
`51251791654425566d8392848b347f32c7258cb9`. Actual compiler is Rust **1.97.1**,
Linux **6.17.0-41-generic**. No runtime, test assertion or timeout was changed
by this revalidation.

The following default workspace commands passed, using `target/`:

```sh
cargo clippy --quiet --locked --offline --workspace --all-targets -- -D warnings
cargo test --quiet --locked --offline --workspace --all-targets
cargo test --quiet --locked --offline --workspace --doc
make test-formation-release-guard
```

All-target runs include the membership dependency tests and benchmark smoke
cases, not performance measurements. The default worker portion has **240
passing tests and one ignored subprocess-only fixture**; its real held-output
test ran. Doctests retain the pre-existing ignored `query_filter!` example.
The release verifier's four negative-control tests passed, then the real normal
release check passed and the fault-feature release failed for exactly the
intended compile-time prohibition. An arbitrary release compilation failure
does not satisfy that guard.

A separate `cargo metadata --locked --offline --format-version 1 --all-features`
inspection traversed only normal dependency edges from `orishu-membership` in
the resolved whole-workspace graph. It contains **19 dependencies**, including
the four expected direct dependencies, with no async runtime, networking,
clock or RNG package from the prohibited catalogue. This supplements the
default-graph tests; it is not a claim that all optional worker tests ran in
the default workspace invocation.

Normal process commands ran sequentially after one default worker/CLI build:

```sh
cargo build --quiet --locked --offline -p orishu-worker -p orishuctl
python3 scripts/test_formation_evidence.py
python3 scripts/test_formation_http.py
python3 scripts/test_formation_udp_relay.py
python3 scripts/check-formation-cli.py
python3 scripts/check-formation-cli.py --policy-partition
python3 scripts/check-formation-cli.py --client-pressure
python3 scripts/check-formation-cli.py --lost-leave-response
```

The helper suites passed **6 / 26 / 3 tests** respectively. All four process
journeys passed without retries: ordinary A→B→C formation and full churn;
operator access under opaque-UDP isolation, conflicting policy convergence
after heal and full churn; mutation-body saturation with independent
reads/peer progress, recovery and shutdown with unfinished clients; and lost
accepted leave-response recovery with an exact receipt and no second identity
change, followed by full churn. Every harness copies its executables privately
before starting workers and owns cleanup of its children and credentials.

| Artifact | SHA-256 |
| --- | --- |
| Normal `target/debug/orishu-worker` | `269c7e5e8109c3d3b0ef0216f087f204bb2b0ac3a912f81f045ce9adf34cc1c6` |
| Normal `target/debug/orishuctl` | `2dda026ff70e0fea00cb62fd6c57ce26df0193a0a069cd070cf3dee168ae829d` |
| `Cargo.lock` | `861336bb203f3c45a23213194734bfea2b7eab1e89c5ed5b781705f34abef7ae` |
| `rust-toolchain.toml` | `9dee632f575fa73074c503286dbf3dda7c078b20c4dd55fa8b853eb6d6d6b210` |

The isolated fault-feature suite passed **239 tests / 1 ignored**, followed by
all-target clippy with warnings denied. Its worker SHA-256 is
`9c1586a65cda7ee64efd5b45e26079490f762ff127893edce2550cfaa2b8fd5f`;
its CLI matches the normal CLI above. Normal binaries were rechecked and were
not overwritten. Commands for this separate batch are:

```sh
set -e
cargo test --quiet --locked --offline -p orishu-worker --all-targets --no-default-features --features formation-fault-test --target-dir target/formation-faults
cargo clippy --quiet --locked --offline -p orishu-worker --all-targets --no-default-features --features formation-fault-test --target-dir target/formation-faults -- -D warnings
cargo build --quiet --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
for scenario in lost-join-ack peer-ejection lost-departure issuer-loss issuer-ejection source-loss dead-assignment removed-assignment blocked-assignment excluded-restart; do
  python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --$scenario
done
```

All ten process scenarios passed and the sequential command exited zero: lost ACK
recovered the original assignment through full churn; peer ejection required
explicit leave; both lost departure announcements fell back to SWIM and
same-certificate readmission; issuer loss retained uncertainty through the real
retry exhaustion and later source restart; issuer ejection lost history without
changing identity and still required operator stop; source loss did not restore
operation history or authorize admission; dead-assignment recovery did not
revive or replace the assignment; removed-assignment recovery did not readmit;
blocked-certificate recovery did not create a replacement identity; and restart
with retained credentials plus a fresh admission could not bypass exclusion.
There were no assertion retries, timer changes or hidden failed runs. Both
normal and fault executable hashes were rechecked after the entire batch and
remained unchanged. Retry-exhaustion cases retain their real
183-second windows and existing 210-second harness observations, not reduced
test timers.

The independent worker capability matrix subsequently passed in the stated
order, using `target/formation-flow-observability` and the unchanged normal
worker above as the feature-omitted peer fixture:

```sh
set -e
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_MINIMAL_WORKER=/home/soultaker/workspace/orishu-kagami/target/debug/orishu-worker cargo test --quiet --locked --offline -p orishu-worker --no-default-features --all-targets --features "$worker_features" --target-dir target/formation-flow-observability
  cargo clippy --quiet --locked --offline -p orishu-worker --no-default-features --all-targets --features "$worker_features" --target-dir target/formation-flow-observability -- -D warnings
done
```

The absolute fixture path records this local invocation, not a portable
deployment prerequisite; elsewhere supply the freshly built feature-omitted
worker's absolute path. The real tracing peer test consumes that fixture,
rather than silently substituting another telemetry-enabled worker.

| Worker capabilities | All-target tests | Lint |
| --- | --- | --- |
| Neither | 240 passed / 1 ignored | Passed, warnings denied |
| Metrics/probes only | 285 passed / 1 ignored | Passed, warnings denied |
| Tracing only | 278 passed / 2 ignored | Passed, warnings denied |
| Both | 331 passed / 3 ignored | Passed, warnings denied |

Ignored cases remain the subprocess-only held-stdout helper and, where compiled,
manual allocation/overhead profiles. The real held-output/recovery and mixed-peer
tests ran. This matrix establishes the current capability/runtime regressions,
not a performance result or automatic reuse of every external deployment recipe.

One current-build official Collector formation walkthrough then passed:

```sh
python3 scripts/check-worker-formation-otelcol.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/debug/orishuctl --otelcol /tmp/orishu-curve-check.so7Ibv/full-quiet/otelcol
```

Collector **0.160.0** received both client→peer→admission causal chains for
A→B→C formation, with **50 matched spans/logs**, **172 series per worker**,
probes and cross-worker policy checks. This is one functional walkthrough, not
an overhead preflight/curve retry. The combined debug worker SHA-256 is
`8f03338db1aa9872810db54059d8136a1eaffdc9f82fa7e342b88f9aa279f834`;
the CLI is the unchanged normal artifact above; Collector SHA-256 is
`abe338fa33865e54412566db5cea4adc82374594b65b49c1099048cdf8cf2451`.
Earlier failed preflights remain retained, not reclassified by this pass.

After the batch, workspace formatting, `make docs-check` (**132 Markdown
files**) and scoped whitespace checks passed. Recomparison with the recorded
source inventory found only documentation edits; no production/test/script or
dependency drift occurred during verification. A host process-name check found
no remaining worker, Collector, Cargo or compiler processes from the completed
batch. No unrelated process or retained evidence was removed.

The external-recipe refresh below completes their current-checkpoint
revalidation. Unresolved performance gates still prevent combined M4 acceptance.

## Current-build operator recipe verification — 2026-09-09

The selected formation-stage recipes were rerun against the combined worker,
feature-omitted worker and CLI identified above. Their hashes stayed unchanged;
comparison with the complete shutdown-diagnostic source inventory found only
documentation edits. No production code, dependency, fixture assertion or
deadline changed in this refresh. These are functional source-built Linux
checks, not another performance curve or a release qualification.

| Recipe / boundary | Current result |
| --- | --- |
| Enabled systemd journal collection | Passed: two invocations, 16 actual Collector spans matched to journal records, retained credentials, fresh formation/node identities, private access and clean bounded stop |
| Enabled rootless container collection | Passed: two invocations, 16 actual Collector spans matched to container logs; the same identity/credential/stop controls; isolated shared receiver namespace and no published ports |
| Base systemd example | All five modes passed: enabled, disabled, probes-only, feature-omitted and unsupported-capability startup refusal; explicit restart and shutdown with unfinished clients retained |
| Base rootless container example | All five modes passed, including no-credential/socket startup refusal, non-root private mounts, same/separate namespace probes and explicit restart |
| Prometheus ingestion | All four catalogues passed: 151 base, 160 logging-only, 163 tracing-only and 172 combined series; an additional 172-series closed-stdout case passed. Real scraper outage/restart must ingest newly advanced counters, not retained samples |
| Alert examples | Six formation rules passed 39 finite pending/firing/recovery/exclusion scenarios. Live ingestion cases evaluated respectively 3, 3, 6, 12 and 6 expressions; no notification-delivery or production-threshold claim |
| Official Collector HTTP and mTLS | Both passed disabled/zero/enabled modes, receipt/redaction and outage/recovery. mTLS also passed missing/wrong client trust, wrong server trust/name and malformed Collector configuration refusal, with independent ready operator control |
| Monitoring mTLS proxy | Passed mutual trust, monitoring/operator authority isolation, actual scrape ingestion, capacity/recovery, pre-HTTP and downstream expiry, and proxy/upstream outage. Both downstream capacity cases observed 16,384 bytes then write expiry at about 5.004 seconds |
| Formation dashboard | Passed 37 rendered panels, eight synthetic query scenarios, tracing-off/on real workers, failed/missing/ambiguous targets, hostile selectors and Collector recovery |
| Incident/manual controls | The existing recipes exercised read-only identity/member/probe triage during telemetry outages and separately authenticated control operations. Worker/cluster stories retain these authority boundaries; no automated admission, restart or removal is inferred from a missing metric |

The dashboard browser captures were not repeated: its HTML/query source is
unchanged, and the previously inspected desktop/mobile layouts remain scoped
visual evidence. The current render/query and real-worker controls above did
run. The one-worker deployment checks remain separate from the independently
verified three-worker causal journey; this is not formation-inside-systemd or
formation-inside-Podman evidence, nor proxy co-deployment in those supervisors.

Reproduction commands with the exact local artifact arguments (the `/tmp`
paths are cached test tools, not portable installation prerequisites):

```sh
set -e
recipe_worker=target/formation-flow-observability/debug/orishu-worker
recipe_minimal=target/debug/orishu-worker
recipe_ctl=target/debug/orishuctl
recipe_collector=/tmp/orishu-curve-check.so7Ibv/full-quiet/otelcol
recipe_prometheus=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64
recipe_nginx=/tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx
recipe_base=docker.io/library/debian@sha256:abc9cb88a5587630d7f915f47b23b0668fe250fbfc6457aa4d52b534c1bbf73f
python3 scripts/test_worker_deployment_receipts.py
python3 scripts/test_worker_formation_receipts.py
python3 scripts/test_worker_otelcol.py
python3 scripts/test_worker_prometheus_logs.py
python3 scripts/test_worker_trace_collector.py
python3 scripts/test_worker_formation_alerts.py --promtool "$recipe_prometheus/promtool"
python3 scripts/test_worker_monitoring_proxy.py
python3 scripts/check-worker-deployment-logs.py --deployment both --worker "$recipe_worker" --ctl "$recipe_ctl" --otelcol "$recipe_collector"
python3 scripts/check-worker-user-service.py --worker "$recipe_worker" --minimal-worker "$recipe_minimal" --ctl "$recipe_ctl" --promtool "$recipe_prometheus/promtool"
python3 scripts/check-worker-container.py --podman podman --base-image "$recipe_base" --worker "$recipe_worker" --minimal-worker "$recipe_minimal" --ctl "$recipe_ctl" --promtool "$recipe_prometheus/promtool"
check_metrics() {
  python3 scripts/check-worker-prometheus.py --worker "$recipe_worker" --ctl "$recipe_ctl" --promtool "$recipe_prometheus/promtool" --prometheus "$recipe_prometheus/prometheus" "$@"
}
check_metrics
check_metrics --log-metrics
check_metrics --trace-metrics
check_metrics --log-metrics --trace-metrics --formation-alerts
check_metrics --log-metrics --trace-metrics --closed-log-output
python3 scripts/check-worker-otelcol.py --worker "$recipe_worker" --ctl "$recipe_ctl" --otelcol "$recipe_collector"
python3 scripts/check-worker-otelcol.py --mtls --worker "$recipe_worker" --ctl "$recipe_ctl" --otelcol "$recipe_collector"
python3 scripts/check-worker-monitoring-proxy.py --nginx "$recipe_nginx" --worker "$recipe_worker" --ctl "$recipe_ctl" --promtool "$recipe_prometheus/promtool" --prometheus "$recipe_prometheus/prometheus"
python3 scripts/check-worker-dashboard.py --worker "$recipe_worker" --ctl "$recipe_ctl" --promtool "$recipe_prometheus/promtool" --prometheus "$recipe_prometheus/prometheus"
```

Helper checks also passed: deployment receipts **7**, formation receipts **8**,
Collector reader **9**, logging ingestion **6**, trace collector **3**, proxy
evidence **9**, and the **39** formation-alert scenarios. The alert helper's
first invocation omitted its required `--promtool` argument and exited during
argument parsing, before any ingestion case started. The corrected invocation
passed; no failed runtime assertion was retried or waived.

Tools remain systemd **257.9-0ubuntu2.5**, Podman **5.4.2**, Collector **0.160.0**,
Prometheus/promtool **3.5.0**, and the previously extracted debug-enabled
Nginx **1.28.0**. Additional tool SHA-256 identities:

| Tool | SHA-256 |
| --- | --- |
| Prometheus | `4ee49f574d80682b6571f017c8f42f2532aa3603b725b51ecb0cb864f06c2282` |
| promtool | `e59210d5474e2c811a4460fb1593d708b94d58107ae62e9d44e88029bb7e5805` |
| Nginx | `4cd30bdb94762bef1c37bda2a92560d2fdc604df250fae6edb3f7700ee3f85fe` |

The enabled collection image was
`2c32e32029f8aebb16a25d1847a5f9e587cba037eee07973c7ec826994b7f4ea`;
base-regression enabled/minimal images were
`2e697e0931b8c77e0c43df3c2810d7946c5a057d65734e490edc55a28aef58cd` /
`d6ff4261e6a09367452a568d9d7c5d19f04a8c44a144f683d22c61af5b22f530`.
Runtime packages remained ca-certificates `20250419`, curl
`8.14.1-2+deb13u4`, libc6 `2.41-12+deb13u3`, libgcc-s1 `14.2.0-19`.
These identify observed builds, not frozen package resolution or published
images. Fixture images/containers and runtime-linked units were removed;
private test PKI/state/receipts were cleaned. Shared journals, the pinned base
and reusable build cache remain. Read-only audits found no remaining fixture
units, labeled containers/images or worker/monitoring processes.

Trace/log correlation and formation-stage operator-recipe implementation are
verified at this checkpoint. The remaining M4 work is the reviewed performance
experiment and final acceptance against its delivered source/profile. Preserve
these passes and rerun only evidence affected by subsequent changes; do not
reopen all delivered recipes merely because performance is still unresolved.
Workspace formatting, scoped whitespace checks and `make docs-check` (**132
Markdown files**) passed after this documentation reconciliation. No new
runtime or wire behavior is introduced by the status updates.

## Post-diagnostic correctness checkpoint — 2026-09-10

The adaptive reconciliation scheduler and bounded sampling cache have now
completed affected correctness verification. This supersedes the earlier
feature/fault counts for these changed paths, not the historical performance
failures. The [formation report](../measurements/formation-adaptive-repair-2026-09-10.md)
and [sampling report](../measurements/formation-sampling-diagnostic-2026-09-10.md)
identify the source changes, deterministic regressions, native comparisons
and limitations. No membership authority, wire field, dependency, operator
setting or migration contract changed.

| Current check | Result |
| --- | --- |
| Four worker feature combinations, all targets and warnings-denied Clippy | Neither: 245 passed / 2 ignored; metrics: 290 / 2; tracing: 286 / 5; both: 342 / 6 |
| Isolated fault-feature all-target tests, lint and build | 244 passed / 2 ignored; lint/build passed |
| Ten fault-process scenarios | All passed once: lost join ACK, peer ejection, lost departure, issuer loss/ejection, source loss, dead/removed/blocked assignment and excluded restart |
| Normal process journeys | Plain formation/churn, client pressure and lost leave response all passed once; the preceding policy-partition pass uses the identical feature-omitted worker and remains applicable |
| Changed observability paths | Preceding combined observable formation/churn passed; current combined release worker passed official Collector 0.160.0 A→B→C formation with two causal chains, 116 matched spans/logs and 172 series/worker |
| Workspace | All-target tests/Clippy, doctests, formatting and release fault-exclusion guard passed; existing ignored doctest retained |

The actual held-output/recovery and mixed-feature peer tests ran. Ignored
tests are explicitly scoped helpers/manual diagnostics, not skipped acceptance
assertions. No fault or normal-process assertion was retried, and the original
retry windows, identity/exclusion assertions and shutdown checks were retained.

Local executable identities, rechecked after verification:

| Artifact | SHA-256 |
| --- | --- |
| Combined release worker | `b3f7db9b2bbb58f571e0e1bcc1f752d8562e5fa725a2c724c19a303fd7a95371` |
| Feature-omitted release worker | `754a4377cfd9f4fb15587f25d30d25bd0c9e8f68f5bbfc3a57adb33297f678b7` |
| Isolated fault debug worker | `832d70dcacea5aa0632799b49b1023aece481b4292b317d877c1094e44106e23` |
| Isolated fault CLI | `2dda026ff70e0fea00cb62fd6c57ce26df0193a0a069cd070cf3dee168ae829d` |
| Frozen normal CLI | `ca2ea5d6837b0c8dc3a6149c47eccacb506a02ebb60f1d34f142a542d53251ff` |

Reproduction of the current normal/fault process checks:

```sh
python3 scripts/check-formation-cli.py --worker target/formation-telemetry-omitted/release/orishu-worker --ctl /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/ctl
python3 scripts/check-formation-cli.py --worker target/formation-telemetry-omitted/release/orishu-worker --ctl /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/ctl --client-pressure
python3 scripts/check-formation-cli.py --worker target/formation-telemetry-omitted/release/orishu-worker --ctl /tmp/orishu-curve-check.so7Ibv/fixed-rate-full-20260910/ctl --lost-leave-response
set -e
for scenario in lost-join-ack peer-ejection lost-departure issuer-loss issuer-ejection source-loss dead-assignment removed-assignment blocked-assignment excluded-restart; do
  python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --"$scenario"
done
```

The four-feature commands above were repeated with `CARGO_BUILD_JOBS=2` and
the absolute `target/formation-telemetry-omitted/release/orishu-worker` fixture,
not the older debug fixture. The current Collector command used
`target/formation-telemetry-enabled/release/orishu-worker` and the frozen normal
CLI above. Local `/tmp` paths identify retained artifacts, not installation
requirements. Source is the dirty worktree based on
`b52dc1abc62741d91f4b18dcc8c9d2c322bccbd9`; performance diagnostics retain their
own exact source/artifact manifests. A future acceptance run must freeze anew.

### Operator-recipe applicability and remaining authority

The preceding systemd/container, scraper, proxy, dashboard and manual checks
are retained scoped evidence, **not claimed as rerun on this release worker**.
The changes do not alter deployment wiring, credentials, HTTP access/probe
contracts, metric catalogue, trace/log fields, sink lifecycle or dashboard
queries. Changed formation timing is covered by the lifecycle/fault/feature
checks; changed random-block production is covered by cache regressions,
serialized exporter tests and the current official Collector journey. No
operator-story/manual contract needs revision for this private optimization;
the observability manual now documents its bounded storage and fallback.
Reassess this applicability if subsequent changes touch those boundaries.

Remaining work is performance acceptance and its final requirement-to-evidence
disposition. The conservative experimental debit is **63m22s of 90 minutes**,
excluding builds, leaving **26m38s**. A fresh unchanged 108-cell curve is
estimated at roughly 40–50 minutes, not guaranteed to fit that remainder.
The operator subsequently **approved the 120-minute total experimental cap
and required activities**. The [fresh batch plan](../measurements/formation-telemetry-plan.md#approved-post-diagnostic-batch--2026-09-10)
enforces the proposed 55-minute batch cap, retaining all rate/setup/loss/noise/
overhead gates, no automatic retry and no host-policy change. The earlier
26m38s remainder belongs to the superseded 90-minute allowance. The new
remainder is 56m38s; approval is not performance acceptance.

## Final disposition

The [completed post-diagnostic experiment](../measurements/formation-post-diagnostic-2026-09-10.md)
closes execution of the requested curve, **not** the combined performance gate.
Its final integrity check covers the frozen worker/source/profile above.
Subsequent changes are reporting/documentation only. N-FORMATION and scoped
operator/correctness evidence remain valid at their documented boundaries;
P-OBSERVABILITY's full formation-overhead acceptance and combined M4 stay open.
The remaining blocker is a reviewed path to stable thirty-worker measurement
and compiled-in-cost comparison, not another formation implementation slice.

Report N-FORMATION, formation portions of P-OBSERVABILITY/P-OBS-DOCS, and
combined M4 separately. Workload execution/storage/observation telemetry,
supported package/image publication, optional Kubernetes, cloud qualification,
credential rotation and future sinks remain with their existing owners; they
are neither implicitly required nor implicitly completed by this checklist.

Any required assertion that fails or lacks sufficient current evidence keeps
M4 open. Any later change to the frozen profile needs explicit scope/applicability
review, not retroactive budget fitting or a silent exemption. This document
records the accepted deployment selection but does not approve overhead budgets
or execute a final validation batch.
