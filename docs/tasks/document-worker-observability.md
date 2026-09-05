# Document and verify operator observability workflows

Status: **planned; deliver alongside P-OBSERVABILITY slices**

Decision: [ADR 0017](../adr/0017-worker-operational-observability.md)
Roadmap package: **P-OBS-DOCS**
Dependency: [Worker observability](implement-worker-observability.md)

## Outcome and gap

An operator can deploy a supported worker build, enable least-privilege
monitoring, scrape metrics, configure safe process probes and use correlated
traces to diagnose an incident. The initial stories and design links are
planned documentation; exact flags, deployment examples, dashboards and
troubleshooting commands require the implemented contracts.

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
