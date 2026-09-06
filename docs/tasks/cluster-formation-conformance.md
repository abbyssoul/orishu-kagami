# Cluster-formation conformance ledger

Status: **incomplete — acceptance audit, not another implementation task**
Checkpoint: **2026-09-07**
Owner: [N-FORMATION](implement-cluster-formation-poc.md)

This maps the task's required failure rows to inspected tests and remaining
work. It does not replace the task's other acceptance criteria or the combined
M4 observability gate. Keep historical narratives in the task/integration
record; update this table when a specific requirement gains evidence.

## Workspace validation refresh — 2026-09-07

The task's default workspace gates were rerun against the current dirty tree
on base `4d6cfba4021e7feeb988c525c7f6fac03150a2ff`, including the recent admission
instruments, catch-up HTTP test and the concurrent workspace additions. This
is a checked integration checkpoint, not final acceptance of unfinished work.

Passed:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc
make docs-check
```

Default all-target tests included the worker's 151 tests, shared client and
membership tests and dependency-purity checks. Criterion targets ran smoke
cases, not performance measurements; absent Gnuplot selected the plotters
backend. Four documentation examples passed; the existing `query_filter!`
example remains explicitly ignored, not verified. Clippy reported the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning, not a denied lint.
Documentation validation passed for 115 Markdown files.

After the default executable tests finished, the combined optional build also
passed `cargo test --locked --offline -p orishu-worker --features
observability,formation-fault-test --all-targets --quiet`: 165 tests (113
library, 24 binary, 28 executable integration), including stalled-owner HTTP
supervision and the new catch-up regression. Clippy with the same feature pair
and `--all-targets -- -D warnings` passed. This is the existing debug fault
feature plus metrics/probes, not the still-unimplemented OTLP feature matrix.

Tests used local socket permission and the default `target` directory. No
implementation or unrelated workspace edits were made by this validation.
Older dated validation sections below remain historical; they do not supersede
this checkpoint. This does not rerun the separate fault-process journeys,
prove test hooks absent from releases, close the stale-IO/HTTP2/recovery audit,
or implement secured monitoring and OTLP. Final acceptance must rerun applicable
gates after the remaining changes and preserve those missing requirements.

## Issuer admission metrics — 2026-09-07

The dirty-worktree worker now publishes three fixed issuer counters: local
core insertion, local core refusal (including redirects), and validated retained
assignment replay. New insertion/refusal counts consume actual core publications;
replay increments only after validation, encoding and session promotion. The
early replay return publishes its updated view without refreshing owner health.
No wire schema, dependency, membership authority or runtime default changed.

`real_join_packet_drives_owner_lock_token_and_admission_outcomes` and
`queued_policy_changes_order_admission_after_authentication` exercise actual
QUIC requests and owner decisions. Assertions distinguish seeded membership from
new admission, valid replay from insertion, and malformed/conflicting/retired
retries from core refusals. Counts survive a real owner leave/replacement and
remain readable after shutdown. This is wire/owner evidence, not a new
three-process telemetry journey.

Validation at this checkpoint:

- `cargo test --locked --offline -p orishu-worker --features observability
  --all-targets --quiet`: passed 162 tests (113 library, 21 binary, 28 executable
  integration), before the additional replacement/retention assertions.
- `cargo test --locked --offline -p orishu-worker --lib
  peer::registry::tests:: --quiet`: the sandbox run failed all ten tests at
  socket creation (`Operation not permitted`); the same command with socket
  permission passed all ten, including the additional retention assertions.
- `cargo clippy --locked --offline -p orishu-worker --all-targets --features
  observability -- -D warnings`: passed after the retention assertions.
- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`:
  passed all 151 default-feature tests (113 library, 18 binary, 20 executable
  integration) after the retention assertions, with socket permission.
- `cargo fmt --all -- --check` and scoped whitespace checks passed;
  `make docs-check` passed (114 Markdown files at the final check).
- `make test-worker-prometheus` with `PROMTOOL` and `PROMETHEUS` under
  `/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/`: passed
  actual ingestion of all 26 series, three alert rules and standalone control
  progress across scraper outage/restart. The test builds the observability
  worker/CLI in `target/debug`; tools are the pinned versions in the test guide.

Real HTTP tests check names/types and a conservative maximum-size projection
under the existing 6 KiB bound. Positive admission values are established by
the wire/owner fixture; the standalone Prometheus journey does not prove
three-worker positive admission scraping. Full fault-process reruns, optional
trace/security matrices, overhead measurements and workspace-wide final
acceptance remain open. See the updated catalogue and operator instructions;
this increment does not close P-OBSERVABILITY or N-FORMATION.

## Three-process admission scrapes — 2026-09-07

The shared public formation harness now has an opt-in `--observability` mode.
It enables three loopback diagnostics listeners, retaining production
QUIC/mTLS, operator authentication, exact membership assertions, bounded
failure capture and the full leave/readmission/crash/restart journey. After
A admits B and B admits C, actual HTTP scrapes must report insertion counts
`[1, 1, 0]` and no core refusals. Restarting B must reset all three admission
counters. The lost-ACK variant additionally requires A's assignment-replay
counter to be positive without a second insertion; B/C cannot claim replay.
Operator and join credentials are excluded from the bounded scrape response.

Commands and results in this dirty-worktree checkpoint:

- `make test-formation-observability`: passed the full journey with the
  observability feature. Its first sandbox execution failed before formation
  at peer socket creation (`Operation not permitted`), recorded in
  `/tmp/orishu-formation-failure-9gjrpime`; the socket-enabled run passed.
- `make test-formation-observability-lost-ack`: passed the full journey with
  both observability and the debug-only formation fault feature, including
  the additional join-token redaction assertion added after the ordinary run.
- `python3 scripts/test_formation_http.py`: 21 tests passed with socket
  permission, including refusal of partial/unmapped observability scenario
  combinations before startup. A sandbox run failed nine socket-dependent
  cases before their assertions; that was not a passing test run.
- `python3 scripts/test_formation_evidence.py`: six tests passed.
- `make test-formation`: passed after both telemetry journeys, rebuilding
  ordinary worker/CLI binaries without either optional feature and running
  all 27 helper/evidence tests plus the full public process journey.
- `make docs-check`: passed (114 Markdown files); scoped whitespace passed.

Feature builds ran sequentially against `target/debug`. These new targets
perform direct HTTP reads, not multi-target Prometheus ingestion. The separate
Prometheus test supplies backend validation. Positive core-refusal counters
remain established by the wire/owner fixture, not this successful admission
journey. Full probe transitions, secured exposure, telemetry overload/outages,
OTLP and measured overhead remain companion acceptance gaps. No worker code,
protocol, dependency, admission rule or default exposure changed in this
harness increment. The overall formation and M4 gates remain open.

## Process probes across wire ejection — 2026-09-07

`make test-formation-observability-ejection` passed with the `observability`
and debug-only `formation-fault-test` features in `target/debug`. This adds
real HTTP probe assertions to the existing public-process peer-ejection
journey, without a health override or a worker behavior change:

- After B completes authenticated admission, all three initialized processes
  report `200` for liveness, readiness and latched startup.
- A sends the existing valid wire tombstone fixture; B reports ejected through
  the public operator API and stops old-formation participation. After survivor
  SWIM observes B as dead, B reports live `200`, ready `503`, startup `200`.
- A remains live/ready/startup-complete despite losing B. B's metrics remain
  readable without fabricating local admission events.
- Explicit authenticated leave returns B to fresh standalone identity and all
  three of its probes return `200`; no restart or automatic readmission is used.

Each probe read uses a two-second socket timeout and a 1 KiB body bound;
failure evidence records route/status, not payloads. The harness explicitly
rejects combining ejection and lost-ACK scenarios. The HTTP/helper regression
suite passed 22 tests with local socket permission. This scenario does not
exercise C's handoff, subsequent crash/restart, transient joining/catch-up,
or cluster-wide administrative removal convergence. Those scopes remain
separate; this evidence does not close the full probe matrix or M4 gate.

After the optional ejection journey, `make test-formation` rebuilt ordinary
worker/CLI binaries and passed all 28 helper/evidence tests plus the full
public handoff/leave/crash/restart/readmission journey. Builds were sequential.
`make docs-check` passed (115 Markdown files), as did scoped whitespace checks.
Workspace-wide Rust and other fault/telemetry feature matrices were not rerun
for this harness-only increment.

## Unresolved-admission probes and executable isolation — 2026-09-07

`make test-formation-observability-issuer-loss` now checks real HTTP on the
surviving applicant after the existing post-insertion issuer crash and again
after retry exhaustion. The source remains live (`200`) but unready (`503`),
with startup latched (`200`); unrelated standalone C remains ready. Source
admission counters remain zero. The existing exact retry, unsafe-new-attempt
and leave refusals, wrong-issuer inspection and test-induced source-history
loss assertions remain intact. No retry deadline or health state was overridden.

The first run **failed** after passing those probe assertions, when the issuer
restart reported `diagnostics require the observability build feature`.
Evidence is retained at `/tmp/orishu-formation-failure-iisqv79s`. Inspection
confirmed a compile-time feature check and reuse of the same executable path
for every launch: a replacement build could change restart capabilities even
though the original live process had served diagnostics.

The harness now copies worker and CLI bytes into a private temporary directory
at entry and uses those same copies for every subprocess/restart. It never
hard-links mutable build outputs, and normal context cleanup removes copies.
`ExecutableSnapshotTests` first failed before the helper existed, then passed
its source-replacement/retained-byte and executable-permission assertions.
This prevents mid-journey replacement; intended input builds must still be
selected without a race at initial capture.

The original issuer-loss command then **passed**, including real retry
exhaustion and both history-loss restarts. During its wait, `make test-formation`
rebuilt the shared default-feature output and also passed all 29 helper/evidence
tests and the full ordinary process journey. The two running journeys used
separate private executable paths, directly exercising replacement protection.
The optional build used `observability,formation-fault-test`; the ordinary
build used neither. The unchanged dependency future-incompatibility warning
remains. No debug logs or temporary production probes were added.

This is admission-uncertainty/probe and harness-isolation evidence, not
catch-up completion, secured telemetry or full M4 acceptance. Worker runtime
code and protocol behavior were unchanged. The fault-driven source restart is
not an operator recovery recommendation. Workspace Rust and the remaining
fault/exporter matrices were not rerun for this harness change.

## HTTP readiness across real catch-up — 2026-09-07

`diagnostics::tests::http_readiness_waits_for_real_admission_catchup` passed.
Two real worker runtimes bind peer listeners and admit over pinned QUIC. The
source's production diagnostics router is served over Unix HTTP. After local
initialization it reports live/ready/startup success. Admission adopts the
target formation and inserts the source, but catch-up maintenance is not yet
started: the source cannot introduce and HTTP readiness is false while
liveness and startup remain successful. Starting normal maintenance transfers
and validates the baseline, reaches Joined/introducer-ready, and restores HTTP
readiness. Owners and the HTTP server shut down normally within the whole
15-second fixture budget; no synthetic health-state override is used.

Validation in this dirty worktree:

- `cargo test --locked --offline -p orishu-worker --features observability
  --bin orishu-worker http_readiness_waits_for_real_admission_catchup -- --nocapture`:
  passed, approximately one second.
- `cargo test --locked --offline -p orishu-worker --features observability
  --all-targets --quiet`: passed 163 tests (113 library, 22 binary, 28 executable
  integration), with local socket permission.
- `cargo clippy --locked --offline -p orishu-worker --all-targets --features
  observability -- -D warnings`, `cargo fmt --all -- --check` and
  `make docs-check`: passed (115 Markdown files).

This is real wire/runtime/HTTP evidence in one test process. Deferring
maintenance is not holding a partial transfer on the wire; interrupted,
expired or invalid catch-up HTTP cases remain separate. No production code,
default, dependency or protocol changed. Full workspace/default/fault-process
and remaining telemetry matrices were not rerun for this test-only increment;
the full formation and M4 gates remain open.

## Current process regression requiring resolution

