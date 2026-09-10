# Implement worker operational observability

Status: **in progress — local probes and health/owner/lane/client metrics verified; full contract and acceptance pending**

Decision: [ADR 0017](../adr/0017-worker-operational-observability.md)
Design: [Operational observability](../orishu-observability.md)
Roadmap package: **P-OBSERVABILITY**

## Outcome and current gap

Operators scrape real per-worker Prometheus metrics, distinguish startup,
liveness and readiness, and correlate bounded distributed traces through an
optional OTLP exporter. The [observability guide](../orishu-observability.md#implemented-local-surface)
reports a feature-gated loopback listener with three health gauges, thirteen owner
counters, ten inbound peer handshake counters, lane-slot gauges, seven aggregate client-service instruments and
selectable process probes. The current source-build catalogue
has [real promtool/Prometheus evidence](../testing-worker-prometheus.md).
This is partial delivery, not acceptance of slices 1–3.
The owner catalogue now distinguishes local admission insertion, core refusal,
validated retained-assignment replay and owner-side membership packet
binding/decoding rejection, plus received SWIM/anti-entropy packets and gossip
items. An aggregate completed-client-handler duration histogram has fixed
buckets. Additional per-route or end-to-end formation timing is not an implicit
M4 requirement; use the finite inventory below to identify required coverage.
Inbound TLS/application handshake outcomes and the two pre-handshake capacity
refusals now have bounded optional counters. Preserve their
[catalogue and exclusions](../orishu-observability.md#inbound-peer-handshake-counters):
they do not count accepted admissions, outbound transport or all pre-core
failures. The shared reliable-exchange pool now adds request/serve outcomes,
duration distributions, partial stream bytes and slot pressure; see its
[catalogue and exclusions](../orishu-observability.md#reliable-peer-exchange-metrics)
and [adapter/process/Prometheus evidence](cluster-formation-conformance.md#reliable-peer-exchange-metrics--2026-09-07).
The [datagram/pre-pool catalogue](../orishu-observability.md#datagram-and-pre-pool-traffic-counters)
additionally covers local submission outcomes/payload bytes, pre-validation
receives and per-connection stream-task refusal. These do not measure all
datagram loss or attribute work to a peer. The
[outbound dial catalogue](../orishu-observability.md#outbound-dial-and-tls-metrics)
now distinguishes whole-attempt and candidate TLS outcomes/durations and the
four-slot capacity gate. These do not measure inbound handshake timing or
end-to-end convergence latency. Use the finite inventory below for required
metric coverage; additional instruments need a named operator assertion. This
is not cross-peer tracing acceptance.
Preserve the [outbound evidence and remaining outcome mapping](cluster-formation-conformance.md#outbound-dial-and-tls-metrics--2026-09-07)
when selecting the next formation instrument.
The [membership deadline catalogue](../orishu-observability.md#membership-deadline-and-abandonment-counters)
adds consumed timer kinds, stale timer input and join/reconciliation abandonment
from actual owner/core outcomes. Leave/ACK cancellation and owner generation
refusal are not deadline expiry. These counters do not establish completed
probes/rounds or convergence timing; the finite inventory below defines the
required metric scope. Metrics alone do not close slice 3's tracing obligations.
See the [deadline/abandonment evidence](cluster-formation-conformance.md#membership-deadline-and-abandonment-metrics--2026-09-07)
for real owner scheduling, wire controls and test-fixture corrections.
Confirm existing worker configuration, listener and logging facilities before
edits; reuse their ownership without treating placeholder state as real health.

The worker now has an owner-only supervision foundation: an actual one-second
owner tick, five-second stale threshold, bounded `OwnerHealth` projection and
phase/stalled-reader tests. See the [implemented input](../orishu-observability.md#implemented-owner-supervision-input).
Reuse it. Process health now adds initialization latching, required-client-task
lifetime guards and immediate shutdown readiness withdrawal. The
[foundation audit](cluster-formation-conformance.md#m4-foundation-evidence-reconciliation--2026-09-07)
records refreshed formation-stage HTTP health, four-feature configuration,
local collector and secured-proxy evidence. The
[downstream proxy increment](cluster-formation-conformance.md#monitoring-proxy-downstream-backpressure-and-expiry--2026-09-07)
now closes the last named foundation evidence gap with actual TLS write pressure,
bounded generation, response-send expiry and request-slot recovery. This scoped
foundation result does not close the separate instrumentation, correlation,
overhead or deployment/operator gates. The
[pre-HTTP proxy increment](cluster-formation-conformance.md#monitoring-proxy-pre-http-pressure-and-expiry--2026-09-07)
now covers mixed incomplete TLS/header clients, header trickle, expiry and
continued authenticated access with spare connection capacity. The
[direct method/error contract](cluster-formation-conformance.md#direct-diagnostics-method-and-error-contract--2026-09-07)
now has explicit bounded dispatch and real TCP process coverage, including
route groups and client metric/trace isolation.
[Diagnostics response backpressure](cluster-formation-conformance.md#diagnostics-response-backpressure--2026-09-07)
now has real HTTP/Unix expiry, control-progress and slot-reuse evidence; preserve
its scoped transport boundary. Cross-peer/log correlation, reviewed overhead and operator
deployment handoff remain separate M4 gates. Release-artifact Prometheus
integration stays with its release milestone. Extend required-role gates when new
production roles land. The slices below retain the complete desired scope;
map existing evidence before treating each bullet as missing implementation.
The production HTTP router now has real admission/pre-baseline/complete-catch-up
coverage in `http_readiness_waits_for_real_admission_catchup`; it defers starting
maintenance, not the health authority. The development-only companion
`http_catchup_failure_stays_live_unready_and_recovers` now holds an established
issuer exchange until it fails, checks live/unready/startup-latched HTTP
responses with retained adopted identity, and then verifies ready recovery.
The companion `http_malformed_catchup_page_stays_live_unready_and_recovers`
corrupts one real page's schema after source authorization, preserves the
authenticated reply envelope and checks the same failure/recovery probe and
identity assertions. The additional
`http_partial_catchup_stays_live_unready_and_recovers` fixture seeds a two-page
source baseline through validated startup domain commands and corrupts page 1
only after the receiver accepts page 0. It verifies catch-up failure,
live/unready/startup-latched probes and unchanged adopted identity, then ready
recovery through ordinary maintenance. These are scoped runtime/wire/HTTP
fixtures, not a complete role/pressure matrix or remote deployment acceptance.
Pre-adoption `Joining` additionally has HTTP coverage through real lost-ACK
admission and exact-assignment recovery in
`http_pre_adoption_join_stays_live_unready_with_original_identity`. Use the
[phase coverage audit](cluster-formation-conformance.md#pre-adoption-http-probes-and-phase-coverage-audit--2026-09-07)
to preserve these distinct cases rather than treating all probe transitions
as missing work.

## Bounded implementation slices

### Accepted logging-output decision

On 2026-09-08 the operator selected Option A, refining the default destination
from stderr to **stdout**, with a replaceable worker-owned output adapter.
The [ADR 0017 refinement](../adr/0017-worker-operational-observability.md#correlated-operational-logs--decision-refinement-2026-09-08)
records the accepted contract. The
[bounded adapter primitives](../orishu-observability.md#bounded-operational-log-adapter)
are implemented with fixed JSON encoding and queue/output/loss limits. Runtime
configuration, lifecycle and sampled-span wiring, nine live loss counters,
ordinary runtime-print reconciliation and local received-span/worker-pressure
tests now have [scoped evidence](cluster-formation-conformance.md#runtime-logging-and-local-span-receipt--2026-09-09).
The [real-worker slow-reader recovery](cluster-formation-conformance.md#real-worker-stdout-reader-recovery--2026-09-09)
fixture also proves resumed output without restarting the worker or clearing
loss counts. The [official Collector formation walkthrough](../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
now matches both admission chains to the respective workers' stdout records,
alongside direct metrics/probes and cross-worker policy checks. Preserve the
remaining selected-deployment and reviewed-overhead gates;
this is not waiting for another sink choice. ADR 0025 is accepted independently.

The [logging-counter backend evidence](cluster-formation-conformance.md#logging-counter-prometheus-ingestion--2026-09-09)
also covers actual Prometheus ingestion with logging alone and alongside
tracing, real closed-stdout failure counters, and fresh samples after scraper
outage. It preserves the disabled-log catalogue and is not a new remote
deployment or overhead qualification.

The selected [systemd/rootless-container collection extensions](cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09)
now add runtime-collected receipt evidence across two explicit starts per
deployment, with bounded shutdown and fresh identities/retained credentials.
The ordinary-process admission chains remain the cross-worker proof; this
does not imply supervisor-specific three-worker or performance qualification.

For the PoC, implement only structured stdout delivery and the adapter seam;
future Unix-datagram/vendor destinations must not require changing event
producers. Prefer a suitable ecosystem interface to a bespoke public framework;
do not add a new crate, sink registry or vendor dependency for hypothetical use.
Specify record encoding, exact field/record/queue-byte bounds, runtime defaults,
sampling/zero-sampling behavior and finite flush/shutdown settings before wiring
the adapter. The stdout destination default is not a decision to turn on every
dependency's logs or enable OTLP export.

Use fixed-name records with reviewed trace/span IDs, bounded fields and buffers,
secret-free diagnostics and distinct queue/encoding/output/shutdown loss counts.
Domain handlers never wait on sink capacity or writes. Exercise real stdout
pipe pressure, closed output, recovery where possible, and a whole-process
shutdown deadline; hold unread output through exit. Formatting/error/drop paths
must not fall back to blocking stdout/stderr. Reconcile existing plain-text
startup/shutdown prints so they cannot corrupt the selected structured stream
or bypass its runtime shutdown policy; document startup-error behavior separately.
Correlate an actual received local span with a parsed record. Keep retention,
durable audit and cloud/Kubernetes qualification outside this delivery.

### Rust logging ecosystem review — 2026-09-08

This is a source/documentation assessment, not a new dependency or performance
result. The lockfile already includes `tracing 0.1.44` transitively, but the worker
declares neither it nor a structured subscriber/appender as direct dependencies.
Existing local OTLP IDs belong to the worker's exporter; a subscriber's internal
span ID must not be substituted for that wire identity.

| Candidate | Fit and limitation |
| --- | --- |
| `tracing` plus `tracing-subscriber` | Preferred starting point for structured events and composable layers. [`MakeWriter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/trait.MakeWriter.html) supplies a replaceable writer interface; the [fmt module](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/index.html) supports newline-delimited JSON. A writer interface alone does not bound formatting or IO. |
| `tracing-appender 0.2.4` | Off-thread bounded-count queue, lossy mode and drop counter. Its [source](https://docs.rs/tracing-appender/0.2.4/src/tracing_appender/non_blocking.rs.html) copies each write into a vector before queue admission, offers blocking mode, and prints to stdout when shutdown enqueue times out. Fixed guard timeouts are not our configurable whole-process contract. Do not adopt it unchanged as proof of byte-bounded, nonblocking shutdown. |
| `slog` / `slog-async` | An alternative structured drain ecosystem; [`AsyncBuilder`](https://docs.rs/slog-async/latest/slog_async/struct.AsyncBuilder.html) exposes channel size and overflow policy. It still requires a separate byte/shutdown audit and explicit correlation with the existing exporter. No demonstrated benefit here justifies choosing a second instrumentation ecosystem. |
| `log` / `env_logger` | Simpler logging; [`Builder`](https://docs.rs/env_logger/latest/env_logger/struct.Builder.html) supports stdout, stderr and custom pipes. That destination support alone does not supply the required span correlation, asynchronous bounded delivery and shutdown contract. |

The inspected subscriber documentation reports version 0.3.23. Its
[fmt-layer source](https://docs.rs/tracing-subscriber/latest/src/tracing_subscriber/fmt/fmt_layer.rs.html)
formats into growable strings before invoking the writer and has direct stderr
error paths. Consequently, a capped `MakeWriter` alone is insufficient. Bound
and allowlist events before formatting; audit span-field storage and all error
paths. A small custom layer with bounded record encoding may be necessary even
when ecosystem event interfaces are reused. This is an implementation finding,
not permission to export arbitrary dependency fields or replace the OTLP stack.

Before dependency adoption, pin the selected versions/features, inspect resolved
dependencies/licenses, and verify the real adapter's byte caps, queue-full loss,
failed writes, redaction, correlation and blocked-output shutdown. Measure its
cost in the final formation-overhead experiment. Prefer reusing suitable parts
over assuming that a crate named “non-blocking” satisfies every boundary.

The [mTLS proxy recipe](../testing-worker-monitoring-proxy.md) now has
source-build evidence for ADR 0017's secured-proxy alternative, including real
Prometheus ingestion and credential/route isolation. Worker-native non-loopback
binding remains refused. Proxy active-request saturation, upstream timeout and
capacity recovery have passing evidence, as does the bounded mixed pre-HTTP
expiry case. Downstream stalled-reader expiry and request-slot reuse now have
separate real-TLS evidence; neither fixture claims exhaustion of Nginx's entire
connection pool or arbitrary slow-client guarantees. Full deployment/security acceptance,
cross-peer/log correlation and overhead remain separate requirements; the recipe
does not close slices 1–3.

### 1. Freeze the operational contract

- Reuse the implemented [tracing settings](../orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable)
  and precedence tests. Local client-service export is now wired to startup,
  the bounded queue/codec/HTTP loop and shutdown. Disabled tracing does not load
  files or create an exporter; omitted capability or invalid setup fails.
  Preserve the actual-worker local receiver/outage test and the file-backed
  mTLS fixture, then extend process-level TLS, security and feature evidence.
  Bounded default loading of the Linux ca-certificates bundle is implemented;
  retain its ambient-override refusal and document unsupported native layouts.
- Finalize the dedicated HTTP listener configuration and default loopback port,
  `observability` and `otlp-tracing` feature declarations, official/minimal
  build matrix, route switches, exact file/env/CLI mapping and validation.
- Publish the health transition matrix and bounded reason enum, required-role
  policy, supervision signal and deadlines. Supervise the actual control loop;
  a separate exporter thread ticking must not mask a stalled membership driver.
  Neither health calculation nor scraping may enumerate unbounded domain state.
- Define HTTP methods/status/content types, metric schema compatibility,
  authentication, monitoring-only credential lifecycle and remote probe-only
  exemption. Specify exact size/concurrency/time bounds and startup bind errors.
- Select and justify pinned optional Rust dependencies; document their feature
  and resolved dependency graphs. Specify metric catalogue, label budgets,
  histogram buckets, sampling, OTLP transport/TLS, exporter limits and overhead
  budgets before implementation. Publish contracts in the focused design,
  configuration and protocol documents; keep ADRs free of wire tables.
- Coordinate N-FORMATION health states and instrumentation hooks; no dependency
  on O-RUNTIME or full client resource compaction blocks this slice.

### 2. Process probes and Prometheus surface

- Implement typed startup settings, optional dedicated listener, bounded
  registry, local health projection and HTTP handlers per ADR 0017. Disabled
  builds exclude optional dependencies; disabled runtime starts no listener.
- Expose measured process/control-loop, request count/duration/error,
  connections, queue occupancy/capacity and telemetry-loss instruments where
  the production subsystem exists. Omit unsupported instruments or explicitly
  mark availability; do not publish placeholder zeros as measurements.
- Keep scrape request accounting from recursively instrumenting exposition.
  Use fixed route/outcome labels; pre-register finite series and bound dynamic
  info-series retirement. Do not allocate per peer/cell/step just to label it.
- Test real HTTP handlers, shutdown readiness transition and supervision
  stalling. Land corresponding operator manual and probe examples from
  P-OBS-DOCS with this slice.

### 3. Formation instrumentation and trace export

Use the [formation metric coverage inventory](cluster-formation-conformance.md#formation-instrumentation-coverage-inventory--2026-09-07)
as the finite metrics acceptance inventory. Existing admission, received activity,
deadline, dial and exchange instruments have scoped evidence; broad historical
“remaining instrumentation” statements do not require reimplementing them.
The [catch-up increment](cluster-formation-conformance.md#admission-state-catch-up-outcome-metrics--2026-09-08)
now covers receiver outcomes and the owner's adoption/fencing decision, closing
the last named metric row at its tested boundaries. This does not accept
trace/log correlation, overhead or the full operator handoff. The
[registry increment](cluster-formation-conformance.md#registered-session-capacity-gauges--2026-09-08)
now supplies retained total/provisional occupancy and capacity, including
outgoing introducer bindings, with owner lifecycle and real HTTP evidence. The
[inbound capacity increment](cluster-formation-conformance.md#inbound-connection-capacity-gauges--2026-09-08)
adds actual TLS/connection-task pressure readings and their operator exposition.
It does not count established sessions or accepted membership. The
separate trace, logging, overhead and operator handoff requirements below remain.

- Reuse `orishu_worker::trace_export` for bounded protobuf encoding behind the
  staged `otlp-tracing` feature. Its fixed-size records, exact envelope sizing,
  count/byte prefix splitting and fixed output slice have codec tests. Connect
  actual sampled worker operations and typed startup limits to its `SpanQueue`:
  local root sampling, active permits, bounded completed records, cancellation
  and overload shedding now have focused/concurrent tests. Its `HttpDelivery`
  primitive has real HTTP/HTTPS fixtures for bounded response consumption,
  supplied trust/client credentials and collector outcomes. Local worker startup
  and client-service instrumentation now use these adapters; extend process-level
  security and lifecycle coverage. Codec compilation alone is not acceptance.
  Reuse the tested `ExportLoop` queue consumer for count/timer/byte batching,
  single-flight delivery, partial-result accounting and whole-deadline shutdown.
  Its primitive fixtures supplement the actual-worker local export test;
  live queue/delivery counters now extend the optional metrics surface without
  waiting for export. Preserve the combined-feature live scrape, bounded
  catalogue and sampling/absence tests. Broader operation instrumentation,
  log correlation and the full companion acceptance remain required.
- Preserve implemented [ADR 0025](../adr/0025-version-peer-trace-context-propagation.md)
  and its [activation evidence](cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09).
  Profile 5 is now the sole peer ALPN; older peers must be rebuilt/restarted.
  The [context/child-span primitives](cluster-formation-conformance.md#trace-context-and-parent-aware-span-primitives--2026-09-08)
  are implemented and tested independently of activation.
  [Authenticated client extraction](cluster-formation-conformance.md#authenticated-client-parent-receipt--2026-09-08)
  now has actual worker/collector receipt evidence. The
  [staged profile-5 codec](cluster-formation-conformance.md#staged-profile-5-wire-codec--2026-09-09)
  now has serialized envelope and packet-omission fixtures without activating
  new wire behavior. [Owned join ancestry](cluster-formation-conformance.md#owned-join-exchange-parent-receipt--2026-09-09)
  now carries client context through the generation-fenced join preparation to
  locally exported outbound exchange spans, with replay/no-new-peer-work evidence.
  The later activation adds receiving admission spans and actual cross-worker
  receipt across three processes, including independent runtime controls and a
  feature-omitted peer. Log correlation, final overhead qualification and the
  combined operator monitoring walkthrough remain required.
- Instrument production N-FORMATION admission outcomes, SWIM/gossip/anti-entropy
  activity, timeouts, decode/auth failures, transport volume/latency and bounded
  queues in worker adapters. Use aggregate counters/histograms, not peer labels.
- Add sampled operation spans and bounded OTLP batching with configurable
  endpoint, trust, credentials, timeout and shutdown flush. Correlate structured
  logs with trace/span IDs without changing log retention or audit guarantees.
  The [OTLP Rust 0.32.0 HTTP exporter source](https://github.com/open-telemetry/opentelemetry-rust/blob/opentelemetry-otlp-0.32.0/opentelemetry-otlp/src/exporter/http/mod.rs)
  merges ambient OTEL header variables during builder setup and encodes the
  complete protobuf body before invoking its HTTP client. A custom HTTP client
  alone therefore does not prove configuration isolation or the required
  pre-allocation request-byte bound. Resolve both at the exporter/encoding
  boundary, with hostile-environment and exact-size tests, before activation.
  Do not clear process-global environment variables to configure one exporter.
- Specify validated client trace context and authenticated peer propagation in
  their protocol documents. Bound context bytes and attributes; reject/drop
  invalid context independently of the domain request where safe. Do not let
  caller sampling flags override local cost limits. Use span links where
  asynchronous gossip/batches have several causes rather than inventing one
  globally ordered trace. Keep raw secrets out of core replay and spans.
- In the real three-worker formation harness, scrape each process and collect
  a correlated client-to-peer operation through a test OTLP receiver. Cross-peer
  correlation cannot be declared complete using only local spans or mock IO.
- Close formation-stage overhead acceptance with slices 1–3, not with the
  later workload instruments in slice 4. Review the scenario, sampling/scrape
  settings, finite measurement budget and acceptance limits before the
  acceptance run. Compare omitted-feature, runtime-disabled and enabled modes
  under representative concurrent control/peer traffic with a three-worker
  baseline and 30-worker coverage; record
  latency, throughput, CPU, exporter memory, scrape duration and telemetry loss,
  together with unchanged domain outcomes. State collector transport and host
  conditions, preserve variation/failures and report unmet limits explicitly.
  The [manual local overhead harness](../testing-worker-overhead.md) supplies a
  reusable single-worker runtime-mode baseline, not omitted-feature,
  three-worker, TLS-collector or exporter-specific memory acceptance. No
  measured overhead budget has yet been accepted. This M4 work needs no solver
  or scientific workload and makes no fleet-scaling claim.
  The [expanded experiment](../measurements/formation-telemetry-plan.md)
  records the accepted normal p95/CPU overhead goal below 10%, temporary
  tolerance up to 20% and troubleshooting classification above 25%.
  Its full 3/10/30-worker curve and 90-minute total execution budget (excluding
  builds; 30 minutes per size) are accepted. Implement and validate its harness,
  then freeze executable/configuration identities before measuring.
  Accepted targets are not measured results;
  logging output and ADR 0025 have their own accepted decisions and evidence.

### 4. Instrument runtime, storage and observation owners

- With O-RUNTIME/O-WASM: admission, attempted/committed/failed steps, step and
  guest duration, cancellations, traps and limits. Distinguish wall-clock
  duration from simulation time; instrumentation never advances the latter.
- With N-CLUSTER: halo wait, barrier/commit latency, ownership transfer and
  recovery outcomes. With O-STORAGE/N-ARTIFACT: verified transfer bytes,
  integrity failures, cache/pinning, completeness, availability, repair and
  replication deficits. Preserve those distinct storage meanings.
- With V-LIVE/V-REPLAY: bounded queues, active observers, snapshot fallback,
  coalescing, replay gaps and delivery latency. Observer identities are not
  metric labels. Use sampled aggregate spans, not per-sample tracing.
- Extend P-SCALE harnesses with reproducible enabled/disabled/sampled overhead,
  series counts, exporter memory, scrape duration and dropped telemetry. Record
  declared numerical equivalence and subsystem cost separately.
  Extend the formation-stage measurements from slice 3 as each workload owner
  lands; later scientific measurements do not defer M4's control-plane budget.

## Acceptance criteria and validation

- Build/test no optional features, each feature alone and both; prove the
  membership resolved dependency purity test still passes.
- Test file/env/CLI precedence, malformed/contradictory config, omitted build
  capabilities, occupied ports, TLS/auth failure, loopback defaults and no
  outbound exporter activity when disabled.
- Parse actual `/metrics` responses with a Prometheus-compatible parser and
  scrape a packaged worker with Prometheus. Verify content type, stable types,
  counters/histograms and bounded cardinality through repeated formation/run
  changes and hostile labels/paths/errors.
- Test startup, idle/stopped/non-compute, join/recovery, control-loop stall,
  graceful shutdown, local dependency failure and peer/collector failure
  against the published probe matrix through real HTTP. Probe-only credentials
  or exemptions cannot read protected metrics or mutate any client route.
- Test oversized requests, slow/concurrent scrapers, authentication attempts,
  invalid trace headers, remote sampling requests, exporter queue saturation,
  unavailable collectors and bounded shutdown. Domain decisions and scientific
  results remain equivalent with telemetry enabled, disabled or dropped.
- Verify trace/log exports contain no seeded credentials, authored payloads or
  guest diagnostics; check that correlated wire spans reach the OTLP receiver.
- Run focused worker/configuration/transport tests while iterating, then
  `cargo fmt --all -- --check`, workspace Clippy with warnings denied, workspace
  all-target and doc tests using `--locked`, and `make docs-check`. Exercise the
  optional-feature matrix separately from default workspace checks.
- P-OBS-DOCS examples pass against the same feature build and metric catalogue.
  Report overhead measurements and any unmet budget before acceptance.

## Dependencies and non-goals

Slices 1–2 can start against the worker startup shell. Slice 3 integrates with
N-FORMATION; slice 4 follows the named runtime/storage/observation owners.
This does not block the independent membership merge correction.

No central monitoring service is added to cluster authority. No bundled
Prometheus/collector service, general profiler, fleet scheduler, durable audit
service, guest-controlled metric registration, scientific result stream or
new telemetry dependency in the functional core is in scope. New cross-cutting
contracts discovered during implementation require explicit documentation.
