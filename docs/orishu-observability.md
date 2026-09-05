# Orishu operational observability

Status: **accepted design; not implemented**

[ADR 0017](adr/0017-worker-operational-observability.md) defines feature-gated
worker metrics, health probes and trace export. This page is the operator
design entry point; it is not a claim that the routes or options exist today.
Actual commands belong in the [worker manual](../apps/orishu-worker/README.md)
when the [implementation task](tasks/implement-worker-observability.md) lands.

## What operators will receive

- A configurable, separately bound worker HTTP listener serving Prometheus
  `/metrics`, `/livez`, `/readyz` and `/startupz`.
- Independently enabled OTLP trace export, with sampling and bounded queues.
- Published build capabilities and an explicit error if requested support was
  compiled out. Official packages include capabilities but enable no endpoint
  or outbound export without operator configuration.
- A metric catalogue and role-aware probe matrix, safe scrape/probe examples,
  collector configuration, dashboards, alerts and troubleshooting guidance.

Metrics and traces describe worker operation. Scientific observations and
immutable checkpoint/result provenance retain their existing authorities.
Operators must not infer scientific validity or durability from HTTP health.

## Planned probe interpretation

| Situation | Liveness | Readiness |
| --- | --- | --- |
| Initializing; supervised loop responsive | Success | Failure until required roles initialize |
| Healthy standalone or idle worker | Success | Success |
| Simulation stopped, no workload, or work admission disabled by policy | Success | Success if enabled roles are ready |
| Joining/recovering with required local state not yet adopted | Success | Failure |
| Peer loss or cluster under-replication, local roles still safe | Success | Success; diagnose cluster state separately |
| Local required role/storage cannot serve safely | Success if loop responsive | Failure |
| Control loop stalled past its supervision deadline | Failure | Failure |
| Graceful shutdown/draining | Success while supervised shutdown progresses | Failure |
| Scraper or trace collector unavailable | Unchanged | Unchanged |

Startup succeeds once required initialization completes and stays successful
for that process lifetime. Before binding, probes may see connection refusal.
Use startup gating and measured liveness thresholds to accommodate legitimate
initialization and load. A readiness failure is not itself a restart request.

## Delivery order and manual work

The first slice instruments local process and formation operations alongside
N-FORMATION, independently of workload execution. Later slices instrument
O-RUNTIME/O-STORAGE, N-CLUSTER/N-ARTIFACT and V-LIVE. Trace propagation across
peers lands only with its versioned wire contract. P-SCALE uses the resulting
measurements but validates scientific equivalence separately.

The [operator documentation task](tasks/document-worker-observability.md)
must publish and exercise configuration/feature matrices, Prometheus scraping,
service/container probe deployment, OTLP export, least-privilege network
exposure, dashboard/alert rules, upgrade notes and incident procedures with
each corresponding implementation slice. Exact flags, environment variables,
port, metric names, histogram buckets, thresholds and exporter dependency
versions are implementation-task decisions, not invented runnable examples.

See the [configuration contract](orishu-configuration.md),
[client-protocol exception](protocol-client.md#worker-operational-diagnostics),
[worker stories](user-stories/orishu/worker-admin.md#monitor-worker-metrics-and-process-health)
and [cluster stories](user-stories/orishu/cluster-admin.md#correlate-cluster-metrics-and-traces).
