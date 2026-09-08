# Orishu operational observability

Status: **local probes, metrics and sampled trace export implemented; full instrumentation and distributed tracing pending**

[ADR 0017](adr/0017-worker-operational-observability.md) defines feature-gated
worker metrics, health probes and trace export. This page is the operator
design entry point. The initial `observability` capability now serves local
health probes, three health gauges, thirteen owner counters, eight lane-slot gauges
and seven client-service instruments, plus inbound handshake and reliable peer
exchange metrics. The
[mTLS proxy recipe](testing-worker-monitoring-proxy.md) provides a tested
source-build secure-proxy path; native remote diagnostics, broader
instrumentation and distributed tracing remain work. See the [worker manual](../apps/orishu-worker/README.md).

The [user-service](testing-worker-user-service.md) and
[rootless container](testing-worker-container.md) recipes have scoped
source-built lifecycle and monitoring evidence. They distinguish process
health from service/container state and keep local namespace access separate
from remote monitoring authorization; neither qualifies a published deployment.

The [tracing configuration](orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable)
now enables local client-service spans in builds with `otlp-tracing`.
The capability remains excluded from default features and disabled at runtime
by default. Enabling an omitted feature or omitting the explicit collector
endpoint fails startup; credential errors fail before worker state/listeners.
Disabled tracing does not load credential files or create an exporter.

Enabled client-service middleware samples fixed-name root spans, retaining no
request paths, headers, credentials or payloads. Handler HTTP 4xx/5xx responses
map to rejected/failed diagnostics; successful handler completion is neither
domain acceptance nor response delivery. Cancellation sheds a cancelled span
best-effort. No client trace context is consumed and no peer context propagates.

The worker wires the existing bounded `SpanQueue`, protobuf codec,
`CollectorFiles` and `ExportLoop` to the typed startup budgets. HTTP delivery
uses an explicit destination with no proxy, redirects, automatic retry or
decompression. Request encoding and streamed response consumption obey byte
caps, and whole-attempt timeouts include the response body. HTTPS currently
uses an explicit CA bundle or the bounded default Linux system bundle described
in the configuration guide; optional mTLS/token files use the bounded Unix
loader. Other native trust-store layouts remain unsupported without explicit CA input.

