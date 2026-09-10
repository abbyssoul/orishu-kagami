# Document and verify operator observability workflows

Status: **partial — formation-stage source-built recipes and selected deployment collection verified; final M4 performance acceptance and later release/workload handoff pending**

Decision: [ADR 0017](../adr/0017-worker-operational-observability.md)
Roadmap package: **P-OBS-DOCS**
Dependency: [Worker observability](implement-worker-observability.md)

The [structured stdout manual](../../apps/orishu-worker/README.md#structured-stdout-logs)
and [configuration table](../orishu-configuration.md#implemented-structured-stdout-logging)
now describe implemented settings, actual sampled IDs, finite records and
loss/shutdown interpretation. Updated worker/cluster stories preserve the
distinction from audit/history. The
[Linux executable evidence](cluster-formation-conformance.md#runtime-logging-and-local-span-receipt--2026-09-09)
is supplemented by the [official Collector three-worker walkthrough](../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough):
both admission chains match per-worker stdout records while direct scrapes,
probes and cross-worker policy checks pass. The selected service/container
collection is separately verified below; final M4 acceptance remains open.
Reuse these deliveries rather than creating another sink.
The manual's slow-reader recovery procedure now has
[real-worker pipe recovery evidence](cluster-formation-conformance.md#real-worker-stdout-reader-recovery--2026-09-09):
resume the reader, retain loss counts, verify a new correlated record and use
unchanged authoritative identity/readiness as the control. This does not qualify
service-manager/container collection or a new destination adapter.

The operator selected both deployment examples on 2026-09-09. Their
[enabled journal and container collection checks](cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09)
now match actual runtime-collected records to official Collector spans, preserve
credentials with fresh identities across deliberate restarts, and require
clean shutdown. The original five-mode recipes remain separate regression
evidence. Final checkpoint/applicability review and overhead remain open;
this does not qualify three-worker formation under either supervisor.

The [current-build recipe refresh](cluster-formation-m4-checklist.md#current-build-operator-recipe-verification--2026-09-09)
now revalidates both deployments, all ingestion catalogues, Collector HTTP/mTLS,
proxy pressure/security and dashboard queries after the formation/shutdown fixes.
Formation-stage recipe implementation is verified; final M4 performance and
acceptance remain open, not another missing sink or deployment-selection choice.

The [logging-counter Prometheus recipe](../testing-worker-prometheus.md#ingest-logging-counters-through-prometheus)
now verifies actual 160/172-series ingestion with independent logging, combined
telemetry and a real broken stdout pipe. New writes/closure refusals generated
while Prometheus is stopped must be ingested after it restarts. This closes the
named local logging-catalogue gap, not remote-proxy or service/container
co-deployment. Existing 151/163-series modes retain their disabled-log semantics.

## Outcome and gap

The intended outcome is that an operator can deploy a supported worker build, enable least-privilege
monitoring, scrape metrics, configure safe process probes and use correlated
traces to diagnose an incident. Formation-stage source-built Linux flags,
deployment examples, dashboards and troubleshooting commands are now documented
and verified at the scoped checkpoints below. Published supported artifacts
and later workload workflows remain separate delivery obligations.

The [local Prometheus guide](../testing-worker-prometheus.md) and
`etc/prometheus-local.yml` now have a real source-build worker scrape and
`promtool` validation through `make test-worker-prometheus`. Remaining work is
final M4 performance acceptance, release artifacts and
later workload-specific monitoring. The scoped scrape, collector, dashboard,
alert and incident deliveries below do not close that broader handoff. The
additional
[mTLS proxy recipe](../testing-worker-monitoring-proxy.md) and
`make test-worker-monitoring-proxy` now exercise certificate/route isolation,
actual Prometheus ingestion and proxy-outage independence through the accepted
secure-proxy option. This does not provide worker-native remote diagnostics,
service/container integration or supported release artifacts.

Three local alert examples now have pinned `promtool` failure/recovery tests
and real-server loading evidence. The guide distinguishes unavailable scrapes,
unready workers and missing telemetry and supplies safe initial responses.
The [optional formation warning group](../testing-worker-prometheus.md#optional-formation-warnings)
adds owner responsiveness, admission/catch-up failures, exhausted membership
budgets, peer timeouts and ten named slot budgets. Its 39 synthetic scenarios
check the real rules, including coexisting counters, reset/idle behavior,
missing data and recovery. Live checks evaluate all loaded expressions against
the full worker catalogue after two scrapes. The trace-loss expression's
same-worker counter collision is corrected with an explicit regression.
These examples are not notification delivery, accepted production thresholds
or the remaining deployment/correlation handoff.

The [formation snapshot dashboard](../testing-worker-dashboard.md) now provides
a versioned, self-contained Prometheus console with 37 per-target panels.
The [acceptance ledger](cluster-formation-conformance.md#formation-snapshot-dashboard--2026-09-08)
records actual HTML rendering, extracted-query fixtures, tracing-off/on workers,
failed/missing/ambiguous targets, hostile selections and collector recovery
and their scoped acceptance. Inspected desktop/mobile captures supplement those
checks; the ledger retains the corrected blank fragment-capture failure.
This delivers the formation dashboard example in slice 5, not remote UI
security, a fleet-scale platform, later workload dashboards or M4's final
deployment/correlation acceptance.

The [live trace-counter catalogue](../orishu-observability.md#live-trace-delivery-and-loss-counters)
now documents sampling, delivery uncertainty, queue pressure, absent-versus-zero
semantics and safe first checks. Its real-worker pressure fixture validates
live exposition with optional pinned `promtool` checks. Remote collector deployment
and the combined deployment handoff remain open; the snapshot dashboard above includes
trace-delivery/loss panels without establishing those capabilities. Three optional
trace-loss warning examples now have synthetic pending/firing/recovery and
reset/idle/disabled/unavailable-target checks, plus real-server rule loading;
production thresholds and notification delivery remain unverified.
The [extended scraper harness](../testing-worker-prometheus.md#ingest-trace-counters-through-prometheus)
now checks 163-series ingestion, including catch-up, membership deadline/abandonment and peer IO metrics, and fresh
values after collector recovery;
its bounded HTTP responder is not an operator collector deployment recipe.

The [pinned local Collector walkthrough](../testing-worker-otelcol.md) now
validates the checked-in OTLP/HTTP configuration with official `otelcol 0.160.0`
and receives real worker spans into inspectable JSONL. Its disabled/zero/enabled,
rejected-mutation, actual collector shutdown/recovery and seeded-redaction
checks pass through worker/CLI processes. This closes the local collector
receipt recipe only; the newer three-worker walkthrough above adds admission/log
correlation. Collector pressure/retention testing and combined
service/container/release delivery remain open. The file exporter is a
disposable diagnostic sink, not a backend
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

The [source-built user-service recipe](../testing-worker-user-service.md) now
delivers a bounded portion of slice 3: actual runtime-linked systemd units,
five capability/runtime configurations, probe/metric and authorization checks,
graceful stop and explicit fresh-identity restart. It changes no packaged unit
or boot policy. System-wide deployment, remote-proxy co-deployment,
release lifecycle remain separate gaps; trace/log correlation is supplied by
the separately verified collection extension. A working
user manager is an explicit prerequisite, not silently provisioned by the test.

The [rootless container recipe](../testing-worker-container.md) adds the local
source-built container portion of slice 3: combined/minimal images, five runtime
modes, private mounts, exec probes, explicit restart and separate/shared-network
scraper checks. The original Dockerfile's release default, published images,
remote proxy co-deployment, remote Collector deployment and Kubernetes remain unqualified. This
recipe predates the accepted stdout output and does not qualify combined
trace/log collection inside containers.

The [formation monitoring runbook](../worker-monitoring-runbook.md) now covers
bounded read-only triage for peer loss, local unreadiness and missing telemetry.
Its [evidence mapping](../worker-monitoring-runbook.md#6-handoff-and-evidence-limits)
separates public CLI/process checks, HTTP fixtures and synthetic alert tests.
This delivers the formation incident procedure within slices 1, 5 and 6; it
does not deliver dashboards, notification routing, service/container deployment,
cross-peer/log correlation or later workload incident procedures.

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
   Use the accepted stdout logging adapter for the PoC and show how to collect
   its structured records from ordinary processes, the selected user service
   or container runtime. Separate bare-metal/container examples from actual
   Kubernetes/cloud qualification. Unix-datagram and vendor-specific adapters
   remain future extensions, not required infrastructure for local startup.
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
Use the [recipe-to-criterion map](cluster-formation-conformance.md#m4-operator-recipe-applicability--2026-09-08)
and [formation task's current handoff disposition](implement-cluster-formation-poc.md#open-m4-work-selection)
before selecting another deployment increment. Preserve the scoped recipes
delivered above; identify a missing
combined-deployment check explicitly rather than treating all service/container
work as unimplemented. The runtime contracts and ordinary-process trace/log
walkthrough are delivered; the [proposed final checklist](cluster-formation-m4-checklist.md)
records the accepted deployment selection and remaining review gates.

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
