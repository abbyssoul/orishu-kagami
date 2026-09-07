# Formation dashboard example

Scope: **optional source-built Linux snapshot console for Prometheus 3.5.0**.
The [versioned template](../etc/prometheus-consoles/orishu-worker.html) displays
one selected scrape target, with links to Prometheus's graph view. It consumes
the existing worker catalogue; it opens no worker port, enables no telemetry
feature and changes no formation authority.

This uses Prometheus's [console-template facility](https://prometheus.io/docs/visualization/consoles/)
with one self-contained HTML template. Prometheus 3 no longer bundles the old
console libraries; none is copied or required here. This is an optional PoC
example, not a requirement to adopt a console platform or a qualified Grafana,
service/container or cross-host monitoring deployment.

## Run the example

First follow the [local scrape recipe](testing-worker-prometheus.md) to start
a source-built worker with `observability` compiled in and explicitly enabled.
The default enabled diagnostics address is `127.0.0.1:9168`. Tracing remains
independent; the dashboard works without it and marks trace panels unavailable.
Use the pinned Prometheus 3.5.0 tools from that recipe for reproducibility,
not as a production-version recommendation.

From the same host network, launch Prometheus with explicit console paths:

```sh
project_root=/absolute/path/to/orishu-kagami
prometheus --config.file="$project_root/etc/prometheus-local.yml" \
  --web.listen-address=127.0.0.1:9090 \
  --web.console.templates="$project_root/etc/prometheus-consoles" \
  --web.console.libraries="$project_root/etc/prometheus-consoles" \
  --query.timeout=2s --storage.tsdb.path=/absolute/private/prometheus-data
```

The console directory contains no `.lib` files; the template defines its own
namespaced row helper. Use a private Prometheus data directory, not worker
credential or membership state. Prometheus access is a separate security
boundary: keep it on loopback for this example. Securing remote worker scrapes
through the monitoring proxy does not secure Prometheus's UI or query API.
Serve only the dedicated console directory, never the workspace or a directory
containing credentials. The example's two-second query timeout bounds each
query, not every request to a shared monitoring deployment.

Open `http://127.0.0.1:9090/consoles/orishu-worker.html`, select the exact target,
then use **Load snapshot** to refresh. Job and instance selections can be
bookmarked in the URL. The page never automatically issues operator commands,
loads external scripts or refreshes in the background. Instrument links open
the corresponding expression in Prometheus's graph view for history.

The default job is `orishu-worker`. Another job, such as the secured scrape
example's `orishu-worker-mtls`, must be selected explicitly. A job name is
limited to 128 ASCII letters/digits, `_`, `.`, `:`, or `-`. An exact instance
label is limited to 256 of those characters plus `[` and `]` for IPv6-style
labels. It is a label match, not an endpoint dial or proof of worker identity.
Selectors, URLs and duplicate job/instance parameters are refused. Invalid
values are not reflected back into the form or used in queries.

Discovery shows up to 32 current `up` series for the selected job. More targets
can be selected by entering their exact instance; this small console does not
provide fleet pagination. Missing discovery is not proof that the intended
worker never existed—compare deployment inventory separately.

## Read the panels

| Group | Meaning and exclusions |
| --- | --- |
| Health | Scrape availability, owner responsiveness, local readiness and startup latch. Neither readiness nor a successful scrape proves membership convergence. |
| Admission and reconciliation | Local insertion/refusal/replay, catch-up receiver failures versus owner adoption, and exhausted join/reconciliation budgets. Consult the original operation; pages, replies and successful IO are not admissions. |
| IO and client service | Named peer timeouts, partial reliable stream byte rates, completed client handlers and a five-minute p95 handler-duration estimate. Handler duration excludes response delivery; nested transport counters can overlap. |
| Capacity | Ten fixed occupancy/positive-capacity percentages. Occupancy includes reservations and retained registry entries; it is not simply queue length or live-socket count. Unconfigured/withdrawn capacity is unavailable, not 0% utilized. |
| Trace delivery | Separate sampling, delivery and local-loss rates. No multi-counter `rate` selector combines colliding label sets. Collector acceptance is not durable storage, and current local traces do not yet establish cross-peer/log correlation. |