The exporter flushes on count or the first buffered record's deadline and
splits requests at the byte cap. Shutdown stops client services, signals the
exporter, then joins its bounded drain before process exit. Interrupted
requests are not retried, and late active spans cannot extend the drain.
A fixed aggregate delivery report and queue sampling/drop counts are emitted
on shutdown. When metrics and tracing are both enabled, the existing `/metrics`
route also exposes [live trace counters](#live-trace-delivery-and-loss-counters).
Trace/span-correlated logs remain unfinished.

The real worker executable has local-collector evidence for a cluster-summary
request, no collector connection while disabled or enabled with zero sampling, payload-label exclusion,
collector-outage status continuity and graceful shutdown. Primitive mTLS,
partial-response, byte/count/timer and stalled-drain fixtures supplement this
process test; they do not establish the complete secured-deployment or
distributed-tracing matrix. M4 still requires cross-peer traces, broader
formation instrumentation, overhead evidence and the operator handoff.

A real-worker mTLS fixture now loads its configured CA, client certificate/key
and collector-only token, verifies client certificate possession at the receiver,
and decodes a client-service span without credential material. Invalid CA PEM,
mismatched key, public token permissions and token symlinks fail before worker
state/socket creation or collector connection. Wrong collector trust, server
name and client identity fail TLS while the worker continues to serve unchanged
formation/node identity and member count and shuts down normally. These are
source-built local fixtures, not platform-root, certificate-rotation or remote
deployment certification.

Middleware-level tests additionally preserve handler responses under full
completed queues, exhausted active slots and a closed exporter. HTTP 202 remains
service completion, not completed asynchronous domain work. Aborting an actual
handler future releases its active slot and records cancellation best-effort.
These fixtures do not replace process-level overload or peer-operation tests.

The actual-worker trace-pressure fixture additionally holds one export and
fills a one-record queue while authenticated lock/unlock and summary requests
complete. Exact shutdown counters establish four queue drops; releasing the
collector delivers two retained records and a fresh request proves capacity
reuse. A final unanswered export remains held until graceful worker exit,
exercising the 100 ms exporter drain against a ten-second attempt timeout.
This is bounded local control-plane evidence, not arbitrary concurrent client,
peer or fleet-scale overload acceptance.

## Implemented local surface

For incident triage, follow the [worker monitoring runbook](worker-monitoring-runbook.md).
It connects probes and metrics to bounded, directly targeted operator reads;
it never treats telemetry as membership or recovery authority.

The optional [formation dashboard](testing-worker-dashboard.md) presents this
catalogue per selected scrape target, with explicit missing/unavailable states,
separate trace-loss rates and graph links. It is a monitoring-backend example,
not a worker endpoint or another source of membership authority.

For actual collector receipt, use the
[pinned local OpenTelemetry Collector walkthrough](testing-worker-otelcol.md)
and `make test-worker-otelcol`. It validates a trace-only loopback receiver and
private JSONL output, including disabled/zero sampling and real collector
shutdown/recovery. This is distinct from the Prometheus harness's HTTP stub;
neither establishes cross-peer correlation, remote collector deployment or
durable retention.
The [Collector mTLS recipe](testing-worker-otelcol-mtls.md) additionally tests
receiver client-certificate enforcement and worker server trust/name checks
with collector-only credentials, including startup refusal and outage recovery.
Its current deployment evidence is loopback, not cross-host qualification.

For reproducible runtime-disabled/enabled request, CPU, memory and scrape
measurements, see the [manual local overhead harness](testing-worker-overhead.md).
Its single-worker scope and unreviewed performance threshold do not close the
M4 formation/telemetry overhead acceptance requirement.

Build with `--features observability` and explicitly set
`--observability.enabled true` to bind `127.0.0.1:9168`. The capability is not
in default features and runtime exposure is disabled by default. The optional
module reuses the existing HTTP stack, adding no dependency or core exporter.
Native non-loopback binds remain refused; remote access uses the separately
documented secured-proxy alternative.
The direct listener accepts only exact, query-free GET paths. Unknown/disabled
paths return 404; queries on enabled paths return 400; other methods on enabled
query-free paths return 405 with `Allow: GET`. HEAD is rejected without a body,
not treated as a health check or scrape. These dispatched errors use fixed
plain-text reasons and `Cache-Control: no-store`, never reflected request data.
Use the canonical paths rather than encoded, case or slash aliases. See the
[HTTP contract](protocol-client.md#worker-operational-diagnostics) and
[real-process method/error evidence](tasks/cluster-formation-conformance.md#direct-diagnostics-method-and-error-contract--2026-09-07).
Any forwarding proxy requires the ADR's independent TLS and monitoring-only
authorization controls.

Metrics and the three health probes are separately selectable route groups
through `observability.metrics` and `observability.probes`. Both default on
inside an explicitly enabled listener; disabled groups return `404`. No route
selection enables a listener by itself, and an enabled listener with no selected
group is rejected. These controls do not implement secured remote exposure.

The base Prometheus text 0.0.4 catalogue has 151 series, extended to 163 when
local tracing is enabled. Only the five duration histograms use labels, each
with eight fixed `le` bucket values; other series have no worker-generated labels.
The original health/owner/lane/client instruments are listed here; peer and
trace extensions are specified below:

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
| `peer_decode_rejections_total` | counter | Membership packets refused by owner-side session binding or membership decoding before reaching the core; not all peer failures |
| `swim_packets_received_total` | counter | Decoded Ping, Ack, PingReq, PingReply and Announce packets, including repeats and uncorrelated replies; not successful probes |
| `anti_entropy_packets_received_total` | counter | Decoded PullRequest and PullReply packets; not completed rounds or converged state |
| `gossip_items_received_total` | counter | Envelope gossip items plus PullReply deltas, including repeated, ignored and foreign items; not newly merged records |
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
| `client_request_duration_seconds` | histogram, seconds | Completed handler duration across all statuses; eight fixed buckets plus `_sum` and `_count`, with the same exclusions as the cumulative counter |

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
treat a successful scrape as evidence of progress. Reads copy thirteen integers,
never membership collections, and enqueue no owner work. Health and counters
are separate bounded reads, not an atomic cluster snapshot. Allow a 32 KiB
scrape-consumer budget, including the inbound peer counters below and optional
trace counters, even at maximum counter values. This raises the earlier
50-series base catalogue's 16 KiB budget; existing names and meanings are unchanged.

### Membership deadline and abandonment counters

Eight unlabelled counters observe the serialized owner's completed core
transitions. They follow runtime metrics enablement: probes-only or disabled
collection allocates no storage, reads no clock and does not scan diagnostics;
compiled-out collection is zero-sized. Enabled startup shares one fixed array
with diagnostic readers. Recording examines only the already-bounded returned
diagnostics, retains no payloads and never enumerates membership or timer history.

All names below use prefix `orishu_worker_membership_`:

| Suffix | Observed event |
| --- | --- |
| `direct_probe_deadlines_total` | Core consumes a current direct or relay probe timer |
| `indirect_probe_deadlines_total` | Core consumes a current indirect probe timer |
| `suspicion_deadlines_total` | Core consumes a current suspicion timer; inspect actual liveness separately |
| `anti_entropy_deadlines_total` | Core consumes a current reconciliation timer |
| `join_retry_due_total` | Core consumes a current join retry/backoff timer, not an admission refusal |
| `stale_timer_inputs_total` | Core emits `StaleTimer`; no current deadline is counted for that token |
| `join_abandoned_total` | Core emits `JoinAbandoned` on retry-budget exhaustion |
| `anti_entropy_abandoned_total` | Core emits `AntiEntropyAbandoned` on timeout or continuation-budget exhaustion |

The owner supplies the actual update's timer input, if any, and its returned
diagnostics. A matching `StaleTimer` excludes that input from deadline counts.
Owner lifecycle rejection occurs before the update and remains in the existing
`orishu_worker_stale_inputs_total`, not these counters. Cancelling or clearing
an armed timer on ACK, leave, adoption or shutdown does not count expiry.
With no eligible indirect helper the core can cancel that phase and begin
suspicion immediately; direct expiry need not imply indirect expiry.

Abandonment and deadline events overlap but are not equivalent. A reconciliation
timeout emits both, whereas too many incomplete replies emit abandonment without
timer expiry. Join retry exhaustion can follow a timer, refusal or session rebind;
it does not prove that an introducer never accepted the assignment. Preserve the
public operation's unresolved outcome and the recovery runbook's stop conditions.
Do not sum these stages into failures, successful probes or globally dead peers,
and do not subtract completed transport requests to infer reconciliation success.

These are process-lifetime saturating `u64` counts, retained across formation
changes and owner shutdown, reset on process restart. Scrapes read eight relaxed
atomic values without owner requests or collector work; readings are not one
transaction. A standalone worker with no attempted join or peer round legitimately
reports zero. No new timer, deadline, retry, health or membership decision is
introduced. These counters do not provide probe RTT, completed-round counts,
convergence timing or exhaustive classification of all core diagnostics.

### Admission-state catch-up outcomes

Twelve unlabelled saturating counters use `orishu_worker_catchup_`. They observe
one owner-authorized receiver job and its later owner completion, never one
counter increment per page or a second admission decision.

| Suffix | Boundary |
| --- | --- |
| `started_total` | Owner selected a registered source and created a catch-up job; a refused/no-source preparation or operation-status read is not a start |
| `transfer_validated_total` | Receiver validated all pages, content proof and credential; this is not owner adoption |
| `transfer_binding_rejected_total` | Receiver rejected source/certificate or formation binding |
| `transfer_invalid_total` | Receiver rejected encoding/schema, correlation, ordering, content proof, completeness or credential binding |
| `transfer_rejected_total` | Source returned a valid structured refusal, including unavailable/expired source history; not a join/admission refusal |
| `transfer_unavailable_total` | Shared exchange failed (including malformed/truncated framing), timed out or refused capacity; use reliable-exchange counters for that lower-level distinction |
| `transfer_timed_out_total` | The receiver's complete 25-second budget expired across exchanges/pages; not a five-second individual exchange deadline |
| `transfer_cancelled_total` | Job dropped before a receiver outcome, including a prepared job that was never executed |
| `owner_adopted_total` | Owner accepted the current completion, installed safe admission state and credential, and entered Joined |
| `owner_not_adopted_total` | Current eligible completion did not enter safe Joined state; includes failed/cancelled transfers and unsuccessful/unsafe baseline adoption |
| `owner_fenced_total` | Owner rejected completion due to generation/attempt, participation, total lifecycle deadline or source/session eligibility |
| `owner_abandoned_total` | Observation dropped without a terminal owner decision, including closed-owner completion disposal or owner failure before that decision |

Transfer and owner counters are separate stages, not disjoint outcomes of one
combined counter. Each normally disposed job contributes one transfer outcome
and one owner outcome. For example, a validated transfer completed after leave
can be owner-fenced; a cancelled job can produce an eligible non-adoption; a
validated result delivered after owner shutdown is abandoned, not adopted or
retroactively a cancelled transfer. A process crash need not flush outstanding
observations. Snapshots read independent atomics, so stage sums are not an
instantaneous transactional assertion.

The receiver records its actual result before handing the private baseline and
credential through the existing reserved completion. A non-cloneable IO guard
accounts for drop/cancellation and follows that completion to the owner. The
owner records its actual adoption/fencing decision; telemetry cannot select it.
One job covers Begin, all bounded pages and Confirm. A new authorized retry is
a new job under the unchanged three-attempt/90-second lifecycle budget; metrics
never reset that budget, and replay does not itself authorize another transfer.

Counters retain process-lifetime totals across formation changes and reset on
process restart. They reuse the optional owner metrics allocation, retain no
identity, baseline, token, page, exception text or clock, and add no scrape-time
IO or traversal. The guard is zero-sized without `observability`; disabled
collection allocates no metrics record. No exporter or trace decision is needed
to use these counters. Scrapes do not create catch-up jobs or advance health.

For failures, consult authenticated join-operation status and source/session
state; transfer validation alone cannot justify enabling introduction. Do not
automatically restart, readmit, select a new admission attempt or clear an
exclusion based on these counters. They are operational diagnostics, not durable
admission history or cluster convergence evidence. The complete catalogue stays
within 32 KiB: maximum-width validation budgets integer counters/gauges/buckets
separately from fractional duration sums/totals and retains the 4 KiB trace
extension reserve.

### Inbound peer handshake counters

Ten unlabelled counters now cover the inbound QUIC/TLS and initial application
handshake boundary. They are collected only when diagnostics and its metrics
route are enabled; a probes-only or runtime-disabled worker allocates no
inbound counter storage. Omitted `observability` builds use a zero-sized no-op.
When collection is enabled but no peer listener is configured, these counters
are present at zero: no inbound handshakes occurred. They do not describe
outbound dials, all UDP packets, catch-up transfers or registered-session traffic.

| Metric suffix (prefix `orishu_worker_peer_inbound_`, suffix `_total`) | Event |
| --- | --- |
| `tls_completed` | Server QUIC/TLS handshake completed; application identity/admission checks remain |
| `tls_failed` | QUIC returned a non-timeout error before TLS completion, including certificate/profile refusal; not an authentication-only count |
| `tls_timed_out` | Outer handshake deadline or QUIC transport timeout expired |
| `tls_cancelled` | Pending TLS future dropped before an outcome was observed |
| `handshake_completed` | Initial application exchange served and owner session returned; neither a delivered reply nor accepted admission |
| `handshake_failed` | Initial exchange returned a non-timeout error, including codec, owner, pool-capacity or transport failures |
| `handshake_timed_out` | Initial application or nested exchange deadline expired |
| `handshake_cancelled` | Pending initial application exchange future dropped |
| `tls_capacity_refused` | Incoming connection refused at the 16-concurrent-TLS gate |
| `connection_capacity_refused` | Incoming connection refused at the 64-connection-task gate, after passing the TLS capacity gate |

Each observed stage terminates in exactly one of its four outcome counters;
pending work has not yet contributed an outcome. Capacity refusal occurs before
either stage and is counted only at the first refusing gate. Cancellation
means future disposal, including shutdown/abort, not every peer disconnect:
a returned IO/owner error counts as failure. The two existing five-second
stage budgets are unchanged. Successful TLS and application handshakes are
distinct from core admission, so do not sum them with admission counters.

These are process-lifetime, saturating `u64` event counts, retained across
formation changes and reset on restart. Updates allocate no per-event storage;
enabled startup allocates one fixed record with ten counters and the pressure
projection below. Event counts use relaxed atomic reads; pressure uses the
separate short lock described below. Neither performs owner requests,
connection scans or exporter IO.
They are not a transactional snapshot and cannot establish peer death, formation
convergence or globally accepted commands. Check public operation status and
the recovery runbook before any mutation; these counts alone never justify
readmission, credential replacement or removing an exclusion.

### Inbound connection-capacity gauges

Four unlabelled gauges use the `orishu_worker_peer_inbound_` prefix:

| Suffix | Reading |
| --- | --- |
| `tls_slots_in_use` | Occupied permits in the actual inbound TLS semaphore, including reserved work not yet polled |
| `tls_slots_capacity` | That adapter's TLS permit limit, currently 16 |
| `connection_slots_in_use` | Last published connection-task `JoinSet` length, including pending TLS, initial application handshake, serving and completed-but-uncollected tasks |
| `connection_slots_capacity` | That adapter's connection-task limit, currently 64 |

These measure adapter budgets, not established sessions, admitted members or
all incoming/outgoing worker connections. TLS capacity is checked before
connection-task capacity; independent readings need not describe one instant.
Connection occupancy is published after spawn and collection, without scanning
sessions on scrape. TLS usage reads the actual semaphore. A short IO-shell
lock protects only the weak semaphore reference and three scalar values; no
owner work, allocation, clock, network IO or collection traversal occurs under it.

With metrics collection enabled, all four are zero before peer-adapter startup
and after its lifetime ends, including abort. Capacity remains published during
the adapter's bounded shutdown until its lifetime guard drops; these gauges
do not count cleanup tasks that outlive that guard. No configured/running peer
listener therefore means zero capacity, not an occupied or failed listener.
The cumulative inbound counters retain their process-lifetime totals. Omitted
or runtime-disabled collection retains no pressure state and exposes no such
metrics; zero is not a substitute for unavailable collection. The unchanged
32 KiB response limit includes these four additional samples.

### Registered-session capacity gauges

Four unlabelled gauges use the `orishu_worker_peer_registry_` prefix:

| Suffix | Reading |
| --- | --- |
| `slots_in_use` | Retained authenticated application-session entries, including closed/expired entries awaiting the owner's normal pruning |
| `slots_capacity` | Total retained-entry bound, currently 64 while the registry exists |
| `provisional_slots_in_use` | Retained provisional subset: incoming applicants **and outgoing pinned-introducer bindings** |
| `provisional_slots_capacity` | Shared provisional-subset bound, currently 16 while the registry exists |

Registration is not admission, live-socket count or introducer readiness.
Promotion removes an entry from the provisional subset without removing its
total slot. Refused/duplicate registration does not add a slot. Closing a
connection does not itself remove a retained entry; the owner's existing
synchronization prunes closed, expired and invalid bindings. No new admission
gate or pruning schedule is introduced by these instruments.

The IO owner publishes all four values as one coherent constant-size atomic
reading after registry changes. Scrapes do not scan sessions, issue an owner
command or advance supervision. The aggregate contains no peer identity,
endpoint, certificate, label or formation history. It reuses the optional
owner metrics allocation; runtime-disabled collection allocates no record,
and compiled-out collection retains no diagnostic counter or projection.

With collection enabled, all four values are zero before the owner starts and
after its registry drops, including owner abort. An active empty registry
publishes zero occupancy with capacities 64/16, including when no peer listener
is configured. Formation changes prune old bindings but retain the active
registry's capacity; owner termination withdraws it. Zero capacity describes
projection availability, not proof of complete transport cleanup or a health
decision. Omitted/disabled collection exposes no such metrics; the existing
cumulative owner event counters retain their totals after registry withdrawal.

For sustained occupancy, compare the provisional subset, inbound task/TLS
budgets and rejection counters, then inspect authenticated formation/operation
status. Do not infer cluster size, identify a failing peer, restart or readmit
from this aggregate alone. These four samples fit the unchanged 32 KiB response
limit; they add no configuration setting or production alert threshold.

### Outbound dial and TLS metrics

The process-wide dialer adds 31 samples: eight terminal stage counters, two
duration histograms (ten samples each), one capacity-refusal counter and two
slot gauges. Collection follows metrics enablement, not tracing or peer-listener
configuration: an explicitly requested join can dial without a peer listener.
Disabled collection allocates nothing and reads no timing clock; compiled-out
collection is zero-sized. Enabled startup owns one fixed allocation, with O(1)
accounting per stage and bounded snapshot work independent of peer history.

All names use `orishu_worker_peer_outbound_`. `{stage}` is the static name
fragment `attempt` or `tls`, never an endpoint or peer label.

| Suffix | Type | Meaning |
| --- | --- | --- |
| `{stage}_completed_total` | counter | Attempt returned a pending handshake reply, or one candidate completed QUIC/TLS; neither proves owner validation or admission |
| `{stage}_failed_total` | counter | Attempt failed configuration or exhausted candidates; TLS candidate failed initiation or returned a non-timeout QUIC error |
| `{stage}_timed_out_total` | counter | Attempt's 15-second outer budget expired; TLS candidate's five-second budget or QUIC transport timeout expired |
| `{stage}_cancelled_total` | counter | Polled stage future dropped before observing its terminal result |
| `{stage}_duration_seconds` | histogram | Terminal stage lifetime, including failures and cancellation, excluding pre-stage capacity refusal |
| `capacity_refused_total` | counter | Four-slot process-wide dial gate refused work without waiting or starting either stage |
| `slots_in_use` | gauge | Occupied outbound-attempt permits, not open connections or membership size |
| `slots_capacity` | gauge | Configured outbound-attempt capacity, currently four |

An attempt starts after acquiring a permit and ends when the dial adapter
returns or its future is disposed. It includes client-TLS configuration, every
candidate and the initial reliable exchange, but not request encoding, target
construction, owner registration, JoinReq/admission or catch-up. As before, the
15-second IO budget starts after local TLS configuration; the measured lifetime
also includes that configuration. TLS timing
starts immediately before each `connect_with` and ends before the application
exchange. At most eight sequential candidates share the existing total budget;
candidate fallback is part of the same attempt. Bootstrap, pending-join
reconnect and admitted-peer reconnect share this dialer and catalogue.

Each started stage contributes exactly one terminal outcome and duration.
Individual candidate timeouts can end in a completed attempt after fallback,
or a failed attempt when candidates are exhausted. An application exchange
error likewise causes fallback/exhaustion; consult reliable-exchange counters
for its timeout/capacity classification. Expiry of the outer attempt cancels
any still-pending nested stage; it does not manufacture another TLS timeout.
Dropping a returned pending reply later does not revise the completed transport
count. Never sum nested stages or reliable exchange lifetimes as independent
operations or interpret transport success as accepted membership.

The finite histogram boundaries are 0.001, 0.005, 0.025, 0.1, 0.5, 1 and 5
seconds plus `+Inf`, inclusive after truncation to microseconds. Durations over
five seconds, including total-attempt timeout, occupy the overflow bucket.
Counts, disjoint internal buckets and duration sums saturate at `u64::MAX`;
rendered buckets/counts are cumulative and saturating. Readings are independent
relaxed atomics, not a transactional snapshot. They survive formation changes
and reset on process restart. No endpoint, certificate, token or error text is
retained. These metrics do not change dial limits, retry policy, wire profile,
health or domain authority, and do not establish all formation-stage timing.

### Reliable peer exchange metrics

The worker-wide reliable-exchange pool exposes 34 additional samples: ten
terminal-outcome counters, two duration histograms (ten samples each), two byte
counters and two slot gauges. Collection follows the same metrics enablement
as inbound handshakes. Disabled collection allocates no counters and reads no
timing clock; compiled-out collection is zero-sized. No peer IDs, phase names,
packet fields, credentials or raw errors become labels or retained metadata.

All names below have prefix `orishu_worker_peer_reliable_`. `{role}` is a static
name fragment, either `request` (locally initiated request/reply) or `serve`
(accepted stream, owner callback and reply). Both cover initial handshakes,
membership and admission-state catch-up through the shared pool.

| Metric suffix | Type | Meaning |
| --- | --- | --- |
| `{role}_completed_total` | counter | Adapter returned success; not domain acceptance or confirmed remote receipt |
| `{role}_failed_total` | counter | Non-timeout/non-capacity error, including codec, local payload validation, transport and owner failure |
| `{role}_timed_out_total` | counter | Outer exchange or nested transfer deadline expired |
| `{role}_capacity_refused_total` | counter | Shared exchange pool refused an attempt without waiting for a slot |
| `{role}_cancelled_total` | counter | Polled exchange future dropped before returning an outcome |
| `{role}_duration_seconds` | histogram | Lifetime from first poll through any terminal outcome, including refusal and cancellation |
| `bytes_sent_total` | counter, bytes | Stream bytes accepted by local QUIC writes, including partial transfers |
| `bytes_received_total` | counter, bytes | Stream bytes consumed by local reads, including subsequently rejected input |
| `slots_in_use` | gauge | Shared permits held by inbound/outbound exchanges, including owner-reply waiting |
| `slots_capacity` | gauge | Fixed process pool capacity, currently 64; not an owner-mailbox lane or member limit |

Each polled attempt contributes exactly one outcome and one duration sample.
An unpolled future contributes nothing; a retry is another transport attempt,
not a new admission. Duration includes stream-credit waiting, IO, required FIN
and, for serving, the owner callback. It excludes preceding dial/TLS and
work queued before the pool call; it is not network RTT or convergence latency.
Inclusive bucket bounds are 0.001, 0.005, 0.025, 0.1, 0.5, 1, 5 and `+Inf`
seconds. `_sum` uses microsecond resolution; `_count` equals the cumulative
`+Inf` bucket. Unlike the client histogram, cancelled attempts are included.

Byte counts include the four-byte frame prefix, but not QUIC/TLS headers,
retransmission overhead, unread buffered bytes or datagrams. Per-exchange local
counts are published at termination, including failure/cancellation, not on
each IO chunk. A held exchange may therefore occupy a slot while its bytes
have not yet appeared. No transfer helper outside this pool is implicitly
covered. Refusal at the registered connection's 16-stream-task gate precedes
the pool and is counted in the separate traffic catalogue below. Pool refusal
before reading consumes zero bytes; malformed input
counts only the bytes actually consumed before rejection.

Counters, duration sums and buckets saturate at `u64::MAX`, survive formation
changes and reset on process restart. Scrapes copy fixed relaxed atomic
readings and sample available permits without owner/collector IO or connection
scans; fields are not one transactional snapshot. Related ingress, exchange
and owner counters describe different boundaries and must not be summed into
a single operation count. With collection enabled and no exchanges, zero
counts and an unused 64-slot pool are real observations, not placeholders.

Use timeouts/failures alongside slot pressure and operation status to narrow an
incident. They do not identify a failed peer or justify automatic readmission,
identity replacement or exclusion removal. The separate
[outbound catalogue](#outbound-dial-and-tls-metrics) covers dial/TLS timing;
inbound handshake and broader formation-stage timing remain separate work.

### Datagram and pre-pool traffic counters

Eight unlabelled counters cover the production datagram submission adapter and
registered-connection receive loop. Names below have prefix `orishu_worker_peer_`.
They share one fixed optional allocation across owner and transport tasks;
collection follows metrics enablement, adds no clock/exporter IO, survives
formation changes and resets on process restart. Disabled collection allocates
no counters; omitted collection is zero-sized. Each event takes constant local
work and retains no packet, peer identity or error text. Values saturate at
`u64::MAX`; independent atomic reads are not a transaction.

| Metric suffix | Counted observation |
| --- | --- |
| `datagrams_submitted_total` | QUIC accepted a payload into its local send path |
| `datagrams_submit_refused_total` | Adapter refused the delivery class, membership/negotiated size or unavailable datagram capability before submitting |
| `datagrams_submit_failed_total` | QUIC returned an error after the adapter's checks, including a closed connection |
| `datagrams_received_total` | QUIC returned one complete payload to a registered receive loop, before owner/session/codec validation |
| `datagrams_oversized_total` | Received payload exceeded the unchanged 1200-byte membership ceiling; a subset of received datagrams, discarded before owner allocation/delivery |
| `datagram_bytes_submitted_total` | Payload bytes accepted into the local send path; refused/failed attempts contribute none |
| `datagram_bytes_received_total` | Payload bytes returned by QUIC, including oversized or subsequently invalid input |
| `stream_capacity_refused_total` | Registered connection's 16-task gate refused an accepted stream before it entered the shared reliable pool |

Submission outcomes are mutually exclusive for each adapter call. Encoding or
missing-route failures before that call are not covered; use the existing
owner send-failure diagnostic separately. Both ordinary member sends and
voluntary-departure submissions use the adapter. Raw test-only fault injections
and external peers bypassing it are not local submission observations.

Bytes are QUIC application payload, not UDP/QUIC/TLS overhead. There is no stream
length prefix on datagrams. Neither submission nor reception establishes a
successful probe or accepted command. QUIC may discard older queued datagrams
to accept a newer submission, and datagrams may be lost or reordered; these
counters do not observe all such loss. Subtracting independent send/receive
totals is not a delivery-loss measurement. Reception before domain validation
differs from the owner catalogue's decoded SWIM/anti-entropy activity.

Stream-task refusal is distinct from negotiated QUIC stream-credit waiting and
the process-wide reliable pool's capacity refusal. It contributes no reliable
exchange outcome or bytes because the pool was never entered. Completed tasks
retain a slot until their completion is collected, so refusal need not mean
sixteen bodies are still being read. Do not sum
these different stages into a single operation count or use them to trigger
automatic restart/readmission. With collection enabled but no peer IO, all
eight counters are present at zero. The full catalogue still fits 32 KiB.

### Live trace delivery and loss counters

With both capabilities built and both tracing and the metrics route enabled,
the same listener adds twelve unlabelled counters. Each name below has prefix
`orishu_worker_trace_` and suffix `_total`:

| Name | Counted event |
| --- | --- |
| `sampled_out` | Operations excluded by local sampling, including a zero sampling rate |
| `active_full` | Sampled operations shed at the active-span capacity |
| `queue_full` | Completed spans shed at completed-queue capacity |
| `closed` | Operations or completed spans shed after consumer closure |
| `invalid_source` | Local entropy, identity or timestamp failure |
| `enqueued` | Completed records admitted to the export queue, not delivered |
| `accepted` | Spans reported accepted by a valid collector response |
| `rejected` | Spans reported rejected by a valid partial-success response |
| `failed` | Spans in failed attempts; remote acceptance may be unknown |
| `encoding_dropped` | Records shed on encoding failure |
| `shutdown_dropped` | Records abandoned by shutdown, including interrupted delivery |
| `warnings` | Valid responses containing discarded collector warning text, not a span count |

These counts accumulate for one process, survive formation changes and saturate
at `u64::MAX`. The queue and exporter publish independent atomic values; a
scrape is not a transaction or a delivery receipt. Scraping copies twelve
integers without waiting for the collector or membership owner, draining the
queue, or generating a client span. Final shutdown counts can appear only in
the shutdown report if the listener has already stopped; a final scrape is
not guaranteed.

Omitted or runtime-disabled tracing omits these series, rather than reporting
invented zeros. Enabled zero sampling still exposes real sampling counts.
Tracing alone opens no diagnostics listener; diagnostics never enables tracing.
The extension is below 4 KiB at maximum values. Allow a 32 KiB response budget
for the combined 163-series surface, including catch-up, membership deadlines and peer IO metrics.
Existing metric names, authorization and probe meanings do not change.

The [extended Prometheus harness](testing-worker-prometheus.md#ingest-trace-counters-through-prometheus)
now ingests all 163 series and requires fresh accepted-span values after a
collector failure/recovery, with authenticated worker control throughout.
Run `make test-worker-trace-prometheus` with the pinned tools. This is local
source-build ingestion evidence, not a collector deployment or dashboard
certification. [Optional trace-loss warnings](testing-worker-prometheus.md#optional-trace-loss-warnings)
have separate synthetic rule-engine tests and real-server loading checks;
their thresholds are examples, not production SLOs or restart policy.

When traces are missing, first check scrape availability and the enabled build
and runtime settings. Compare counter increases across scrapes: sampling is
intentional, active/queue shedding indicates local pressure, and failed or
rejected delivery points to the collector path. Inspect configured destination,
trust and collector capacity through their normal operator interfaces; never
print credentials to diagnose them. Do not derive an exact lost-span balance
from one non-atomic scrape, automatically increase buffers, restart a worker,
or change membership based on telemetry loss. Use the worker's authenticated
operation status for domain outcomes.

### Owner and client counter interpretation

Peer decode rejections count one failed owner registry decode per packet.
They exclude stale lifecycle input rejected before decoding, TLS/application
handshakes, framing/CBOR failures caught in the transport, pre-enqueue byte and
queue limits, admission-state catch-up, core admission refusals and refused
assignment replays. No packet bytes, peer identities or error strings are
retained. Publishing this aggregate does not refresh owner supervision or
scan membership; a rise cannot establish peer death or authorize removal.

Received activity is recorded once after successful owner-side membership
decoding and before core processing or assignment replay. SWIM and anti-entropy
counts are disjoint packet counts. Gossip counts items, including both envelope
and pull-reply collections; an identical item in both counts twice. Collection
lengths are read in constant time without walking or copying their contents.
Activity remains counted if the core subsequently ignores/rejects the input.
Transport failures, rejected decoding, handshakes, admission-state catch-up and
direct test/core inputs bypass this accounting. None of these counters proves
successful probe correlation, reconciliation completion or formation convergence.

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

The duration histogram uses inclusive upper bounds of `0.001`, `0.005`, `0.025`,
`0.1`, `0.5`, `1`, `5` seconds and `+Inf`, at microsecond resolution. These
bounds distinguish fast local handling from delays approaching the existing
five-second handler budget; they are not measured production SLOs. Completed
4xx/5xx handlers are included; cancelled futures are excluded. Seven finite
threshold comparisons and one exclusive-bucket atomic increment bound the
added completion work. Scraping cumulatively sums eight exclusive counters,
so buckets are nondecreasing and `+Inf` equals the histogram `_count` in every
scrape. Bucket reads and the duration sum are not transactional with each other
or with the separate completed counter. `_sum` reuses the same saturating
microsecond accumulator as `_seconds_total`; no second duration is measured.
All histogram counts saturate at `u64::MAX`. Approximate quantiles have only
the resolution provided by these fixed buckets and combine all client routes;
they do not describe per-route or peer-network latency.

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
The ordinary companion also requires nonzero received SWIM, anti-entropy and
gossip activity at every worker after handoff, and zero activity counters on
the fresh standalone restart before readmission. Exact membership assertions,
not these activity measurements, still establish convergence.
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
remains distinct from the runtime's existing catch-up fault tests. The companion
`http_catchup_failure_stays_live_unready_and_recovers` runs with both
`observability` and `formation-fault-test`: a held established issuer exchange
fails while the source remains live, unready and startup-latched, then normal
catch-up restores readiness after release. It does not exercise malformed
baseline bytes or partially valid pages. The additional
`http_malformed_catchup_page_stays_live_unready_and_recovers` fixture changes
one normally authorized page reply to an unsupported page schema, keeping its
authenticated envelope valid. The real receiver refuses it; HTTP probes stay
live/unready/startup-latched with unchanged adopted identity before ordinary
retry restores readiness. The
`http_partial_catchup_stays_live_unready_and_recovers` variant uses 40 validated
source blocklist entries to produce two pages and corrupts page 1 only. The
real sequential receiver reaches that request after accepting page 0; failure
still withholds readiness and introduction until a complete retry succeeds.
These are not separate-process or secure remote deployment tests. See the
[partial-transfer evidence](tasks/cluster-formation-conformance.md#partial-catch-up-through-http-probes--2026-09-07),
[malformed-page evidence](tasks/cluster-formation-conformance.md#malformed-catch-up-page-through-http-probes--2026-09-07)
and the
[HTTP failure evidence](tasks/cluster-formation-conformance.md#catch-up-exchange-failure-through-http-probes--2026-09-07).

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

The [phase coverage audit](tasks/cluster-formation-conformance.md#pre-adoption-http-probes-and-phase-coverage-audit--2026-09-07)
maps implemented formation phases to named HTTP/process tests. In particular,
pre-adoption joining retains the source identity and withholds readiness even
when the issuer has admitted it but its ACK was lost; readiness is restored
only after assignment recovery and complete catch-up. Workload/storage and
collector-specific rows below remain dependent on their owning implementations.

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

Two additional production-router tests cover HTTP/1.1 diagnostics response
backpressure over real Unix sockets. A finite pipeline of actual `/metrics`
requests fills transport buffers; an observing wrapper records a pending write
without manufacturing the stall. Response generation and written-byte counts
then stop advancing. With the normal sixteen-connection limit, fresh probes
and authenticated operator lock/unlock still complete. A one-slot fixture
instead keeps a fresh probe queued until the existing five-second write timeout
reclaims the stalled connection. It then serves that probe and fresh metrics
while the original client remains open and unread. These tests preserve the
real owner and unchanged identities; they do not establish TCP/proxy downstream,
HTTP/2 flow-control or whole-process shutdown-under-write-pressure behavior.
See the [scoped evidence](tasks/cluster-formation-conformance.md#diagnostics-response-backpressure--2026-09-07).

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

The [formation instrumentation inventory](tasks/cluster-formation-conformance.md#formation-instrumentation-coverage-inventory--2026-09-07)
maps current counters and histograms to the actual owner/adapter outcomes.
The [inbound capacity gauges](#inbound-connection-capacity-gauges) now cover
the accepting adapter's budgets; [registry gauges](#registered-session-capacity-gauges)
separately expose retained total/provisional session budgets. The
[catch-up counters](#admission-state-catch-up-outcomes) now distinguish receiver
outcomes from owner adoption/fencing. Inbound refusal
totals are historical counts, outbound slots measure attempts, and reliable
slots measure stream work; none is a total established-peer-connection count.
The finite formation-metrics inventory is covered at its recorded boundaries;
trace/log correlation, reviewed overhead and full operator handoff remain open.
This does not promise per-peer attribution or convergence/step latency metrics.

The first slice instruments local process and formation operations alongside
N-FORMATION, independently of workload execution. Later slices instrument
O-RUNTIME/O-STORAGE, N-CLUSTER/N-ARTIFACT and V-LIVE. Trace propagation across
peers lands only with its versioned wire contract. P-SCALE uses the resulting
measurements but validates scientific equivalence separately.

[ADR 0025](adr/0025-version-peer-trace-context-propagation.md) accepts the
profile-5-only change and IO-only context ownership, with coordinated restart
and no profile-4 fallback. Implementation is pending: profile 4 still rejects
the added field. Existing bounded local span delivery remains distinct from
the unimplemented incoming-context extraction and cross-peer propagation.

The [accepted operational-log refinement](adr/0017-worker-operational-observability.md#correlated-operational-logs--decision-refinement-2026-09-08)
selects structured stdout by default behind a bounded asynchronous output
adapter. The [Rust ecosystem review](tasks/implement-worker-observability.md#rust-logging-ecosystem-review--2026-09-08)
identifies candidate instrumentation/writer interfaces and the additional
formatting, byte-budget and shutdown work required. This is not implemented
logging or an accepted vendor sink; future destinations retain the same bounds.

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
