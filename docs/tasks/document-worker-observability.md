# Document and verify operator observability workflows

Status: **partial — local/mTLS-proxy scrape and local Collector receipt recipes verified; remaining operator handoff pending**

Decision: [ADR 0017](../adr/0017-worker-operational-observability.md)
Roadmap package: **P-OBS-DOCS**
Dependency: [Worker observability](implement-worker-observability.md)

## Outcome and gap

An operator can deploy a supported worker build, enable least-privilege
monitoring, scrape metrics, configure safe process probes and use correlated
traces to diagnose an incident. The initial stories and design links are
planned documentation; exact flags, deployment examples, dashboards and
troubleshooting commands require the implemented contracts.

The [local Prometheus guide](../testing-worker-prometheus.md) and
`etc/prometheus-local.yml` now have a real source-build worker scrape and
`promtool` validation through `make test-worker-prometheus`. Remote access,
service/container deployment, release artifacts, remote collector configuration,
dashboards/alerts and the broader incident runbooks remain work. The local
example alone does not close this task. The additional
[mTLS proxy recipe](../testing-worker-monitoring-proxy.md) and
`make test-worker-monitoring-proxy` now exercise certificate/route isolation,
actual Prometheus ingestion and proxy-outage independence through the accepted
secure-proxy option. This does not provide worker-native remote diagnostics,
service/container integration or supported release artifacts.

Three local alert examples now have pinned `promtool` failure/recovery tests
and real-server loading evidence. The guide distinguishes unavailable scrapes,
unready workers and missing telemetry and supplies safe initial responses.
This is not notification delivery, a complete incident manual or the full
dashboard/alert set required as additional instruments land.

The [live trace-counter catalogue](../orishu-observability.md#live-trace-delivery-and-loss-counters)
now documents sampling, delivery uncertainty, queue pressure, absent-versus-zero
semantics and safe first checks. Its real-worker pressure fixture validates
live exposition with optional pinned `promtool` checks. Remote collector deployment,
cross-peer correlation and trace-loss dashboards remain open. Three optional
trace-loss warning examples now have synthetic pending/firing/recovery and
reset/idle/disabled/unavailable-target checks, plus real-server rule loading;
production thresholds and notification delivery remain unverified.
The [extended scraper harness](../testing-worker-prometheus.md#ingest-trace-counters-through-prometheus)
now checks 143-series ingestion, including membership deadline/abandonment and peer IO metrics, and fresh
values after collector recovery;
its bounded HTTP responder is not an operator collector deployment recipe.

The [pinned local Collector walkthrough](../testing-worker-otelcol.md) now
validates the checked-in OTLP/HTTP configuration with official `otelcol 0.160.0`
and receives real worker spans into inspectable JSONL. Its disabled/zero/enabled,
rejected-mutation, actual collector shutdown/recovery and seeded-redaction
checks pass through worker/CLI processes. This closes the local collector
receipt recipe only: remote collector security, cross-peer/log correlation,
collector pressure/retention testing and service/container/release delivery
remain open. The file exporter is a disposable diagnostic sink, not a backend
durability or audit guarantee.

The [Collector mTLS overlay](../testing-worker-otelcol-mtls.md) now also has
actual receiver-security evidence: dedicated server/client CA roles, startup
refusal for missing/malformed client trust and mismatched server keys,
plaintext/old-TLS/no-client-certificate refusal, and real worker failures for
wrong client issuer, server trust or endpoint name. Its secure happy path
retains disabled/zero sampling, file receipts and collector outage/recovery.
These source-built loopback checks do not qualify cross-host deployment,
rotation/revocation, bearer authorization or collector pressure/retention.

## Delivery slices

1. With the listener/configuration contract, update
   [worker stories](../user-stories/orishu/worker-admin.md) and
   [cluster stories](../user-stories/orishu/cluster-admin.md) to the final
   supported behaviors. Reconcile [configuration](../orishu-configuration.md),
   [client protocol](../protocol-client.md), [observability guide](../orishu-observability.md)
   and the [worker manual](../../apps/orishu-worker/README.md). Clearly mark
   planned versus implemented routes, features and platform support.
2. With metrics/probes, publish tested Cargo feature commands, official release
   capability matrix, every file/env/CLI option and default, HTTP routes,
   security/credential setup, metric catalogue and compatibility policy.
   Add runnable local and secured remote Prometheus scrape examples under
   `etc/`, with a configuration validation command and no real credentials.
3. Publish service-manager/container examples and an optional Kubernetes
   deployment recipe with startup/liveness/readiness probes, management-network
   exposure, graceful termination and scrape discovery. It is a deployment
   example, not a requirement to run Kubernetes or add a scheduler to Orishu.
   Explain loopback in containers, remote probe-only access versus protected
   metrics, and a local exec probe alternative where HTTP auth is unsuitable.
   Do not ship liveness restart policies based on peer count or simulation rate.
4. With traces, publish a tested OTLP collector configuration and correlation
   walkthrough across client/peer operations, credential redaction, sampling,
   queue/drop diagnostics, collector outage and shutdown behavior. Explain
   that sampled traces and operational logs are not durable audit/provenance.
5. With each runtime/cluster/storage instrument set, add versioned dashboard
   and alert examples using actual metric names. Cover unreachable workers,
   sustained unready state, driver lag, rejection/timeouts, queue saturation,
   telemetry drops and later commit stalls, integrity failures and replication
   deficits. Distinguish absent telemetry from measured zero; group by scrape
   target without creating unbounded labels. Document threshold rationale,
   workload applicability and false positives for idle/stopped runs.
6. Update [release documentation](../releasing.md), package/container manifests
   and operator installation instructions so included features, exposed ports,
   version compatibility and unsupported minimal-build behavior are explicit.
   Add an incident runbook for peer loss, unready worker, slow steps, missing
   results and missing telemetry; every procedure identifies the authority to
   consult and avoids automatic destructive recovery based on one signal.

## Acceptance criteria

Apply these criteria at the owning milestone's scope. The
[M4 closure gates](implement-cluster-formation-poc.md#acceptance-criteria)
require tested formation-stage workflows against the declared source-built
Linux checkpoint; they do not require or qualify unpublished release artifacts.
Record later release/platform and workload obligations separately, with their
owning milestone, rather than closing this entire task when M4 passes. Required
M4 recipes cannot be waived merely by labelling them unsupported.

- A fresh operator follows the manual against built release artifacts and
  sees a real scrape, expected probe transitions and a correlated trace.
- Prometheus configuration and alert rules validate with `promtool`; supported
  collector configuration validates using its pinned test version. Exercise
  the examples in an integration harness and record the exact versions.
- Dashboard/alert queries reference the actual catalogue; synthetic failure
  and idle fixtures test alert behavior and recovery. No sample embeds a real
  secret or exposes remote metrics without the documented access controls.
- Feature-disabled, runtime-disabled and enabled deployment paths are covered.
  Documentation changes accompany the corresponding implementation slice;
  they are not postponed until the release milestone.
- `make docs-check` passes, new links are inspected, and tested/manual-only
  platform examples are identified honestly.

## Non-goals

Operating a hosted telemetry backend, promising production SLO thresholds
without measurements, requiring Kubernetes, or changing cluster authority,
operator privileges, storage durability or scientific acceptance criteria.