All 37 panels use documented metric names. Counter rates are per second over
five minutes and handle individual counter resets; they are not process-lifetime
totals. A recent window can straddle a restart, so record process lifecycle
when comparing it. Rate panels also require the corresponding current series;
old range samples cannot make a now-absent instrument appear available.

One successful, unambiguous current `up` series is required before any worker
values are displayed. Failed, missing or duplicate matching targets withhold
those values instead of presenting retained data as current health. The scrape
panel itself still distinguishes measured zero, absence and ambiguity. Within
an available target, absent/insufficient samples, non-finite estimates and
multiple matching series have explicit unavailable states. In particular,
an idle histogram without observations is not a measured zero latency.

These are separately evaluated queries, not an atomic snapshot or a scientific
observation. A state change between queries can produce a transient disagreement.
The page uses no workload, peer or request IDs as labels and never guesses
an incident cause from a color or a member count.

Follow the [monitoring incident runbook](worker-monitoring-runbook.md) and,
for interrupted joins, the [admission recovery runbook](cluster-admission-recovery.md).
The page includes a short read-only reminder and names those versioned files
in the matching checkout; it does not assume the Markdown is served by
Prometheus. The [optional alert examples](testing-worker-prometheus.md#optional-formation-warnings)
have separate pending/threshold semantics. No dashboard value authorizes a
restart, leave, fresh admission or exclusion removal.

## Verify and inspect

The integration harness starts two real workers, with tracing off/on, plus
an inaccessible scrape target and a deliberately ambiguous target-label job.
It renders the actual template through Prometheus and validates its extracted
queries with promtool against eight synthetic scenarios: known values, idle,
reset, unready, zero capacity, absent/stale instruments and another target.
No dashboard PromQL is copied into the fixture generator.

```sh
make test-worker-dashboard PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

This builds both optional worker capabilities in `target/debug`; keep other
feature-changing builds sequential. For already-built worker/CLI paths, invoke
[the harness](../scripts/check-worker-dashboard.py) directly:

```sh
python3 scripts/check-worker-dashboard.py \
  --worker /absolute/path/to/orishu-worker --ctl /absolute/path/to/orishuctl \
  --promtool /absolute/path/to/promtool --prometheus /absolute/path/to/prometheus
```

The checks include disabled trace panels, zero-capacity availability, real
collector failure/recovery, unchanged worker identities, graph/navigation
links, wrong/oversized/duplicate selectors and bounded maximum-length rendering.
HTTP checks allow at most 256 KiB and one second per response; readiness/data
polling is capped at ten seconds, tool invocations at ten seconds and ordinary
process shutdown at three seconds before forced cleanup. These are test budgets,
not production SLOs. Each selected page has a fixed maximum of 40 template
queries; the discovery query examines the job's target set before limiting
display to 32. Query timeout does not establish a whole-page server deadline.
Do not expose this or the general Prometheus query API publicly without its
own reviewed access and resource policy.

For optional visual inspection, append `--browser /absolute/path/to/chromium`
and `--screenshot-dir /absolute/path/to/a-new-directory`. The harness uses a
temporary browser profile and captures desktop/mobile snapshots; existing
output directories are refused. Node.js must be on `PATH` for the optional
capture helper; no npm packages are required. The verified versions are Node.js
25.8.2 and Chrome headless shell 132.0.6834.110. The helper uses a private
[DevTools protocol](https://chromedevtools.github.io/devtools-protocol/)
pipe, not a remote debugging listener. It checks the 37-panel DOM, viewport and
health-anchor position before capturing desktop, mobile and mobile-health PNGs.
The complete helper has a ten-second deadline, an 8 MiB protocol-message buffer
limit and a 4 MiB limit per screenshot. It runs without the browser sandbox only
for this isolated, script-free loopback fixture; that is not a browser deployment
recommendation. Capture success is not visual acceptance: inspect all three
images for wrapping, readable values and navigation. The
[recorded acceptance](tasks/cluster-formation-conformance.md#formation-snapshot-dashboard--2026-09-08)
includes that inspection and the earlier blank fragment-capture failure.

This validates a snapshot dashboard and its query semantics, not continuous
fleet monitoring, live fault-driven notifications, backend durability, overhead
budgets, arbitrary Prometheus versions, remote UI security or full M4 operator
acceptance. Those boundaries remain separately tracked in
[P-OBS-DOCS](tasks/document-worker-observability.md).