**Current disposition: observation budget corrected; historical failures retained.**
The [admission-baseline audit](#admission-baseline-condition-audit--2026-09-06)
and later reproduction failed the ten-second post-leave readmission assertion.
Passing retries did not establish their precise cause. The
[controlled timing follow-up](#controlled-departure-round-and-readmission-budget--2026-09-07)
now demonstrates that this cutoff was insufficient for an outstanding round
against a departing peer, followed by the next periodic reconciliation.
The process harness uses a seventeen-second observation budget derived from
the unchanged runtime bounds, retaining the exact-record assertions. This
reconciles the acceptance budget; it is not a runtime fix or a claim that the
historical executions had the same internal timing. Final N-FORMATION still
requires the remaining matrix and final process validation.

The [readmission reproducer](#readmission-reproducer-and-diagnostic-reruns--2026-09-06)
provides a shorter public-process loop using the same current assertions and
budget as the full journey. Additional passing reruns did not establish a root cause.
The [bounded reproduction batch](#readmission-failure-reproduced-with-role-evidence--2026-09-07)
has now reproduced the same failure and confirms that the original introducer
is missing the readmitted identity, rather than merely reporting it non-live.

## Current workspace validation — 2026-09-06

After the silent-client idle-deadline change, all task-required default
workspace validation commands were rerun against the working tree and
**passed**:

```sh
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc --quiet
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

The all-target run includes the shared client, membership dependency-purity
tests, worker executable integration tests and CLI tests, not only the worker
library. Worker targets passed 133 tests (100 library, 16 binary, 17 integration).
Criterion targets ran smoke cases, not performance measurements; missing
Gnuplot selected the plotters backend. Documentation validation checked 90
Markdown files. Workspace doc tests passed two examples and explicitly ignored
one existing `query_filter!` example in `crates/orishu/src/model/mod.rs`; that
ignored example is not executable evidence. Clippy still reports the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning, not a denied lint.

The ordinary three-worker CLI journey and default/fault-feature worker suites
were separately rerun in the [idle-deadline follow-up](#silent-client-connection-expiry--2026-09-06).
The workspace commands above do not select optional telemetry, run the separate
fault-process journeys, verify release exclusion of test hooks, or prove the
remaining acceptance matrix complete. Missing cases remain open. This is a
current integration baseline; rerun final validation after subsequent changes.

The final documentation recheck in this turn **failed** after an unrelated,
untracked `docs/tasks/kagami/README.md` appeared. Its eight relative task links
refer to missing files; inspection found only the index in that directory.
The earlier 90-file check passed before that concurrent addition. No Kagami
files were changed by this audit. Thus workspace Rust validation passed, but
the latest repository-wide documentation result is failed, not green. Scoped
whitespace checks for this audit's two edited documentation files passed.

## Initial audit verification — historical checkpoint

These commands were rerun at the initial audit checkpoint and **passed**:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-membership --all-targets --quiet
```

The worker suite passed 110 tests. Membership passed 205 tests, including its
three dependency-purity tests, plus benchmark smoke cases. Benchmark smoke
success is not a performance or scaling measurement.

Process journeys below have recorded passing results in the owning task;
they were **not rerun in this audit**. Their commands remain reproduction
instructions, not new verification. Optional telemetry, full-workspace tests
and missing scenarios were not run. A partial row remains open even when its
supporting unit/wire tests pass.

## Required failure matrix

| Task scenario | Inspected evidence and boundary | Current disposition / missing evidence |
| --- | --- | --- |
| A introduces B; B introduces C | `check_public_adoption` asserts exact identities, certificates, liveness, completed catch-up and introduction through B. `make test-formation` is the ordinary-process reproduction command. | **Passed, recorded process evidence.** Preserve in final acceptance rerun. |
| ACK lost after insertion; operator retries | `make test-formation-lost-ack`, `test-formation-issuer-loss`, `test-formation-source-loss`, `test-formation-dead-assignment`, `test-formation-removed-assignment`, `test-formation-blocked-assignment` cover recovery/stop outcomes. Named cases and limits are recorded in N-FORMATION. | **Partial overall.** Recorded fault journeys pass; audit every preflight-6 unavailable-history/outcome combination before closing recovery, rather than inferring completeness from these cases. |
| Concurrent joins and final capacity slot | `concurrent_allocations_recheck_certificate_capacity_and_lock_before_insertion` exercises core interleavings. `concurrent_authenticated_joins_share_one_local_capacity_slot` releases two real authenticated JoinReq exchanges together through the production dispatcher and owner. | **Passed at core and real wire/owner boundaries.** Exactly one accepted identity/certificate and one `CapacityExhausted` reply, with two records including the introducer. This establishes local capacity, not global slot reservation or multi-process operator-request serialization. |
| Simultaneous member dials; connection loss while both processes live | `simultaneous_admitted_dials_recover_crossed_connections_without_readmission` forces crossed admitted handshakes, two duplicate rejections, automatic repair, then a second disconnect and repair. The existing catch-up lifecycle fixture also covers ordinary transport loss. | **Passed at real runtime/wire boundary.** Five consecutive focused runs and the default worker suite passed in the follow-up below. Two owners share one process; the joiner is admitted but still catching up. Not partition/heal, fleet-scale or arbitrary-topology evidence. |
| Unreachable/stale advertised endpoints | Silent-first fallback/policy exchange, four-real-attempt adapter saturation, missing-route failure tests, and `runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown`. | **Passed at bounded adapter/runtime boundaries.** Runtime-owned maintenance emits to four silent routes, fills the shared dial budget, then leave/generation change or shutdown cancels it without test-owned task abortion. This is not fleet churn, DNS discovery or arbitrary topology evidence. |
| Lock during credential verification | Core interleaving test above; `queued_policy_changes_order_admission_after_authentication` adds real-wire lock-before-join, unlock-before-join and lock-after-acceptance cases. | **Passed at current core and wire/owner boundaries.** Verification and allocation run inline in one owner turn. The test queues control and decoded peer input without yielding, using the existing control-lane priority, not a new asynchronous verifier. Distributed partition policy remains a separate row. |
| Concurrent lock/unlock; temporary partition | Core version/conflict tests, real anti-entropy wire tests, and `check-formation-cli.py --policy-partition`: three ordinary worker processes, opaque UDP relays, independent Unix operator endpoints, conflicting local policies and exact winning-version receipts after heal. | **Passed for bounded complete peer isolation.** Full handoff/leave/crash/restart journey also passes through relays. Does not establish long partitions after Dead retirement, selective asymmetric topology, or global lock fencing. |
| Empty/paginated/changing/truncated admission baseline | The [condition audit](#admission-baseline-condition-audit--2026-09-06) maps empty encoding, ordered pagination, changing-source real wire transfer, retention/receiver faults, atomic merge, readiness/lifecycle fences and public handoff to named assertions. | **Baseline condition mapping complete; bounded tests passed.** The full process rerun's later readmission assertion failed at ten seconds. Its observation budget is now reconciled by the controlled timing evidence; baseline coverage alone did not establish that correction or close final conformance. |
| Old session/timer/verification/send after leave/adoption | `accepted_binding_is_rechecked_after_removal_and_generation_changes`, `queued_policy_command_is_fenced_after_preceding_leave`, `real_owner_runs_periodic_work_and_fences_old_formation_inputs`; runtime catch-up/join reconnect lifecycle tests, including [late successful reconnect after replacement](#late-reconnect-replacement-evidence-and-diagnostic-cleanup--2026-09-07). | **Partial audit.** Pending reconnect now has non-shutdown late-success disposal evidence as well as shutdown coverage; complete the remaining cross-class mapping before closing the row. Process leave/restart success alone is insufficient. |
| Voluntary leave announcement dropped | `make test-formation-lost-departure` requires two suppressed encoded sends and survivor SWIM detection, then same-certificate readmission without clearing restrictions. | **Passed, recorded process evidence.** Separate public-response loss is covered by `make test-formation-lost-leave-response`. |
| Leave followed by same-certificate readmission | The [exact-view follow-up](#bounded-late-observation-and-exact-readmission-views--2026-09-07) checks every worker's ID/certificate/liveness map, including departed history and receipt replay; the [controlled timing fixture](#controlled-departure-round-and-readmission-budget--2026-09-07) establishes why the original ten-second observation budget was insufficient. | **Ordinary process journey passed with the corrected seventeen-second budget.** Historical failures are retained without claiming their precise runtime cause. Final fault-process reruns remain required; the diagnostic-only later snapshot is helper-tested, not captured process failure evidence. |
| Restart with prior certificate; tombstoned self learns ejection | `make test-formation-excluded-restart` proves refusal with retained blocked certificate and successful different-certificate control; `make test-formation-peer-ejection` proves post-adoption wire self-ejection and explicit leave. | **Passed for these recorded process cases.** Fixture sender is not a public removal API and does not prove cluster-wide administrative removal convergence. |
| Datagram reordering/replay and oversized gossip | Core ordering and wire gossip-budget tests; `authenticated_reordered_datagrams_preserve_probe_ids_under_replay`; `authenticated_acks_complete_only_their_outstanding_probe`, using actual owner-scheduled Pings and real QUIC replies. | **Passed at core and authenticated wire/owner boundaries.** Unknown and completed probe IDs cannot consume a current probe or raise incarnation; matching ACKs with lower envelope sequences complete only their named probe. This is direct-probe evidence, not a new indirect-relay or network-loss guarantee. |
| Peer/client flood, slow reads, incomplete frames | Peer limit/pressure tests, real-worker header tests, `stalled_summary_reader_times_out_without_blocking_control`, and `check-formation-cli.py --client-pressure`. | **Scoped pressure scenarios passed; final bounds audit remains.** Peer saturation/reclamation, slow-body recovery, header limits, response-write backpressure and unfinished-client shutdown have evidence. The slow-reader test uses the production Unix server/handlers with transport observation inside one test process; it is not fleet-load or future streaming evidence. HTTP probes belong to the telemetry gate. |
| Wrong/missing operator authority or join/monitor credentials used for mutation | Shared `exercise_mutation_credentials` fixture runs four named tests over Unix/TLS and HTTP/1.1/HTTP/2, each with valid lock/unlock/leave/join requests and eight rejected credential forms. HTTP/2 additionally checks positive raw-wire operations and TLS `h2` ALPN. | **Passed for the recorded four transport/protocol matrices.** Actual monitoring credentials remain a companion observability requirement; they are not fabricated for these tests. This evidence does not close HTTP/2 flow-control or incomplete-header bounds. |

For focused reproduction, use the owning package and exact test filter, e.g.:

```sh
cargo test --locked --offline -p orishu-worker --lib real_handshake_is_decided_by_owner_and_closed_when_formation_changes
cargo test --locked --offline -p orishu-membership --test admission concurrent_allocations_recheck_certificate_capacity_and_lock_before_insertion
```

## Admitted connection collision and recovery — follow-up evidence

On 2026-09-06, the focused test below passed five consecutive runs (about
15 seconds each). The default worker suite passed **111 tests** (91 library,
15 binary, 5 integration). These results supersede the original 110-test count
for the worker only; the membership suite above was not rerun in this follow-up.

```sh
cargo test --locked --offline -p orishu-worker --lib simultaneous_admitted_dials --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
```

The following also passed in this follow-up: the fault-feature suite ran the
same 111 tests. This does not rerun the independent-process fault journeys.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
make docs-check
git diff --check
```

The test pauses only automatic member maintenance while arranging two real
handshakes. Both incoming bindings exist before either outgoing reply reaches
its serialized owner. Both outgoing registrations return `Duplicate`, closing
the crossed transports. Restarted maintenance alone establishes replacement
sessions: a lock update converges, then a second disconnect is followed by an
unlock update. Each round requires a new completed reliable exchange and two
Alive records. Generation, formation, exact member IDs and certificate bindings
remain unchanged; neither owner has tombstones. No replacement join is issued.
The outer fixture completes catch-up and joins both owner shutdown tasks.

Each repair is bounded to 10 seconds for this healthy two-worker loopback
fixture: one-second maintenance cadence, five-second anti-entropy cadence and
scheduling margin. Handshake arrangement and closure have separate five- and
two-second bounds; a 45-second whole-fixture deadline covers setup and cleanup.
These are regression deadlines, not a guarantee for unreachable routes or a
production SLO. Existing dial/session/exchange limit tests remain the resource
cap evidence; this case does not measure sustained overload. The read-only
owner snapshot seam is `cfg(test)`; no production API or protocol changes were
needed.

## Concurrent admission at local capacity — follow-up evidence

On 2026-09-06, `concurrent_authenticated_joins_share_one_local_capacity_slot`
passed five consecutive focused runs, then the default worker suite passed
**112 tests** (92 library, 15 binary, 5 integration). This supersedes the prior
worker count; previous process journeys were not rerun.

```sh
cargo test --locked --offline -p orishu-worker --lib concurrent_authenticated_joins --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
```

Both mutually authenticated application sessions exist before a three-party
barrier releases the two applicant requests. Real QUIC, codec, dispatcher,
owner admission and serialized replies decide the winner; no test mutation
inserts a member. Capacity is two including the introducer. Both replies must
decode: exactly one acceptance and one capacity refusal. The owner's final
model contains only its original ID and the accepted ID with the winning
certificate, unchanged formation and no tombstones. A lock command, owner
shutdown, dispatcher shutdown and connection closure must complete within two
seconds; the entire fixture is bounded to 15 seconds. These are test deadlines,
not an overload SLO. The two applicants use separate certificates and sessions
inside one test process; no cluster-wide capacity guarantee is inferred.

The first two runs failed with a valid capacity redirect: the generic fixture
advertised the first admitted applicant as an introducer at a synthetic IP.
The final fixture explicitly models dial-only, non-introducing applicants with
no advertised peer endpoints. With no redirect candidate, `CapacityExhausted`
is the exact expected response. The production refusal behavior was not
changed or weakened; no temporary instrumentation remains.

The fault-feature worker suite also passed all 112 tests. Scoped Clippy,
workspace formatting, documentation and whitespace checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
git diff --check
```

## Queued lock and admission ordering — follow-up evidence

On 2026-09-06, `queued_policy_changes_order_admission_after_authentication`
passed five consecutive runs. The default worker suite passed **113 tests**
(93 library, 15 binary, 5 integration), superseding the previous worker count.

```sh
cargo test --locked --offline -p orishu-worker --lib queued_policy_changes --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The existing real QUIC admission fixture now has a separately runnable range
of three policy cases. Authentication precedes the policy changes. After the
JoinReq frame arrives, lock/unlock and decoded peer delivery are queued without
yielding; the actual owner processes its available control budget first. Lock
refuses with `MembershipLocked`, unlock permits admission. A third case applies
lock after the accepted owner response but before writing it to the applicant:
the reply remains accepted and its identity is not undone. Assertions cover
policy, generation, formation, member count, accepted certificate binding,
absence of a refused certificate and absence of tombstones. Malformed/session
guard checks and owner shutdown remain part of the fixture. A ten-second outer
deadline bounds all three cases, including cleanup.

This does not promise FIFO ordering across separate mailboxes or that control
always precedes peers under a depleted fairness budget. It verifies the actual
ordering arranged by the fixture, paired with the existing core interleaving
test. No production scheduling or verification behavior changed, and no new
test hook was needed. Process-level partition/heal and full-workspace final
acceptance were not run in this follow-up.

The fault-feature worker suite also passed all 113 tests. The following
additional checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
git diff --check
```

## Independent-process policy partition and heal — follow-up evidence

On 2026-09-06, the direct process journey passed:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/test_formation_udp_relay.py
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
python3 scripts/check-formation-cli.py --policy-partition
```

The repeatable entry point `make test-formation-policy-partition` also passed
as a second complete run in this follow-up. It builds
ordinary binaries, runs 15 harness-helper tests and runs the full journey.
No worker fault feature, packet decryption, firewall privilege or remote fault
API is required. Three advertised loopback UDP relays forward only to their
fixed worker listener. Each relay caps source routes at eight, has no payload
history/offline queue, synchronizes partition changes with forwarding, and
stops its thread within one second. Helper tests establish distinct reverse
routes, bidirectional drops, no replay on heal and refusal at route capacity.

After A admits B and B admits C, ordinary cross-worker lock/unlock converges.
All relays then drop in both directions, interrupting A–B, A–C and B–C; all
Unix client endpoints stay usable. A locks, B locks then unlocks, and C retains
its prior unlocked view. Public receipts prove B's policy counter is newer
than A's; public summaries must show `[locked, unlocked, unlocked]` during
isolation. At least one actual peer datagram must be intercepted within two
seconds. Commands, interception and divergent-view inspection must finish
within three seconds, with unconditional healing in `finally`. This short fault
window targets recovery before retirement, not long-partition recovery.

After healing, bounded public polling requires unlocked views. Authenticated
no-op unlocks with fresh operation IDs inspect each current policy version;
all must equal B's exact winning tuple, not only its boolean. These consume
bounded operation-history entries (at most ten three-worker observations),
not a new read API, and are only issued after unlocked views converge. Exact
IDs/certificates and Alive states must remain unchanged. The same journey then
verifies voluntary leave/readmission and kill/restart/readmission, including
the expected final five-record history. Relays preserve backend source ports
across healing, and treat worker-down ICMP refusal as dropped transport.

The three-second fault limit and existing ten-second convergence polling are
test budgets, not production SLOs. Helper and process evidence does not establish
selective partitions, partitions lasting past Dead retirement, sustained
ingress overload or telemetry supervision. No production code changed.

## Stale routes and bounded fallback — completed checkpoints

Exercise stale advertised endpoint candidates followed by a usable candidate
through the real member dialer, then require restored policy reconciliation.
Measure control/shutdown progress while unreachable attempts consume the
configured dial budget; retain bounded pending-send failure behavior. This is
not new discovery, DNS support or an unbounded offline send queue.

### Silent-candidate fallback checkpoint — 2026-09-06

`admitted_dial_skips_silent_candidate_and_restores_policy_exchange` passed a
focused run in about 15 seconds. The fixture starts two owners with matching
already-admitted records (not a fresh join journey). Its membership-derived
target contains a bound silent UDP endpoint followed by a real QUIC listener.
The first socket must receive a packet, and one dial permit must be held.
A lock command completes within one second while the attempt is pending.
The unchanged five-second per-candidate deadline permits fallback; owner ACK
validation precedes registered traffic. Real reliable exchange and policy
adoption follow, and both owners, dispatcher, pump and connection close within
two seconds after recovery. This is not autonomous maintenance/fleet evidence.

Initial test compilation needed two test-only corrections (a non-Debug result
and an exchange-pool borrow). Three runtime attempts then timed out because
the original seven-second post-fallback deadline omitted an anti-entropy round
started before routing existed. Bounded views showed Alive peers and an open
connection; targeted diagnostics identified the outstanding round and its
eventual abandonment. The final budget is the configured ten-second round
timeout plus the five-second cadence and two seconds of scheduling margin,
inside a 30-second whole-fixture bound. Runtime deadlines were not extended,
and no production fix or SLO improvement is claimed. Temporary diagnostics
were removed. This finding explains why immediate fallback is not proof of
immediate reconciliation.

```sh
cargo test --locked --offline -p orishu-worker --lib admitted_dial_skips --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The saturation checkpoint below now adds four real unreachable attempts,
bounded refusal of a fifth and adapter cancellation. Do not mark the row
complete from the single pending-dial fallback fixture alone.

After the deadline correction, three consecutive focused repeats passed
(10–15 seconds each). Both default and fault-feature worker suites passed
**114 tests** (94 library, 15 binary, 5 integration), superseding earlier worker
counts. Both scoped Clippy configurations, workspace formatting, documentation
and whitespace checks passed. Process journeys and full-workspace tests were
not rerun in this checkpoint.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
git diff --check
```

### Real dial saturation and cancellation checkpoint — 2026-09-06

`real_silent_dials_bound_capacity_and_release_it_on_cancellation` passes through
the production member handshake adapter. Four separate bound silent UDP sockets
must receive QUIC Initial packets, with four live attempt tasks and no available
dial permits. A fifth attempt must return `Overloaded` within 100 ms, not queue.
While the attempts remain pending, an owner lock command succeeds; cancelling
the IO JoinSet restores all four permits and empties the task set, then owner
shutdown completes. That sequence has a one-second bound and the entire fixture
has a five-second bound. These are test budgets, not production SLOs.

The adapter test deliberately owns the JoinSet: an owner handle does not itself
own arbitrary dial futures. It does not claim that runtime-owned maintenance
jobs are automatically cancelled by this test. Targets are secret-free test
snapshots for unreachable members; no TLS/application handshake succeeds and
no membership insertion is asserted. Keep the prior authenticated healthy
fallback and missing-send-route tests alongside this capacity evidence. No
production code or dependency changed.

```sh
cargo test --locked --offline -p orishu-worker --lib real_silent_dials --quiet
```

The subsequent checkpoint covers runtime-owned pending-dial shutdown/generation
cancellation. Arrange enough unreachable admitted routes for real maintenance
to occupy its shared dial budget, then invoke the real lifecycle path and
assert its tasks terminate; do not replace that evidence with this adapter's
test-owned cancellation.

Five consecutive focused repeats passed, followed by both default and
fault-feature worker suites: **115 tests each** (95 library, 15 binary,
5 integration). Both scoped Clippy configurations, workspace formatting,
`make docs-check` and `git diff --check` passed. The commands are the same
worker-suite/feature commands recorded immediately above, plus the focused
test command in this checkpoint. Full-workspace and process-fault journeys
were not rerun; their earlier evidence is unchanged.

### Runtime-owned maintenance cancellation checkpoint — 2026-09-06

`runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown` passed its
focused run in about seven seconds. Each of its two cases arranges a valid
already-admitted five-record fixture before runtime startup, with four bound
silent endpoints and IDs selected by the normal canonical dialing rule. Real
maintenance must emit QUIC traffic to each endpoint within four seconds and
occupy all four shared dial permits. The test does not create dial jobs itself.

The leave case invokes the production runtime leave-operation method, requires
a changed receipt, fresh formation/node IDs, a new generation and a standalone
one-record view. Normal maintenance must release all old-generation permits.
The shutdown case invokes shutdown directly while saturated. Both cases require
the real owner task to finish successfully, all permits to return, and the
maintenance task slot to be empty. Lifecycle work is bounded to two seconds
(covering the one-second maintenance cadence); both cases together have a
16-second fixture limit. No test code takes or aborts the maintenance task.
The only added inspection is a `cfg(test)` read of available dial permits.

Two initial runs failed at credential setup with `UnsafePath`, before any
runtime/dial activity. Using a fresh child directory lets the production loader
create its required private directory; it does not relax path security or
change runtime behavior. No temporary diagnostic instrumentation was added.

```sh
cargo test --locked --offline -p orishu-worker --lib runtime_maintenance_cancels --quiet
```

Three consecutive focused repeats passed, followed by both default and
fault-feature worker suites: **116 tests each** (96 library, 15 binary,
5 integration). Both scoped Clippy configurations, workspace formatting,
`make docs-check` and `git diff --check` passed. Process journeys and full-workspace
tests were not rerun; this is runtime boundary evidence, not a new process run.

## Authenticated datagram reordering and replay — completed checkpoints

Drive reordered and duplicate SWIM datagrams through actual authenticated
worker delivery and assert probe correlation, replay bounds and eventual
liveness. Preserve byte-budgeted gossip and stream/datagram distinctions.
Existing semantic ordering and codec tests remain necessary but do not alone
prove this delivery path. Combined slow-input/ingress overload and optional
telemetry remain open acceptance work.

### Reordered and repeated Ping checkpoint — 2026-09-06

`authenticated_reordered_datagrams_preserve_probe_ids_under_replay` passed a
focused run through a real mutually authenticated connection, member handshake,
production dispatcher and serialized owner. An already-admitted two-member
fixture sends sequence/probe pairs `(100,700)`, `(99,701)`, `(100,700)` and
`(1,703)`; each must receive a datagram ACK with its own probe ID and the
authenticated receiver identity. The repeated Ping is intentionally valid.
An unsolicited ACK with probe/incarnation 999 is followed by a fresh Ping;
the fresh response must progress, no extra ACK appears during the bounded
quiet interval, and membership count, Alive count and held incarnation remain
unchanged. The five-second outer deadline includes owner/dispatcher cleanup.

The initial test incorrectly expected rejection of a repeated envelope sequence
and failed on its correlated ACK. Inspection of the documented sequence contract
and core dispatch confirmed that the 128-entry sequence window rejects replayed
**stream** requests, not SWIM datagrams. Datagram correlation/incarnation checks
are authoritative. The expectation was corrected without changing production
replay semantics. No temporary production instrumentation was needed.

```sh
cargo test --locked --offline -p orishu-worker --lib authenticated_reordered_datagrams --quiet
```

The unsolicited-ACK negative assertion is supporting state evidence, not proof
that a late reply cannot satisfy an outstanding probe. The subsequent checkpoint
adds that explicit pending-probe journey. Do not close the row by
treating datagram sequence rejection as an acceptance requirement.

Five consecutive focused repeats passed. Default and fault-feature worker
suites each passed **117 tests** (97 library, 15 binary, 5 integration).
Both scoped Clippy configurations, workspace formatting, documentation and
whitespace checks passed using the worker validation commands recorded above.
No full-workspace or independent-process journey was rerun in this checkpoint.

### Outstanding-probe ACK correlation checkpoint — 2026-09-06

`authenticated_acks_complete_only_their_outstanding_probe` passed its focused
run. It shares only setup with the previous Ping test: a real mTLS connection,
member handshake and production dispatcher. No test command starts a probe.
The test reads two successive scheduled owner Pings from QUIC and checks the
owner's pending-probe map through its read-only snapshot seam.

For the first probe, an ACK naming an unknown ID and incarnation 999 must
produce a diagnostic without consuming the pending probe or changing the held
incarnation. For the second probe, the same check replays the first completed
probe's ID. In each round a matching ACK uses a lower envelope sequence (9 then
8, after 100), removes exactly that probe and adopts incarnation 1 then 2 with
two Alive members. This distinguishes datagram sequence ordering from probe
identity. The receiver's real codec and session binding process every reply.

Each scheduled Ping has a two-second receive budget. Rejected/accepted ACK
processing has a 200 ms assertion budget, shorter than the configured 500 ms
direct-probe timeout, so timer expiry cannot stand in for successful matching
ACK evidence. The shared five-second whole-fixture bound includes shutdown.
No production behavior, timing or protocol changed.

```sh
cargo test --locked --offline -p orishu-worker --lib authenticated_acks_complete --quiet
```

Five consecutive focused repeats passed (about two seconds each). Default
and fault-feature worker suites each passed **118 tests** (98 library,
15 binary, 5 integration). Both scoped Clippy configurations, workspace
formatting, documentation and whitespace checks passed using the validation
commands recorded above. Full-workspace and independent-process journeys
were not rerun in this checkpoint.

## Next implementation: combined slow-input and ingress pressure

The follow-ups below now provide scoped evidence for peer, header/body and
response pressure plus shutdown. Preserve this historical section/link;
the next action is the complete limits and remaining conformance/recovery
audit, not another implementation of these passing scenarios.

Exercise real authenticated slow/incomplete streams and peer ingress while
measuring bounded owner control/shutdown progress. Name the capacities and
expected refusal/drop behavior; independently green limit tests do not prove
combined supervision. Optional HTTP probes and telemetry-outage cases remain
owned by the companion observability gate, not fabricated health in this test.

### Incomplete streams plus live datagram pressure — 2026-09-06

`incomplete_streams_and_datagram_pressure_preserve_control_and_shutdown` passed
its focused run in about 0.3 seconds. Real mTLS/member handshake and the normal
dispatcher precede the fault. The peer opens 15–16 streams against the configured
16-stream transport limit, accounting for handshake credit. Each sends a valid
4,096-byte length prefix but only one body byte and no FIN. Held worker exchange
permits prove that the receiver admitted those incomplete frames; the next
stream open must remain blocked for 100 ms. Frame deadlines are unchanged.

A concurrent task sends bounded valid Ping datagrams (at most 4,096, one per
millisecond). At least 32 must be submitted and owner transitions must increase
by at least 16 while incomplete streams still hold their permits. Lock and
unlock must complete within one second, and datagram production must continue
through that interval. Shutdown has two seconds to close the connection,
finish the pressure task and restore all 64 worker-wide exchange permits while
the test still retains the incomplete client streams. The entire fixture is
bounded to five seconds. The only new read seam is a `cfg(test)` exchange-permit
count; no production timing, limits or behavior changed.

The first two runs hit the outer deadline. A third, narrowed diagnostic run
located the wait at the sixteenth data-stream open: only fifteen were available
after handshake in that run. The test now measures bounded available credit
rather than assuming handshake credit has already returned. It still requires
at least fifteen held incomplete streams and refusal of the next open. Temporary
stage diagnostics were removed. This is not worker-wide 64-exchange saturation
or client HTTP ingress coverage; those remain the next acceptance work.

```sh
cargo test --locked --offline -p orishu-worker --lib incomplete_streams_and_datagram_pressure --quiet
```

Five consecutive focused repeats passed. Default and fault-feature worker
suites each passed **119 tests** (99 library, 15 binary, 5 integration).
Both scoped Clippy configurations, workspace formatting, documentation and
whitespace checks passed. Full-workspace and independent-process journeys were
not rerun; previously recorded results remain historical at those boundaries.

### Worker-wide exchange saturation and reclamation — 2026-09-06

`multiple_peers_exhaust_global_exchanges_and_recover_after_disconnect` passed
its focused run. Five separately authenticated provisional peer sessions use
the production TLS/application handshake and dispatcher. Four hold thirteen
incomplete length-prefixed streams each; the fifth holds twelve. The fixture
requires zero available worker exchange permits before opening the excess
stream, whose response direction must reset within 500 ms, rather than waiting
for the five-second frame deadline. This is global capacity, not per-session
transport credit or manually held semaphore permits.

An owner lock must succeed within one second at full occupancy. Disconnecting
the first peer must return exactly thirteen permits within one second while
the test still retains its client stream handles. A surviving connection then
opens another incomplete stream and consumes one returned permit, proving
capacity is reusable. Shutdown must join the owner and dispatcher, close all
connections and restore all 64 permits within two seconds. The full fixture
has a five-second deadline. No production code or limit changed.

```sh
cargo test --locked --offline -p orishu-worker --lib multiple_peers_exhaust --quiet
```

This evidence complements the admitted-peer datagram-pressure case; these
five sessions remain provisional and do not claim workload/admission success.
Next exercise the actual client listener with slow/incomplete client requests
while peer work is active, using bounded response/control/shutdown assertions.
Do not close the combined peer/client row from peer-only tests.

Five consecutive focused repeats passed. Default and fault-feature worker
suites each passed **120 tests** (100 library, 15 binary, 5 integration).
Both scoped Clippy configurations, workspace formatting, documentation and
whitespace checks passed using the worker validation commands recorded above.
Full-workspace and independent-process journeys were not rerun here.

### Slow client bodies with live formation traffic — 2026-09-06

The direct `python3 scripts/check-formation-cli.py --client-pressure` journey
passed using ordinary worker binaries. It runs three independent workers with
real QUIC formation traffic and Unix operator listeners. After introducer
handoff, sixteen authenticated POST requests to the first worker's leave route
declare 4,096-byte bodies but supply only one byte. Each must receive a bounded
`100 Continue` response before the next is opened, establishing that body
reading started under its mutation-handler permit. No malformed request can
complete a leave or mutate membership.

An extra authenticated malformed request must receive HTTP 503, not a parse
error, proving full mutation capacity. A lock through another worker must
converge to all three public summaries within two seconds, including the worker
with sixteen blocked body readers; status reads have independent capacity.
The harness completes exactly one malformed body and requires its HTTP 400
response, then performs a successful authenticated unlock through the loaded
worker. These assertions must finish within four seconds of setup, before the
unchanged five-second body timeout, so expiry cannot masquerade as recovered
capacity. Member counts remain three, and the surviving unfinished sockets are
closed by scoped cleanup. Peer unlock convergence and the full ordinary
leave/crash/restart/readmission journey follow.

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
python3 scripts/check-formation-cli.py --client-pressure
```

Both six-test helper suites passed. `make test-formation-client-pressure` also
passed as a second full process run, building ordinary binaries, running helpers
and running this complete scenario. Documentation and whitespace checks passed.
No production feature, wire contract or
deadline changed. Credentials are read from private fixture state, never
printed or included in failure assertions. Every retained socket has bounded
IO waits and scoped cleanup. The mode is exclusive of the other fault modes.

Remaining client-listener conformance must cover incomplete headers, slow
response consumption and shutdown with unfinished clients still present. This
scenario releases held bodies before the later normal shutdown and must not
be cited as evidence for that final case.

### Shutdown with unfinished clients — 2026-09-06

The extended `make test-formation-client-pressure` initially **failed** after
the full handoff/churn journey: worker A exceeded the shared three-second
SIGTERM-to-exit deadline with unfinished clients retained. Redacted failure
evidence is at `/tmp/orishu-formation-failure-13gjigqy` (local diagnostic
artifact, not a required repository fixture).

The new executable integration regression
`shutdown_cancels_unfinished_client_requests` reproduced that timeout in a
single worker. Its first setup attempt failed the private-directory check;
correcting fixture permissions to 0700 exposed the intended shutdown failure.
The test waits for actual `100 Continue` on an authenticated mutation before
supplying one body byte, holds another partial-header socket, sends SIGTERM and
retains both sockets until process exit and socket cleanup. The existing
three-second process deadline is unchanged.

Cause: the signal handler sent a 30-second graceful-stop request while the
owner supervisor sent a one-second request. The pinned server implementation
consumes only the first stop command before draining connections, so the longer
request could win and prevent the intended supervisor deadline taking effect.
The signal handler now requests runtime shutdown only; the owner supervisor
alone stops client listeners. No membership authority, wire format or command
replay semantics changed. No temporary diagnostic logging was added.

After the fix, the focused regression passed, followed by three additional
passing runs (about 1.1 seconds each). The original full
`make test-formation-client-pressure` **passed**, including final shutdown while
the test retains its sockets, EOF/reset checks and cleanup. This supersedes
the prior checkpoint's shutdown gap only; it does not prove header capacity or
expiry, or actual response backpressure. The shared helper suite now passes
nine HTTP tests, including incomplete-body admission and refusal/EOF negatives;
the six evidence-helper tests also pass.

```sh
cargo test --locked --offline -p orishu-worker --test standalone shutdown_cancels_unfinished_client_requests --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
make test-formation-client-pressure
```

Both worker configurations passed **121 tests** (100 library, 15 binary,
6 integration). Scoped Clippy with warnings denied passed in both configurations;
workspace formatting, `make docs-check` (90 Markdown files) and
`git diff --check` passed. Full-workspace Rust and optional observability acceptance were
not run. The existing `proc-macro-error2` future-incompatibility build warning
remains unrelated to this fix.

### Pre-authentication connection and header limits — 2026-09-06

The production client-server constructor now applies fixed formation-PoC
limits before route authentication: 64 accepted connections per listener,
five-second HTTP/1 head deadline, 8 KiB HTTP/1 buffer and 32 headers. Handler
limits remain separate. Saturation pauses acceptance at the OS backlog; it
does not promise an HTTP response to an unaccepted connection or reserve a new
operator connection. No dependencies or configuration keys were added.

`incomplete_headers_bound_connections_and_expire` first **failed** because a
65th connection received a response while 64 sockets were held. After applying
the limits it **passed** in about 5.2 seconds. Each held socket first completes
a real summary response, proving acceptance rather than backlog occupancy.
Sixty-three then hold incomplete request heads; the remaining established
connection still serves status. The extra connection receives no response for
100 ms at capacity. Dropping one held socket admits that waiting request,
with status and reuse together bounded to one second and setup/reuse below
three seconds (before expiry). Retained headers close/reset within a shared
seven-second observation budget, then a fresh summary and shutdown succeed.
The fixture has a 15-second outer bound.

`oversized_and_excessive_headers_are_rejected_before_handlers` **passed**:
a 9,000-byte header value and 33 additional headers receive actual HTTP 431
responses within two seconds each, followed by a successful summary. The first
run failed because `read_to_end` encountered reset after early rejection;
diagnosis confirmed the HTTP 431 bytes precede the reset. The corrected reader
retains those bytes and still requires HTTP 431. Bare EOF/reset cannot pass.
No production change or debug logging was needed for this fixture correction.

The extended `make test-formation-client-pressure` **passed** the complete
handoff/churn/shutdown journey. A new phase holds sixty proven-accepted
incomplete heads at A, leaving four listener slots for public reads. A lock
through B converges to all three summaries within two seconds, with setup and
assertions below four seconds so header expiry cannot explain progress. This
complements the exact-capacity executable test; it does not claim unlimited
new-client progress at full capacity. Nine HTTP-helper and six evidence-helper
tests passed in that command.

```sh
cargo test --locked --offline -p orishu-worker --test standalone incomplete_headers_bound_connections_and_expire --quiet
cargo test --locked --offline -p orishu-worker --test standalone oversized_and_excessive_headers --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
make test-formation-client-pressure
```

Both worker suites **passed 123 tests** (100 library, 15 binary, 8 integration).
Default and fault-feature scoped Clippy with warnings denied, workspace
formatting, `make docs-check` (90 Markdown files) and `git diff --check` passed.
Full-workspace and optional observability acceptance were not run. Slow
response-reader backpressure is still required: prove a requested write
actually stalls, bounded response work/buffering, unrelated control progress
and server-side reclamation. Header expiry or a small response fitting the
socket buffer cannot substitute for that evidence.

### Stalled response writes — 2026-09-06

`stalled_summary_reader_times_out_without_blocking_control` initially **failed**:
a proven pending Unix transport write remained alive through the seven-second
reclamation cutoff. The production server now sets a five-second write-stall
timeout. The same test then **passed**, including stronger authenticated-control
and pipeline-work assertions. This is not a total transfer timeout and does
not require a progressing reader to finish within five seconds.

The test runs the production client-server constructor, summary/lock handlers,
CBOR writer and real membership owner on a real Unix socket. It submits exactly
2,048 complete summary requests and reads none of their responses. This finite
pipeline makes the ordinary bounded responses fill the actual socket buffers;
it does not invent a large status record or claim that a small buffered reply
alone establishes backpressure. A test-only wrapper forwards scalar and vectored
IO unchanged while recording `Pending`, written byte counts, `TimedOut` and
transport drop. A counting hook observes dispatch without changing responses.

The fixture requires a real pending write within two seconds. An independent
authenticated client must read status and receive an accepted lock receipt
within one second. Dispatch count and written bytes then remain unchanged for
100 ms while the slow client is still unread; fewer than 2,048 responses were
constructed and fewer than 1 MiB written. Reclamation must occur within seven
seconds, with an observed write timeout, without client reads, client close or
server shutdown causing it. Only afterward does the fixture drain bounded
HTTP 200 bytes, accepting reset after buffered responses, and shut down the
owner/server. The whole test has a 12-second deadline. Its JoinSet cancels the
server on fixture failure; no test observer or fault route enters normal builds.

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker stalled_summary_reader --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

Both worker suites **passed 124 tests** (100 library, 16 binary, 8 integration).
`make test-formation-client-pressure` **passed** after enabling the write-stall
deadline, including all fifteen HTTP/evidence helper tests and the full
three-process handoff/pressure/churn/shutdown journey. Default and fault-feature
scoped Clippy, workspace formatting, documentation (90 Markdown files) and
whitespace checks passed.
This is bounded handler/transport evidence, not a measured peak-memory result,
minimum-throughput SLO or large future-streaming-response test. Preserve the
earlier process peer-pressure scenarios and audit the complete limits matrix
before closing formation conformance. Full-workspace and optional observability
acceptance were not run.

### Mutation authority matrix and client error status — 2026-09-06

`every_mutation_rejects_wrong_credential_classes_before_state_change` uses two
ordinary worker processes. The target exposes real privileged join material;
the source receives valid, encoded version-1 lock, unlock, leave and join
requests. Each request is exercised with eight credential forms:

| Credential form | Required result for all four intents |
| --- | --- |
| Missing header | HTTP 401 |
| Wrong 64-character bearer | HTTP 401 |
| Other worker's actual operator bearer | HTTP 401 |
| Target formation's actual join bearer | HTTP 401 |
| Duplicate correct local operator headers | HTTP 401 |
| Correct operator then join bearer | HTTP 401 |
| Join bearer then correct operator | HTTP 401 |
| Correct token with Basic scheme | HTTP 401 |

All 32 exchanges require actual bounded HTTP 401 responses (not transport
failure), with no local/operator/target join credential bytes in responses.
After each, public status retains exact source formation/node identity,
standalone participation, one member and unlocked policy. Target membership
remains unchanged. An authenticated join-status lookup returns HTTP 404 with
no retained operation. Correct local authority subsequently uses the rejected
lock/unlock/leave IDs successfully; standalone leave is explicitly a no-op.
Individual raw exchanges have two-second deadlines and the whole fixture has
15 seconds, with process reaping on failure.

The matrix first passed its rejection checks, but the added no-history check
exposed a client decoder bug. Initial fixture expectations tried the empty-body
404 variant, then HTTP 404 `ApiError`; both failed because the CBOR error was
decoded as a successful raw envelope and later converted to `ApiError` with
status zero. The temporary secret-free status probe confirmed `UnknownOperation`
with status zero, not an accepted join. It has been removed. The shared response
decoder now preserves non-success HTTP status and refuses success envelopes on
non-success responses. GET/DELETE error fixtures now check 404/403 status, and
a new fixture checks a false success envelope under HTTP 503.

```sh
cargo test --locked --offline -p orishu-worker --test standalone every_mutation_rejects --quiet
cargo test --locked --offline -p orishu -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

After the fix, the client passed **95 tests** and each worker configuration
passed **125 tests** (100 library, 16 binary, 9 integration). The earlier worker
suite failures were the same no-history assertion before the client fix.
`make test-formation` also passed: ordinary binary build, nine HTTP and six
evidence-helper tests, and the complete public handoff/leave/crash/restart/
readmission journey. Scoped Clippy passed for both default and fault-feature
configurations; workspace formatting, documentation and whitespace checks
passed. Full-workspace Rust and optional-observability acceptance were not run.
No wire schema changed; Rust raw-caller behavior now returns non-success CBOR
envelopes as errors with real status rather than `Ok(ApiResponse::Error)`.

**Remaining audit:** actual supported transports include TLS/TCP and HTTP/2,
not only Unix HTTP/1. Existing HTTPS inspection tests do not prove the complete
mutation credential matrix or HTTP/2 stream/header/flow-control limits. Map and
test those requirements next. Monitoring-only credentials must still be tested
when P-OBSERVABILITY implements them. This checkpoint does not close that gate
or the other recovery/baseline/stale-completion rows.

### TLS mutation credential matrix — 2026-09-06

`tls_mutations_reject_wrong_credential_classes_before_state_change` **passed**
in about 1.2 seconds. It shares the preceding eight-credential/four-intent
matrix with the Unix regression. Two ordinary worker processes provide actual
target join material and distinct operator credentials. The source exposes
both its Unix inspection socket and an explicitly configured TLS/TCP listener;
all 32 negative mutation exchanges use the TLS listener.

The raw client trusts only the generated server certificate, validates the IP
server name, offers only `http/1.1` and requires that ALPN selection. Every
exchange still requires actual HTTP 401 bytes and bounded responses without
credential disclosure. EOF without TLS close-notify or reset is tolerated only
after those rejection assertions can be satisfied, never as authorization
evidence by itself. Socket connect/read/write waits are one second, each
exchange has the existing two-second outer bound and the whole fixture retains
its 15-second deadline. Blocking TLS IO runs off the async runtime thread.

After each rejection the source's public Unix summary retains exact identity,
standalone participation, one member and unlocked policy. The authenticated
production client uses TLS for readiness, the absent join-operation lookup
(404), lock, unlock and the no-op standalone leave receipt. Target membership
remains unchanged. The raw negatives prove HTTP/1.1; the normal client's
negotiation is not claimed as explicit HTTP/2 evidence. No production code,
wire shape, dependencies or deadlines changed in this increment.

```sh
cargo test --locked --offline -p orishu-worker --test standalone tls_mutations_reject --quiet
```

Default and `formation-fault-test` all-target worker suites both **passed 126
tests** (100 library, 16 binary, 10 integration). Scoped Clippy with warnings
denied passed in both configurations; workspace formatting, documentation
(90 Markdown files) and whitespace checks passed. Full-workspace Rust,
the separate CLI churn harness and observability acceptance were not rerun
for this test/documentation-only increment.

HTTP/2 credential handling and explicit stream/header/flow-control bounds are
the next transport audit. The earlier TLS inspection tests and these TLS
HTTP/1.1 mutations do not close that work. Monitoring authority remains gated
on its actual implementation.

### Explicit HTTP/2 bounds and stream/header enforcement — 2026-09-06

The production server now explicitly selects 16 concurrent streams per HTTP/2
connection, 8 KiB decoded header lists, 4 KiB HPACK tables, 16 KiB frames,
65,535-byte initial stream/connection receive windows with adaptive growth
disabled, and a 16 KiB per-stream send-buffer setting. These are additional to
the existing connection and handler caps, not replacement global capacities.

`http2_stream_capacity_is_advertised_enforced_and_reclaimed` starts an ordinary
worker process and sends the real HTTP/2 preface, SETTINGS and bounded HPACK
header blocks over its Unix client listener. It first **failed** because the
server advertised the library default of 200 streams. With explicit settings,
the test verifies advertised stream/header limits and frame/stream-window
values, then holds sixteen authenticated POST streams without body completion.
The seventeenth must receive `RST_STREAM(REFUSED_STREAM)`. Independent HTTP/1
status remains available. Cancelling stream 1, then receiving a PING ACK, allows
a replacement stream whose completed malformed body must reach the production
decoder and return CBOR `InvalidRequest`, not overload or authorization failure.

A further validly encoded HEADERS frame carries a 9,000-byte value: it fits the
frame limit but exceeds the decoded header-list limit. The test requires a
terminal HTTP 431 HEADERS frame, checking its small HPACK golden encoding.
A subsequent valid summary stream on that same connection returns the real
one-member, unlocked state. These assertions finish within three seconds,
before body expiry can explain capacity reuse. Shutdown retains fifteen
unfinished streams until the process exits under the existing three-second
deadline; the complete fixture has a ten-second bound.

Two fixture assumptions were corrected through diagnosis: this HTTP/2 path
does not emit the HTTP/1-style `100 Continue` signal, so waiting for it instead
observed the five-second `OutcomeUnknown` response; and oversized headers can
end with HTTP 431 rather than a reset. Early focused runs and both suites failed
on those assumptions. The corrected test requires actual protocol responses,
not sleeps or transport closes as evidence. Temporary bounded, secret-free
frame diagnostics were removed. No dependency or wire schema was added.

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_stream_capacity --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

Both final worker suites **passed 127 tests** (100 library, 16 binary,
11 integration). `make test-formation` passed the full public journey plus
nine HTTP and six evidence-helper tests. Both scoped Clippy configurations,
workspace formatting and documentation checks passed. Whitespace checking
passed for this increment's files; repository-wide `git diff --check` reported
unrelated trailing whitespace in concurrently changed `TODO.md` lines 228 and
239, which were left untouched. The existing `proc-macro-error2`
future-incompatibility warning remains.

This is Unix HTTP/2 stream/header evidence, not TLS ALPN or
the complete credential matrix. Receive-window and send-buffer values are
configured; this test does not establish their full adversarial enforcement.
Still test stream/connection flow-control exhaustion, withheld response window
credit, incomplete HEADERS/CONTINUATION progress and HTTP/2 credential classes.
In particular, withholding window credit may not cause a pending socket write,
so existing transport write-stall evidence cannot close stream send liveness.
Full-workspace Rust and observability acceptance remain unverified.

### HTTP/2 credential matrices and positive controls — 2026-09-06

`http2_mutations_reject_wrong_credential_classes_before_state_change` and
`tls_http2_mutations_reject_wrong_credential_classes_before_state_change`
**passed**, first with the shared negative matrix, then with added positive
controls (about 2.9 seconds for both tests together). Each starts two ordinary
worker processes and repeats the prior eight credential forms against all
four valid mutation intents. The TLS variant validates the generated server
certificate and IP identity and requires negotiated `h2`; the Unix variant
uses the actual HTTP/2 preface on the Unix listener.

Every negative exchange uses a fresh HPACK context, literal non-indexed
credential fields, real HEADERS/DATA frames and a finite reader. It requires
the HTTP 401 status header's golden HPACK encoding plus a decoded CBOR
`Unauthorized` envelope, bounded to 8 KiB, with no credential bytes in the body.
The reader accepts at most 32 frames, bounds each to 16 KiB and acknowledges
server SETTINGS. Socket read/write waits are one second and each negative
exchange has a two-second outer deadline. Existing identity/state/history and
correct-authority checks remain shared with the HTTP/1.1 matrices.

Positive raw HTTP/2 requests with fresh operation IDs then lock and unlock the
source, verified through public state, and obtain the no-op standalone leave
receipt. Finally the formerly rejected join request receives HTTP 200/202 and
its matching identified connecting/admitting/catching-up/joined outcome.
This proves authorized submission, **not** completed catch-up. The fixture
shuts down both workers under its existing bounded cleanup; successful join
completion remains covered by the separate formation journey. The complete
matrix fixture retains its 15-second deadline. No production code, dependencies
or protocol schema changed.

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_mutations_reject --quiet
```

Default and `formation-fault-test` all-target worker suites both **passed 129
tests** (100 library, 16 binary, 13 integration). Scoped Clippy with warnings
denied passed in both configurations. Workspace formatting, documentation
(90 Markdown files) and whitespace checks for this increment's files passed.
Full-workspace Rust, the separate CLI churn harness and optional observability
acceptance were not rerun for this test/documentation-only increment.

Across all four named fixtures, there are 128 negative intent/credential
exchanges, with each TLS fixture requiring its intended ALPN protocol. This
closes the scoped Unix/TLS, HTTP/1.1/HTTP/2 mutation credential mapping. It does
not substitute for actual monitoring-only credentials, flow-control exhaustion,
response-window-credit stalls, incomplete HEADERS/CONTINUATION handling or
the remaining recovery and lifecycle audits.

### HTTP/2 response credit exhaustion and recovery — 2026-09-06

The existing working-tree tests were inspected and rerun through real worker
processes and the production Unix HTTP/2 listener:

- `http2_zero_response_window_isolated_and_recovers` sets initial response
  stream credit to zero, retains sixteen responses and requires refusal of a
  seventeenth. Independent authenticated summary/lock requests complete within
  one second. Cancelling one stream admits a replacement; returning credit to
  one old stream and the replacement releases only their DATA. Decoded summaries
  retain the exact formation/source identity and show the lock state captured
  before and after the mutation. Shutdown finishes with fourteen responses
  still window-blocked, before the test closes its connection.
- `http2_connection_response_credit_is_enforced_and_recovers` issues at most
  512 bounded summary requests without connection window updates. It counts
  actual DATA bytes until the full 65,535-byte connection credit is exhausted;
  each stream retains its separate initial credit. A subsequent response head
  and PING acknowledgement progress without DATA bypassing connection credit.
  Independent inspection remains available. A connection window update resumes
  the pending bodies, which decode to the original formation/source identity.

Both fixtures have ten-second whole-journey deadlines and bounded response
buffers; worker shutdown has its existing three-second deadline. These are
regression bounds, not production SLOs or measured memory/throughput claims.
No production behavior, dependencies or wire schema changed in this follow-up.

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_ --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
```

All commands **passed**: five focused HTTP/2 tests; 131 tests in each worker
configuration (100 library, 16 binary, 15 integration); both scoped Clippy runs.
These results supersede the previous worker count, not its scoped limitations.
The separate CLI churn/fault journeys, full-workspace Rust tests and optional
observability acceptance were not rerun.

This closes the response-credit exhaustion and explicit recovery cases only.
It does not establish timeout-driven reclamation when credit is never returned,
inbound flow-control violation handling, incomplete HEADERS/CONTINUATION bounds,
TLS-specific pressure behavior, or a combined peer-flood guarantee. Preserve
these distinctions in the remaining ingress audit; the prior socket-write
timeout test does not prove a response-window timeout.

### HTTP/2 continuation count and binding — 2026-09-06

`http2_continuation_budget_and_stream_binding_are_enforced` adds a real-worker
Unix HTTP/2 regression against the locked `h2` 0.4.19 decoder and current worker
settings. Three fresh connections exercise:

- A HEADERS block without END_HEADERS, five empty non-final CONTINUATION
  frames, then a final sixth continuation: the real summary response decodes
  with the original formation/source identity.
- The same sequence with a non-final sixth continuation: actual GOAWAY with
  `ENHANCE_YOUR_CALM` (11), with no handler response accepted by the test.
- A continuation naming a different stream: actual GOAWAY with
  `PROTOCOL_ERROR` (1), with no handler response accepted by the test.

Each outcome has a one-second deadline and sixteen-frame read budget. Fresh
public inspection after each case confirms the worker remains usable with the
same formation. The entire fixture, including cleanup, has a ten-second bound.
The focused command **passed**:

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_continuation --quiet
```

This is decoder-work/binding evidence, not a header assembly timeout,
partial-frame timeout, proof of process-wide saturation or TLS pressure test.
No production behavior or dependency changed. The remaining ingress audit
must still establish the missing temporal bounds independently.

Default and `formation-fault-test` worker all-target suites both **passed 132
tests** (100 library, 16 binary, 16 integration), using the same commands as
the response-credit follow-up above. Both scoped Clippy configurations passed
with warnings denied. Workspace formatting, `make docs-check` (90 Markdown
files) and scoped whitespace checks passed. Full-workspace Rust, separate CLI
process journeys and optional observability acceptance were not rerun.

### Silent client connection expiry — 2026-09-06

The shared production client server now configures a ten-second transport
inactivity deadline for Unix and TLS HTTP/1.1/HTTP/2, in addition to the
existing five-second HTTP/1 header and pending-write deadlines. A successful
transport read or write resets inactivity. Previously the idle fuse was unset;
neither HTTP/1 header timing nor a pending-write timer covered a silent HTTP/2
header block or response waiting for window credit.

`http2_silent_partial_headers_frames_and_responses_expire` retains three real
Unix HTTP/2 connections: HEADERS without END_HEADERS, a frame with one of its
two declared payload bytes missing, and a completed GET whose response HEADERS
arrive with zero DATA credit. Each stays open through an initial 100 ms
observation, excluding immediate malformed-input rejection as a passing result.
Without sending missing bytes, credit or cancellation, the fixture requires
EOF/reset for all three under one shared twelve-second deadline. Reads and
terminal bytes are bounded. The worker remains running, with public formation
inspection before and after expiry; shutdown happens only afterward. The
whole fixture has an eighteen-second deadline.

The initial focused run passed; the strengthened early-open regression also
**passed**, as did all-target worker suites in default and fault-feature builds:
**133 tests each** (100 library, 16 binary, 17 integration).

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_silent --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

Compatibility: idle client connections now close; callers must reconnect and
use operation/status replay when a mutation receipt is uncertain. The deadline
does not change accepted domain outcomes or peer QUIC sessions. No dependency
or wire schema changed. The HTTP/2 expiry fixture uses Unix, not TLS, and does
not prove full connection-capacity reclamation under saturation.

This closes silent-connection timing only. PINGs, other streams or trickled
bytes can maintain connection activity. Absolute header assembly/partial-frame
deadlines and per-stream withheld-credit timing still require their own
contract and evidence; do not relabel this inactivity timer as those bounds.

`make test-formation` **passed** its full ordinary CLI handoff, leave,
crash/restart and readmission journey, plus six evidence-helper and nine
HTTP-helper tests. Both scoped worker Clippy configurations passed with
warnings denied; workspace formatting, documentation (90 Markdown files) and
scoped whitespace checks passed. The build still reports the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning. Separate fault
journeys, full-workspace Rust and optional observability acceptance were not
rerun in this follow-up.

### Stale-IO completion inventory — 2026-09-06

Inspection of `driver::Control`, `PeerDelivery`, the owner loop and effect
interpreter distinguishes actual asynchronous completions from inline work.
This inventory narrows the remaining stale-IO audit; source inspection alone
does not upgrade the entire failure-matrix row to passed.

| Completion class | Production fence and evidence | Remaining scope |
| --- | --- | --- |
| Decoded peer packet / authenticated handshake | `Owner::peer` and the session registry recheck generation and binding; `real_handshake_is_decided_by_owner_and_closed_when_formation_changes`, plus registry binding/ejection tests | Preserve actual wire coverage; not a claim about every fault combination |
| Initial join preparation | `Control::JoinPrepared` checks generation, source formation and standalone participation; new `stale_join_preparation_cancellation_preserves_new_operation` tests the real reserved cancellation after replacement | Still stage a successful old handshake completion across lifecycle replacement; cancellation alone does not prove connection disposal |
| Pending-join reconnect | `JoinReconnectFinished` checks generation, joining state, previous session, reconnect ID and active attempt; runtime cancellation and `shutdown_fences_successful_join_reconnect_handshake` tests | Map non-shutdown late-success cases separately before closing this class |
| Admission baseline/credential transfer | `CatchupFinished` checks generation, attempt ID, participation, total deadline and current source session before adoption; named leave/ejection/shutdown/session-loss/deadline regressions in the catch-up record | Existing successful-result barriers cover these lifecycle cases; retain their real wire/runtime boundary limitations |
| Reliable exchange reply / failure | Owner send tasks capture generation/session; successful replies pass through `Owner::peer`, stale errors cannot update the new generation's failure counter; lifecycle change aborts outstanding sends | Need an explicit held-success/held-error completion mapping, not just source inspection or missing-route tests |
| Timer expiry | Timer map belongs to the owner and is cleared on identity change/ejection; expired tokens are applied synchronously, with stale-token core regressions in `swim.rs` and `join.rs` | Preserve token tests and map pending-timer lifecycle coverage at the owner boundary |
| Credential verification, ID allocation and peer selection | Production effect interpreter computes these inline and queues typed outcomes in the same bounded owner turn; identity replacement clears its pending inline queue | No asynchronous production job exists to complete after leave. Queued-policy and local-capacity wire tests exercise actual owner ordering; do not invent a verifier task to test a nonexistent race |

The optional `reserve_verification` API has reservation/cancellation tests but
is not called by production credential verification. It cannot stand in for
the actual inline verification path. Shutdown owns and drops the serialized
owner and its work; admission/catch-up completion tests must still distinguish
late delivery from orderly cancellation.

The new three-second owner regression retains an initial `JoinPreparation`,
changes lifecycle through the real owner command path, starts a preparation
with a new operation ID and formation, then drops the old preparation. An
ordered status request proves the old reserved completion was handled. The
new operation remains `Connecting`, the old operation stays
`FailedBeforeAdmission`, and replacement formation/node/generation are
unchanged. Dropping the new preparation then produces its own expected failure,
and owner shutdown completes normally. The focused command **passed**:

```sh
cargo test --locked --offline -p orishu-worker --lib stale_join_preparation --quiet
```

This is production owner/completion-path evidence inside one test process,
not a real network handshake or a public operator leave-during-join guarantee.
The test deliberately stages the lifecycle transition below the public API's
busy-operation guard. No production semantics or dependency changed.

Default and `formation-fault-test` all-target worker suites both **passed 134
tests** (101 library, 16 binary, 17 integration). Scoped Clippy passed with
warnings denied in both configurations; workspace formatting and scoped
whitespace checks passed. Full-workspace Rust, separate process journeys and
optional observability were not rerun for this owner-test/documentation change.
The documentation check still fails on missing links in concurrently authored
Kagami tasks; those unrelated files were not modified by this increment.

### Late successful initial handshake and shutdown disposal — 2026-09-06

`late_successful_initial_handshake_is_closed_after_leave_or_shutdown` stages
real pinned QUIC/TLS handshake IO against a live worker dispatcher, then holds
the resulting `PendingHandshake` before its original reserved completion.
It exercises source lifecycle replacement and completed source shutdown
separately. After late delivery, the connection must report `LocallyClosed`
within one second and its endpoint must drain within three seconds, without
the test closing that endpoint. The target remains a cluster of one. In the
leave case the source retains its replacement formation/node, standalone
participation and `FailedBeforeAdmission` operation. The whole two-case
fixture has a fifteen-second bound and joins both runtime shutdown tasks.

The initial focused run and both worker suites **failed** this new test.
Tagged connection observation isolated the failure to shutdown: leave closed
promptly, but the post-shutdown connection remained open. It was not merely
QUIC drain latency. Tokio's owned mailbox permit can publish after receiver
destruction; the queued resource-bearing value then survives while another
sender keeps the channel allocation alive. Successful transport IO had no
remaining owner to dispose of its connection.

`send_reserved_control` now retains a shared disposal handle through reserved
publication. If the returned sender is closed, it takes and drops the value;
otherwise the owner consumes it or receiver destruction drops it. The slot is
never released to race a fresh admission, and only the small completion value
is shared—not membership authority. Join preparation, pending-join reconnect
and catch-up use this path. The bounded allocation occurs per shell completion,
not per packet, timer or simulation step. No network protocol or dependency
changed. The connection observer exists only under `cfg(test)`.

The corrected real-handshake test **passed**. A second regression,
`reserved_control_disposes_payload_on_either_side_of_receiver_drop`, tests
both publication orderings while deliberately retaining a sender. Existing
owner cancellation/replay tests remain the live-delivery controls. Temporary
`DEBUG-initial-close` instrumentation was removed.

```sh
cargo test --locked --offline -p orishu-worker --lib late_successful_initial_handshake --quiet
```

This extends the initial-preparation inventory row with successful leave and
shutdown disposal evidence. It uses real wire IO and runtime owners inside
one test process; lifecycle replacement is staged below the public API's busy
guard. It does not prove public leave-during-join support or close the separate
reliable-send and timer completion audit gaps.

Both corrected worker all-target suites **passed 136 tests** (103 library,
16 binary, 17 integration), in default and `formation-fault-test`
configurations. Scoped Clippy passed with warnings denied in both builds.
`make docs-check` now passes 100 Markdown files after the concurrent Kagami
task files arrived. The subsequent workspace formatting invocation could not
load a newly added, unrelated `crates/kagami-document` manifest with no target
yet; direct Rustfmt checks for the three changed worker source files and scoped
whitespace checks passed. No files in that concurrent crate were modified by
this increment. Full-workspace Rust and optional observability acceptance were
not rerun after this fix.

`make test-formation` also **passed** the full ordinary CLI handoff,
leave/crash/restart/readmission journey and its six evidence-helper plus nine
HTTP-helper tests. The build reports the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning. Separate development fault-process journeys
were not rerun here; a passing fault-feature unit suite is not their substitute.

### Process regression reruns after completion disposal — 2026-09-06

All three commands **passed** against rebuilt workers after the reserved
completion fix. Each ran its full ordinary handoff/leave/crash/restart/
readmission journey, not an admission-only diagnostic:

| Command | Verified fault/pressure assertion |
| --- | --- |
| `make test-formation-lost-ack` | Development-only post-insertion ACK loss recovers the original assigned ID; the fault marker, source operation/recovery reference and issuer inspection agree, without duplicate live membership |
| `make test-formation-client-pressure` | Slow authenticated bodies saturate mutation capacity; reads/peer policy progress, released capacity restores control, unfinished headers retain peer progress, and all three processes shut down within the shared three-second budget while incomplete client sockets remain held |
| `make test-formation-policy-partition` | Opaque peer isolation preserves independent operator access; conflicting policies converge after heal to the exact winning version with unchanged live identities, followed by full churn |

The pressure target passed nine HTTP-helper and six evidence-helper tests.
The partition target additionally passed three UDP-relay tests. The lost-ACK
target rebuilt its explicit `formation-fault-test` binaries in
`target/formation-faults`; the other two targets used ordinary binaries.
Builds report the existing `proc-macro-error2 v2.0.1` future-incompatibility
warning. Workspace formatting and documentation checks (101 Markdown files)
also passed after the concurrent metadata/documentation gaps were resolved.

These are scoped reruns, not closure of every interrupted-admission outcome,
long/asymmetric partition, TLS/HTTP2 pressure combination, release-feature
exclusion or optional observability gate. Preserve the fault durations and
pre-expiry assertions in the harness; successful retries do not erase failures
from earlier checkpoints.

Post-fix workspace validation also **passed**:

```sh
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc --quiet
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Worker targets passed 136 tests. Workspace doc tests passed three examples and
retained the one explicitly ignored `query_filter!` example. Benchmark smoke
targets used the plotters fallback; this is not a performance measurement.
The newly completed concurrent Kagami document crate was included by workspace
discovery, but no Kagami implementation was changed or reviewed in this audit.
Scoped whitespace checks for the edited task documentation passed. This
supersedes the earlier post-fix metadata/documentation blockers, not the open
formation acceptance rows or optional telemetry requirements.

### Reliable-send result consumption across generations — 2026-09-06

The owner loop's existing successful/error result handling is now factored
into `Owner::send_finished`, used directly by the production send-task join
branch. No result semantics, scheduling, transport or wire contract changed.
A `cfg(test)` owner command invokes that same method and returns its actual
view after processing, avoiding a stale published snapshot as the assertion.

`old_send_results_cannot_change_replacement_formation_or_counters` drives a
real serialized owner through lifecycle replacement, then presents an old
generation's successful byte result and error result. Replacement generation,
formation/node, standalone participation, completed-exchange count, failed-send
count and diagnostics remain unchanged. The successful payload is deliberately
malformed: generation rejection must precede decoding. A current-generation
error increments the failure count as a positive control. The owner shuts down
cleanly under the fixture's three-second total deadline.

```sh
cargo test --locked --offline -p orishu-worker --lib old_send_results --quiet
```

The focused regression **passed**. Its first two invocation attempts could not
load a concurrently created `kagami-session` manifest without a target; those
were workspace-resolution failures, not failing assertions. No concurrent
Kagami files were modified. Resolution subsequently recovered.

This supplies owner-consumer generation-fencing evidence for both completed
result classes. It does not execute a real exchange, create a valid same-session
wire reply, or prove cancellation of an in-flight send future. Keep those
transport/binding and cancellation guarantees separate in the stale-IO audit;
the whole matrix row remains open pending its remaining mappings.

Default and `formation-fault-test` worker all-target suites **passed 137 tests
each** (104 library, 16 binary, 17 integration). Both scoped Clippy runs passed
with warnings denied. Direct Rustfmt for `driver.rs`, documentation checks
(101 Markdown files) and scoped whitespace checks passed. Workspace formatting
reported differences in concurrently edited `kagami-session` files; those
files were not reformatted by this increment. Separate process journeys,
full-workspace tests and optional telemetry were not rerun after this refactor.

### Owner timer cleanup on leave — 2026-09-06

`leave_clears_real_owner_deadlines_and_fences_late_timer_input` starts the real
serialized owner with known peers and requests a probe round. A read-only
`cfg(test)` snapshot observes the actual timer map and captures a token whose
deadline is still pending. The test does not insert a synthetic timer into
that map. A subsequent leave and same-control-lane snapshot prove that the
replacement lifecycle has no retained deadlines.

Delivering the captured token with its original generation then increments
the stale-input counter exactly once. Replacement formation/node identity,
standalone one-member state, diagnostics and failed-send count remain
unchanged; the timer map remains empty and owner shutdown succeeds. The entire
fixture has a three-second deadline. The focused regression **passed**:

```sh
cargo test --locked --offline -p orishu-worker --lib leave_clears_real_owner --quiet
```

This supplies the pending-timer owner/leave mapping in the stale-IO inventory.
Core tests separately cover obsolete suspicion and join tokens. This fixture
does not run a network exchange or prove every ejection/adoption/shutdown
interleaving. It observes existing production cleanup; no production behavior,
protocol, persisted format or dependency changed.

Default and `formation-fault-test` worker all-target suites **passed 138 tests
each** (105 library, 16 binary, 17 integration). Scoped Clippy passed with
warnings denied in both configurations. Workspace formatting, documentation
checks (102 Markdown files) and scoped whitespace checks passed. Full-workspace
tests, separate process journeys and optional telemetry were not rerun for
this test-only increment.

### Owner-supervised operational health foundation — 2026-09-06

`driver::View` now carries an optional monotonic owner-progress timestamp.
The existing one-second owner probe tick updates it even when participation
disallows new peer work. This heartbeat is produced by the serialized owner,
not the reader/exporter. `Handle::health` reads its bounded projection without
sending a core command. `health::OwnerHealth` uses a five-second deadline and
finite reasons, keeping responsiveness separate from formation readiness.

Two pure matrix tests cover every participation state, initial absence, exact
expiry boundary, regressed supplied time and closed owner. The real-owner
`health_reads_cannot_mask_stalled_owner_shutdown` regression observes first
progress, proves a locked non-introducing standalone owner remains healthy,
then holds the existing test shutdown barrier. Continuous reads cannot alter
the last timestamp: the state changes from responsive/non-ready `Stopping`
to `Stalled` after the budget. Releasing the owner produces `Closed`. The
fixture has an eight-second whole-journey bound and does not fabricate time
or a healthy exporter heartbeat. Both focused commands **passed**:

```sh
cargo test --locked --offline -p orishu-worker --lib health:: --quiet
cargo test --locked --offline -p orishu-worker --lib health_reads --quiet
```

This implements the N-FORMATION supervision hook consumed by P-OBSERVABILITY.
It adds no exporter dependency to the core, no public endpoint or wire schema,
and no workload authority. Process initialization/required-role checks and
startup latching remain necessary before the optional adapter can serve probes.
Do not equate `formation_ready()` with full process `/readyz` or cluster
convergence. Metrics, security/feature configuration, traces and recipes remain
undelivered. The five-second budget is not a measured production SLO.

Default and `formation-fault-test` worker all-target suites **passed 141 tests
each** (108 library, 16 binary, 17 integration). Scoped Clippy passed with
warnings denied in both builds. Workspace formatting, documentation checks
(102 Markdown files) and scoped whitespace checks passed. Full-workspace Rust,
separate process journeys and optional-exporter acceptance were not rerun in
this foundation increment.

`cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
also **passed all three dependency-purity tests** after the health hook landed.

### Process startup and required-role health composition — 2026-09-06

`RunningWorker::health` now combines the existing owner supervision input with
a process-lifetime initialization latch, sticky required-role failure and
shutdown intent. The executable marks initialization only after all configured
client listeners bind. Every client-server task owns a `RequiredRole` lifetime
guard, so normal exit, panic or cancellation withholds readiness. This does not
reset startup success or assert the owner is dead. Shutdown withholds readiness
before waiting for its owner acknowledgement. Current static configuration has
no role-restart operation, so a role failure remains latched for the process.

The pure state regression checks startup before/after initialization, owner
transition gating, role failure, repeated initialization, shutdown precedence
and retained startup after owner closure. The runtime regression
`process_health_tracks_initialization_required_task_loss_and_shutdown` uses a
real owner and an actual cancelled task holding the same guard as the listener
tasks. It verifies healthy ready state, responsive-but-unready role failure,
and closed/stopping state with startup still latched under three seconds.
This is task-lifetime integration evidence, not an HTTP probe test.

Both first worker-suite runs failed setup with `UnsafePath` before reaching
health assertions: the new fixture reused a non-private temporary directory
as its credential directory. Diagnosis followed the existing ownership/mode
guards; changing only the fixture to a loader-created private child made the
focused regression pass. No security check was relaxed and no temporary
debug instrumentation was added.

```sh
cargo test --locked --offline -p orishu-worker --lib health:: --quiet
cargo test --locked --offline -p orishu-worker --lib process_health_tracks --quiet
```

Both focused commands **passed**. No optional features, diagnostic listener,
HTTP health routes, metrics, traces or operator deployment recipes ship in this
increment. The projection is local and unversioned internal state, not a new
wire resource. Future required solver/storage roles must extend the process
initialization/failure gates; current health must not claim those services.

Corrected default and `formation-fault-test` all-target worker suites **passed
143 tests each** (110 library, 16 binary, 17 integration). `make test-formation`
passed the full ordinary CLI handoff/leave/crash/restart/readmission journey,
with six evidence-helper and nine HTTP-helper tests. Workspace formatting,
documentation (102 Markdown files) and scoped whitespace checks passed. Builds
retain the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.
Full-workspace Rust, separate fault-process and optional-exporter acceptance
were not rerun after this process-health change.

Both scoped Clippy configurations passed again after the fixture correction.

### Optional diagnostics HTTP lifecycle — 2026-09-06

The initial `observability` listener has executable loopback evidence for
healthy `/metrics`, `/livez`, `/readyz` and `/startupz`, runtime-disabled startup
while the configured port is occupied, and startup rejection of unsupported
exposure before credential creation. Its three label-free health gauges are
not the full formation metric catalogue or Prometheus parser acceptance.

`diagnostics::tests::http_probes_track_initialization_role_failure_and_closed_owner`
adds real HTTP requests through the production router and bounded server,
backed by an actual membership owner. It checks initialization refusal,
successful startup, safe membership lock/unlock, cancelled required-role
readiness failure, repeated initialization not clearing that failure, and
closed-owner liveness/readiness failure while startup stays latched. Every
phase compares probe statuses with the exported health gauges; responses
remain bounded, plain text and non-cacheable. The entire fixture has a
five-second deadline and individual requests a one-second deadline.

The lifecycle fixture uses a Unix socket and deliberately keeps diagnostics
reachable after owner shutdown. It establishes HTTP projection of real owner
and role state, not independent-process shutdown ordering, join/recovery or
stalled-owner HTTP behavior. The executable loopback fixture separately covers
TCP startup and healthy routes. No new public fault control or production
health transition was introduced by this test increment.

The focused lifecycle test **passed**. The observability-enabled worker suite
**passed 147 tests** (110 library, 18 binary, 19 integration), and scoped Clippy
passed with warnings denied. These runs preceded the additional lock/unlock
assertions. The default-feature suite **passed 145 tests** (110 library,
17 binary, 18 integration); it does not compile the optional lifecycle test.
Exact reproduction commands:

```sh
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_probes_track --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
```

Feature-changing test runs use `target/debug` sequentially to avoid replacing
the executable underneath another suite. Membership dependency-purity tests
passed all three checks; formatting, documentation validation (102 Markdown
files) and scoped whitespace checks passed. Full-workspace Rust, separate
fault-process journeys, Prometheus/OTLP integration and remaining optional
feature/security matrices were not run for this increment.

After adding the lock/unlock assertions, the combined optional/fault-feature
suite **passed all 147 tests**, and its scoped Clippy check also passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test -- -D warnings
```

This is feature-build regression evidence, not a rerun of the separate
fault-process journeys or the not-yet-implemented `otlp-tracing` matrix.

### Diagnostics enablement precedence through the executable — 2026-09-06

`diagnostics_enablement_precedence_and_invalid_config_are_checked_at_startup`
starts the actual worker with YAML, child-process environment overrides and
CLI options. Five cases cover file enablement, environment enable/disable
overrides and CLI enable/disable overrides of the environment. Child-local
environment settings avoid mutating the concurrent test process environment.

Disabled cases retain an occupied diagnostics port while inspecting a real
one-member summary and terminating gracefully. Enabled cases request a
non-loopback bind and require exit code 2 before the state directory or
credentials exist: feature-disabled builds report the unavailable capability;
feature-enabled builds report unsupported remote exposure. Every case has a
five-second whole-journey deadline, with subprocess cleanup on failure.

The default-build focused test **passed**:

```sh
cargo test --locked --offline -p orishu-worker --test standalone diagnostics_enablement --quiet
```

This closes the executable enablement-precedence evidence gap, not the whole
configuration matrix. Bind-address precedence, malformed file/env/CLI values,
enabled occupied-port handling, route-specific settings and remote TLS/auth
still require their own evidence or implementation. Existing healthy TCP
probe tests, not these deliberate rejection cases, establish enabled serving.

The observability-enabled all-target suite **passed 148 tests** (110 library,
18 binary, 20 integration). Scoped Clippy passed with warnings denied in both
default and observability builds. Formatting, scoped whitespace and
documentation validation (102 Markdown files) passed. Feature-changing suites
ran sequentially against the shared `target/debug` executable path.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

No production configuration behavior changed in this increment. Full-workspace
Rust, separate process-fault journeys, the fault-feature suite and telemetry
backend integration were not rerun; their acceptance remains separate.

The subsequent default-feature all-target suite also **passed 146 tests**
(110 library, 17 binary, 19 integration):

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
```

### Configurable diagnostic route groups — 2026-09-06

The worker now accepts typed `observability.metrics` and `observability.probes`
startup settings with file < environment < CLI precedence. Metrics selects
`/metrics`; probes selects `/livez`, `/readyz` and `/startupz` together. Both
default on inside an explicitly enabled listener. Selecting a group does not
enable diagnostics, an enabled listener with neither selected is rejected,
and disabled routes are not mounted. Probe-only mode still requires loopback;
this is not remote-exemption or monitoring-credential implementation.

`config::tests::diagnostics_route_groups_are_explicit_and_cannot_enable_an_empty_listener`
checks defaults and all four group combinations with the listener disabled and
enabled, including omitted build capability. The executable regression
`diagnostics_route_groups_follow_file_environment_and_cli` exercises metrics-only,
probes-only and both, using each of the three configuration sources. Lower
priority sources deliberately disagree so the nine-process matrix proves the
effective route selection, not just argument parsing. Actual TCP requests
require 200 for healthy selected routes and 404 for absent routes; secret
client routes remain absent and normal formation identity stays unchanged.

The fixture bounds the complete matrix to twenty seconds, every diagnostics
request to one second and graceful process termination to three seconds. The
shared request helper tolerates connection refusal while the listener binds,
without extending its total request deadline. Default disabled behavior and
enablement precedence retain their existing executable regressions.

Focused configuration tests **passed two tests**, and focused executable
diagnostics tests **passed four tests**. The observability-enabled all-target
suite **passed 150 tests** (110 library, 19 binary, 21 integration); scoped
Clippy passed with warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker diagnostics_ --quiet
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

Configuration, protocol, worker manual, observability guide and worker-operator
story now describe this partial delivery. No wire profile, persisted formation
format, domain authority or dependency changed. Existing settings retain their
behavior; the new settings are opt-in route restrictions. Full remote security,
probe lifecycle/pressure coverage, Prometheus catalogue/backend integration,
tracing and deployment recipes remain open. This does not close M4.

The sequential default-feature suite **passed 147 tests** (110 library,
18 binary, 19 integration), and default scoped Clippy passed with warnings
denied. Membership dependency-purity passed all three tests; workspace
formatting, documentation validation (102 Markdown files) and scoped
whitespace checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
cargo fmt --all -- --check
make docs-check
```

Full-workspace Rust and separate process-fault journeys were not rerun for
this local diagnostics routing change.

The subsequent combined observability/fault-feature suite **passed all 150
tests** (110 library, 19 binary, 21 integration):

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test --quiet
```

This verifies that build combination, not the separate process-fault journeys
or the future OTLP feature matrix.

### Owner counters in Prometheus exposition — 2026-09-06

`Handle::counters` and `RunningWorker::owner_counters` copy six integers from
the existing published owner view without cloning membership/identity data,
enumerating peers or submitting work. They retain the last published values
after owner closure. These shell-owned counters already saturate, accumulate
across formation generations and start from zero in a fresh process; no core
exporter or new dependency was introduced.

The optional `/metrics` catalogue now adds `membership_transitions_total`,
`stale_inputs_total`, `core_diagnostics_total`, `foreign_gossip_total`,
`reliable_replies_total` and `send_failures_total`, each prefixed
`orishu_worker_`. They are unlabelled counters with explicit event semantics,
not accepted-command or peer-death counts. The three health gauges remain.
Health and counters are separately sampled local projections; they do not
promise one atomic scientific or cluster snapshot. The catalogue and manuals
document process restart and closed-owner interpretation.

The HTTP lifecycle regression checks each counter type/value representation,
bounded response size, transition growth after actual lock/unlock commands,
and exact agreement with retained owner counters after closure. Its first
extended run **failed** because the fixture expected standalone leave to
create a new identity. The owner explicitly returns `changed=false` for this
case; checking that receipt confirmed a test-assumption error. The corrected
test preserves that no-op and **passed**, without changing leave semantics.
The existing `leave_clears_real_owner_deadlines_and_fences_late_timer_input`
regression separately checks counter continuity across a real owner generation
change and a counted late timer, including retained counts after closure.

The focused HTTP command passed after correction; its earlier failure remains
recorded above:

```sh
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_probes_track --quiet
```

This is initial formation instrumentation, not the full metric/latency/queue
catalogue, a Prometheus-compatible parser check, an external scrape, an
overhead measurement or trace delivery. Those acceptance requirements remain
open together with the rest of M4.

After correction, the observability all-target suite **passed 150 tests**
(110 library, 19 binary, 21 integration), and the sequential default suite
**passed 147 tests** (110 library, 18 binary, 19 integration). Scoped Clippy
passed with warnings denied in both configurations. All three membership
dependency-purity tests, workspace formatting, documentation validation (102
Markdown files) and scoped whitespace checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
cargo fmt --all -- --check
make docs-check
```

Full-workspace Rust, fault-feature and separate process-fault journeys were
not rerun for this counter-projection increment. No external monitoring
backend was used.

### Real Prometheus parser and source-build scrape — 2026-09-06

`make test-worker-prometheus` builds the `observability` worker and runs
`scripts/check-worker-prometheus.py` against pinned official Prometheus 3.5.0
tools. The download's Linux amd64 SHA-256 matched the official release checksum
before extraction; version assertions also run in the harness. Downloads are
not part of the Make target and add no worker or membership dependency.

The first sandboxed invocation **failed before spawning workers** because
socket creation was prohibited. The approved loopback-capable invocation
**passed**. After adding the malformed-input negative control and Make target,
the complete target **passed again**, without retrying a failed application
scenario. The tool rejects malformed exposition, accepts a bounded real
worker `/metrics` response and validates `etc/prometheus-local.yml` with only
the ephemeral target address substituted. A separate Prometheus process then
ingests and returns exactly the nine expected series with only metric-name,
job and instance labels. Healthy gauges are one, samples are finite and
non-negative, the actual operator token is absent, and the worker remains
ready. Both processes terminate gracefully within their cleanup deadlines.

Reproduction with verified local copies of the pinned tools:

```sh
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

See [the test guide](../testing-worker-prometheus.md) for the exact checksum,
version, bounds, isolation, example and limitations. The source-build local
parser/scraper requirement now has real external-tool evidence. Packaged
release artifacts, remote TLS/auth, full formation metrics, overhead, long-run
cardinality, exporter failure, traces and dashboard/alert recipes remain
unverified or unimplemented; this does not close the companion tasks or M4.
No Rust behavior changed in this harness/configuration/documentation increment;
full Rust suites and separate formation-fault journeys were not rerun.

The script's CLI/syntax check, scoped whitespace checks and repository
documentation validation **passed** (103 Markdown files). The local test
guide is indexed and the companion task/roadmap status records partial
operator-documentation delivery rather than treating the full handoff as done.

### Local operator alert rules and recovery fixtures — 2026-09-06

`etc/prometheus-worker-alerts.yml` defines the versioned
`orishu-worker-local-v1` group with three warning examples: scrape unavailable,
reachable-but-sustained-unready, and successful scrape lacking readiness data.
The readiness rules match by scrape job/instance and require successful `up`;
missing series are never converted to measured zero. The one/two-minute pending
delays and one-minute evaluation cadence are example budgets, not measured SLOs
or automatic recovery authority.

`prometheus-worker-alerts.test.yml` exercises three synthetic rule-engine
scenarios: healthy/transient/down/unready/missing separation, recovery of all
three alerts, and stale readiness versus disappearance of a discovery target.
The latter explicitly records the inventory limitation rather than pretending
that a vanished `up` series proves a healthy or dead process.

The pinned Prometheus 3.5.0 rule test **passed**, and the updated complete
worker/Prometheus target **passed**. It validates the rule syntax and fixtures,
copies the checked-in rules alongside the generated scrape configuration,
ingests nine real worker series, and verifies that the separate server loaded
the exact three alert rules with no alerts on the short healthy run:

```sh
/path/to/promtool test rules etc/prometheus-worker-alerts.test.yml
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

The [operator test guide](../testing-worker-prometheus.md#local-alert-examples-and-operator-response)
documents threshold rationale, idle and missing-data semantics, discovery
limitations and safe inspection steps. Neither an alert nor a scrape authorizes
restart, leave, new admission or exclusion removal. Rule state transitions have
synthetic engine evidence; this is not a real-worker failure journey held for
the full pending delays or Alertmanager notification-delivery evidence.
Dashboards and the remaining instrument-specific alerts/runbooks remain open.

Documentation validation **passed** (103 Markdown files) and scoped whitespace
checks passed. No Rust behavior changed; full Rust or formation-fault suites
were not rerun for this rule/example/harness change. Concurrent release and CI
work in the worktree was not modified by this increment.

### Scraper outage, operator control and fresh re-ingestion — 2026-09-06

The real Prometheus harness now builds and invokes `orishuctl` as well as the
observability-enabled worker. After the initial nine-series scrape and alert
loading check, it stops and reaps the actual Prometheus process. While that
process is absent, two authenticated CLI operations lock and unlock the worker
using its formation precondition and explicit operation IDs. Receipts and
subsequent public status agree, formation identity is unchanged, `/readyz`
stays successful, and the real owner transition counter advances.

The fixture restarts Prometheus against its existing temporary TSDB and
requires ingestion of at least the counter value reached while the scraper
was absent. Since that value is strictly greater than the value read after
the original scraper exited, old persisted samples cannot satisfy recovery.
The worker remains the same process throughout. Polling and subprocess cleanup
retain explicit deadlines; credentials are supplied to the CLI by file path,
not printed or placed in process arguments.

The full target **passed**:

```sh
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

The run used the previously checksum-verified official Prometheus 3.5.0 tools.
The build emitted the existing `proc-macro-error2 v2.0.1` future-incompatibility
warning; it was not a build or test failure. No Rust behavior changed, and full
Rust suites or separate formation-fault journeys were not rerun. This is
standalone scraper-outage/control evidence, not a multi-worker admission/SWIM
journey, scrape-saturation test, diagnostics-task failure or OTLP queue/collector
outage test. Those remaining companion requirements are not waived.

The script's CLI/syntax check, scoped whitespace checks and documentation
validation **passed** (103 Markdown files). The operator guide now documents
the actual outage/recovery assertions and their single-worker scope.

### Malformed diagnostics startup settings — 2026-09-06

`malformed_diagnostics_configuration_exits_before_credentials` exercises the
actual worker executable with invalid `enabled`, `bind`, `metrics` and `probes`
values from YAML, environment and CLI, plus an unknown nested YAML key. Each
of the thirteen cases has a three-second deadline and requires exit code 2,
an actionable parser error, no credential/state directory and no client socket.
Child-local environment overrides are isolated from concurrent test processes.
The default disabled listener does not make malformed typed settings valid.

The focused default-feature regression **passed**:

```sh
cargo test --locked --offline -p orishu-worker --test standalone malformed_diagnostics --quiet
```

This covers those malformed-input paths, not every configuration combination.
Bind-address precedence and enabled occupied-port handling remain separate
acceptance work. The existing route-selection unit matrix covers an enabled
empty listener; that is not yet independent-process rejection evidence.
No production configuration or security behavior changed in this increment.

The same focused test **passed with observability enabled**, covering all
thirteen cases again. Observability-enabled scoped Clippy with warnings denied,
workspace formatting, scoped whitespace and documentation validation (103
Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone malformed_diagnostics --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, fault-feature/process journeys and the external
Prometheus harness were not rerun for this focused test-only increment.

### Fail-fast diagnostics listener binding — 2026-09-06

The executable now uses fallible diagnostics binding after configuration/fault
validation but before credential creation, membership-owner startup or client
listener binding. It retains the resulting acceptor until serving starts,
rather than probing availability and racing a second bind. Disabled diagnostics
still bind nothing; a TCP connection during initialization is not a successful
health response. No listener/security policy or wire protocol changed.

The new executable regression holds a real loopback port throughout startup.
Its initial run **failed as expected**, observing the old `AddrInUse` panic
and exit code 101. After the change it **passed**: exit code 2, a clear
`cannot bind diagnostics listener` error, no panic, no state/credential
directory or client socket, and the fixture-owned conflicting listener remains
untouched. Each run has a three-second whole-process deadline:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone occupied_diagnostics --quiet
```

This establishes occupied diagnostics-port failure and the new startup order,
not failure atomicity for every other worker initialization path. The worker
manual and configuration guide document the behavior and safe operator action.

After the fix, the observability-enabled all-target suite **passed 152 tests**
(110 library, 19 binary, 23 integration), and the sequential default suite
**passed 148 tests** (110 library, 18 binary, 20 integration). Scoped Clippy
passed with warnings denied in both builds. Workspace formatting, scoped
whitespace and documentation validation (103 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full-workspace Rust and fault-feature/process journeys were not rerun for this
startup-order change; these results do not establish final M4 acceptance.

The separate `make test-worker-prometheus` target also **passed** with the
verified Prometheus 3.5.0 tools after the change: actual parser acceptance,
nine-series ingestion, tested alert loading, authenticated control during
scraper outage and fresh re-ingestion all remain working. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

### Diagnostics bind-address precedence through serving — 2026-09-06

`diagnostics_bind_precedence_uses_only_the_selected_address` starts real workers
for file, environment and CLI address selection. Three distinct loopback
candidate sockets are reserved; only the expected winning address is released.
The other candidates remain occupied throughout startup and HTTP assertions.
This proves actual bind selection, not just parsing: `/readyz` and `/metrics`
must be served on the winning address, the normal operator summary must retain
the same formation identity, and graceful termination must succeed. The whole
three-case journey has a twelve-second deadline; child environment settings
are isolated and all fixture-owned sockets remain untouched.

The focused regression **passed**, and observability-enabled scoped Clippy
passed with warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_bind_precedence --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

Together with the separate malformed-setting, enablement, route-selection and
occupied-port tests, this closes the documented bind-precedence evidence gap.
It does not establish secured remote binds, all platform/address families,
release feature matrices or every contradictory-setting process path. No
production behavior changed; full Rust suites, separate formation faults and
the Prometheus backend harness were not rerun for this test-only increment.

The broader executable diagnostics group **passed all seven tests**:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
```

Workspace formatting, scoped whitespace and documentation validation also
passed (103 Markdown files).

### Bounded owner-lane pressure metrics — 2026-09-06

`Handle::pressure` and `RunningWorker::owner_pressure` expose fixed-size local
readings for peer, control, completion and shutdown lanes. Each reading copies
the configured channel capacity and used slots; reserved permits count as
occupied before sending a message. No message collection is cloned, no
capacity is consumed by inspection, and no core/network/exporter dependency
was added. Different lanes are sampled independently, not as an atomic
scheduling or cluster-capacity snapshot.

The optional Prometheus catalogue adds `orishu_worker_{lane}_slots_in_use` and
`orishu_worker_{lane}_slots_capacity` for exactly those four lane names, eight
unlabelled gauges. Capacities are 64, 16, 64 and 1. The catalogue now contains
seventeen series and remains below the documented 4 KiB exposition bound.
These readings are slot occupancy, not running tasks or queue length excluding
reservations; liveness remains independently supervised.

The existing peer saturation regression now checks full peer occupancy and
untouched control/completion/shutdown capacity. The reserved-verification
regression checks that a full set of outstanding permits consumes all completion
slots even before messages are sent. HTTP lifecycle assertions verify gauge
types, configured capacities and occupancy bounds. The external Prometheus
harness now requires all seventeen names and validates capacity/occupancy
values after actual ingestion.

The observability-enabled all-target suite **passed 153 tests** (110 library,
19 binary, 24 integration), and scoped Clippy passed with warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

This is additive local channel instrumentation, not full request/transport
metrics, scrape-overload evidence, trace export or a measured overhead budget.
The metric catalogue, worker manual, protocol maturity and operator story
describe the exact semantics; the remaining companion gates stay open.

The subsequent default suite **passed 148 tests** (110 library, 18 binary,
20 integration). All three membership dependency-purity checks passed, as did
workspace formatting, scoped whitespace and documentation validation (103
Markdown files). The real `make test-worker-prometheus` target **passed** with
the verified 3.5.0 tools: seventeen series parsed/ingested, fixed slot capacities
and bounded integral occupancy, three tested alerts loaded, operator control
during scraper outage and fresh ingestion after recovery. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

Full-workspace Rust and separate fault-feature/process journeys were not rerun
for this additive metric projection. These results do not close M4.

### Diagnostics HTTP/1.1 header limits — 2026-09-06

`diagnostics_reject_oversized_headers_and_preserve_normal_access` uses an actual
observability-enabled executable and TCP diagnostics listener. It sends 32
headers (accepted), 33 and 42 headers (431), and a header value larger than the
8192-byte request-head budget (431). Rejections contain no metric exposition;
response reads are bounded to 8192 bytes and two seconds. After every case,
ordinary metric/liveness requests still succeed and the real Unix operator
summary retains the original formation identity. The whole fixture, including
graceful process cleanup, has an eight-second deadline.

The initial oversized-input regression passed before adding the exact-count
boundary checks. This is real diagnostics HTTP/1.1 admission/recovery evidence,
not concurrent scraper saturation, stalled response writes, HTTP/2 ingress,
peer/control fairness under load, or remote-auth acceptance. No production
limits changed; the test exercises the existing shared server configuration.

With the exact-count assertions, the executable diagnostics group **passed all
eight tests**, and observability-enabled scoped Clippy passed with warnings
denied. Workspace formatting, scoped whitespace and documentation validation
(103 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, separate formation-fault journeys and external
Prometheus acceptance were not rerun for this test-only increment.

### Shutdown with unfinished diagnostics requests — 2026-09-06

`unfinished_diagnostics_requests_do_not_prevent_process_shutdown` starts the
actual worker with its optional TCP diagnostics listener and normal Unix client
surface. Eight persistent diagnostics connections each receive a complete
successful startup probe before sending an unfinished metrics request head.
This confirms server acceptance, not merely sockets waiting in a listen
backlog. The connections remain open through SIGTERM and process exit; test
cleanup does not manufacture successful server cancellation.

While those clients remain held, liveness and the normal operator summary
remain available with unchanged formation identity. Graceful termination must
exit successfully within three seconds and remove the worker's own Unix socket.
Each held diagnostics connection must then observe EOF or connection reset.
The full fixture has an eight-second deadline and bounded response reads.

The focused executable test **passed**:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone unfinished_diagnostics --quiet
```

Only half of the sixteen diagnostics connections are held, deliberately leaving
capacity for probes. This is unfinished-IO shutdown isolation, not exact-limit
saturation, overload fairness, slow-response backpressure, stalled-owner health
or HTTP/2 conformance. No production shutdown behavior changed.

The combined observability/fault-feature all-target suite **passed 155 tests**
(110 library, 19 binary, 26 integration), and scoped Clippy passed with warnings
denied in that configuration. Workspace formatting, scoped whitespace and
documentation validation (103 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
```

This verifies the combined feature build, not the separate fault-process
journeys. Full-workspace Rust, default-worker suites and external Prometheus
checks were not rerun for this test-only increment.

### Unfinished diagnostics expiry and operator progress — 2026-09-06

The accepted-connection fixture now has an additional live-process expiry
path. Eight persistent connections first receive successful startup probes,
then hold unfinished metrics request heads. Authenticated lock/unlock calls
through the normal Unix client must complete within one second each while
those clients remain held; their receipts reflect the requested policy. This
adds actual owner-command progress to the earlier read-only checks.

Without shutdown or closing the test clients first, all held connections must
reach EOF/reset within one shared seven-second observation deadline (a bounded
408 response before EOF is permitted). This exercises the configured
five-second HTTP/1.1 head timeout with test margin. The same worker must then
serve normal liveness/metrics and the unchanged formation identity before
graceful termination. The expiry journey has a twelve-second total deadline;
the original shutdown path retains its eight-second total and three-second
process-termination bounds, with a shared one-second post-exit close check.

Both focused lifecycle tests **passed** in approximately five seconds after
the authenticated control assertions were added:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone unfinished_diagnostics --quiet
```

This proves deadline-driven reclamation and local control with eight held
diagnostics connections, not exact sixteen-connection saturation, active
trickle attacks, HTTP/2, peer-flood fairness or slow response backpressure.
No production timeout, authorization or shutdown behavior changed.

The wider executable diagnostics group **passed all ten tests**. Scoped
observability-enabled Clippy passed with warnings denied; workspace formatting,
scoped whitespace and documentation validation passed (104 Markdown files in
the final check after concurrent documentation additions):

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, fault-feature/process journeys and external
Prometheus acceptance were not rerun for this test-only increment.

### Full diagnostics connection budget and recovery — 2026-09-06

`diagnostics_full_connection_budget_preserves_operator_control_and_recovers`
extends the accepted-connection fixture to all sixteen diagnostics slots. Each
connection receives a successful startup probe before sending an unfinished
request head. Setup must finish within two seconds, before the five-second
head deadline can reclaim the earliest connection. A seventeenth connection
must close/refuse or remain waiting for the bounded 500 ms check; any response
byte fails the assertion. This does not count kernel backlog entries as served
HTTP requests or promise one particular refusal status.

Authenticated lock/unlock still executes through the independent Unix client
surface within one second each, and the complete control check must finish
within four seconds of pressure setup, before header expiry. All sixteen held
clients subsequently observe server-side closure under the shared expiry
budget. Normal liveness and metrics requests then succeed on the same worker
with unchanged formation identity, followed by graceful shutdown. No test
client is closed early to free any of the sixteen accepted slots.

The focused real-process regression **passed** in approximately five seconds:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_full_connection --quiet
```

This closes the scoped HTTP/1.1 diagnostics connection-budget/control/recovery
case, not arbitrary floods, active trickle traffic, TLS/HTTP2 behavior, peer
fairness, or slow-response buffering. Probes share the diagnostics connection
budget: saturation may make a probe unreachable without changing owner health.
Operator tools must not infer owner death from that transport failure alone.
No production connection limit, scheduling or membership behavior changed.

The wider executable diagnostics group **passed all eleven tests**. Scoped
observability-enabled Clippy, workspace formatting, scoped whitespace and
documentation validation (104 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, separate fault-feature/process journeys and the
external Prometheus harness were not rerun for this test-only increment.

## Stalled-shutdown owner through HTTP — 2026-09-06

`diagnostics::tests::http_reads_cannot_hide_stalled_owner_shutdown` **passed**:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_reads_cannot_hide --quiet
```

The fixture uses a real membership owner and the production diagnostics
router/server over Unix. Healthy initialized probes first succeed. The existing
owner shutdown-hold fixture, now also available through the development-only
fault feature's Rust API, acknowledges shutdown while preventing further owner
progress. Repeated successful metric reads and failed readiness reads cannot
refresh supervision. Within the configured five-second deadline plus a
one-second observation allowance, the still-open owner becomes stalled:
`/livez` and `/readyz` return 503, `/startupz` and `/metrics` return 200, and
metric health values agree. Releasing the hold permits normal owner closure,
with the same failed-liveness and latched-startup HTTP results.

The whole fixture has a ten-second deadline and each HTTP request a one-second
deadline. There is no new CLI or remotely callable fault control; the existing
release-build prohibition covers the fault feature. This is in-process real
owner/HTTP evidence for a stalled shutdown, not independent-process running
owner stalls, transport saturation or executable listener shutdown ordering.
Other probe states and the full companion gates remain open.

Validation against this dirty-worktree checkpoint **passed**: combined-feature
worker all-target tests (110 library, 20 binary, 28 integration; 158 total),
default worker all-target tests (110 library, 18 binary, 20 integration; 148
total), and scoped Clippy with warnings denied for both configurations:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Feature-changing test builds ran sequentially against `target/debug`.
Formatting, scoped whitespace checks and documentation validation (104 Markdown
files) passed. Full-workspace tests, independent formation fault journeys,
release-build exclusion and the external Prometheus harness were not rerun;
existing evidence for those boundaries is not upgraded by this increment.

## Running-owner stall and HTTP recovery — 2026-09-06

`diagnostics::tests::http_probes_detect_running_owner_stall_and_recovery`
**passed** in the focused combined-feature binary test:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_probes_detect_running --quiet
```

A development-only Rust control pauses the real owner at its IO loop, without
pausing the async executor, changing membership or fabricating a progress
timestamp. The fixture waits for acknowledgment of that pause. No CLI/network
route exposes it; the existing fault-feature release-build prohibition applies.
Dropping or sending the release channel resumes the owner.

The shared real Unix HTTP fixture now exercises both running and shutdown
stalls. The running case starts fully initialized and healthy, then continuously
reads metrics, liveness and readiness while owner transitions remain frozen.
Within the five-second supervision deadline plus one second of observation
allowance, the still-open owner becomes stalled. HTTP then reports 503 for
liveness/readiness and 200 for startup/metrics, with matching health gauges.
The transition counter has not advanced. After release, the actual owner tick
restores health within two seconds; transitions resume without changing
formation ID, node ID or participation. Normal shutdown and closed-owner
probe assertions finish the ten-second-bounded fixture.

This establishes running-owner stall/recovery through real owner and HTTP
boundaries in one process. It is not an OS-wide freeze, independent-process
fault journey, admission recovery or evidence for every probe/feature/security
combination. The previous shutdown fixture remains a separate regression.

Validation **passed** against this dirty-worktree checkpoint: combined-feature
worker all-target tests (110 library, 21 binary, 28 integration; 159 total),
default worker all-target tests (110 library, 18 binary, 20 integration; 148
total), and scoped Clippy with warnings denied for each configuration:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

The initial formatting check identified one assertion requiring wrapping;
after correction, formatting and documentation checks (104 Markdown files)
passed. Test builds with different feature sets ran sequentially using
`target/debug`. Full-workspace tests, independent process-fault journeys,
release exclusion builds and the external Prometheus harness were not rerun.
No production protocol, dependency or health deadline changed.

## Admission-baseline condition audit — 2026-09-06

The baseline row has the following concrete evidence mapping. These tests
exercise different authorities: immutable encoding/retention, authenticated
transfer, atomic domain merge, receiving-owner readiness and public handoff.
None alone proves all five boundaries.

| Required condition | Named evidence and asserted boundary |
| --- | --- |
| Explicit empty baseline | `empty_baseline_is_explicit_and_mutation_cannot_change_frozen_pages`: one policy-only page, missing pages cannot finish, atomic empty installation preserves the model. `explicit_introducer_runtime_admits_but_adoption_withholds_readiness`: real empty-source catch-up is not introduction-ready until completion. |
| Complete ordered pagination | `pages_require_complete_ordered_identity_bound_content`: two pages/41 records, skipped/duplicate/missing pages rejected. `completion_requires_every_page_and_current_identity_authority`: confirmation requires both issued pages and matching root; a later certificate restriction revokes access. |
| Source changes during transfer | `source_changes_between_pages_preserve_frozen_transfer_and_refresh_next_baseline`: real mTLS QUIC, source policy/blocklist changes after page 0, original transfer remains coherent, fresh transfer has a newer snapshot/different root and updated records. |
| Immutable retry and finite retention | `retained_retry_is_immutable_and_expiry_does_not_reuse_snapshot_identity`: exact retry preserves bytes/descriptor despite source changes; expiry refuses continuation and a new begin gets a new ID. `live_leases_are_not_evicted_and_continuations_are_requester_and_generation_bound`: four leases, overload without eviction, requester/generation guards. |
| Truncated, cross-snapshot, expired or corrupt transfer | The four `peer::catchup::client::tests` fault cases each fail over real QUIC without credential confirmation, then complete a fresh two-page transfer using the same connection and one-slot exchange pool. |
| Newer receiver state and atomic rejection | `newer_local_restrictions_survive_stale_and_empty_source_views`, `late_invalid_record_rolls_back_policy_blocklist_gossip_and_removal`, `conflict_wrong_identity_duplicates_and_union_capacity_are_atomic`: pure production merge preserves newer restrictions and rolls back invalid/conflicting/over-capacity installation. |
| Incomplete/cancelled transfer and exhaustion | `explicit_introducer_runtime_admits_but_adoption_withholds_readiness` holds/cancels a prepared job, observes retained failure and no token, then completes automatic retry. `exhausted_catchup_attempts_remain_non_introducing` refuses a fourth attempt; `adoption_deadline_refuses_successful_catchup_completion` refuses late verified success at the owner deadline. |
| Stale completion and self-removal | Named runtime leave/ejection/shutdown/session-loss regressions preserve generation/session fences. `self_removal_is_explicit_and_never_readiness` and `ejection_fences_prepared_catchup_and_its_late_cancellation` pair pure self-removal with real-owner suppression of token/readiness restoration. |
| Public introducer handoff | `check_public_adoption` polls B's `joined` operation and introduction readiness, retrieves formation-bound material through B and admits C through it. The process run below passed these assertions before a later readmission convergence failure; it did not pass the whole journey. |

The new changing-source fixture runs both transfers within ten seconds using
the real source retention/resource handler and receiver. Source changes use
ordinary domain commands at the fixture boundary; this is not a multi-process
policy partition or a claim of globally latest policy. The four existing
receiver faults remain separate named regressions in the shared fixture.

Verification passed: 12 worker catch-up tests, all 149 default worker tests
(111 library, 18 binary, 20 integration), five pure baseline tests, scoped
worker Clippy with warnings denied, formatting and docs checks (104 files):

```sh
cargo test --locked --offline -p orishu-worker --lib peer::catchup --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-membership baseline::tests --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

`make test-formation` **failed** at “readmission retains dead history and three
live identities,” after the earlier handoff, voluntary leave and receiving
worker readmission assertions. Its six evidence-helper and nine HTTP-helper
tests passed. Private failure evidence is retained at
`/tmp/orishu-formation-failure-7t0gehiq`. The final recorded summaries show A
with three records/two live identities while B and C have four records/three
live identities. All processes were alive before harness cleanup. This does
not identify the cause or justify increasing the unchanged convergence budget.
The failure prevents a green final formation acceptance claim; preserve it
alongside later reproductions or resolutions.

A second run of `python3 scripts/check-formation-cli.py` against the unchanged
`target/debug` executables and unchanged deadlines **passed** the complete
handoff/leave/crash/restart/readmission journey. No production fix was applied.
The first failure remains unexplained. The initial diagnostic report lacks
the peer-session and rejected-input details needed to distinguish delayed
reconciliation, stale-session recovery or rejected membership state; further
reproduction must collect evidence at those boundaries rather than infer a
cause from aggregate counts. Optional-feature suites and release builds were
not rerun in this increment. The build retains the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning.

## Readmission reproducer and diagnostic reruns — 2026-09-06

Following the diagnose workflow, the unchanged full journey was run four times
with temporary secret-free worker debug instrumentation: two serial runs and
two concurrently executing, isolated runs. All four **passed**. The debug
probes reported registry connection counts, owner/exchange/rejection counters
and finite rejection categories; they changed no core transition, timeout or
test assertion. No failed instrumented run was captured, so these results do
not discriminate delayed reconciliation, stale-session recovery and rejected
membership state. The original failure remains open.

All `[DEBUG-readmission]` output and its registry inspection helper were
removed, then ordinary worker/CLI binaries were rebuilt. The new
`--readmission-only` harness mode calls the existing public journey, preserving
handoff, policy, leave/replay and readmission assertions and their deadlines.
It stops with normal bounded cleanup immediately after the former failing
checkpoint, before killing/restarting B. It also skips the separate standalone
CLI checks. Other scenario flags are rejected before process startup, and its
success message explicitly disclaims full conformance. No worker hook or
production API is introduced.

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --readmission-only
```

Two isolated executions of this diagnostic command **passed** against the
rebuilt, uninstrumented `target/debug` binaries; they overlapped in time.
Those runs establish that the new mode reaches and checks its intended
production boundary, not that the intermittent failure has been corrected.

Two routing/conflict regressions exercise the real harness entry point with
process creation substituted only for those argument tests. They are not
formation evidence. The complete HTTP/helper suite **passed 11 tests**, and
the failure-evidence suite **passed six tests**. Temporary instrumentation was
removed before formatting validation; documentation checks passed (104 files).
No Rust runtime fix was made, and this increment does not close the original
failure or the full N-FORMATION/M4 gates.

Final `cargo fmt --all -- --check`, `make docs-check` and scoped whitespace
checks passed. Rust unit/workspace suites, optional telemetry and fault-feature
builds were not rerun; production Rust source is unchanged after removing the
temporary debug probes. The build still reports the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning.

## Bounded post-failure readmission evidence — 2026-09-06

The original failure report retained aggregate counts, but raw identity
redaction prevented reliable correlation of individual member records. The
public harness now captures one `ls` response from each of the three workers
only after the readmission-convergence assertion fails. Before redaction, it
projects records onto four fixed harness roles: original introducer, second
introducer, departed identity and readmitted identity. Each role reports record
count, finite liveness states and a certificate-binding match boolean; the
projection also counts unexpected records. No identity strings, names,
endpoints, payload text or credentials enter this projection.

The capture refuses more than 64 KiB before JSON parsing or more than sixteen
decoded records. Every subprocess retains the existing three-second bound;
three reads permit three additional three-second subprocess waits plus bounded
local capture work before normal cleanup. Missing identity, failed command, timeout and invalid data do not
become equivalent: unavailable observations are explicit. These sequential
reads are later than the failed assertion and are not an atomic cluster view.
The assertion is never retried, its deadline is unchanged, and later convergence
cannot change the original failed result.

Three new regression tests exercise the actual projection/capture helper:
role correlation without exported identities, absent record versus failed or
timed-out read (one attempt per worker), and malformed/oversized input. The
full socket/harness suite **passed 14 tests** and the existing failure-evidence
suite **passed six tests**:

```sh
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
make docs-check
```

This is harness evidence hardening, not a membership fix or new diagnostic API.
Five consecutive `python3 scripts/check-formation-cli.py --readmission-only`
executions **passed**, stopping the loop on any failure. They used unchanged
ordinary `target/debug` binaries and the existing convergence deadline. No
failure activated the new capture during these process runs; its error paths
are established by the focused helper tests, not claimed as reproduced process
fault evidence. Final documentation validation (104 files) and scoped whitespace
checks passed. No worker/core source, wire contract or timeout changed; Rust
suites, feature/release matrices and the full crash/restart journey were not
rerun in this increment.

The original intermittent readmission-convergence failure remains open until
its cause is established and corrected or its acceptance contract is explicitly
reconciled with evidence. Passing repetitions alone cannot close it.

## Client-service metrics and real Prometheus ingestion — 2026-09-07

The optional worker IO shell now aggregates six client-service instruments:
in-flight, completed, HTTP 4xx, HTTP 5xx and cancelled handler counts, plus
cumulative completed-handler duration in seconds. Counters saturate; duration
uses a microsecond accumulator. No request bytes, route/path, identity or error
text is retained. Independent atomic reads are not a transactional snapshot.
The middleware is attached only when both the diagnostics listener and metrics
route are runtime-enabled. Diagnostics have a separate service and cannot
recursively count scrapes. No dependency or membership-core boundary changed.

Accounting covers service-handler execution, including authentication and
unknown route/method responses, not HTTP parsing failures before service entry,
socket response delivery or domain command acceptance. A dropped future releases
its in-flight count and increments cancellation rather than completion/duration.
Latency percentiles, per-route histograms, transport accounting and the remaining
formation/trace instruments are still required separately.

`loopback_diagnostics_expose_real_health_and_runtime_disable_binds_nothing`
now verifies an actual worker's initial CLI/client summary is counted; repeated
scrapes leave that count unchanged; real Unix client 401/404/405 responses add
exactly three completions/rejections; and no supplied hostile path appears in
metrics. The cancellation test drives the actual middleware/flow-control future
into a waiting handler, aborts it, and checks released occupancy and distinct
cancellation. This latter test is a middleware boundary, not evidence that
every client disconnect cancels an admitted operation. A unit test also covers
5xx classification, saturation and duration exposition. An initial compile
check exposed overly narrow module re-export visibility, corrected before the
passing tests below.

Worker all-target suites **passed** sequentially against this dirty-worktree
checkpoint and shared `target/debug` paths: observability 160 tests (111 library,
21 binary, 28 integration); observability plus fault fixtures 162 (111/23/28);
default 149 (111/18/20). Scoped Clippy with warnings denied passed for all three
configurations. Commands:

```sh
cargo test --locked --offline -p orishu-worker --features observability --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The pinned Prometheus 3.5.0 harness **passed** with the expanded twenty-three
series and unchanged three alert rules. It parses the actual worker response
under the new 6 KiB bound, verifies the positive client-completion counter from
real operator requests, performs a real scrape, stops the scraper while
authenticated lock/unlock continues, and requires new ingestion after restart:

```sh
make test-worker-prometheus PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool PROMETHEUS=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

The catalogue, parser harness, protocol guide, operator manual/story, task and
roadmap now agree on this partial surface. Broader M4 observability gates and
the intermittent readmission-convergence failure remain open. Full-workspace
tests, independent-process formation journeys, packaged release checks and
enabled/disabled overhead measurements were not rerun; the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

The three membership dependency-purity tests also **passed** through
`cargo test --locked --offline -p orishu-membership --test dependencies --quiet`.
Final formatting and scoped whitespace checks passed. The request exposition
regression renders maximum integer values to enforce its fixed output
contribution. An earlier documentation check passed 104 Markdown files, but the
final repository-wide recheck **failed** after concurrent Kagami ADR additions:
ADR 0020 links to missing `tasks/kagami/define-composed-object-execution.md`,
ADR 0021 to missing `tasks/kagami/implement-particle-emitters.md`, and ADR 0022
to missing `tasks/kagami/implement-kagami-viewport-workflows.md`. Those unrelated
files were not changed by this increment. The latest docs result is failed,
not superseded by the earlier green result.

## Readmission failure reproduced with role evidence — 2026-09-07

The diagnose reproduction phase ran a finite batch of at most ten unchanged
`--readmission-only` journeys, stopping at the first failure. Trials 1–8
**passed**; trial 9 **failed** at the original “readmission retains dead history
and three live identities” assertion. Trial 10 was **not run**. The shell loop
ended through `break`, so its zero exit status is not a passing test result.
No runtime fix, diagnostic logging, harness assertion or deadline change was
made during this batch.

The worker and CLI were rebuilt with ordinary default features:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --readmission-only
```

The repeated command used `target/debug` throughout; no competing feature build
replaced either executable. The dirty-worktree base was
`4d6cfba4021e7feeb988c525c7f6fac03150a2ff`, not a clean revision. Executable
SHA-256 values identify the actual build:

| Executable | SHA-256 |
| --- | --- |
| `target/debug/orishu-worker` | `18800b179888fd1310bf010407a6b6128fef8b03c32c4cc2bbe1272a3e2225c3` |
| `target/debug/orishuctl` | `7d6f9cf179f3cce89570ef62b1456dca63fc58e399d6d4b23a8959d6d5f6d478` |

Private failure artifacts are retained at
`/tmp/orishu-formation-failure-nyk0z35g`. Unlike the original failure, this run
activated the bounded public-list role projection. Its post-assertion reads
reported:

| View | Original / second introducer | Departed identity | Readmitted identity | Unexpected records |
| --- | --- | --- | --- | --- |
| A | Both Alive, expected certificate bindings | Dead, expected binding | Absent | 0 |
| B | Both Alive, expected certificate bindings | Dead, expected binding | Alive, expected binding | 0 |
| C | Both Alive, expected certificate bindings | Dead, expected binding | Alive, expected binding | 0 |

All three processes were alive before failure cleanup. These sequential reads
are not an atomic snapshot, but distinguish a missing record at A from a
wrong-liveness or wrong-certificate explanation of its summary. They do not
show whether A received and rejected the record or never received it. Ordinary
logs contain listener startup only. The original ten-second assertion remains
a failed acceptance check, not a demonstrated production convergence SLO.

The next diagnostic experiment should distinguish these ranked predictions:

1. Delayed reconciliation: a bounded observation after the failed assertion
   eventually sees the exact new identity at A, without any operator mutation.
2. Stale-session delivery failure: bounded session/reconnect evidence shows
   repeated rejection or absence of a usable route after C changes identity.
3. Membership rejection: the missing record reaches A, but a finite merge
   diagnostic identifies the rejected identity/certificate/version condition.

Collect only the evidence needed to discriminate those cases; preserve the
original failed result even if later reads converge. A post-failure observation
window must be explicitly bounded and cannot become a longer pass deadline.
No hypothesis is established yet, and the full process journey and M4 remain
open. The discovered count-only readmission success check also needs exact
identity/fingerprint/liveness assertions across all three views; that separate
validation improvement must not be presented as a fix for this failure.

The build succeeded with the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning. Rust suites, optional-feature matrices, release
checks and the full crash/restart journey were not rerun in this investigation.

## Bounded late observation and exact readmission views — 2026-09-07

The next diagnose experiment adds read-only evidence after a failed assertion,
not a longer pass deadline. `capture_readmission_failure` preserves the existing
three-worker role projection. Only `--readmission-only` then waits twenty
seconds and captures one more public `ls` response per worker. The two sets
are labelled `readmission-diagnostic` and `readmission-late-diagnostic` and
remain in the sixteen-entry evidence tail even when the CLI records its own
six responses. The original assertion is re-raised regardless of later state.

Each read retains the existing three-second subprocess timeout and bounded
projection. Thus the diagnostic mode permits six such waits plus twenty
seconds (38 seconds plus bounded local work) before failure cleanup. Normal
and fault process journeys still take only the initial three reads. Failed,
malformed and unavailable reads never become evidence of absence or recovery.
No runtime logging, new worker endpoint, mutation or recovery attempt is used.

Three helper tests were written before this capture wrapper existed and failed
with the expected missing-function errors; they pass with its implementation.
They verify both snapshots surviving the real bounded evidence container,
later appearance without losing initial absence, no repeat/wait in normal
mode, and a fixed six reads even when every lookup fails. Their wait is mocked;
they do not establish real twenty-second process observation evidence.

A finite batch of ten `python3 scripts/check-formation-cli.py --readmission-only`
runs **passed** against the unchanged default binaries identified in the
[preceding reproduction record](#readmission-failure-reproduced-with-role-evidence--2026-09-07).
No failing run activated the late capture. This does not establish or refute
delayed reconciliation, stale-session delivery failure or record rejection.
The original and reproduced failures remain unresolved; further diagnosis
must obtain evidence at those boundaries rather than count these passes as a
runtime correction.

Separately, the public journey's readmission success condition was strengthened.
The original four-record/three-live summary poll is unchanged. A second public
list poll uses only the original polling budget's remainder and requires all
three workers to hold the exact expected identity/certificate/liveness map.
The old identity must remain Dead, the readmitted identity must be Alive with
its retained certificate, and neither duplicate labels nor aggregate counts
can hide duplicate/unknown IDs or wrong bindings. As elsewhere in this harness,
the polling cutoff is checked between bounded CLI reads, not an assertion of
hard real-time completion within ten seconds.

Three additional predicate tests were written before the predicate existed,
failed for that missing function, then passed. They cover reordered valid
records, count-preserving wrong/duplicate IDs, wrong certificates, swapped
liveness, missing/extra records and malformed input. These are harness
regressions, not a membership-core bug reproduction.

After the exact-view change, the full ordinary public journey **passed**:

```sh
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
python3 scripts/check-formation-cli.py
make docs-check
```

The helper suites passed 20 and six tests respectively. The real process run
includes standalone CLI authorization checks, A-to-B-to-C handoff, policy,
leave/receipt replay, exact readmission views, crash/restart/readmission and
bounded cleanup; it is not the diagnostic subset. Documentation validation
passed 111 Markdown files and scoped whitespace checks passed. The worker
manual describes both the stronger assertion and diagnostic-only wait.
No Rust source, executable, wire profile or worker deadline changed. Rust
suites, optional telemetry, release matrices and separate fault-process
journeys were not rerun. These results close the count-only assertion gap,
not the intermittent regression or full N-FORMATION/M4 acceptance.

## Late reconnect replacement evidence and diagnostic cleanup — 2026-09-07

The readmission investigation first ran twenty `--readmission-only` journeys
against a default-feature worker with temporary `[DEBUG-readmission-round]`
probes. All twenty **passed**. The probes recorded owner counts, open/retained
connection counts, outstanding reconciliation round number/exchange count,
target liveness and remaining timer duration, plus finite registry errors and
core diagnostic discriminants. No identity, credential or payload was logged.
Logs used the existing bounded harness failure tail; no failed process run
captured them or activated the later snapshot. Instrumentation can perturb
scheduling, and these passing repetitions neither explain nor resolve the
original failure.

All temporary probes and the registry connection-count helper were removed,
their absence checked, and ordinary worker/CLI binaries rebuilt. No production
behavior or timeout change remains from the experiment. Rather than another
unchanged repetition batch, the next diagnostic fixture should control the
reconciliation/leave timing at an existing runtime/wire seam and observe the
outstanding round's disposal and subsequent membership delivery. It must
reproduce the missing-record behavior before being treated as an explanation
of the public-process failure; a plausible timing mechanism is not a cause.

Separately, the existing but previously unmapped runtime test
`replacement_fences_successful_join_reconnect_and_preserves_new_operation`
was inspected and **passed** in a named focused run and the default worker
suite. It holds an actual successful pinned TLS/application handshake for a
pending-join reconnect, changes the source lifecycle through the real owner,
and prepares a new operation before delivering the old result. The held
connection is still open before delivery and must become `LocallyClosed`
within one second afterward, without the fixture closing it. The new operation
remains Connecting; the old operation's status, replacement generation and
summary are unchanged; the target still has only its own member. Dropping the
new preparation then yields its own FailedBeforeAdmission cancellation result.
The whole fixture is bounded to ten seconds and joins normal shutdown tasks.

This fills the inventory's non-shutdown late-success pending-reconnect case.
Lifecycle replacement is deliberately staged below the public API's busy
guard; it does not authorize public leave during unresolved admission. It is
real wire/runtime evidence inside one process, not a readmission-regression
fix or proof of every stale-IO combination. The overall stale-IO row remains
partial until its remaining completion-class mapping is reconciled.

After diagnostic cleanup, verification **passed**:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --lib replacement_fences_successful_join_reconnect -- --nocapture
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

The default worker suite passed 150 tests (112 library, 18 binary, 20
integration); the focused run passed one named test. Documentation validation
passed 111 Markdown files and scoped whitespace checks passed. The build
retains the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.
The complete ordinary crash/restart journey, fault-process scenarios,
optional telemetry and workspace-wide Rust suites were not rerun after this
cleanup. N-FORMATION and combined M4 acceptance remain incomplete.

## Controlled departure round and readmission budget — 2026-09-07

`peer::readmission_timing_tests::departed_reconciliation_round_can_delay_learning_a_readmitted_identity`
provides a controlled counterexample to the old ten-second readmission
expectation. It uses a real production membership owner, QUIC/mTLS sessions,
application handshake validation, serialized messages and the normal
five-second owner reconciliation cadence and ten-second core round timeout.
No worker fault flag, runtime scheduling override or new dependency is added.

The fixture starts with already-admitted test models, completes a real pull
to clear startup routing uncertainty, then starts another ordinary owner
reconciliation command one second after that periodic round. The chosen peer
receives the actual request and withholds its reply. Authenticated gossip and
a correlated Ping/Ack establish all three original IDs as Alive before that
peer sends its authenticated self-departure. The owner closes its connection
and marks the old identity Dead, but retains the outstanding core round.

The surviving test peer connects through the same production handshake and
answers probes without piggyback gossip, deliberately requiring anti-entropy
repair. It holds a new record using the departed peer's certificate and a
distinct ID. At ten seconds the owner still has only the three original
records and no new identity. After timeout and the next cadence, a new real
PullReq receives the missing record through a validated PullReply. The owner
then has four records, three live identities, the expected certificate binding
and the old Dead record. The completed focused run measured about 13.985 seconds
from surviving-peer connection to repair, within the seventeen-second
observation budget. The whole fixture, including normal shutdown, is bounded
to forty seconds.

This is an authenticated wire/owner timing fixture, not three worker processes
or a new admission procedure: models and the readmitted record are arranged
as test-peer source state; the actual CLI assignment/recovery path remains
covered by the process harness. It proves a possible missing-record timing
window under the existing contracts. It does not establish which exact
internal events occurred in the two historical process failures, whose
internal timing was not captured.

An initial partial-view version passed at about 13.990 seconds. Strengthening
the setup to require three live original records exposed a fixture failure
before departure: its test peer answered Pings but did not refute startup
suspicion. The adapter now applies the actual membership core's refutation
transition and emits its Alive/ACK with the resulting incarnation. The
strengthened focused test and default worker suite passed afterward. The
initial setup failure is not attributed to the worker runtime.

The harness now uses `READMISSION_CONVERGENCE_SECONDS = 10 + 5 + 2`: one
outstanding round timeout, one owner cadence and scheduling margin. Summary
counts and exact ID/certificate/liveness checks share that polling budget;
the second check gets no fresh window. Other scenario deadlines are unchanged,
and post-failure capture still cannot turn a failed assertion into success.
The owning peer document, worker manual, task and roadmap distinguish this
evidence-based assertion correction from a runtime fix or production SLO.
The former ten-second failures remain recorded rather than overwritten by
passing retries. Failures under the corrected budget require investigation.

Verification passed:

```sh
cargo test --locked --offline -p orishu-worker --lib departed_reconciliation_round -- --nocapture
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
make test-formation
cargo fmt --all -- --check
make docs-check
```

The default worker suite passed 151 tests (113 library, 18 binary, 20
integration). The full Make journey passed its six evidence-helper and twenty
HTTP/helper tests, standalone authorization checks, public handoff, policy,
leave/receipt replay, readmission, crash/restart/readmission and cleanup. This
was the complete journey, not the diagnostic subset. Documentation checks
passed 113 Markdown files after concurrent unrelated documentation additions;
scoped whitespace checks passed. The existing `proc-macro-error2 v2.0.1`
future-incompatibility warning remains. Optional telemetry, fault-feature
process journeys, release exclusion and full-workspace Rust validation were
not rerun; final N-FORMATION and M4 remain open.

A second `python3 scripts/check-formation-cli.py` execution also **passed** the
complete ordinary journey with the revised budget. It used the same rebuilt
default binaries, not a feature or fault build. Final documentation and scoped
whitespace checks passed after the task/roadmap reconciliation.

## Still required beyond this matrix

Keep N-FORMATION's trust/bootstrap, unsupported-command honesty, configuration,
identity/credential persistence, secret handling, compatibility, platform and
final workspace validation criteria. Complete the current acceptance ledger
and synchronize protocol/manual/roadmap statuses before closing the task.

P-OBSERVABILITY slices 1–3 and formation P-OBS-DOCS remain incomplete. The
local feature-gated listener, health gauges and owner/process supervision are
partial delivery; full feature/security/probe matrices, Prometheus integration,
formation instrumentation, cross-peer traces and tested operator recipes are
mandatory for combined M4. This audit does not defer or substitute for them.
