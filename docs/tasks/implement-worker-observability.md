# Implement worker operational observability

Status: **in progress — local probes and health/owner/lane/client metrics verified; full contract and acceptance pending**

Decision: [ADR 0017](../adr/0017-worker-operational-observability.md)
Design: [Operational observability](../orishu-observability.md)
Roadmap package: **P-OBSERVABILITY**

## Outcome and current gap

Operators scrape real per-worker Prometheus metrics, distinguish startup,
liveness and readiness, and correlate bounded distributed traces through an
optional OTLP exporter. The [observability guide](../orishu-observability.md#implemented-local-surface)
reports a feature-gated loopback listener with three health gauges, nine owner
counters, lane-slot gauges, six aggregate client-service instruments and
selectable process probes. The current source-build catalogue
has [real promtool/Prometheus evidence](../testing-worker-prometheus.md).
This is partial delivery, not acceptance of slices 1–3.
The owner catalogue now distinguishes local admission insertion, core refusal
and validated retained-assignment replay. Broader formation activity/latency
and pre-core failure instruments remain in slice 3.
Confirm existing worker configuration, listener and logging facilities before
edits; reuse their ownership without treating placeholder state as real health.

The worker now has an owner-only supervision foundation: an actual one-second
owner tick, five-second stale threshold, bounded `OwnerHealth` projection and
phase/stalled-reader tests. See the [implemented input](../orishu-observability.md#implemented-owner-supervision-input).
Reuse it. Process health now adds initialization latching, required-client-task
lifetime guards and immediate shutdown readiness withdrawal. Remaining work
includes the full HTTP probe transition/pressure matrix, feature/configuration
conformance, secured remote exposure, broader process/formation metrics,
release-artifact Prometheus integration and trace export. Extend required-role gates when new
production roles land. The slices below retain the complete desired scope;
map existing evidence before treating each bullet as missing implementation.
The production HTTP router now has real admission/pre-baseline/complete-catch-up
coverage in `http_readiness_waits_for_real_admission_catchup`; it defers starting
maintenance, not the health authority. Partial or invalid transfer HTTP coverage
and the remaining role/pressure combinations are still separate acceptance work.

## Bounded implementation slices

### 1. Freeze the operational contract

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

- Instrument production N-FORMATION admission outcomes, SWIM/gossip/anti-entropy
  activity, timeouts, decode/auth failures, transport volume/latency and bounded
  queues in worker adapters. Use aggregate counters/histograms, not peer labels.
- Add sampled operation spans and bounded OTLP batching with configurable
  endpoint, trust, credentials, timeout and shutdown flush. Correlate structured
  logs with trace/span IDs without changing log retention or audit guarantees.
- Specify validated client trace context and authenticated peer propagation in
  their protocol documents. Bound context bytes and attributes; reject/drop
  invalid context independently of the domain request where safe. Do not let
  caller sampling flags override local cost limits. Use span links where
  asynchronous gossip/batches have several causes rather than inventing one
  globally ordered trace. Keep raw secrets out of core replay and spans.
- In the real three-worker formation harness, scrape each process and collect
  a correlated client-to-peer operation through a test OTLP receiver. Cross-peer
  correlation cannot be declared complete using only local spans or mock IO.

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
