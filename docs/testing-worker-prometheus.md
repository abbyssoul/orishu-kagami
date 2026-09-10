# Test worker metrics with Prometheus

Status: **local source-build parser/scraper acceptance implemented; broader operator handoff pending**

The [worker observability task](tasks/implement-worker-observability.md) requires
real parser and scraper evidence. `make test-worker-prometheus` builds the
optional worker capability and operator CLI, validates actual exposition with `promtool`, checks
the [local scrape example](../etc/prometheus-local.yml), starts a separate
Prometheus process and queries its ingested series. It then stops the scraper,
exercises authenticated worker control and verifies fresh ingestion after
restarting Prometheus. This is a development
test, not a bundled monitoring service or a supported release-artifact test.

Slow-reader checks are separate from successful Prometheus ingestion. The
[diagnostics backpressure evidence](tasks/cluster-formation-conformance.md#diagnostics-response-backpressure--2026-09-07)
uses actual HTTP metrics and server write expiry over Unix sockets. It verifies
bounded pipelining, continued operator control and connection-slot reuse;
it does not certify slow downstream readers through the TLS proxy. Probes
share diagnostics capacity, so a scrape/probe transport failure alone is not
evidence of a dead owner or permission to restart/readmit a worker.

The direct listener's [method/error matrix](tasks/cluster-formation-conformance.md#direct-diagnostics-method-and-error-contract--2026-09-07)
also runs against actual TCP worker processes. Use exact query-free GET paths;
HEAD is rejected, not a lightweight probe. A 400/404/405 indicates a route,
method or configuration mismatch, not the `/readyz` health result. Error replies
do not contain metrics or echo request credentials and cannot be cached.

## Prerequisites and reproduction

### Companion formation scrapes

`make test-formation-observability` runs the full public three-worker
handoff/leave/readmission/crash/restart journey with an explicitly enabled
loopback diagnostics listener on each process. After exact membership
convergence it scrapes real HTTP responses and checks issuer-local admission
counts of A=1, B=1, C=0, rather than counting membership learned through gossip.
It also checks that restarting B resets its issuer counters. Each read has a
two-second socket timeout and 32 KiB response bound; operator tokens must not
appear in responses, and failure evidence retains only the aggregate counts.
Catch-up scrapes additionally require no receiver job on initial introducer A,
one owner adoption on each joiner B/C, and complete transfer/owner stage sums
after those jobs are quiescent. Leave retains the counters; restart clears them.
Exact membership and public operation assertions remain the domain evidence.

`make test-formation-observability-lost-ack` additionally compiles the debug-only
formation fault feature and suppresses A's first admission reply. The same full
journey requires A's replay counter to become positive without another insertion.
These targets must run sequentially with other builds using `target/debug`.
At harness entry, worker and CLI bytes are copied into a private temporary
directory and every restart uses those copies. This prevents a later build
from changing the running journey's feature set. Select/build the intended
feature set before entry; copying does not coordinate concurrent builds at
that initial selection boundary. Copies are removed on harness exit.
Neither is a supported release build or a complete probe/telemetry fault matrix.
They exercise HTTP scrapes directly; the separate Prometheus target above
establishes parser and backend ingestion behavior.

The separate `make test-formation-observability-ejection` target enables both
debug fault and observability features. It drives real authenticated admission,
then wire self-ejection, survivor SWIM detection and an explicit operator leave.
HTTP `/livez`, `/readyz` and `/startupz` are checked while joined, ejected and
returned to standalone: ejection withdraws readiness but does not fail liveness
or reset startup; peer loss alone does not make the survivor unready. Each
read has a two-second socket timeout and 1 KiB body bound. This scenario does
not execute C's handoff or the later crash/restart journey, and is not evidence
for join-in-progress, incomplete catch-up or arbitrary removal convergence.

`make test-formation-observability-issuer-loss` exercises the existing
post-insertion issuer-crash fault with HTTP probes on the surviving applicant.
Immediately after issuer loss and after the real retry window exhausts, the
applicant must remain live but unready, with startup latched. An unrelated
standalone worker remains ready. The journey preserves the public unresolved
status, exact retry, refusal of unsafe new admission/leave, and lost-history
inspection after test-induced restarts. Those restarts are fault evidence, not
an operator recovery recommendation. Allow the unchanged 183-second retry
window (210-second observation budget); the test does not shorten it. This is
not a catch-up completion or full formation churn test.

### Prometheus tools



Use a Unix host with Python 3, the workspace Rust toolchain and the official
Prometheus **3.5.0** `promtool` and `prometheus` binaries. This pinned version
makes the test reproducible; it is not a recommendation to deploy that version
in production. Obtain the correct platform archive from the
[official release](https://github.com/prometheus/prometheus/releases/tag/v3.5.0)
and verify it against that release's `sha256sums.txt` before extraction. The
Linux amd64 archive used for the recorded check has SHA-256:

```text
e811827af26d822afb09a4f28314f61b618b12cff5369835a67f674d8b46f39a
```

With both binaries on `PATH`:

```sh
make test-worker-prometheus
```

Or specify absolute paths:

```sh
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

The target downloads nothing. It uses the normal `target/debug/orishu-worker`
path, so do not run another feature-changing build against that path while
the fixture is live. A sandbox must permit local sockets and subprocesses.
All service addresses bind `127.0.0.1`; no external telemetry destination or
remote-management listener is used.

## What the check establishes

The main scraper harness below runs without enabled tracing and checks the
base 151-series catalogue. It does not ingest the twelve trace counters.
For their bounded maximum-value and actual-worker live-scrape/parser checks,
run with the same pinned **promtool 3.5.0**:

```sh
ORISHU_TEST_PROMTOOL=/path/to/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --bin orishu-worker maximum_trace_catalogue --target-dir target/formation-flow-observability
ORISHU_TEST_PROMTOOL=/path/to/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --test standalone tracing_saturation --target-dir target/formation-flow-observability
ORISHU_TEST_PROMTOOL=/path/to/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --test standalone real_worker_peer_handshake_failures --target-dir target/formation-flow-observability
```

The first checks all twelve counters at `u64::MAX` within a 4 KiB extension.
The second validates 163 live samples during a held collector request and after
recovery, with a 32 KiB response budget, exact queue/delivery counts and secret
exclusion. It retains real authenticated lock/unlock and shutdown assertions.
The third checks actual inbound/reliable-exchange samples from a malformed
authenticated handshake, probes-only route exclusion, unchanged identity and
readiness, and conservative maximum-width base exposition through the parser.
It also holds a valid registered applicant, scrapes total/provisional occupancy
1 then 0 after connection release/pruning, and performs authenticated lock/unlock
without admitting the applicant. Registry capacity is distinct from the existing
TLS/connection-task readings; see the [catalogue](orishu-observability.md#registered-session-capacity-gauges).
The [reliable-exchange evidence](tasks/cluster-formation-conformance.md#reliable-peer-exchange-metrics--2026-09-07)
separately maps real stream pressure and the three-worker transport counters;
this one-worker fixture does not establish those journeys.
Without `ORISHU_TEST_PROMTOOL`, those tests still check their Rust assertions
but skip the external parser; report parser evidence only when explicitly run.
These checks do not start Prometheus or validate an OTLP deployment recipe.
Keep feature-changing builds sequential in the shared target directory.

### Ingest trace counters through Prometheus

`make test-worker-trace-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus`
builds both worker capabilities and runs the extended harness with the same
pinned 3.5.0 tools. It requires local sockets, uses only loopback services and
downloads nothing. Like the base target, it uses `target/debug`; do not run a
feature-changing build against that directory while either fixture is live.
For an already-built worker/CLI in an isolated target directory, pass their
paths to `scripts/check-worker-prometheus.py --trace-metrics` along with the
two tool paths.

The extension checks all 163 series through real Prometheus queries, retaining
the base names, finite values, target-only labels and three existing alert
checks. It also validates and loads the three optional trace-loss rules below.
A loopback collector fixture initially responds with HTTP 503. The
scraper must observe `trace_failed_total >= 1` and `trace_accepted_total = 0`
(both with the `orishu_worker_` prefix). Authenticated lock and formation
inspection still work and readiness stays successful. After only the collector
recovers, an authenticated unlock produces new telemetry and Prometheus must
ingest `trace_accepted_total >= 1`; the earlier TSDB samples cannot pass this
gate. The ordinary scraper-stop/control/restart journey then runs too.

The collector handles at most 64 requests serially, with a 4 KiB header bound,
1024-byte body limit, one-second input deadline and two-second thread cleanup.
It checks HTTP framing and returns a valid empty protobuf success response;
it is not an OTLP decoder or a collector deployment recipe. Rust receiver tests
remain the trace-content evidence. Fixture tests exercise failure/recovery,
oversized-body refusal before reading and silent-input expiry. This does not
establish production alert thresholds, multi-worker correlation, native remote
monitoring or supported release artifacts.

### Ingest logging counters through Prometheus

The logging option is independent of trace export. With the same pinned 3.5.0
tools, run all three logging profiles:

```sh
make test-worker-log-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus WORKER_PROMETHEUS_TARGET_DIR=target/formation-flow-observability
```

This target builds both telemetry capabilities into the specified idle target
directory (default `target/worker-prometheus`), runs the logging validator and
bounded HTTP collector fixture tests, then checks these profiles sequentially:

| Harness options | Ingested worker series | Logging behavior |
| --- | --- | --- |
| `--log-metrics` | 160 | Lifecycle output; tracing disabled, no trace counters |
| `--log-metrics --trace-metrics --formation-alerts` | 172 | Lifecycle and fully sampled operation output; twelve existing alert expressions evaluated |
| `--log-metrics --trace-metrics --closed-log-output` | 172 | Real stdout pipe has no reader; terminal output failure and subsequent closure refusals, with healthy worker control/probes |

For existing binaries, pass any row's options to this command without rebuilding:

```sh
python3 scripts/check-worker-prometheus.py --log-metrics --trace-metrics --formation-alerts --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /path/to/promtool --prometheus /path/to/prometheus
```

The nine `orishu_worker_log_*_total` counters must have the exact finite catalogue,
non-negative integer values and only Prometheus target labels. Parser acceptance
and real query results are both required. The normal fixture sends stdout to
the null device: it requires acknowledged writes, not retained log content or
durability. Its queue/encoding/output/closure/shutdown loss counters stay zero.
The closed-pipe profile instead requires exactly one terminal output failure
and zero acknowledged writes; it must never reopen/retry the sink or restart
the worker to pass. The same authenticated identity, membership-policy and
probe assertions run during collector and scraper outages.

In both trace-enabled logging profiles, operator operations while Prometheus
is stopped must advance `written` or, for the broken pipe, `closed`. Restarted
Prometheus must ingest that new value, in addition to the existing fresh
membership and trace counters. Retained pre-outage TSDB samples cannot pass.
In the tracing-disabled profile, no new operation log is expected: enabled
lifecycle logging remains independent, and only the existing membership counter
must advance across scraper outage. Disabled-logging recipes retain their
151/163-series catalogues; absence is not a zero-valued log instrument.

The existing ten-second observation/command, one-second HTTP and three-second
process-exit budgets apply, with unchanged 32 KiB exposition and 64 KiB query
response limits. Negative controls cover missing/invalid/fractional counters,
unexpected loss, fake closed-pipe writes/retries and stale TSDB values, plus a
subprocess proving the pipe has no reader. The trace receiver remains a bounded
HTTP fixture, not the official Collector. Use the separate
[formation/log receipt walkthrough](testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
for content and causal evidence. These checks do not establish remote-proxy
ingestion, new log alert policies, shutdown-final log accounting, retention,
service/container deployment or overhead budgets.

### Base catalogue and scraper recovery

The main `make test-worker-prometheus` harness establishes:

- The pinned validator rejects a deliberately malformed control input and
  accepts the actual worker's Prometheus text, using
  [`promtool check metrics`](https://prometheus.io/docs/prometheus/latest/command-line/promtool/#promtool-check-metrics).
- The response has the expected content type, disables caching, stays within
  32 KiB and excludes the actual worker operator token. There are exactly
  151 finite, non-negative samples with the documented names, including five
  histograms with eight fixed buckets and sum/count each. Buckets are cumulative
  and `+Inf` equals count; the client sum matches its cumulative duration counter.
  This fixture enables no peer listener and requests no joins: reliable,
  outbound, membership deadline/abandonment and datagram/pre-pool counts are measured zero with an unused
  64-slot exchange pool and four-slot dialer, not evidence of exercised peer IO.
  The active owner registry reports zero occupancy and capacities 64/16 even
  without a peer listener; all four inbound-adapter gauges are zero instead.
- `promtool check config` accepts the checked-in example with only the worker
  address substituted for an ephemeral test port.
- `promtool check rules` and `promtool test rules` validate the three versioned
  local alerts and their synthetic failure/recovery fixtures. The real server
  also loads the exact three alert names while scraping the healthy worker.
- A real Prometheus process ingests all 151 series. Queries see only the
  metric name, Prometheus's target `job`/`instance` labels and the histograms'
  fixed `le` labels, the expected
  local target, and healthy values for the three process gauges.
  Lane gauges have their fixed capacities and integral occupancy within those
  bounds, including reserved permits rather than only queued messages.
  The completed client-request counter is positive after real CLI inspection;
  client duration counts handler execution, not response delivery.
- After stopping and reaping Prometheus, authenticated CLI lock/unlock commands
  still change the directly targeted worker's real state. Formation identity
  stays unchanged, readiness remains successful and owner transitions advance.
- Restarted Prometheus ingests a transition counter reached only during the
  outage. Retained pre-outage TSDB samples cannot establish recovery. This uses
  the same worker process and credentials, not a replacement worker.
- The worker remains ready after ingestion and scraper recovery. Processes terminate cleanly;
  temporary credentials, configuration and TSDB data are removed on exit.

Readiness and ingestion polling each have a ten-second deadline; HTTP calls
have a one-second timeout and explicit body limits. Tool invocations have a
ten-second deadline; operator commands also set the CLI's two-second request
timeout. Each process gets three seconds for graceful termination,
then is killed/reaped and the check fails. Ports are selected dynamically;
another process claiming a released test port causes failure, not silent
retargeting or an unexplained successful retry.

## Local alert examples and operator response

For a per-worker view of these instruments, use the optional
[formation dashboard](testing-worker-dashboard.md). It runs in Prometheus,
withholds current worker values after failed/absent/ambiguous scrapes and
requires current series for rate panels. Alert delays and dashboard snapshots
remain different views; neither authorizes a recovery action.

The [monitoring incident runbook](worker-monitoring-runbook.md) gives the
read-only workflow behind these first actions, including exact identity checks,
probe-versus-transport distinctions and bounded escalation. The ordinary
three-worker companion now checks healthy probes and matching health gauges
while locked and at surviving-worker suspected/dead peer checkpoints. The
trace Prometheus journey checks `cluster info`, `ls`, `inspect` and all probes
during collector failure and scraper shutdown. Test mutations and process
faults are controls, not runbook recovery steps.

Keep [the rule file](../etc/prometheus-worker-alerts.yml) beside
`prometheus-local.yml`; the example loads it using a relative path. The harness
copies both into its private configuration directory. Rule tests live in
[the test fixture](../etc/prometheus-worker-alerts.test.yml) and use the pinned
tool's [rule-testing format](https://prometheus.io/docs/prometheus/latest/configuration/unit_testing_rules/).

| Alert | Condition and example delay | First operator action |
| --- | --- | --- |
| `OrishuWorkerScrapeUnavailable` | Configured target has `up=0` for 1 minute | Check process/supervisor state, target address, diagnostics enablement and `/metrics` route selection. A failed scrape is not proof of worker death. |
| `OrishuWorkerSustainedUnready` | Scrape succeeds and `orishu_worker_ready=0` for 2 minutes | Inspect `/livez`, `/startupz` and the directly targeted worker's formation/operation status through its normal operator interface. Distinguish initialization, transition, required-role failure and shutdown. |
| `OrishuWorkerReadinessSeriesMissing` | Scrape succeeds but the readiness series is absent for 2 minutes | Check that this is the intended worker endpoint, the metric catalogue/build version and metric relabeling. Missing telemetry is not measured zero. |

These are warning examples, not measured SLOs or restart policies. One-minute
rule evaluation and pending delays reduce transient notifications; adjust them
to measured startup, transition and monitoring budgets before deployment.
Healthy idle/locked/non-compute workers report readiness independently of peer
count and simulation rate; these rules do not test either quantity. Probe route
selection does not remove health gauges from an enabled metrics response.
If metrics are intentionally disabled, remove or change that scrape target
rather than restarting a healthy worker to satisfy an invalid monitoring setup.

The fixtures test pending thresholds, recovery, healthy/transient exclusions,
missing/stale series and separation between unavailable and unready states.
They also explicitly show the discovery limit: once a target and its `up`
series disappear, these per-target rules cannot prove that a worker is missing.
Compare expected deployment inventory with discovery separately; do not use
`or vector(0)` to fabricate a worker state from absence.

No alert authorizes leave, fresh admission, exclusion removal, credential
rotation or process restart. Use the [formation recovery runbook](cluster-admission-recovery.md)
when an interrupted join is involved; sampled metrics cannot establish whether
remote admission occurred. An unready signal is not itself a restart request.
Alertmanager delivery, notification routing and dashboards remain separate work.

### Optional trace-loss warnings

When tracing is intentionally enabled, add
`prometheus-worker-trace-alerts.yml` to the configuration's `rule_files` beside
the base rule file. The [optional rules](../etc/prometheus-worker-trace-alerts.yml)
have [pinned test fixtures](../etc/prometheus-worker-trace-alerts.test.yml);
`make test-worker-trace-prometheus` validates both and verifies the six rules
are loaded by the real server. The ordinary target continues to load only the
three base rules.

All three trace warnings require a successful current scrape, a positive
five-minute counter rate, and a two-minute pending interval, evaluated once
per minute. These are deliberately sensitive examples of recently observed
loss, not measured SLOs: one burst can keep the condition positive until it
ages out of the lookback window. Tune severity, windows and notification
routing to the deployment; do not attach automatic recovery actions.

| Alert | Evidence | First operator action |
| --- | --- | --- |
| `OrishuWorkerTraceLocalDrops` | Active-slot, completed-queue, encoding or local identity/clock failure counters increased | Compare the individual counters. For queue pressure, inspect collector progress and the configured sampling rate before changing capacity. For encoding/source failures, preserve bounded diagnostics and investigate the worker. |
| `OrishuWorkerTraceDeliveryFailures` | An export attempt failed; collector acceptance may still be unknown | Verify the configured endpoint, trust, credentials and collector availability without printing secret values. Inspect the collector's own diagnostics. Do not replay a domain operation to repair a missing trace. |
| `OrishuWorkerTraceCollectorRejections` | A valid collector response reported rejected spans | Consult the collector's capacity, policy and schema diagnostics; a successful HTTP exchange alone does not mean every span was accepted. |

The rules evaluate [`rate`](https://prometheus.io/docs/prometheus/latest/querying/functions/#rate)
on each named counter before combining positive conditions with `or`, so
individual counter resets remain independent. Do not select several unlabelled
metric names inside one `rate`: it drops the metric name and can produce
duplicate label sets before an outer aggregation runs. The regression fixture
puts all four local-loss counters on one worker, including a reset alongside
new queue loss; the earlier separate-worker fixtures did not exercise that case.
Labels remain
`job` and `instance`; trace/peer identities never become alert labels. Sampling
exclusions, shutdown abandonment, consumer closure and warning-only responses
are not treated as these failure conditions. Inspect their separate counters
when needed; a collector warning alone does not establish span rejection.

Missing trace series do not become zeros or an automatic alert: feature and
runtime disablement legitimately omit them. Compare expected configuration
and discovery inventory separately. No trace-loss rule can prove a worker is
missing after its `up` series disappears. A scrape failure belongs to the base
scrape-unavailable alert, not a fresh trace-delivery diagnosis. Idle/zero-sampled
workers, counter resets without new losses and disabled tracing remain quiet
in the fixtures; deleted targets stop these per-target warnings.

Unit fixtures establish pending/firing/recovery and exclusions using
[promtool's rule-testing clock](https://prometheus.io/docs/prometheus/latest/configuration/unit_testing_rules/).
The real-process journey now also waits for at least two worker scrapes and
executes every loaded alert expression through Prometheus's query API against
the complete coexisting catalogue. This detects evaluation errors that loading
rules before their first scheduled evaluation could miss. It does not prove
two-minute fault-driven alert firing or notification delivery. Consult the
authenticated worker operation status and the formation recovery runbook for
domain outcomes; telemetry loss cannot authorize restart, leave or readmission.

### Optional formation warnings

The [formation rule group](../etc/prometheus-worker-formation-alerts.yml) adds
six examples using the existing process/formation catalogue; it creates no new
worker metrics. Add that file beside the base rule file in `rule_files` only
when wanted. It does not require `otlp-tracing`. Like the base and trace rules,
it selects `job="orishu-worker"`. If using another scrape job (including the
mTLS example's `orishu-worker-mtls`), adapt **every** metric and `up` selector
together and rerun validation. Merely loading rules does not retarget them.

| Alert | Condition | First operator check |
| --- | --- | --- |
| `OrishuWorkerOwnerUnresponsive` | Successful scrape with owner responsiveness zero for two minutes | Follow the runbook's owner/role checks; responsive HTTP does not prove owner progress. This notification delay does not change the five-second supervision threshold. |
| `OrishuWorkerAdmissionRefusals` | Positive recent local admission-refusal rate | Check the intended lock, credentials and public operation outcome. This is **informational**: correct policy enforcement can refuse joins while the formation stays healthy. |
| `OrishuWorkerCatchupTransferFailures` | A receiver binding, validation, structured rejection, unavailable-transfer or whole-transfer-timeout counter increased recently | Inspect the original operation and current participation; successful transfer is not adoption, and a failure does not authorize a new join. |
| `OrishuWorkerMembershipAbandoned` | A join or anti-entropy abandonment counter increased recently | Distinguish exhausted admission recovery from exhausted reconciliation using the individual counters and targeted operation/peer views. Preserve unresolved admission and its stop procedure. |
| `OrishuWorkerPeerTimeouts` | An inbound handshake, outbound dial/TLS or reliable-exchange timeout counter increased recently | Check the named adapter, routes, peer authentication and capacity. Nested timeout counters can overlap; they do not count unique failed peers or incidents. |
| `OrishuWorkerCapacityExhausted` | A named slot budget has positive capacity and observed occupancy at least that capacity for two minutes | Check reservations, refusal counters and downstream progress. Retained registry slots are not live sockets; propose capacity/sampling changes only after identifying the responsible budget. |

All six require `up=1`, evaluate once per minute and use a two-minute pending
interval. Counter warnings use a five-minute rate window; one error burst can
keep a condition positive until its samples age out. These are sensitive
examples, not measured SLOs, continuous-state guarantees or restart policies.
Full occupancy is used instead of an arbitrary utilization percentage; a brief
reservation alone should not fire, although between-scrape activity is unseen.
Review delays/severity against the actual deployment before routing alerts.

Capacity matching uses one finite `budget` label derived from ten explicitly
selected metric families: `peer`, `control`, `completion`, `shutdown`,
`peer_inbound_tls`, `peer_inbound_connection`, `peer_registry`,
`peer_registry_provisional`, `peer_reliable` and `peer_outbound`. It matches each
occupancy only to that worker's corresponding positive capacity; zero/absent
capacity is unavailable, not saturation. No peer, formation or operation IDs
become alert labels. This is a Prometheus expression label, not a new label on
worker exposition. See [vector matching](https://prometheus.io/docs/prometheus/latest/querying/operators/#vector-matching).
With the example's target labels, the group has at most fifteen simultaneous
alert instances per target: five scalar conditions and ten named budgets.

The event rules combine positive **conditions**, not event totals. Their
expression values must not be presented as summed failure counts. Timers
consumed by SWIM, successful assignment replay, transfer validation, lifecycle
cancellation/fencing and owner abandonment do not trigger unrelated receiver
or transport failures. Healthy idle and correctly locked workers do not fail
owner/capacity rules solely because they lack a workload or reject admission.

Missing/stale gauges are not zeros. A rate can still describe recent errors
from samples retained in its five-minute window after that particular series
disappears; it is not a current availability assertion. A failed scrape gates
these warnings off, and a deleted target still requires inventory comparison.
Use the base unavailable/missing-series checks and the
[incident runbook](worker-monitoring-runbook.md), not fabricated zero values.

#### Formation alert verification

The [finite fixture generator](../scripts/test_worker_formation_alerts.py)
checks each selected counter and all ten capacity families separately, then
their coexistence, pending/firing/recovery, idle/reset and missing-data cases.
It evaluates the actual rule file with promtool; it does not copy or emulate
the PromQL. Fixture metric names must belong to the catalogue that the real
ingestion harness verifies. Unknown budget families are deliberately excluded.

With pinned Prometheus/promtool 3.5.0 as above:

```sh
python3 scripts/test_worker_formation_alerts.py --promtool /path/to/promtool
make test-worker-formation-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

The first runs 39 finite synthetic scenarios (all six rules checked at three
times in every case) without sockets. The second builds both telemetry
capabilities and runs actual ingestion, expression evaluation and the existing
collector/scraper outage journey with both optional alert groups. It uses
`target/debug`; keep feature-changing builds sequential. For an isolated
already-built worker/CLI, use `check-worker-prometheus.py` with their explicit
paths and these independent options:

| Options | Ingested worker series | Loaded/evaluated alert expressions |
| --- | --- | --- |
| Neither | 151 | 3 base |
| `--trace-metrics` | 163 | 3 base + 3 trace |
| `--formation-alerts` | 151 | 3 base + 6 formation |
| Both | 163 | 3 base + 3 trace + 6 formation |

The live checks wait for two samples before evaluating every loaded expression
against the real catalogue, retaining the finite HTTP limits and ten-second
polling budgets. They prove error-free evaluation and healthy-formation
exclusions, not failure-driven notifications after the full pending delays.
Synthetic tests establish that timing; dashboards, notification routing and
deployment qualification remain separate deliverables.

## Limits

This validates the current local source-build catalogue, not packaged release
artifacts, secured remote access, complete formation metrics, per-route/peer histograms,
cross-peer trace export, dashboards or notification delivery, full collector deployment, long-running cardinality
or performance budgets. The one-second scrape interval is a short test/demo
setting, not a measured production recommendation. Both Prometheus and worker
must share the host network for the local example; container loopback refers
to that container, not another container or the host.
Alert state transitions above have synthetic rule-engine evidence; the short
real-process check proves rule loading and healthy scraping, not fault-driven
notifications after the full pending delays.
Scraper-outage evidence covers one standalone worker and its authenticated
lock/unlock surface. It does not establish three-worker admission/SWIM behavior
under scrape saturation, a failed diagnostics task, or bounded trace-export
queues. A stopped Prometheus cannot evaluate its own alerts; monitor that
service independently rather than expecting these worker rules to report it.
