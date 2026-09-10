# Implement the operational cluster-formation PoC

Status: **N-FORMATION accepted for source-built Linux; combined M4 handoff open** —
formation criteria and final process validation pass at their documented
boundaries; telemetry and its operator handoff remain incomplete

Roadmap package: **N-FORMATION**; companion milestone: **M4**

Decisions: [ADR 0013](../adr/0013-cluster-formation-and-node-identity.md) and
[ADR 0017](../adr/0017-worker-operational-observability.md)

Accepted companion decision: [ADR 0025](../adr/0025-version-peer-trace-context-propagation.md)
selects profile 5 only, with coordinated rebuild/restart and no profile-4
fallback. Profile 5 is now active with
[three-worker causal receipt and compatibility evidence](cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09).
The separate logging decision is also accepted: structured stdout by default,
bounded asynchronous delivery and a replaceable worker-owned sink adapter;
Unix-datagram/vendor sinks remain future work.

Design: [Orishu runtime design](../orishu-runtime-design.md),
[peer protocol](../protocol-p2p.md),
[client protocol](../protocol-client.md), and
[worker/cluster administrator stories](../user-stories/orishu/README.md)

Prerequisite:
[the implemented sans-IO membership core](implement-membership-model.md),
including its accepted liveness-gossip merge correction. Verify the recorded
regression suite when integrating; completed core work is not a new prerequisite.

## Outcome

An operator can start three real `orishu-worker` processes, explicitly join
them into one authenticated formation, and inspect and control that formation
with `orishuctl`. The workers exchange real peer traffic and drive
`orishu-membership`; no workload is loaded and no scientific computation is
performed.

The combined **M4** outcome adds an operator-verifiable monitoring journey:
scrape each worker, observe truthful process probes, and receive a sampled
client-to-peer trace using the documented secure configuration. Formation must
remain correct with telemetry compiled out, runtime-disabled or unavailable.
This companion outcome is still open; it does not reopen accepted formation
work or introduce scientific execution.

## How to use this task

This records accepted formation work and an unfinished M4 companion handoff,
not a greenfield implementation brief.

