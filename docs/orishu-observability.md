# Orishu operational observability

Status: **loopback probes and health metrics implemented; full instrumentation and tracing pending**

[ADR 0017](adr/0017-worker-operational-observability.md) defines feature-gated
worker metrics, health probes and trace export. This page is the operator
design entry point. The initial `observability` capability now serves local
health probes, three health gauges, nine owner counters, eight lane-slot gauges
and six client-service instruments; remote security, broader instrumentation
and OTLP remain implementation work. See the [worker manual](../apps/orishu-worker/README.md).

## Implemented local surface

Build with `--features observability` and explicitly set
`--observability.enabled true` to bind `127.0.0.1:9168`. The capability is not
in default features and runtime exposure is disabled by default. The optional
module reuses the existing HTTP stack, adding no dependency or core exporter.
Non-loopback binds are rejected until secured remote exposure is implemented.
Any forwarding proxy requires the ADR's independent TLS and monitoring-only
authorization controls.

Metrics and the three health probes are separately selectable route groups
through `observability.metrics` and `observability.probes`. Both default on
inside an explicitly enabled listener; disabled groups return `404`. No route
selection enables a listener by itself, and an enabled listener with no selected
group is rejected. These controls do not implement secured remote exposure.

The fixed Prometheus text 0.0.4 catalogue has no labels and twenty-six series:

| Metric (all prefixed `orishu_worker_`) | Type | Meaning |
| --- | --- | --- |
| `owner_responsive` | gauge, 0/1 | Same local supervision input as liveness |
| `ready` | gauge, 0/1 | Same local process input as readiness |
| `startup_complete` | gauge, 0/1 | Latched required initialization |
| `membership_transitions_total` | counter | Completed core transitions, including periodic/control work; not accepted operator commands |
| `stale_inputs_total` | counter | Inputs rejected by lifecycle-generation fencing |
| `core_diagnostics_total` | counter | Structured core diagnostic outcomes; payloads are not exported |
| `foreign_gossip_total` | counter | Foreign gossip items handed off without a workload owner in this PoC |
| `reliable_replies_total` | counter | Completed reliable transport replies; not domain acceptance |
| `send_failures_total` | counter | Missing routes, send overload and transport failures; not a peer-death count |
| `admissions_accepted_total` | counter | New members inserted by local core admission, even if the reply is lost; excludes learned membership and assignment replay |
| `admissions_rejected_total` | counter | Local core admission refusals, including refusals with redirect candidates; excludes pre-core codec/authentication failures and refused assignment replays |
| `admission_assignment_replays_total` | counter | Retained assignments successfully validated, encoded and rebound to a session; not reply delivery or a new admission |
| `{lane}_slots_in_use` | gauge | Occupied slots including queued messages and reserved permits; four fixed lanes below |
| `{lane}_slots_capacity` | gauge | Configured slot limit for that lane; four fixed lanes below |
| `client_requests_in_flight` | gauge | Client service handler futures currently executing |
| `client_requests_completed_total` | counter | Handler futures that returned, across all HTTP status classes; not delivered responses or accepted domain commands |
| `client_requests_rejected_total` | counter | Completed handler responses with HTTP 4xx, including authorization refusal, unknown paths and unsupported methods |
| `client_requests_failed_total` | counter | Completed handler responses with HTTP 5xx |
| `client_requests_cancelled_total` | counter | Handler futures dropped before returning, including cancellation/panic unwinding; not all client disconnects |
| `client_request_duration_seconds_total` | counter, seconds | Cumulative completed handler duration at microsecond resolution; excludes cancelled handlers and socket response delivery |

The lane names are exactly `peer`, `control`, `completion` and `shutdown`,
with capacities 64, 16, 64 and 1 respectively. These are local channel budgets,
not cluster member limits or running-task counts. A reservation consumes a
slot before a message is sent, so `slots_in_use` must not be interpreted as
queue length alone. Readings are bounded between zero and capacity, copy no
messages, and do not reserve or release capacity. Each lane is sampled
independently; consult liveness separately, including after owner closure.

