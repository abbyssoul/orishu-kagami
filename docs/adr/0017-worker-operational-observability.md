# 0017 — Expose bounded worker metrics, traces and health probes

Status: **accepted**
Date: **2026-09-05**

Implementation update, **2026-09-09**: formation-stage source-built Linux
metrics, probes, trace export and structured stdout are implemented with
[scoped validation](../tasks/cluster-formation-m4-checklist.md#post-reliability-regression-checkpoint--2026-09-09).
This does not close combined M4 performance acceptance, qualify published
packages/images, or implement later workload instruments.

## Context

Operators need to monitor and diagnose real workers before distributed
scientific execution is complete. Existing node diagnostics, logs and scaling
objectives do not define a scrape endpoint, process probes or trace export.
Operational telemetry must not become simulation observations, committed
provenance, audit authority, or an input to membership and step decisions.

The client protocol primarily specifies authenticated CBOR over HTTP/3.
Prometheus and deployment probes need a conventional HTTP surface, including
when a worker intentionally disables researcher/client admission. Requiring
cluster-administrator credentials for routine scraping would grant excessive
authority to monitoring infrastructure.

## Decision

### Optional capability, explicit exposure

Provide an `observability` Cargo feature in `orishu-worker` for a dedicated
HTTP diagnostics listener exposing Prometheus metrics and process probes.
Provide a separate `otlp-tracing` feature for OpenTelemetry-compatible trace
export over OTLP. Both feature names are implemented; runtime settings and
the supported local surface are documented in the
[observability guide](../orishu-observability.md#implemented-local-surface).
Keep optional exporter dependencies in the worker IO shell; neither feature
may add dependencies to the sans-IO membership core.

Both features are independently selectable and excluded from Cargo default
features. Official operator packages/images should include both capabilities
and publish their feature set, while minimal builds may omit them. Runtime
listener and trace export remain disabled by default in every build. Enabling
an unavailable capability is a startup error, never a silent no-op.

The diagnostics listener has an explicit bind address and port, separate from
peer and client admission flags. When enabled without a bind override it uses
loopback; the implemented default is `127.0.0.1:9168`. It supports
HTTP/1.1 for standard scrapers and probe clients. No external collector,
Prometheus installation or companion configuration is required to run a worker.

### One worker's diagnostics, separate from cluster authority

The listener exposes only read-only, node-local diagnostics:

| Route | Meaning | Response |
| --- | --- | --- |
| `GET /metrics` | Bounded snapshot of this process's operational instruments | Prometheus text exposition with the appropriate content type |
| `GET /livez` | The process's supervised control loop is responsive | `200` or `503`, bounded plain-text reason |
| `GET /readyz` | Local initialization is complete and configured roles can be served safely | `200` or `503`, bounded plain-text reason |
| `GET /startupz` | Required local initialization has completed | `200` or `503`; success latches for the process lifetime |

These routes are exceptions to the client API's CBOR/envelope rules and do not
relay another node's diagnostics. They cannot mutate cluster or workload state.
Scrape targets identify worker processes; ephemeral node/formation labels do
not establish endpoint identity or trust.

Liveness is based on local supervision progress, not an always-OK handler and
not simulation-step progress. Readiness becomes false during startup, shutdown
or a local transition that prevents safe service of enabled roles. A healthy
standalone, idle, stopped, or deliberately non-compute worker can be ready.
Joining or recovering may withhold readiness until required local identity,
policy and state are adopted. Peer loss, membership suspicion, no loaded
workload, insufficient cluster capacity, under-replication, or an unavailable
telemetry backend alone do not make the process dead or unready. Required
local storage or role initialization failures can withhold readiness.

Readiness is not a promise that a particular workload can be admitted or that
the cluster can commit its next step. SWIM remains the membership failure
detector. Probe handlers read bounded local health state and perform no peer,
storage, collector or solver IO per request. The task must define supervision
deadlines and the complete role/state matrix before implementation.

### Bounded metrics and traces

Metrics cover process/control-loop health, client requests, membership,
transport, admission and bounded queues first; runtime, sandbox, storage and
observation-delivery instruments arrive with their owning implementations.
Use a documented metric catalogue, units, types, histogram buckets and finite
label sets. Do not label series with workload/artifact digests, peer IDs,
partition IDs, request/trace IDs, raw paths, names, error text or plugin input.
Any identity/build info series must have a bounded current set and retire old
identity values on formation changes. Scraping reads aggregates, never scans
all partitions, artifacts or peer histories.

Trace client operations, admission, peer exchanges and later workload
lifecycle, step/halo/barrier, sandbox and artifact operations. Use static span
names, bounded attributes, configurable sampling and bounded asynchronous
batch export. Never create an unconditional span per cell, sample or guest
host call. Reviewed IDs may be trace attributes for correlation, not metric
labels. Bound and validate propagated trace context on authenticated client
and peer boundaries; a trace ID is never command identity or authorization.
Peer wire context requires explicit protocol documentation and compatibility
fixtures before it ships. Do not propagate arbitrary baggage or payloads.

No credentials, join tokens, certificates/private keys, authored expressions,
artifact bytes or guest-controlled diagnostic text are exported implicitly.
Exporter failures and full queues shed telemetry with bounded local diagnostics;
they cannot block membership, simulation commit, admission or shutdown.
Telemetry is best-effort and cannot replace required command decisions, durable
artifact records or provenance. Measure enabled/disabled and sampled trace
overhead on representative workloads; publish limits and measurements.

### Correlated operational logs — decision refinement, 2026-09-08

The operator selected structured standard-stream output for the PoC, explicitly
refining the initial stderr proposal to **stdout by default**. Keep event
production separate from output through a small worker-owned adapter. This
supports bare-metal service managers, container runtimes and cloud collection
without requiring a vendor SDK or local logging daemon to run the worker.
Kubernetes supports both standard streams; stdout is our chosen default, not
a Kubernetes-only requirement. See its [logging architecture](https://kubernetes.io/docs/concepts/cluster-administration/logging/).

Use bounded asynchronous delivery, bounded record fields/encoding and queue
bytes, explicit loss accounting and a finite whole-process shutdown deadline.
Slow, full, closed or unavailable output sheds diagnostic records without
blocking domain handlers. Runtime failure/flush diagnostics must not bypass
that boundary with synchronous standard-stream writes. Records correlate with
reviewed trace/span IDs but never become durable audit or scientific provenance.

Destination defaults do not select log levels, sampling or runtime enablement;
publish those settings and zero-sampling behavior in the implementation
contract. This decision does not enable metrics or trace export implicitly.

Research Rust logging crates before selecting and pinning dependencies; use
ecosystem instrumentation/adapter interfaces where they fit, while proving
Orishu's stricter byte, redaction and shutdown guarantees. Keep dependencies
in the IO shell. Unix-datagram and vendor sinks are future adapter extensions,
not PoC implementations, a new plugin registry or a reason to extract a crate.
Each future sink must preserve the same bounded/nonblocking contract.

This design is accepted. The 2026-09-09
[runtime implementation evidence](../tasks/cluster-formation-conformance.md#runtime-logging-and-local-span-receipt--2026-09-09)
covers local span/log receipt, explicit settings, live loss counters and held
stdout worker exit, with later
[reader recovery evidence](../tasks/cluster-formation-conformance.md#real-worker-stdout-reader-recovery--2026-09-09).
The [official Collector three-worker walkthrough](../tasks/cluster-formation-conformance.md#official-collector-formation-and-log-correlation--2026-09-09)
also verifies admission chains against the participating workers' stdout logs.
The selected [systemd/rootless-container extensions](../tasks/cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09)
now verify records collected by those runtimes against received local spans.
Final checkpoint/applicability and reviewed-overhead gates
remain in the [owning task](../tasks/implement-worker-observability.md#accepted-logging-output-decision).

### Access and configuration

Use the existing file < environment < CLI precedence. Specify listener,
route enablement, security, scrape limits, supervision thresholds, trace
destination, transport credentials, sampling, queue/batch limits and shutdown
flush deadline as typed startup configuration. No live configuration API is
introduced by this decision.

Loopback diagnostics may be read without credentials. Remote metrics require
TLS and a monitoring-only credential or mTLS policy that grants no client API
mutation rights; a loopback listener behind an authenticated TLS proxy is also
permitted. Do not reuse a join token or require a cluster-admin token to scrape.
An explicit remote probe-only exemption may expose the small health responses
on a restricted management network; it never exempts `/metrics` or any client
API. Document deployment controls and test the route separation. No wildcard
listener or unauthenticated remote metrics is enabled implicitly.

Bound requests, concurrent scrapes, response work/bytes and deadlines. Exporter
endpoints are operator-configured destinations; validate transport and TLS
settings and redact credentials. Listener/security misconfiguration fails
startup clearly; collector unavailability after valid configuration does not.

## Alternatives and consequences

Reusing only the client listener couples diagnostics to client admission and
its encoding/access policy. A dedicated listener adds a port and explicit
security configuration but provides a predictable monitoring surface without
changing scientific APIs. External log parsing alone lacks typed counters,
latency distributions and correlated operations. Always-on exporters would
violate config-free startup and add unnecessary dependencies and overhead.

The worker owns instrumentation in its IO adapters, consuming existing core
outcomes. No new shared telemetry crate is required without a second consumer.
Metric schema changes and probe meaning become operator compatibility
concerns. Release builds, dashboards, alerts, manuals and feature combinations
need validation alongside the implementation.

## Delivery and references

- [Implementation task](../tasks/implement-worker-observability.md)
- [Operator documentation and deployment task](../tasks/document-worker-observability.md)
- [Operational observability design](../orishu-observability.md)
- [Prometheus instrumentation guidance](https://prometheus.io/docs/practices/instrumentation/)
  and [metric naming](https://prometheus.io/docs/practices/naming/)
- [Kubernetes probe semantics](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-probes/)
- [OpenTelemetry sampling](https://opentelemetry.io/docs/concepts/sampling/)
  and [sensitive-data guidance](https://opentelemetry.io/docs/security/handling-sensitive-data/)