| Read first | Purpose |
| --- | --- |
| [Open M4 work](#open-m4-work-selection) | Remaining deliverables, their owners and decision gates |
| [M4 checklist](cluster-formation-m4-checklist.md) | Finite requirement/evidence map; deployment choice accepted and scoped collection verified, overhead/final acceptance pending |
| [Scope and companion ownership](#roadmap-placement-and-dependencies) | Separate formation, telemetry and operator-documentation obligations |
| [Operator journey](#m4-operator-journey) and [capability/runtime matrix](#m4-capability-and-runtime-matrix) | Observable behavior the companion delivery must demonstrate |
| [Acceptance criteria](#acceptance-criteria) and [conformance ledger](cluster-formation-conformance.md) | Completion contract versus recorded evidence and its applicable checkpoint |

The original preflight decisions, slices 1–6 and delivered increment briefs
below are retained reference material, not pending work. Consult them only
when a selected change affects their contract. The separate
[integration record](cluster-formation-integration-record.md) preserves history.

This task owns outcomes and acceptance; ADRs own architectural decisions and
the client/peer protocol documents own wire contracts and exact limits. Record
new test results in the conformance ledger rather than extending this task's
historical narrative. A documentation review neither verifies the current
implementation nor authorizes starting the next implementation increment.

Use the [increment checklist](#selecting-and-closing-an-audit-increment) to
agree one bounded implementation brief after this review. Neither reviewing
this document nor choosing a candidate accepts a pending ADR or starts execution.

## Current disposition and remaining scope

The [final acceptance disposition](cluster-formation-conformance.md#final-formation-acceptance-disposition--2026-09-07)
maps the non-matrix gates and records refreshed default workspace checks,
release fault exclusion, ordinary three-worker evidence and all thirteen final
process reruns. N-FORMATION is accepted for the documented source-built Linux
scope. The remaining execution work is P-OBSERVABILITY slices 1–3 and the
formation portions of P-OBS-DOCS; combined M4 acceptance stays open.

Preserve these companion increments at their recorded boundaries:

- Local client-service OTLP receipt, disabled/outage behavior,
  [worker mTLS/credential checks](cluster-formation-conformance.md#real-worker-collector-credential-and-mtls-matrix--2026-09-07),
  [saturation/recovery/shutdown](cluster-formation-conformance.md#real-worker-trace-saturation-recovery-and-shutdown--2026-09-07),
  [bounded Linux trust](cluster-formation-conformance.md#bounded-default-linux-collector-trust--2026-09-07)
  and [live delivery/loss counters](#live-trace-delivery-and-loss-diagnostics-increment).
- [Inbound handshake metrics](cluster-formation-conformance.md#inbound-peer-handshake-metrics--2026-09-07)
  cover optional TLS/application outcomes and pre-handshake capacity refusals,
  not outbound transport, volume/latency or every formation instrument. The
  [reliable-exchange catalogue](../orishu-observability.md#reliable-peer-exchange-metrics)
  separately defines request/serve outcomes, duration, partial stream bytes and
  pool pressure, with [scoped conformance evidence](cluster-formation-conformance.md#reliable-peer-exchange-metrics--2026-09-07);
  these do not establish full formation coverage. Separate
  [datagram/pre-pool counters](../orishu-observability.md#datagram-and-pre-pool-traffic-counters)
  cover submission outcomes, payload bytes, pre-validation receives and the
  registered stream-task gate, not outbound dial/TLS or all datagram loss.
  The [outbound dial catalogue](../orishu-observability.md#outbound-dial-and-tls-metrics)
  adds attempt/candidate outcomes and duration plus four-slot pressure; returned
  handshake replies still require owner validation and do not establish admission.
  See the [outbound evidence](cluster-formation-conformance.md#outbound-dial-and-tls-metrics--2026-09-07)
  for tested boundaries and remaining instrumentation mapping.
  [Membership deadline/abandonment counters](../orishu-observability.md#membership-deadline-and-abandonment-counters)
  now distinguish consumed core timers, stale timer input and retry/round
  exhaustion; they do not establish successful probes or convergence latency.
  Preserve the [owner/wire controls and evidence](cluster-formation-conformance.md#membership-deadline-and-abandonment-metrics--2026-09-07),
  including the distinctions between cancelled timers and expiry, and between
  lost-ACK uncertainty and admission refusal.
- The [local Collector recipe](../testing-worker-otelcol.md) and
  [mTLS extension](../testing-worker-otelcol-mtls.md) establish scoped receipt,
  outage and security evidence on loopback. The additional
  [three-worker walkthrough](../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
  adds admission/log correlation and direct metrics/probes, not cross-host
  deployment, credential lifecycle or combined service/container qualification.
- The [local overhead baseline](../testing-worker-overhead.md) is measurement
  evidence, not an accepted budget or a concurrent three-worker result.

The [open M4 table](#open-m4-work-selection) is the remaining-work inventory.
The [conformance ledger](cluster-formation-conformance.md) owns detailed results
and their applicability; the [integration record](cluster-formation-integration-record.md)
preserves history, including superseded gaps and failures. Formation acceptance
applies to the linked checkpoint, not unverified later changes in a dirty
worktree. Retained briefs below are not instructions to repeat completed audits.

In particular, preserve the
[controlled readmission timing correction](cluster-formation-conformance.md#controlled-departure-round-and-readmission-budget--2026-09-07)
and the historical failures it followed. The seventeen-second observation
budget retains exact identity, certificate and liveness assertions; it is a
test-budget correction, not a runtime fix, production SLO or proven explanation
of every earlier failure. Final acceptance rests on the linked complete
validation, not on increasing that budget alone.

| Boundary | Recorded evidence | Disposition and follow-up |
| --- | --- | --- |
| Public formation and introducer handoff | A admits B, B completes automatic catch-up and admits C; exact identities, fingerprints, live views, authenticated status/replay and cross-worker lock/unlock; mapped collision/capacity/policy-ordering, partition/heal, stale-route, datagram, authority and ingress evidence | Accepted at the linked formation checkpoint; preserve regressions and reassess evidence affected by later changes |
| Catch-up and lifecycle fencing | Real receiver completion fenced after leave/ejection/shutdown/session loss/deadline; bounded attempt exhaustion; expired/truncated/cross-snapshot/bad-digest receiver failures and clean retry; post-adoption wire self-ejection and late admitted-peer handshake evidence; companion malformed/partial-baseline HTTP failure/recovery tests | Formation gate accepted; companion HTTP fixtures do not close the remaining P-OBSERVABILITY role/pressure and deployment matrix |
| Leave and ordinary restart | CLI leave/readmission, lost departure announcements with SWIM fallback, discarded public leave response with exact receipt recovery, SIGKILL/suspicion/death and restart with retained credentials/fresh identities; separate excluded-restart evidence | Accepted with final process reruns under the corrected readmission budget; no unspecified Cartesian product of faults |
| Interrupted admission | Profile-4 original-assignment replay; public lost-ACK recovery; issuer/source history loss and Dead/removed/certificate-blocked assignments have independent-process stop evidence; original-issuer ejection covers unavailable accepted history with unchanged issuer identity | Accepted; preserve the recovery mapping and tested stop procedures, with no additional unavailable-history combination currently named as missing |
| Operational handoff | ADR 0017 is accepted; the [observability guide](../orishu-observability.md#implemented-local-surface) and ledger record the finite formation-metrics inventory, process probes and local official-Collector admission/log correlation; the [local Prometheus guide](../testing-worker-prometheus.md) and [mTLS-proxy guide](../testing-worker-monitoring-proxy.md) record scoped scrape/security/pressure evidence | Preserve completed metric, foundation and local correlation evidence. Reviewed overhead and the remaining selected-deployment handoff stay open; neither companion task is complete |

### Remaining reviewable increments

The remaining work is the companion M4 observability handoff. N-FORMATION's
final validation and acceptance are complete at the linked checkpoint.
Retained regression and review briefs below preserve its closure contract;
they are not new implementation increments. M4 remains open until both
companion deliveries pass too.

Preserve the catch-up lifecycle and receiver-fault regressions in the evidence
ledger, together with deterministic incomplete/cancelled-transfer readiness
and the public A-to-B-to-C handoff. Their bounded wire/runtime coverage does
not replace public ejection/exclusion or process-level fault acceptance.

#### Open M4 work selection

Select one bounded increment from this table after mapping the latest companion
evidence. These are work-selection categories, not instructions to implement
all remaining M4 work together. The owning tasks retain the detailed acceptance
criteria; completed formation reviews below are reference material.

The [formation setup prerequisite](../measurements/formation-reliability-2026-09-09.md#approved-policy-verification--2026-09-09)
now has six passing bounded checks at 3/10/30 workers with telemetry omitted
and metrics enabled, under the explicitly approved shared 60-second setup
policy. Earlier short-gate failures remain historical failures. This scoped
check does not qualify overhead or guarantee every full-curve cell will pass.

Trace/log correlation and the selected formation-stage operator recipes are
now [verified at the post-reliability source checkpoint](cluster-formation-m4-checklist.md#current-build-operator-recipe-verification--2026-09-09),
including both deployment styles, real ingestion, security/pressure, alerts,
dashboard queries and incident controls. Their former open implementation rows
are closed for that scope. Preserve their evidence; reassess only affected
checks if subsequent source/profile changes require it.

| Open item | Owner | Decision or input needed first | Bounded exit artifact |
| --- | --- | --- | --- |
| Formation telemetry overhead | P-OBSERVABILITY slices 1–3 | Review retained baseline noise, full-sampling cost/loss, the [shutdown fix's applicability](../measurements/formation-telemetry-2026-09-09.md#shutdown-log-follow-up--2026-09-09) and [per-size budget feasibility](../measurements/formation-reliability-2026-09-09.md#approved-policy-verification--2026-09-09), then freeze the next pre-run manifest. Curve, cost bands and the 90-minute execution budget remain accepted; the setup-policy revision does not enlarge the 30-minute per-size limit | Reproducible omitted-feature/disabled/enabled comparisons with concurrent load at 3/10/30 workers; latency, throughput, CPU, memory, scrape and loss results against reviewed limits. Final acceptance covers the delivered peer-tracing/logging configuration, not an exploratory substitute. |
| Final M4 acceptance | N-FORMATION and companion owners | Resolve the performance gate, identify the delivered source/build/profile and assess any changes since the [passing correctness checkpoint](cluster-formation-m4-checklist.md#post-reliability-regression-checkpoint--2026-09-09) | One final requirement-to-evidence disposition, retaining scoped formation/operator passes, unresolved findings, exact measurement identities and any affected reruns. Do not infer completion of later release/workload obligations. |

The historical [recipe-to-criterion map](cluster-formation-conformance.md#m4-operator-recipe-applicability--2026-09-08)
records the local scrape, secured proxy, Collector, user-service,
rootless-container, dashboard and incident boundaries. Its then-pending
correlation/configuration checks now have the current-checkpoint verification
above. Maintain that distinction when selecting work. Distinguish a missing
combined-deployment assertion from an entirely missing recipe. Published
packages/images, optional Kubernetes and later workload examples do not become
new M4 prerequisites merely because they remain unfinished in P-OBS-DOCS.
Do not move an existing M4 requirement to a later milestone without review.

The subsequent [logging-counter backend check](cluster-formation-conformance.md#logging-counter-prometheus-ingestion--2026-09-09)
closes the named local Prometheus-ingestion gap for the nine logging counters,
including independent enablement, broken output and fresh scraper recovery.
Keep its one-worker backend boundary separate from the three-worker official
Collector receipt and the remaining selected-deployment review.

On 2026-09-09 the operator selected **both existing deployment examples** for
enabled trace/log collection: systemd user services and rootless Podman
containers. Those [bounded collection extensions](cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09)
now pass alongside the original five-mode deployment regressions. Retain them
under the [M4 checklist](cluster-formation-m4-checklist.md), together with the
ordinary-process causal proof. Overhead harness validation, measurements and
final acceptance remain open; curve and budget are accepted in the linked plan.

Profile-5 admission propagation now has [activation evidence](cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09):
client → outbound join exchange → receiving admission spans across the public
A-admits-B, B-admits-C journey, with independent runtime controls and a
feature-omitted peer. Preserve its codec, replay, compatibility and receipt
regressions. This closes the initial cross-peer trace deliverable, not log
correlation, the combined monitoring walkthrough or reviewed overhead.

Both peer-profile and logging-output decisions are accepted, as are the normal
instrumentation cost goal and full 3/10/30-worker curve with up to 90 minutes of
measurement excluding builds. Harness validation and the pre-run manifest freeze
remain implementation work. Decision acceptance does not qualify
performance. Peer propagation was implemented independently of logging,
but final M4 correlation/overhead evidence must cover both delivered contracts.
The decision Q&A itself does not start an implementation increment.

The approved quiet-host measurement was subsequently attempted once. It
completed all three-worker cells but could not reach ten-worker timed load;
thirty workers remain unmeasured. The report's noise and full-sampling cost/loss
findings prevent overhead acceptance. A separate
[shutdown follow-up](../measurements/formation-telemetry-2026-09-09.md#shutdown-log-follow-up--2026-09-09)
fixes reproduced final-event contention and verifies real three-worker
zero-sampling load/shutdown; it does not replace the original failed cell.
Preserve that regression and the now-passing scoped setup checks above; review
the remaining findings and per-size feasibility before rerunning
the unchanged or explicitly revised measurement plan; no automatic retry or
silent budget fitting.

The [formation monitoring runbook](../worker-monitoring-runbook.md) supplies
the read-only peer-loss, unreadiness and missing-telemetry incident procedure.
Its [scoped evidence](cluster-formation-conformance.md#formation-monitoring-incident-runbook--2026-09-08)
is one part of the now-verified operator handoff; the linked current-checkpoint
refresh supplies deployment and correlation checks separately. The dashboard
example is tracked separately below.
Preserve the tested procedure rather than selecting it again as an unspecified
missing runbook.

The [optional formation alerts](../testing-worker-prometheus.md#optional-formation-warnings)
cover the existing owner, admission/catch-up, membership-budget, transport and
capacity signals with finite rule-engine fixtures and live query evaluation.
They remain examples, not production thresholds or notification delivery;
their evidence does not qualify deployment or the separate dashboard example.

The [formation dashboard example](../testing-worker-dashboard.md) has
[recorded acceptance](cluster-formation-conformance.md#formation-snapshot-dashboard--2026-09-08)
for per-target HTML/query behavior and inspected desktop/mobile captures. The
ledger preserves the blank fragment-capture failure and corrected browser helper,
alongside exact commands, checkpoint and limitations. This closes the previously
missing dashboard evidence mapping. It preserves missing-data semantics and graph
navigation without adding a worker API; it does not qualify deployment,
correlation or the combined M4 operator journey.

The [source-built user-service recipe](../testing-worker-user-service.md) has
[scoped systemd evidence](cluster-formation-conformance.md#source-built-user-service-monitoring--2026-09-08)
for five feature/runtime modes, private access, explicit stop/restart and
fixture-crash recovery. It closes that user-service example only; packaged
system/container deployment and trace/log correlation are not implied. Preserve
its no-automatic-restart policy without changing the separate package scaffold.

The [rootless container recipe](../testing-worker-container.md) has
[local evaluation evidence](cluster-formation-conformance.md#source-built-rootless-container-monitoring--2026-09-08)
for private state, five runtime modes, exec probes, loopback namespace isolation
and explicit stop/restart. It closes that local example, not published-image,
Kubernetes or remote-proxy/collector co-deployment acceptance. No automatic
readmission or telemetry-driven restart is introduced.

For overhead, the operator accepted the linked 3/10/30-worker experiment, six
modes and six rounds per size, with 30 minutes per size / 90 minutes total
excluding builds. The p95/CPU normal goal is below 10%, up to 20% is temporary
only and above 25% belongs to troubleshooting. Freeze executable/configuration
identities before the acceptance run. Keep exploratory measurements
distinct from budget acceptance; report all repetitions, drops and failures.
Later trace/log changes require an
explicit assessment of which earlier measurements remain applicable, not an
automatic claim that the local baseline qualifies the final configuration.

Local instrumentation, probe/security evidence and operator recipes can be
reviewed without accepting peer propagation or choosing a logging output.
The [foundation audit](cluster-formation-conformance.md#m4-foundation-evidence-reconciliation--2026-09-07)
now maps scoped evidence for all its named assertions, including the
[downstream proxy increment](cluster-formation-conformance.md#monitoring-proxy-downstream-backpressure-and-expiry--2026-09-07).
Preserve those results and their limits; this does not accept full deployment
security or the remaining companion work above.
Before execution, name the selected assertion, evidence to preserve, exact
validation command and stop condition using the
[increment checklist](#selecting-and-closing-an-audit-increment). This review
does not select or start an implementation increment.

#### Delivered increment: catch-up outcome metrics

The [catch-up evidence](cluster-formation-conformance.md#admission-state-catch-up-outcome-metrics--2026-09-08)
and [catalogue](../orishu-observability.md#admission-state-catch-up-outcomes)
close the last named row of the finite formation-metrics inventory. Twelve
optional counters distinguish owner-authorized jobs, receiver outcomes and
terminal owner decisions. Pages are not attempts; a validated transfer can
still be fenced or abandoned without becoming an adopted baseline.

Preserve the real receiver faults, progressing whole-transfer expiry, active
page cancellation/reuse, lifecycle fencing and HTTP failure/recovery evidence.
The three-worker companion checks adoption counts, leave retention and process
restart reset without replacing its exact membership/operation assertions.
Worker/CLI manuals and both operator story sets describe safe interpretation.
The catalogue remains within 32 KiB, with integer and fractional maximum widths
checked according to their actual types. No retry budget or peer profile changed.

This completes neither P-OBSERVABILITY nor M4. Trace/log correlation, their
pending decisions, formation overhead and the full operator handoff remain in
the open work table. Historical broad instrumentation statements do not create
new mandatory instruments without a named operator assertion and reviewed scope.

#### Delivered increment: registered-session visibility

The [registry evidence](cluster-formation-conformance.md#registered-session-capacity-gauges--2026-09-08)
and [catalogue](../orishu-observability.md#registered-session-capacity-gauges)
close the retained-session visibility row in P-OBSERVABILITY slices 2–3.
Four optional gauges distinguish the total registry budget and its provisional
subset, including outgoing introducer bindings. They are neither live-socket
counts nor membership or readiness authority. Existing limits and pruning
remain unchanged; scrapes read a constant-size IO-owned projection.

The worker manual, scrape instructions and both operator story sets explain
retained entries, availability and safe first checks. Capacity remains visible
for an active empty owner without a peer listener and withdraws on registry
drop; closing transport alone does not erase retained slots. Preserve the
real registration/promotion/refusal, capacity/reuse, owner lifecycle and HTTP
evidence, separately from the earlier inbound task/TLS gauges.

The later catch-up increment covers receiver/owner outcomes separately. Neither
increment closes trace/log correlation, the pending peer-profile decision,
formation-stage overhead or the full operator handoff.
Select remaining work from the open M4 table, not this delivered brief.

#### Delivered increment: proxy downstream-reader evidence

**Delivered:** the [conformance evidence](cluster-formation-conformance.md#monitoring-proxy-downstream-backpressure-and-expiry--2026-09-07)
closes the foundation ledger's **proxy downstream reader** assertion under
P-OBSERVABILITY slices 1–2, with the corresponding P-OBS-DOCS recipe update.
This was missing evidence, not a demonstrated runtime defect. Neither ADR 0025
nor a logging-output choice was needed or accepted. The brief below retains
the completed contract; it does not schedule another implementation.

- **Preserve:** the [secured-proxy recipe](../testing-worker-monitoring-proxy.md)
  and its credential/route isolation, real Prometheus ingestion, upstream
  pressure/recovery and pre-HTTP expiry evidence. Direct diagnostics
  backpressure tests establish a different transport boundary.
- **Question:** when an authenticated monitoring client stops reading actual
  metrics responses, does the proxy bound response generation/buffering,
  release stalled work within its documented timeout contract, and preserve
  legitimate monitoring and authenticated operator progress?
- **Required evidence:** a finite pressure source using
  real worker responses; an observable proxy-side blocked write; explicit
  byte/work and observation budgets; a healthy-reader control; and successful
  capacity reuse before the stalled client is read or closed by fixture
  cleanup. An unread response alone is not proof of backpressure. If the
  fixture cannot establish that boundary, report the missing evidence rather
  than claiming a pass or inflating the metric catalogue.
- **Validation contract:** extend the existing harness and run
  `make test-worker-monitoring-proxy` with the explicit tool paths documented
  in its [executable acceptance](../testing-worker-monitoring-proxy.md#executable-acceptance),
  then `make docs-check`. Record the checkpoint, build/features, actual
  commands, failures and limitations in the existing conformance ledger.
- **Documentation and stop condition:** reconcile the proxy recipe and its
  configuration comments; review the worker manual and both operator story
  sets for affected claims. Return either scoped passing evidence or one
  demonstrated defect/missing assertion for review. Do not change production
  limits merely to make a fixture pass, add worker-native remote TLS, qualify
  an untested deployment, or close other M4 rows through this increment.

#### Completed review: M4 foundation evidence

**Audit completed:** the [assertion dispositions and focused reruns](cluster-formation-conformance.md#m4-foundation-evidence-reconciliation--2026-09-07)
now separate covered foundation behavior from concrete remaining cases. The
[proposed increment](cluster-formation-conformance.md#proposed-next-increment-diagnostics-response-backpressure)
has since gained [real diagnostics backpressure evidence](cluster-formation-conformance.md#diagnostics-response-backpressure--2026-09-07).
The later direct method/error and pre-HTTP proxy increments are linked in the
ledger. The audit is not scheduled to repeat; the later downstream proxy
increment closes its last named evidence gap at the documented boundary.

Maintain the existing assertion table against P-OBSERVABILITY slices 1–2 and
their acceptance criteria. For affected assertions, record the contract, tested
boundary, applicable checkpoint/features/transport, disposition and exact next
action. Use `covered`, `needs rerun`, `missing evidence` or
`missing implementation`; a documentation review can assess recorded evidence
but cannot claim a fresh test pass. Historical missing-exporter statements do
not override the later local Collector evidence. Keep workload/storage/release
obligations with their owning milestones, without deferring an M4 requirement.

Foundation evidence and its affected documentation are now reconciled at that
scope. Formation instrumentation, correlation, overhead and the full operator
handoff retain their separate M4 gates.

#### Regression evidence to retain

1. **Corrected readmission timing gate.** The controlled fixture
   and documented seventeen-second process observation budget supersede the
   unsupported ten-second expectation. Preserve the exact-record check and
   historical failures in final process reruns. A failure under the corrected
   budget needs the bounded investigation below; do not repeat unchanged
   diagnostic batches solely to erase historical failures. This regression
   does not block independent observability work.
2. **Completed slow-input and ingress audit.** The
   [case-to-contract closure](cluster-formation-conformance.md#ingress-pressure-case-to-contract-closure--2026-09-07)
   now maps the required row to passing evidence, including exact serialized
   output-cap coverage. Retain these regressions for final acceptance. The
   following links preserve its audit trail, not a new missing-work inventory. Use the
   [ledger's pressure checklist and subsequent evidence](cluster-formation-conformance.md#next-implementation-combined-slow-input-and-ingress-pressure).
   First reconcile any existing work with its assertions; add only missing
   evidence or a demonstrated production fix. The original “next” heading is
   not evidence that its cases are still missing. Current documented gaps
   are reconciled by the closure above, not another blanket pressure matrix.
   [HTTP/2 response delivery](cluster-formation-conformance.md#http2-response-delivery-deadlines--2026-09-07)
   now has absolute expiry evidence for withheld stream/connection credit and
   cancellation/completion controls; preserve its stated transport boundaries.
   [Inbound DATA/window-update refusal](cluster-formation-conformance.md#inbound-http2-flow-control-boundaries--2026-09-07)
   now has scoped wire/server evidence; preserve its documented test isolation
   and do not mistake it for a TLS-handshake or operator journey. Absolute input assembly
   now has [watchdog and real Unix/TLS expiry evidence](cluster-formation-conformance.md#absolute-http2-assembly-watchdog--2026-09-07);
   preserve its controls and the separately recorded silent-client expiry.
   [Client TLS pre-HTTP evidence](cluster-formation-conformance.md#client-tls-pre-http-deadline-evidence--2026-09-07)
   now covers silent/trickled handshake expiry and independent authenticated
   control. Preserve its documented five-second detection/ten-second handshake
   deadline ordering; it is not a full-capacity TLS saturation claim.
   This is an acceptance audit, not a new scheduling or health authority. Preserve the recorded
   collision/recovery, local-capacity, policy-ordering and process partition tests.
   Use the bounded completion contract below rather than repeating the already
   recorded peer saturation and slow-body tests.

The [stale-IO cross-class audit](cluster-formation-conformance.md#stale-io-cross-class-audit--2026-09-07)
and [late admitted-peer handshake regression](cluster-formation-conformance.md#late-admitted-peer-handshake-completion--2026-09-07)
also remain accepted regression evidence at their recorded boundaries. Use the
[open M4 work table](#open-m4-work-selection) as the single work-selection list;
the retained briefs below explain completed increments or conditional audits,
not additional prerequisites. Local exporter receipt, security and pressure
results remain scoped evidence, not cross-peer tracing or full M4 acceptance.

#### Live trace delivery and loss diagnostics increment

**Owner:** P-OBSERVABILITY slices 2–3, with the corresponding P-OBS-DOCS
catalogue and troubleshooting updates. This is a delivered increment's retained
contract, not an open work item. Its scoped validation is recorded in the
conformance ledger.

**Original gap:** shutdown-only queue/delivery totals did not establish live
visibility during collector stalls. The worker now publishes twelve bounded
counters through the existing metrics route; maximum-value/parser and real
stalled/recovered scrape checks cover this increment. This is not acceptance
of trace-loss dashboards or the whole companion task. The
[Prometheus extension](../testing-worker-prometheus.md#ingest-trace-counters-through-prometheus)
records ingestion and fresh values after collector recovery. Consult the
[current catalogue](../orishu-observability.md) for its series budget; historical
ingestion results retain the catalogue tested at their checkpoint in the ledger.
See the [live-counter evidence](cluster-formation-conformance.md#live-trace-delivery-and-loss-metrics--2026-09-07)
for exact commands, feature coverage and limitations.

- Publish bounded aggregate delivery/loss diagnostics through the existing
  optional `/metrics` surface. Keep sampling decisions, queued records,
  collector-reported acceptance/rejection, uncertain delivery failures and
  shutdown abandonment semantically distinct; none proves domain acceptance.
- Specify finite series, types, reset/lifetime rules, units, cardinality and
  worst-case exposition size in the owning catalogue. Scrapes must not wait
  for exporter progress, drain its queue or recursively produce client spans.
- Preserve feature independence: tracing alone opens no diagnostics listener;
  enabling metrics does not enable tracing. Document absent instruments versus
  measured zero when tracing is omitted or disabled.
- Extend the existing real-worker saturation fixture to scrape during a held
  export and after recovery, while retaining its authenticated mutation and
  bounded shutdown assertions. Validate exposition with the pinned Prometheus
  tooling, including maximum counter values and the applicable feature modes.
- Update the configuration/worker manual and telemetry-loss troubleshooting
  instructions with the tested behavior. Record exact commands, checkpoint,
  results and limitations in the ledger; do not promote unrun checks to passes.

**Exit:** the catalogue, focused live-scrape/pressure evidence, relevant feature
checks and `make docs-check` pass. Report any remaining parser or process gap
explicitly and stop for review of the next increment. No peer profile change,
new listener, native trust-store port, log-correlation implementation, workload
instrumentation or release packaging belongs to this increment. It does not
close M4's cross-peer trace, overhead or full operator-handoff gates.

For each remaining scenario, maintain a current evidence row with checkpoint,
named test/harness case, production boundary, exact command, result and
limitation. Use `not run`, `failed` or `passed` explicitly; historical reports
remain historical until rerun. Derive convergence deadlines from configured
probe/suspicion/reconciliation bounds and record them with the scenario.
Increasing a deadline is not a runtime fix or a measured production SLO.

### Completed review: final formation acceptance

**Completed:** the linked acceptance disposition and final process batch are
the result of this review. The procedure below is retained for evidence review,
not scheduled as the next task.

**Outcome:** one finite disposition of N-FORMATION, separate from the still-open
M4 telemetry handoff. This is an evidence/documentation review; execution of the
validation plan or implementation of a discovered gap is a subsequent task.

1. Map each N-FORMATION acceptance criterion to its current ledger evidence.
   Include the non-matrix gates: dependency purity, credential bootstrap,
   supported transports, release exclusion of fault controls, operator manuals
   and repository validation. Record an exact missing assertion rather than
   “more conformance” when evidence is insufficient.
2. Identify the intended acceptance checkpoint and a finite list of commands
   requiring a rerun. Record which existing results already apply, which are
   historical, and which changed production boundaries invalidate earlier
   evidence. A docs-only edit does not itself invalidate runtime evidence.
   Preserve every failure and the corrected readmission timing regression.
3. State the supported platform/build/transport scope and remaining manual
   checks. Untested platforms and unpublished release artifacts remain
   unsupported, not an implied requirement to port or package the PoC.
4. Produce a closure recommendation or a bounded follow-up with its owning
   contract, missing assertion, commands and stop condition. Keep
   P-OBSERVABILITY and P-OBS-DOCS dispositions separate; pending ADR 0025 cannot
   be accepted through a formation completion claim.

Use one final-acceptance table in the conformance ledger with columns for
criterion, evidence link, applicable checkpoint/build/transport, disposition,
and remaining action. Dispositions are `satisfied`, `needs rerun`, or `gap`;
they summarize evidence applicability, not a new test result. Any excluded
criterion needs a separately reviewed scope change. Link the detailed test
results instead of duplicating their history in this task.

The review handoff must identify the exact next action: a finite validation
batch if only reruns remain, a bounded implementation brief if a required
assertion is missing, or a completion recommendation if all criteria are
already satisfied. Record N-FORMATION, P-OBSERVABILITY for M4, P-OBS-DOCS for M4,
and combined M4 separately. Do not turn the last recommendation into an
implementation or validation run during this documentation review.

**Exit:** the review accounts for every formation criterion and leaves either
a justified completion recommendation or an explicit finite checklist. Do not
invent additional compound-fault permutations, rerun unchanged suites
indefinitely, or start N-CLUSTER as part of this review.

### HTTP/2 assembly-deadline increment

The ledger records a concrete
[active partial-frame counterexample](cluster-formation-conformance.md#active-http2-frame-deadline-counterexample--2026-09-07),
not just a missing test mapping. The
[watchdog follow-up](cluster-formation-conformance.md#absolute-http2-assembly-watchdog--2026-09-07)
now records the implementation and scoped expiry/control regressions. The
brief below preserves this increment's acceptance requirements; it is not an
instruction to reimplement it. Response-delivery and inbound flow-control
evidence are linked in the retained ingress regression above; their final
case-to-contract audit remains distinct from this completed scoped implementation.

- **Contract:** the client protocol's formation ingress limits. Specify an
  absolute incomplete-frame and logical header-block assembly budget, its
  start/reset conditions and expiry behavior before implementing it. Traffic
  progress must not silently reset an absolute budget. Do not select an
  arbitrary maximum connection age as a substitute.
- **Scope:** enforce that contract at the responsible client transport
  boundary. Preserve HTTP/1.1 limits and healthy HTTP/2 multiplexing; record
  which assertions apply to Unix and certificate-verified TLS modes.
- **Evidence:** turn the diagnostic survival assertion into server-driven
  expiry evidence; cover fragmented input, a header block continued across
  frames, unrelated active traffic, legitimate completion within budget and
  capacity reclamation. Retain the existing silent-client regressions.
- **Exit:** the focused regression command recorded in the linked ledger,
  the affected transport tests and documentation checks pass. Record exact
  deadlines, feature/transport coverage and a bounded failure artifact. The
  historical diagnostic's passing result is reproduction evidence, not proof
  that the ingress requirement passes.
- **Non-goals:** HTTP/2 withheld-response-credit deadlines, the rest of the
  pressure matrix, membership changes, new public APIs and telemetry export.
  Track their acceptance in the corresponding ledger rows; exclusion from
  this increment does not mean they are still unimplemented.

### Conditional investigation: a new readmission failure

The controlled timing follow-up above has completed the initial investigation's
test-budget correction. The checklist below remains the procedure for any new
failure under that budget, not an instruction to restart the old investigation.

Review one bounded investigation, not a restart of the formation programme.
Use the [recorded failure and diagnostic follow-ups](cluster-formation-conformance.md#admission-baseline-condition-audit--2026-09-06)
as the starting evidence. Do not infer the cause from differing member counts
alone or treat a worker's health probe as proof of membership convergence.

- **Trigger and question:** a new full public journey fails its exact-view
  assertion under the corrected observation budget. Why did surviving views
  fail to converge after voluntary leave and same-certificate readmission?
  Historical ten-second failures alone do not trigger another investigation.
- **Before execution:** record the worktree checkpoint, feature build,
  configured convergence bounds, selected diagnostic command and finite trial
  budget. Reuse existing diagnostic subsets and bounded failure capture before
  adding instrumentation. Preserve identity redaction and process cleanup.
- **Exit artifact:** retain every trial's result and identify either a supported
  cause with a proposed bounded fix/regression test, or an unresolved finding
  with the missing evidence and one proposed next experiment. Exhausting the
  trial budget ends this investigation increment, not the acceptance gap.
- **Fix acceptance, if separately selected:** establish the failing behavior at
  the boundary responsible, verify the correction there, then rerun the full
  public handoff/leave/readmission/crash/restart journey. A shortened diagnostic
  or a larger timeout cannot substitute for that journey.
- **Non-goals:** new recovery authority, automatic restart/readmission,
  unbounded repeated runs, additional public diagnostic APIs, or expanding the
  pressure matrix without evidence linking it to the failure.

This is a conditional procedure, not the default next increment. Reviewing
this task does not start an investigation or authorize implementation.

### Selecting and closing an audit increment

Before implementing another case, turn one remaining ledger gap into a bounded
checklist: required behavior, owning protocol section, existing evidence,
missing assertion, supported transport/feature configuration, and exit command.
Distinguish a missing evidence mapping from a missing test or a demonstrated
implementation defect. An audit may finish with documentation alone when the
existing evidence already establishes the requirement at the right boundary.

The next review should produce one short increment brief containing:

- the exact open ledger row or non-matrix acceptance criterion and its owning contract;
- the existing evidence to preserve and the specific missing assertion;
- the bounded implementation/test scope, or an evidence-only mapping if enough;
- the exit command, time/resource budget and required failure artifact; and
- the affected operator instructions and explicit non-goals.

For an instrumentation increment, identify the responsible adapter and exact
event being counted or timed before naming instruments. Distinguish transport
failure, timeout, local capacity refusal and lifecycle cancellation from domain
rejection. Count transport attempts separately from domain outcomes; replay of
an accepted assignment is not a new admission.
Specify feature/runtime-disabled behavior, finite series and label sets, units,
reset semantics and the worst-case exposition bound. Acceptance includes a
successful control alongside the failure case, real adapter evidence and
scrape/parser validation when metrics change. Update the owning catalogue and
affected operator examples together; do not rewrite historical series counts
as though the new catalogue had been tested at an older checkpoint.

Keep a finite formation-instrument coverage inventory in P-OBSERVABILITY's
evidence, using its slice-3 scope: admission, SWIM/gossip/anti-entropy,
decode/authentication failures, transport volume/latency and bounded queues.
For each required assertion, name its owner/event, existing catalogue entry
and evidence, or the exact missing instrument/test. Separate `covered`,
`needs rerun`, `missing implementation` and `missing evidence`; neither a broad
category name nor a nearby passing counter closes the row. Any deferred M4
requirement needs a reviewed scope change, not an unsupported placeholder zero.

For durations, define the start, terminal outcomes and whether retries or
cancelled attempts contribute. For transfer volume, define the measured layer,
direction, framing and partial-transfer accounting; bytes accepted by a local
transport do not prove remote receipt or domain acceptance. These definitions
belong in the owning catalogue, not a second task-local metric schema.

Do not select all remaining increments as one implementation task. An evidence
audit ends with a mapped result or a concrete missing case; implementation of
that case is the next reviewable decision, not an implicit follow-on.

The logging-output refinement in ADR 0017 and peer-profile ADR 0025 are accepted.
Implement within those contracts; seek review for a departure rather than
reopening the same choices. Record implementation/test gaps separately from
the remaining performance-budget review. Accepted design is not passing evidence.

For the next implementation review, present that checklist as one selected
increment with its non-goals and stop condition. Select from the remaining
gaps above after reconciling the latest ledger entries. Retained regression
lists do not require repeating an earlier implementation or completing another
ingress audit before independent observability work. If the selected case
reveals a new authority or protocol decision, return that decision for review
before expanding the implementation scope.

For ingress, distinguish HTTP/1.1 header/body and socket-write limits from
HTTP/2 header assembly, stream capacity and stream/connection flow control.
A socket-write timeout does not establish a deadline while a peer withholds
HTTP/2 credit. Map the documented Unix and TLS client modes explicitly; tests
over one mode may support a shared handler contract but do not establish the
other mode's handshake or transport-specific behavior.

For recovery and stale IO, enumerate the unresolved outcomes/completion classes
against preflight area 6. Select combined faults that can change an authority,
generation, retained-history or resource-release decision; do not interpret
“combined faults” as an unbounded Cartesian product. Explain equivalent cases
and exclusions in the ledger. Excluding a required acceptance case needs an
explicitly reviewed scope change, not merely a passing nearby test.

Close the increment when its checklist is mapped and its missing assertions
pass, with limitations recorded. Preserve failures and their resolutions;
retries do not erase a failed result. Stop there for review of the next
increment. For M4 closure, assess which accepted formation results remain
applicable and which need a rerun because a production boundary changed. A
docs-only increment requires documentation checks, not another full formation
fault batch. Final acceptance still requires applicable evidence for the
complete matrix and validation commands below, not an endlessly expanding set
of pressure cases.

### Retained acceptance contract: unfinished client IO

The [shutdown follow-up](cluster-formation-conformance.md#shutdown-with-unfinished-clients--2026-09-06)
now records passing process evidence for the shutdown row below and its
single-owner drain fix. Preserve that regression. There is now scoped evidence
for every case below. The
[header-limit follow-up](cluster-formation-conformance.md#pre-authentication-connection-and-header-limits--2026-09-06)
records exact-capacity, expiry and rejection tests plus public peer/control
progress with sixty incomplete heads. Preserve those regressions too.
The [stalled-write follow-up](cluster-formation-conformance.md#stalled-response-writes--2026-09-06)
adds observed real Unix write backpressure, stopped pipeline processing,
authenticated control progress and timeout-driven reclamation. Their
[case-to-contract closure](cluster-formation-conformance.md#ingress-pressure-case-to-contract-closure--2026-09-07)
records acceptance at the stated boundaries; do not reimplement them merely
because the original completion contract remains below.

This contract covers the recorded client-facing cases in the ingress increment; it does
not exhaust every supported protocol's ingress audit. Use the PoC's documented
client transport matrix and retain real authenticated peer traffic. Do not add
unsupported transports merely to complete this audit.
Implementation already present in a dirty worktree is not passing evidence:
inspect and validate it before adding another implementation.

Before changing handlers, record the existing header/body read deadlines,
connection/request capacities, response byte/work limits and shutdown budget
in the owning client protocol. Identify which limits apply before authentication;
a mutation semaphore alone does not establish a bound on unfinished headers
or clients that never read a response. If a limit is unspecified, close that
narrow contract first without changing membership or command semantics.

| Case to retain in the audit | Required acceptance evidence |
| --- | --- |
| Unfinished HTTP headers | Hold incomplete requests before authentication. Prove the configured admission/connection bound and read deadline, while legitimate status requests and peer/control work make bounded progress. Release or expire the clients and demonstrate capacity reuse. |
| Slow response reader | Use a valid, bounded response large enough to encounter actual transport backpressure. Keep the reader stalled while proving response buffering/work stays bounded and unrelated control progresses; verify cancellation or deadline releases capacity. A small response fitting in a socket buffer does not prove this case. |
| Shutdown with unfinished clients | Retain incomplete-header and admitted incomplete-body sockets until the workers exit. Trigger the supported shutdown path and assert a single whole-journey deadline, clean process exits and safe client-socket cleanup. Closing test clients before shutdown is not evidence of server-side cancellation. |

Keep the pressure source finite and record evidence that the intended capacity
or backpressure was actually reached. Test cleanup must run on failure too,
but must not manufacture the successful cancellation being asserted. Preserve
the existing full handoff/churn journey after shared harness changes.

The exit artifact is updated conformance rows with exact bounds, commands,
results and limitations, plus any changed client contract and worker manual.
These cases do not require a new public diagnostic endpoint. Process health
routes, stalled-owner probe behavior and telemetry-outage testing remain the
explicit P-OBSERVABILITY companion gate.

### Interrupted-admission recovery completion checklist

The [operator recovery runbook](../cluster-admission-recovery.md) now specifies
the observation budget, identity checks and stop conditions. Source correlation
and issuer inspection are wired. The recovery matrix, including unavailable
accepted history after issuer ejection, now has mapped evidence. Preserve those
cases during final acceptance rather than treating this retained checklist as
an inventory of missing combinations. Preserve the distinction
between existing pending recovery and an exhausted/unverifiable attempt that
must stop.

The [history-loss audit](cluster-formation-conformance.md#recovery-outcome-and-history-loss-audit--2026-09-07)
now has passing [original-issuer ejection evidence](cluster-formation-conformance.md#original-issuer-ejection-and-unavailable-accepted-history--2026-09-07)
for its last explicit combination: accepted history is cleared while issuer
identity remains, and the source stops unresolved. Preserve that process
regression and the other mapped recovery outcomes during final acceptance;
do not reopen the historical missing-case brief as implementation work.

The [historical checkpoints](cluster-formation-integration-record.md#dated-lifecycle-and-recovery-checkpoints)
cover Dead, removed and certificate-blocked
assignments with the original source alive, plus source loss with and without
a surviving issuer. Do not repeat their implementation as missing work. When
resuming this recovery audit, check unverified combinations and the case-to-test table
before closing this slice. Later checkpoints verify certificate exclusion
across fresh admission after restart and receiving-worker post-adoption
self-ejection over authenticated gossip; preserve their stated scope limits.

If a later change affects this accepted contract, start with preflight area 6,
not a second implementation of the successful join path. Preserve the tested
recovery or stop procedure through supported interfaces.

Treat the recorded correlation, issuer-inspection and replay contracts as the
baseline, not new implementation work. Review their owning protocol sections
before changing behavior. The following is the retained verification procedure,
not an open execution checklist:

1. Map the existing named tests and commands to the cases below. Reuse evidence
   at its actual boundary; a standalone inspection-response test does not prove
   an interrupted-admission journey. Mark missing combinations `not run`.
   Enumerate the finite missing cases before adding tests; distinguish issuer
   record absence, unreachable/failed lookup and a response that fails identity
   or version validation. Do not collapse these into a successful inspection
   or infer non-admission from any of them.
2. Add the missing independent-process journeys, arranging faults through
   explicit test-only controls and asserting the public source/issuer views.
   Preserve original operation, attempt, formation and certificate correlation
   throughout; do not reconstruct an attempt from a worker label.

   | Case | Required operator-visible result |
   | --- | --- |
   | Accepted assignment remains usable after ACK loss | The pending attempt recovers the original assigned ID, without another live identity or a reset retry budget; retain the existing regression. |
   | Assignment becomes dead, removed or certificate-restricted before recovery | Exact retry cannot revive the assignment or bypass restrictions. Issuer inspection and source status lead to the runbook's bounded stop/review path; exercise the distinct restriction cases. |
   | Original issuer is unavailable or has restarted | Retain the verified issuer-loss journey: uncertainty survives retry exhaustion, and a restarted issuer cannot substitute fresh history for the original ledger. |
   | Original source restarts with retained credentials | Old operation history is unavailable and the new standalone identity does not recover the old assignment merely by reusing its certificate. Follow the runbook's stop condition without submitting a fresh join as recovery. |
   | A retained source reference has no verifiable issuer record | A missing record, failed lookup or mismatched response is not proof of non-admission. Preserve uncertainty and stop within the documented observation budget. |

   Supplement the required process journeys with client/CLI wire tests for
   invalid inspection responses. Check echoed request/attempt identity, source
   formation and issuer, and supported schema against the owning client
   contract. Preserve valid unknown-history/wrong-issuer outcomes as such;
   successful inspection transport or CLI exit is not successful recovery.
   Assert bounded failure and no follow-up mutation. An adversarial response
   fixture establishes client validation only, not real issuer retention,
   restart behavior or the complete operator stop procedure.

3. Re-run the complete lost-ACK/handoff/churn journey after any recovery or
   liveness fix. Admission-only and handoff-only diagnostics remain useful
   regressions but cannot close the full journey. Record repeat counts and
   failures without replacing a failed run with an unexplained successful retry.
4. Reconcile the [worker manual](../../apps/orishu-worker/README.md),
   [CLI manual](../../apps/orishu-ctl/README.md) and
   [cluster administrator stories](../user-stories/orishu/cluster-admin.md)
   with exact inspection targets, bounded waits and safe next actions. These
   formation recovery instructions belong to N-FORMATION; telemetry-based
   troubleshooting remains coordinated with P-OBS-DOCS.

The exit artifact is a case-to-test evidence table plus the tested runbook,
not another status endpoint. The linked acceptance ledger records closure;
reopen only a named assertion whose evidence is invalidated by a later change
or contradicted by a new failure.
This documentation review does not rerun or upgrade the recorded evidence.

Missing test evidence leaves the acceptance row open. Missing or unverifiable
admission evidence during an operator journey must lead to an explicit
stop/escalation result, not an automatic restart, new attempt, alternate
introducer or exclusion removal.
This slice does not require adding a general administrative removal API,
distributed admission ledger or new recovery service. If a safe path needs
one of those, record the decision and separate scope before implementation.

## Formation demonstration

This is the **N-FORMATION** work package. It proves the operational control
plane before N-CLUSTER adds partition ownership, halos, distributed step
commit, artifacts, or compute. A successful demonstration is:

```text
worker-a starts as formation A (one member)
worker-b starts as formation B (one member)
worker-c starts as formation C (one member)
        |
        | explicit join through authenticated peer sessions
        v
worker-a, worker-b, worker-c converge on formation A (three assigned node IDs)
        |
        | local or authenticated remote client API
        v
orishuctl cluster info / ls / inspect / cluster lock / cluster unlock / leave
```

The slice should exercise production seams, not a network simulator: separate
OS processes, QUIC connections, the bounded wire codec, the real membership
driver, the worker's client listener, and the existing CLI command path.

## Roadmap placement and dependencies

```text
N-MEMBERSHIP (implemented and accepted, including merge correction)
        |
        +--> peer trust/codec preflight
        +--> cluster-policy reconciliation
        +--> membership subset of O-API-SHAPE
                    |
                    v
             N-FORMATION             <- this task
             real three-worker administrative PoC
                    |
                    v
             N-CLUSTER
             distributed scientific execution
```

This diagram shows the formation dependency only. N-CLUSTER also requires the
roadmap's proven single-node O-RUNTIME semantics and S-WORKLOAD component graph;
formation acceptance alone is neither readiness nor authorization to start
distributed scientific execution.

N-FORMATION does **not** depend on O-RUNTIME, O-WASM, workload schemas,
partitioning, storage, Kagami, or a physics plugin. It may reuse the worker's
existing client listener and `orishuctl` surface, but only after the
membership/status subset of O-API-SHAPE is made explicit. Unresolved workload,
checkpoint, result, log, event, and audit endpoints do not block this slice.

Coordinate the [worker observability task](implement-worker-observability.md)
as a companion M4 delivery: its process probes, formation metrics and sampled
client/peer trace export exercise this task's production driver and transport.
Publish bounded health states and diagnostic outcomes for the worker adapters;
do not put exporter dependencies, secrets or trace context in the membership
core. The transport can develop independently, but the operational M4 demo
includes scrape/probe/trace evidence and
[operator documentation](document-worker-observability.md). This adds no
dependency on workload execution. N-FORMATION owns driver/transport behavior;
P-OBSERVABILITY owns exporters and optional dependencies; P-OBS-DOCS owns
operator recipes. The combined M4 demonstration requires all three deliveries,
but exporters need not be implemented before the transport slice can start.

The companion contract is already accepted in ADR 0017; do not substitute an
always-on route in the mutation API or reopen the listener decision here:

| Delivery | M4 obligation |
| --- | --- |
| N-FORMATION | Real owner supervision/progress and bounded diagnostic outcomes; no exporter dependencies in the sans-IO core |
| P-OBSERVABILITY slices 1–2 | Independently feature-gated `observability` capability; configurable dedicated HTTP diagnostics listener for `/metrics`, `/livez`, `/readyz` and `/startupz`; runtime exposure disabled by default |
| P-OBSERVABILITY slice 3 | Independently feature-gated `otlp-tracing`, disabled by default; bounded sampled client/peer traces reaching a test OTLP receiver |
| P-OBS-DOCS formation portions | Tested feature/configuration, secure scrape, probe and collector recipes; updated worker/CLI manuals and worker/cluster operator stories |

Both optional features are excluded from Cargo defaults. Remote metrics require
the ADR's TLS and monitoring-only authorization policy (or its secured proxy
option); a remote probe exemption never opens metrics or mutation routes.
Publish exact settings and the health matrix in the owning observability
documents as the companion contract lands. The table defines the full M4
obligation, not its implementation status. Consult the
[documented local surface](../orishu-observability.md#implemented-local-surface)
for currently reported capabilities. Probe availability alone proves neither
secured remote access nor metric completeness or trace delivery; each has its
own evidence and independently controlled configuration.

## Required preflight decisions

Close these narrowly before writing the adapter that depends on them. Record
wire changes in `protocol-p2p.md` and client-resource changes in
`protocol-client.md`; do not hide either contract in worker-private structs.

For each preflight area, land the selected behavior, state transitions,
version/compatibility consequences, exact bounds and golden/hostile fixtures.
Listing alternatives is not completion. Reconcile imported protocol examples
with those choices before writing dependent handlers.

Treat each area as a separate review gate, not a requirement to redesign all
six before any work can proceed. Existing documented choices and recorded
implementation evidence remain the baseline; close only the unresolved parts.
A gate is closed when its owning protocol section specifies the behavior and
bounds, its tests are identified, and its dependent slice can proceed without
inventing security or lifecycle semantics. A cross-cutting departure from the
accepted ADRs requires an ADR update, not just a task-local decision.

The imperative wording below preserves the original contract requirements;
it is not a fresh inventory of missing implementation. Resolve each requirement
against the owning protocol and conformance ledger before reopening it. In
particular, recorded trust pinning, policy replication, credential bootstrap
and profile-4 replay remain the baseline unless evidence reveals a gap. The
references below to the original datagram-size contradiction, node-local lock
limitation and Tier 1 mutation entry describe issues resolved for the accepted
formation checkpoint, not current protocol defects.

### 1. Trust bootstrap

Specify how a joiner authenticates the introducer before disclosing the join
token. “Accept any self-signed server certificate and then send the token” is
not acceptable because an active intermediary could steal the admission
credential. Choose and test one explicit mechanism, such as join material that
binds the target formation and introducer certificate fingerprint, or an
operator-provisioned CA/trust root.

Specify how the operator securely obtains and transfers that join material.
A redirect is an untrusted endpoint candidate, not permission to disclose a
token to a new certificate. Bound redirects, DNS resolution, connection
attempts and total join duration; every candidate must satisfy the trust rule.

The handshake must distinguish a provisional applicant from an admitted
member. After admission, the QUIC session is pinned to the formation-assigned
`NodeId` and certificate fingerprint held by membership. A label, DNS name,
endpoint hint, or claimed envelope field never establishes that binding.

Define the PoC credential lifecycle too: certificate creation/loading, secure
local permissions, the formation join token, and what a restart retains. Token
rotation and certificate rotation may remain deferred, but the CLI must not
claim they work if they do not.

Specify how an admitted worker obtains the credential material needed to act
as an introducer for the same formation. Tokens are absent from ordinary
gossip, membership snapshots, traces and replayable core state. Public policy
catch-up alone cannot enable introduction if token verification is not ready.
Test a third worker joining through the second worker.

Define a post-admission readiness gate. The core's join snapshot contains
members, while cluster policy, blocklist entries, and tombstones converge
through their owning reconciliation paths. A newly admitted worker must not
act as an introducer until it has obtained and validated the admission-relevant
state needed to enforce every gate; otherwise a removed or blocked identity
could exploit the catch-up window.

Define how completion is proved: a bounded, identified snapshot or equivalent
reconciliation barrier covering policy, blocklist and tombstones, including
concurrent updates, empty collections and expired continuations. Receiving one
page or seeing matching member counts is insufficient. This proves adoption
of a defined admission-state baseline, not globally latest state under a
network partition; normal convergence and admission re-checks remain necessary.

### 2. Wire adapter and bounds

Define one versioned wire DTO/codec that maps the peer protocol's envelope and
membership payloads to `orishu-membership` semantic inputs/effects. The raw
join token belongs only in the encrypted wire/credential adapter and must not
enter the replayable core, diagnostics, or ordinary logs.

The decoder must enforce frame, datagram, string, byte, collection, snapshot,
gossip, Merkle, and nesting bounds while decoding, before allocating the full
claimed value. It reports the actual encoded snapshot size to the core. Stream
versus datagram mapping follows message semantics in `protocol-p2p.md`; a
datagram that cannot fit is not silently promoted if doing so changes loss or
ordering semantics.

Keep wire DTOs distinct where the transport carries facts the core
deliberately omits. Do not add `Serialize` to every core enum merely to bypass
the adapter or allow secrets into replay fixtures.

Specify envelope sequence scope, replay-window bounds, request correlation,
connection replacement and overflow behavior. Valid reordered datagrams and
idempotent domain retries must remain valid; a sequence high-water mark must
not discard every older datagram. Disable replayable early data for admission
and mutations. Reject duplicate map keys, trailing frames/bytes where forbidden,
unsupported versions and excessive nesting before state adoption.

Resolve the peer document's datagram-size contradiction: its blanket stream
fallback conflicts with the message-specific SWIM mapping. Define a fitting
base message and byte-budgeted gossip selection; defer excess gossip to later
dissemination/anti-entropy and report an unencodable base message. Do not
silently change a datagram operation into a reliable stream. Golden tests must
cover the actual framing/CBOR representation, not only semantic JSON fixtures.

Close admitted-peer connection management as part of this gate: who initiates
connections after adoption/reconciliation, how simultaneous dials converge to
a usable session, and how a lost connection is recovered. Start from the
documented registry's duplicate-connection rule; demonstrate that it cannot
leave both workers repeatedly rejecting each other's surviving connection.
Specify bounded per-peer and process-wide dial work, endpoint selection,
backoff, and cancellation on generation change. Endpoint hints are untrusted;
every reconnect must revalidate the current formation, assigned identity,
certificate and admission restrictions. Reconnecting an admitted peer is not
automatic admission of an unknown worker.

Define what happens to an effect when its route is absent or a connection
fails: which messages expire under core timers, which exchanges may retry,
and which return a correlated failure. Do not accumulate an unbounded offline
send queue or treat a successful transport write as domain acceptance. Record
the supported connection topology and its resource cost for the three-worker
PoC; do not imply that a bounded all-to-all demonstration proves fleet scale.

### 3. Cluster-wide membership policy

Replace the core's documented node-local `membershipLocked` limitation before
exposing `orishuctl cluster lock` as a cluster operation. Split cluster-scoped,
versioned membership policy from node-local admission facts such as
`accepts.peers`, local connection capacity, and supported protocol range. The
cluster lock must gossip and reconcile through bounded anti-entropy, and its
version/conflict behavior needs fixtures.

Joining and administrative removal must consult the same converged lock. A
lock accepted through one worker becomes visible from every worker and blocks
admission through every introducer after convergence. Unlock is a newer
cluster-policy transition, not deletion of local state. The raw join token is
secret material and is not part of this gossiped policy.

Specify the policy's canonical leaf/tag, hash input, version ownership, equal-
version conflicts and anti-entropy participation. A local lock response means
local acceptance, not a synchronous cluster-wide fence. Concurrent lock/unlock
commands converge through the documented ordering; partitioned or catching-up
nodes must not claim globally current policy. Re-check locally held admission
gates when applying credential-verification evidence before inserting a member.
Test actual owner ordering; asynchronous verification is not a requirement to
introduce a new pending-job boundary solely to stage this race.

Distinguish a local connection/admission limit from a formation-wide member
limit. Serialization at one introducer prevents local over-admission, not
concurrent admissions at different introducers. Do not promise a globally
reserved final slot without a separate coordination contract; state and test
the limits this PoC actually enforces.

### 4. Minimal client resource surface

Freeze only the resource shapes needed by this PoC:

- cluster/formation summary, including immutable `formationId`, display
  `clusterName`, member count, membership lock, and explicit “no workload”;
- membership list and one-member inspection, including `NodeId`, worker label,
  liveness/incarnation, role/admission flags, advertised peer endpoints, and
  the source/freshness of a cached view;
- current join material through an appropriately privileged local/admin path;
- begin join on the worker being moved, retained join-operation status, and
  authenticated read-only inspection of the original issuer's admission record;
- voluntary leave, lock and unlock, including their identified receipt/replay
  contracts; and
- structured rejection/diagnostic responses for failed joins and stale or
  unauthorized mutations.

Recovery is part of this minimal surface, not deferred full CLI parity. Reuse
the [issuer inspection](../protocol-client.md#issuer-admission-inspection)
contract and [operator recovery runbook](../cluster-admission-recovery.md);
do not introduce a general operation service or historical audit API.

Use noun-shaped versioned resources and the authority rules in the runtime
design. The synthetic cluster summary is a projection of the current
formation, not a persisted authored cluster manifest. The client adapter may
reuse compatible imported DTOs, but it must convert explicitly to the shared
identity types instead of keeping a second `NodeId`, removal mode, or
formation representation.

Close the local administrator bootstrap: config-free startup permits local
reads but does not authorize mutations or token retrieval. Specify a secure
same-user credential provisioning path usable by both CLI and test harness;
operator, join and monitoring credentials remain non-interchangeable. Resolve
the current `DELETE /membership` Tier 1 table entry against the rule that all
mutations require authenticated authority. Freeze which local/remote client
transports the PoC supports; update protocol maturity and reject unsupported
modes explicitly rather than implicitly promising every documented fallback.

Define asynchronous command outcomes and bounded operation tracking, including
request identity, retry/replay behavior, timeouts and expiry. A successful HTTP
exchange is not proof that join/catch-up or cluster convergence completed.
Specify direct-worker targeting for join/leave and stale formation preconditions
for mutations; delayed requests must not operate on a newly adopted formation.
Distinguish a standalone cluster of one from an already joined member even if
all other members are dead; member count alone cannot decide join eligibility.

### 5. Ejection and restart semantics

Define what the worker shell does when the core learns that its own assigned
identity has been removed: stop participating in that formation, close its
peer sessions, and expose an actionable local state. Do not leave a process
sending as a tombstoned identity.

For this PoC, restart begins a fresh standalone formation and requires explicit
readmission with a new formation-assigned ID. Retain the securely stored
certificate identity where configured so restart does not silently bypass an
active formation's certificate-based exclusion. Do not restore old membership,
policy or assigned IDs. Automatic formation recovery and certificate rotation
remain separate work; document behavior when credentials cannot be loaded.

### 6. Interrupted transitions and stale IO

Specify join attempt identity and recovery when an introducer inserts a member
but its ACK is lost. Bound pending admissions and retry records; reconnecting
the same authenticated applicant must not allocate repeated live identities
or strand it permanently behind duplicate-certificate rejection. Retry may
recover the accepted outcome or return an explicit unresolved state with a
bounded recovery procedure; it must not pretend the insertion never occurred.
Define the case of concurrent attempts through two introducers, or explicitly
serialize supported attempts and reject concurrent operator requests. A lost
client connection alone must not silently undo an accepted domain transition.

Close the recovery contract with an explicit outcome table: no request emitted,
request emitted but acceptance unknown, accepted assignment still usable,
assignment now dead/removed, and introducer or applicant restarted. Bind retry
identity to authenticated credentials and the originating lifecycle, not a
worker label or certificate alone: retaining a certificate on restart must not
restore an old assigned ID. Specify changed-payload conflicts, record capacity,
retention/expiry and behavior when a record is unavailable. Neither eviction
nor reconnect may silently turn an uncertain attempt into a new admission or
reset its total retry budget. Local serialization is not a cluster-wide
exactly-once guarantee across independently partitioned introducers.

If the selected PoC behavior remains explicitly unresolved, the recovery
procedure must name the operator authority and target to inspect, the safe
preconditions for any new admission, bounded waits, and the stop/escalation
condition when those preconditions cannot be established. Demonstrate it through
the supported operator surface; “restart and retry” without checking the old
assignment and exclusions is not an adequate procedure. Record any wire/profile
change and compatibility tests in the owning protocol before implementation.

Every deferred timer, effect result, queued send and decoded peer input carries
the local lifecycle/attempt generation or equivalent validated binding needed
to reject stale completion after adoption, leave, ejection or shutdown. Audit
actual deferred completion classes separately from credential verification,
ID allocation or peer selection performed inline within one serialized owner
turn. Inline work needs the applicable gate/order checks, not an invented
asynchronous job merely to exercise stale completion. Record that distinction
and its evidence in the lifecycle matrix. Serialize formation
transitions; failed pre-adoption join retains the standalone state. After
adoption, catch-up failure is an explicit degraded target-formation state, not
an automatic rollback to the abandoned formation.

Voluntary leave is a self-announced liveness transition, not record deletion
or an operator tombstone. Because its announcement is lossy, immediate receipt
by every peer is not promised: survivors eventually detect departure through
announcement dissemination or normal SWIM. Bound shutdown flushing and close
old sessions; old work must never be re-sent using the new formation identity.

## Implementation slices

### Review checkpoints

These checkpoints organize the original delivery sequence. The first four
are complete at the linked formation acceptance checkpoint; only the companion
handoff remains open. Their retained requirements do not schedule a new pass
through slices 1–6 or authorize adding workload execution.

| Checkpoint | Disposition | Required evidence |
| --- | --- | --- |
| Contract closure — slice 1 | Completed for formation; ADR 0025 is a separate companion decision | Selected formation decisions linked to protocol sections; compatibility consequences and bounded failure behavior specified |
| Two-worker vertical slice — slices 2–5 | Completed | Production processes, authenticated operator request, peer handshake/admission, validated adoption and observable operation status; no direct core call standing in for a worker route |
| Introducer handoff — slice 6 happy path | Completed | A admits B, then B admits C; all three views converge, and B cannot introduce while catch-up or credential readiness is incomplete |
| Formation conformance — slices 3–6 | Accepted for source-built Linux | Required failure/convergence matrix and N-FORMATION criteria mapped to passing evidence, with reproducible commands and bounded cleanup |
| Operational M4 handoff — slice 7 and companion tasks | Open; select from the open M4 work table | Feature matrix, per-worker scrapes/probes, correlated peer trace and executable operator recipes; formation still works without telemetry |

Keep implementation evidence separate from acceptance: name the test or harness
and what production boundary it exercises, record checks actually run, and list
remaining gaps. Documentation review alone does not revalidate implementation
claims. In particular, helper coverage cannot close a process-level checkpoint.

### Evidence record

Previously reported tests and integration results are preserved in the
[historical integration record](cluster-formation-integration-record.md).
They are not fresh verification or a substitute for the acceptance ledger.
Record new results against the checkpoints and failure scenarios below, with
an exact runnable command and the production boundary exercised.

The [dated lifecycle and recovery checkpoints](cluster-formation-integration-record.md#dated-lifecycle-and-recovery-checkpoints)
are preserved there unchanged, including failures and limitations. Use the
[conformance ledger](cluster-formation-conformance.md) for current acceptance
disposition; append new evidence there rather than another planning status in
this task. A documentation-only review does not rerun these commands or promote
a partial row to passed.

### 1. Close the contracts

- Resolve the six preflight areas above and add hostile/golden fixtures.
- Add a versioned, cluster-scoped membership-lock entity to its owning pure
  model and merge path; preserve local admission configuration separately.
- Reconcile the membership subset of the imported `orishu` client DTOs with
  `orishu-identity` rather than translating identity through arbitrary strings.
- Mark unsupported imported CLI operations honestly. In particular, do not
  advertise token/certificate rotation, graceful compute drain, or historical
  audit as implemented by this PoC.

### 2. Peer codec and transport adapter

- Implement bounded CBOR envelope encoding/decoding and length-prefixed stream
  framing, with golden bytes and malformed/truncated/oversized input tests.
- Implement raw QUIC with mTLS, provisional join sessions, admitted session
  pinning, formation guards, stream/datagram routing, connection limits, and
  orderly shutdown.
- Implement bounded admitted-peer dialing/reconnection under the preflight
  contract. Verify that members learned through another introducer can exchange
  traffic without a test harness manually creating their sessions.
- Keep QUIC, TLS, sockets, DNS, and codec dependencies outside
  `orishu-membership`. Start with cohesive, testable modules in the worker, its
  real caller. Extract a crate only when a second consumer or a demonstrated
  deep interface justifies it; tests alone do not require a new crate.
- Reject unknown, cross-formation, certificate-mismatched, replayed stream, and
  over-budget input before it can fan out work.

### 3. Membership driver

- Give each worker one serialized owner of `Membership`. All peer input,
  operator commands, timer expiries, and effect outcomes enter its mailbox;
  no handler mutates membership behind the transition function.
- Execute every current effect: send, arm/cancel timer, select eligible peers,
  allocate a cryptographically random collision-checked ID, verify credentials
  and source networks, and publish structured changes/diagnostics.
- Schedule periodic probe and anti-entropy commands. Bound mailbox capacity,
  concurrent streams/sessions, timers, queued sends, and published events;
  define overload behavior instead of relying on unbounded channels.
- Define fairness and reserved processing capacity for control, actionable
  timer expiries and effect completions under peer/client floods. Coalesce only
  permitted diagnostics or supersedable work; never drop a correctness-bearing
  completion and leave an admission permanently pending. Timer scheduling uses
  monotonic shell time and generations, not wall-clock order.
- Publish bounded health/progress and instrumentable outcomes for
  P-OBSERVABILITY from this real owner. Health requests and scrapes must not
  acquire long-held locks or drive core transitions.
- Keep foreign gossip explicitly handed off. With no workload owner in this
  milestone it is ignored or reported according to the protocol, never parsed
  into membership state.

### 4. Worker lifecycle and configuration

- Extend the typed configuration path for worker/cluster labels, peer
  listeners and advertised endpoints, `accepts.peers`, limits, trust material,
  and optional explicit join inputs. Preserve file < environment < CLI
  precedence and config-free local startup.
- A fresh worker generates explicit standalone `FormationId` and `NodeId`
  values distinct from labels and immediately exposes truthful one-member
  status.
- Join atomically abandons the old standalone formation only after a validated
  acceptance. It remains in bounded catch-up/not-ready state and cannot
  introduce another worker until admission-relevant policy, blocklist, and
  tombstone state is synchronized. A state-changing leave announces departure,
  closes old sessions, generates fresh standalone identities, and reports both
  old and new formation state. Preserve the client protocol's distinct
  standalone-of-one no-op: retain identity and return its recorded unchanged
  receipt. A standalone introducer with other member records leaves normally;
  member count alone does not determine participation or leave eligibility.
- Keep peer listeners separate from client listeners and never open a remote
  port under the safe default configuration.
- Define and test peer-admission configuration independently of binding a peer
  listener. Reject contradictory enabled-role/listener settings at startup;
  merely advertising `accepts.peers` must never bypass the owner-held readiness
  gate. Missing target credentials or incomplete catch-up refuse introduction
  without terminating the membership owner or silently restoring old tokens.
- Keep advertised role, `introducerReady`, join-operation completion and process
  `/readyz` distinct. Coordinate their state matrix with P-OBSERVABILITY: a
  healthy worker deliberately configured not to introduce can be process-ready;
  a locked formation can safely enforce refusal without being unhealthy. A
  successful probe never authorizes admission or proves cluster convergence.
- Distinguish bound from advertised addresses, reject unusable peer endpoint
  claims and define port-zero reporting for tests. Wire the accepted core's
  anti-entropy depth/round limits through the shared configuration path.
- Exercise fresh restart, ejection, cancelled join and shutdown with pending
  effects. Secure certificate persistence is in scope; retained formation
  state and scientific artifact storage are not.
- Test reuse of the same configured Unix socket after graceful shutdown and
  process loss. Cleanup/recovery must preserve active listeners, symlinks and
  unrelated files, reject unsafe ownership/permissions, and report conflicts
  without deleting another process's endpoint.

### 5. Operator API and `orishuctl`

- Implement the production worker routes and library client methods for the
  minimal resource surface above; remove the placeholder success path for
  those routes.
- Wire the existing CLI concepts for `cluster info`, `ls`, `inspect`, `token`,
  `join`, `join-status`, `cluster lock`, `cluster unlock`, and `leave` to those
  real routes. Document bounded polling, pending versus terminal phases and
  exit-status meaning; command exit success alone must not mean join completion.
- Include the runbook's original-issuer inspection and exact receipt-replay
  procedures in CLI acceptance. Test direct-worker targeting and preservation
  of the original correlation; inspection must never submit a replacement join.
- Make JSON/YAML output stable enough for automation and table output useful
  to a human. Always display formation identity separately from cluster name
  and node identity separately from worker name.
- Local same-user reads may follow the documented Tier 1 policy. Remote reads
  and every mutation require the applicable authenticated authority; peer join
  credentials never authorize the client API.
- Return structured pending/accepted/rejected outcomes and provide the bounded
  completion/status path chosen in preflight. Keep operation IDs and formation
  preconditions in automation output. Secret retrieval must be explicit and
  never printed in ordinary status, logs or error output.

### 6. Multi-process conformance harness

- Start three worker subprocesses with isolated runtime directories and
  dynamically allocated listeners. Drive formation through `orishuctl` or the
  same public client methods it uses.
- Run at least the successful operator journey through the CLI binary; library
  calls alone do not validate command routing, authentication or output. Use
  machine-readable output for assertions and the same production listeners.
- Poll observable state with bounded deadlines; do not use fixed sleeps as
  proof of convergence.
- Capture per-process logs and deterministic test artifacts on failure, while
  redacting join tokens and private-key material.
- Reap every subprocess and bound cleanup on success, failure and timeout.
  Inject faults at real transport/effect boundaries with narrowly scoped test
  hooks when needed; preserve actual QUIC/TLS/codec and client authorization.
- Cover duplicate worker/cluster labels, reordered startup, an invalid token,
  a wrong formation/fingerprint, lock/unlock through different entry nodes,
  voluntary leave, process loss and SWIM visibility, and clean shutdown.

### 7. Observability and operator handoff

- Map formation instruments to actual owner/adapter outcomes before wiring
  exporters. Distinguish a newly accepted admission, a rejected admission and
  replay of a retained assignment; an HTTP success or a delivered reply alone
  does not establish any of those outcomes. Document counter reset/lifetime,
  units, finite labels and unavailable instruments in the companion catalogue.
  Membership convergence remains an exact-state assertion, not a metric or
  probe inference.
- Integrate P-OBSERVABILITY slices 1–3 against the driver and peer adapter:
  scrape each worker, exercise the probe matrix, and collect a sampled
  client-to-peer trace in a test OTLP receiver. Keep this in a feature-enabled
  companion harness; the formation harness must also pass with optional
  telemetry compiled out and with exporters disabled.
- Preserve the implemented [trace profile ADR](../adr/0025-version-peer-trace-context-propagation.md)
  and [activation evidence](cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09).
  Profile 5 requires a coordinated rebuild/restart with no fallback. The initial
  admission propagation is delivered; log correlation and the combined operator/
  overhead acceptance still require their own evidence.
- Verify telemetry outage/overload does not change admission, lock, leave or
  SWIM outcomes, and a stalled driver cannot hide behind a responsive HTTP
  exporter. Validate bounded trace context on the serialized peer/client path.
- Deliver P-OBS-DOCS formation recipes with the above features. Update worker
  configuration/manual, operator stories, protocol maturity, task index and
  roadmap to the actual supported command and transport matrix. Distinguish
  N-FORMATION completion from combined M4 observability acceptance.
- Limit this handoff to process and formation instruments and their tested
  deployment recipes. Runtime/storage/observation instruments and their
  workload-specific dashboards remain with P-OBSERVABILITY slice 4 and its
  owning milestones; no fabricated workload is needed to close M4.

#### M4 operator journey

Use the existing three-worker public journey as the domain control and extend
its companion evidence. The [official Collector walkthrough](../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
now supplies local admission/log correlation; this contract still requires a
final applicability review across all companion runs. Before running it, name the
chosen client operation and its causally related peer exchange, expected
observations, configuration, finite deadlines and applicable evidence to reuse.

1. Start isolated workers and perform A-to-B-to-C introduction through the
   authenticated operator surface. Retain exact formation, assigned identity,
   certificate and liveness assertions; do not replace them with successful
   scrapes, ready probes or matching member counts.
2. Scrape each worker separately through the documented monitoring access
   path. Verify the applicable catalogue and actual formation activity, and
   correlate probe transitions with the published local health matrix.
   Monitoring credentials must not authorize operator mutations, and a
   probe-only route configuration must not expose metrics.
3. After the accepted peer-profile contract is implemented, receive the
   selected client/peer spans in the test OTLP receiver and assert their
   documented parent or link relationship across worker processes. Matching
   operation IDs or unrelated local spans are insufficient. After the accepted
   stdout adapter is delivered, match a received span to a bounded operational record.
   Do not require all background gossip to belong to one trace.
4. Exercise the named collector/scraper pressure or outage cases with bounded
   recovery and shutdown. Assert the same domain outcomes and identity guards
   under the controlled scenario; unavailable telemetry alone must not change
   process health or trigger readmission. Telemetry drops are permitted and
   accounted for, not hidden behind a requirement for lossless export.
5. Run the corresponding P-OBS-DOCS instructions against that build: feature
   settings, secure scrape/probe/collector setup, applicable alerts and safe
   incident checks. Record automated versus manual evidence, tool versions,
   limitations and the separate overhead result before recommending M4 closure.

These assertions may be established by linked, bounded companion runs; they
do not require one monolithic test or every fault combined in every feature
mode. Reuse scoped security and pressure evidence where its contract and build
remain applicable. The peer-profile, stdout, scaling curve and cost decisions
are accepted; outstanding measurement and final-checkpoint work does not
invalidate independent metrics/probe or correlation evidence.

#### M4 capability and runtime matrix

Build capabilities and runtime enablement are separate axes. Record evidence
for each row; compiling a feature alone does not prove its startup or IO
behavior. All rows retain the same accepted peer wire contract for the tested
checkpoint, independently of exporter availability.

| Cargo capabilities | Required runtime configurations | Expected telemetry boundary |
| --- | --- | --- |
| Neither | Default startup; separately request each omitted capability | Formation works without telemetry; default opens no diagnostics listener or exporter connection; either unsupported request fails startup explicitly |
| `observability` only | Listener disabled; probes only; metrics/probes enabled; request tracing | Disabled opens no diagnostics listener; probes-only leaves metrics unavailable; enabled exposes only configured routes; tracing request fails explicitly |
| `otlp-tracing` only | Tracing disabled; tracing enabled; request diagnostics | Disabled makes no exporter connection; enabled exports sampled operations without opening a diagnostics listener; diagnostics request fails explicitly |
| Both | Both disabled; diagnostics only; tracing only; both enabled | Each capability remains independently controlled; combined operation does not couple scrape/probe progress to collector progress |

Map the existing formation and companion harnesses to these configurations;
startup/refusal tests may establish configuration rows, but must not substitute
for the required real three-worker enabled and telemetry-free journeys. Reuse
separate zero-sampling, malformed configuration, security, saturation and
shutdown tests from P-OBSERVABILITY. Define any additional combined case by its
missing assertion rather than multiplying all cases into a Cartesian matrix.

## Required failure and convergence evidence

Use pure transition tests for state matrices and real process/wire tests for
adapter guarantees. In addition to the three-worker happy path, cover:

Choose and record the minimum sufficient boundary before implementing each
remaining case:

- Independent worker processes plus public operator requests are required for
  the introducer handoff, interrupted-admission operator procedure, process
  loss/restart, excluded restart/ejection, lost departure and partition/heal
  journeys. Test-only fault injection may arrange the fault; assertions must
  inspect production interfaces, not mutate the core to manufacture success.
- Real authenticated wire/runtime tests can establish codec rejection,
  connection collision/reconnect, partial catch-up, stale completion and
  resource-limit behavior. State which process-level row they support rather
  than presenting them as a substitute for that journey.
- Pure tests establish transition orderings, merge conflicts and exact budget
  boundaries. Pair them with wire/runtime evidence when the property depends
  on decoding, session binding, scheduling or effect delivery.

For partition tests, specify which links are interrupted, which remain usable
and when healing occurs. For overload tests, name the configured capacity,
expected refusal/drop behavior and a measurable bound on control/shutdown
progress. Avoid replacing these assertions with merely “did not crash.”

| Scenario | Required evidence |
| --- | --- |
| A introduces B; B introduces C | Trust, token-verification readiness and complete admission-state catch-up work beyond the initial introducer |
| ACK lost after insertion; operator retries | Documented recovery without duplicate live IDs or false failure/rollback claims |
| Concurrent joins and final capacity slot | Requests are serialized/rejected as specified; gate re-checks prevent local over-admission |
| Simultaneous member dials; connection loss with both processes alive | Session collision handling and bounded reconnect restore real peer exchanges without a new join or duplicate membership; transport loss alone does not write removal state |
| Unreachable or stale advertised endpoints | Dial work/backoff and pending sends remain bounded; recovered routes resume reconciliation, while control/shutdown remain responsive |
| Lock during credential verification | Acceptance re-checks the updated policy before insertion |
| Concurrent lock/unlock; temporary peer partition | Deterministic policy convergence after reconnect, with no claim of a global lock before convergence |
| Empty, paginated, changing or truncated admission-state baseline | Introduction is enabled only after validated completion; malformed/expired catch-up remains bounded and non-introducing |
| Old session, timer, verification or send completes after leave/adoption | Generation fencing prevents mutation or emission into either abandoned or unrelated formation state |
| Voluntary leave announcement dropped | Old views converge through SWIM without requiring record erasure or creating a tombstone |
| Leave followed by same-certificate readmission | All surviving views converge on the new assigned identity while preserving the old departed record; exact leave-receipt replay after readmission does not leave again. Retain the controlled timing regression and rerun the full journey under the documented corrected budget; preserve historical failures without claiming a proven runtime cause. |
| Restart with prior certificate; tombstoned self learns ejection | New standalone identity; no old-session participation; target formation still enforces certificate exclusion |
| Datagram reordering/replay and oversized gossip | Valid probe correlations survive bounded replay handling; oversized piggybacking never changes transport semantics |
| Peer/client flood, slow reads and incomplete frames | Admission work, allocation, queues and timers stay bounded; control and health remain meaningfully supervised |
| Missing/wrong operator credential or monitoring/join credential used for mutation | No mutation, secret disclosure or misleading success |

Removal administration need not be exposed to exercise self-ejection: a
test peer may supply the valid tombstone through the production wire path.
Record any test-only fault controls explicitly and keep them unavailable in
normal releases. Avoid claiming arbitrary churn/scaling correctness from this
bounded three-process test.

## Administrator stories trialled

This PoC provides executable evidence for the membership-only portions of:

- config-free local startup, worker and cluster labels, peer listeners, peer
  capacity, and peer-admission flags;
- list and inspect cluster nodes, and inspect cluster status;
- retrieve current join material and explicitly join a worker;
- lock and unlock cluster membership; and
- voluntarily leave and observe liveness after a worker becomes unreachable.

It does not complete stories whose acceptance requires workload drain,
artifact transfer, durable audit history, runtime/storage telemetry, packaging,
or distributed computation. Story/CLI documentation must say which fields or
modes remain unavailable rather than filling them with plausible placeholders.

## Acceptance criteria

The criteria below close **N-FORMATION**, except for the explicitly marked
combined M4 gate. Exporter delivery remains in P-OBSERVABILITY and operator
monitoring recipes in P-OBS-DOCS; neither is silently dropped when formation
passes, nor does unfinished companion work make completed formation evidence
disappear. Report the status of all three packages separately at handoff.

Use these closure gates without marking an entire companion programme complete
when only its formation portion ships:

| Gate | Required closure artifact |
| --- | --- |
| N-FORMATION | All formation criteria and required failure rows mapped to passing evidence at the stated boundary, final validation and tested formation/recovery manuals |
| P-OBSERVABILITY for M4 | Slices 1–3 accepted against the same worker contract, including disabled/enabled modes, probe transitions, security, bounded metrics, trace/log correlation, a received cross-peer trace and reviewed formation-stage overhead evidence |
| P-OBS-DOCS for M4 | Tested formation-stage scrape/probe/collector recipes, applicable dashboards/runbooks and updated operator stories/manuals; unsupported release/platform paths explicitly identified |
| Combined M4 | All three gates above pass together; no workload execution or slice-4 telemetry dependency |

For this source-built Linux milestone, the companion tasks' later packaged
release and workload-specific criteria remain tracked there; they are not
silently claimed by M4 acceptance. The M4 ledger must identify the applicable
criteria and the owner of each later-stage obligation. A source-build recipe
is not evidence that a package, image or another platform is supported.

Before the final M4 validation batch, freeze a finite acceptance checklist:
selected deployment recipe(s), client/peer correlation operation, logging
output, capability configurations, reviewed overhead budget and exact commands.
Map each item to existing evidence or a named missing assertion. Record the
worker checkpoint and companion tool/configuration versions together so results
from incompatible profiles or configurations cannot collectively imply a pass.
Any new requirement after that review needs an explicit scope decision; any
failed required assertion remains open until resolved, not waived by the freeze.

- The membership core's liveness-gossip merge correction is accepted, the core
  remains sans-IO, and its complete test suite stays green.
- The trust-bootstrap decision prevents disclosure of the join token to an
  unauthenticated introducer and pins admitted sessions to formation, node ID,
  and certificate fingerprint.
- Three real worker processes starting as three formations can become one
  three-member formation through explicit operator-led joins. All views
  converge on the exact same `FormationId`, assigned `NodeId` set, labels,
  fingerprints, and liveness state.
- Duplicate worker names and reused cluster names do not merge identities or
  satisfy a formation guard.
- `orishuctl cluster info`, `ls`, and `inspect` return real state through the
  worker's production client listener and clearly distinguish identity,
  labels, liveness, source, and freshness.
- Locking through one worker converges to the others; a join attempted through
  another introducer is rejected as locked. Unlocking through a different
  worker converges and permits a valid join. Administrative removal is also
  refused while locked if it is exposed by this slice.
- Wrong token, target formation, certificate binding, sender binding,
  protocol version, replayed stream message, and oversized/truncated input
  produce bounded failures without applying invalid input. Distinguish
  rejection before insertion from a lost or invalid reply after remote
  insertion: the latter must retain an unresolved outcome and use the specified
  recovery procedure, not claim that no admission occurred. Where safe, expose
  structured operator diagnostics; unauthenticated malformed peer input need
  not receive a detailed response.
- A newly admitted worker cannot introduce another node until cluster policy,
  blocklist, tombstone and admission-credential catch-up has completed; timeout or malformed
  catch-up leaves it non-introducing with an actionable status.
- Killing one process causes surviving views to progress through the specified
  SWIM liveness states without writing an operator-removal tombstone. A late
  packet or stale timer cannot restore it incorrectly.
- A state-changing voluntary leave returns that worker to a fresh standalone
  formation without a tombstone and the old formation converges on its
  departure as liveness state; its membership record need not disappear.
  Leaving an already standalone formation of one is a recorded no-op with
  unchanged identities. Exact receipt replay must not cause a second leave or
  identity change, including after subsequent readmission. Verify both cases
  through the supported authenticated client surface.
- Mailboxes, sessions, connections, frames, datagrams, timers, decoded
  collections, diagnostics, and retry/fan-out work have tested limits.
- Required multi-process journeys use real QUIC/mTLS and real serialized
  client requests; no journey's pass condition calls the membership core
  directly or depends on a mock transport. Pure transition tests and real-wire
  fixtures remain valid evidence for the distinct boundaries assigned in the
  failure matrix; they do not substitute for a required process journey.
- The failure/convergence matrix above passes at the appropriate production
  boundary, including interrupted joins, policy races and stale IO fencing.
- Every row in that matrix maps to a named test/harness case and its runnable
  command. Record platform prerequisites and any remaining manual checks;
  skipped cases are gaps, not passing evidence. Check in the operator journey
  and its expected assertions so another contributor can reproduce acceptance.
- Before closing the task, consolidate the historical integration record into
  a current evidence ledger: checkpoint/scenario, named test, production
  boundary, exact command, result and remaining limitation. Reconcile superseded
  protocol maturity statements and task/roadmap statuses; do not leave both
  “unwired” and “implemented” as current descriptions of the same route.
- Local administrator bootstrap and direct-worker targeting are documented and
  tested. No Tier 1 mutation exception or default-success placeholder remains
  in the supported surface.
- The task's core formation evidence passes independently of telemetry.
  Combined M4 acceptance additionally requires the feature-enabled scrape,
  probe and cross-peer trace tests and operator recipes from slice 7; report
  outstanding companion work rather than declaring the whole milestone done.
- `cargo fmt --all -- --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace --all-targets`,
  `cargo test --locked --workspace --doc`, and `make docs-check` pass.
  Run the complete formation multi-process harness separately if it is not
  part of the default suite, including its explicit fault-build cases; prove
  test-only controls are absent from normal release builds. Preserve the
  membership dependency-purity test and record platform-only manual checks.
  For the combined M4 gate, additionally run the observability-feature
  build/test matrix and companion telemetry harness. Default workspace tests
  alone establish neither fault-build nor optional-exporter acceptance.
  For M4, map evidence to the [capability/runtime matrix](#m4-capability-and-runtime-matrix)
  and [operator journey](#m4-operator-journey). Until peer propagation is wired,
  distinguish local client-service receipt from unavailable cross-peer tracing;
  do not report distributed collector acceptance.
  Record the revision or dirty-worktree checkpoint, exact feature set and
  executable/target directory for each run. Run different feature builds
  sequentially when they share executable paths, or isolate their target
  directories; one build must not replace a binary used by another live test.

## Non-goals

- Workload admission, WebAssembly execution, partition assignment, halo
  exchange, step voting/commit, scientific results, checkpoints, or reset.
- Artifact chunk transfer, placement, replication, purge reconciliation, or
  availability repair.
- Kagami connectivity or observation streaming.
- Automatic discovery/admission, DNS-defined cluster membership, or mDNS.
- A durable authored cluster manifest or a permanent leader/scheduler.
- Token rotation, certificate rotation, rolling protocol upgrades, or retained
  formation recovery after restart. Changing these non-goals requires a new
  bounded task and any necessary architecture decision, not an incidental
  expansion of the IO adapter.
- Full `orishuctl` parity. Workload, result, checkpoint, storage, historical
  event/audit/log, graceful compute-drain, and monitor UI paths remain owned by
  later work packages.
- Performance or scientific scaling claims. This task proves a bounded
  operational control plane at three workers; distributed computation is the
  following milestone.
