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

## Prerequisites and reproduction

### Companion formation scrapes

`make test-formation-observability` runs the full public three-worker
handoff/leave/readmission/crash/restart journey with an explicitly enabled
loopback diagnostics listener on each process. After exact membership
convergence it scrapes real HTTP responses and checks issuer-local admission
counts of A=1, B=1, C=0, rather than counting membership learned through gossip.
It also checks that restarting B resets its issuer counters. Each read has a
two-second socket timeout and 6 KiB response bound; operator tokens must not
appear in responses, and failure evidence retains only the aggregate counts.

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

- The pinned validator rejects a deliberately malformed control input and
  accepts the actual worker's Prometheus text, using
  [`promtool check metrics`](https://prometheus.io/docs/prometheus/latest/command-line/promtool/#promtool-check-metrics).
- The response has the expected content type, disables caching, stays within
  6 KiB and excludes the actual worker operator token. There are exactly
  twenty-six finite, non-negative samples with the documented names.
- `promtool check config` accepts the checked-in example with only the worker
  address substituted for an ephemeral test port.
- `promtool check rules` and `promtool test rules` validate the three versioned
  local alerts and their synthetic failure/recovery fixtures. The real server
  also loads the exact three alert names while scraping the healthy worker.
- A real Prometheus process ingests all twenty-six series. Queries see only the
  metric name and Prometheus's target `job`/`instance` labels, the expected
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

## Limits

This validates the current local source-build catalogue, not packaged release
artifacts, secured remote access, complete formation metrics, histograms,
trace export, dashboards or notification delivery, OTLP exporter/collector failure, long-running cardinality
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