Owner counters are unitless event/item counts from the owner's last published
projection. They accumulate across formation changes, reset on process restart
and saturate at `u64::MAX` rather than wrapping. After owner closure the last
published values remain available; consult liveness separately and do not
treat a successful scrape as evidence of progress. Reads copy nine integers,
never membership collections, and enqueue no owner work. Health and counters
are separate bounded reads, not an atomic cluster snapshot. The fixed output
is below 6 KiB even at the maximum counter value.

Admission counts are issuer-local events, not cluster-wide totals or joiner
catch-up completion. Repeated valid assignment replays increment only the replay
counter; malformed or conflicting retries rejected before the core increment
neither admission counter. These fixed owner aggregates remain active without
an exporter, like the existing owner counters; disabling telemetry is not a
claim of zero instrumentation cost. Enabled/disabled overhead acceptance remains
open.

Client accounting is shared across this process's configured client services
and attached only when the `observability` feature, runtime listener and metrics
route are enabled. It wraps handler execution after HTTP parsing/routing;
malformed input rejected by the transport before service entry is not counted.
No request bytes, identity, method, path or error text is retained or used as a
label. Diagnostics use a separate, uninstrumented service, preventing recursive
scrape accounting. Disabling runtime metrics removes the accounting middleware.

The five client counters saturate rather than wrap and reset on process restart,
not formation changes. Duration uses a saturating microsecond accumulator,
rendered in seconds. In-flight accounting is released when the future completes
or is dropped. Counter fields are independent atomic reads, not a transactional
snapshot; do not require equations across fields in one scrape. Completed time
divided by completed count supports aggregate mean handler latency, not a
percentile, network latency, admission duration or scientific step rate.

No unsupported workload measurements are emitted. Per-route request histograms and remaining queue instruments,
broader formation counters, histograms, release-artifact Prometheus integration, overhead
measurements and versioned dashboard/alert recipes remain in
P-OBSERVABILITY/P-OBS-DOCS.

The current source-build catalogue now has actual `promtool` validation and
Prometheus ingestion evidence. Run `make test-worker-prometheus` with the
pinned external tools; see [the test guide](testing-worker-prometheus.md) and
[local scrape example](../etc/prometheus-local.yml). This is local parser and
scraper acceptance, not remote security or release packaging. The local example
also includes three versioned, rule-engine-tested alerts for scrape failure,
sustained unreadiness and missing readiness telemetry; see the test guide for
threshold rationale, absence semantics and safe operator responses. Dashboards,
notification delivery and the remaining instrument-specific alerts are pending.
The same local harness stops and reaps Prometheus, verifies authenticated
worker lock/unlock and continued readiness/progress, then requires new metric
ingestion after restarting the scraper. This establishes standalone scraper
outage isolation, not OTLP-exporter failure or multi-worker overload acceptance.

The companion [formation scrape targets](testing-worker-prometheus.md#companion-formation-scrapes)
now check positive issuer-local insertion and lost-ACK replay counts through
real HTTP on three worker processes, including counter reset after restart.
They retain the full public churn journey. This supplements the backend test;
it is not a full probe matrix, multi-target Prometheus or OTLP acceptance.
The ejection companion additionally verifies real HTTP liveness/readiness/startup
after wire self-ejection and explicit return to standalone, including a healthy
survivor after peer loss. Transient joining/catch-up and the remaining probe
matrix are not established by that stable-state process journey.
The issuer-loss companion verifies live-but-unready, startup-latched responses
after loss and real admission retry exhaustion, while preserving unresolved
operation status and bounded operator stop behavior.

`diagnostics::tests::http_readiness_waits_for_real_admission_catchup` also
exercises the production HTTP router around real pinned-QUIC admission and
catch-up. Delaying maintenance holds the adopted, pre-baseline state: liveness
and startup remain successful while readiness is false. Starting maintenance
completes the real baseline and restores readiness. This is a two-owner runtime
fixture in one process with Unix HTTP diagnostics, not a process-level pause
during a partial transfer. HTTP coverage of interrupted/invalid transfers
remains distinct from the runtime's existing catch-up fault tests.

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

### Implemented owner-supervision input

The worker's serialized membership owner publishes a monotonic progress
timestamp on its existing one-second control-loop tick. Before the first tick
the input is `Starting`; at five seconds without progress it is `Stalled`.
Owner termination is `Closed`. Reads use a bounded local projection and never
refresh the timestamp or enqueue membership work. This five-second PoC budget
is a supervision threshold, not a measured service SLO or a peer-loss timeout.

`OwnerHealth` separates responsiveness from the formation portion of readiness:
standalone/joined are formation-ready; joining, unresolved admission and
catch-up are transitioning; ejected and stopping are not formation-ready.
Those phases remain responsive while the owner actually progresses. Membership
lock and lack of introducer readiness do not by themselves mark an owner dead.
Tests exercise every phase, exact deadline boundaries and a genuinely stalled
owner during both running operation and held shutdown while readers continue
polling.

The optional listener now projects `/livez`, `/readyz` and `/startupz`. The worker combines
owner supervision with a process-lifetime startup latch, required-listener
lifetime guards and shutdown intent. The composition root marks initialization
complete after configured listeners bind; before then readiness is withheld.
Any required listener task exit/panic/cancellation latches role failure. Repeated
initialization cannot clear that failure, and the PoC requires restart rather
than claiming unsupported listener recovery. Shutdown withholds readiness
immediately, before the owner necessarily consumes the request. The startup
latch remains set across later role failure, formation transition and shutdown.

Real HTTP tests cover healthy routes, safe membership lock/unlock, initialization, required-role failure
and closed-owner responses, including retained startup success. The lifecycle
fixture uses the production HTTP server/router over Unix with a real owner;
it keeps diagnostics reachable after owner shutdown to inspect the response,
so it does not establish executable listener shutdown ordering. A separate
executable test keeps eight previously served diagnostics connections open
with unfinished request heads through SIGTERM: normal probes/operator reads
remain available beforehand, the process exits within three seconds, and its
connections and client socket close without client-side cleanup driving success.
This covers shutdown isolation with spare diagnostics capacity, not saturation.
The same accepted-connection fixture also verifies expiry without shutdown:
authenticated operator lock/unlock completes while requests are held, all eight
connections close under the head-timeout observation budget, and normal probes
and metrics remain available afterward. This does not cover active trickle
traffic. A separate full-budget test accepts sixteen connections, prevents an
extra request from being served during the bounded check, verifies authenticated
operator control before head expiry, and verifies normal serving after all held
connections expire. Probes share that diagnostics budget and can be unreachable
under saturation even while the owner is healthy; transport failure is not
itself proof of owner death. This is scoped HTTP/1.1 connection pressure, not
arbitrary flood or slow-response/HTTP2 acceptance.

The combined `observability,formation-fault-test` HTTP fixture also holds the
real owner after acknowledged shutdown. Repeated scrapes and readiness reads
cannot extend its five-second supervision deadline: liveness changes from
success to failure while metrics remain readable, readiness remains failed,
and startup remains latched. Releasing the fixture then proves closed-owner
responses. This development-only Rust seam exposes no CLI or network fault
control. It proves stalled-shutdown probe semantics with a retained Unix HTTP
listener, not an independently stalled running process or executable shutdown
ordering. A second fixture pauses the running owner without requesting
shutdown. Probes initially remain healthy during the supervision grace period,
then liveness and readiness fail while startup and metric serving remain
available. The transition counter stays fixed during the hold. Releasing the
pause lets the owner's actual tick restore health and transition progress;
formation identity, node identity and participation remain unchanged. This
uses the same development-only Rust control boundary, not an OS process freeze
or a remotely exposed pause API.

Joining, admission recovery, overload and secured-remote HTTP matrices remain
incomplete. Future required local
roles must register their initialization and failure before enabling readiness;
the current gates cover formation-owner and configured client-listener roles,
not unimplemented solver/storage services. Required-role
initialization cannot be inferred from member count or `introducerReady`.
Feature gates, listener configuration/security, metrics, trace export and
operator deployment recipes remain in the companion tasks.

### Remaining delivery

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
