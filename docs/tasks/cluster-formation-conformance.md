# Cluster-formation conformance ledger

Status: **N-FORMATION accepted for the tested source-built Linux scope; combined M4 incomplete**
Checkpoint: **2026-09-08**
Owner: [N-FORMATION](implement-cluster-formation-poc.md)

This maps the task's required failure rows to inspected tests and remaining
work. It does not replace the task's other acceptance criteria or the combined
M4 observability gate. Keep historical narratives in the integration
record; update this table when a specific requirement gains evidence.

## Stdout logging decision and ecosystem review — 2026-09-08

The operator accepted standard-stream Option A and explicitly chose stdout
instead of the initial stderr proposal. The
[ADR 0017 refinement](../adr/0017-worker-operational-observability.md#correlated-operational-logs--decision-refinement-2026-09-08)
records bounded asynchronous delivery and a replaceable worker-owned adapter.
Additional Unix-datagram/vendor sinks are future work. This settles the output
choice, not its implementation, sampling/settings contract or overhead budget.

The [ecosystem assessment](implement-worker-observability.md#rust-logging-ecosystem-review--2026-09-08)
compares official crate documentation/source with current worker dependencies.
It recommends evaluating tracing/subscriber interfaces while identifying
formatting-before-writer allocation and appender shutdown-output paths that
prevent assuming a drop-in byte/shutdown guarantee. No crate was added, no
runtime code changed and no output/pressure test was run. Earlier references
to a pending logging choice below retain their historical checkpoint only.
Current planning references now point to the accepted stdout contract.

Decision/research documentation is validated with `make docs-check` and scoped
whitespace checks. The remaining work is implementation and real correlation,
output-pressure, feature and shutdown evidence, followed by reviewed overhead
and the final operator walkthrough. M4 remains incomplete.

## Peer-profile decision accepted — 2026-09-08

The operator selected Option A in the decision Q&A: accept
[ADR 0025](../adr/0025-version-peer-trace-context-propagation.md), implement
profile 5 only, and require coordinated worker rebuild/restart without a
profile-4 fallback. There are no supported published releases requiring a
dual-profile compatibility path. Logging output and overhead budgets remain
separate unresolved choices. The earlier proposed-status statements below are
historical; this decision supersedes that gate, not their test evidence.

Only decision/planning documentation changed. Current workers still use profile
4; no propagation, binary compatibility, restart or collector test is claimed
by this acceptance. Implementation and the ADR's complete validation remain
required before activation. M4 is not complete.

## M4 operator recipe applicability — 2026-09-08

This is the finite recipe-to-criterion map requested by the
[open M4 work table](implement-cluster-formation-poc.md#open-m4-work-selection).
It reconciles the current recipes, their harness boundaries and recorded
results; it is not a new runtime test pass or final deployment acceptance.
It completes the operator-evidence inventory, not the remaining implementation.

| Recipe | M4 assertion and existing evidence/checkpoint | Missing check or action | Owner |
| --- | --- | --- | --- |
| [Local Prometheus](../testing-worker-prometheus.md) | Per-worker real exposition/ingestion, explicit route enablement and absent-versus-zero interpretation. [Foundation](#m4-foundation-evidence-reconciliation--2026-09-07), later [catch-up catalogue/three-worker evidence](#admission-state-catch-up-outcome-metrics--2026-09-08) | Preserve those separate parser, backend and public-formation boundaries. Assess catalogue/build applicability when final tracing/logging lands; direct HTTP scrapes do not substitute for Prometheus ingestion. | P-OBS-DOCS slices 1–2; P-OBSERVABILITY |
| [Secured monitoring proxy](../testing-worker-monitoring-proxy.md) | Monitoring-only mTLS, route separation, real ingestion, worker independence, upstream and downstream pressure. [Foundation](#m4-foundation-evidence-reconciliation--2026-09-07) and [downstream evidence](#monitoring-proxy-downstream-backpressure-and-expiry--2026-09-07) | Same-host security boundary is covered. Any required cross-host or supervisor/proxy co-deployment needs a named profile and its own missing assertion; it cannot be inferred from these runs. | P-OBS-DOCS slices 2–3 |
| [Local Collector](../testing-worker-otelcol.md) and [mTLS overlay](../testing-worker-otelcol-mtls.md) | Actual local span receipts, disabled/zero sampling, outage/recovery and seeded redaction; dedicated TLS trust/credential refusals. [Local recipe](#pinned-local-collector-operator-recipe--2026-09-07) and [mTLS receiver](#pinned-collector-mtls-receiver-recipe--2026-09-07) | Add the selected real client-to-peer causal receipt and matched operational log after their decisions and implementation. Existing local root spans are insufficient. Configured file retention is not tested durability or disk-pressure evidence. | P-OBS-DOCS slice 4; P-OBSERVABILITY slice 3 |
| [User service](../testing-worker-user-service.md) | Five feature/runtime modes, actual manager lifecycle, private authority, probe/metrics access and explicit fresh-identity restart. [Systemd evidence](#source-built-user-service-monitoring--2026-09-08) | Existing fixture explicitly disables trace export. Do not call its journal capture correlated-log or sink-pressure evidence; apply final output instructions to this deployment if selected for the final correlation journey. | P-OBS-DOCS slice 3 |
| [Rootless container](../testing-worker-container.md) | Five runtime modes, private state, exec probes, same/separate namespace scraping and explicit stop/restart. [Podman evidence](#source-built-rootless-container-monitoring--2026-09-08) | Existing fixture explicitly disables tracing. Final collector/log co-deployment, if selected, needs actual receipt across the declared namespace/mount boundary; a shared-network metrics scrape does not prove it. | P-OBS-DOCS slice 3 |
| [Formation dashboard](../testing-worker-dashboard.md) and [alert examples](../testing-worker-prometheus.md#optional-formation-warnings) | Actual catalogue/query behavior, unavailable/idle/failure controls and inspected browser layouts. [Dashboard](#formation-snapshot-dashboard--2026-09-08) and [alert evidence](#formation-alert-examples-and-same-worker-trace-rule-correction--2026-09-08) | Preserve delivered examples; rerun affected catalogue/query checks if final telemetry changes their inputs. Production thresholds and notification delivery are not established by the examples. | P-OBS-DOCS slice 5 |
| [Incident runbook](../worker-monitoring-runbook.md), worker/CLI manuals and both operator story sets | Read-only peer-loss, unreadiness and missing-telemetry triage with identity guards and safe stop conditions. [Runbook evidence](#formation-monitoring-incident-runbook--2026-09-08) | Extend the correlation walkthrough and final feature/compatibility instructions with their actual deliveries. Do not infer admission, restart or removal authority from telemetry. | P-OBS-DOCS slices 1, 4 and 6 |
| [Formation overhead proposal](../measurements/formation-telemetry-plan.md) | Defines the candidate concurrent three-worker comparison; [old single-worker measurements](#optimized-local-telemetry-overhead-baseline--2026-09-07) remain historical | Review profile/budgets, implement the proposed experiment and measure the final accepted trace/log configuration. No accepted budget or final three-worker result exists. | P-OBSERVABILITY slices 1–3 |

The source check for this mapping still finds
`orishu-membership/4` in `apps/orishu-worker/src/peer/tls.rs`, with the
profile-4 trace-field rejection fixture in `peer/wire.rs`. ADR 0025 was proposed
at this mapping checkpoint; the later decision above accepts it. The [logging-output gate](implement-worker-observability.md#accepted-logging-output-decision)
still explicitly requires a user choice. Continuing the implementation goal
does not accept either decision. Collector receipt, systemd journal capture
and container output must not be combined into invented cross-peer/log evidence.

### Remaining handoff and stop condition

Select the final source-built Linux deployment profile(s) before the final
validation batch, as already required by the task. For each selected profile,
name only the assertions absent from the map above. Published artifacts,
system-wide package lifecycle and other platform qualification retain
P-INSTALL/M8 ownership; scientific monitoring retains slice-4/runtime ownership.
Optional Kubernetes is not implicitly selected. This mapping neither waives a
required M4 assertion nor adds every untested combination as a new prerequisite.

The next dependent implementations are bounded correlated logging and peer
propagation after their respective decisions. Final overhead and the operator
correlation walkthrough follow those delivered contracts. Do not repeat an
unchanged foundation, service/container or dashboard audit as a substitute for
these missing outcomes. A newly discovered defect may justify its own focused
regression; none was demonstrated by this documentation mapping.

Validation for this mapping is `make docs-check` and scoped `git diff --check`.
No worker binary, peer profile, runtime setting, performance budget or recorded
test result was changed. N-FORMATION's recorded acceptance is preserved and
combined M4 remains incomplete.

## Source-built rootless container monitoring — 2026-09-08

The [container recipe](../testing-worker-container.md),
[evaluation Containerfile](../../etc/containers/Containerfile.worker-poc) and
[real Podman harness](../../scripts/check-worker-container.py) deliver the
local source-built container example in P-OBS-DOCS slice 3. The root Dockerfile
and its release-default gap remain unchanged. No image is published and no
host port is exposed; this is not remote proxy/collector co-deployment,
Kubernetes, cross-peer/log correlation or combined M4 acceptance.

### Real container evidence

| Assertion | Passed boundary |
| --- | --- |
| Safe default command | Combined-capability image uses only private Unix client/state paths with peer admission, diagnostics and tracing disabled. Curl inside the worker's namespace gets connection refusal on the diagnostics port |
| Enabled and probes-only modes | All three real process probes return success; enabled metrics parse through promtool 3.5.0. Probes-only keeps health available while `/metrics` returns 404. The explicit Podman healthcheck command reports healthy without a scheduled timer or health-triggered restart |
| Omitted capability | The minimal image runs with diagnostics off. Requesting its omitted capability exits with status 2 before creating credential files or the client socket |
| Non-root/private state | Actual container UID matches the mapped non-root caller; rootfs is read-only, network mode is `none` and no ports are published. Credential files retain caller ownership and mode 0600, as does the private Unix socket's mode |
| Client authority | All four running modes refuse tokenless lock/unlock and accept the corresponding identified requests using the private operator token file; that token is absent from scraped metrics and captured container output |
| Network namespace boundary | A separate no-network container cannot scrape the worker's loopback (curl connection refusal). An explicitly network-sharing container can scrape and parse real metrics without receiving worker-state mounts. Neither case uses host networking or remote credentials |
| Lifecycle | Podman sends SIGTERM with a ten-second stop allowance. Each running mode exits zero within the twelve-second observation budget and removes its owned socket. Explicit start creates fresh formation/node IDs while retaining byte-identical certificate/operator credential files |
| Cleanup | Only UUID-named containers and images bearing this run's matching fixture label are stopped/removed. No existing container, permanent service, host package, registry or boot policy is changed; temporary test state is removed afterward |

The initial full harness and the following Make-target run passed all five
modes and both namespace controls. The final Make-target rerun adds token
exclusion from both stdout and stderr. There was no demonstrated runtime defect
or increased worker deadline. Container state/health observations establish
only these assertions, not SWIM convergence, a three-worker container formation,
probe transitions during catch-up or absence of all possible network traffic.
The broader probe and formation contracts retain their separately recorded
runtime/wire/process evidence.

### Build provenance and reproduction

Rootless Podman was `5.4.2`, using `crun` on the existing Linux host. Sandbox
access to its runtime directory was initially refused; image inspection,
construction and fixtures used approved access. The official Debian
trixie-slim base was explicitly downloaded, rather than silently replacing it
with a cached older runtime. The host-built worker requires glibc 2.38; cached
Debian 12 was not used as compatibility evidence.

- Base reference: `docker.io/library/debian@sha256:abc9cb88a5587630d7f915f47b23b0668fe250fbfc6457aa4d52b534c1bbf73f`.
- Base image ID: `e426a54f50cc4cf82dd5cab8ba8426ed02c391840cb5a62dfd987542dbabea3b`.
- Runtime packages reported by the running evaluation image: `ca-certificates=20250419`, `curl=8.14.1-2+deb13u4`, `libc6=2.41-12+deb13u3`, `libgcc-s1=14.2.0-19`.
- Combined worker SHA-256: `ac500b489f011683f1fb193a6915d04f2af580f5879378b5b43e15309ec11f68`.
- Minimal worker SHA-256: `fdc12b5124998c0554142776995660645dd8265b7fb24d6cd1fcc45079efb91c`.
- CLI SHA-256: `55d4462d568b404e7397bcd0c0c11a34188eeddfd66a179e3962c42e860faf5e`.

The dirty source base remains `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`.
Both source builds pass; the known `proc-macro-error2 v2.0.1`
future-compatibility warning remains. The actual evaluation image IDs are
printed by every run, with unique fixture labels; images are then removed.
For example, the first Make run used combined
`0708b46d2f8c862ece91a35bedf863d38f1c537521a156cc05eb395f782d6c6c`
and minimal `d9dd378b045fad051b48487dd6560a16f7ce0de02fb47b6d6481c1d70aed7e32`.
The base digest does not freeze apt package resolution or make subsequent
images byte-identical; record updated package versions and rerun acceptance.

```sh
podman pull docker.io/library/debian:trixie-slim
podman image inspect docker.io/library/debian:trixie-slim --format '{{.Id}} {{.RepoDigests}}'
python3 scripts/check-worker-container.py --base-image docker.io/library/debian@sha256:abc9cb88a5587630d7f915f47b23b0668fe250fbfc6457aa4d52b534c1bbf73f --worker target/formation-flow-observability/debug/orishu-worker --minimal-worker target/worker-service-minimal/debug/orishu-worker --ctl target/worker-service-minimal/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool
make test-worker-container WORKER_SERVICE_TARGET_DIR=target/formation-flow-observability PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool
make docs-check
```

The guide uses the resolved immutable digest for future pulls; the original
tag-based discovery command above is retained as actual history. Builds are
limited to 180 seconds each and ordinary tool calls/observations to fifteen
seconds. Curl uses a one-second request limit; the real CLI uses two seconds.
These are test/evaluation guards, not reviewed performance budgets. The image
contains operational CLI/curl tools, not a qualified minimal release runtime.

Downloaded base images and reusable build cache remain locally; no global
prune runs. Fixture images/containers and their temporary private state are
removed, and no image is pushed. Documentation and scoped whitespace checks
pass. No full workspace/fault matrix, actual Prometheus server in the namespace,
remote TLS proxy container, trace-export/logging, Kubernetes, Docker, ARM,
SELinux, publication or formation-overhead acceptance was run. Operator manuals,
stories, installation support wording and task/roadmap links now identify this
bounded delivery without closing those separate gates.

## Source-built user-service monitoring — 2026-09-08

The [user-service recipe](../testing-worker-user-service.md) and
[unit example](../../etc/systemd/orishu-worker-poc.service.example) now have
real systemd user-manager evidence. This closes the named source-built
user-service example in P-OBS-DOCS slice 3, not system-wide/container deployment,
published package lifecycle, remote-proxy co-deployment or combined M4.

The checked-in [harness](../../scripts/check-worker-user-service.py) substitutes
only paths and the selected diagnostic settings in the actual example, verifies
it with `systemd-analyze --user verify`, links it at runtime and controls it
through `systemctl --user`. No direct worker subprocess substitutes for the
service-manager boundary. The worker and CLI executables are copied into a
private fixture directory before any service starts, so rebuilds cannot change
the tested binaries underneath a live service.

| Required assertion | Passed evidence |
| --- | --- |
| Enabled service | Real active worker, three HTTP probes at 200, bounded metrics parsed by promtool 3.5.0, readiness gauge at one and no operator API route on the diagnostics listener |
| Runtime disabled | Combined-capability worker serves its real Unix client but does not listen on the selected diagnostics port |
| Probes only | All three probes succeed while `/metrics` returns 404 |
| Feature omitted | Minimal worker starts and serves client requests with diagnostics disabled; requesting the omitted capability instead yields unit `Result=exit-code`, status 2, no credentials/client socket and no restart |
| Local authority and permissions | All four running modes retain owner-only socket and credential files. Unauthenticated lock/unlock requests fail, authenticated identified requests succeed, and direct summaries retain the original formation while reflecting the lock state |
| Graceful service stop | SIGTERM/control-group policy, ten-second stop setting, 0077 umask and NoNewPrivileges are read back from the actual manager. Stop finishes within ten seconds with success/status zero/MainPID zero and owned socket removal; enabled modes retain an incomplete real diagnostics request until server-driven EOF |
| Explicit restart | Each running mode restarts only on an explicit start command, with fresh formation/node IDs and byte-identical persisted identity/operator credential files |
| Unexpected process loss | The enabled fixture alone receives SIGKILL through systemctl; its unit reports signal/status 9, MainPID zero, Restart=no and zero automatic restarts. Explicit subsequent startup reclaims its stale socket, retains credentials, creates fresh standalone identities and returns ready |
| Fixture cleanup | Every UUID-named runtime unit is stopped, has only its own failed status cleared, is unlinked, and reaches LoadState=not-found before temporary files are removed. The unrelated degraded user-manager state is not repaired or treated as a worker failure |

Three complete five-mode runs passed. The first covered service/configuration,
probes and clean stop/restart; the second added public authorization/control
checks and deliberate crash recovery; the final checked-in Make target reran
that complete set with both isolated feature builds. No production worker or
packaged unit was changed, and there was no demonstrated runtime defect in
this increment. Test unit journal records remain in the user's journal; the
bounded tail is checked for the actual operator token without printing it.
Journal capture does not settle the pending structured logging/output decision
or prove nonblocking log behavior under sink pressure.

Commands used:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --target-dir target/worker-service-minimal
python3 scripts/check-worker-user-service.py --worker target/formation-flow-observability/debug/orishu-worker --minimal-worker target/worker-service-minimal/debug/orishu-worker --ctl target/worker-service-minimal/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool
make test-worker-user-service WORKER_SERVICE_TARGET_DIR=target/formation-flow-observability PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool
make docs-check
```

Linux systemd version was `257.9-0ubuntu2.5` (reports systemd 257); the user
manager was available but globally degraded by unrelated state. Initial sandbox
bus access was denied; the scoped test ran with approved user-bus access.
The build checkpoint remains dirty base
`5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`. Executable SHA-256 values:

- Combined worker, `target/formation-flow-observability/debug/orishu-worker`:
  `ac500b489f011683f1fb193a6915d04f2af580f5879378b5b43e15309ec11f68`.
- Minimal worker, `target/worker-service-minimal/debug/orishu-worker`:
  `fdc12b5124998c0554142776995660645dd8265b7fb24d6cd1fcc45079efb91c`.
- CLI, `target/worker-service-minimal/debug/orishuctl`:
  `55d4462d568b404e7397bcd0c0c11a34188eeddfd66a179e3962c42e860faf5e`.

The build passes with the retained `proc-macro-error2 v2.0.1` future-compatibility
warning. Documentation and scoped whitespace checks pass. The service test
limits tool calls to fifteen seconds and observations to ten seconds, with
one-second diagnostics socket and two-second CLI request timeouts. These are
fixture/service-stop budgets, not measured formation or production SLOs.

No full workspace/fault matrix, three-worker systemd journey, all-four-Cargo-
capability service matrix, actual Prometheus server ingestion under systemd,
remote proxy service, collector/log pressure or formation overhead acceptance
was run here. Existing wire, scrape and process evidence remains scoped to its
own checkpoint. The worker manual, installation guide, operator story sets and
task/roadmap entries now link this recipe without promoting it to a packaged
installation or boot/logout guarantee. M4 and both companion tasks remain open.

## Formation snapshot dashboard — 2026-09-08

The optional [dashboard recipe](../testing-worker-dashboard.md) and
[self-contained console](../../etc/prometheus-consoles/orishu-worker.html)
deliver the formation-stage dashboard example in P-OBS-DOCS slice 5. Prometheus
serves it; no worker route, metric, dependency, wire profile, health policy or
formation authority changes. This records scoped dashboard acceptance, not
deployment qualification, cross-peer/log correlation or combined M4 closure.

### Evidence and boundaries

| Assertion | Result and production boundary |
| --- | --- |
| Real template rendering | Prometheus 3.5.0 renders all 37 panels against two ordinary workers, one with tracing disabled and one enabled; health values and optional trace availability match those configurations |
| Query semantics | Eight synthetic scenarios evaluate all 37 expressions extracted from the rendered HTML: 296 promtool checks for known values, idle, counter reset, unreadiness, zero capacity, absent instruments, stale instruments and another target |
| Instrument identity | Selected metric names are checked against the existing ingestion harness catalogue; live panels exercise the real worker exposition. This run does not repeat the separate full 151/163-series ingestion acceptance |
| Target availability | Unselected, failed, missing and deliberately ambiguous targets have distinct states; unavailable targets withhold worker values. No peer listener leaves inbound capacities unavailable, not 0% utilized |
| Selector safety and navigation | Oversized job/instance, script-like input and duplicate job/instance selections are refused. Maximum valid 128/256-character selections remain within the response cap. Graph/navigation links resolve with bounded same-server redirects; actual operator tokens are absent from rendered HTML |
| Collector failure/recovery | After baseline scrapes, fresh read-only client operations establish positive delivery-failure and then acceptance rates as the bounded HTTP responder recovers. Source node identities remain unchanged and the traced worker retains its formation ID. This responder checks framing/status, not decoded OTLP causality or backend durability |
| Browser layout | Node.js 25.8.2 and Chrome headless shell 132.0.6834.110 validate the 37-panel DOM, requested viewport widths and health-anchor position through a private CDP pipe. Desktop 1280×1800, mobile 390×1000 and scrolled mobile-health 390×1000 captures were visually inspected for readable controls, wrapping and table values |
| Build and documentation | The locked offline combined-capability worker/CLI build passes. `node --check scripts/capture-worker-dashboard.mjs`, `make docs-check` and scoped whitespace checks pass; the known `proc-macro-error2 v2.0.1` future-compatibility warning remains |

The selected-page query count is at most 40. Harness HTTP responses are capped
at 256 KiB with a one-second socket timeout; polling and tool invocations have
ten-second budgets. The browser helper's whole-process budget is ten seconds,
its CDP response buffer is bounded at 8 MiB and each screenshot at 4 MiB.
Ordinary child shutdown allows three seconds before forced cleanup. These are
fixture limits, not production SLOs or a Prometheus whole-page server deadline.
The browser's private profile and debugging pipe are test infrastructure, not
an exposed service. Screenshot creation alone is not visual acceptance.

### Capture failure retained

The first refreshed run passed the HTML/query/worker checks but produced a
blank `mobile-health.png` when Chromium's CLI screenshot used a fragment URL.
The PNG exceeded the old 1,000-byte threshold, so that assertion did not prove
visible content. Adding compositor and virtual-time flags still produced a
blank capture; those flags were not retained as a fix. This was a failed visual
check, not a demonstrated worker or Prometheus-query defect.

The optional capture now uses the checked-in
[browser helper](../../scripts/capture-worker-dashboard.mjs): wait for the
expected DOM, verify the viewport and scrolled anchor position, then capture
the explicitly positioned viewport. The corrected captures show the mobile
health table and following admission rows. Both corrected browser runs passed;
the later duplicate-job assertion also passed in the complete non-browser
journey. No fixture deadline or worker limit was increased. The prior session
handle was missing when resumed; its old screenshot files were not treated as
proof of a terminal successful run.

### Reproduction and applicability

Commands executed from the repository root:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
node --check scripts/capture-worker-dashboard.mjs
python3 scripts/check-worker-dashboard.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus --browser /home/soultaker/.cache/puppeteer/chrome-headless-shell/linux-132.0.6834.110/chrome-headless-shell-linux64/chrome-headless-shell --screenshot-dir /tmp/orishu-dashboard-verified-20260908
python3 scripts/check-worker-dashboard.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
make docs-check
```

The first corrected browser run used `/tmp/orishu-dashboard-cdp-20260908`;
the command above produced the separately inspected final captures. These are
local disposable artifacts, not published screenshots. Use new output paths
when reproducing; the harness refuses overwriting an existing directory.

Dirty source-built Linux base is
`5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`. The refreshed worker SHA-256 is
`ac500b489f011683f1fb193a6915d04f2af580f5879378b5b43e15309ec11f68`; CLI is
`fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda`, both in
`target/formation-flow-observability/debug`. Source assets and the fixture are
read from this checkout, not embedded in those binaries. Both worker features
are compiled in; the two runtime tracing modes are not separate feature builds.

No full workspace/fault matrix, three-worker formation rerun, overhead acceptance,
remote UI security, notification delivery or packaged release validation was
run for this dashboard increment. Preserve their separate evidence and open
gates. The worker manual, both operator story sets and task/roadmap entries
already link the dashboard; the formation task now links this missing evidence
mapping instead of scheduling another dashboard implementation.

## Formation alert examples and same-worker trace-rule correction — 2026-09-08

The optional [formation warning group](../../etc/prometheus-worker-formation-alerts.yml)
now covers existing owner responsiveness, admission/catch-up failures,
membership abandonment, peer IO timeout and slot-capacity instruments. It adds
six alert definitions, not new worker telemetry or monitoring exposure.
The [operator guide](../testing-worker-prometheus.md#optional-formation-warnings)
documents independent enablement, job-selector adaptation, example threshold
rationale, absent-versus-zero semantics and safe runbook responses.

Every warning requires a successful current scrape. Admission refusal is
informational because correct lock/policy enforcement can reject admission.
Counter conditions use independent five-minute rates and a boolean union of
positive symptoms, not a summed incident count. Lifecycle cancellation,
fencing, assignment replay and consumed SWIM timers are not recast as receiver
or transport failures. Capacity compares occupancy with the corresponding
positive capacity for ten fixed budget families. Zero/absent capacity does not
mean saturation, and retained slots include reservations or registry entries,
not only active messages or sockets.

### Regression found and fixed

The first formation-rule fixture run failed when several unlabelled counters
shared one target: `rate` discarded their metric names, producing duplicate
output label sets before the outer `sum` could aggregate them. The corrected
rules evaluate each distinct counter's rate and combine positive conditions
with `or`. All 39 scenarios then passed.

The pre-existing `OrishuWorkerTraceLocalDrops` example used the same invalid
multi-name pattern. A new checked-in regression places all four local-loss
counters on one worker, including one resetting while another increases.
Before the rule correction, promtool failed at one minute with
`vector cannot contain metrics with the same labelset`; afterward the complete
trace fixture file passed. Earlier individual-counter fixtures used different
targets and did not establish coexisting-series evaluation. This correction
supersedes that part of the earlier trace-alert evidence, not formation or
worker exporter behavior. Alert names/labels and pending policy are unchanged;
operators using the old trace rule should load the corrected file through
their normal Prometheus configuration workflow. No running service was changed
by this verification.

The real ingestion harness now waits for two worker samples and executes
every loaded rule expression through Prometheus's query API. Rule loading or
an unevaluated `health` field alone cannot close the evaluation assertion.
The first enhanced base journey reached its scraper-recovery observation
deadline because the new loop reused the catalogue query variable captured by
the ingestion check. Renaming that rule-local variable fixed the harness;
no polling, scrape or worker deadline was increased. All four complete
journeys subsequently passed.

### Evidence and reproduction

| Assertion | Result and boundary |
| --- | --- |
| Selected counters and budgets exist | The fixture generator checks every selected metric name against the catalogue that the real worker ingestion harness verifies |
| Positive symptoms, pending/firing/recovery | 39 generated finite scenarios pass through promtool 3.5.0, checking all six rules at one, four and ten minutes in each scenario. Every selected counter and all ten budgets are exercised separately; all ten capacities also coexist on one target |
| Exclusions and isolation | Fixtures cover idle, independent counter resets, a reset alongside another error, absent/stale gauges, failed scrapes, deleted discovery targets, zero/missing capacity, transient/below-capacity occupancy, unknown budget families, distinct jobs/targets and unrelated cancellation/fencing/timer outcomes |
| Existing trace-loss regression | Complete static trace rule fixtures pass, including four same-target counters with a reset and a new loss; the pre-fix failure is retained above |
| Default rules | Ordinary tracing-disabled worker: 151 ingested series, three loaded/live-evaluated expressions; scraper outage, read-only triage, authenticated controls and fresh re-ingestion pass |
| Trace rules | Tracing enabled: 163 series, six loaded/live-evaluated expressions; real collector failure/recovery plus scraper outage/control/recovery pass |
| Formation rules | Tracing disabled, optional formation rules enabled: 151 series and nine loaded/live-evaluated expressions; complete scraper journey passes |
| Both optional groups | Tracing and formation rules enabled: 163 series and twelve loaded/live-evaluated expressions; complete collector/scraper journeys pass |
| Documentation | `make docs-check` passes for 124 Markdown files; scoped whitespace checks pass. Worker manual, both operator story sets, incident runbook, owning task and task/roadmap status links are reconciled |

Commands used from the repository root:

```sh
python3 scripts/test_worker_formation_alerts.py --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool
/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool test rules etc/prometheus-worker-trace-alerts.test.yml
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

The final command passed without extra flags, with `--trace-metrics`, with
`--formation-alerts`, and with both. These four runs used the same ordinary
combined-capability worker, changing runtime tracing and rule selection;
they are not four Cargo feature builds. Processes and credentials were isolated
on loopback and cleaned up through the existing bounded harness. The new
`make test-worker-formation-prometheus` convenience target invokes the combined
case using `target/debug`; verification here used the explicit isolated paths
above instead. No downloads or external telemetry destinations were needed.

Dirty source-built Linux base remains
`5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`. The worker SHA-256 remains
`ac500b489f011683f1fb193a6915d04f2af580f5879378b5b43e15309ec11f68` and CLI remains
`fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` in
`target/formation-flow-observability/debug`. No production Rust, metric schema,
wire profile, authority or health threshold changed. The known
`proc-macro-error2 v2.0.1` future-compatibility warning remains.

These are tested example rules and query evaluation, not failure-driven live
notifications after their complete pending intervals, Alertmanager delivery,
reviewed production SLOs or dashboard/deployment qualification. No full
workspace, formation fault-matrix, secured-proxy/Collector recipe, overhead or
release rerun is claimed. M4 remains open for its pending trace/log decisions
and delivery, reviewed formation overhead and full operator/deployment handoff.

## Formation monitoring incident runbook — 2026-09-08

P-OBS-DOCS now has a [read-only formation incident runbook](../worker-monitoring-runbook.md)
for peer loss, local unreadiness and missing telemetry. It connects the current
catalogue/probe contract to public worker/CLI inspection and the existing
interrupted-admission stop procedure. It does not add a diagnostic API,
remediation action, alert threshold or process supervision policy.

The runbook distinguishes failed monitoring access from a health response,
missing series from measured zero, historical operation outcomes from current
participation, and local membership records from direct reachability. Commands
have explicit response/time bounds; peer observation is finite, and admission
recovery retains its original observation budget. Worker/CLI manuals, both
operator story sets, the documentation index and planning entries now link it.

### Assertions and refreshed evidence

| Assertion | Evidence and boundary |
| --- | --- |
| Healthy locked formation | The existing `check-formation-cli.py --observability` public journey now reads all three probes and corresponding health gauges on all workers after lock convergence. Responses have the exact bounded bodies and no-store policy; safely enforcing the lock does not fail readiness |
| Healthy survivors while a peer is suspected/dead | The same complete handoff/leave/readmission/crash/restart journey checks survivor probes/gauges at both liveness checkpoints. Every crashed-peer inspection retains exact schema, formation, queried source, target, certificate and local-view semantics. Three focused helper tests reject missing/wrong identity, schema and source even when a label matches |
| Read-only triage during telemetry outage | The extended real Prometheus journey runs `cluster info`, `ls`, `inspect` and all probes before failure, during collector failure, while Prometheus is stopped/reaped and after scraper recovery. Worker identity/source remains unchanged. Existing lock/unlock controls, fresh ingestion and exporter failure/recovery assertions are preserved; these test mutations are not runbook instructions |
| Metrics and alert compatibility | Prometheus/promtool 3.5.0 pass with 163 series and six rules, including synthetic alert state/exclusion fixtures. Neither catalogue nor rule expression changed. This remains real ingestion plus synthetic alert timing, not notification delivery |
| Owner/role failure interpretation | Two existing production-router HTTP tests pass: `http_probes_track_initialization_role_failure_and_closed_owner` and `http_probes_detect_running_owner_stall_and_recovery`. The latter holds the real owner while scrapes remain possible, then checks health/progress recovery. These are in-process owner/Unix-HTTP fixtures, not an OS-process stall or remote control API |
| Copyable local examples | The runbook's curl GET commands (two-second total limit, one-second connect limit, 1 KiB probes / 32 KiB metrics, HTTP status suffix) and read-only CLI commands ran against a fresh isolated ordinary worker. curl 8.14.1 returned healthy bounded replies and the expected gauges. The temporary worker terminated through the existing bounded process guard; no operator credential was required for these local same-user reads |
| Repository documentation | `make docs-check` passes for 124 Markdown files; scoped `git diff --check` passes. Links into the runbook and its source contracts were inspected |

The HTTP helper suite passes **26 tests**; the evidence helper suite passes
**6 tests**. The initial sandboxed helper run encountered nine socket-permission
errors; the approved local-socket rerun passed without test changes. An initial
build command used the application directory name `orishu-ctl` as its package
ID and was rejected; the corrected `orishuctl` build below passed. Neither was
a worker runtime failure. The pre-existing `proc-macro-error2 v2.0.1`
future-compatibility warning remains.

Commands run from the repository root (socket checks need local socket access):

```sh
python3 -m unittest discover -s scripts -p test_formation_http.py
python3 -m unittest discover -s scripts -p test_formation_evidence.py
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-formation-cli.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus --trace-metrics
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing,formation-fault-test --bin orishu-worker http_probes_ --target-dir target/formation-flow-observability
make docs-check
```

The supplementary curl/CLI smoke uses the exact commands in the runbook with
the fixture's private socket and ephemeral loopback port substituted. It is
a healthy local walkthrough, not fresh verification of every incident branch.
The older catch-up/ejection/unresolved and secured-proxy evidence retains its
recorded scope; those fault/deployment journeys were not rerun here.

Dirty source-built Linux base is
`5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`; the ordinary worker has both telemetry
capabilities and no formation-fault feature. Only documentation and test
harnesses changed in this increment. No production Rust, wire profile,
credentials, retries, metric catalogue or health behavior changed.

| Ordinary executable | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `ac500b489f011683f1fb193a6915d04f2af580f5879378b5b43e15309ec11f68` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

This completes the formation incident-triage increment, not M4 or all of
P-OBS-DOCS. Service/container deployment, dashboards, applicable correlation
walkthroughs and final operator acceptance remain open, together with trace/log
decisions and reviewed formation overhead. No full workspace, four-feature or
full fault-matrix rerun, new secured-proxy/Collector qualification, overhead
measurement or release acceptance is claimed.

## Admission-state catch-up outcome metrics — 2026-09-08

The final named formation-metric row now has receiver, owner-lifecycle, HTTP
and independent-process evidence. Twelve optional `orishu_worker_catchup_`
counters distinguish authorized job creation, seven receiver outcomes and four
terminal owner outcomes. The [catalogue](../orishu-observability.md#admission-state-catch-up-outcomes)
owns their names and precise accounting. A job spans Begin, all pages and
Confirm; neither pages nor successful reliable exchanges count as adoption.

`CatchupPreparation` retains one non-cloneable IO observation beside its existing
reserved completion permit. The observed receiver wrapper records the actual
result before the old optional completion is sent. The observation moves with
that completion; owner acceptance/fencing records its terminal decision and
drop accounts for cancellation or an abandoned owner decision. Credentials,
baselines, peer IDs and operation IDs never enter the metrics record. Existing
adoption/error behavior, three-attempt/90-second lifecycle bounds, 25-second
transfer limit, exchange deadlines, peer profile and admission authority are
unchanged. The guard is zero-sized without the feature; enabled collection
reuses the optional fixed owner-metrics allocation and disabled collection
allocates no record. No core dependency or per-page telemetry is added.

| Assertion | Evidence and boundary |
| --- | --- |
| Source binding, structured refusal, truncated frame, cross-snapshot and digest rejection; valid retry | `peer::catchup::client::tests` now uses the production observed receiver wrapper over authenticated QUIC. The existing cases check exact finite outcomes, no premature Confirm, one validated retry and shared-slot reuse. An intentionally wrong expected fingerprint fails before requests are emitted. These fixtures do not perform owner adoption; their observations end as owner-abandoned |
| Progressing whole-transfer deadline | `whole_transfer_deadline_with_progress_is_one_outcome_and_allows_retry` supplies a real multi-page baseline with 320 blocklist records, delays each initial page by three seconds (below the individual five-second exchange budget), reaches the actual 25-second receiver timeout and retries successfully on the same connection. The source loop is capped at 32 requests and the complete fixture at 35 seconds; the focused run finished in about 27.1 seconds |
| Active IO cancellation | `cancelling_an_incomplete_page_records_cancellation_and_releases_the_exchange` holds the second page after a valid prefix and partial frame, confirms the only exchange permit is occupied, drops the receiver future before source cleanup, observes immediate permit reuse and completes a fresh transfer. Two-second coordination and the ten-second fixture bound remain explicit; the cancelled transfer is not an invalid or timed-out one |
| No-op collection, one outcome per stage, saturation | Three `formation_metrics::catchup::tests` cover omitted/disabled storage, duplicate terminal notifications, validated-result abandonment and saturating counters without changing registry/deadline readings |
| Authorized starts, cancelled preparation, retry exhaustion and adoption | Existing `runtime::tests` catch-up journeys now enable metrics and check every event. Refused concurrent preparation adds no start. Three cancelled jobs consume the existing attempt budget without per-page inflation; a valid retry records one actual safe owner adoption |
| Late valid/cancelled completion | Existing runtime leave, ejection, source-session loss and expired-lifecycle fixtures check owner fencing after real receiver completion or prepared-job cancellation. The closed-owner shutdown case records transfer validation plus owner abandonment, not adoption or retroactive transfer cancellation. Existing FIFO barriers establish completion consumption |
| HTTP failures and recovery | Ten `diagnostics::tests::http_` cases pass in the combined telemetry/fault build, including real admission and malformed/partial-baseline/stalled-exchange failures. Catch-up cases scrape zero before scheduling, the appropriate receiver failure with no adoption, then one validated owner adoption after recovery while preserving identity and truthful probes. These are runtime/wire/Unix-HTTP fixtures, not independent-process fault journeys |
| Public three-worker flow | The ordinary `check-formation-cli.py --observability` journey now scrapes all twelve counters: A has no receiver job, B/C each have one adoption after exact public handoff, quiescent stage sums match started jobs, leave retains counters and process restart clears them. Full leave/readmission/crash/restart and original identity/credential assertions pass |
| Catalogue and access | Real/max-width parser checks, 151-base/163-combined Prometheus ingestion and the secured Nginx proxy recipe pass under the unchanged 32 KiB limit. Collector/scraper outage and recovery retain authenticated control; the proxy retains actual downstream write-expiry evidence |

The first lifecycle-test edit named the runtime's worker type incorrectly;
compilation failed and the fixture was corrected to `RunningWorker`. The first
whole-deadline fixtures incorrectly assumed ten pages: the 200-record source
reported seven, and 320 records plus the policy leaf reported eleven. The test
now requires the relevant bound—at least nine three-second pages—then validates
the complete returned baseline on retry. No pagination or deadline changed.
The active-cancellation test initially called a feature-gated pressure method
in an omitted-feature build; it now reads the existing feature-independent
test permit count. These were fixture corrections, not runtime fixes.

The first focused HTTP batch passed three cases and failed seven on the old
blanket maximum-width estimate: 29,595 bytes plus a 4,096-byte trace reserve.
That estimate assigned seven fractional bytes to every integer sample. The
refined bound retains twenty digits for all integer counts/gauges/buckets and
seven additional bytes only for duration sums/totals, consistent with the
maximum-value parser fixture. The conservative combined bound is now 32,676
bytes, still below 32,768; actual maximum-width payloads also parse successfully.
The production limit was not increased and no instrument was omitted to pass.
The repeated ten-case HTTP batch passed.

The complete four-feature all-target suites passed before the final test-only
active-cancellation extension:

| Capabilities | Library passed | Binary passed | Process passed |
| --- | --- | --- | --- |
| Neither | 129 | 37 | 35 |
| `observability` | 155 | 44 | 45 |
| `otlp-tracing` | 154 | 37 | 37 |
| Both | 180 | 45 | 48 |

Trace-enabled library suites additionally pass the provider-isolation child
while marking it ignored in the outer suite. The combined process suite still
ignores manual overhead measurement. After adding active cancellation, that
case passed in each feature mode and all seven receiver cases passed together
with both telemetry features. Four-feature Clippy passes with warnings denied;
three membership dependency-purity tests, six evidence-harness tests and
twenty-three HTTP-harness tests pass. Commands used for the successful checks:

```sh
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --lib whole_transfer_deadline_with_progress --target-dir target/formation-flow-observability --quiet
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing,formation-fault-test --bin orishu-worker diagnostics::tests::http_ --target-dir target/formation-flow-observability --quiet
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability --quiet || exit
done
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --lib cancelling_an_incomplete_page --target-dir target/formation-flow-observability --quiet || exit
done
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --lib peer::catchup::client::tests --target-dir target/formation-flow-observability --quiet
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
python3 scripts/test_formation_evidence.py
python3 scripts/test_formation_http.py
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus --trace-metrics
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --observability
```

Both Prometheus modes also pass three/six alert rules, scraper outage and fresh
re-ingestion; the combined mode includes collector failure/recovery. Proxy
downstream checks retain one response and 16,384 written bytes at capacities
16 and 1, with approximately five-second expiry and subsequent reuse. No proxy
production configuration changed.

Dirty source-built Linux base remains
`5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`. The final ordinary worker has both
telemetry capabilities, tracing disabled unless explicitly configured, and no
formation-fault feature. Shared-target builds ran sequentially and did not
replace executables used by a live harness. Socket checks used approved sandbox
access. The pre-existing `proc-macro-error2 v2.0.1` future-compatibility warning
remains. This is not a new full workspace/fault-matrix run, separate Collector
recipe run, accepted overhead measurement or release qualification.

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `ac500b489f011683f1fb193a6915d04f2af580f5879378b5b43e15309ec11f68` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

The catalogue, worker/CLI manuals, operator stories, configuration/Prometheus
guides and planning inventories are reconciled with the additive counters.
The finite formation-metrics inventory is covered at its named boundaries;
cross-peer/log correlation, their unaccepted choices, formation-stage overhead
and full operator/deployment handoff still keep M4 open.

## Registered-session capacity gauges — 2026-09-08

The retained-session inventory row now has implementation, real owner/transport
accounting, HTTP scrape and operator documentation evidence. Four optional
`peer_registry_` gauges expose total retained entries and the provisional
subset, with capacities 64 and 16. Outgoing pinned-introducer bindings share
the latter budget with incoming applicants. The
[catalogue](../orishu-observability.md#registered-session-capacity-gauges)
defines the exact names and availability semantics; this is not a count of
live sockets, admitted members or introduction-ready workers.

The registry updates one diagnostic provisional count alongside its existing
insert/promotion/pruning paths. The actual admission gate continues to inspect
the retained entries, not the diagnostic scalar. Publication packs four bounded
counts into one atomic reading in the existing optional formation-metrics
allocation; this internal representation is neither wire nor persisted data.
Scrapes perform a fixed-size read, without session traversal, an owner command
or supervision progress. A dropped owner registry withdraws all four readings;
an active empty registry retains its capacities even without a peer listener.
Closing transport alone leaves entries counted until normal pruning. Formation
changes keep the active registry capacity and remove invalid old bindings.

No peer profile, admission limit, pruning schedule, credential/default, core
dependency or health authority changed. Runtime-disabled collection allocates
no metrics record; feature-omitted builds have no diagnostic scalar/projection.
The compiled-in bookkeeping is not claimed to have zero CPU overhead.

| Assertion | Production boundary and evidence |
| --- | --- |
| Coherent snapshots, disabled collection, retained cumulative events | `formation_metrics::tests::registry_snapshot_is_coherent_and_does_not_reset_events` alternates complete readings with a concurrent reader; `disabled_collection_retains_no_storage` covers disabled and feature-omitted modes |
| Inbound registration, duplicate refusal, formation change, shutdown | `peer::registry::tests::real_handshake_is_decided_by_owner_and_closed_when_formation_changes` uses actual QUIC framing and owner registration, checks 1/1 occupancy, unchanged duplicate counts, generation pruning and zero capacities after owner exit |
| Promotion versus admission refusal | Existing real join/owner tests now check provisional occupancy before the join and total/provisional readings after insertion or refusal; accepted promotion retains one total slot and retires its provisional slot |
| Provisional capacity and expiry | `applicant_capacity_expiry_and_drop_are_bounded_with_real_connections` registers sixteen real authenticated connections, refuses the seventeenth, advances the existing pruning input beyond expiry and checks empty active then dropped registry readings; it does not claim wall-clock timer scheduling evidence |
| Total capacity, refusal and reuse | `registered_member_capacity_refusal_and_reuse_are_distinct_from_provisional` retains sixty-four admitted bindings with zero provisional occupancy, refuses a sixty-fifth, closes/prunes an actual entry and reuses the slot. The 15-second fixture uses real QUIC identities and production registry/codec calls; member records are seeded, so this is not sixty-four public admissions |
| Outgoing provisional registration | `peer::dial::tests::lost_join_ack_keeps_remote_insertion_and_reports_unresolved_locally` observes 1/64 total and 1/16 provisional usage after the real dial/handshake/owner registration, before beginning the retained lost-ACK journey |
| Owner abort | `owner_abort_with_retained_session_withdraws_registry_pressure` registers a real applicant, aborts the owner, awaits task cancellation, checks all four readings zero and requires the held connection to close within the five-second fixture budget |
| Executable exposition and control | `peer_ingress_metrics::real_worker_peer_handshake_failures_reach_optional_scrapes` preserves malformed-handshake and TLS-task checks, then sends a valid application handshake, scrapes total/provisional occupancy 1→0 after release/pruning, and performs authenticated lock/unlock with unchanged one-member identity. Probes-only mode exercises registration/control with no metrics route. The complete fixture is bounded to twenty seconds |
| Parser, byte/series bounds and collector isolation | The process scrape test validates actual and conservative maximum-width exposition with promtool. Combined tracing pressure tests require 151 samples within the unchanged 32 KiB bound. Real Prometheus ingestion checks 139 base/151 combined series, including active empty registry capacities 64/16 when inbound-adapter capacity is zero |

An initial focused registry run passed all ten existing tests. The added total-
capacity fixture initially failed compilation because its failure message tried
to format `HandshakeReply`, which deliberately has no `Debug` implementation.
The fixture now formats only the error; no production debug/payload exposure
was added. The first combined all-target run then passed 176 library tests
(plus the isolated provider child) and 45 binary tests; the process suite
passed 47 cases but failed `diagnostics_bind_precedence_uses_only_the_selected_address`
when the worker reported `Address already in use` before startup. That fixture
releases its selected reservation before spawning the worker. The conflicting
port owner was not captured, so no specific competing process or runtime defect
is claimed. Its isolated rerun passed; the subsequent complete four-feature
matrix passed without modifying bind policy or adding silent retries.

| Capabilities | Library passed | Binary passed | Process passed |
| --- | --- | --- | --- |
| Neither | 127 | 37 | 35 |
| `observability` | 151 | 44 | 45 |
| `otlp-tracing` | 152 | 37 | 37 |
| Both | 176 | 45 | 48 |

Each trace-enabled library suite additionally runs its provider-isolation case
successfully in a child while marking it ignored in the outer suite. The
combined process suite leaves the manual overhead measurement ignored. All
four Clippy modes pass with warnings denied, and the three membership
dependency-purity tests pass. Exact successful commands:

```sh
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --test standalone diagnostics_bind_precedence_uses_only_the_selected_address --target-dir target/formation-flow-observability --quiet
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability --quiet || exit
done
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus --trace-metrics
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --observability
```

Both real Prometheus modes pass with three/six alert rules, scraper outage/control
and fresh re-ingestion; the combined mode also exercises collector failure and
recovery. The real Nginx proxy recipe passes its credential/route isolation,
scrape, upstream/pre-HTTP/downstream pressure and recovery assertions. Both
downstream capacities retain one generated response, 16,384 written bytes and
approximately five-second write expiry. No production proxy setting changed.
An earlier Prometheus command misspelled its tool directory as `linux-adm64`;
it stopped with `FileNotFoundError` before testing, then the corrected commands
above passed. This invocation error is not an ingestion failure.

The ordinary three-worker observability harness also passes public handoff,
leave/readmission, crash/restart, exact formation views and existing admission,
activity, transport and deadline scrape assertions. It runs with metrics enabled
and tracing runtime-disabled. This preserves the public formation control at
the new binary checkpoint; the specific registered-occupancy HTTP assertions
are in the process test above, not newly added to this three-worker harness.

Validation uses the dirty source-built Linux base
`5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`. Feature builds were sequential in the
shared target directory; the rebuilt ordinary binaries contain both telemetry
capabilities and no formation-fault feature. Socket tests ran with approved
sandbox access. The pre-existing `proc-macro-error2 v2.0.1` future-compatibility
warning remains. Full workspace/fault-matrix validation, a new Collector recipe
run, overhead measurement and release qualification are not claimed here.

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `a931122e0d05f8f115a61bffbe00271eed9e8aa20fba5a1dfc37206166de1562` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

The catalogue, worker manual, protocol terminology, configuration/Prometheus
guides, both operator story sets and task inventories are reconciled with this
additive surface. Final worker formatting, `make docs-check` (123 Markdown
files) and scoped tracked-document whitespace checks pass. The worker's
combined-feature doc-test command completes successfully with zero examples;
it is not additional runtime or full-workspace evidence.

```sh
cargo fmt -p orishu-worker -- --check
make docs-check
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --doc --target-dir target/formation-flow-observability --quiet
```

Catch-up validation/adoption outcomes remain the metric gap;
trace/log decisions, formation overhead and the complete M4 operator handoff
remain open.

## Inbound connection-capacity gauges — 2026-09-08

The inbound occupancy row now has implementation and real adapter/HTTP evidence.
Four optional gauges expose the existing TLS permit budget and connection-task
budget through `IngressSnapshot` and the diagnostics listener. The
[catalogue](../orishu-observability.md#inbound-connection-capacity-gauges)
defines names, last-published versus live readings, availability and shutdown
semantics. The limits remain sixteen TLS permits and sixty-four connection
tasks; no admission, wire profile, credential, default or core type changed.

The metrics record holds ten existing atomic counters and one constant-size
pressure projection. Scrapes read actual semaphore availability and the
adapter-published `JoinSet` length through a short lock protecting only a weak
semaphore reference and three scalars. Spawn and collection publish occupancy;
completed-but-uncollected tasks remain included. There is no per-connection
metric allocation or scrape-time session scan. The adapter lifetime guard
withdraws occupancy/capacity on exit or abort; weak-reference matching prevents
an old guard from updating or clearing a replacement adapter's projection.
Cumulative outcomes retain their process-lifetime values. Omitted collection
and its guard remain zero-sized; runtime-disabled collection retains no weak
reference, pressure allocation or snapshot.

| Assertion | Evidence |
| --- | --- |
| Initial, occupied and released actual permits; guard replacement/drop | `peer::ingress::tests::pressure_tracks_actual_permits_and_guard_lifetime_without_stale_reset` uses a real semaphore, private production guard and exact readings |
| TLS capacity/expiry/reuse | `peer::server::metrics_tests::tls_capacity_refusal_and_deadline_release_are_observable` holds sixteen actual TLS exchanges, checks 16/16 occupancy, refusal, timeout-driven 0/0 occupancy and a successful fresh connection |
| Connection-task capacity distinct from TLS | `connection_capacity_refusal_and_reuse_are_observable` completes TLS for sixty-four held application handshakes, checks TLS usage zero and task usage 64, then task usage 63 after collection and 64 after healthy reuse |
| Normal shutdown | `errors_success_and_shutdown_cancellation_are_distinct` observes one held TLS permit and three connection tasks, retains unchanged owner identity/admission outcomes, and requires zero gauges after dispatcher shutdown with exact cumulative outcomes |
| Adapter abort with unfinished work | `adapter_abort_with_pending_tls_and_handshake_withdraws_pressure` aborts the real dispatcher while TLS and application handshake work are held; gauges withdraw, connections close and both cancellation counters advance |
| Executable exposition and live task occupancy | `peer_ingress_metrics::real_worker_peer_handshake_failures_reach_optional_scrapes` checks four gauge types, capacities 16/64, 0→1→0 task occupancy around a retained TLS-complete connection, continuing readiness/public status, probes-only absence, actual/max-width promtool parsing and unchanged identity |
| No peer listener and combined tracing | Optional-feature process tests retain disabled behavior; the real Prometheus harness requires all four gauges at zero when no adapter is configured. Combined exporter-pressure tests now require 147 samples within 32 KiB |

The initial three-test ingress run passed the two capacity cases but failed
the initial-state assertion: the fixture sampled before the spawned adapter
had run and incorrectly expected capacity 16 instead of the specified zero.
The wait now permits the valid pre-start zero projection before requiring the
active capacities. This was a fixture-ordering correction, not a runtime fix.
All 85 peer tests then passed, followed by the complete worker all-target suites
in all four feature combinations:

| Capabilities | Library passed | Binary passed | Process passed |
| --- | --- | --- | --- |
| Neither | 125 | 37 | 35 |
| `observability` | 148 | 44 | 45 |
| `otlp-tracing` | 150 | 37 | 37 |
| Both | 173 | 45 | 48 |

Trace-enabled library runs each mark one provider-isolation test ignored in the
outer suite and execute it successfully in a separate child (one additional
pass). The combined process suite leaves the manual overhead measurement
ignored; no performance budget is accepted by these results.

```sh
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --lib peer:: --target-dir target/formation-flow-observability --quiet
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability --quiet || exit
done
```

The catalogue adds four samples: 135 base and 147 with runtime tracing, with
the same five histograms and 32 KiB response limit. Existing metric names and
meanings are unchanged; historical 131/143-series results below remain results
for their earlier checkpoints. Worker/manual, configuration/protocol catalogue
counts, Prometheus consumers and both operator story sets are reconciled.
Four-feature Clippy with warnings denied and the three membership dependency
purity tests passed. The ordinary worker and CLI were then rebuilt with both
telemetry capabilities and no fault feature. Prometheus 3.5.0 parsed/ingested
135 samples with three alerts and 147 samples with six alerts; the combined
run retained trace collector failure/recovery, and both retained operator
control during scraper outage/re-scrape. The complete secured Nginx proxy
journey also passed against this build, including both downstream pressure
variants with 16,384 successful TLS-write bytes and approximately five-second
expiry. No production proxy setting was changed.

After the full matrix, the two full-ingress fixtures gained explicit real
owner lock/unlock commands under the occupied 16-TLS/64-task budgets, bounded
to one second before expiry. All four ingress tests passed again in each
metrics-capable build. These use the production already-authorized owner
control lane; they are not a new remote credential test. Combined-feature
Clippy was repeated for this final test-only strengthening.

```sh
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus --trace-metrics
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
cargo test --locked --offline -p orishu-worker --no-default-features --features observability --lib peer::server::metrics_tests --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --lib peer::server::metrics_tests --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability -- -D warnings
```

Tests requiring local sockets ran with approved sandbox access. Builds sharing
the target directory were sequential and did not replace a live journey's
executables. Dirty base remains `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1` on
source-built Linux; validation began September 7 and finished September 8.
The build retains the pre-existing `proc-macro-error2 v2.0.1` future-compatibility
warning. No full workspace/fault-formation batch, new Collector recipe run,
overhead measurement or release qualification was performed.
Worker formatting, `make docs-check` (123 Markdown files), and scoped
tracked-document whitespace checks passed on the final documentation revision.

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `dbf67ce69b741089545ae2082dfe419cba81972d93678cbcf45a1b3f3cc3f9f9` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

At that checkpoint, registered-session visibility and catch-up outcomes were
the next metric increments; the [later registry evidence](#registered-session-capacity-gauges--2026-09-08)
now covers the former. This does not close tracing/logging decisions,
formation-stage overhead or the full deployment/operator M4 handoff.

## Formation instrumentation coverage inventory — 2026-09-07

This is the finite metrics inventory requested by P-OBSERVABILITY slices 2–3
and the formation task's increment-selection contract. It reconciles actual
owner/adapter hooks, the current catalogue and existing test evidence. It does
not require a new instrument merely because an older checkpoint ended with
“broader instrumentation remains”; nor does a nearby counter establish a
different event. Trace/log correlation and overhead retain their separate gates.

`Covered` below means the named metric assertion has the linked evidence at
its documented boundary, not that every suite was rerun in this audit. Current
source inspection found concrete connection-visibility and catch-up outcome
gaps. No runtime, catalogue, feature, protocol or dependency changed here.

| Required observation | Actual owner/event and existing evidence | Disposition |
| --- | --- | --- |
| Issuer-local admission outcomes | `driver::Owner::apply` consumes `MemberAdmitted` and `AdmissionRejected`; `replay_join` counts only after validated encoding/session promotion. [Issuer wire tests](#issuer-admission-metrics--2026-09-07) distinguish insertion, refusal and replay; [process scrapes](#three-process-admission-scrapes--2026-09-07) exercise actual HTTP visibility | Covered for issuer insertion/refusal/replay; not joiner catch-up success or successful reply delivery |
| SWIM, gossip and anti-entropy activity | `TrafficCounters::record` runs after registry packet decoding and before core handling. It counts five SWIM body kinds, pull requests/replies and bounded collection lengths. [Activity evidence](#received-formation-activity-metrics--2026-09-07) includes repeated/ignored input, constant-time collection accounting and three-worker scrapes | Covered for decoded receive activity; not successful probes, newly merged records or convergence latency |
| Membership deadlines, retry exhaustion and stale timers | `formation_metrics::record` consumes actual transition diagnostics and the timer passed to that transition. [Deadline evidence](#membership-deadline-and-abandonment-metrics--2026-09-07) checks current versus stale timers and join/reconciliation abandonment | Covered; cancellation and peer-death decisions are distinct, and lost-ACK exhaustion can remain unresolved |
| Membership-packet decode/session rejection | Owner registry decode failure increments `peer_decode_rejections_total`; [wire/owner evidence](#owner-side-peer-decode-rejection-metric--2026-09-07) checks exact increments without admission or supervision progress | Covered at the owner packet boundary; not all framing, handshake or catch-up errors |
| Inbound authentication/application handshake failure | `peer::server` records TLS and initial application-stage outcomes with drop guards. [Ingress evidence](#inbound-peer-handshake-metrics--2026-09-07) separates failure, deadline, cancellation and local refusal, including real certificate and schema failures | Covered as stage aggregates, not authentication-only or per-cause classifications; focused tests and executable scrape refreshed below |
| Outbound attempts and TLS, including latency | `Dialer` records attempt/candidate terminal outcomes and fixed histograms; [outbound evidence](#outbound-dial-and-tls-metrics--2026-09-07) tests failure, cancellation, timeout, candidate fallback and four-slot pressure | Covered for those stage lifetimes, not accepted membership or incoming TLS duration |
| Reliable transport outcomes, latency and volume | Shared `ExchangePool` wraps handshake, membership and catch-up request/serve IO; [exchange evidence](#reliable-peer-exchange-metrics--2026-09-07) verifies exclusive terminal outcomes, histograms, partial framed bytes, refusal and cancellation | Covered for pool lifetime/application stream bytes; excludes decode/validation performed after a successful pool return, QUIC overhead and remote domain acceptance |
| Datagram volume and pre-pool refusal | `Traffic::submit`, registered receive and stream-task admission record finite outcomes and payload lengths; [datagram evidence](#datagram-and-pre-pool-traffic-metrics--2026-09-07) exercises actual QUIC success/refusal/failure, oversized receive and stream-task refusal | Covered at those local boundaries; not end-to-end loss, retransmissions or per-peer labels |
| Owner and transport queue pressure | `Handle::pressure`, `ExchangePool::pressure` and `Dialer::pressure` read four owner lanes, the 64-slot exchange pool and four outbound-attempt slots. Existing owner/HTTP and transport capacity tests distinguish queued/reserved work, cancellation and reuse | Covered for these published budgets; they are not live connection or session counts |
| Current inbound connection/TLS occupancy and capacity | `peer::server::spawn_observed` owns a 16-permit TLS semaphore and separate 64-entry connection-task `JoinSet`; the [inbound gauge increment](#inbound-connection-capacity-gauges--2026-09-08) adds actual pressure readings and HTTP exposition | Covered for adapter budgets, including uncollected completions, expiry/reuse and abort; not established session counts |
| Current registered-session visibility | `peer::registry::Registry` retains up to 64 authenticated entries, with a shared 16-provisional subset across incoming applicants and outgoing introducers. The [registry gauge increment](#registered-session-capacity-gauges--2026-09-08) publishes coherent occupancy/capacity through optional formation metrics | Covered for retained registry budgets, promotion/refusal, pruning/reuse, owner lifetime and HTTP exposition; not live-socket count, admitted membership or catch-up adoption |
| Joiner admission-state validation outcomes | The [catch-up increment](#admission-state-catch-up-outcome-metrics--2026-09-08) records actual receiver results and follows the reserved completion to owner adoption/fencing or abandonment | Covered for whole-job transfer/owner outcomes, invalid/refused/binding/truncated input, real total expiry, active cancellation/reuse, lifecycle fencing and HTTP/process exposition; not per-page attempts, a second admission decision or durable recovery history |
| Metric safety and configuration | The original inventory checked 131 base/143 combined series; registry visibility extended it to 139/151 and the [catch-up increment](#admission-state-catch-up-outcome-metrics--2026-09-08) validates the current 151/163 catalogue. Five finite histograms, bounded snapshots and the 32 KiB limit remain. [Foundation evidence](#m4-foundation-evidence-reconciliation--2026-09-07) covers configuration/feature independence, hostile labels/paths, saturation and scrapes | Covered for the current catalogue, including type-specific maximum-width and actual parser checks; future extensions must recheck feature/absence semantics, cardinality and maximum bytes |

The inventory treats received activity, transport latency and domain outcomes
as separate observations. It neither silently requires per-message histograms
or successful-probe/convergence measurements nor claims those measurements
exist. Additional timing instruments need a named operator assertion and event
definition; scientific instrumentation and fleet scaling remain later work.

### Next bounded increment: inbound connection-capacity gauges

**Delivered:** the [later gauge evidence](#inbound-connection-capacity-gauges--2026-09-08)
closes this increment. Its original contract remains below for reference, not
as another scheduled implementation. The later registry increment covers
retained-session visibility; catch-up outcomes remain a separate open row.

Close the inbound row without altering admission, concurrency limits or
peer profiles. Publish optional constant-size occupancy/capacity readings for
the existing TLS semaphore and connection-task budget. Count the resource the
server actually uses to admit work: a connection task may be pending TLS,
awaiting its application handshake, serving traffic, or completed but not yet
collected. It is not necessarily an established session or admitted member.
Do not infer current usage by subtracting independent cumulative counters.

Before wiring exposition, specify gauge names, lifetime/availability when no
peer listener runs, and release/reset behavior during expiry, task collection,
owner closure and adapter abort in the owning catalogue. Keep optional storage
in the IO shell; no session scans on scrape, new clock, labels, core dependency
or public control API is needed. Retain the independently compiled-out and
runtime-disabled behavior; absent collection is not a measured zero.

Extend the existing three `peer::server::metrics_tests` and
`real_worker_peer_handshake_failures_reach_optional_scrapes` rather than
constructing a second server. Assert initial/occupied/full/released readings,
successful control and capacity reuse, bounded teardown, unchanged outcomes,
and real HTTP samples. Recheck maximum exposition, parser/ingestion and relevant
feature combinations; update the catalogue, worker manual and both operator
story sets. Exit when these assertions pass at their named boundaries. Registry
occupancy, catch-up outcomes, new histograms, trace propagation, logging output,
overhead and fleet/deployment acceptance are not folded into this increment.

### Audit validation and limits

Inspected current worker runtime wiring, owner transitions/replay/traffic,
registry and catch-up client, ingress/dial/exchange adapters, metrics rendering
and their relevant tests. Refreshed exactly these commands with approved local
socket access, sequentially in the isolated target directory:

```sh
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --lib peer::server::metrics_tests --target-dir target/formation-flow-observability -- --nocapture
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --test standalone real_worker_peer_handshake_failures_reach_optional_scrapes --target-dir target/formation-flow-observability -- --nocapture
```

Both **passed**: three real ingress tests (169 other library tests filtered)
and one actual-worker scrape test (48 other process tests filtered). The latter
checks metrics-enabled and probes-only startup, exact 131 base samples, real
malformed authenticated handshake deltas, unchanged core outcomes/identity,
and pinned promtool parsing, including conservative maximum-width exposition.
Tracing is compiled in but runtime-disabled in this process fixture. These
passes verify existing counters and resource enforcement, not missing gauges.
Dirty base is `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`, source-built Linux.
No full worker/workspace suite, new four-feature matrix, independent formation
journey, Prometheus server, Collector, proxy or overhead run is claimed here.
`make docs-check` passed for 123 Markdown files, and scoped tracked-document
whitespace checks passed. No implementation or metric schema changed.
The original audit left three missing rows; the later inbound gauge increment
closes one. Registry visibility and catch-up outcomes remain open, as do the
separate trace/log, overhead and operator gates. This audit does not close M4.

## Monitoring proxy downstream backpressure and expiry — 2026-09-07

The final missing proxy foundation assertion now has real-worker/Nginx evidence
in `check-worker-monitoring-proxy.py::downstream_pressure`. The
[operator recipe](../testing-worker-monitoring-proxy.md#downstream-response-backpressure-and-expiry)
defines the finite fixture, evidence interpretation and deployment limitations.
No worker code, production limit, wire profile, credential policy or executable
changed. Only fixture debug logging and, in the explicit one-slot control, the
active request capacity differ from the existing proxy configuration.

The client uses a small receive window/segment size and verified monitoring
mTLS, then queues 100 real metrics requests without reading responses. A
healthy-reader control uses the same client settings. A bounded debug observer
correlates the source port, actual failed TLS write and subsequent WANT_WRITE;
it retains numeric aggregates only. Request counts and successful TLS-write
bytes must stop increasing while blocked and remain unchanged through the
response-send timeout. An unrelated timeout, closure without write pressure,
or fixture cleanup cannot satisfy the assertions.

| Variant | Observed processed requests / queued | Successful TLS-write bytes | Response-send expiry after first blocked write | Control and recovery |
| --- | --- | --- | --- | --- |
| Normal 16-request capacity | 1 / 100 | 16,384 | 5.000 s | Authenticated proxy metrics/probes, direct worker readiness and CLI lock/unlock succeed before expiry; fresh access succeeds afterward |
| Fixture-only 1-request capacity | 1 / 100 | 16,384 | 5.004 s | Proxy metrics/probes return 429 while occupied; direct readiness and CLI lock/unlock succeed; fresh proxy metrics/probes succeed after expiry while the original client remains open and unread |

The first normal-capacity extension passed the complete journey. Adding the
healthy-reader and one-slot controls also passed. The final rerun additionally
requires the timeout to identify response sending, not another connection
phase, and checks observer bounds through recovery/shutdown output. All nine
helper tests passed, including real socket EOF/byte controls,
connection-correlated write evidence, rejection of handshake/read retry and
unrelated timeout evidence, bounded debug input/inventory and raw-field discard.
No runtime defect was demonstrated or fixed by this increment.

Exact executed commands (local socket tests required approved sandbox access):

```sh
python3 scripts/test_worker_monitoring_proxy.py
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

Both commands **passed**. The full journey retains real Prometheus ingestion,
credential/route isolation, upstream release/timeout/reuse, mixed pre-HTTP
expiry, proxy/worker outage controls and exact final worker identity/state.
All fixture processes and observer threads exited within their cleanup budgets.
`make docs-check` passed for 123 Markdown files; scoped tracked-file whitespace
checks passed. The explicit commands above used the isolated existing binaries,
not a separate execution of the Makefile target or a default-target rebuild.
The tools remain Nginx 1.28.0 (`--with-debug`) and Prometheus/promtool 3.5.0 on
Linux, with dirty base `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`. The worker
was built with both telemetry features; tracing stays runtime-disabled in this
harness. Neither binary was rebuilt; SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `265291717a2c0b17a8130f6cf2b9dd797e5244fa7dfef527311bcfe2e551ca38` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

This establishes stalled-reader response work, expiry, independent control and
request-slot recovery, not continuously progressing trickle readers, complete
connection-pool exhaustion, process-memory measurement or whole-process
shutdown under stalled writes. No fresh Rust feature/workspace suite, formation
fault batch, Collector run, overhead measurement or release qualification was
performed. The proxy recipe, worker manual and both operator story sets now
link these limits. The foundation's named source-built Linux evidence gaps are
covered; formation instruments, trace/log and cross-peer correlation, reviewed
overhead and full P-OBS-DOCS deployment/incident handoff remain separate M4 gates.

## Monitoring proxy pre-HTTP pressure and expiry — 2026-09-07

The existing Nginx recipe now has a finite mixed pre-HTTP pressure journey in
`check-worker-monitoring-proxy.py`. No worker code, proxy timeout, connection
limit, TLS policy, credential or wire profile changed. Configuration comments
and the [operator recipe](../testing-worker-monitoring-proxy.md#pre-http-pressure-and-expiry)
clarify that the sixteen-request limit starts after a complete HTTP header,
while the 64-connection worker budget also includes pending and upstream work.
The recipe links the pinned Nginx handshake source and official directive
semantics; those sources inform the case but do not substitute for its runtime
evidence.

The harness holds sixteen real TLS ClientHello exchanges after receiving server
handshake bytes, withholding the remaining client flight. It also completes
sixteen TLS handshakes with valid monitoring certificates and sends incomplete
HTTP heads. Eight stay silent; eight receive one further byte at each of two,
two-and-a-half and three seconds. All setup must finish within two seconds.
Actual server flights/completed handshakes distinguish accepted connections
from the operating-system backlog. No client-side certificate-verification
bypass or synthetic server response is introduced.

Thirty-two concurrent EOF observers require server-side closure within one
seven-second budget from the first setup, with a four-second minimum from each
connection's creation. Concurrent observation before expiry rejects early
refusal rather than hiding it behind another connection's timeout. TLS response
observations are capped at 32 KiB per connection, HTTP at 8 KiB. Header expiry
may close silently or return 408, never metrics. Read timeout is failure, not
closure evidence. The extra header bytes do not extend the absolute header
budget. Reads begin after the trickle writes, avoiding concurrent operations on
one Python SSL object.

Before expiry, authenticated proxy probes/scrapes, direct worker readiness and
real CLI lock/unlock all succeed within the four-second setup/control window.
Fresh authenticated proxy requests also succeed after expiry while all original
client-side socket handles remain open. The original worker's exact public
state/identity comparison remains in the full journey. Cleanup never supplies
EOF or restarts the proxy to manufacture successful access; every process is
bounded and reaped through the existing harness guards.

The first idle-only mixed-pressure extension passed the complete proxy journey.
It was then strengthened with per-connection minimum times and the eight
trickling heads; that final complete journey also **passed**. The five new
`test_worker_monitoring_proxy.py` controls cover a real ClientHello, bounded
socket-pair EOF, oversized response, already-expired deadline and refusal to
treat a silent open socket as closure. The initial sandboxed helper run passed
two cases and failed three at denied socket send/shutdown operations (`EPERM`);
the approved local-socket rerun passed all five. The Makefile's existing proxy
target now runs these controls before the external journey.

The final rerun also includes the initial server flight in each TLS connection's
32 KiB total observation budget; all five helper controls and the full proxy
journey passed on that revision. `make docs-check` passed for 123 Markdown files,
as did scoped tracked whitespace checks. `make -n test-worker-monitoring-proxy`
confirmed the helper is in the recipe; that dry run is not a separate executed
Makefile journey or a rebuild of the default target directory.

```sh
python3 scripts/test_worker_monitoring_proxy.py
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
make docs-check
```

The full final run retains mutual trust/refusal, route and credential isolation,
actual Prometheus ingestion, both upstream capacity release/expiry branches,
proxy loss and absent-upstream controls. Nginx 1.28.0 and Prometheus/promtool
3.5.0 were the existing pinned local tools, not newly deployed services. Dirty
base remains `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1` on Linux. The previously
built worker has both telemetry features, with diagnostics enabled and tracing
runtime-disabled in this harness; no executable was rebuilt. SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `265291717a2c0b17a8130f6cf2b9dd797e5244fa7dfef527311bcfe2e551ca38` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

This covers the named finite pre-HTTP expiry/control case with spare capacity,
not exhaustion of the entire 64-connection pool, arbitrary TLS flooding or
proxy downstream response-write pressure. The latter remains the foundation's
next missing assertion. There is no new full-workspace Rust validation,
formation process/fault batch, collector recipe, overhead measurement or release
qualification. M4's other instrumentation/correlation/operator gates remain
open, and no compatibility or migration change is introduced by this test work.

## Direct diagnostics method and error contract — 2026-09-07

The foundation audit's direct-listener method/error gap now has an explicit
[HTTP dispatch contract](../protocol-client.md#worker-operational-diagnostics)
and real TCP process coverage. Enabled exact paths reject query parameters
(including an empty query) with 400 before method checking, then reject non-GET
methods with 405 and `Allow: GET`. Unknown or disabled paths return 404 first.
Raw path comparison prevents router-normalized case, percent/slash or dot-path
aliases from exposing diagnostics. HEAD receives error headers without a body;
it is not an alternate scrape or health operation.

The production router handles these errors itself rather than inheriting
framework error pages. Every dispatched error has `Cache-Control: no-store`,
plain-text content type and one fixed reason no longer than 64 bytes. It does
not inspect worker health/counters, read request bodies or echo request data.
Transport parsing/limit failures before dispatch retain their separate
contract. The existing 16-connection budget, body catalogue and timeouts are
unchanged; no new listener, authorization mechanism or dependency was added.

`diagnostics_direct_methods_paths_and_errors_are_bounded_and_isolated` starts
actual workers in metrics-and-probes, metrics-only and probes-only modes. It
tests the four routes against GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS,
TRACE and an extension method, with absent/empty/nonempty queries. Additional
raw requests cover unknown paths, operator token/mutation paths and aliases.
Each error response is capped at 2 KiB including headers; non-HEAD bodies must
equal their fixed reason, and HEAD bodies must be empty. Requests carry a real
operator token, marked path/query/header/body data, HTML/JSON Accept preferences
and sampled trace context; none may be reflected or yield metric samples.

The fixture preserves exact formation/node/member/lock state and working GET
controls. Where metrics are enabled it compares client/trace instruments before
and after all error traffic. In the combined build, tracing is explicitly
enabled at zero sampling: a positive client sampling counter proves active
instrumentation, unchanged counters prove diagnostics did not enter that path,
and the local receiver observes no connection. This is isolation evidence,
not a new context-propagation or distributed-trace implementation.

The first observability-only regression **failed** on `HEAD /metrics`: the
existing framework rejection lacked `Cache-Control: no-store`. Explicit dispatch
corrected that behavior. One intermediate build lacked the `salvo::http::Method`
import; after correction the focused process test passed in observability-only
and combined builds, including exact reasons and the positive trace control.
The complete matrix and external-harness results are recorded below rather
than inferred from that focused pass.

Compatibility: canonical GET scrape/probe behavior is unchanged. Clients must
not rely on encoded/slash aliases, framework-generated HTML/JSON errors or HEAD
as a probe. Disabled-route precedence stays 404; query errors on enabled routes
stay 400. No persisted, peer-wire or authority migration is involved.

Validation at dirty base `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1` passed:

| Feature build | Library / binary / subprocess tests passed |
| --- | --- |
| Neither | 125 / 37 / 35 |
| `observability` | 146 / 44 / 45 |
| `otlp-tracing` | 150 / 37 / 37 |
| Both | 171 / 45 / 48 |

Tracing builds retain one ignored provider-isolation fixture in the library
listing, whose environment-isolated child test runs separately; the combined
subprocess suite also retains the ignored manual overhead measurement. Neither
ignored item is claimed as ordinary suite evidence. All four all-target Clippy
runs passed with warnings denied. The final separate
`observability,formation-fault-test` diagnostics suite passed ten tests,
including lifecycle, catch-up, owner-stall and backpressure controls. The
membership resolved-dependency contract passed three tests.

```sh
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability --quiet || exit
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,formation-fault-test --bin orishu-worker diagnostics::tests --target-dir target/formation-flow-observability --quiet
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
```

The final ordinary worker/CLI build passed, retaining the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning. Linux/Rust 1.97.1
feature builds were sequential in the shared target; external processes began
only after the final non-fault build. Tested binary SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `265291717a2c0b17a8130f6cf2b9dd797e5244fa7dfef527311bcfe2e551ca38` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

Real Prometheus 3.5.0 passed 143-series and six-alert ingestion, collector
failure/recovery and operator control during scraper loss/re-scrape. The
secured-proxy harness passed mutual trust, isolation, real scraping,
active-request capacity/recovery and proxy/upstream outage:

```sh
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
cargo fmt -p orishu-worker -- --check
make docs-check
```

Worker formatting, docs validation (123 Markdown files) and scoped tracked
whitespace checks passed. The worker manual, observability/scrape guides and
task indexes now describe the dispatch contract and remaining scope. No full
workspace suite, independent three-worker/fault process batch, pinned Collector
process recipe, new overhead measurement or release qualification was rerun.
The foundation's remaining proxy pre-HTTP/downstream pressure assertions and
the other M4 gates stay open.

## Diagnostics response backpressure — 2026-09-07

The foundation audit's slow diagnostics reader gap now has two named tests
through the production diagnostics router and bounded HTTP server, with a real
membership owner and separate authenticated operator listener:

- `diagnostics::tests::http_stalled_metrics_reader_expires_without_blocking_probes_or_control`
- `diagnostics::tests::http_stalled_metrics_reader_reclaims_the_only_connection_slot`

Both use real Unix transport and 1,024 valid pipelined `/metrics` requests,
without inflating the actual response or mocking writes. The existing test-only
observing acceptor now has sibling-module visibility; it forwards scalar and
vectored IO unchanged, recording actual pending writes, bytes, timeout errors
and transport drop. No production interface or server limit changed.

After witnessing a pending write, the fixture verifies positive but incomplete
handler execution and stable handler/byte counts over 100 ms. It requires fewer
than all 1,024 requests to have been generated and less than 1 MiB written into
the held transport. These are finite fixture bounds, not measured production
SLOs or total process RSS. The catalogue retains its separate 32 KiB body bound.

At normal capacity (16), fresh health/metric requests and authenticated
lock/unlock complete during the stalled write. A one-slot variant proves actual
reclamation: a second probe receives no response during a bounded pre-expiry
check, then succeeds after server-side timeout and drop. Both require the
existing five-second write expiry, a four-second minimum post-pressure
observation, unchanged formation/node/member identity and a healthy-reader
control. Only afterward does the fixture drain/close the stalled client and
request owner/server shutdown. Cleanup cannot manufacture successful
reclamation. A twelve-second outer timeout bounds the fixture, including
cleanup; server tasks use a `JoinSet` so failure drops abort them.

The first compile attempt used nonexistent `Summary.locked`; the test was
corrected to the public `Summary.membership_locked`. The two focused cases then
passed without any runtime fix. Broader binary checks passed: **37** tests with
neither feature, **44** with `observability`, **37** with `otlp-tracing`, **45**
with both, and **50** with `observability,formation-fault-test`. These include the
existing shared-helper operator-response/assembly and HTTP health regressions.
All four normal feature builds passed worker all-target Clippy with warnings
denied. Commands:

```sh
for worker_features in '' observability otlp-tracing observability,otlp-tracing observability,formation-fault-test; do
  cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --bin orishu-worker --target-dir target/formation-flow-observability --quiet || exit
done
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
```

Checkpoint: dirty base `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`, Linux,
Rust 1.97.1. Shared-target builds were sequential; no external harness used a
replaced binary. Only tests, test-only helper visibility and documentation
changed. There is no runtime compatibility/migration effect.

After the broad matrix, a final positive handler-count assertion was added.
Its first focused rerun failed at sandboxed Unix listener creation (`EPERM`),
before exercising the handler. The approved socket-enabled rerun passed both
cases in observability-only and combined builds; both all-target Clippy checks
also passed on that final revision:

```sh
for worker_features in observability observability,otlp-tracing; do
  cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --bin orishu-worker diagnostics::tests::http_stalled_metrics_reader --target-dir target/formation-flow-observability --quiet || exit
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
cargo fmt -p orishu-worker -- --check
make docs-check
```

Worker formatting, documentation validation (123 Markdown files) and scoped
tracked-document whitespace checks passed. The observability guide, local
scrape recipe and worker manual now describe the precise tested behavior.

This closes the named HTTP/Unix diagnostics response-backpressure assertion,
not TCP/proxy downstream pressure, HTTP/2 credit behavior, full diagnostics
saturation or whole-process shutdown with a blocked response write. Direct
listener method/error coverage and proxy pre-HTTP/downstream assertions remain
in the foundation inventory. No full-workspace/all-target test run, independent
formation process batch, Prometheus/proxy/Collector harness, overhead measurement
or release qualification is claimed. M4 remains open.

## M4 foundation evidence reconciliation — 2026-09-07

This completes the formation task's foundation **audit**, not P-OBSERVABILITY
slices 1–2 or M4 acceptance. Current handlers, configuration, process fixtures
and the checked-in proxy recipe were inspected before the focused reruns below.
No runtime, test, dependency, feature, credential or wire contract changed.
`covered` means evidence at the named boundary; it does not mean every row was
rerun or that a fixture establishes a wider deployment guarantee.

### Assertion dispositions

The owning requirements are [ADR 0017](../adr/0017-worker-operational-observability.md),
the [diagnostics protocol](../protocol-client.md#worker-operational-diagnostics),
the [health matrix](../orishu-observability.md#planned-probe-interpretation),
and P-OBSERVABILITY slices 1–2 and acceptance criteria. Instrument completeness,
correlation and overhead retain their separate M4 gates.

| Assertion | Evidence and boundary | Disposition / remaining action |
| --- | --- | --- |
| Initializing, idle/non-compute standalone and lock/unlock health | `diagnostics::tests::http_probes_track_initialization_role_failure_and_closed_owner`, production HTTP router and actual owner; refreshed below | Covered; successful readiness is not permission to introduce or proof of globally converged lock state |
| Startup remains latched after role failure, transition and closure | Same HTTP fixture and catch-up fixtures; finite `ProcessHealthState`/`OwnerHealth` projections in `health.rs` | Covered at runtime/HTTP boundary; no storage role is fabricated |
| Required-role failure withholds readiness | Same HTTP fixture cancels a guarded role task; `main.rs` gives production client-listener tasks the same lifetime guard | Covered for current client-role supervision, not recovery of failed listeners or future roles |
| Pre-adoption joining and post-adoption incomplete/failed catch-up | All five `http_*catchup*`/`http_pre_adoption*` cases in the refreshed diagnostics suite, including malformed and partial baselines | Covered with real wire/runtime/HTTP and exact identity assertions; not a substitute for process restart/ejection journeys |
| Exhausted admission, peer loss and self-ejection | [Phase audit](#pre-adoption-http-probes-and-phase-coverage-audit--2026-09-07) links distinct issuer-loss and wire-ejection process evidence | Covered at those recorded checkpoints, not rerun here; no affected runtime change in this audit |
| Owner stall/recovery and shutdown cannot be hidden by scrapes | `http_probes_detect_running_owner_stall_and_recovery`, `http_reads_cannot_hide_stalled_owner_shutdown`; refreshed actual-owner/HTTP tests | Covered; reads do not refresh supervision |
| Whole-process shutdown with unfinished diagnostics | `unfinished_diagnostics_requests_do_not_prevent_process_shutdown`; refreshed executable/SIGTERM path with fixture sockets held open | Covered for unfinished requests, not stalled response writes |
| Optional capability/defaults and independent route groups | `config::tests::diagnostics_*` plus `diagnostics_route_groups_follow_file_environment_and_cli` and `loopback_diagnostics_expose_real_health_and_runtime_disable_binds_nothing` | Covered by refreshed four-feature config/process checks: runtime off, selected port remains occupied without conflict, probes-only/metrics-only and disabled-route 404 |
| Configuration precedence and startup refusal | `diagnostics_bind_precedence_uses_only_the_selected_address`, `diagnostics_enablement_precedence_and_invalid_config_are_checked_at_startup`, malformed/occupied-port/unsupported-exposure tests | Covered by refreshed subprocess tests; refusals precede credential/state creation where asserted, and wildcard/non-loopback binding remains refused |
| Trace capability independent of diagnostics | Refreshed `standalone` tracing tests in all four builds, including omitted-feature refusal, disabled/zero/enabled receipt, configuration precedence, credential validation and actual collector mTLS | Covered at these local subprocess boundaries; does not establish the entire three-worker telemetry-independence journey |
| Successful GET content/status, bounded probes, caching and route absence | Refreshed HTTP health assertions verify 200/503, plain text and `no-store`; local process checks cover metrics content type, query rejection and absent operator/token routes | Covered for those assertions; direct-listener method/error-route matrix remains below |
| Finite metric catalogue, parser and maximum exposition | `peer_ingress_metrics::real_worker_peer_handshake_failures_reach_optional_scrapes` refreshed with promtool on actual and conservative maximum-width bodies; live trace-pressure fixture checks 143 samples | Covered for 131 base / 143 combined samples and the unchanged 32 KiB body budget; not a downstream response-delivery test |
| Scrapes do not recursively count client requests or export secrets | Refreshed loopback process and trace receipt/pressure tests; repeated scrapes preserve client counts, seeded token/name checks, fixed aggregate catalogue | Covered at these boundaries; trace/log redaction still needs the selected logging output |
| Oversized heads, header count, unfinished reads and full connection capacity | Refreshed `diagnostics_reject_oversized_headers_and_preserve_normal_access`, both unfinished-request expiry variants and `diagnostics_full_connection_budget_preserves_operator_control_and_recovers` | Covered: 16 actually served/held connections, bounded refusal/expiry, authenticated control and capacity reuse; not arbitrary flood or slow-response evidence |
| Secured remote access alternative and credential separation | Refreshed `check-worker-monitoring-proxy.py`: real Nginx mTLS, both trust directions, exact routes/methods, no operator-token substitution, monitoring credential denied mutation, authorized control and Prometheus ingestion | Covered on source-built Linux/loopback for the accepted proxy boundary; native worker TLS and anonymous remote probes are not supplied or required by this recipe |
| Proxy upstream pressure, timeout, recovery and outage isolation | Same refreshed harness: 16 held upstream requests, 17th 429, release and unreleased-timeout branches, upstream EOF/reuse, worker loss 502, proxy loss without identity/health change | Covered at the documented upstream boundary; downstream readers and pre-HTTP TLS pressure remain below |
| Local collector/scraper outage, telemetry drops and bounded shutdown | Refreshed `tracing_pressure` executable test and real Prometheus `--trace-metrics` harness: held export, queue-full accounting, lock/unlock, receipt recovery and shutdown; scraper loss/re-scrape | Covered locally; supersedes the old phase audit's “export unavailable” gap, but not concurrent multi-worker outage/overhead acceptance |
| Direct diagnostics HTTP methods and error responses | [Explicit error dispatch and actual TCP worker matrix](#direct-diagnostics-method-and-error-contract--2026-09-07), covering method/query/path precedence and all route groups | Covered: fixed non-cacheable reasons, HEAD without a body, no aliases/reflected input/metric or client-span side effects, and unchanged exact formation state; pre-dispatch transport failures retain their own contract |
| Slow diagnostics response reader | Initially supported only by operator-summary/shared-server evidence; now has [two actual diagnostics router tests](#diagnostics-response-backpressure--2026-09-07) | Covered at the named HTTP/Unix boundary: actual write pressure, bounded generation, timeout/drop, queued-probe slot reuse and continuing control; proxy downstream remains separate |
| Proxy pre-HTTP/TLS pressure | [Mixed pre-HTTP expiry journey](#monitoring-proxy-pre-http-pressure-and-expiry--2026-09-07): 16 incomplete TLS handshakes plus 16 incomplete headers, including eight trickling heads | Covered for the finite accepted-client/expiry/control case with spare capacity; not full worker-pool exhaustion or arbitrary TLS flood resistance |
| Proxy downstream reader | [Actual TLS write-pressure journey](#monitoring-proxy-downstream-backpressure-and-expiry--2026-09-07), unchanged production buffering/timeouts, real metrics, healthy-reader and one-request-capacity controls | Covered for finite stalled-reader pressure: bounded generation, response-send expiry, request-slot reuse and continued legitimate access/control without reading or closing the stalled client to manufacture success |
| Applicable manuals, stories and recipes | Local Prometheus, secured-proxy and Collector recipes have recorded scoped runs; worker/CLI manuals and both operator stories describe the boundaries | Covered for these foundation findings, including direct-listener and proxy pressure instructions; service/container deployment and remaining dashboards/incident walkthroughs stay with P-OBS-DOCS, not this test mapping |

At the original audit checkpoint no runtime defect was demonstrated. The later
direct-handler regression above exposed and corrected framework error dispatch.
The later proxy downstream increment closes the last named foundation evidence
gap at its scoped boundary; nearby shared-transport tests alone did not waive
it. Monitoring credential lifecycle beyond the tested provisioning/trust
path remains an operator handoff obligation; this audit does not certify
rotation/revocation. Optional anonymous remote probe exemption is not enabled,
so it grants no exception to the all-mTLS proxy policy.

Workload stop, storage-role initialization/failure and replication-specific
instruments stay with P-OBSERVABILITY slice 4 and their runtime/storage owners.
Packaged artifacts/platform qualification stays with P-INSTALL/M8 and
P-OBS-DOCS's release slice. Formation instrumentation completeness, cross-peer
and log correlation, reviewed concurrent overhead and the full M4 operator
journey remain required in their existing rows; none is waived by this audit.

### Refreshed evidence

Dirty-worktree base: `5b7da3492086b098dd1a6e2e7db5b1eb97d33db1`;
Linux `6.17.0-41-generic`, Rust `1.97.1`. Feature builds ran sequentially in
`target/formation-flow-observability`, before external harnesses began. The
first sandboxed diagnostic subprocess run failed nine tests at TCP listener
creation (`EPERM`), with two passing startup checks. The approved socket-enabled
rerun passed; this was not a worker-behavior failure or a changed test deadline.

| Refreshed suite | Result |
| --- | --- |
| `observability,formation-fault-test` binary `diagnostics::tests` | 8 passed |
| `standalone diagnostics`, features: observability / neither / tracing / both | 11 / 3 / 3 / 11 passed |
| Binary `config::tests::diagnostics`, each of the four feature sets | 2 passed each |
| `standalone tracing`, features: neither / observability / tracing / both | 4 / 4 / 6 / 6 passed |
| Combined-feature `standalone peer_ingress_metrics`, promtool explicitly enabled | 1 passed, including real and maximum-width parser controls |
| Membership resolved dependency contract | 3 passed |
| Real monitoring proxy | Passed mutual trust, isolation, ingestion, active-request capacity/recovery and proxy/upstream outage |
| Real Prometheus with trace metrics | Passed 143-series / 6-alert ingestion, collector failure/recovery and operator progress during scraper loss |

Exact focused Rust commands:

```sh
cargo test --locked --offline -p orishu-worker --no-default-features --features observability,formation-fault-test --bin orishu-worker diagnostics::tests --target-dir target/formation-flow-observability --quiet
for worker_features in observability '' otlp-tracing observability,otlp-tracing; do
  cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --test standalone diagnostics --target-dir target/formation-flow-observability --quiet || exit
done
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --bin orishu-worker config::tests::diagnostics --target-dir target/formation-flow-observability --quiet || exit
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --test standalone tracing --target-dir target/formation-flow-observability --quiet || exit
done
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features observability,otlp-tracing --test standalone peer_ingress_metrics --target-dir target/formation-flow-observability --quiet
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
```

The final ordinary worker executable has both telemetry features and no fault
feature, SHA-256 `7eed544e372d2e519589718933b97004919c3c40aa9c24cfc215f76641974ec7`.
The existing CLI was not rebuilt and retains the preceding checkpoint's hash
`fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda`.
The external runs used that exact pair; this does not relabel the CLI as a new
build. Commands:

```sh
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

No full workspace suite, Clippy, complete optional-feature all-target suite,
three-worker/fault process batch, pinned Collector process recipe, benchmark or
release qualification was rerun. Scoped checks above must not be reported as
those broader acceptance gates.

`make docs-check` passed for 123 Markdown files, and scoped `git diff --check`
passed for the four changed planning/ledger documents. Existing dirty runtime
and harness work was preserved; no code edits were made by this audit.

### Proposed next increment: diagnostics response backpressure

**Completed at the [scoped HTTP/Unix boundary above](#diagnostics-response-backpressure--2026-09-07).**
The following brief is retained as its acceptance contract, not the next open
implementation task.

Prioritize the concrete missing slow-reader assertion. Reuse the existing
observed real-transport helper and production diagnostics router/server. Bound
the pipelined GET workload; witness a real pending write before asserting
pressure, verify that response generation does not consume the entire workload,
and retain the stalled client while the server's existing write deadline
reclaims it. In parallel, assert fresh probes and authenticated operator control
on unpressured capacity. Include a healthy reader/reuse control and bounded
cleanup on failure. A response merely fitting in the socket buffer is not
backpressure evidence.

The proposed focused exit command is the existing binary
`cargo test --locked --offline -p orishu-worker --no-default-features --features observability --bin orishu-worker diagnostics::tests --target-dir target/formation-flow-observability`;
verify the newly added named case actually runs, then repeat with both telemetry
features and record its boundary. Preserve the current 5-second write budget,
use a finite fixture observation/cleanup budget, and capture only bounded,
secret-free pressure evidence. A discovered defect requires its own failing
regression before a production correction. Update the observability guide and
monitoring recipe's precise slow-reader limitation with the result.

Stop after this case and its controls; proxy TLS/downstream pressure, the direct
method contract, new instruments and correlation decisions are not part of it.
This is the audit's proposed implementation handoff, not a claim that the case
already exists or that a missing assertion has passed.

## Membership deadline and abandonment metrics — 2026-09-07

The serialized owner now records eight optional counters from its actual timer
input and completed core transition's diagnostics. The
[catalogue](../orishu-observability.md#membership-deadline-and-abandonment-counters)
distinguishes consumed direct/relay, indirect, suspicion, reconciliation and
join-retry timers; stale core timer input; and join/reconciliation abandonment.
No core type, membership decision, timer/retry budget, peer profile, credential,
listener default or dependency changed. Metrics remain diagnostic observations,
not an additional health or recovery authority.

One fixed eight-counter allocation is enabled only with runtime metrics.
Disabled collection does not scan diagnostics, allocate storage or read a
clock; omitted capability is zero-sized. Enabled recording is O(returned
diagnostics), bounded by the core's existing per-update limit, with O(1)
retained storage. Input generation and diagnostic payloads are never retained.
Counters saturate, survive formation replacement and owner closure, and reset
on process restart. The catalogue is now 131 base / 143 tracing samples, still
within 32 KiB; prior 123/135-series evidence below remains historical.

| Assertion | Evidence boundary |
| --- | --- |
| Disabled storage, fixed classification and saturation | `formation_metrics::tests` exercises every counter, excludes matching stale tokens, and proves saturation |
| Core stale-token semantics | `real_core_rejects_unowned_timers_without_counting_deadlines` folds all five unowned timer kinds through the real core; only stale-input observations increase |
| Abandonment without a timeout | `real_core_abandonment_without_timer_is_not_deadline_expiry` exhausts actual reconciliation continuation and join session-rebind budgets through core transitions; abandonment increases without any deadline counter |
| Real owner timer consumption | `owner_deadline_metrics_follow_real_timers_and_survive_leave` uses the production owner scheduler and one/two seeded peers with no usable routes. Fixture-only 50 ms probe/indirect, 100 ms suspicion/reconciliation and multiplier-one bounds reach expiry; actual member state is checked independently. One peer has no helper and cancels indirect probing, while two peers exercise its deadline |
| Stale input, retention and leave cancellation | Same fixture sends a current-generation unowned token, then leaves and shuts down with counters retained. `leave_clears_real_owner_deadlines_and_fences_late_timer_input` retains real pending timers, verifies leave clears them, and rejects old-owner-generation delivery without counting core expiry or stale core input |
| Real wire ACK control | `authenticated_acks_complete_only_their_outstanding_probe` keeps rejected unrelated/replayed ACKs and valid correlated ACKs on authenticated datagrams; an answered probe adds no deadline observations. Earlier pre-session expired probes remain counted |
| Real wire join uncertainty | `lost_join_ack_keeps_remote_insertion_and_reports_unresolved_locally` drops the actual accepted reply with a fixture-only single-attempt/500 ms retry budget. Exactly one retry-due and join-abandoned event accompanies unchanged source identity and unresolved status, while the issuer retains its accepted member; no source admission refusal is invented |
| Executable catalogue and disabled routes | Existing malformed-initial-handshake fixture now checks 131 samples with all eight owner counts at zero, unchanged identity/readiness, metrics absent in probes-only mode, and actual/maximum-width promtool parsing within 32 KiB |

These are intentionally different evidence boundaries. Seeded owner timers
prove scheduler/core accounting, not authenticated process-level failure
detection. Pure transition controls prove event classification, not peer IO.
The full three-worker journey provides the separate executable crash/retention
and restart wiring evidence recorded below.

Two focused test assumptions failed and were corrected without production
behavior changes:

- Initial owner fixture expected an indirect expiry with only one peer. The
  core's no-helper branch cancels that timer and starts suspicion immediately.
  The final fixture retains that zero-expiry control and adds a helper-present
  case for actual indirect timeout. Its focused rerun passed.
- Initial ACK fixture expected process-lifetime direct-expiry count zero. A
  probe started before its peer session existed had already expired. The final
  control compares observations around each actual received/answered probe;
  it retains prior history rather than resetting counters. Its rerun passed.

Four metric/core tests, the real owner timer fixture, lost-ACK fixture, real
ACK fixture and leave-cancellation fixture passed focused checks. All four
complete worker all-target capability suites passed:

| Features | Library passed | Binary passed | Subprocess passed | Explicitly ignored |
| --- | --- | --- | --- | --- |
| Neither | 125 | 37 | 35 | None |
| `observability` | 146 | 42 | 44 | None |
| `otlp-tracing` | 150 | 37 | 37 | One manual provider profile |
| Both | 171 | 43 | 47 | Manual provider profile and overhead measurement |

Actual and maximum-width Prometheus parsing passed with the optional metrics
capability. Clippy passed in all four capability builds, the three membership
dependency-contract tests passed, and all 23 formation HTTP helper tests passed.
The ignored manual cases do not establish an overhead budget.

```sh
cargo test --locked --offline -p orishu-worker --features observability --lib formation_metrics --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features observability --lib owner_deadline_metrics --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features observability --lib lost_join_ack_keeps_remote --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features observability --lib authenticated_acks_complete --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features observability --lib leave_clears_real_owner_deadlines --target-dir target/formation-flow-observability --quiet
for worker_features in observability '' otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability --quiet || exit
done
```

The final source-built Linux worker/CLI pair has both optional capabilities and
no fault feature. Build passed with the existing `proc-macro-error2 2.0.1`
future-compatibility warning. Different feature builds ran sequentially before
external harnesses started; none replaced a live executable. Dirty-worktree
base is `5e459a40e9b3b349116afa0c59e71a5e545e2fd4` (Linux 6.17.0-41, Rust 1.97.1).

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `7bbf2e027d21c21c05ad3458bf90138474d7f94a28aa3cf7975f1461f94e9a8e` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

```sh
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
python3 scripts/test_formation_http.py
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
cargo fmt -p orishu-worker -- --check
make docs-check
```

Real Prometheus 3.5.0 passed 131-series/three-alert and 143-series/six-alert
ingestion, retaining actual collector failure/recovery in the latter and
operator control during scraper outage in both. Its no-peer/no-join fixture
checks all eight new counters are zero; those zeros are not exercised-deadline
evidence. The secured Nginx proxy passed mutual trust, route/credential
isolation, real scrape ingestion, capacity reclamation and proxy/upstream outage.

The complete ordinary three-worker journey passed handoff, lock/unlock,
leave/readmission and crash/restart. New assertions check all eight counters
on each process, retention across C's leave, and reset to zero on B's fresh
standalone restart. Around B's SIGKILL, the sum of direct-probe and suspicion
deadline counts across survivors increases. This is aggregate process activity,
not per-peer attribution: the retained exact public identity/liveness assertions
and observed suspicion-to-death journey independently establish B's state.

```sh
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --observability --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

The catalogue, configuration/protocol references, worker manual, operator
stories, roadmap and task indices are reconciled. Formatting, scoped
tracked-file whitespace and `make docs-check` (123 Markdown files) passed.

This closes the previously unclassified deadline/abandonment boundary at the
stated scopes, not every formation instrument. Existing received activity,
admission and transport metrics retain their definitions. Completed probe or
reconciliation outcomes, broader core diagnostic classification and any
required formation-stage durations still need a finite event/evidence mapping;
the aggregate core-diagnostic count is not evidence for each subtype. Correlated
peer spans and logs remain gated by ADR 0025 and the logging-output choice.
Probe/security acceptance, reviewed concurrent overhead and remaining operator
handoff also stay open. No full workspace, separate fault-process matrix,
Collector mTLS rerun, release qualification or overhead acceptance is claimed.

## Outbound dial and TLS metrics — 2026-09-07

P-OBSERVABILITY slice 3 now observes the shared outbound dialer: whole attempts,
individual QUIC/TLS candidates, terminal duration and the four-slot capacity
gate. The [catalogue](../orishu-observability.md#outbound-dial-and-tls-metrics)
adds 31 samples, making 123 base or 135 with tracing enabled, within the existing
32 KiB consumer bound. No peer profile, core message, credential, dependency,
retry/connection budget or runtime default changes. Previous 92/104-series
results below remain historical, not evidence of the new catalogue.

The dialer owns one optional fixed allocation. Disabled collection reads no
clock and allocates no counters; compiled-out collection is zero-sized. Counts
and duration sums/buckets saturate. Scrapes copy bounded independent readings
without owner or exporter work. No target identity, endpoint, certificate or
error payload is retained. A completed attempt returns a pending reply for
owner validation; it does not establish admission, catch-up or recovery.

| Assertion | Evidence boundary and result |
| --- | --- |
| Disabled collection, finite timing and saturation | `peer::dial_metrics::tests` pass: no disabled allocation/timing, exclusive outcomes including disposal, inclusive histogram boundaries, saturating counters/buckets/sums |
| Successful bootstrap and certificate refusal | `real_dispatcher_dial_pins_tls_and_owner_ack_identity` passes with exact attempt/TLS counts for a valid reply and wrong pin; wrong introducer identity is still rejected by the separate owner/session validation path |
| Candidate timeout followed by successful reconnect | `admitted_dial_skips_silent_candidate_and_restores_policy_exchange` passes: a real silent UDP candidate consumes a slot, five-second timeout precedes healthy fallback, one completed attempt/two terminal TLS candidates are counted, and authenticated policy reconciliation resumes |
| Dial pressure, refusal and cancellation | `real_silent_dials_bound_capacity_and_release_it_on_cancellation` passes: four real QUIC Initial packets fill the four permits, fifth call refuses without waiting or a stage duration, owner control progresses, disposing the four tasks records exactly four cancelled attempts/TLS candidates and releases every slot |
| Candidate exhaustion versus whole deadline | `outbound_metrics_distinguish_candidate_exhaustion_total_timeout_and_local_failure` passes: one silent candidate produces TLS timeout plus failed attempt; four candidate slots reach the real 15-second total timeout; pending nested work is cancelled or has already timed out, never counted twice; a closed local endpoint produces initiation failure |
| Failure/cancellation after TLS succeeds | `outbound_metrics_cancel_after_tls_and_reject_application_exchange` passes: real mutual TLS and framed request, then either withheld reply/future disposal or stream reset. TLS remains completed; only the attempt becomes cancelled/failed; owned connection closes and dial capacity is released |
| Executable exposition and disabled-route control | `real_worker_peer_handshake_failures_reach_optional_scrapes` passes with 123 samples, unused dialer counts at zero/capacity four, actual and maximum-width promtool parsing, unchanged identity/readiness, and metrics absent in probes-only mode |
| Combined catalogue during exporter pressure | Existing combined-feature subprocess suite passes with 135 samples, held/recovered exporter and bounded response; this is local export, not peer-context propagation |

The ten focused dial/metric tests passed first. Full worker all-target results:

| Features | Library passed | Binary passed | Subprocess passed | Explicitly ignored |
| --- | --- | --- | --- | --- |
| Neither | 124 | 37 | 35 | None |
| `observability` | 141 | 42 | 44 | None |
| `otlp-tracing` | 149 | 37 | 37 | One manual provider profile |
| Both | 166 | 43 | 47 | Manual provider profile and overhead measurement |

```sh
cargo test --locked --offline -p orishu-worker --features observability --lib peer::dial --target-dir target/formation-flow-observability --quiet
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features observability --all-targets --target-dir target/formation-flow-observability --quiet
for worker_features in '' otlp-tracing observability,otlp-tracing; do
  ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability --quiet || exit
done
```

The initial unprivileged observability all-target invocation failed its library
stage (77 passed, 64 failed across socket-dependent fixtures). The authorized
rerun of the same command passed all three targets as listed above; retain the
failed invocation rather than treating it as a pass. The other three complete
feature runs used authorized local-socket execution. The focused ten-test run,
23 formation HTTP helper tests, worker formatting and documentation checks also
passed. Manual profiling/overhead tests were not run and establish no budget.

Clippy passed in all four capability configurations; the membership dependency
contract passed all three tests. The final worker/CLI build passed with both
optional capabilities and no fault feature. The existing
`proc-macro-error2 2.0.1` future-compatibility warning remains. Feature-changing
builds ran sequentially; no subsequent build replaced a live test executable.

```sh
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
python3 scripts/test_formation_http.py
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
```

Real Prometheus 3.5.0 passed 123-series/three-alert and 135-series/six-alert
ingestion, with collector failure/recovery in the latter and operator control
during scraper outage in both. The secured Nginx proxy passed mutual trust,
route/credential isolation, scrape ingestion, request-capacity reclamation and
proxy/upstream outage. These no-join scraper fixtures assert zero outbound
activity and an unused four-slot dialer; they do not prove exercised outbound
IO. The independent three-worker journey supplies that production wiring check.

```sh
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --observability --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

The complete ordinary three-worker run passed. It checks positive completed
attempts/TLS candidates on both joining workers,
bounded per-worker gauges/histograms, retained counts across C's leave and zero
outbound activity on B's fresh standalone restart, alongside the complete
ordinary handoff/lock/leave/readmission/crash/restart journey.

Dirty-worktree base is `5e459a40e9b3b349116afa0c59e71a5e545e2fd4`, source-built
Linux 6.17.0-41, Rust 1.97.1. Executable SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `8fe91af6f358e6b07a1928304da627baff85ea51185561605a80090e8121c55f` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

Catalogue/configuration, protocol cross-references, source-build worker manual,
operator stories and roadmap/task indices are reconciled. Worker formatting,
scoped tracked-file whitespace and `make docs-check` pass. No
Collector mTLS rerun or new overhead measurement is claimed here.

Remaining slice-3 work is not another implementation of this dialer. Preserve
the preceding inbound, reliable and datagram evidence. Map the still-aggregate
formation diagnostics and outcomes to the required catalogue: `Owner::apply`
currently counts `transition.diagnostics.len()` without distinguishing such
core outcomes as `AntiEntropyAbandoned`, `JoinAbandoned` or stale correlation.
Received SWIM/gossip/anti-entropy activity and transport lifetime are not proof
of completed probes/rounds or formation-stage timing. Audit the required finite
events and their actual owner/core outputs before naming further instruments;
do not add peer labels or infer domain timeout from every expired timer.
Cross-peer propagation and trace/log correlation remain gated by ADR 0025 and
the logging-output choice. Full probe/security acceptance, reviewed concurrent
formation overhead and the remaining operator handoff also stay open. No full
workspace, separate fault-process matrix or release/platform qualification is
claimed by this increment.

## Datagram and pre-pool traffic metrics — 2026-09-07

P-OBSERVABILITY slice 3 now covers the production datagram submission adapter
and registered receive loop with eight optional process-lifetime counters.
The [catalogue](../orishu-observability.md#datagram-and-pre-pool-traffic-counters)
defines submitted/refused/failed outcomes, payload bytes, pre-validation
reception, oversized input and per-connection stream-task refusal. Normal
member sends and departure sends share the observed adapter; the old unobserved
`ExchangePool::datagram` helper retains its behavior. No peer profile, core
message, authority, datagram semantics, transfer/connection limit, deadline,
dependency or runtime exposure default changes.

The worker shares one optional fixed array between its owner and transport
handles. Collection allocates nothing per event, reads no clock and retains no
peer/packet/error data; compiled-out collection is zero-sized. Existing limits
remain enforced with metrics disabled. All counters saturate, survive formation
changes and reset on restart. These observations do not measure all datagram
loss, remote delivery, accepted admission or successful probes.

| Assertion | Evidence boundary |
| --- | --- |
| Disabled collection, byte boundary and saturation | `peer::traffic::tests`: absent storage/snapshot with unchanged 1200-byte check, inclusive/over-cap receives, stream refusal and saturation of all eight counters |
| Local submission outcomes | `real_datagram_submissions_distinguish_refusal_failure_and_local_success`: real authenticated QUIC; wrong class, oversized payload and a peer without datagram capability are refused; a closed connection fails after checks; one valid payload is received and contributes exactly one submitted byte |
| Pre-validation receive accounting | `registered_io_counts_hostile_datagrams_and_pre_pool_stream_refusal`: production registered loop counts an oversized payload without owner decode, counts malformed in-cap CBOR before owner rejection, then receives a valid Ping and emits its correlated Ack with exact request/reply payload totals |
| Per-connection refusal and recovery | Same fixture holds sixteen incomplete streams, observes the seventeenth reset before pool reservation, and verifies only the initial handshake has a terminal reliable outcome. Lock/unlock still completes. Releasing one stream permits a valid serialized PullRequest/PullReply exchange |
| Departure branch and replay | The controlled fixture runs no periodic probe/anti-entropy maintenance: accepted leave adds exactly one submission to its sole peer, changes formation identity and retains counters. Exact receipt replay emits no second departure. Pending stream slots are reclaimed through leave, followed by bounded owner/dispatcher shutdown |
| Executable catalogue and feature isolation | The existing malformed-handshake executable fixture checks all eight new counters are zero, 92 base samples, probes-only exclusion and unchanged identity/readiness; maximum-width base text plus 4 KiB trace headroom fits 32 KiB and passes promtool. The held-export fixture checks 104 combined samples |
| Three-worker production wiring | The complete ordinary CLI journey exposes positive submitted/received datagram and byte totals on every worker, retains them across C's leave and observes zero totals on B's fresh standalone restart before readmission; exact membership and prior reliable/admission/activity assertions remain |

The registered-loop fixture deliberately advertises 32 QUIC bidirectional
streams and uses a 1400-byte initial/minimum MTU to reach the application's
16-task and 1200-byte gates independently. These are fixture-only overrides,
not changes to production transport budgets or default-capacity deployment
evidence. The process journey retains production settings. Aggregate scrapes
prove retention/reset and activity, not which specific departure packet caused
an increment; exact departure accounting is established by the controlled
fixture above.

All four worker all-target capability suites and Clippy configurations passed:

| Features | Library passed | Binary passed | Subprocess passed | Explicitly ignored |
| --- | --- | --- | --- | --- |
| Neither | 123 | 37 | 35 | None |
| `observability` | 136 | 42 | 44 | None |
| `otlp-tracing` | 148 | 37 | 37 | One manual provider profile |
| Both | 161 | 43 | 47 | Manual provider profile and overhead measurement |

```sh
cargo test --locked --offline -p orishu-worker --features observability --lib peer::traffic --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features observability --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
```

The four focused traffic tests passed first; subsequent exact departure/replay
and reliable-outcome separation assertions also pass in the final combined
suite. Real Prometheus 3.5.0 passed 92-series/three-alert and
104-series/six-alert ingestion, including fresh values after scraper and trace
collector recovery. The secured Nginx proxy passed mutual-trust/route isolation,
scraping, sixteen held requests, capacity reuse and proxy/upstream outage. The
complete ordinary three-worker formation journey passed handoff, lock/unlock,
leave/readmission and crash/restart with the new metrics assertions.

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --observability --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

The executable pair was built together with both optional capabilities and no
fault feature; feature-changing builds ran sequentially and did not replace
live-test binaries. Dirty-worktree base is
`5e459a40e9b3b349116afa0c59e71a5e545e2fd4`, source-built Linux, Rust 1.97.1.
Executable SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `a57aee148d03f182f026577c694575224cdc62922c9fc0ebf363a108c3928913` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

The three membership dependency-contract tests and 23 formation HTTP helper
tests passed. Worker formatting, scoped tracked-file whitespace and
`make docs-check` (123 Markdown files) passed. No failed check was observed in
this increment; the existing `proc-macro-error2 2.0.1` future-compatibility
warning remains. Validation commands:

```sh
python3 scripts/test_formation_http.py
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
cargo fmt -p orishu-worker -- --check
make docs-check
```

The catalogue adds eight names and keeps the existing 32 KiB consumer budget;
historical 84/96-series results below remain scoped to their checkpoint. The
protocol/catalogue/configuration, worker manual, operator stories, roadmap and
task indices are reconciled. The preceding inventory's datagram IO and
registered stream-task refusal gaps are now covered at these boundaries;
outbound dial/TLS and broader formation-stage timing/outcome mapping remain.
M4 still requires the remaining probe/security, instrumentation, correlation,
reviewed overhead and operator/deployment gates. ADR 0025 and the logging-output
choice are unchanged. No complete workspace, separate fault-process matrix,
external Collector mTLS rerun, release qualification or new overhead acceptance
is claimed here; unrelated concurrent workload-format edits were preserved.

## Reliable peer exchange metrics — 2026-09-07

P-OBSERVABILITY slice 3 now instruments the shared worker `ExchangePool`:
request/serve terminal outcomes, duration distributions, partial stream bytes
and pool occupancy/capacity. The
[catalogue](../orishu-observability.md#reliable-peer-exchange-metrics) owns exact
events, units, buckets, reset rules and exclusions. No membership semantics,
profile, credential policy, transfer limits, dependencies or deadline changes
are introduced. The existing runtime metrics switch selects one fixed counter
allocation before startup; disabled collection reads no timing clock and
retains the original QUIC IO helpers. Omitted collection is zero-sized.

Byte accounting performs constant local work per IO completion and publishes
aggregates once at termination, including cancellation and partial failures.
It includes framing, not datagrams or QUIC/TLS overhead; locally accepted bytes
are not delivered bytes. Duration includes failures, early refusals and
cancellation, unlike the completed-client-handler histogram. Counters are
process-lifetime, saturating and non-transactional across a scrape.

| Assertion | Evidence boundary |
| --- | --- |
| Optional allocation/clock, saturation and exact histogram boundaries | `peer::exchange_metrics::tests`: disabled snapshot/storage, terminal cancellation, partial bytes, saturated outcomes/bytes/sum/buckets and inclusive finite/overflow bucket selection |
| Real success and invalid partial input | `peer::exchange::metrics_tests::real_io_counts_success_and_rejected_partial_frames`: authenticated QUIC, exact request/reply byte totals, local invalid payload, truncated prefix/body, trailing byte and over-cap frame; rejected input never invokes the callback and healthy exchanges reuse capacity |
| Shared refusal and cancellation | `real_pool_refusal_cancellation_and_reuse_keep_exact_outcomes`: held request/owner callback, occupied slot in both pools, independently refused second request/serve, task cancellation, exact terminal counts and healthy reuse |
| Actual deadlines | `real_request_and_partial_input_deadlines_are_counted`: unanswered request and incomplete input each reach the existing five-second deadline, retain partial bytes, release slots and permit healthy reuse |
| Actual write pressure | `real_write_backpressure_keeps_partial_sent_bytes_on_timeout`: fixture-only 64-byte QUIC windows force a partial reply, which times out with positive but incomplete byte count and releases its pool permit |
| Production scrape and probes-only mode | `real_worker_peer_handshake_failures_reach_optional_scrapes`: a real malformed authenticated handshake reaches `serve_failed_total`, one duration sample and exactly five received bytes; no accepted/rejected core admission or identity/readiness change |
| Finite exposition and collector pressure | The same subprocess fixture checks 84 base samples and conservative maximum width, including 4 KiB trace headroom; actual and maximum-width text pass pinned promtool. The existing real-worker tracing-pressure fixture now validates 96 samples through held export, recovery and bounded shutdown |

The consumer budget increases from 16 KiB to 32 KiB for 84 base/96 combined
samples. Existing names and meanings remain unchanged. Three histograms each
use eight fixed `le` values; all other samples are unlabelled. Scrape consumers,
the protocol/catalogue/configuration, worker manual and both operator stories
are reconciled. Historical 50/62-series results below remain unchanged.

### Feature validation

Complete worker all-target suites passed sequentially in
`target/formation-flow-observability`:

| Features | Library passed | Binary passed | Subprocess passed | Explicitly ignored |
| --- | --- | --- | --- | --- |
| Neither | 122 | 37 | 35 | None |
| `observability` | 132 | 42 | 44 | None |
| `otlp-tracing` | 147 | 37 | 37 | One manual provider profile |
| Both | 157 | 43 | 47 | Manual provider profile and overhead measurement |

```sh
cargo test --locked --offline -p orishu-worker --no-default-features --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features observability --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
```

All four Clippy checks passed. The initial focused compilation exposed a
borrowed async test closure; adding `move` fixed it, and seven focused exchange
tests then passed. The later boundary test is included in the final feature
counts above. The preceding preparation attempt had been blocked by a
concurrently incomplete `orishu-workload` manifest; that crate now has its own
target. No unrelated scaffold or workspace membership was changed to bypass it.
The HTTP harness helper initially hit sandbox socket restrictions; its
authorized rerun passed all 23 tests. Nine Collector helper tests also passed.

### External acceptance and remaining scope

The worker and CLI were built together with both optional capabilities and
no fault feature. The following external checks passed against that executable
pair, without a feature-changing build replacing either live-test binary:

| Journey | Result and boundary |
| --- | --- |
| Prometheus 3.5.0, tracing disabled | 84 series, three existing alerts, exact catalogue/labels/histograms, healthy process gauges and real scraper outage/re-scrape |
| Prometheus 3.5.0, tracing enabled | 96 series, six existing alerts, collector failure/recovery and fresh post-outage delivery values |
| Secured monitoring proxy | Real mutual-trust/route isolation, scrape, sixteen held requests, capacity reuse and proxy/upstream outages |
| Complete ordinary three-worker formation | Public introducer handoff, exact live views, lock/unlock, leave/readmission and crash/restart; every worker exposed positive request/serve completion, stream bytes and duration counts, worker C retained counters across leave, and restarted B exposed zero reliable counts before readmission |
| Pinned Collector 0.160.0 mTLS recipe | Three invalid receiver-startup configurations and seven worker scenarios passed, including disabled/zero/enabled receipts, separate credential/trust/name failures and actual collector stop/restart with continued authenticated control; loopback scope only |

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --observability --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
python3 scripts/check-worker-otelcol.py --mtls --otelcol /tmp/orishu-otelcol-check.JlHHuV/otelcol --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

Checkpoint: dirty-worktree base `5e459a40e9b3b349116afa0c59e71a5e545e2fd4`,
source-built Linux 6.17.0-41-generic, Rust 1.97.1. Executable SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `c9dab22391bfab5b712722460113b74550ab371a6faace2bf51b404862a7ba6b` |
| `target/formation-flow-observability/debug/orishuctl` | `fc4b4e29e8282b09005c3976c2ab2a8165f02c560218fd3a8c17713fb3bcdfda` |

Both executable hashes were unchanged after the external checks. Concurrent
workload-format adoption continued in the workspace during this increment;
its manifest/model/documentation changes were preserved. Results apply to the
tested source snapshots and this executable pair, not a blanket validation of
all later concurrent edits or the complete current workspace.

The three membership dependency-contract tests passed. Worker formatting,
scoped tracked-file whitespace and documentation validation (123 Markdown
files) also passed. The existing `proc-macro-error2 2.0.1` future-compatibility
warning remains. Final check commands, including the helper tests above:

```sh
python3 scripts/test_worker_otelcol.py
python3 scripts/test_formation_http.py
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
cargo fmt -p orishu-worker -- --check
make docs-check
```

The finite slice-3 coverage inventory remains:

| Area | Current evidence / remaining assertion |
| --- | --- |
| Admission insertion, core refusal, retained-assignment replay | Covered at the previous owner-counter checkpoint; preserve distinctions from transport success |
| SWIM, gossip and anti-entropy received activity | Covered by owner counters and the existing three-worker journey; received packet/item counts are not completed probes/rounds or convergence latency |
| Inbound handshake outcomes and early capacity gates | Covered by the preceding inbound increment; returned TLS failure is not authentication-only attribution |
| Reliable exchange outcomes, duration, bytes and pool pressure | Covered at the adapter, executable scrape, parser/Prometheus and complete ordinary three-worker boundaries above; no per-peer or phase attribution implied |
| Pre-pool transport failures and volume/timing outside the pool | Missing instrumentation coverage: outbound dial/TLS, registered per-connection stream-task refusal and datagram IO are not counted by this increment; map their exact required events before implementation |
| Broader formation-stage timings and timeout/reconciliation outcomes | Missing evidence mapping beyond received activity and aggregate exchange lifetime; no full slice-3 closure claim |
| Cross-peer and log-correlated traces | Still gated by proposed ADR 0025 and the separate logging-output choice; neither is accepted by this increment |

M4 remains open for the remaining probe/security matrix, instrumentation,
correlation, reviewed formation overhead and operator/deployment handoff. No
full workspace, separate fault-process matrix, release/platform qualification
or new overhead acceptance is claimed here.

## Inbound peer handshake metrics — 2026-09-07

P-OBSERVABILITY slice 3 now accounts for inbound QUIC/TLS and initial
application handshake outcomes before normal membership exchange. The worker
adapter owns one optional ten-counter array; no core types, peer profile,
admission decisions, connection limits or deadlines change. Runtime-disabled
metrics/probes-only allocate no array; omitted `observability` uses a zero-sized
no-op. Counters survive formation changes, saturate and reset on process restart.
The [catalogue](../orishu-observability.md#inbound-peer-handshake-counters)
defines failure versus timeout versus dropped-future cancellation and the
separate TLS-slot/connection-task refusals. Successful handshakes are not
admissions; failures are not an authentication-only or peer-death signal.

| Assertion | Evidence boundary |
| --- | --- |
| Runtime-disabled collection and saturation | `peer::ingress::tests` verifies absent storage/snapshot, exclusive outcomes and `u64::MAX` saturation |
| Missing certificate, wrong application schema, healthy handshake and cancellation | `peer::server::metrics_tests::errors_success_and_shutdown_cancellation_are_distinct` uses real QUIC and owner shutdown; asserts exact counter deltas, unchanged formation/member count, no core admission or membership-decode counts |
| TLS capacity and both stage deadlines | `tls_capacity_refusal_and_deadline_release_are_observable` holds sixteen real handshakes through bounded UDP relays, observes the seventeenth refusal, server timeout counters, a healthy connection after release and silent application-handshake expiry |
| Connection-task capacity | `connection_capacity_refusal_and_reuse_are_observable` completes TLS on sixty-four retained application handshakes, observes connection refusal, releases one slot and completes a healthy exchange; remaining futures are cancelled at shutdown |
| Actual executable configuration and scrape | `peer_ingress_metrics::real_worker_peer_handshake_failures_reach_optional_scrapes` verifies a malformed authenticated application exchange with metrics enabled and probes-only; enabled mode checks all fifty samples, exact new deltas, unchanged core outcomes, readiness/identity and optional real `promtool` parsing |

Both base and trace-enabled scrapes now use a 16 KiB consumer budget: fifty
base series, sixty-two with runtime tracing. The real-worker test conservatively
bounds every base value at twenty integer plus seven fractional bytes and
reserves a further 4 KiB for the separately maximum-value-tested trace block.
The catalogue, configuration/protocol text, worker manual, both operator story
sets and affected scrape consumers are updated. Historical forty/fifty-two
series results below remain evidence for their original checkpoints.

Focused observability-only adapter tests (three) passed; combined-feature peer
tests passed (71 library, one binary), followed by the new subprocess fixture.
Complete worker suites then passed for all four capability builds, run
sequentially in the same isolated target:

| Features | Library | Binary | Subprocess | Explicitly ignored |
| --- | --- | --- | --- | --- |
| Neither | 121 | 37 | 35 | None |
| `observability` | 125 | 42 | 44 | None |
| `otlp-tracing` | 146 | 37 | 37 | One manual provider profile |
| Both | 150 | 43 | 47 | Manual provider profile and overhead measurement |

The following commands identify these runs; ignored measurements are not
performance acceptance:

```sh
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features observability --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --no-default-features --features otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
```

Clippy passed with warnings denied in each of those four feature combinations:

```sh
for worker_features in '' observability otlp-tracing observability,otlp-tracing; do
  cargo clippy --locked --offline -p orishu-worker --no-default-features --features "$worker_features" --all-targets --target-dir target/formation-flow-observability -- -D warnings || exit
done
```

The production worker and CLI were then built together, with both optional
capabilities compiled in and no fault feature. Real Prometheus 3.5.0 checks
passed for fifty series/three existing alerts with tracing disabled and
sixty-two series/six existing alerts with tracing enabled. The latter retains
collector failure/recovery; both retain operator control through scraper
outage/re-scrape. These are the existing rule examples, not newly accepted
handshake alert thresholds.

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

Executable SHA-256 after that build:

| Artifact | SHA-256 |
| --- | --- |
| `target/formation-flow-observability/debug/orishu-worker` | `9eecb5382ee8ac21c397ae349b93ea48e19cdff4b24295dde5765e98db5b9f38` |
| `target/formation-flow-observability/debug/orishuctl` | `1ac5bfa714be04cdfc3218a5b13d6a8b1019e9f4c297840db32e58f651be0f68` |

The same executable pair also passed the secured Nginx monitoring-proxy
journey (mTLS isolation, real scrape, sixteen held requests, capacity recovery
and proxy/upstream outages) and the complete ordinary three-worker formation
journey with metrics enabled. The latter retained public introducer handoff,
leave/readmission, crash/restart, exact membership assertions and existing
admission/activity counter reset checks. It does not rerun the separate fault
matrix or establish new per-handshake alert thresholds.

```sh
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-formation-cli.py --observability --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

Preserved failures: the first broader attempt failed at socket creation under
sandbox restrictions; its authorized rerun passed peer coverage. The new
subprocess fixture initially exited before serving its summary; explicitly
securing its temporary socket directory to mode 0700 made its rerun pass.
The first full suite caught an old 8 KiB read limit on successful `/metrics`
responses in the header-limit fixture. Only its successful-scrape budget was
raised to 16 KiB; rejected responses retain 8 KiB. The subsequent complete
suite above passed. Neither fixture correction changes production admission or
transport limits. The three collector-responder helper tests also initially
hit sandbox socket refusal and passed after authorization. Nine Collector
receipt/TLS helper tests and 23 formation HTTP harness tests passed. The
existing `proc-macro-error2 2.0.1` future-compatibility warning remains.

Final checks passed: the three membership dependency-contract tests, worker
formatting, scoped tracked-file whitespace checks and documentation validation
(123 Markdown files). Helper test and final validation commands:

```sh
python3 scripts/test_worker_trace_collector.py
python3 scripts/test_worker_otelcol.py
python3 scripts/test_formation_http.py
CARGO_NET_OFFLINE=true cargo test --locked --offline -p orishu-membership --test dependencies --target-dir target/formation-flow-observability --quiet
cargo fmt -p orishu-worker -- --check
make docs-check
```

Checkpoint: source-built Linux at dirty-worktree base
`5e459a40e9b3b349116afa0c59e71a5e545e2fd4`, isolated target
`target/formation-flow-observability`, Rust 1.97.1, Linux 6.17.0-41-generic,
no formation fault controls.
This closes the inbound handshake-counter increment, not M4. No full workspace,
fault-feature matrix, release/platform qualification, new overhead measurement
or external Collector mTLS recipe rerun is claimed here. Broader instruments,
logging-output choice/correlation,
ADR 0025 peer propagation, reviewed overhead and full operator deployment gates
remain open.

## Pinned Collector mTLS receiver recipe — 2026-09-07

The [mTLS Collector guide](../testing-worker-otelcol-mtls.md) and
`etc/otelcol-worker-mtls.yml` extend the real-receiver operator recipe without
changing Rust production code, exporter settings, peer compatibility or
membership authority. The overlay adds TLS to the existing loopback OTLP/HTTP
receiver, requires TLS 1.3 and uses a fixed nonempty client-CA filename. It does
not create an unauthenticated fallback listener. Relative certificate paths
resolve from an explicitly private collector working directory; server/client
trust is collector-specific and separate from Orishu credentials.

The complete mTLS target passed nine helper/receipt-validator tests, three
collector startup-refusal cases and seven actual worker/CLI/collector scenarios:

```sh
CARGO_NET_OFFLINE=true make test-worker-otelcol-mtls OTELCOL=/tmp/orishu-otelcol-check.JlHHuV/otelcol WORKER_OTELCOL_TARGET_DIR=target/formation-flow-observability
CARGO_NET_OFFLINE=true make test-worker-otelcol OTELCOL=/tmp/orishu-otelcol-check.JlHHuV/otelcol WORKER_OTELCOL_TARGET_DIR=target/formation-flow-observability
make docs-check
```

The second command reran the original HTTP path after the shared harness
changes; all three runtime modes and outage/recovery passed. Builds ran
sequentially in the idle isolated target. The default new Make target uses
`target/worker-otelcol`, not ordinary `target/debug`.

Evidence from the mTLS run:

- Missing/malformed client-CA material and a server certificate/key mismatch
  fail real collector startup with the relevant diagnostic and no surviving
  listener or span receipt. A missing client CA never degrades to server-only
  TLS. The helper checks actual startup, not only configuration validation.
- Plain HTTP receives 400. A client without a certificate receives the TLS 1.3
  certificate-required alert; a TLS-1.2-only client receives the protocol-version
  alert. Unit negative controls reject a normal HTTP response, an unrelated
  TLS alert or TCP connection refusal as proof of the required access refusal.
- Disabled and zero-sampled workers produce no spans. The enabled secure
  journey delivers three initial roots, retains control/readiness through real
  collector shutdown with two failed exports, then delivers two fresh roots
  after collector-only restart to a new output file. Final delivery counters
  are five accepted and two failed; file inspection and clean shutdown pass.
- Four independently started workers exercise absent client identity, an
  unrelated client issuer, an unrelated server trust root and a validly signed
  server certificate with the wrong name for the worker URL. Each records
  three failed exports, zero accepted/received spans and successful authenticated
  lock/status with unchanged formation identity and successful readiness.
  A separately verified receiver control works before and after each case;
  its name-mismatch control verifies the server's actual SAN without disabling
  hostname verification or changing the worker's endpoint.
- Receipt checks exclude generated certificate/key PEM and continuous base64
  payloads as well as the existing operator-token, label and operation markers.
  OpenSSL creates only private one-day fixture material, removed with the test
  directory; no CA or credential is installed into a host trust store.

The first expanded startup run stopped at the malformed-CA assertion: the
collector failed startup, but the assertion expected a narrower inner parse
diagnostic. The pinned CA loader source identifies the enclosing client-CA
failure category; the assertion now checks that category while retaining
nonzero exit, invalid input, absent listener and absent receipt checks.
Subsequent full mTLS runs and the final Make target passed; this was a harness
diagnostic mismatch, not a collector or worker runtime fix.

Checkpoint: source-built Linux at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, both optional capabilities, ordinary
debug binaries without formation fault controls. Python 3.13 and OpenSSL 3.5.3
were used. The pinned Collector and worker/CLI executable hashes match the
[local-recipe checkpoint](#pinned-local-collector-operator-recipe--2026-09-07).
The existing `proc-macro-error2 2.0.1` future-compatibility warning remains.
Documentation checks pass (123 Markdown files), as do scoped whitespace checks.
No full workspace, formation fault or omitted-feature build matrix ran here.

The guide now states timeout boundaries precisely: the 120-second alarm bounds
the journey, commands have ten seconds, observation deadlines are checked
between attempts, and the one-second socket timeout is not an absolute deadline
for an arbitrarily trickled HTTP exchange. Each managed process has a separate
three-second graceful cleanup budget; cleanup may extend past the alarm.

These results close the scoped receiver-security recipe, not cross-host
networking, certificate rotation/revocation, bearer authorization, hostile TLS
saturation, collector disk/memory pressure or service/container/release
qualification. Cross-peer/log correlation, dashboards and representative
overhead remain open. Neither pending logging output nor ADR 0025 was selected;
combined M4 remains incomplete and formation acceptance is unchanged.

## Pinned local Collector operator recipe — 2026-09-07

The [local Collector walkthrough](../testing-worker-otelcol.md) and
`etc/otelcol-worker-local.yml` now use official `otelcol 0.160.0`, not a test
HTTP responder, to receive real worker OTLP and write private JSONL receipts.
The receiver is trace-only and loopback-only; the example configures request
size/read/write bounds, a memory limiter and rotating diagnostic files.
Collector pressure, rotation/disk failure and remote security are not tested
by this increment. The file exporter remains an alpha diagnostic sink, not
durable provenance or backend acceptance.

The final target passed with seven receipt-validator tests and all three real
worker/CLI/collector runtime-mode journeys:

```sh
CARGO_NET_OFFLINE=true make test-worker-otelcol OTELCOL=/tmp/orishu-otelcol-check.JlHHuV/otelcol WORKER_OTELCOL_TARGET_DIR=target/formation-flow-observability
make docs-check
```

The target validates the real example and requires rejection of an invalid
configuration; malformed protobuf receives HTTP 400. Disabled and zero-sampled
workers have no received spans after clean shutdown. Enabled-mode evidence
contains exactly three initial local roots (summary, refused unauthenticated
lock, unchanged summary), with one rejected adapter outcome. Seeded credentials,
worker labels and operation names are absent. After stopping/reaping the real
collector, authenticated lock and summary preserve formation identity and
readiness while two exports fail. Restarting only the collector with a fresh
file yields two new completed roots for unlock/summary; worker delivery counters
reach five accepted and two failed, without replaying the failed spans. No
scrape/probe creates a client span. The public `--inspect-traces` command
validates the final private file before fixture cleanup.

Byte/count, duplicate-field/ID, invalid identity/context/name/attribute, finite
outcome and partial-final-file negative controls pass. The script enforces a
120-second whole-run limit, ten-second command/observation limits, one-second
HTTP operations, bounded receipt reads and three-second graceful process exits
with kill/reap on timeout. The worker exits before its receiver and its client
socket is removed. Raw process output is not exposed; fixed progress messages
identify the last phase. Generated fixture state/receipts are ephemeral; the
manual guide provides a separate private-file inspection workflow.

Checkpoint: source-built Linux, dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, both optional capabilities, ordinary
debug binaries without formation fault controls. Final executable hashes:

| Executable | SHA-256 |
| --- | --- |
| Official Collector 0.160.0 | `abe338fa33865e54412566db5cea4adc82374594b65b49c1099048cdf8cf2451` |
| Worker | `a45694590c87ee3d2f41b824ec9e47b0164ce9127d52aa61a0e3b1e96f36be8c` |
| CLI | `6352c735efdbeb9108f0d18ba84a968c215679c36b0c7867df2de084494b73aa` |

The archive checksum matched the official published checksum recorded in the
guide. Initial sandbox network/socket attempts were refused and rerun with
approval. The first socket-enabled journey failed because the harness helper
shadowed Python's HTTP module; the import was corrected and cleanup now
preserves the original failure instead of replacing it with a startup-time
termination status. Subsequent complete journeys and the final Make target
passed. The build retains the existing `proc-macro-error2 2.0.1`
future-compatibility warning. Documentation validation passes (122 Markdown
files); scoped whitespace checks pass.

This increment changes test infrastructure, example configuration and manuals,
not Rust production code, dependencies, peer compatibility or domain authority.
The Make target defaults to its own `target/worker-otelcol` directory; this run
explicitly reused the idle isolated directory above. No full workspace,
formation fault matrix or omitted-feature build matrix ran in this increment.
P-OBS-DOCS now has a local collector receipt recipe, while remote collector
security, cross-peer/log correlation, dashboards, release/deployment and
representative overhead gates remain open. N-FORMATION's accepted checkpoint
and the incomplete combined M4 disposition are unchanged.

## Optimized local telemetry overhead baseline — 2026-09-07

An ignored manual Linux benchmark now compares runtime-disabled telemetry,
metrics-only, zero sampling, default 1000-ppm sampling and full sampling using
the same optimized combined-feature worker. Three rotating-order rounds use
real reused Unix HTTP clients, 64 warm-up requests and two-second/100,000-request
bounded cells. Every measured response preserves exact local formation/node
identity, one live member and unlocked state. Enabled metrics are scraped
approximately every 250 ms, interleaved with requests. A bounded local receiver
decodes actual OTLP batches and responds successfully. Worker CPU/RSS and
scrape/request timing windows, collector exclusion and shutdown accounting are
defined in the [measurement guide](../testing-worker-overhead.md).

The [retained fifteen-cell artifact](../measurements/worker-telemetry-2026-09-07.jsonl)
records all observations, not just averages. It was produced with:

```sh
ORISHU_BENCH_REPORT=/tmp/orishu-overhead.SB0ECE/approved-measurements.jsonl cargo test --locked --offline --release -p orishu-worker --features observability,otlp-tracing --test standalone measure_local_telemetry_overhead --target-dir target/formation-flow-observability -- --ignored --nocapture --test-threads=1
```

The measurement passed in 31.24 seconds at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, using Rust 1.97.1, Linux
6.17.0-41-generic, Intel i7-1370P/20 logical CPUs and 100 process ticks/second.
The optimized worker hash is
`7492acecc4eaed9f22934ee1d78c0ac9c56091c41577ed2c79d730716f4861c8`;
the JSONL artifact hash is
`5e57a2b9390d306e0eb31f868e996df845ae8d20073e534125360a87005ed0fb`.
No other build/test was launched during the measurement; host load/frequency
scaling were not controlled. All modes used both compiled capabilities, no
formation fault feature, and fresh private runtime state. Feature-omitted
binary cost was not measured.

Default-sampling median request latency increased 8.9–10.0% within rounds;
throughput fell 8.9–10.9%. Full sampling reduced throughput 21.5–22.6%. The
guide reports the full per-mode summaries and noise limitations. No reviewed
overhead threshold exists, so passing benchmark assertions are measurement
validity, not performance acceptance. The retained run reported no telemetry
loss; an earlier passing run's truncated output showed 128 shutdown-abandoned
records in one full-sampling cell. That earlier run is explicitly preserved as
incomplete capture, not erased or represented by the later zero-loss result.
The initial create-new artifact attempt failed on sandbox socket permissions;
the approved retry used a different filename and left the empty artifact intact.

No Rust production, dependency, peer profile or persisted domain format changed.
The new code is isolated manual measurement infrastructure. Wider three-worker,
concurrent/remote-collector, exporter-specific allocation and omitted-feature
measurements, a reviewed budget, trace/log correlation and peer propagation
remain open. This does not close M4 or alter N-FORMATION's accepted checkpoint.

Final scoped validation also passed:

```sh
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --test standalone telemetry_overhead --target-dir target/formation-flow-observability --quiet
cargo fmt -p orishu-worker -- --check
make docs-check
```

Initial Clippy rejected the constant-valued debug-build assertion. It was
replaced with a compile-conditional startup guard and an explicit expected-panic
test; measured runtime behavior and the worker executable are unchanged. The
normal test selection passes that guard test and intentionally ignores the
manual measurement (one passed, one ignored); the optimized execution above
is the measurement evidence. Formatting, scoped whitespace and documentation
checks pass (120 Markdown files). No full workspace or formation suite ran here.

## Optional trace-loss alert examples — 2026-09-07

Three optional rules now distinguish local shedding (active/queue pressure,
encoding and local source failures), failed delivery with uncertain remote
acceptance, and explicit collector rejection. They require `up=1`, a positive
five-minute per-counter rate and a two-minute pending interval, evaluated once
per minute. These are sensitive example warnings about recent loss, not
measured SLOs or automatic restart/removal policies. The operator guide records
safe first checks and the possibility that a single burst remains visible
until the lookback window clears.

`prometheus-worker-trace-alerts.test.yml` covers all four local-loss counter
families, delivery failures, collector rejections, pending/firing/recovery,
idle/zero counters, increasing intentional sampling/shutdown/closure/warning
counts, absent tracing, counter resets without new losses, unavailable scrapes
and deleted/stale targets. No missing series is replaced with zero. The
extended 52-series harness checks the optional rule file and its fixtures,
loads both groups into real Prometheus and verifies six rule names/types and
no reported rule errors. The base forty-series path retains only its original
three rules. Rule firing uses synthetic time; the short process journey is
loading/ingestion evidence, not two-minute fault-driven notification evidence.

At dirty-worktree base `42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, these passed
with the previously built combined-feature worker and pinned 3.5.0 tools:

```sh
/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool check rules etc/prometheus-worker-trace-alerts.yml
/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool test rules etc/prometheus-worker-trace-alerts.test.yml
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 -m py_compile scripts/check-worker-prometheus.py
make docs-check
```

Executable SHA-256 values are worker
`5c64caeef7d6b9ad768bb3ab606cdaebde632f8042913572a0c2e2d85a3c18d7`
and CLI `e1c8f0136e09abe0d40dac5d0170775726c3808261ab2e980ba0d49cc37963f4`.
Process journeys ran sequentially with approved local-socket permissions;
no feature-changing build ran during them. Documentation and scoped whitespace
checks pass. No Rust production, dependency, peer profile or persisted format
changed, and no workspace-wide or three-worker formation suite was rerun.

Trace-correlated logging remains open pending an operator-output choice; code
inspection found no configured structured logging sink to extend. The
observability task records the required bounded writer/shutdown and correlation
evidence. Peer propagation separately awaits ADR 0025 review. These decisions,
dashboards, notification delivery, collector deployment, overhead and remaining
M4 acceptance are not closed by the new warning examples.

## Prometheus trace-counter ingestion and collector recovery — 2026-09-07

The existing scraper harness now accepts `--trace-metrics`, starts an explicitly
enabled tracing worker and a bounded loopback HTTP responder, and queries all
52 series through Prometheus 3.5.0. Its initial HTTP 503 responses must produce
ingested positive failed-span counts while accepted spans remain zero. Real
authenticated lock and formation inspection succeed and readiness stays true.
After the responder switches to HTTP 200, an authenticated unlock produces new
telemetry and the backend must ingest a positive accepted count. Earlier TSDB
samples cannot satisfy this gate. The existing scraper-stop, authenticated
control and fresh post-restart transition-count checks run afterward too.

The extension preserves the base catalogue and verifies all sample names,
finite nonnegative values, target-only labels (plus the fixed histogram
buckets), secret-free exposition, a 16 KiB combined scrape bound and the three
existing alert examples. The default harness still checks forty series within
8 KiB with tracing disabled; no global catalogue expectations were replaced.

`worker_trace_collector.py` is a fixture, not an OTLP backend implementation.
It handles at most 64 serial requests with 4 KiB headers, 1024-byte bodies,
one-second input deadlines and two-second thread cleanup. It validates HTTP
framing and returns a valid empty protobuf response without interpreting span
content. Existing Rust receiver tests remain that separate content evidence.
Three fixture tests cover failure/recovery, exact/over-limit body sizes and
silent-input expiry. `test-worker-trace-prometheus` exposes the combined check
through Make; the documented direct script path supports isolated build targets.

At dirty-worktree base `42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, these passed:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/test_worker_trace_collector.py
python3 scripts/check-worker-prometheus.py --trace-metrics --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
python3 -m py_compile scripts/check-worker-prometheus.py scripts/worker_trace_collector.py scripts/test_worker_trace_collector.py
make docs-check
```

The three initial sandboxed fixture tests failed with socket-creation
`PermissionError`; approved local-socket execution passed. Both real-server
journeys passed with pinned 3.5.0 tools. Builds and journeys were sequential in
the shared target, whose final worker has both features and no fault hooks.
The build retained the existing `proc-macro-error2` future-compatibility warning.
The new Make target was inspected with `make -n`; its isolated-target equivalent
above was executed rather than replacing an unrelated `target/debug` binary.

No Rust production behavior, dependency, protocol, authority or persisted format
changed. Documentation links and scoped whitespace checks pass. This closes the
previous live-counter entry's missing local Prometheus-ingestion evidence, not
trace-loss dashboards/alerts, trace/log correlation, peer propagation, collector
deployment, overhead or the full M4 handoff. No workspace-wide suite or
three-worker formation journey was rerun in this harness-only increment.

## Live trace delivery and loss metrics — 2026-09-07

The exporter now publishes its six delivery counters to an independently
readable atomic snapshot. The diagnostics adapter combines it with the six
existing queue counters, adding twelve unlabelled `orishu_worker_trace_*_total`
series only when tracing and the metrics surface are enabled. Reading them
does not wait for exporter IO, drain records, drive membership or create a
client span. Values are independent observations, not transactional receipts;
shutdown reporting remains necessary because a final scrape is not guaranteed.

`maximum_trace_catalogue_is_finite_and_bounded` verifies all twelve distinct
counter names at `u64::MAX`, their types and a maximum extension below 4 KiB.
Existing base-catalogue tests retain the below-8-KiB bound; combined consumers
now allow 16 KiB. The real-worker saturation fixture scrapes 52 series while
the first export is held (enqueued 2, queue-full 4, accepted 0) and after two
responses and a fresh operation (enqueued 3, queue-full 4, accepted 2).
Authenticated lock/unlock, unchanged formation identity, exact shutdown losses
and the held-socket shutdown deadline remain asserted. Both live responses
and maximum-value exposition pass `promtool check metrics` when explicitly
selected by `ORISHU_TEST_PROMTOOL`.

The local-operation process fixture additionally checks that runtime-disabled
tracing omits the twelve series and enabled zero sampling exposes real sampling
counts without queue admission or collector connection. Export-loop tests
check live/final snapshot equality for success, failure, partial rejection and
shutdown abandonment; a separate test preserves field mapping and saturation.

Validation at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, using Rust 1.97.1 and the installed
promtool 3.5.0, passed:

```sh
ORISHU_TEST_PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features otlp-tracing --test standalone tracing_ --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features observability --all-targets tracing_ --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --test standalone tracing_ --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo fmt -p orishu-worker -- --check
make docs-check
```

The final combined run passed 232 worker tests (144 library, 43 binary, 45
integration), plus the environment-isolated HTTP subprocess case. The focused
tracing-only selection passed six integration tests; metrics-only and default
selections each passed four integration tests. The metrics-only filter matched
no library/binary tests, so it does not establish those full suites. An initial
sandboxed combined run failed with 80 library tests passing and 64 failing;
the approved socket/system-trust rerun and final combined rerun passed. Feature
builds were sequential in the shared target; final executables contain both
telemetry features and no formation fault hooks.

Combined-feature all-target worker Clippy with warnings denied, worker
formatting, scoped whitespace and documentation checks passed (119 Markdown
files).

The task, roadmap/index, configuration, worker manual and worker-operator story
now distinguish existing local telemetry from remaining distributed/operator
work. The test guide records how to opt into parser checks; absent that setting,
ordinary Rust tests do not establish external parser validation. Full Prometheus
ingestion of the twelve added series, trace-loss alerts/dashboards, log
correlation, peer propagation, collector deployment and measured overhead remain
open. No dependency, peer profile, core authority or persisted format changed.
No workspace-wide suite, three-worker formation journey or benchmark ran in
this increment; N-FORMATION's accepted checkpoint is unchanged and M4 stays open.

## Bounded default Linux collector trust — 2026-09-07

Enabled HTTPS export without an explicit CA file now loads the supported Linux
`/etc/ssl/certs/ca-certificates.crt` bundle. Explicit CA input still replaces
default trust and never falls back on error. The default loader checks a 1 MiB
file limit, 1024-certificate count limit, root ownership of traversed directories
and file, regular/single-link file type, non-symlink traversal and write
permissions. Empty/malformed/unsafe/missing/oversized input fails startup.
No directory scanning, SSL_CERT_FILE/SSL_CERT_DIR overrides, ambient SDK trust
settings or new dependency are used. The existing cached native-root loader
consults environment paths and does not provide these read/count bounds, so it
was not adopted for this path.

The [configuration guide](../orishu-configuration.md#implemented-local-tracing-configuration)
documents the Debian/Ubuntu bundle layout, its authoritative tooling reference,
startup-only loading and unsupported native stores. Other layouts require
explicit CA input; Windows/macOS native integration is not claimed. The Linux
integration fixture requires the installed system bundle and never changes
host trust to install test certificates.

The actual-worker credential matrix now has a ninth Linux case: no explicit
CA file, with ambient SSL_CERT_FILE/SSL_CERT_DIR pointing at a generated fixture
CA. Startup successfully loads system trust, rejects that otherwise-valid fixture
TLS identity, continues serving unchanged formation/node identity and member
count, and exits gracefully. Existing explicit-root positive/negative cases
run with the same hostile environment and still behave as expected. A new
unit test covers exact/over-limit bundle byte and certificate counts and invalid
contents; ownership checks also refuse non-root-owned system-source inputs.
This establishes loading/isolation/rejection, not a live publicly trusted
collector or every distribution's trust-store behavior.

Validation at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667` passed:

```sh
cargo test --locked --offline -p orishu-worker --test standalone --features otlp-tracing tracing_files --target-dir target/formation-flow-observability -- --nocapture
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo fmt -p orishu-worker -- --check
make docs-check
```

The combined suite passed 230 worker tests (143 library, 42 binary, 45
integration), plus the environment-isolated HTTP subprocess case. Socket and
system-store checks ran with approved permissions; shared-target builds were
sequential and final test executables have both telemetry features and no fault
hooks. Combined-feature all-target worker Clippy with warnings denied also
passed. Formatting, scoped whitespace and docs checks passed (119 Markdown files).
Live loss diagnostics, log correlation, peer propagation, operator collector
recipes and overhead remain incomplete; additional native-store support is
outside the tested Linux layout. No workspace-wide suite, formation journey
or overhead benchmark was run in this increment.

## Real-worker trace saturation, recovery and shutdown — 2026-09-07

Graceful tracing shutdown now reports the existing queue sampling/drop counters
alongside delivery aggregates. The queue handle remains available until the
export loop has joined; shutdown still explicitly closes its consumer and does
not wait for all sender handles to disappear. This adds one fixed-size shutdown
diagnostic, not live metrics or an additional membership/health authority.

`tracing_pressure::tracing_saturation_preserves_mutations_recovers_and_bounds_shutdown`
starts the actual worker with full sampling, batch/queue capacities of one,
1024-byte export requests, a ten-second attempt timeout and a 100 ms exporter
shutdown budget. The receiver holds the first complete request unanswered,
then a summary fills the queue. Two authenticated lock/unlock requests and their
summary checks finish within three seconds, before export timeout, and preserve
formation identity with the requested lock state. Only then does the fixture
release the first and queued requests, receive both and verify a fresh summary
can produce another export. That third request remains unanswered until after
the process terminates through the supported signal path.

Exact final counts are two collector acceptances, four queue-full drops, three
enqueued records and one shutdown abandonment, with no failed attempt. This
establishes actual queue saturation, capacity recovery and exporter deadline
precedence; collector cleanup does not manufacture successful shutdown.
The process guard kills/reaps on failure, the scenario has a 15-second outer
bound, stderr is bounded to 8192 bytes and excludes the operator token, and
ordinary worker termination retains its three-second process bound. The three-
second control check is fixture evidence, not a production latency SLO.

Focused tracing-only validation passed on the first run at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667`:

```sh
cargo test --locked --offline -p orishu-worker --test standalone --features otlp-tracing tracing_saturation --target-dir target/formation-flow-observability -- --nocapture
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo fmt -p orishu-worker -- --check
make docs-check
```

Clippy, formatting, scoped whitespace and docs checks passed (119 Markdown
files). The combined-feature suite also passed 229 worker tests (142 library,
42 binary, 45 integration), plus the environment-isolated HTTP subprocess:

```sh
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
```

Builds were sequential in the shared target; final test executables have both
telemetry features and no fault hooks. Socket/process tests used approved
permissions. This does not prove arbitrary simultaneous peer/client pressure,
scientific equivalence, or live diagnostic availability. Platform roots,
trace/log correlation, live loss diagnostics, peer propagation, operator
collector recipes and overhead remain open. No dependency or wire profile
changed; no workspace-wide suite, formation journey or benchmark ran here.

## Real-worker collector credential and mTLS matrix — 2026-09-07

`tracing_security::tracing_files_fail_before_startup_and_valid_files_deliver_over_mtls`
adds eight cases through the actual worker executable and a local TLS receiver:

- Invalid CA PEM, mismatched client key, group/other-readable token and token
  symlink each exit 2 before creating worker state/socket or connecting to the
  collector. Stderr diagnostics are bounded and exclude seeded secrets and paths.
- Wrong collector trust, wrong server name and unrelated client certificate
  fail the real TLS handshake. The worker still returns its original formation
  ID, node ID and member count and terminates gracefully. These valid startup
  configurations fail at collector authentication, not as worker startup errors.
- The valid control loads all four configured files, proves the exact client
  certificate at the collector, sends the collector-only bearer header and
  delivers a decoded local request span with no seeded secret material.
  Collector loss afterward preserves status and graceful shutdown.

Each worker has private temporary credentials/socket directories. The matrix
has a 20-second outer bound, startup failures have two-second process bounds,
collector attempts use one second and exporter shutdown uses 100 ms. Fixture
worker guards kill/reap children on failure; no fault hooks or external PKI
executables are involved. Certificate trust anchors are generated fixture
identities, not a platform trust-store or real deployment certification.

At dirty-worktree base `42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, focused
tracing-only runs passed the initial five and extended seven cases; the final
combined-feature suite passed all eight, within 228 worker tests (142 library,
42 binary, 44 integration), plus the isolated HTTP subprocess case:

```sh
cargo test --locked --offline -p orishu-worker --test standalone --features otlp-tracing tracing_files --target-dir target/formation-flow-observability -- --nocapture
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo fmt -p orishu-worker -- --check
make docs-check
```

Clippy, formatting, scoped whitespace and docs checks passed (119 Markdown
files). Shared-target builds were sequential; the final test executables have
both telemetry features and no fault hooks. Socket/process tests used approved
permissions. No workspace-wide suite, formation journey or benchmark ran here.

No production behavior, dependency, protocol or authority changed. These cases
extend the existing primitive TLS tests to startup and real operation delivery;
they do not close process-level saturation, platform roots, log correlation,
live drop diagnostics, peer tracing, operator recipes or overhead acceptance.

## Trace sampling and middleware independence — 2026-09-07

The actual-worker local receiver test now also enables tracing with zero
sampling. It serves real summary requests without a collector connection and
terminates with zero delivery counts. Disabled, zero-sampled and fully sampled
workers preserve the tested formation summary and graceful shutdown behavior.
A new omitted-capability test supplies a valid loopback endpoint: default
builds refuse enablement with exit 2 before creating state/socket or connecting,
with a bounded diagnostic that does not echo the endpoint. This distinguishes
capability refusal from the earlier missing-endpoint cases.

Two middleware tests exercise the production `TraceRequests` handler directly.
Completed HTTP 200/202, rejected 401 and failed 503 retain their original status
and response header. Full completed queues, exhausted active slots, a closed
consumer and zero sampling still invoke the underlying handler exactly once.
Aborting a held handler future records cancellation and releases active capacity.
These are handler/queue tests, not a process-level overload or distributed
operation-equivalence claim.

At dirty-worktree base `42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, the default
`standalone tracing_` selection passed four cases. The combined-feature worker
suite passed 227 tests (142 library, 42 binary, 43 integration), plus the
environment-isolated HTTP subprocess case. Combined-feature all-target Clippy
with warnings denied also passed:

```sh
cargo test --locked --offline -p orishu-worker --test standalone tracing_ --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
```

The same startup selection also passed four cases with `observability` only
and four with `otlp-tracing` only:

```sh
cargo test --locked --offline -p orishu-worker --test standalone --features observability tracing_ --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --test standalone --features otlp-tracing tracing_ --target-dir target/formation-flow-observability
```

This establishes the scoped tracing-startup behavior in all four feature
combinations, not the entire M4 feature/security matrix. Builds ran sequentially
with approved socket permissions; the final target executable has tracing only
and no fault hooks. Worker formatting, scoped whitespace and documentation
checks passed (119 Markdown files).

No production behavior, dependency, wire profile or numerical contract changed.
Process TLS/security, process-level saturation, platform roots, log correlation,
live drop diagnostics, peer spans, operator recipes and overhead remain open.
No full-workspace suite, formation journey or benchmark was rerun here.

## Local worker trace activation and real operation receipt — 2026-09-07

The `otlp-tracing` build now accepts enabled local export with an explicit
endpoint. Typed startup configuration prepares credentials/transport and the
bounded queue/codec/export loop before worker state or listener creation.
Disabled tracing constructs none of these and does no credential/collector IO;
builds omitting the capability still reject enablement. HTTPS currently requires
an explicit CA bundle. No implicit platform-root policy was substituted.

Production client services attach `TraceRequests` independently of Prometheus
metrics. It creates locally sampled fixed-name root spans and maps completed
HTTP service responses to finite diagnostic outcomes; drop retains the queue's
cancellation behavior. It reads no request payload/header/path for export and
does not consume incoming context. Handler completion is not domain acceptance,
socket delivery or a completed asynchronous join. The diagnostics service has
no tracing middleware. Main owns the export task and, after client shutdown,
signals and joins its bounded drain, reporting fixed aggregate delivery counts.

`tracing_worker_operations_reach_collector_only_when_enabled` runs the actual
worker executable with a private Unix socket and a local HTTP OTLP receiver.
It submits the existing public cluster-summary client call and decodes a real
single client-service span with nonzero identity and no parent or seeded worker
label. Runtime-disabled mode makes no collector connection. After collector
closure, another summary retains formation identity/member count and the worker
terminates gracefully. The enabled run reported one accepted span and one
failed attempt; collector failure did not terminate the worker. The fixture's
initial run failed the existing Unix socket parent-permission check because
its temporary directory was group-writable; explicitly provisioning mode 0700
fixed the fixture without weakening production security.

Validation at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667` passed:

```sh
cargo test --locked --offline -p orishu-worker --test standalone --features otlp-tracing tracing_worker_operations --target-dir target/formation-flow-observability -- --nocapture
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --test standalone tracing_ --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --bin orishu-worker config::tracing --target-dir target/formation-flow-observability
```

The combined suite passed 225 tests (140 library, 42 binary, 43 integration),
plus the isolated HTTP subprocess case. The default build passed three tracing
startup/precedence integration tests and five configuration unit tests. Local
network tests used approved socket permissions. Shared-target builds were
sequential; the final build is default/no tracing, despite the target name.
Worker formatting, scoped whitespace checks and `make docs-check` passed
(119 Markdown files). The old configuration fragment is retained as an explicit
anchor while its heading and operator text now describe local export accurately.

This is local client-service export, not distributed tracing acceptance. Real
process TLS/credential failure and sampling/overload matrices, platform roots,
trace-correlated logs and live drop diagnostics, broader operation spans,
ADR 0025 peer propagation, collector recipes and overhead remain open. Primitive
tests still support their narrower boundaries; no workspace suite, formation
journey, platform certification or overhead benchmark was rerun here.

## Bounded export-loop batching and shutdown — 2026-09-07

The optional `ExportLoop` now owns the existing bounded record receiver,
`BatchLimits` codec and single-flight `HttpDelivery`. A reusable record buffer
holds at most the configured batch count (maximum 256), separate from the
bounded input queue and byte-capped encoded request. The first buffered record
starts the flush interval; further arrivals do not postpone it. Full or expired
batches split into byte-fitting requests without changing record order.
Prefix removal shifts at most 256 records per request; no unbounded history or
retry buffer is retained.

Delivery failures shed the attempted records and allow later batches to
proceed. Valid partial-success responses update accepted/rejected/warning
counts; encoding failures shed a record instead of permanently stalling the
queue. These final saturating aggregates are not live exported metrics yet.
Failed/cancelled requests may have reached the collector; counters are not
exactly-once delivery receipts or domain authority.

Shutdown signal or lifecycle-sender loss closes receiver admission and cancels
the in-flight attempt without retry. Remaining buffered/queued records drain
under one absolute shutdown deadline, including subsequent HTTP attempts.
Expiry counts and drops remaining records. Active guards completing after
closure shed their spans through the queue's existing closed-consumer path;
they cannot extend the drain. The runtime still needs to own/spawn/join this
loop and supply its shutdown signal; no detached production task is introduced.

Four new real HTTP receiver tests pass: count-two and timer-driven partial
flush while producers remain alive; 503 failure followed by successful delivery;
shutdown-sender loss draining 64 records through 1024-byte requests with exact
partial-rejection accounting; and explicit shutdown with an in-flight stalled
request, a full queue and a late active span. The stalled case verifies a 100 ms
total drain budget despite two-second attempt limits, four abandoned spans,
queue overflow and closed-producer accounting. It holds collector sockets until
after the exporter terminates, then aborts and joins the fixture task.

Focused validation at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667`:

```sh
cargo test --locked --offline -p orishu-worker --lib --features otlp-tracing trace_export::worker --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo fmt -p orishu-worker -- --check
make docs-check
```

All four focused cases, Clippy, worker formatting, scoped whitespace and docs
checks passed (119 Markdown files). The subsequent combined-feature command
also passed 224 worker tests (140 library, 42 binary, 42 integration), plus the
isolated HTTP subprocess case:

```sh
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
```

Socket fixtures ran with approved permissions. Shared-target builds were
sequential; final test executables include both telemetry features and no fault
hooks. This increment adds no dependency or wire
change. Startup activation, platform roots, actual operator instrumentation,
live trace/log/loss diagnostics, cross-peer propagation and operator collector
recipes remain open. Synthetic local receipt does not satisfy M4's required
production client-to-peer trace. No full-workspace suite, formation process
journey or overhead benchmark was run for this increment.

## Bounded collector credential-file loading — 2026-09-07

The optional Unix `CollectorFiles::prepare` adapter now constructs HTTP delivery
from explicit collector files without connecting, creating or modifying files.
It does not reuse worker, operator or monitoring credentials. HTTPS currently
requires an explicit CA bundle; platform-root loading remains an open startup
requirement. Plaintext loopback refuses any supplied credential/trust files.

The [configuration contract](../orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable)
records fixed file sizes, certificate counts, token grammar and path rules.
Descriptor-relative traversal rejects symlinks and parent traversal. Root/user
ownership, directory write protection and private key/token permissions are
checked on opened descriptors. Public CA/chain files may be readable but not
group/other writable. Regular-file/single-link checks and nonblocking open
reject FIFOs without a writer; a fixed allocation with one sentinel byte also
checks growth after metadata inspection. Errors omit paths and contents.

Two new tests cover file byte boundaries, permissions, hard/symbolic links,
unsafe ancestors, directories/FIFOs, malformed PEM, certificate count limits,
wrong/multiple keys and invalid/oversized tokens. The existing successful mTLS
fixture now loads CA, client chain/key and token files through this adapter
before the real HTTP exchange; negative trust/name/client cases remain intact.
The initial tests failed because temporary directories inherited mode 0775.
An ownership/mode diagnostic confirmed that exact refusal; fixtures now
explicitly provision 0700 directories. Production permission checks were not
weakened and temporary diagnostics were removed.

At dirty-worktree base `42d2b2ce10ecd36c03a87da89dcb5de8446c1667`, the focused
tracing suite passed 16 tests and the combined-feature worker suite passed 220
tests (136 library, 42 binary, 42 integration), plus the environment-isolated
HTTP subprocess case. Local-socket tests ran with approved permissions.

```sh
cargo test --locked --offline -p orishu-worker --lib --features otlp-tracing trace_export --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
```

Combined-feature all-target worker Clippy with warnings denied, worker format
checking, scoped whitespace checks and `make docs-check` also passed (119
Markdown files).

Builds were sequential in the shared target; the combined test executable has
both optional features and no fault hooks. This increment adds no dependency.
It does not activate tracing: startup still refuses enabled export, and disabled
startup does not read collector files. Platform roots, startup integration,
actual operation instrumentation, export/shutdown lifecycle, cross-peer traces
and operator collector recipes remain incomplete. No full-workspace or formation
process suite, platform port or overhead benchmark was run here.

## Bounded OTLP HTTP delivery and TLS fixtures — 2026-09-07

The staged `HttpDelivery` primitive sends a codec-bounded batch over HTTP/1.1
to an explicitly validated destination. Startup and delivery share the same
redacted endpoint validator. The client disables proxies, redirects, automatic
retries and decompression. A whole-attempt deadline covers connection, response
headers and streamed body; declared and streamed body sizes are both bounded.
Only protobuf HTTP 200 responses are decoded. Partial-success rejection counts
must fit the submitted batch; collector warning text is not retained. Empty
batches perform no IO. This is an adapter, not a running export queue consumer.

Five HTTP-module tests now cover synthetic-record receipt, redirects, declared
and chunked exact/over-limit responses, timeout, malformed protobuf, compressed
response refusal, partial-success bounds and configuration refusal. Real TLS
fixtures use independently generated collector/client authorities: valid mTLS
and a collector-only bearer header succeed; wrong server roots/name and absent
or unrelated client credentials fail. They use ordinary rustls verification,
not formation's peer verifier. A separately bounded subprocess reruns HTTP
delivery with hostile proxy and OTEL destination/header variables; requests
still reach the explicit fixture without injected credentials or headers.

Validation at dirty-worktree base
`42d2b2ce10ecd36c03a87da89dcb5de8446c1667` passed:

```sh
cargo test --locked --offline -p orishu-worker --lib --features otlp-tracing trace_export::http --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --bin orishu-worker config::tracing --target-dir target/formation-flow-observability
cargo fmt -p orishu-worker -- --check
make docs-check
```

The focused run initially covered the three HTTP tests; the subsequent TLS run
covered four. The final combined suite covered all five HTTP-module tests and
passed 218 worker tests (134 library, 42 binary, 42 integration), plus the
isolated subprocess's repeated HTTP case. A final tracing-only run also passed
all five HTTP-module tests and its subprocess case. Its first sandboxed attempt
failed at loopback bind with `PermissionDenied`; rerunning the exact command
with approved socket permissions passed, without code or timeout changes.
The no-optional-feature configuration run passed all five tracing tests after
the shared validator move. Worker formatting, scoped whitespace and docs checks
passed (119 Markdown files). The last focused executable includes `otlp-tracing`
only, with no fault hooks; the target name does not identify its feature set.
The first Clippy run caught the new
subprocess test's missing Tokio `process` feature; adding that dev-only feature
resolved it, and the exact Clippy command then passed. `tokio-rustls = 0.26.4`
is a pinned test dependency already present transitively; no new package was
downloaded. Builds sharing the target directory were sequential.

These fixtures supply TLS configuration in memory and export synthetic spans.
They do not prove safe credential-file loading, startup trust-root selection,
actual operator instrumentation, export-loop batching/shutdown, production
collector compatibility or cross-peer propagation. Runtime activation remains
explicitly unavailable and ADR 0025 remains proposed. No formation wire change,
full-workspace validation, formation process rerun or overhead measurement was
performed in this increment. Earlier formation acceptance is unchanged.

## Bounded trace sampling and queue lifecycle — 2026-09-07

The optional `trace_export::SpanQueue` now creates local sampled root spans,
reserves active capacity before retaining a record and uses a separately bounded
completed-record channel. Zero sampling avoids clock and entropy acquisition.
Other sampling uses the existing worker crypto provider, with the configured
rate rounded down to a 32-bit probability bucket. No caller flag can override
that local policy; peer context is not yet accepted or emitted.

`ActiveSpan::finish` publishes once with the adapter outcome. Dropping an active
guard publishes cancellation best-effort and releases its permit. Publication
uses `try_send`, never waits for the receiver, and distinguishes full queue from
closed consumer. Invalid entropy-derived identity or timestamp acquisition
also sheds the span and releases its slot. End timestamps use monotonic elapsed
time added to the captured Unix start time, preventing wall-clock rollback from
reversing the interval. Fixed saturating counters distinguish sampling, active
exhaustion, queue overflow, closure, invalid source and successful enqueue;
none is a delivery receipt or an exported metric yet.

Five new tests cover sampling/limit boundaries, active/queue/closed-consumer
capacity recovery, cancellation of an actual Tokio task followed by protobuf
encoding, invalid-identity release/counter saturation and concurrent producers.
The concurrency test holds sixteen producers at a barrier against four active
slots and two completed slots, verifies exactly twelve active refusals and two
queue drops, then proves reuse. The barrier/join phase has a three-second bound.

Focused `trace_export` tests passed all nine codec/lifecycle cases. Combined
`observability,otlp-tracing` Clippy passed with warnings denied. No dependency,
peer profile, client route or runtime startup behavior changed. Worker requests
do not instantiate this queue yet: HTTP delivery, safe credential loading,
export/shutdown lifecycle and real collector receipt remain required before
lifting `tracing.enabled=true` refusal. This is progress on the M4 exporter,
not a substitute for its end-to-end acceptance.

Validation passed:

```sh
cargo test --locked --offline -p orishu-worker --lib --features otlp-tracing trace_export --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo fmt -p orishu-worker -- --check
make docs-check
```

The combined-feature suite passed 213 tests (129 library, 42 binary, 42
integration), including real executable startup refusal. Builds were sequential
in the shared target; the final test binary includes both telemetry features
and no fault hooks. Docs validation passed 119 Markdown files and scoped
whitespace checks passed. No full-workspace suite, independent formation
journey, external collector or overhead benchmark was run in this increment.

## Bounded OTLP protobuf codec — 2026-09-07

`orishu_worker::trace_export` now provides a fixed-size completed-span record
and a count/byte-bounded batch encoder behind the staged `otlp-tracing` feature.
The record validates nonzero trace/span/parent IDs, rejects self-parenting and
checks ordered Unix-nanosecond timestamps before allocating. Operation and
outcome names are finite enums; arbitrary payloads, attributes, events, links,
baggage and credentials cannot enter this record shape. This initial shape
does not claim multi-cause peer causality or propagation.

The encoder uses pinned `opentelemetry-proto = 0.32.0` generated trace messages
and `prost = 0.14.3`, without default OTLP/gRPC/log/metric exporter features.
Their transitive SDK dependency is optional with the worker feature too; no
exporter is constructed and no ambient OTEL settings are read. Generated
message allocations are bounded by fixed record vocabulary and at most 256
selected spans. In O(selected spans) work, nested length-prefix sizes select
the maximal count/byte-fitting prefix. Output size is checked against the
maintained message's `encoded_len` before allocating a fixed slice for encoding.
There is no growable output writer, and a too-large first record is an explicit
error for the future queue consumer to shed. Input tails are not silently lost.

Tests cover invalid identities/time, empty input, count limits, exact byte
acceptance and one-byte-smaller splitting, order/identity/timestamp-preserving
round trips, 256-span nested varint boundaries and oversize refusal. Initial
compilation found the generated `KeyValue.key_strindex` field; defaulting that
optional schema field corrected the initializer. The offline dependency attempt
failed because optional crates were not cached; the permitted locked download
and subsequent offline runs succeeded. Existing unrelated lockfile/toolchain
changes were preserved; this increment used the selected Rust 1.97 toolchain.

Passed checks:

```sh
cargo test --locked --offline -p orishu-worker --features otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --features observability,otlp-tracing --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,otlp-tracing --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
cargo tree --locked --offline -p orishu-worker --no-default-features --edges normal
cargo fmt -p orishu-worker -- --check
make docs-check
```

The OTLP-only worker suite passed 195 tests (124 library, 37 binary, 34
integration); both telemetry features together passed 208 tests (124 library,
42 binary, 42 integration). All three membership dependency checks passed.
Builds ran sequentially in the shared target; its final worker test binary
includes both telemetry features and no fault hooks. Documentation validation
passed 119 Markdown files, and scoped whitespace checks passed. The default
worker runtime dependency tree contains neither OpenTelemetry nor Prost 0.14.
This is an implemented codec, not runtime trace export: actual operation
instrumentation, sampling, active/queue bounds, credential loading, secure HTTP
delivery, loss/shutdown handling and collector receipt remain unfinished.
`tracing.enabled=true` still fails before state/listener creation in every
build. Peer profile 4 is unchanged and ADR 0025 remains proposed. Formation
acceptance remains at the preceding checkpoint; no formation process harness
or full workspace Rust suite was rerun by this codec increment.

## Final process validation batch — 2026-09-07

The following remaining acceptance commands passed against the dirty worktree
based on `9d71b752902a3aaeca376968d8f03424a2361b20`:

```sh
make test-formation-client-pressure test-formation-policy-partition test-formation-lost-leave-response
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --target-dir target/formation-faults --quiet
make test-formation-lost-ack test-formation-peer-ejection test-formation-lost-departure
```

The first three Make targets ran sequentially using ordinary, telemetry-disabled binaries
in `target/debug`. Their executable and lockfile hashes match the preceding
[default checkpoint](#default-validation-checkpoint). Each process harness
privately copies its executables before starting workers, including for
restarts. Local socket permission was granted; no transport mock or core-only
substitute was used for these process assertions.

| Final rerun | Result and acceptance boundary |
| --- | --- |
| Client pressure | **passed**: saturated authenticated mutation bodies, independent reads and peer policy progress, release/reuse, partial headers, full formation churn and shutdown with unfinished clients |
| Policy partition/heal | **passed**: real opaque-UDP peer isolation, independently available operator sockets, conflicting local policy versions, exact winning-version convergence after healing, then full churn |
| Lost leave response | **passed**: discarded actual accepted response, exact authenticated CLI receipt replay without a second identity change, then full churn |
| Fault-feature worker suite | **passed**, 190 tests: 120 library, 37 binary, 33 integration; `formation-fault-test` only, no observability, isolated `target/formation-faults` |
| Lost JoinAck | **passed**: deliberate loss after remote insertion recovered the original assigned identity, followed by the full handoff/leave/crash/restart/readmission journey |
| Peer ejection | **passed**: joined worker received its tombstone over authenticated peer gossip, stopped participation, retained its identity while ejected and required explicit leave to become a fresh standalone formation |
| Lost departure | **passed**: both actual departure sends were suppressed; survivor SWIM detected departure, followed by same-certificate readmission and full churn |
| Issuer loss | **passed** in the following history-loss batch: original issuer loss retained uncertainty through retry exhaustion; subsequent source crash/restart lost operation history without restoring the accepted assignment and required the documented operator stop |
| Issuer ejection | **passed**: wire-ejected original issuer lost accepted history while retaining its formation/node identity; the original source preserved uncertainty and reached the bounded operator-stop result |
| Source loss | **passed**: source crash lost operation history while the surviving issuer retained the original assignment; read-only inspection did not authorize readmission |
| Dead assignment | **passed**: replay refused the dead accepted assignment without revival or duplicate identity; the original source exhausted retries and reached the bounded operator-stop outcome |
| Removed assignment | **passed**: replay refused the removed accepted assignment without readmission; the original source exhausted retries and required the documented operator stop |
| Certificate-blocked assignment | **passed**: replay refused the blocked accepted certificate without a replacement identity; the original source exhausted retries and required operator stop |
| Excluded restart | **passed**: restarted source retained its blocked certificate; fresh standalone identities and a fresh admission attempt could not bypass exclusion |

The initial lost-ACK, peer-ejection and lost-departure targets ran sequentially after the fault-feature suite,
using `target/formation-faults/debug` with `formation-fault-test` only. The fault
worker SHA-256 was `db5df791a0c948ae39619bee12049cc76541943081f53a5c261b885e779b5159`;
the CLI hash matches the default checkpoint. Normal operator binaries were not
replaced with a fault build. All six initial process journeys completed successfully;
no failed assertion was retried or hidden in this batch.

The ordinary targets also passed their invoked helper suites: 23 HTTP tests,
six evidence tests and three UDP-relay tests where selected. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains. This initial
batch left the seven fault journeys below outstanding; the follow-on batch
completed them. Neither batch establishes optional telemetry acceptance.

The follow-on command is:

```sh
make test-formation-issuer-loss test-formation-issuer-ejection test-formation-source-loss test-formation-dead-assignment test-formation-removed-assignment test-formation-blocked-assignment test-formation-excluded-restart
```

All seven targets passed and the complete command exited successfully. The
isolated fault binaries and lockfile hashes were rechecked after completion
and still match this checkpoint. Together with the six preceding process
journeys, default/fault-feature suites and release guard, this closes the
finite final formation validation list. Historical failures remain recorded;
none of these final runs required an assertion retry or a timeout adjustment.
Documentation validation passed 119 Markdown files and scoped whitespace
checks passed after recording this result.

## Final formation acceptance disposition — 2026-09-07

This records N-FORMATION acceptance for source-built Linux, not combined M4
completion or certification of untested platforms/release artifacts. The
required failure matrix below remains the scenario-to-test mapping; this table
adds the non-matrix gates that previously had no single closure disposition.
`satisfied` applies only to the named boundary, `needs rerun` preserves existing
evidence pending the final process checkpoint, and `gap` identifies unfinished
capability. None converts a historical execution into a new passing test.

| Criterion / gate | Evidence and applicable boundary | Disposition / remaining action |
| --- | --- | --- |
| Accepted core correction and sans-IO dependency purity | Current `orishu-membership` all-target suite, including `the_core_has_no_io_async_clock_or_entropy_dependency`, resolved-graph budget and merge regressions | **satisfied** for the default dependency graph; exporter feature graphs must be checked when they exist |
| Trust bootstrap and pinned formation/node/certificate sessions | Current worker library suite; `peer::tls` mutual-trust/profile tests, real owner admission and authenticated wire fixtures; [required matrix](#required-failure-matrix) | **satisfied** at the mapped adapter/wire boundaries; retain process handoff evidence separately |
| Identity and secure restart persistence | Current `credentials` tests for identity/operator persistence, exclusive ownership, corruption refusal, permissions and symlinks; `real_workers_start_distinct_and_restart_without_restoring_membership` | **satisfied** for source-built Linux; no retained formation recovery or non-Unix credential support claimed |
| Safe socket lifecycle and config-free startup | Current `unix_socket` active/stale socket, replacement/symlink and unsafe-parent tests; real standalone executable tests | **satisfied** for the tested Unix listener and Linux filesystem contract |
| Client authority, real reads and unsupported modes | Current `real_inspection_reports_identity_and_rejects_unsupported_sources`, four mutation-credential matrices and plaintext-TCP refusal; [client contract](../protocol-client.md#implemented-formation-subset) | **satisfied** for Unix and verified TLS/TCP, HTTP/1.1 and HTTP/2; HTTP/3 and workload APIs remain unsupported |
| Public formation, labels, policy, catch-up, leave/replay, loss/restart and failure matrix | [Required matrix](#required-failure-matrix), [final process batch](#final-process-validation-batch--2026-09-07), [corrected readmission budget](#controlled-departure-round-and-readmission-budget--2026-09-07) | **satisfied** at the mapped production boundaries, including all final process reruns; no arbitrary-fault or fleet-scale claim |
| Bounded ingress, completion fencing and shutdown | Current default and fault-feature worker suites, final process pressure batch, [ingress closure](#ingress-pressure-case-to-contract-closure--2026-09-07) and [stale-IO mapping](#stale-io-cross-class-audit--2026-09-07) | **satisfied** at the mapped unit/runtime/executable and pressure-process boundaries; remaining recovery journeys retain their separate final rerun requirement |
| Fault controls excluded from normal releases | Current `make test-formation-release-guard`: four checker tests, successful normal release check, exactly the intended fault-feature compiler refusal | **satisfied** for the checked release configurations; not certification of a published package |
| Operator bootstrap, direct targeting and recovery instructions | [Worker manual](../../apps/orishu-worker/README.md), [CLI manual](../../apps/orishu-ctl/README.md), [recovery runbook](../cluster-admission-recovery.md), [story audit](#operator-support-and-story-maturity-reconciliation--2026-09-07) | **satisfied** for documented source-build membership scope; final fault-process runs must retain the runbook's exact correlation and stop assertions |
| Truthful protocol maturity | Reconciled the client protocol's stale claims that peer integration and identified recovery APIs were pending against current owner startup, routes and tests | **satisfied** for these identified contradictions; no wire shape or authority changed |
| Default repository validation | Commands and checkpoint below | **satisfied** for the recorded default workspace checks, not optional-feature or independent fault-process acceptance |
| P-OBSERVABILITY for M4 | Existing local/proxy scrape and probe evidence; `otlp-tracing` now compiles the bounded codec only; TLS adapter remains profile 4 | **gap**: actual bounded OTLP delivery, broader instrumentation, complete feature/security/overhead acceptance and accepted peer propagation remain required |
| P-OBS-DOCS for M4 | Local and mTLS-proxy source-build recipes; [documentation task](document-worker-observability.md) | **gap**: tested collector/correlation walkthrough and remaining formation deployment/dashboard/runbook handoff; later workload and release portions keep their own milestones |
| N-FORMATION / combined M4 disposition | Formation rows above and both companion gates | **N-FORMATION accepted** for the tested source-built Linux contract; **combined M4 incomplete** pending the companion capabilities and ADR 0025 review |

### Default validation checkpoint

Base HEAD is `9d71b752902a3aaeca376968d8f03424a2361b20`, with existing dirty
workspace changes preserved. Source-built Linux, default `target/`, no optional
worker features. These commands passed:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo test --locked --offline -p orishu-membership --all-targets --quiet
cargo test --locked --offline --workspace --all-targets --quiet
cargo test --locked --offline --workspace --doc --quiet
make test-formation-release-guard
make test-formation
make docs-check
```

The full default worker portion passed 120 library, 37 binary and 34
integration tests. The membership-only run passed 205 tests and benchmark
smoke cases; these are not performance measurements. Workspace doctests retain
one pre-existing ignored `query_filter!` example. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains. Workspace
formatting now passes; the earlier missing monitor-library failure is historical
and was not repaired by this formation increment. Socket tests ran with local
socket permission. No runtime source or dependency was changed in this audit.

The ordinary Make journey passed six evidence-helper tests, 23 HTTP-helper
tests and the full three-worker CLI handoff, leave/replay, crash/restart and
readmission journey under the corrected budget. Documentation validation
passed 119 Markdown files; scoped whitespace checks passed. Executable and
lockfile hashes were unchanged between the process run and final inspection:

| Checkpoint artifact | SHA-256 |
| --- | --- |
| `target/debug/orishu-worker` | `3650425b963c9b5f72933511e66e804c6128fd1497b2112b9fa0b1e5e4363bdf` |
| `target/debug/orishuctl` | `7d6f9cf179f3cce89570ef62b1456dca63fc58e399d6d4b23a8959d6d5f6d478` |
| `Cargo.lock` | `a6b1e97eb8ccd7453b37d9b2f2a4e7e77a9520c2bac647c4c65315976e35d6b8` |

### Finite remaining formation validation

The [final process batch](#final-process-validation-batch--2026-09-07) completed
all thirteen listed process reruns and the fault-feature all-target suite.
There is no remaining formation-only validation item at this checkpoint.
Retain these regressions when their owning code changes; do not restart this
completed checklist merely because historical missing-case briefs remain
below. Optional telemetry implementation/testing and its operator handoff stay
with the separate M4 gates, which remain open.

## Staged collector TLS and credential settings — 2026-09-07

Collector CA, client certificate/key and bearer-token file settings now follow
file/environment/CLI precedence under `spec.tracing`. Paths must be nonempty
and at most 4096 encoded bytes. Client certificate/key settings must be paired;
any collector trust or credential file requires an explicit HTTPS endpoint,
including when tracing is disabled. Loopback HTTP remains credential-free.
The [configuration contract](../orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable)
defines these independent collector settings and the remaining loading/TLS
obligations. No worker identity or operator credential is reused implicitly.

Unit tests cover pairing, HTTPS/no-endpoint refusal, exact path length limits,
explicit-only overlay and the retained unsupported-exporter gate. Five real
executable cases cover a missing key, environment-supplied key, CLI repair of
an empty environment key, an empty CLI override and plaintext endpoint refusal.
Valid disabled cases reach the deliberate later peer configuration error even
though every credential file is nonexistent in the private working directory.
Errors remain bounded and do not echo the tested key/token paths. Every case
exits within two seconds without state or client-socket creation.

Validation:

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker config::tracing --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --test standalone tracing_ --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo fmt -p orishu-worker -- --check
make docs-check
```

Focused minimal-build tests passed (five unit and three executable tests).
The observability-only worker suite passed **204 tests** (120 library, 42
binary, 42 integration). Worker formatting and docs validation passed (119
Markdown files), as did scoped whitespace checks. Workspace-wide
`cargo fmt --all -- --check` **failed** because the concurrently modified
`apps/orishu-monitor/Cargo.toml` referenced an absent `src/lib.rs`; that
unrelated work was preserved, not repaired by this increment. The shared
target's final tested executable has observability and no fault feature.

This is structural startup validation, not credential loading, permissions/PEM
verification, collector authentication or delivered spans. No exporter feature,
dependency, peer profile or formation authority changed. Full-workspace Rust,
fault-feature and external collector/process journeys were not rerun. Actual
safe bounded loading and TLS enforcement remain required before activation;
the capability-refusal gate is unchanged and M4 acceptance remains open.

## Staged OTLP destination validation — 2026-09-07

The worker accepts `spec.tracing.endpoint`, `ORISHU_TRACING_ENDPOINT` and
`--tracing.endpoint` through the normal explicit-only overlay. The
[destination contract](../orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable)
targets explicit complete OTLP/HTTP trace URLs: HTTPS, or HTTP only for literal
loopback IPs; bounded ASCII input; valid explicit ports and non-root paths;
no URL credentials, query, fragment, escapes or dot segments. No address/path
is inferred. The existing URI parser is reused without another dependency.

Raw staged input has redacted debug output. Validation occurs after overlays,
with a fixed diagnostic rather than a CLI parser error echoing credential-bearing
input. Four additional executable precedence cases check invalid file input,
environment and CLI overrides, and secret-free startup errors before state or
listener creation. The twelve existing budget cases remain in the same test.
This is parsing/validation only: no DNS, TLS, collector connection, exporter
feature or peer-profile change. Enabling tracing remains explicitly unavailable.

Initial compilation exposed the URI re-export path and the need to clone the
non-Copy endpoint separately from numeric overlays; both were corrected. The
hostile endpoint test then **failed** because the URI library's optional-port
accessor reports an out-of-range supplied port as absent. Explicit suffix
validation now rejects 65536, zero, empty and nonnumeric ports, while 1 and
65535 pass. Exact 2048-byte acceptance/2049-byte refusal is also tested.

Validation against the current dirty worktree:

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker config::tracing --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --test standalone tracing_ --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Focused minimal-build tests passed (four unit tests and two executable tests).
The final observability-only suite passed **202 tests** (120 library, 41 binary,
41 integration), including the expanded port/length fixtures. Builds remain
sequential in the shared target; the final test executable includes observability
without fault hooks. Formatting and documentation checks passed (119 Markdown
files). Full-workspace Rust, fault-feature and external collector/process
formation journeys were not rerun. These optional configuration fields are
additive; they neither activate OTLP nor change formation/wire compatibility.

## Staged tracing active-span and byte budgets — 2026-09-07

The startup configuration now adds independent active-span capacity, serialized
export-request bytes and collector-response bytes to the previously staged
queue/batch/time budgets. Defaults/ranges and the file/environment/CLI names
are published in the [configuration contract](../orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable).
All values are validated even with tracing disabled. Enabled tracing still
fails explicitly as unavailable; there is no new Cargo feature, exporter,
collector activity or peer-profile change in this increment.

`active_span_and_exchange_byte_budgets_have_exact_boundaries` checks each lower
and upper boundary and adjacent refusals, plus explicit-only overlays.
`tracing_memory_budget_precedence_is_validated_before_startup` exercises twelve
real executable cases: invalid file values; valid environment overriding an
invalid file; valid CLI overriding an invalid environment; and invalid CLI
overriding otherwise valid values, for all three budgets. Valid cases reach a
deliberate later configuration error; invalid cases name the offending budget.
All exit within two seconds without creating state or a client socket.

Validation passed against the current dirty worktree:

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker config::tracing --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --test standalone tracing_ --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Minimal-build tracing unit tests passed (3), as did both executable tracing
tests. The observability-only worker suite passed **201 tests** (120 library,
40 binary, 41 integration). Builds were sequential in the shared target; its
last tested executable includes observability without fault hooks. Formatting,
scoped whitespace and final docs checks passed (119 Markdown files). No full-workspace
Rust suite, fault-feature suite, process formation journey or collector harness
was rerun. Clippy passed in both tested feature configurations. The optional
configuration fields are additive; no dependency, formation persistence or
peer/client wire representation changed.

These settings are an exporter prerequisite, not enforced delivery budgets yet.
The documented implementation obligations include reservation before retaining
active state, bounded encoding before buffer growth, byte-aware batch splitting
and oversized-span shedding, bounded response consumption and independent
per-span payload limits. Endpoint/TLS/credentials, dependency/SDK isolation,
actual export enforcement and received/outage tests remain open. Configuration
acceptance must not be promoted to trace-export or combined M4 acceptance.

## Monitoring proxy timeout and capacity reclamation — 2026-09-07

The pressure harness now has a second, non-releasing branch. It first proves
the same sixteen held upstream requests, seventeenth-request 429 and successful
direct readiness/operator control as the release branch. It then leaves the
requests unanswered. All sixteen clients receive non-cacheable 504 responses
without metric samples; their upstream sockets receive EOF from Nginx.
Responses and upstream closure share one seven-second observation deadline,
with a four-second lower bound distinguishing timeout from immediate refusal.

The fixture retains the original client/relay socket handles until after a
fresh authenticated readiness request reaches the relay and receives the real
worker response. Thus neither closing test clients nor releasing a relay
manufactures reclamation. Separate operation IDs ensure each branch actually
locks/unlocks the worker rather than replaying earlier receipts. No proxy or
worker deadline was changed to pass the test.

The complete journey **passed** with both release and expiry branches and all
preceding TLS, authority, real Prometheus and outage controls:

```sh
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
make docs-check
```

This uses the existing observability-only source-built executables, privately
copied by the harness, and the previously recorded Nginx/Prometheus versions.
Only the harness and operator/evidence docs changed. Rust suites and separate
formation-fault journeys were not rerun. The tested timeout is for an upstream
sending no response bytes, not an absolute trickling-upstream deadline, slow
downstream behavior or all TLS/connection saturation cases. Those limitations
and the remaining M4 obligations are retained in the operator recipe.

## Monitoring proxy request capacity and recovery — 2026-09-07

The secure-proxy harness now tests the configured sixteen-active-request limit
using a finite loopback TCP relay between the real proxy and worker. Each of
sixteen mTLS clients sends `/metrics`; the fixture accepts and reads every
upstream request before holding its response. This proves admitted active work,
not merely operating-system listen backlog or a metric response in a socket
buffer. A seventeenth authenticated request receives 429 with no metric samples
and `Cache-Control: no-store`.

While the requests remain held, direct worker readiness is successful and real
authorized CLI lock/unlock completes with the expected state. The fixture must
reach release within four seconds, before the proxy's five-second read timeout.
It forwards each held request to the real worker and relays the unchanged
bounded response bytes. All sixteen return 200 with actual ready metrics;
after their sockets close, a fresh readiness request reaches the relay and
returns the actual worker's 200 response. Operator summary/identity is unchanged
after unlock. The relay bounds request heads to 4 KiB and metric responses to
9 KiB including headers, with two-second socket operations and bounded cleanup.

Validation **passed** using the same isolated source-built worker/CLI pair and
Nginx 1.28.0 / Prometheus 3.5.0 tools as the preceding checkpoint:

```sh
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
make docs-check
```

The same run retains mutual-trust refusal, authorized/unauthorized mutation,
real Prometheus ingestion, proxy loss and unavailable-upstream checks. This
closes the proxy active-request capacity/reuse case, not full connection/TLS
saturation, slow downstream readers, absolute expiry or fleet-scale behavior.
No proxy limit, worker runtime or protocol changed; Rust suites and separate
formation-fault process journeys were not rerun. Remaining M4 obligations stay
open rather than being inferred from this scoped pressure result.

## Monitoring proxy mutual trust and missing upstream — 2026-09-07

The secure-proxy harness now checks both TLS trust directions: monitoring
clients reject the server under an unrelated CA or an incorrect hostname,
while the preceding absent/wrong-CA client refusal remains in the same run.
No certificate-verification bypass is used to establish successful scraping.

After its real worker exits normally, the same proxy configuration is started
with no listening upstream. Authenticated requests to all four diagnostics
routes return 502, remain non-cacheable and expose no metric samples.
Unauthenticated clients remain refused; the operator route remains 404.
This is an actual connection-refused upstream, not a mocked worker response.
It does not establish slow/malicious upstream bounds, revocation or rotation.
The operator recipe now distinguishes this proxy error from a worker readiness
503 and explicitly rules out cached health or administrative-route fallback.

The harness also captures private worker/CLI executable copies before startup,
preserving one tested build pair across later rebuilds. Existing fixture PKI
and bounded process cleanup remain unchanged.

Validation command (same source-built observability-only binaries and pinned
tools as the preceding checkpoint):

```sh
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
make docs-check
```

The extended journey **passed**, including actual Prometheus ingestion, denied
monitoring mutation, authorized operator lock/unlock and proxy-outage controls.
No worker code, protocol or exporter configuration changed in this increment;
Rust suites and formation-fault process journeys were not rerun. Full M4
acceptance remains open.

## Secure monitoring proxy and real scrape — 2026-09-07

The accepted ADR 0017 proxy option now has checked-in Nginx and Prometheus
configuration, an executable harness and an operator recipe:
[secure monitoring proxy](../testing-worker-monitoring-proxy.md).
Nginx requires monitoring-CA mTLS for the forwarded diagnostics routes; the
worker remains loopback-only. No monitoring credential gains peer/client
authority, no exporter dependency was added and no protocol changed.

The final harness verifies real trusted scrapes and all three probes; refusal
of absent/wrong-CA clients; exact route/method restrictions; an operator bearer
token cannot replace mTLS; a monitoring certificate fingerprint cannot authorize
the actual CLI mutation; genuine operator credentials can lock/unlock; promtool
parses exposition; real Prometheus ingests `orishu_worker_ready`; and proxy
shutdown leaves the worker ready with unchanged formation identity/state.

Initial failures are retained here: sandbox socket creation was refused and
the test was rerun with local socket access. Nginx configuration validation
then exposed distribution-default temporary paths under `/var/lib/nginx`;
all paths are now prefix-local. Strict Python TLS validation refused generated
CA certificates lacking key usage; the fixture CA now declares CA constraints
and signing usage. Finally the authorized CLI control exposed missing required
formation/operation arguments in both mutation calls. The earlier nonzero
refusal was argument parsing, not authorization evidence. The corrected
negative call requires command-failure exit 1, alongside a successful authorized
control. None of these fixture corrections changed worker authority or TLS
verification rules.

Reproduction used the current dirty worktree and these commands:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability --target-dir target/formation-flow-observability
python3 scripts/check-worker-monitoring-proxy.py --nginx /tmp/orishu-nginx-check.XA33KT/extracted/usr/sbin/nginx --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
make docs-check
```

The build and final harness **passed**, including the case-sensitive route
control and authorized mutation control. Documentation validation passed
(118 Markdown files), as did scoped whitespace checks. The existing
`proc-macro-error2 v2.0.1` future-incompatibility build warning remains.

The Nginx distribution package `1.28.0-6ubuntu1.8` was downloaded and extracted
under `/tmp/orishu-nginx-check.XA33KT`, not installed as a service. Nginx reports
1.28.0; Prometheus/promtool report 3.5.0. The target binaries include
observability but no fault feature. Test PKI and all fixture processes are
temporary and cleaned up; the downloaded test tool remains reusable.

This is a same-host source-build security/scrape journey, not a remote-host
firewall, release artifact, container/service or full pressure certification.
Proxy timeout/capacity configuration is not saturation evidence. Native remote
worker binding remains refused; tracing, overhead and full M4 handoff remain
open. No Rust test suite, release guard or independent formation-fault journey
was rerun for this configuration/harness increment.

## Pre-adoption HTTP probes and phase coverage audit — 2026-09-07

The HTTP phase audit identified pre-adoption `Joining` as distinct from the
already covered post-adoption catch-up and exhausted `JoinUnresolved` states.
`http_pre_adoption_join_stays_live_unready_with_original_identity` now uses the
existing one-shot lost-ACK hook. Real authenticated issuer admission occurs,
but the source has not received the assignment. While its published phase is
`Joining`, it retains its original standalone formation/node and one-member
view, with introduction disabled. HTTP liveness/startup remain 200 and
readiness is 503. Ordinary reconnect recovers exactly the issuer's recorded
assigned ID; complete catch-up then restores readiness without another identity
change. No new fault control or runtime behavior was introduced.

The first fixture **failed** its two-second observation deadline: holding the
issuer before the prerequisite handshake kept the source out of `Joining`.
Using actual insertion/lost-ACK evidence corrected the fault location, not the
runtime or its deadlines. The corrected focused test passed before the final
assigned-ID assertion was added; the full suite below validates that assertion.

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_pre_adoption --target-dir target/formation-flow-observability
```

The current phase mapping is below. Existing process/collector results remain
the evidence of their linked checkpoints, not new executions in this audit.

| Published probe situation | Named evidence / remaining boundary |
| --- | --- |
| Initializing, then healthy standalone without compute | `http_probes_track_initialization_role_failure_and_closed_owner`; real HTTP around owner initialization and a non-compute standalone |
| Membership locked/unlocked | The same HTTP fixture applies real lock commands and remains ready while enforcing policy |
| Pre-adoption joining | New `http_pre_adoption_join_stays_live_unready_with_original_identity`; real lost-ACK and exact-assignment recovery |
| Adopted, incomplete/failed catch-up | `http_readiness_waits_for_real_admission_catchup`, `http_catchup_failure_stays_live_unready_and_recovers`, `http_malformed_catchup_page_stays_live_unready_and_recovers`, `http_partial_catchup_stays_live_unready_and_recovers` |
| Exhausted unresolved admission | [Issuer-loss process probes](#unresolved-admission-probes-and-executable-isolation--2026-09-07) |
| Peer loss / local ejection | [Wire-ejection process probes](#process-probes-across-wire-ejection--2026-09-07); healthy survivor distinguished from ejected worker |
| Required local role unavailable | `http_probes_track_initialization_role_failure_and_closed_owner` cancels a guarded role task and checks sticky readiness failure; storage-specific failure awaits a real storage role |
| Owner stall and recovery | `http_probes_detect_running_owner_stall_and_recovery`; actual owner pause, HTTP remains available but cannot refresh supervision |
| Supervised shutdown / owner closure | `http_reads_cannot_hide_stalled_owner_shutdown` and the closed-owner HTTP fixture; whole-process cancellation remains separately covered by unfinished-diagnostics shutdown tests |
| Scraper/collector unavailable | Existing [Prometheus outage recipe](../testing-worker-prometheus.md) covers standalone scraper isolation; actual OTLP collector outage remains unavailable until export exists |

This mapping is not the entire observability acceptance matrix. Workload-stop,
under-replication and storage-role cases await their actual owning runtimes;
they are not fabricated for M4. The remaining M4 requirements include secure
remote route separation, trace export/propagation/outage, broader formation
instruments, overhead and executable operator deployment handoff. Existing
diagnostics pressure tests retain their scoped ingress/control guarantees;
this audit does not claim every phase under every overload combination.

Validation of the final test revision passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_readiness_waits --target-dir target/formation-flow-observability
cargo fmt --all -- --check
make docs-check
```

The combined suite passed **204 tests** (120 library, 45 binary, 39 integration),
and Clippy passed with warnings denied. The observability-only happy-path test
passed after sequential rebuilding without fault hooks; that is the shared
target's last binary test build. Formatting, scoped whitespace and docs checks
passed (117 Markdown files). This increment changed a test fixture and docs,
not production code. No full-workspace Rust suite, release check, independent
formation process journey, Prometheus collector or OTLP receiver was rerun.

## Partial catch-up through HTTP probes — 2026-09-07

`http_partial_catchup_stays_live_unready_and_recovers` extends the malformed-page
fixture to a valid prefix followed by a failing second page. The target starts
with 40 name-block records installed through validated domain commands before
its owner/dispatcher starts. Their reserved fixture labels do not match either
test worker. Normal source capture and pagination produce a two-page baseline;
the development-only corruption seam now selects a zero-based page index.
Non-page replies and page 0 are returned unchanged; page 1 gets schema zero.

The production receiver requests page 1 only after validating page 0. The
one-shot corruption acknowledgement therefore establishes a valid received
prefix, not just a descriptor claiming multiple pages. The source subsequently
reports `CatchUpFailed`, remains live/unready/startup-latched over HTTP and
retains its adopted identity without introducing. Ordinary maintenance retries
without another corruption, completes catch-up and restores readiness and
introduction with the same identity. No completed transfer or health state is
injected. The whole fixture, including normal shutdown, remains bounded to
forty seconds.

Focused reproduction passed (one test):

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_partial_catchup --target-dir target/formation-flow-observability
```

This covers the valid-prefix/failing-page HTTP combination identified in the
previous checkpoint. It is still two owners in one process with real QUIC and
Unix HTTP, not a separate-process or secure remote diagnostics deployment.
It does not assert that ordinary gossip cannot deliver the same public policy
records independently; the authority assertion is that partial baseline
receipt never grants introduction or completes the catch-up operation.
Fixture setup and indexed corruption exist only with `formation-fault-test`;
normal protocol bytes, baseline limits and core dependencies are unchanged.

The following validation passed against the current dirty worktree:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_readiness_waits --target-dir target/formation-flow-observability
python3 scripts/check-formation-release.py --cargo cargo
cargo fmt --all -- --check
make docs-check
```

The combined suite passed **203 tests** (120 library, 44 binary, 39 integration),
including the preceding stalled-exchange and malformed-first-page regressions.
Clippy passed with warnings denied. The observability-only happy-path test
passed after a sequential rebuild without fault hooks; that is the shared
feature target's last binary test build. The release checker verified a healthy
normal release check and exactly the intended fault-feature rejection, using
the default target directory. Formatting, scoped whitespace checks and docs
validation passed (117 Markdown files). No full-workspace Rust suite, separate
formation process journey, Prometheus collector or OTLP receiver was rerun.

## Malformed catch-up page through HTTP probes — 2026-09-07

`http_malformed_catchup_page_stays_live_unready_and_recovers` now exercises
malformed page rejection through the real receiver and production HTTP probes.
After pinned admission, the issuer's development-only Rust seam changes one
normally authorized and encoded page to schema zero. The reply's authenticated
formation/source/request envelope and CBOR remain intact; no health state,
core state or completed transfer is manufactured. A one-shot acknowledgement
proves the test reached page encoding rather than merely failing setup.

The source reports `CatchUpFailed`, retains its adopted formation/node identity
and cannot introduce. HTTP `/livez` and `/startupz` remain 200 while `/readyz`
is 503. The fault consumes itself; ordinary maintenance obtains a valid
baseline, reaches joined/introducer-ready and restores all probes to 200 with
unchanged identity. Both owners and HTTP serving shut down normally within
the forty-second whole-fixture deadline. The corruption seam has no CLI or
network control and is compiled only with `formation-fault-test`.

Focused reproduction passed (one test):

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_malformed_catchup --target-dir target/formation-flow-observability
```

This is two real runtime owners and Unix HTTP in one process. It does not
establish independent-process fault behavior, a valid multi-page prefix before
a later failing page, secure remote diagnostics or OTLP export. Those distinct
M4 requirements remain open. Normal profile-4 encoding and receiver behavior
are unchanged; no protocol or persisted-data migration is introduced.

Validation against the current dirty worktree passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_readiness_waits --target-dir target/formation-flow-observability
python3 scripts/check-formation-release.py --cargo cargo
cargo fmt --all -- --check
make docs-check
```

The combined suite passed **202 tests** (120 library, 43 binary, 39 integration).
Its initial sandboxed run failed 53 library cases on local socket permissions;
the same complete suite passed when rerun with socket access. Clippy passed
with warnings denied. The observability-only happy-path regression passed
after sequential rebuilding without fault hooks. Normal release checking and
the exact fault-feature compile-time rejection both passed. Formatting and
documentation checks passed (117 Markdown files). The shared feature target's
last binary test build has observability only; the release check uses the
default target directory. No full-workspace Rust suite, independent-process
formation harness, Prometheus scrape or OTLP receiver was rerun in this increment.

## Catch-up exchange failure through HTTP probes — 2026-09-07

The feature-enabled diagnostic fixture now includes
`http_catchup_failure_stays_live_unready_and_recovers`. It performs real pinned
admission, waits for decoded SWIM traffic on both owners to prove their
post-adoption member sessions are established, then holds the issuer owner
through the existing development-only scheduling seam. Catch-up maintenance
runs normally; the source's established exchange cannot complete.

Until the source reports `CatchUpFailed`, each HTTP probe check requires
`/livez` 200, `/readyz` 503 and `/startupz` 200, with unchanged adopted
formation/node identity and introduction disabled. Failure must be observed
between four and twelve seconds, distinguishing a held exchange from immediate
setup refusal. Releasing the issuer permits ordinary bounded maintenance to
complete catch-up; the source then reports joined/introducer-ready and all
three probes return 200. Both owners and the diagnostic server shut down
normally under the forty-second total fixture budget.

The first fixture **failed** because it paused the issuer before the new
admitted session existed: it held handshake establishment rather than an
actual catch-up exchange, so no failed transfer was observed within twelve
seconds. Replacing that scheduling assumption with actual decoded member
traffic made the intended boundary observable. No timeout, health calculation,
protocol or production owner behavior was changed to make the test pass.

This extends unfinished-exchange HTTP coverage, not partial-page or malformed
baseline-byte coverage. It uses real runtime/wire and production HTTP handlers
within one test process, not independent worker processes or a secured remote
diagnostics deployment. Those separate M4 requirements remain open.

Focused reproduction passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_catchup_failure --target-dir target/formation-flow-observability
```

The combined observability/fault worker suite passed **201 tests** (120
library, 42 binary, 39 integration), and all-target Clippy passed with warnings
denied. The ordinary observability-only catch-up probe regression also passed
after rebuilding sequentially without fault hooks:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --target-dir target/formation-flow-observability -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_readiness_waits --target-dir target/formation-flow-observability
```

Workspace formatting and documentation checks passed (117 Markdown files).
No default-feature/full-workspace Rust suite, separate process journey,
Prometheus collector or OTLP export test was rerun for this diagnostic test
extension. The shared target's last binary test build has observability only.

## Operator support and story maturity reconciliation — 2026-09-07

Reviewed CLI dispatch, worker routes and the manuals against the passing
formation/recovery matrix. The CLI manual now enumerates the supported
membership-only surface, its indirect inspection boundary and direct-worker
operation targeting. Imported workload/storage/removal/log/audit commands are
not represented as supported worker capabilities merely because they appear
in CLI help. No unsupported command or public mutation was enabled by this
documentation change.

Corrected stale CLI claims that retired-assignment/issuer-loss recovery was
unfinished: the mapped bounded stop paths have process evidence, whereas
automatic restoration remains unsupported. Removed the worker README's
contradictory assertion that all metrics/probes were unimplemented; it now
links the actual optional loopback surface and explicitly excludes unfinished
remote security and OTLP export. A local lock receipt still does not establish
convergence, even though the separate public journey now tests convergence.
The peer protocol's initial lost-ACK fixture is distinguished from the later
recovery evidence rather than described as the whole current implementation.

Both operator story documents now identify which membership-only outcomes the
PoC exercises and which broader outcomes remain desired work. They retain
workload, audit, resource telemetry and deployment ambitions without claiming
the formation tests complete them. Platform statements are limited to the
source-built Linux evidence; non-Unix credential loading fails explicitly,
and other Unix platforms/release artifacts are not certified by these runs.

This documentation-only checkpoint reuses the preceding full-workspace and
named process evidence; no runtime tests were rerun or statuses upgraded solely
from prose. `make docs-check` passed. Final task-wide acceptance reconciliation
and P-OBSERVABILITY/P-OBS-DOCS delivery remain open.

## Workspace validation and current maturity reconciliation — 2026-09-07

The required default workspace checks were executed against the dirty worktree
based on `9d71b752902a3aaeca376968d8f03424a2361b20`, using the default `target/`
directory with no optional worker features. Existing unrelated Kagami and other
worktree changes were preserved; this is not validation of clean HEAD alone.

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc --quiet
make docs-check
```

All commands **passed** after the all-target suite was rerun with local-socket
permission. Its initial sandboxed invocation stopped in `orishu` with 21 socket
binding permission failures (`Operation not permitted`); these were not failing
domain assertions. The full rerun completed, including 120 worker library,
34 worker binary and 32 worker integration tests, membership dependency checks
and the other workspace packages. Benchmark targets ran smoke cases, not
measured performance/scaling trials. Doctests passed with one pre-existing
ignored `query_filter!` example in `crates/orishu/src/model/mod.rs`; that example
was not executed and is not formation evidence. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains. Documentation
validation passed 117 Markdown files.

Removed obsolete current-tense claims in the client protocol that identified
join execution, status and polling were unwired. Their implemented route,
supervision and replay sections now agree. Task/roadmap index status records
the passing mapped formation matrix rather than describing its closed recovery
and pressure rows as missing implementation. No runtime code changed here.

This is a workspace validation checkpoint, not complete goal acceptance:

- Complete the final task-wide authority/configuration/platform/manual audit
  and reconcile remaining historical maturity text outside these join sections.
  The mapped failure table alone does not establish every handoff criterion.
- Preserve separate ordinary/fault-process results and release-guard evidence
  from their checkpoints; no separate process journey or release build was
  rerun by the workspace commands above.
- P-OBSERVABILITY/P-OBS-DOCS remain incomplete: default tests do not exercise
  secured remote monitoring, the complete probe/feature matrix, trace export,
  cross-peer propagation or all operator deployment recipes.
- ADR 0025 remains proposed. Passing formation tests do not approve profile 5
  or waive the required received cross-peer trace for combined M4.

## Ingress pressure case-to-contract closure — 2026-09-07

This reconciles the final pressure row against the current client contract,
production server settings and named assertions. The preceding default worker
suite (185 tests) and full `--client-pressure` handoff/churn journey passed;
their exact commands and checkpoint remain in the TLS evidence below. This
audit does not present those executions as another rerun.

| Contract / scenario | Evidence at the required boundary |
| --- | --- |
| 64 accepted client connections; unfinished heads; reclaimed capacity | `incomplete_headers_bound_connections_and_expire` proves acceptance with responses on all 64 sockets, refusal to serve the queued connection, continued control and immediate reuse before expiry. `--client-pressure` retains 60 partial heads with real peer policy progress. |
| Five-second HTTP/1 head limit, 8 KiB buffer, 32 headers | `oversized_and_excessive_headers_are_rejected_before_handlers`, the incomplete-head expiry test and the public pressure journey verify rejection/expiry before handler admission. |
| Pre-HTTP TLS stall | `unfinished_tls_handshakes_expire_without_blocking_authenticated_control` verifies silent and ciphertext-trickled connections, earlier protocol-detection expiry, independent authenticated control and a fresh successful TLS connection. It does not claim full-capacity TLS saturation. |
| Handler body work and 16-slot mutation pressure | `--client-pressure` establishes actual partial-body occupancy, bounded refusal, independent reads/peer control, release and reuse; the existing request/credential matrices retain endpoint-specific byte limits and valid controls. |
| Five-second stalled transport writes; bounded pipelining | `stalled_summary_reader_times_out_without_blocking_control` observes a real pending Unix write, unchanged handler-call/byte counts while blocked, authenticated lock progress and server-driven expiry. A small successful response is not its proof of backpressure. |
| HTTP/2 input assembly and ten-second inactivity | Unix/TLS `*_active_partial_frame_expires_despite_progress` and `*_continued_header_block_has_one_absolute_budget`; `http2_silent_partial_headers_frames_and_responses_expire`; watchdog unit tests and `assembly_expiry_reclaims_the_only_accepted_connection_slot`. Long-lived completed-assembly controls preserve valid multiplexing. |
| HTTP/2 stream/header/frame settings and continuation work | `http2_stream_capacity_is_advertised_enforced_and_reclaimed` checks advertised limits, seventeenth-stream refusal and reuse; `http2_continuation_budget_and_stream_binding_are_enforced` covers excessive/mismatched continuations and a legal final-sixth continuation. Fixed HPACK/send-buffer settings are configuration bounds, not independent heap measurements. |
| HTTP/2 receive windows and invalid window updates | `connection_receive_credit_accepts_exact_limit_then_refuses_one_byte`, `stream_receive_credit_is_independent_of_connection_credit`, `invalid_window_updates_have_stream_and_connection_error_scope`. These use the production Unix decoder/server; the stream-isolation case explicitly enlarges only connection credit. |
| HTTP/2 response credit, absolute delivery and cancellation | `http2_zero_response_window_isolated_and_recovers`, `http2_connection_response_credit_is_enforced_and_recovers`, exhausted-credit/withheld-credit expiry tests (including TLS), `http2_reset_and_completed_response_cancel_delivery_deadlines` and delivery-observer tests. They separate transport acceptance from receipt delivery and bound metadata to sixteen responses. |
| Peer incomplete streams plus datagram flood | `incomplete_streams_and_datagram_pressure_preserve_control_and_shutdown` proves transport-credit occupancy, live decoded datagrams, one-second lock/unlock progress and two-second shutdown/reclamation. `multiple_peers_exhaust_global_exchanges_and_recover_after_disconnect` establishes the shared 64-exchange cap across peers. |
| Bounded serialized response output | `output_cap_accepts_exact_limit_and_refuses_growth_before_copying` exercises the actual shared serializer at exactly 1 MiB and one byte over, plus refusal without writer length/capacity growth. Resource pagination tests bound domain construction separately. |
| Whole shutdown with unfinished clients | `shutdown_cancels_unfinished_client_requests` and the full public pressure journey retain incomplete sockets until worker exit, assert a shared three-second shutdown budget and socket cleanup. The fixture does not close clients first to manufacture cancellation. |

The row is **passed at these mapped boundaries**. Shared plaintext parser,
owner and codec contracts do not require duplicating every case over every
transport; actual TLS authentication and TLS-specific assembly/delivery/stall
tests remain distinct. This is finite three-worker operational conformance,
not a fleet throughput guarantee, a minimum-throughput policy, process RSS
benchmark or availability under sustained saturation. Monitoring HTTP pressure,
stalled-owner probes and exporter isolation remain P-OBSERVABILITY acceptance.

The only added code is a codec regression; runtime semantics and limits are
unchanged. A five-byte CBOR byte-string header plus payload fills the exact
1 MiB limit. The next byte is refused before copying; an empty write at capacity
remains valid. This test closes the explicit output-cap assertion instead of
inferring it from a small real response.

Current focused verification passed **34 worker binary tests** and **five
codec tests**, including that new boundary test:

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker --target-dir target/formation-flow-observability --quiet
cargo test --locked --offline -p orishu-worker --lib peer::codec --target-dir target/formation-flow-observability
```

Final full-workspace validation, remaining release/platform/manual closure and
combined M4 acceptance are separate; this row's closure does not complete the
formation task or its observability companions.

## Client TLS pre-HTTP deadline evidence — 2026-09-07

`unfinished_tls_handshakes_expire_without_blocking_authenticated_control`
starts the actual worker with certificate-verified HTTPS control. It holds one
silent TCP connection and one incomplete TLS record, then trickles ciphertext
while issuing authenticated summary requests on a separate connection. Both
unfinished connections must close without fixture cancellation, and a fresh
authenticated TLS client must still retrieve the real standalone summary.
The whole fixture has a 25-second deadline and terminates the worker normally.

The initial fixture failed its assumed eight-second lower bound: the partial
connection closed at about five seconds. Inspection of the pinned Salvo
`HandshakeStream` and protocol-detection path established that
the initial five-second HTTP protocol-detection read wraps the lazy TLS stream.
It therefore expires before the ten-second TLS handshake fallback. The fixture
now requires a held connection (not immediate parser refusal), expiry between
four and seven seconds and successful independent control throughout. This is
a corrected test assumption, not a repaired runtime timeout or an unexplained
passing retry.

The worker explicitly configures the existing ten-second TLS fallback instead
of inheriting the dependency default. The client protocol and worker manual now
document both deadlines and their ordering. No capability, transport, authority
or default timeout changed. The existing remote-summary authentication test
shares startup setup with this new case and retains its original assertions.

This proves bounded silent/trickled TLS input below listener capacity. It is
not a full 64-connection TLS saturation experiment, client-certificate denial
matrix, or a claim of service availability under sustained adversarial load.
Unix/TLS plaintext HTTP/2 assembly and delivery deadlines retain their separate
tests; TLS handshake expiry is not substituted for them.

Focused reproduction **passed** after correcting the lower-bound assumption:

```sh
cargo test --locked --offline -p orishu-worker --test standalone unfinished_tls_handshakes --target-dir target/formation-flow-observability
```

Broader verification at the same dirty-worktree checkpoint **passed**:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability -- -D warnings
python3 scripts/check-formation-cli.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --client-pressure
cargo fmt --all -- --check
make docs-check
```

Default worker tests passed **185 tests** (119 library, 34 binary, 32
integration). The private-executable pressure journey passed slow-body mutation
saturation, independent reads/peer policy, capacity recovery, full churn and
shutdown with unfinished clients. Documentation checked 117 Markdown files.
Optional telemetry/fault-feature suites and full-workspace Rust tests were not
rerun for this explicit-existing-default and executable-regression change.
The final ingress case-to-contract audit and combined M4 remain open.

## Original issuer ejection and unavailable accepted history — 2026-09-07

The new `--issuer-ejection` journey targets the history-loss combination from
the audit below. A inserts B and deliberately loses the ACK. The harness pauses
B, joins C through A normally, then signals C to send A's valid tombstone over
the authenticated peer connection. A must become ejected without changing its
formation/node identity and refuse join-material retrieval. After B resumes,
each poll checks the original operation/reference and the matching issuer's
`recordUnavailable` report, plus the issuer's exact remaining B/C member set.
The original source must exhaust its normal retry budget, preserve its old
standalone identity and reject new join/leave work. Exact original request
replay must return the exhausted record unchanged. Both processes stay alive;
no restart or alternate introducer supplies a substitute outcome.

The existing debug-only signal fixture now selects a joined worker's recorded
introducer by node identity when that reference belongs to its current
formation; without such a reference it retains the unambiguous sole-peer rule.
It still requires an actual live registered peer route and fitting datagram.
This is authenticated test-peer input, not an operator removal API or a claim
that the sender committed a cluster-wide removal. Normal builds do not acquire
the fixture, and release builds continue to reject `formation-fault-test`.

Reproduction uses the new `make test-formation-issuer-ejection` target, or:

```sh
cargo build --locked --offline -p orishu-worker --features formation-fault-test -p orishuctl --target-dir target/formation-faults
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --issuer-ejection
```

The worker manual and recovery runbook explain why identity-matching
`recordUnavailable` is not evidence of non-insertion. No protocol profile,
production recovery rule, dependency or persisted format changed.

At this dirty-worktree checkpoint the new journey **passed once**, using the
unchanged retry schedule and a 210-second source observation bound. The
existing `--peer-ejection` journey also passed, preserving the sole-peer
fixture's behavior. Both used private captured executables from the fault
target; subsequent test builds could not replace their running binaries.

Additional verification **passed**:

```sh
cargo test --locked --offline -p orishu-worker --features formation-fault-test --all-targets --target-dir target/formation-faults --quiet
cargo clippy --locked --offline -p orishu-worker --features formation-fault-test --all-targets --target-dir target/formation-faults -- -D warnings
cargo test --locked --offline -p orishu-worker --test standalone ordinary_executable_rejects_all_formation_fault_controls_before_startup --target-dir target/formation-flow-observability
python3 scripts/test_formation_evidence.py
python3 scripts/test_formation_http.py
python3 scripts/test_formation_release.py
python3 scripts/check-formation-release.py --cargo cargo
cargo fmt --all -- --check
make docs-check
```

The fault-feature worker suite passed **183 tests** (119 library, 34 binary,
30 integration); the separate normal-executable rejection test passed its six
fault switches. Helpers passed six evidence, 23 HTTP and four release tests.
The first HTTP-helper invocation failed nine socket cases with sandbox
`Operation not permitted`; rerunning with local-socket permission passed all
23. Release validation proved both normal release success and the precise
fault-feature compile-time refusal, not an unrelated build failure. The
existing `proc-macro-error2 v2.0.1` future-incompatibility warning remains.
Documentation validation passed 117 Markdown files.

This closes the concrete unavailable-accepted-history process gap in the
preceding recovery audit. The older missing-case brief is retained as history,
not reopened work. The mapped recovery row is now passed at its stated
boundaries; final formation acceptance still includes the other matrix rows,
workspace verification and synchronized handoff. Optional telemetry and the
complete workspace test suite were not rerun in this increment.

## Recovery outcome and history-loss audit — 2026-09-07

The finite audit distinguishes the source's retained operation from the
issuer's retained assignment. Inspection reviewed `Control::InspectAdmission`,
the two `admission_replays` reset sites, the no-eviction ledger and the public
CLI fault journeys. These are separate outcomes, not interchangeable examples
of a failed join:

| Condition | Existing evidence | Remaining distinction |
| --- | --- | --- |
| No admission emitted | Join-operation phase invariants and runtime initial-handshake failure; `failedBeforeAdmission` cannot describe emitted/adopted work | No claim about another attempt or a previous source lifecycle |
| Lost ACK, original assignment usable | Full `--lost-join-ack` public handoff/leave/readmission/crash/restart journey | Recovery retains the original assignment and retry identity |
| Assignment Dead, removed or certificate-blocked | Separate `--dead-assignment`, `--removed-assignment`, `--blocked-assignment` journeys | Preserve the three restriction mechanisms rather than merging their assertions |
| Original issuer unreachable, then restarted | `--issuer-loss`: real retry exhaustion, retained source uncertainty, restarted issuer `wrongIssuer` | Lookup failure is not a negative admission report |
| Source history lost, original issuer survives | `--source-loss`: stalled issuer lookup followed by source crash/restart, retained issuer assignment and source `UnknownOperation` | Stop without a replacement join; retained certificate is not retained operation history |
| Both histories lost | Second crash/restart stage of `--issuer-loss` | Fresh source identity and missing operation do not repair the original uncertainty |
| No accepted record for the referenced pair | Ordinary unknown-attempt inspection and `--excluded-restart`'s actual rejected fresh attempt | These do not prove loss of a previously accepted record at an identity-matching issuer |
| Invalid, uncorrelated or failed inspection response | CLI `inspection_preserves_evidence_and_refuses_uncorrelated_reports_without_mutation`, twelve fixture cases | Client/wire validation only, not issuer retention or a process recovery journey |
| Issuer self-ejected after insertion, before source recovery | Source inspection finds a distinct ledger-reset path in `Owner::eject` | **Missing combined process case**, specified below |

The no-eviction ledger test proves capacity cannot discard a retained accepted
assignment while that ledger remains installed. It does **not** prove all
history loss changes the issuer identity: `Owner::eject` clears the ledger but
keeps the formation/node pair. `InspectAdmission` checks formation, node and
certificate, then looks up the record; its current branches therefore predict
`recordUnavailable`, not `wrongIssuer`, for this ejected original issuer.
This is a source-derived prediction, not passing process evidence. The existing
self-ejection journey tests the applicant after adoption, not the issuer of an
unresolved admission. Do not close the recovery row by substituting it.

### Next bounded case: accepted record lost through issuer ejection

Arrange a real admission insertion and lost ACK, then have an authenticated
test peer deliver the issuer's own valid tombstone through the production wire
path before the source recovers. Keep the original source attempt and issuer
process alive. Through public operator requests, require:

- issuer participation `ejected`, unchanged original formation/node/pin and
  refusal of further introduction;
- inspection of the exact retained source reference returning the matching
  original issuer identity with `recordUnavailable`;
- source retaining original correlation and unadopted identity, ending in the
  existing bounded unresolved/stop path without duplicate assignment;
- exact request replay preserving exhaustion, and fresh join/leave refusing
  while uncertainty remains; no alternate introducer, restart, certificate
  rotation or exclusion removal as recovery.

Use existing narrowly scoped fault seams or a bounded fixture extension, not a
new public removal API. Retain the real retry budget and whole-journey cleanup.
If the observed result differs, record it before proposing a behavior change;
this audit does not authorize replacing the accepted recovery authority.
The recovery matrix stays partial until this process case and final validation
are evidenced.

### Current verification

All seven fault-process commands below **passed once** at this dirty-worktree
checkpoint. The debug worker was rebuilt with `formation-fault-test` and no
optional telemetry. Each harness captured private executable copies and used
isolated state directories and dynamically allocated listeners; the commands
ran concurrently without subsequent writes to their input executable paths.
The full lost-ACK command includes handoff, leave/readmission and crash/restart,
not only admission. Issuer loss used the unchanged 183-second retry windows
and the harness's 210-second observation bound; no clock acceleration was used.

```sh
cargo build --locked --offline -p orishu-worker --features formation-fault-test -p orishuctl --target-dir target/formation-faults
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --issuer-loss
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --source-loss
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --dead-assignment
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --removed-assignment
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --blocked-assignment
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --lost-join-ack
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --excluded-restart
```

The inspection CLI fixture (one test, twelve cases), accepted-ledger test and
four join-operation tests also **passed**, along with six evidence-helper and
23 HTTP-helper tests:

```sh
cargo test --locked --offline -p orishuctl --test admission_inspection --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --lib admission_replay --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --lib join_operations --target-dir target/formation-flow-observability
python3 scripts/test_formation_evidence.py
python3 scripts/test_formation_http.py
make docs-check
```

Documentation checks passed (117 Markdown files). No code changed in this
audit. The build retains the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning. Full workspace, complete feature matrix,
release guards, other fault scenarios and telemetry journeys were not rerun.
Passing these cases does not erase earlier failures or close the distinct
issuer-ejection case above.

## Late admitted-peer handshake completion — 2026-09-07

`peer::dial::tests::late_admitted_handshake_is_fenced_after_leave_and_shutdown`
closes the specific held-success gap identified by the cross-class audit below.
Two owners start with matching admitted identity/certificate records; a real
`Dialer::member_handshake` performs pinned QUIC/TLS and the application exchange.
The test holds its successful result before registering it with the source
owner through `accept_member_handshake_reply`.

Four cases run under one twenty-second fixture deadline:

- Current-generation completion registers successfully, keeps its connection
  open and preserves the two-member view.
- After state-changing leave, the original generation receives `Stale`; the
  replacement formation/node and one-member view remain intact, with no added
  admission or completed-exchange count.
- A completion queued immediately before reserved shutdown is discarded;
  its response channel closes while the owner handle and observer connection
  remain alive.
- Delivery after owner shutdown returns `Unavailable` explicitly.

For every refusal/discard case, the connection is demonstrably open before
delivery, becomes `LocallyClosed` within one second and the client endpoint
drains to zero connections within three seconds. The fixture does not close
the endpoint to manufacture this result. Both owners and the dispatcher finish
normally. No production logic, protocol, dependency or public API changed.
Already-admitted fixture setup is not evidence of an operator admission journey;
the test exercises the production outgoing-handshake completion boundary.

The focused regression passed on the current dirty worktree:

```sh
cargo test --locked --offline -p orishu-worker --lib late_admitted_handshake --target-dir target/formation-flow-observability
```

The default-feature worker all-target suite also passed **184 tests** (119
library, 34 binary, 31 integration), and scoped all-target Clippy passed with
warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --target-dir target/formation-flow-observability -- -D warnings
```

The initial scoped formatting check identified two wraps in the new test;
`cargo fmt -p orishu-worker` corrected them. Fault-feature and optional-exporter
builds, separate process journeys and full-workspace Rust tests were not rerun
for this test-only increment.

Together with the preceding cross-class audit and its rerun tests, this closes
the mapped stale-IO failure row at its specified core/owner/real-wire boundaries.
It does not close final process reruns, other formation rows, optional telemetry
or M4. The older audit's missing-case wording below is historical and resolved
by this checkpoint, not an instruction to implement the same regression again.

## Stale-IO cross-class audit — 2026-09-07

Inspected the current owner input/completion branches, lifecycle cleanup,
runtime maintenance and named tests against preflight area 6. This reconciles
the older inventory with subsequent regressions; it does not reopen their
implementation. One specific admitted-peer completion case remains unmapped.

| Class | Current evidence and boundary | Audit disposition |
| --- | --- | --- |
| Incoming handshake and decoded packet | `real_handshake_is_decided_by_owner_and_closed_when_formation_changes` observes real connection retirement and stale incoming-handshake refusal. `accepted_binding_is_rechecked_after_removal_and_generation_changes` checks current binding. Old successful send results below enter the same `Owner::peer` packet fence before decoding. | Mapped at wire/registry/owner boundaries, not a separate process journey. |
| Initial join preparation | `stale_join_preparation_cancellation_preserves_new_operation`, `late_successful_initial_handshake_is_closed_after_leave_or_shutdown`, and `reserved_control_disposes_payload_on_either_side_of_receiver_drop` cover cancelled preparation, genuine held success, replacement and both mailbox-disposal orderings. | Mapped; successful connection disposal is observed without test-owned endpoint closure. Replacement is staged below the public busy guard. |
| Pending-join reconnect | `replacement_fences_successful_join_reconnect_and_preserves_new_operation`, `shutdown_fences_successful_join_reconnect_handshake`, and `cancelled_join_reconnect_jobs_release_reservation_without_resetting_budget`. | Mapped at real runtime/wire and reserved-completion boundaries; these are not admitted-peer maintenance handshakes. |
| Admission baseline/credentials | `leave_fences_successful_catchup_completion`, `ejection_fences_successful_catchup_completion`, `shutdown_fences_successful_catchup_completion`, `ejection_fences_prepared_catchup_and_its_late_cancellation`, `source_session_loss_refuses_successful_catchup_then_retries`, and `adoption_deadline_refuses_successful_catchup_completion`. | Mapped; generation, attempt, participation, source session and whole catch-up deadline have distinct coverage. |
| Reliable send success/error and active exchange | `old_send_results_cannot_change_replacement_formation_or_counters` checks both completed-result classes at the production consumer, with a current-generation error control. `leave_disposes_real_outgoing_pull_and_reclaims_exchange_capacity`, `shutdown_disposes_real_outgoing_pull_and_reclaims_exchange_capacity`, and `wire_ejection_disposes_real_outgoing_pull_and_reclaims_exchange_capacity` retain a real remote reply stream until owner disposal. | Mapped; consumer fencing and transport cancellation are separate evidence, not interchangeable claims. |
| Pending timers | `leave_clears_real_owner_deadlines_and_fences_late_timer_input` observes the actual armed timer map, cleanup and stale delivery. Core `a_stale_direct_timeout_cannot_touch_a_newer_probe`, `a_stale_suspicion_timer_cannot_kill_a_recovered_member`, `a_stale_join_timer_does_not_retry` and `reconnect_preserves_join_budget_and_fences_old_timer_and_reply` cover token correlation. | Mapped at owner/core boundaries; no claim that every timer kind has been held through every lifecycle permutation. |
| Inline credential verification, ID allocation and peer selection | `queued_policy_changes_order_admission_after_authentication` and `concurrent_authenticated_joins_share_one_local_capacity_slot` exercise the actual serialized owner ordering and gate rechecks. | Mapped as inline work, not deferred IO. Optional verifier reservation tests do not establish a production asynchronous verifier. |
| Admitted-peer outgoing handshake | `runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown` proves cancellation of actual pending dials. `admitted_dial_skips_silent_candidate_and_restores_policy_exchange` and the collision/recovery test prove successful current-generation registration. | **Missing held-success case:** deliver an already successful admitted-peer handshake to `accept_member_handshake_reply` after lifecycle replacement, and observe refusal/disposal. Pending-join reconnect tests use a different completion branch. |

Lifecycle equivalence is limited to the inspected production branches. Adoption
and state-changing leave both change the formation/node pair and execute the
same generation increment, timer clear, send abortion and inline-queue clear
in `Owner::apply`. Ejection has its own cleanup branch and is covered separately
by catch-up and real outgoing-exchange tests. Shutdown disposes the owner and
reserved completions; it is not treated as another successful adoption.
The core adoption tests verify that old membership state is not imported.
These mappings avoid an unbounded Cartesian product without claiming public
leave-during-join, arbitrary churn or a new recovery authority.

### Next bounded regression: late admitted-peer handshake success

- Establish two existing admitted identities and obtain a genuine pinned
  TLS/application handshake result through `Dialer::member_handshake`.
- Retain the successful connection/result before owner registration; replace
  the source lifecycle, then submit through the existing
  `accept_member_handshake_reply` path with the original generation.
- Assert structured stale refusal, bounded connection disposal without
  test-owned closure, unchanged replacement identity/membership and no new
  registered session. Include a current-generation success control. Cover
  owner shutdown separately if the same fixture can exercise its mailbox
  disposal boundary without introducing a production test-only authority.
- Keep the fixture bounded and use real QUIC/TLS/codec. No new public route,
  retry policy, wire profile or asynchronous verifier is required. Change
  production code only if the regression demonstrates a defect.

This audit stops with that concrete missing assertion; the stale-IO matrix row
remains partial until it passes. The other mapped classes need preservation
and final acceptance reruns, not another blanket implementation inventory.

Current dirty-worktree verification **passed**, with no optional worker
features and isolated target directory:

```sh
cargo test --locked --offline -p orishu-worker --lib --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-membership --test swim --test join --target-dir target/formation-flow-observability
```

The worker library passed **118 tests**; membership join and SWIM passed **25
and 41 tests** respectively. No runtime code changed. Worker binary/integration
suites, fault-process journeys, optional telemetry, full workspace checks and
dependency-purity tests were not rerun in this audit. This does not close
N-FORMATION or combined M4.

## Staged tracing resource configuration — 2026-09-07

`spec.tracing` and flattened `--tracing.*` arguments now use one typed,
unknown-field-rejecting configuration. Sampling, queue, batch, flush interval,
export timeout and shutdown timeout have finite validated ranges. Explicit
CLI/environment values overlay file settings; omitted settings do not replace
file values. The [configuration guide](../orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable)
records defaults, ranges and the separate pending active-span/batch memory and
endpoint-security obligations.

This is configuration groundwork, not exporter delivery. `enabled=true`
currently fails before worker state/listeners are created, including when
`observability` is compiled in. No empty exporter feature, endpoint, SDK
dependency or outbound collector activity was introduced. Profile 5 remains
proposed, not approved or activated.

At the current dirty worktree, these checks passed using
`target/formation-flow-observability`:

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker config:: --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --all-targets tracing --target-dir target/formation-flow-observability
cargo test --locked --offline -p orishu-worker --features observability --all-targets tracing --target-dir target/formation-flow-observability
```

The default configuration suite passed 13 tests. Each tracing-filter run
passed two configuration tests and one executable test with five enablement
precedence cases. Disabled cases deliberately reach a later peer-listener
configuration error; they are not evidence of a running exporter. Enabled
cases report the explicit unavailable capability. Both paths verify absence
of state and socket creation. No listener permission was needed for these
startup-refusal tests. Formatting and documentation checks passed.

Endpoint/TLS/credential validation, actual optional export, bounded active spans,
collector responses/outages, cross-peer propagation and the full formation/M4
acceptance matrices remain open. These tests do not replace those deliverables.

## Trace propagation compatibility preflight — 2026-09-07

Current profile 4 requires exactly seven membership-envelope fields. A trace
extension cannot be enabled transparently against that decoder.
`profile_four_refuses_trace_extension_without_changing_domain_decode` now
verifies the current ALPN, successful ordinary Ping decoding and refusal of
added `traceParent` fields containing valid-looking, null or malformed values.
All **eight** `peer::wire::tests` passed via:

```sh
cargo test --locked --offline -p orishu-worker --lib peer::wire::tests --target-dir target/formation-flow-observability
```

This is current-profile compatibility evidence, not trace propagation.
[ADR 0025](../adr/0025-version-peer-trace-context-propagation.md) proposes a
review-gated profile 5, bounded client/peer context, IO-only ownership and
omission of telemetry when packet space is insufficient. Protocol sections and
the observability/formation tasks link the proposal. No ALPN, peer DTO,
authority, exporter feature or runtime tracing behavior changed. Review is
required before activating the proposed peer extension; local exporter
configuration/delivery can proceed independently. Broader goal acceptance
remains open.

## Aggregate client-duration histogram — 2026-09-07

The runtime-metrics-only client accounting now exports
`orishu_worker_client_request_duration_seconds` as a classic histogram with
inclusive bounds 1 ms, 5 ms, 25 ms, 100 ms, 500 ms, 1 s, 5 s and `+Inf`.
The existing cumulative duration counter is retained. Both use the same
microsecond accumulator; completed 4xx/5xx handlers are included, cancellation
and socket delivery are excluded. These buckets distinguish fast handling from
delays approaching the handler budget, not production SLOs or per-route latency.

Eight exclusive atomic counters require one bucket update per completion.
Scrapes sum them cumulatively with saturation, preserving bucket monotonicity
and equality of `+Inf` and histogram count. Sum/count and separate request
counters are not one transactional snapshot. There are no dynamic labels or
new dependencies: only the eight fixed `le` values. The catalogue now has
**40 series**, still below the tested **8 KiB** maximum exposition bound.

`duration_histogram_includes_boundaries_and_excludes_cancelled_work` exercises
every finite boundary and the next microsecond, duplicate bucket accumulation,
the exact duration sum, cancellation and `u64::MAX` saturation. Existing real
service-cancellation and HTTP lifecycle/catalogue tests remain passing.
The real Prometheus harness now validates raw and ingested histogram buckets,
count, finite values and allowed labels, in addition to official parser and
alert-rule checks. Prometheus 3.5.0 ingested all forty series and passed the
three alerts and standalone scraper outage/recovery journey.

Validation at dirty base `9d71b752902a3aaeca376968d8f03424a2361b20`:

```sh
cargo test --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --features observability --all-targets --target-dir target/formation-flow-observability -- -D warnings
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
cargo fmt -p orishu-worker -- --check
make docs-check
```

All passed: **192 worker tests** (117 library, 37 binary, 38 integration),
Clippy, pinned Prometheus, formatting and documentation checks. Initial focused
HTTP tests were denied socket access by the sandbox; the full suite passed
with listener permission. The target retains an observability-enabled worker.
Default-feature, full workspace, three-worker/fault and OTLP matrices were not
rerun for this feature-local increment. Remaining peer latency, tracing,
overhead, secured deployment and formation acceptance requirements stay open.

## Received formation activity metrics — 2026-09-07

The worker now publishes three additional unlabelled process-lifetime counters:
`swim_packets_received_total`, `anti_entropy_packets_received_total` and
`gossip_items_received_total`, all under the `orishu_worker_` prefix. They record
successfully decoded owner-side membership traffic before core processing.
SWIM includes all five probe/announcement variants; anti-entropy includes pull
requests/replies; gossip counts envelope items plus pull-reply deltas. Repeated
or subsequently ignored input counts as received activity, never as successful
probe correlation, new merges, completed rounds or convergence. Collection
lengths are read without iteration/allocation. Counts saturate and remain
independent of optional exporter dependencies.

Evidence includes `received_activity_counts_items_not_merges_and_saturates`,
the exact six-packet count in
`authenticated_reordered_datagrams_preserve_probe_ids_under_replay`, and real
member-traffic assertions in
`real_join_packet_drives_owner_lock_token_and_admission_outcomes`: both owners
receive SWIM/anti-entropy, and wire-delivered ejection gossip increments the
item count. The first unit fixture did not compile because it supplied a
`LocalIdentity` instead of a membership record; it was corrected before tests
passed. No domain validation was relaxed.

The expanded catalogue contains **30 series / 13 owner counters**. Its
documented and tested worst-case scrape bound is now **8 KiB**, up from 6 KiB;
protocol/manual and both scrape harness limits agree. Existing metric meanings
are unchanged. Prometheus 3.5.0 parsed and scraped all 30 series, validated the
three existing alerts, and passed scraper outage/recovery with operator control.

At dirty base `9d71b752902a3aaeca376968d8f03424a2361b20`, with isolated target
`target/formation-flow-observability`, these commands passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability --target-dir target/formation-flow-observability -- -D warnings
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

The observability suite passed **191 tests** (117 library, 36 binary, 38
integration). HTTP/evidence helper suites passed 23 and 6 tests; a repeat HTTP
helper run denied Unix sockets in the sandbox and passed when permitted.
The subsequent default-feature suite passed **179 tests** (117 library, 32
binary, 30 integration), with default-feature Clippy also passing; commands
match the worker commands above with `--features observability` omitted.
`cargo fmt -p orishu-worker -- --check` and `make docs-check` passed.

The full three-process companion also passed:

```sh
python3 scripts/check-formation-cli.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --observability
```

It checks nonzero received activity on every worker within a fifteen-second
observation budget after handoff, then verifies zero activity on fresh standalone
restart before readmission. Existing exact membership, admission counters,
leave/readmission and crash/restart checks remain intact. The process journey
uses private executable snapshots, so the later default build cannot replace
its restart executable. The target directory now contains a default-feature
worker; rebuild with observability before rerunning either scrape command.
The full workspace/fault matrix, send/timeout/latency instruments, tracing,
overhead measurements and remaining M4 handoff are not closed by these counts.

## Owner-side peer decode rejection metric — 2026-09-07

`orishu_worker_peer_decode_rejections_total` now counts membership packets
refused by the owner's session registry/binding decoder before core delivery.
It is one unlabelled, saturating process-lifetime integer, retained across
formation changes and owner closure. Publishing it updates only the aggregate,
not the membership projection or supervision timestamp. It adds no dependency
to the membership core. The catalogue and worker manual specify exclusions:
transport framing, handshake, pre-enqueue limits, stale lifecycle input,
catch-up and admission/replay refusal are not this counter.

The expanded `real_handshake_is_decided_by_owner_and_closed_when_formation_changes`
test sends two invalid membership shapes through real authenticated QUIC and
stream framing, verifies exact increments without admissions, and retains the
count after leave/shutdown. Its first fixture used invalid CBOR, which the
outgoing transport correctly refused before sending; the waiting receiver
then hit the fixture deadline. The corrected inputs are valid CBOR of the wrong
membership shape, so the test reaches the intended owner boundary. This was a
fixture correction, not a decoder runtime fix.

The HTTP catalogue tests cover the new type/sample, retained projection and
worst-case exposition bound. Prometheus 3.5.0 parsed and scraped all **27**
series; three alert examples and operator control during scraper outage and
re-scrape passed. The catalogue remains below 6 KiB, with ten owner counters.

Validation used dirty base `9d71b752902a3aaeca376968d8f03424a2361b20` and
`target/formation-flow-observability`; feature builds were run sequentially:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --target-dir target/formation-flow-observability
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability --target-dir target/formation-flow-observability -- -D warnings
python3 scripts/check-worker-prometheus.py --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --promtool /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool --prometheus /tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

These passed: **190 worker tests** (116 library, 36 binary, 38 integration),
Clippy and the real Prometheus journey. The temporary pinned-tool location is
specific to this checkpoint; the Prometheus guide explains tool provisioning.
The subsequent default-feature worker suite passed **178 tests** (116 library,
32 binary, 30 integration) using the same test command with
`--features observability` omitted. Default-feature Clippy passed with the
same omission. `cargo fmt -p orishu-worker -- --check` and `make docs-check`
also passed. The target directory ends with a default-feature executable;
rebuild with observability before using it for a scrape journey.
Full workspace/fault-process validation, broader transport/SWIM/anti-entropy
instruments, histograms, tracing and the remaining M4 gates are not established
by this counter.

## Stalled original issuer lookup — 2026-09-07

The existing `check-formation-cli.py --source-loss` journey now also inspects
the original issuer while that process is paused after confirmed insertion
and lost ACK. The authenticated CLI lookup fails without a result at its
two-second request timeout (three-second subprocess bound; at least 1.5 seconds
observed). The issuer is still alive. Public source status retains the exact
recovery reference and source/target identities, remains admitting or unresolved,
and has not adopted another formation or assigned identity. No replacement join
is submitted as a consequence of the lookup failure.

The rest of the same process journey then passes: injected source crash,
retained certificate/operator credentials, fresh standalone identities,
authenticated `UnknownOperation`, and original assignment evidence from the
resumed issuer. This establishes failed-lookup uncertainty against a real
stalled issuer, supplementing the controlled CLI response fixtures. It is not
a full 300-second operator observation-cutoff test or proof of all recovery
combinations. Pausing/killing workers is test fault injection, not a runbook
recommendation. The recovery manual now describes this assertion.

At dirty base `9d71b752902a3aaeca376968d8f03424a2361b20`, these commands passed:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --source-loss
python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --lost-join-ack
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
```

The helper suites passed 23 and 6 tests respectively. The real process journey
ran with permission for local sockets; executable snapshots are private to the
journey, including restarts. Observability is not enabled in this build.
The full lost-ACK rerun recovered the original assignment and passed introducer
handoff, leave/readmission, crash/restart and final exact membership checks.
`make docs-check` passed for 115 Markdown files. No Rust production code changed;
the other fault modes, full workspace and combined M4 feature matrix were not
rerun here and remain required for final acceptance.

## CLI admission-inspection correlation — 2026-09-07

`inspection_preserves_evidence_and_refuses_uncorrelated_reports_without_mutation`
in `apps/orishu-ctl/tests/admission_inspection.rs` invokes the actual CLI against
a controlled Unix HTTP/CBOR responder. Twelve cases cover current membership,
missing history, retired/restricted assignment, a restarted/wrong issuer,
mismatched attempt, source formation, source node, applicant fingerprint and
issuer fingerprint, unsupported report version, HTTP lookup failure and timeout.

Each case verifies the exact read-only inspection request and operator bearer,
preserves the complete valid JSON report, and checks that no second connection
or request follows. Invalid/failed responses produce a nonzero exit, an error
and no result output. Both output streams are checked for the seeded credential.
Successful inspection of unknown history or the wrong issuer remains a report,
not admission recovery. The timeout case retains the socket without replying;
the CLI's one-second request budget expires, bounded by a five-second fixture
deadline. Cleanup kills/reaps the CLI on assertion failure too.

This supplements the shared client's correlation tests and the unavailable-
issuer-evidence recovery row. It does **not** close the interrupted-admission
process journey, issuer retention/restart behavior, TLS targeting or the final
operator stop-procedure audit. No production behavior or wire contract changed.
The only dependency change is a test-only reference to the already-resolved
`ciborium` 0.2.2, with the corresponding CLI lockfile dependency entry.

Validation at dirty base `9d71b752902a3aaeca376968d8f03424a2361b20`, default CLI
features, isolated target `target/formation-flow-observability`:

- `cargo test --locked --offline -p orishuctl --all-targets --target-dir target/formation-flow-observability` — passed, six unit and two executable tests; the new executable test covers twelve cases.
- `cargo clippy --locked --offline -p orishuctl --all-targets --target-dir target/formation-flow-observability -- -D warnings` — passed.
- `cargo fmt -p orishuctl -- --check` — passed.

The initial focused test could not bind its Unix socket in the sandbox; the
permitted focused rerun and full CLI suite passed. Cargo reports an existing
future-incompatibility warning for `proc-macro-error2` 2.0.1. Full workspace,
worker/fault-process and observability matrices were not rerun for this
test-only increment; their closure requirements remain unchanged.

## HTTP/2 response delivery deadlines — 2026-09-07

The new real-worker regression
`http2_withheld_response_credit_expires_despite_pings` first failed at its
seven-second observation deadline: a fully received summary request produced
response HEADERS, but zero stream credit withheld DATA while PING ACKs kept
the connection active. The response was outside both the handler deadline
and a pending socket write. This is the pre-fix counterexample, not a passing
idle-timeout result. It passed at about five seconds after the correction.

The plaintext IO observer now tracks at most sixteen response stream IDs and
absolute deadlines, sharing the existing connection-owned watchdog. A response
has five seconds from its first outgoing HEADERS byte to complete END_STREAM;
END_STREAM HEADERS with continuations wait for the final END_HEADERS payload.
Inbound/outbound reset cancels its response budget. Partial outgoing frames
also retain an absolute first-byte deadline, covering headers before stream
identification. Neither DATA progress, window updates nor other traffic renews
these budgets. The earliest pending budget closes the offending connection,
not the membership owner. No response-body copy, exporter dependency or
per-response task was added. Fixed metadata exhaustion fails closed.

The [client ingress/delivery contract](../protocol-client.md#formation-client-ingress-limits)
and worker manual specify whole-connection expiry, TLS buffering/receipt
limitations and unchanged operation-status/replay obligations. This is finite
PoC response delivery, not a maximum connection age or an assertion of remote
receipt. Future long-lived response routes require their own reviewed contract.

Evidence:

- `http2_withheld_response_credit_expires_despite_pings`: real executable,
  Unix HTTP/2, zero stream credit, repeated PING ACKs, expiry while the client
  remains open, then independent summary progress.
- `http2_exhausted_connection_credit_expires_despite_pings`: first consumes
  exactly 65,535 connection DATA bytes with finite valid summary requests,
  retaining full per-stream credit. Two responses then remain incomplete;
  PINGs cannot prevent expiry. Its shared fixture preserves the positive
  connection-credit restoration test.
- `tls_http2_withheld_response_credit_expires_despite_pings`: pinned server
  certificate, negotiated `h2`, valid operator credential and HTTP 200 response
  HEADERS, then stream-credit stall and expiry despite outgoing PINGs. The
  fixture proves authenticated TLS summary access again afterward.
- `http2_reset_and_completed_response_cancel_delivery_deadlines`: cancel the
  blocked response, complete another, and continue using that same connection
  beyond both old deadlines. Existing capacity/recovery and long-lived
  multiplexing controls remain passing evidence.
- Four `client_assembly::delivery::tests` cover fixed capacity, cancellation
  and reuse, fragmented output, partial END_STREAM payloads, continued final
  headers, immutable deadlines across DATA/trailers/other streams and the
  seventeenth pending response. The watcher remains independent of IO polling.

The initial failing and corrected focused command used the isolated target
directory to avoid shared executable replacement during concurrent work:

```sh
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --test standalone http2_withheld_response -- --nocapture
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --test standalone credit_expires -- --nocapture
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --test standalone tls_http2_withheld -- --nocapture
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --test standalone delivery_deadlines -- --nocapture
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --all-targets --quiet
cargo clippy --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --all-targets -- -D warnings
```

The corrected focused tests and full observability suite passed: **190 tests**
(116 library, 36 binary, 38 executable integration), followed by observability
Clippy. Checkpoint: dirty tree on base
`9d71b752902a3aaeca376968d8f03424a2361b20`. Feature builds run sequentially in
this directory; it is an isolated validation output, not a published artifact.

The default-feature suite then passed **178 tests** (116 library, 32 binary,
30 executable integration), and default Clippy passed. The fixture's PING
reader retains partially received frame headers across timer ticks; expected
server closure racing a PING write is also accepted, while the lower timing
bound and prior PING evidence still reject premature disconnects. These
fixture cleanup changes do not alter production deadlines. Worker formatting
and `make docs-check` passed (115 Markdown files).

```sh
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --all-targets -- -D warnings
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --test standalone http2_ -- --nocapture
```

The final filtered run passed all sixteen HTTP/2 executable tests after the
fixture cleanup, including both expiry scopes, TLS, reset/completion,
assembly, credentials, capacity and recovery. No temporary debug logging was
added. The diagnose loop established the pre-fix failure before runtime edits;
the correction targets transport delivery rather than treating handler
completion or unrelated connection traffic as delivery progress.

These results address the named response-credit timing gap, not all formation
conformance or M4 acceptance. The complete lifecycle/recovery audit, final
fault-process journeys, broader telemetry/security/tracing and operator handoff
remain required. No test-only fault-build or workspace-wide Rust validation is
claimed by this increment.

## Inbound HTTP/2 flow-control boundaries — 2026-09-07

For subsequent response-credit expiry evidence, see the
[delivery follow-up](#http2-response-delivery-deadlines--2026-09-07).

`client_flow_tests` now exercises the production client-server wrapper and
pinned HTTP/2 decoder over actual Unix sockets. A test-only handler holds
request bodies unread, so application consumption cannot replenish receive
credit and obscure the exact boundary. This is a wire/server fixture, not
an authenticated operator, TLS-handshake or multi-process membership journey.
No production limit, runtime behavior, dependency or wire format changed.

| Named test | Evidence and limit |
| --- | --- |
| `connection_receive_credit_accepts_exact_limit_then_refuses_one_byte` | Production 65,535-byte connection/stream windows; two streams retain 32,768 and 32,767 bytes. A PING ACK proves exact-limit acceptance. Empty END_STREAM is valid at zero connection credit. One further byte on the other stream produces GOAWAY `FLOW_CONTROL_ERROR`, then server-side transport closure. Neither individual stream exceeded its limit. |
| `stream_receive_credit_is_independent_of_connection_credit` | The fixture alone raises connection credit to 131,070, explicitly verified on the wire, retaining the production 65,535-byte stream limit. Exact-limit DATA succeeds; one extra byte resets only that stream with `FLOW_CONTROL_ERROR`. The connection still answers PING and serves HTTP 200 on a new stream. This is an isolation configuration, not the worker's default connection window. |
| `invalid_window_updates_have_stream_and_connection_error_scope` | Legal increments and the reserved-bit control succeed. Window overflow yields GOAWAY for connection scope and RST_STREAM for stream scope. Zero increments on either scope and a three-byte update payload yield GOAWAY `PROTOCOL_ERROR`. Stream resets preserve another request; connection errors close the retained client. |

Every case also proves a separate connection can obtain HTTP 200, with a
bounded response. Each fixture has a six-second whole-journey timeout,
16 KiB maximum frame allocation, bounded follow-up frame loops and explicit
server shutdown. The bodies remain unread by construction; no fake credit
event, direct decoder invocation or membership-core call manufactures refusal.

Initial fixture failures were retained during development: the enlarged-window
handshake assumed SETTINGS ACK could not precede WINDOW_UPDATE; it can. The
zero-increment stream case initially expected RST_STREAM, and the short update
expected FRAME_SIZE_ERROR. Pinned decoder inspection and
[RFC 9113 section 5.4](https://www.rfc-editor.org/rfc/rfc9113.html#section-5.4)
confirm connection escalation and a generic PROTOCOL_ERROR are permitted.
The corrected tests assert those exact outcomes, rather than accepting any
disconnect as success. Both DATA boundary cases passed without runtime fixes.

The focused command and default worker Clippy passed on the dirty tree at base
`9d71b752902a3aaeca376968d8f03424a2361b20`, using the default `target` directory:

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker client_flow_tests -- --nocapture
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The three focused tests cover seven refusal cases and their valid controls.
The complete default suite also passed: 170 tests (116 library, 28 binary,
26 executable integration). The initial observability run passed its 116
library and 32 binary tests, but ten of 34 executable tests failed because
the worker subprocess reported `diagnostics require the observability build
feature`. This is feature/executable mismatch evidence at the shared
`target/debug/orishu-worker` path, not a flow-control regression. A subsequent
validation uses an isolated output directory instead of retrying that shared
path:

```sh
cargo test --locked --offline --target-dir target/formation-flow-observability -p orishu-worker --features observability --all-targets --quiet
```

The isolated run **passed all 182 observability tests** (116 library, 32
binary, 34 executable integration), including the ten previously mismatched
subprocess cases. No shared executable replacement occurred in that directory.
Worker formatting, scoped whitespace and `make docs-check` passed (115
Markdown files). Workspace-wide Rust checks, release/fault builds and separate
formation process harnesses were not rerun for this test-only increment.

This closes the named decoder-level inbound DATA/window-update checks, not
the entire ingress row. Withheld-response-credit deadlines remain open;
neither inbound refusal nor assembly expiry proves that temporal property.
Final formation fault journeys and the broader M4 gates remain required.

## Workspace validation refresh — 2026-09-07

The task's default workspace gates were rerun against the current dirty tree
on base `4d6cfba4021e7feeb988c525c7f6fac03150a2ff`, including the recent admission
instruments, catch-up HTTP test and the concurrent workspace additions. This
is a checked integration checkpoint, not final acceptance of unfinished work.

Passed:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc
make docs-check
```

Default all-target tests included the worker's 151 tests, shared client and
membership tests and dependency-purity checks. Criterion targets ran smoke
cases, not performance measurements; absent Gnuplot selected the plotters
backend. Four documentation examples passed; the existing `query_filter!`
example remains explicitly ignored, not verified. Clippy reported the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning, not a denied lint.
Documentation validation passed for 115 Markdown files.

After the default executable tests finished, the combined optional build also
passed `cargo test --locked --offline -p orishu-worker --features
observability,formation-fault-test --all-targets --quiet`: 165 tests (113
library, 24 binary, 28 executable integration), including stalled-owner HTTP
supervision and the new catch-up regression. Clippy with the same feature pair
and `--all-targets -- -D warnings` passed. This is the existing debug fault
feature plus metrics/probes, not the still-unimplemented OTLP feature matrix.

Tests used local socket permission and the default `target` directory. No
implementation or unrelated workspace edits were made by this validation.
Older dated validation sections below remain historical; they do not supersede
this checkpoint. This does not rerun the separate fault-process journeys,
prove test hooks absent from releases, close the stale-IO/HTTP2/recovery audit,
or implement secured monitoring and OTLP. Final acceptance must rerun applicable
gates after the remaining changes and preserve those missing requirements.

## Issuer admission metrics — 2026-09-07

The dirty-worktree worker now publishes three fixed issuer counters: local
core insertion, local core refusal (including redirects), and validated retained
assignment replay. New insertion/refusal counts consume actual core publications;
replay increments only after validation, encoding and session promotion. The
early replay return publishes its updated view without refreshing owner health.
No wire schema, dependency, membership authority or runtime default changed.

`real_join_packet_drives_owner_lock_token_and_admission_outcomes` and
`queued_policy_changes_order_admission_after_authentication` exercise actual
QUIC requests and owner decisions. Assertions distinguish seeded membership from
new admission, valid replay from insertion, and malformed/conflicting/retired
retries from core refusals. Counts survive a real owner leave/replacement and
remain readable after shutdown. This is wire/owner evidence, not a new
three-process telemetry journey.

Validation at this checkpoint:

- `cargo test --locked --offline -p orishu-worker --features observability
  --all-targets --quiet`: passed 162 tests (113 library, 21 binary, 28 executable
  integration), before the additional replacement/retention assertions.
- `cargo test --locked --offline -p orishu-worker --lib
  peer::registry::tests:: --quiet`: the sandbox run failed all ten tests at
  socket creation (`Operation not permitted`); the same command with socket
  permission passed all ten, including the additional retention assertions.
- `cargo clippy --locked --offline -p orishu-worker --all-targets --features
  observability -- -D warnings`: passed after the retention assertions.
- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`:
  passed all 151 default-feature tests (113 library, 18 binary, 20 executable
  integration) after the retention assertions, with socket permission.
- `cargo fmt --all -- --check` and scoped whitespace checks passed;
  `make docs-check` passed (114 Markdown files at the final check).
- `make test-worker-prometheus` with `PROMTOOL` and `PROMETHEUS` under
  `/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/`: passed
  actual ingestion of all 26 series, three alert rules and standalone control
  progress across scraper outage/restart. The test builds the observability
  worker/CLI in `target/debug`; tools are the pinned versions in the test guide.

Real HTTP tests check names/types and a conservative maximum-size projection
under the existing 6 KiB bound. Positive admission values are established by
the wire/owner fixture; the standalone Prometheus journey does not prove
three-worker positive admission scraping. Full fault-process reruns, optional
trace/security matrices, overhead measurements and workspace-wide final
acceptance remain open. See the updated catalogue and operator instructions;
this increment does not close P-OBSERVABILITY or N-FORMATION.

## Three-process admission scrapes — 2026-09-07

The shared public formation harness now has an opt-in `--observability` mode.
It enables three loopback diagnostics listeners, retaining production
QUIC/mTLS, operator authentication, exact membership assertions, bounded
failure capture and the full leave/readmission/crash/restart journey. After
A admits B and B admits C, actual HTTP scrapes must report insertion counts
`[1, 1, 0]` and no core refusals. Restarting B must reset all three admission
counters. The lost-ACK variant additionally requires A's assignment-replay
counter to be positive without a second insertion; B/C cannot claim replay.
Operator and join credentials are excluded from the bounded scrape response.

Commands and results in this dirty-worktree checkpoint:

- `make test-formation-observability`: passed the full journey with the
  observability feature. Its first sandbox execution failed before formation
  at peer socket creation (`Operation not permitted`), recorded in
  `/tmp/orishu-formation-failure-9gjrpime`; the socket-enabled run passed.
- `make test-formation-observability-lost-ack`: passed the full journey with
  both observability and the debug-only formation fault feature, including
  the additional join-token redaction assertion added after the ordinary run.
- `python3 scripts/test_formation_http.py`: 21 tests passed with socket
  permission, including refusal of partial/unmapped observability scenario
  combinations before startup. A sandbox run failed nine socket-dependent
  cases before their assertions; that was not a passing test run.
- `python3 scripts/test_formation_evidence.py`: six tests passed.
- `make test-formation`: passed after both telemetry journeys, rebuilding
  ordinary worker/CLI binaries without either optional feature and running
  all 27 helper/evidence tests plus the full public process journey.
- `make docs-check`: passed (114 Markdown files); scoped whitespace passed.

Feature builds ran sequentially against `target/debug`. These new targets
perform direct HTTP reads, not multi-target Prometheus ingestion. The separate
Prometheus test supplies backend validation. Positive core-refusal counters
remain established by the wire/owner fixture, not this successful admission
journey. Full probe transitions, secured exposure, telemetry overload/outages,
OTLP and measured overhead remain companion acceptance gaps. No worker code,
protocol, dependency, admission rule or default exposure changed in this
harness increment. The overall formation and M4 gates remain open.

## Process probes across wire ejection — 2026-09-07

`make test-formation-observability-ejection` passed with the `observability`
and debug-only `formation-fault-test` features in `target/debug`. This adds
real HTTP probe assertions to the existing public-process peer-ejection
journey, without a health override or a worker behavior change:

- After B completes authenticated admission, all three initialized processes
  report `200` for liveness, readiness and latched startup.
- A sends the existing valid wire tombstone fixture; B reports ejected through
  the public operator API and stops old-formation participation. After survivor
  SWIM observes B as dead, B reports live `200`, ready `503`, startup `200`.
- A remains live/ready/startup-complete despite losing B. B's metrics remain
  readable without fabricating local admission events.
- Explicit authenticated leave returns B to fresh standalone identity and all
  three of its probes return `200`; no restart or automatic readmission is used.

Each probe read uses a two-second socket timeout and a 1 KiB body bound;
failure evidence records route/status, not payloads. The harness explicitly
rejects combining ejection and lost-ACK scenarios. The HTTP/helper regression
suite passed 22 tests with local socket permission. This scenario does not
exercise C's handoff, subsequent crash/restart, transient joining/catch-up,
or cluster-wide administrative removal convergence. Those scopes remain
separate; this evidence does not close the full probe matrix or M4 gate.

After the optional ejection journey, `make test-formation` rebuilt ordinary
worker/CLI binaries and passed all 28 helper/evidence tests plus the full
public handoff/leave/crash/restart/readmission journey. Builds were sequential.
`make docs-check` passed (115 Markdown files), as did scoped whitespace checks.
Workspace-wide Rust and other fault/telemetry feature matrices were not rerun
for this harness-only increment.

## Unresolved-admission probes and executable isolation — 2026-09-07

`make test-formation-observability-issuer-loss` now checks real HTTP on the
surviving applicant after the existing post-insertion issuer crash and again
after retry exhaustion. The source remains live (`200`) but unready (`503`),
with startup latched (`200`); unrelated standalone C remains ready. Source
admission counters remain zero. The existing exact retry, unsafe-new-attempt
and leave refusals, wrong-issuer inspection and test-induced source-history
loss assertions remain intact. No retry deadline or health state was overridden.

The first run **failed** after passing those probe assertions, when the issuer
restart reported `diagnostics require the observability build feature`.
Evidence is retained at `/tmp/orishu-formation-failure-iisqv79s`. Inspection
confirmed a compile-time feature check and reuse of the same executable path
for every launch: a replacement build could change restart capabilities even
though the original live process had served diagnostics.

The harness now copies worker and CLI bytes into a private temporary directory
at entry and uses those same copies for every subprocess/restart. It never
hard-links mutable build outputs, and normal context cleanup removes copies.
`ExecutableSnapshotTests` first failed before the helper existed, then passed
its source-replacement/retained-byte and executable-permission assertions.
This prevents mid-journey replacement; intended input builds must still be
selected without a race at initial capture.

The original issuer-loss command then **passed**, including real retry
exhaustion and both history-loss restarts. During its wait, `make test-formation`
rebuilt the shared default-feature output and also passed all 29 helper/evidence
tests and the full ordinary process journey. The two running journeys used
separate private executable paths, directly exercising replacement protection.
The optional build used `observability,formation-fault-test`; the ordinary
build used neither. The unchanged dependency future-incompatibility warning
remains. No debug logs or temporary production probes were added.

This is admission-uncertainty/probe and harness-isolation evidence, not
catch-up completion, secured telemetry or full M4 acceptance. Worker runtime
code and protocol behavior were unchanged. The fault-driven source restart is
not an operator recovery recommendation. Workspace Rust and the remaining
fault/exporter matrices were not rerun for this harness change.

## HTTP readiness across real catch-up — 2026-09-07

`diagnostics::tests::http_readiness_waits_for_real_admission_catchup` passed.
Two real worker runtimes bind peer listeners and admit over pinned QUIC. The
source's production diagnostics router is served over Unix HTTP. After local
initialization it reports live/ready/startup success. Admission adopts the
target formation and inserts the source, but catch-up maintenance is not yet
started: the source cannot introduce and HTTP readiness is false while
liveness and startup remain successful. Starting normal maintenance transfers
and validates the baseline, reaches Joined/introducer-ready, and restores HTTP
readiness. Owners and the HTTP server shut down normally within the whole
15-second fixture budget; no synthetic health-state override is used.

Validation in this dirty worktree:

- `cargo test --locked --offline -p orishu-worker --features observability
  --bin orishu-worker http_readiness_waits_for_real_admission_catchup -- --nocapture`:
  passed, approximately one second.
- `cargo test --locked --offline -p orishu-worker --features observability
  --all-targets --quiet`: passed 163 tests (113 library, 22 binary, 28 executable
  integration), with local socket permission.
- `cargo clippy --locked --offline -p orishu-worker --all-targets --features
  observability -- -D warnings`, `cargo fmt --all -- --check` and
  `make docs-check`: passed (115 Markdown files).

This is real wire/runtime/HTTP evidence in one test process. Deferring
maintenance is not holding a partial transfer on the wire; interrupted,
expired or invalid catch-up HTTP cases remain separate. No production code,
default, dependency or protocol changed. Full workspace/default/fault-process
and remaining telemetry matrices were not rerun for this test-only increment;
the full formation and M4 gates remain open.

## Real outgoing exchange disposal on leave — 2026-09-07

`peer::readmission_timing_tests::leave_disposes_real_outgoing_pull_and_reclaims_exchange_capacity`
passed through real pinned QUIC, authenticated member handshake, owner-driven
anti-entropy and an outgoing reliable exchange. The remote fixture receives
the actual PullRequest and retains its reply stream. With exchange capacity
observably occupied and the session still open, a normal owner leave changes
formation. Within one second the old connection closes and all 64 exchange
slots are available; writing the held old reply fails. Replacement membership
contains only the new local identity, with no retained anti-entropy round and
no old-reply/failure counter increase. A subsequent real owner lock succeeds,
followed by normal owner/dispatcher shutdown. No test-owned cancellation or
remote connection close manufactures reclamation.

The focused command passed (approximately 15 seconds, within the 20-second
fixture budget):

```sh
cargo test --locked --offline -p orishu-worker --lib leave_disposes_real_outgoing_pull -- --nocapture
```

This fills the in-flight **owner outgoing-send cancellation on leave** boundary,
complementing `old_send_results_cannot_change_replacement_formation_or_counters`,
which tests already-completed success/error consumption. These are different
races. The setup seeds already-admitted models; it is not public admission,
every lifecycle combination, or proof of full stale-IO conformance. Production
behavior and protocol contracts are unchanged.

The default worker all-target suite subsequently passed all 152 tests
(114 library, 18 binary, 20 executable integration) using
`cargo test --locked --offline -p orishu-worker --all-targets --quiet` with
local socket permission. Default worker Clippy with `--all-targets -- -D warnings`,
workspace formatting and `make docs-check` passed (115 Markdown files).
Optional features, full workspace Rust and separate process/fault journeys
were not rerun for this test-only change.

## Real outgoing exchange disposal on shutdown — 2026-09-07

The outgoing PullRequest fixture now also covers normal owner shutdown, using
the same real pinned/application-authenticated peer and occupied exchange pool
as the leave regression. `shutdown_disposes_real_outgoing_pull_and_reclaims_exchange_capacity`
holds the remote reply stream open, then requires owner shutdown, owner task
completion, dispatcher completion and remote connection closure within one
second. All 64 exchange slots are reclaimed; the held reply can no longer be
written and no successful-reply or failed-send event is fabricated. The test
does not abort a task or close the peer connection to establish those results.

`cargo test --locked --offline -p orishu-worker --lib disposes_real_outgoing_pull
-- --nocapture` passed both leave and shutdown cases (approximately 15 seconds,
including waiting for the real owner reconciliation cadence). Default worker
Clippy with `--all-targets -- -D warnings` passed. This extends the IO disposal
mapping, while the existing stale-result consumer test separately covers
already-completed success/error results. Ejection/adoption and other lifecycle
classes still need their final cross-class audit; neither this fixture nor an
incoming-stream shutdown test closes the whole matrix. No production behavior
or public protocol changed.

The full default worker all-target command subsequently passed 153 tests
(115 library, 18 binary, 20 executable integration), with local socket
permission. Workspace formatting, whitespace checks and `make docs-check`
passed (115 Markdown files). Optional feature suites, full workspace Rust and
separate fault-process journeys were not rerun for this test-only increment.

## Outgoing exchange disposal on authenticated ejection — 2026-09-07

`wire_ejection_disposes_real_outgoing_pull_and_reclaims_exchange_capacity`
passed alongside the leave and shutdown variants of the shared real-QUIC
fixture. While the owner awaits a remote PullReply, its authenticated peer
sends an encoded Ping carrying an identified, non-deferred removal tombstone.
Within one second the receiver closes that connection and reclaims all 64
exchange slots. Its generation advances and participation becomes Ejected,
with introducer readiness false; formation and assigned identity remain the
ejected identity, rather than silently becoming standalone. A late write on
the held response fails, and neither reply nor send-failure counters increase.
Normal owner and dispatcher shutdown completes afterward.

The three-case command passed in approximately 15 seconds:

```sh
cargo test --locked --offline -p orishu-worker --lib disposes_real_outgoing_pull -- --nocapture
```

This completes explicit outgoing in-flight cancellation evidence for leave,
shutdown and wire self-ejection, complementing the already-completed
success/error consumer test. It is not an administrative removal API or
cluster-wide removal convergence. Admission/adoption uses its distinct
pending-join lifecycle and remains part of the final cross-class mapping;
the whole stale-IO row is not closed by this fixture. No production behavior,
dependency or protocol changed.

The default worker all-target suite passed all 154 tests (116 library,
18 binary, 20 executable integration) using local socket permission. Default
worker all-target Clippy with warnings denied, workspace formatting, whitespace
and documentation checks passed (115 Markdown files). Full workspace Rust,
optional feature matrices and separate process journeys were not rerun for
this test-only change.

## Release fault-feature exclusion gate — 2026-09-07

The existing non-debug compile guard was exercised directly:
`cargo check --locked --offline --release -p orishu-worker --features
formation-fault-test` failed specifically with `formation-fault-test is forbidden
in release builds`, while the same normal release check without that feature
passed. `make test-formation-release-guard` now automates both checks, requiring
the exact compiler diagnostic rather than treating any failed build as proof.
Four helper tests cover the intended diagnostic, unrelated compile failure,
unexpected fault-build success and broken normal release; all passed. The CI
quality job now invokes this target; local validation passed, but no remote CI
run is claimed here.

The new ordinary executable regression tries all six current formation fault
flags individually, requires CLI unknown-argument exit 2 within two seconds,
and proves no state directory/credentials or client socket was created. Its
initial implementation failed compilation because Tokio process support is
not enabled; it was corrected to reuse the existing child cleanup wrapper,
without adding a dependency. The corrected focused test and the full default
worker suite passed: 155 tests (116 library, 18 binary, 21 executable integration).
Default worker Clippy, worker-only formatting, documentation validation
(115 Markdown files) and scoped whitespace checks passed.

Workspace formatting did **not** pass at this checkpoint: concurrent work
temporarily referenced missing `crates/kagami-catalog/src/quantity.rs` and had
format differences in `crates/orishu-variables`. Those unrelated files were
not changed by this increment. This is distinct from the passing earlier
workspace checkpoint; no fresh workspace-wide green result is claimed.

The release check establishes the repository's normal release configuration
and the existing compile guard. It does not execute a published release
artifact or cover custom profile/debug-assertion overrides. Fault-process
journeys, optional telemetry, packaging and final conformance remain separate
acceptance obligations. The full task remains open.

## Active HTTP/2 frame deadline counterexample — 2026-09-07

Historical pre-fix result; see the [assembly watchdog follow-up](#absolute-http2-assembly-watchdog--2026-09-07)
for the subsequent implementation and expiry regressions.

`http2_active_partial_frame_outlives_connection_idle_budget` established the
remaining temporal gap through the actual worker Unix HTTP/2 listener. After
the preface/settings exchange, it declares a 16 KiB HEADERS frame but supplies
only twelve bytes at one byte per second. The connection remains open beyond
the ten-second inactivity budget; legitimate operator summary reads still
work. The incomplete request never reaches a handler. This is a finite
counterexample with an 18-second whole-fixture bound and at most 8 KiB of
received output, not a workload or remote load test.

The initial fixture failed before serving because its socket directory lacked
the required private permissions. After setting mode 0700, the focused command
passed in approximately 12 seconds:

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_active_partial_frame -- --nocapture
```

**The required ingress behavior is still missing.** Passing this diagnostic
means the gap reproduced, not that temporal conformance passed. The pinned
server exposes an HTTP/1 head timeout and transport idle/write timeouts, not
an HTTP/2 assembly deadline. The next implementation must put an absolute
frame/header-block bound at the decoder/transport boundary, document exact
semantics for active traffic and multiplexing, then replace the survival
assertion with server-driven expiry. A handler semaphore, silent-idle timeout,
continuation-count test or broad connection-age cap is not equivalent evidence.
Preserve legitimate HTTP/2 traffic, transport security and independent control
progress when implementing that bound. No runtime change was made here.

The default worker suite passed 156 tests (116 library, 18 binary,
22 executable integration), including this diagnostic counterexample. Default
worker Clippy, worker formatting, scoped whitespace and documentation checks
passed (115 Markdown files). This green suite does not close the explicitly
reproduced ingress gap. Workspace-wide Rust, optional features and separate
fault-process journeys were not rerun in this audit.

## Current process regression requiring resolution

**Current disposition: observation budget corrected; historical failures retained.**
The [admission-baseline audit](#admission-baseline-condition-audit--2026-09-06)
and later reproduction failed the ten-second post-leave readmission assertion.
Passing retries did not establish their precise cause. The
[controlled timing follow-up](#controlled-departure-round-and-readmission-budget--2026-09-07)
now demonstrates that this cutoff was insufficient for an outstanding round
against a departing peer, followed by the next periodic reconciliation.
The process harness uses a seventeen-second observation budget derived from
the unchanged runtime bounds, retaining the exact-record assertions. This
reconciles the acceptance budget; it is not a runtime fix or a claim that the
historical executions had the same internal timing. Final N-FORMATION still
requires the remaining matrix and final process validation.

The [readmission reproducer](#readmission-reproducer-and-diagnostic-reruns--2026-09-06)
provides a shorter public-process loop using the same current assertions and
budget as the full journey. Additional passing reruns did not establish a root cause.
The [bounded reproduction batch](#readmission-failure-reproduced-with-role-evidence--2026-09-07)
has now reproduced the same failure and confirms that the original introducer
is missing the readmitted identity, rather than merely reporting it non-live.

## Current workspace validation — 2026-09-06

After the silent-client idle-deadline change, all task-required default
workspace validation commands were rerun against the working tree and
**passed**:

```sh
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc --quiet
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

The all-target run includes the shared client, membership dependency-purity
tests, worker executable integration tests and CLI tests, not only the worker
library. Worker targets passed 133 tests (100 library, 16 binary, 17 integration).
Criterion targets ran smoke cases, not performance measurements; missing
Gnuplot selected the plotters backend. Documentation validation checked 90
Markdown files. Workspace doc tests passed two examples and explicitly ignored
one existing `query_filter!` example in `crates/orishu/src/model/mod.rs`; that
ignored example is not executable evidence. Clippy still reports the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning, not a denied lint.

The ordinary three-worker CLI journey and default/fault-feature worker suites
were separately rerun in the [idle-deadline follow-up](#silent-client-connection-expiry--2026-09-06).
The workspace commands above do not select optional telemetry, run the separate
fault-process journeys, verify release exclusion of test hooks, or prove the
remaining acceptance matrix complete. Missing cases remain open. This is a
current integration baseline; rerun final validation after subsequent changes.

The final documentation recheck in this turn **failed** after an unrelated,
untracked `docs/tasks/kagami/README.md` appeared. Its eight relative task links
refer to missing files; inspection found only the index in that directory.
The earlier 90-file check passed before that concurrent addition. No Kagami
files were changed by this audit. Thus workspace Rust validation passed, but
the latest repository-wide documentation result is failed, not green. Scoped
whitespace checks for this audit's two edited documentation files passed.

## Initial audit verification — historical checkpoint

These commands were rerun at the initial audit checkpoint and **passed**:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-membership --all-targets --quiet
```

The worker suite passed 110 tests. Membership passed 205 tests, including its
three dependency-purity tests, plus benchmark smoke cases. Benchmark smoke
success is not a performance or scaling measurement.

Process journeys below have recorded passing results in the owning task;
they were **not rerun in this audit**. Their commands remain reproduction
instructions, not new verification. Optional telemetry, full-workspace tests
and missing scenarios were not run. A partial row remains open even when its
supporting unit/wire tests pass.

## Required failure matrix

This case mapping retains each row's original evidence boundaries and historical
rerun notes. The [final process batch](#final-process-validation-batch--2026-09-07)
has now completed those formation reruns; the
[final disposition](#final-formation-acceptance-disposition--2026-09-07) governs
current acceptance. Historical failures and narrower fixture limits remain
evidence, not a new remaining-work list.

| Task scenario | Inspected evidence and boundary | Current disposition / missing evidence |
| --- | --- | --- |
| A introduces B; B introduces C | `check_public_adoption` asserts exact identities, certificates, liveness, completed catch-up and introduction through B. `make test-formation` is the ordinary-process reproduction command. | **Passed, recorded process evidence.** Preserve in final acceptance rerun. |
| ACK lost after insertion; operator retries | The [history-loss audit](#recovery-outcome-and-history-loss-audit--2026-09-07) maps source/issuer history, restriction and inspection outcomes with seven passing fault journeys. The [issuer-ejection process case](#original-issuer-ejection-and-unavailable-accepted-history--2026-09-07) now verifies accepted history loss without issuer identity change. | **Passed at the mapped process and CLI/wire boundaries.** Identity-matching `recordUnavailable` retains the source's uncertainty through bounded exhaustion, just as restart/lookup failure cannot authorize another admission. Preserve these cases for final acceptance; no claim of durable history or arbitrary fault combinations. |
| Concurrent joins and final capacity slot | `concurrent_allocations_recheck_certificate_capacity_and_lock_before_insertion` exercises core interleavings. `concurrent_authenticated_joins_share_one_local_capacity_slot` releases two real authenticated JoinReq exchanges together through the production dispatcher and owner. | **Passed at core and real wire/owner boundaries.** Exactly one accepted identity/certificate and one `CapacityExhausted` reply, with two records including the introducer. This establishes local capacity, not global slot reservation or multi-process operator-request serialization. |
| Simultaneous member dials; connection loss while both processes live | `simultaneous_admitted_dials_recover_crossed_connections_without_readmission` forces crossed admitted handshakes, two duplicate rejections, automatic repair, then a second disconnect and repair. The existing catch-up lifecycle fixture also covers ordinary transport loss. | **Passed at real runtime/wire boundary.** Five consecutive focused runs and the default worker suite passed in the follow-up below. Two owners share one process; the joiner is admitted but still catching up. Not partition/heal, fleet-scale or arbitrary-topology evidence. |
| Unreachable/stale advertised endpoints | Silent-first fallback/policy exchange, four-real-attempt adapter saturation, missing-route failure tests, and `runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown`. | **Passed at bounded adapter/runtime boundaries.** Runtime-owned maintenance emits to four silent routes, fills the shared dial budget, then leave/generation change or shutdown cancels it without test-owned task abortion. This is not fleet churn, DNS discovery or arbitrary topology evidence. |
| Lock during credential verification | Core interleaving test above; `queued_policy_changes_order_admission_after_authentication` adds real-wire lock-before-join, unlock-before-join and lock-after-acceptance cases. | **Passed at current core and wire/owner boundaries.** Verification and allocation run inline in one owner turn. The test queues control and decoded peer input without yielding, using the existing control-lane priority, not a new asynchronous verifier. Distributed partition policy remains a separate row. |
| Concurrent lock/unlock; temporary partition | Core version/conflict tests, real anti-entropy wire tests, and `check-formation-cli.py --policy-partition`: three ordinary worker processes, opaque UDP relays, independent Unix operator endpoints, conflicting local policies and exact winning-version receipts after heal. | **Passed for bounded complete peer isolation.** Full handoff/leave/crash/restart journey also passes through relays. Does not establish long partitions after Dead retirement, selective asymmetric topology, or global lock fencing. |
| Empty/paginated/changing/truncated admission baseline | The [condition audit](#admission-baseline-condition-audit--2026-09-06) maps empty encoding, ordered pagination, changing-source real wire transfer, retention/receiver faults, atomic merge, readiness/lifecycle fences and public handoff to named assertions. | **Baseline condition mapping complete; bounded tests passed.** The full process rerun's later readmission assertion failed at ten seconds. Its observation budget is now reconciled by the controlled timing evidence; baseline coverage alone did not establish that correction or close final conformance. |
| Old session/timer/verification/send after leave/adoption | The [cross-class audit](#stale-io-cross-class-audit--2026-09-07) maps incoming packets/handshakes, join preparation/reconnect, catch-up, send results/cancellation, timers and inline work. The [late admitted-peer completion regression](#late-admitted-peer-handshake-completion--2026-09-07) closes its last explicit gap. | **Passed at the mapped core/owner/real-wire boundaries.** Current, post-leave, queued-shutdown and closed-owner admitted-handshake cases now pass. Preserve these regressions in final acceptance; this does not claim every lifecycle permutation or substitute for the separately required process journeys. |
| Voluntary leave announcement dropped | `make test-formation-lost-departure` requires two suppressed encoded sends and survivor SWIM detection, then same-certificate readmission without clearing restrictions. | **Passed, recorded process evidence.** Separate public-response loss is covered by `make test-formation-lost-leave-response`. |
| Leave followed by same-certificate readmission | The [exact-view follow-up](#bounded-late-observation-and-exact-readmission-views--2026-09-07) checks every worker's ID/certificate/liveness map, including departed history and receipt replay; the [controlled timing fixture](#controlled-departure-round-and-readmission-budget--2026-09-07) establishes why the original ten-second observation budget was insufficient. | **Ordinary process journey passed with the corrected seventeen-second budget.** Historical failures are retained without claiming their precise runtime cause. Final fault-process reruns remain required; the diagnostic-only later snapshot is helper-tested, not captured process failure evidence. |
| Restart with prior certificate; tombstoned self learns ejection | `make test-formation-excluded-restart` proves refusal with retained blocked certificate and successful different-certificate control; `make test-formation-peer-ejection` proves post-adoption wire self-ejection and explicit leave. | **Passed for these recorded process cases.** Fixture sender is not a public removal API and does not prove cluster-wide administrative removal convergence. |
| Datagram reordering/replay and oversized gossip | Core ordering and wire gossip-budget tests; `authenticated_reordered_datagrams_preserve_probe_ids_under_replay`; `authenticated_acks_complete_only_their_outstanding_probe`, using actual owner-scheduled Pings and real QUIC replies. | **Passed at core and authenticated wire/owner boundaries.** Unknown and completed probe IDs cannot consume a current probe or raise incarnation; matching ACKs with lower envelope sequences complete only their named probe. This is direct-probe evidence, not a new indirect-relay or network-loss guarantee. |
| Peer/client flood, slow reads, incomplete frames | The [ingress case-to-contract closure](#ingress-pressure-case-to-contract-closure--2026-09-07) maps client/peer capacity, input/decode/output bounds, deadlines, control progress and shutdown to named evidence. | **Passed at the mapped executable, wire/server, owner and codec boundaries.** Exact output-cap coverage supplements the full client-pressure journey and existing Unix/TLS regressions. Configuration bounds are distinguished from heap measurements; monitoring pressure and fleet-load claims are not established by this row. |
| Wrong/missing operator authority or join/monitor credentials used for mutation | Shared `exercise_mutation_credentials` fixture runs four named tests over Unix/TLS and HTTP/1.1/HTTP/2, each with valid lock/unlock/leave/join requests and eight rejected credential forms. HTTP/2 additionally checks positive raw-wire operations and TLS `h2` ALPN. | **Passed for the recorded four transport/protocol matrices.** Actual monitoring credentials remain a companion observability requirement; they are not fabricated for these tests. This evidence does not close HTTP/2 flow-control or incomplete-header bounds. |

For focused reproduction, use the owning package and exact test filter, e.g.:

```sh
cargo test --locked --offline -p orishu-worker --lib real_handshake_is_decided_by_owner_and_closed_when_formation_changes
cargo test --locked --offline -p orishu-membership --test admission concurrent_allocations_recheck_certificate_capacity_and_lock_before_insertion
```

## Admitted connection collision and recovery — follow-up evidence

On 2026-09-06, the focused test below passed five consecutive runs (about
15 seconds each). The default worker suite passed **111 tests** (91 library,
15 binary, 5 integration). These results supersede the original 110-test count
for the worker only; the membership suite above was not rerun in this follow-up.

```sh
cargo test --locked --offline -p orishu-worker --lib simultaneous_admitted_dials --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
```

The following also passed in this follow-up: the fault-feature suite ran the
same 111 tests. This does not rerun the independent-process fault journeys.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
make docs-check
git diff --check
```

The test pauses only automatic member maintenance while arranging two real
handshakes. Both incoming bindings exist before either outgoing reply reaches
its serialized owner. Both outgoing registrations return `Duplicate`, closing
the crossed transports. Restarted maintenance alone establishes replacement
sessions: a lock update converges, then a second disconnect is followed by an
unlock update. Each round requires a new completed reliable exchange and two
Alive records. Generation, formation, exact member IDs and certificate bindings
remain unchanged; neither owner has tombstones. No replacement join is issued.
The outer fixture completes catch-up and joins both owner shutdown tasks.

Each repair is bounded to 10 seconds for this healthy two-worker loopback
fixture: one-second maintenance cadence, five-second anti-entropy cadence and
scheduling margin. Handshake arrangement and closure have separate five- and
two-second bounds; a 45-second whole-fixture deadline covers setup and cleanup.
These are regression deadlines, not a guarantee for unreachable routes or a
production SLO. Existing dial/session/exchange limit tests remain the resource
cap evidence; this case does not measure sustained overload. The read-only
owner snapshot seam is `cfg(test)`; no production API or protocol changes were
needed.

## Concurrent admission at local capacity — follow-up evidence

On 2026-09-06, `concurrent_authenticated_joins_share_one_local_capacity_slot`
passed five consecutive focused runs, then the default worker suite passed
**112 tests** (92 library, 15 binary, 5 integration). This supersedes the prior
worker count; previous process journeys were not rerun.

```sh
cargo test --locked --offline -p orishu-worker --lib concurrent_authenticated_joins --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
```

Both mutually authenticated application sessions exist before a three-party
barrier releases the two applicant requests. Real QUIC, codec, dispatcher,
owner admission and serialized replies decide the winner; no test mutation
inserts a member. Capacity is two including the introducer. Both replies must
decode: exactly one acceptance and one capacity refusal. The owner's final
model contains only its original ID and the accepted ID with the winning
certificate, unchanged formation and no tombstones. A lock command, owner
shutdown, dispatcher shutdown and connection closure must complete within two
seconds; the entire fixture is bounded to 15 seconds. These are test deadlines,
not an overload SLO. The two applicants use separate certificates and sessions
inside one test process; no cluster-wide capacity guarantee is inferred.

The first two runs failed with a valid capacity redirect: the generic fixture
advertised the first admitted applicant as an introducer at a synthetic IP.
The final fixture explicitly models dial-only, non-introducing applicants with
no advertised peer endpoints. With no redirect candidate, `CapacityExhausted`
is the exact expected response. The production refusal behavior was not
changed or weakened; no temporary instrumentation remains.

The fault-feature worker suite also passed all 112 tests. Scoped Clippy,
workspace formatting, documentation and whitespace checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
git diff --check
```

## Queued lock and admission ordering — follow-up evidence

On 2026-09-06, `queued_policy_changes_order_admission_after_authentication`
passed five consecutive runs. The default worker suite passed **113 tests**
(93 library, 15 binary, 5 integration), superseding the previous worker count.

```sh
cargo test --locked --offline -p orishu-worker --lib queued_policy_changes --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The existing real QUIC admission fixture now has a separately runnable range
of three policy cases. Authentication precedes the policy changes. After the
JoinReq frame arrives, lock/unlock and decoded peer delivery are queued without
yielding; the actual owner processes its available control budget first. Lock
refuses with `MembershipLocked`, unlock permits admission. A third case applies
lock after the accepted owner response but before writing it to the applicant:
the reply remains accepted and its identity is not undone. Assertions cover
policy, generation, formation, member count, accepted certificate binding,
absence of a refused certificate and absence of tombstones. Malformed/session
guard checks and owner shutdown remain part of the fixture. A ten-second outer
deadline bounds all three cases, including cleanup.

This does not promise FIFO ordering across separate mailboxes or that control
always precedes peers under a depleted fairness budget. It verifies the actual
ordering arranged by the fixture, paired with the existing core interleaving
test. No production scheduling or verification behavior changed, and no new
test hook was needed. Process-level partition/heal and full-workspace final
acceptance were not run in this follow-up.

The fault-feature worker suite also passed all 113 tests. The following
additional checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
git diff --check
```

## Independent-process policy partition and heal — follow-up evidence

On 2026-09-06, the direct process journey passed:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/test_formation_udp_relay.py
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
python3 scripts/check-formation-cli.py --policy-partition
```

The repeatable entry point `make test-formation-policy-partition` also passed
as a second complete run in this follow-up. It builds
ordinary binaries, runs 15 harness-helper tests and runs the full journey.
No worker fault feature, packet decryption, firewall privilege or remote fault
API is required. Three advertised loopback UDP relays forward only to their
fixed worker listener. Each relay caps source routes at eight, has no payload
history/offline queue, synchronizes partition changes with forwarding, and
stops its thread within one second. Helper tests establish distinct reverse
routes, bidirectional drops, no replay on heal and refusal at route capacity.

After A admits B and B admits C, ordinary cross-worker lock/unlock converges.
All relays then drop in both directions, interrupting A–B, A–C and B–C; all
Unix client endpoints stay usable. A locks, B locks then unlocks, and C retains
its prior unlocked view. Public receipts prove B's policy counter is newer
than A's; public summaries must show `[locked, unlocked, unlocked]` during
isolation. At least one actual peer datagram must be intercepted within two
seconds. Commands, interception and divergent-view inspection must finish
within three seconds, with unconditional healing in `finally`. This short fault
window targets recovery before retirement, not long-partition recovery.

After healing, bounded public polling requires unlocked views. Authenticated
no-op unlocks with fresh operation IDs inspect each current policy version;
all must equal B's exact winning tuple, not only its boolean. These consume
bounded operation-history entries (at most ten three-worker observations),
not a new read API, and are only issued after unlocked views converge. Exact
IDs/certificates and Alive states must remain unchanged. The same journey then
verifies voluntary leave/readmission and kill/restart/readmission, including
the expected final five-record history. Relays preserve backend source ports
across healing, and treat worker-down ICMP refusal as dropped transport.

The three-second fault limit and existing ten-second convergence polling are
test budgets, not production SLOs. Helper and process evidence does not establish
selective partitions, partitions lasting past Dead retirement, sustained
ingress overload or telemetry supervision. No production code changed.

## Stale routes and bounded fallback — completed checkpoints

Exercise stale advertised endpoint candidates followed by a usable candidate
through the real member dialer, then require restored policy reconciliation.
Measure control/shutdown progress while unreachable attempts consume the
configured dial budget; retain bounded pending-send failure behavior. This is
not new discovery, DNS support or an unbounded offline send queue.

### Silent-candidate fallback checkpoint — 2026-09-06

`admitted_dial_skips_silent_candidate_and_restores_policy_exchange` passed a
focused run in about 15 seconds. The fixture starts two owners with matching
already-admitted records (not a fresh join journey). Its membership-derived
target contains a bound silent UDP endpoint followed by a real QUIC listener.
The first socket must receive a packet, and one dial permit must be held.
A lock command completes within one second while the attempt is pending.
The unchanged five-second per-candidate deadline permits fallback; owner ACK
validation precedes registered traffic. Real reliable exchange and policy
adoption follow, and both owners, dispatcher, pump and connection close within
two seconds after recovery. This is not autonomous maintenance/fleet evidence.

Initial test compilation needed two test-only corrections (a non-Debug result
and an exchange-pool borrow). Three runtime attempts then timed out because
the original seven-second post-fallback deadline omitted an anti-entropy round
started before routing existed. Bounded views showed Alive peers and an open
connection; targeted diagnostics identified the outstanding round and its
eventual abandonment. The final budget is the configured ten-second round
timeout plus the five-second cadence and two seconds of scheduling margin,
inside a 30-second whole-fixture bound. Runtime deadlines were not extended,
and no production fix or SLO improvement is claimed. Temporary diagnostics
were removed. This finding explains why immediate fallback is not proof of
immediate reconciliation.

```sh
cargo test --locked --offline -p orishu-worker --lib admitted_dial_skips --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The saturation checkpoint below now adds four real unreachable attempts,
bounded refusal of a fifth and adapter cancellation. Do not mark the row
complete from the single pending-dial fallback fixture alone.

After the deadline correction, three consecutive focused repeats passed
(10–15 seconds each). Both default and fault-feature worker suites passed
**114 tests** (94 library, 15 binary, 5 integration), superseding earlier worker
counts. Both scoped Clippy configurations, workspace formatting, documentation
and whitespace checks passed. Process journeys and full-workspace tests were
not rerun in this checkpoint.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
git diff --check
```

### Real dial saturation and cancellation checkpoint — 2026-09-06

`real_silent_dials_bound_capacity_and_release_it_on_cancellation` passes through
the production member handshake adapter. Four separate bound silent UDP sockets
must receive QUIC Initial packets, with four live attempt tasks and no available
dial permits. A fifth attempt must return `Overloaded` within 100 ms, not queue.
While the attempts remain pending, an owner lock command succeeds; cancelling
the IO JoinSet restores all four permits and empties the task set, then owner
shutdown completes. That sequence has a one-second bound and the entire fixture
has a five-second bound. These are test budgets, not production SLOs.

The adapter test deliberately owns the JoinSet: an owner handle does not itself
own arbitrary dial futures. It does not claim that runtime-owned maintenance
jobs are automatically cancelled by this test. Targets are secret-free test
snapshots for unreachable members; no TLS/application handshake succeeds and
no membership insertion is asserted. Keep the prior authenticated healthy
fallback and missing-send-route tests alongside this capacity evidence. No
production code or dependency changed.

```sh
cargo test --locked --offline -p orishu-worker --lib real_silent_dials --quiet
```

The subsequent checkpoint covers runtime-owned pending-dial shutdown/generation
cancellation. Arrange enough unreachable admitted routes for real maintenance
to occupy its shared dial budget, then invoke the real lifecycle path and
assert its tasks terminate; do not replace that evidence with this adapter's
test-owned cancellation.

Five consecutive focused repeats passed, followed by both default and
fault-feature worker suites: **115 tests each** (95 library, 15 binary,
5 integration). Both scoped Clippy configurations, workspace formatting,
`make docs-check` and `git diff --check` passed. The commands are the same
worker-suite/feature commands recorded immediately above, plus the focused
test command in this checkpoint. Full-workspace and process-fault journeys
were not rerun; their earlier evidence is unchanged.

### Runtime-owned maintenance cancellation checkpoint — 2026-09-06

`runtime_maintenance_cancels_saturated_dials_on_leave_and_shutdown` passed its
focused run in about seven seconds. Each of its two cases arranges a valid
already-admitted five-record fixture before runtime startup, with four bound
silent endpoints and IDs selected by the normal canonical dialing rule. Real
maintenance must emit QUIC traffic to each endpoint within four seconds and
occupy all four shared dial permits. The test does not create dial jobs itself.

The leave case invokes the production runtime leave-operation method, requires
a changed receipt, fresh formation/node IDs, a new generation and a standalone
one-record view. Normal maintenance must release all old-generation permits.
The shutdown case invokes shutdown directly while saturated. Both cases require
the real owner task to finish successfully, all permits to return, and the
maintenance task slot to be empty. Lifecycle work is bounded to two seconds
(covering the one-second maintenance cadence); both cases together have a
16-second fixture limit. No test code takes or aborts the maintenance task.
The only added inspection is a `cfg(test)` read of available dial permits.

Two initial runs failed at credential setup with `UnsafePath`, before any
runtime/dial activity. Using a fresh child directory lets the production loader
create its required private directory; it does not relax path security or
change runtime behavior. No temporary diagnostic instrumentation was added.

```sh
cargo test --locked --offline -p orishu-worker --lib runtime_maintenance_cancels --quiet
```

Three consecutive focused repeats passed, followed by both default and
fault-feature worker suites: **116 tests each** (96 library, 15 binary,
5 integration). Both scoped Clippy configurations, workspace formatting,
`make docs-check` and `git diff --check` passed. Process journeys and full-workspace
tests were not rerun; this is runtime boundary evidence, not a new process run.

## Authenticated datagram reordering and replay — completed checkpoints

Drive reordered and duplicate SWIM datagrams through actual authenticated
worker delivery and assert probe correlation, replay bounds and eventual
liveness. Preserve byte-budgeted gossip and stream/datagram distinctions.
Existing semantic ordering and codec tests remain necessary but do not alone
prove this delivery path. Combined slow-input/ingress overload and optional
telemetry remain open acceptance work.

### Reordered and repeated Ping checkpoint — 2026-09-06

`authenticated_reordered_datagrams_preserve_probe_ids_under_replay` passed a
focused run through a real mutually authenticated connection, member handshake,
production dispatcher and serialized owner. An already-admitted two-member
fixture sends sequence/probe pairs `(100,700)`, `(99,701)`, `(100,700)` and
`(1,703)`; each must receive a datagram ACK with its own probe ID and the
authenticated receiver identity. The repeated Ping is intentionally valid.
An unsolicited ACK with probe/incarnation 999 is followed by a fresh Ping;
the fresh response must progress, no extra ACK appears during the bounded
quiet interval, and membership count, Alive count and held incarnation remain
unchanged. The five-second outer deadline includes owner/dispatcher cleanup.

The initial test incorrectly expected rejection of a repeated envelope sequence
and failed on its correlated ACK. Inspection of the documented sequence contract
and core dispatch confirmed that the 128-entry sequence window rejects replayed
**stream** requests, not SWIM datagrams. Datagram correlation/incarnation checks
are authoritative. The expectation was corrected without changing production
replay semantics. No temporary production instrumentation was needed.

```sh
cargo test --locked --offline -p orishu-worker --lib authenticated_reordered_datagrams --quiet
```

The unsolicited-ACK negative assertion is supporting state evidence, not proof
that a late reply cannot satisfy an outstanding probe. The subsequent checkpoint
adds that explicit pending-probe journey. Do not close the row by
treating datagram sequence rejection as an acceptance requirement.

Five consecutive focused repeats passed. Default and fault-feature worker
suites each passed **117 tests** (97 library, 15 binary, 5 integration).
Both scoped Clippy configurations, workspace formatting, documentation and
whitespace checks passed using the worker validation commands recorded above.
No full-workspace or independent-process journey was rerun in this checkpoint.

### Outstanding-probe ACK correlation checkpoint — 2026-09-06

`authenticated_acks_complete_only_their_outstanding_probe` passed its focused
run. It shares only setup with the previous Ping test: a real mTLS connection,
member handshake and production dispatcher. No test command starts a probe.
The test reads two successive scheduled owner Pings from QUIC and checks the
owner's pending-probe map through its read-only snapshot seam.

For the first probe, an ACK naming an unknown ID and incarnation 999 must
produce a diagnostic without consuming the pending probe or changing the held
incarnation. For the second probe, the same check replays the first completed
probe's ID. In each round a matching ACK uses a lower envelope sequence (9 then
8, after 100), removes exactly that probe and adopts incarnation 1 then 2 with
two Alive members. This distinguishes datagram sequence ordering from probe
identity. The receiver's real codec and session binding process every reply.

Each scheduled Ping has a two-second receive budget. Rejected/accepted ACK
processing has a 200 ms assertion budget, shorter than the configured 500 ms
direct-probe timeout, so timer expiry cannot stand in for successful matching
ACK evidence. The shared five-second whole-fixture bound includes shutdown.
No production behavior, timing or protocol changed.

```sh
cargo test --locked --offline -p orishu-worker --lib authenticated_acks_complete --quiet
```

Five consecutive focused repeats passed (about two seconds each). Default
and fault-feature worker suites each passed **118 tests** (98 library,
15 binary, 5 integration). Both scoped Clippy configurations, workspace
formatting, documentation and whitespace checks passed using the validation
commands recorded above. Full-workspace and independent-process journeys
were not rerun in this checkpoint.

## Next implementation: combined slow-input and ingress pressure

The follow-ups below now provide scoped evidence for peer, header/body and
response pressure plus shutdown. Preserve this historical section/link;
the next action is the complete limits and remaining conformance/recovery
audit, not another implementation of these passing scenarios.

Exercise real authenticated slow/incomplete streams and peer ingress while
measuring bounded owner control/shutdown progress. Name the capacities and
expected refusal/drop behavior; independently green limit tests do not prove
combined supervision. Optional HTTP probes and telemetry-outage cases remain
owned by the companion observability gate, not fabricated health in this test.

### Incomplete streams plus live datagram pressure — 2026-09-06

`incomplete_streams_and_datagram_pressure_preserve_control_and_shutdown` passed
its focused run in about 0.3 seconds. Real mTLS/member handshake and the normal
dispatcher precede the fault. The peer opens 15–16 streams against the configured
16-stream transport limit, accounting for handshake credit. Each sends a valid
4,096-byte length prefix but only one body byte and no FIN. Held worker exchange
permits prove that the receiver admitted those incomplete frames; the next
stream open must remain blocked for 100 ms. Frame deadlines are unchanged.

A concurrent task sends bounded valid Ping datagrams (at most 4,096, one per
millisecond). At least 32 must be submitted and owner transitions must increase
by at least 16 while incomplete streams still hold their permits. Lock and
unlock must complete within one second, and datagram production must continue
through that interval. Shutdown has two seconds to close the connection,
finish the pressure task and restore all 64 worker-wide exchange permits while
the test still retains the incomplete client streams. The entire fixture is
bounded to five seconds. The only new read seam is a `cfg(test)` exchange-permit
count; no production timing, limits or behavior changed.

The first two runs hit the outer deadline. A third, narrowed diagnostic run
located the wait at the sixteenth data-stream open: only fifteen were available
after handshake in that run. The test now measures bounded available credit
rather than assuming handshake credit has already returned. It still requires
at least fifteen held incomplete streams and refusal of the next open. Temporary
stage diagnostics were removed. This is not worker-wide 64-exchange saturation
or client HTTP ingress coverage; those remain the next acceptance work.

```sh
cargo test --locked --offline -p orishu-worker --lib incomplete_streams_and_datagram_pressure --quiet
```

Five consecutive focused repeats passed. Default and fault-feature worker
suites each passed **119 tests** (99 library, 15 binary, 5 integration).
Both scoped Clippy configurations, workspace formatting, documentation and
whitespace checks passed. Full-workspace and independent-process journeys were
not rerun; previously recorded results remain historical at those boundaries.

### Worker-wide exchange saturation and reclamation — 2026-09-06

`multiple_peers_exhaust_global_exchanges_and_recover_after_disconnect` passed
its focused run. Five separately authenticated provisional peer sessions use
the production TLS/application handshake and dispatcher. Four hold thirteen
incomplete length-prefixed streams each; the fifth holds twelve. The fixture
requires zero available worker exchange permits before opening the excess
stream, whose response direction must reset within 500 ms, rather than waiting
for the five-second frame deadline. This is global capacity, not per-session
transport credit or manually held semaphore permits.

An owner lock must succeed within one second at full occupancy. Disconnecting
the first peer must return exactly thirteen permits within one second while
the test still retains its client stream handles. A surviving connection then
opens another incomplete stream and consumes one returned permit, proving
capacity is reusable. Shutdown must join the owner and dispatcher, close all
connections and restore all 64 permits within two seconds. The full fixture
has a five-second deadline. No production code or limit changed.

```sh
cargo test --locked --offline -p orishu-worker --lib multiple_peers_exhaust --quiet
```

This evidence complements the admitted-peer datagram-pressure case; these
five sessions remain provisional and do not claim workload/admission success.
Next exercise the actual client listener with slow/incomplete client requests
while peer work is active, using bounded response/control/shutdown assertions.
Do not close the combined peer/client row from peer-only tests.

Five consecutive focused repeats passed. Default and fault-feature worker
suites each passed **120 tests** (100 library, 15 binary, 5 integration).
Both scoped Clippy configurations, workspace formatting, documentation and
whitespace checks passed using the worker validation commands recorded above.
Full-workspace and independent-process journeys were not rerun here.

### Slow client bodies with live formation traffic — 2026-09-06

The direct `python3 scripts/check-formation-cli.py --client-pressure` journey
passed using ordinary worker binaries. It runs three independent workers with
real QUIC formation traffic and Unix operator listeners. After introducer
handoff, sixteen authenticated POST requests to the first worker's leave route
declare 4,096-byte bodies but supply only one byte. Each must receive a bounded
`100 Continue` response before the next is opened, establishing that body
reading started under its mutation-handler permit. No malformed request can
complete a leave or mutate membership.

An extra authenticated malformed request must receive HTTP 503, not a parse
error, proving full mutation capacity. A lock through another worker must
converge to all three public summaries within two seconds, including the worker
with sixteen blocked body readers; status reads have independent capacity.
The harness completes exactly one malformed body and requires its HTTP 400
response, then performs a successful authenticated unlock through the loaded
worker. These assertions must finish within four seconds of setup, before the
unchanged five-second body timeout, so expiry cannot masquerade as recovered
capacity. Member counts remain three, and the surviving unfinished sockets are
closed by scoped cleanup. Peer unlock convergence and the full ordinary
leave/crash/restart/readmission journey follow.

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
python3 scripts/check-formation-cli.py --client-pressure
```

Both six-test helper suites passed. `make test-formation-client-pressure` also
passed as a second full process run, building ordinary binaries, running helpers
and running this complete scenario. Documentation and whitespace checks passed.
No production feature, wire contract or
deadline changed. Credentials are read from private fixture state, never
printed or included in failure assertions. Every retained socket has bounded
IO waits and scoped cleanup. The mode is exclusive of the other fault modes.

Remaining client-listener conformance must cover incomplete headers, slow
response consumption and shutdown with unfinished clients still present. This
scenario releases held bodies before the later normal shutdown and must not
be cited as evidence for that final case.

### Shutdown with unfinished clients — 2026-09-06

The extended `make test-formation-client-pressure` initially **failed** after
the full handoff/churn journey: worker A exceeded the shared three-second
SIGTERM-to-exit deadline with unfinished clients retained. Redacted failure
evidence is at `/tmp/orishu-formation-failure-13gjigqy` (local diagnostic
artifact, not a required repository fixture).

The new executable integration regression
`shutdown_cancels_unfinished_client_requests` reproduced that timeout in a
single worker. Its first setup attempt failed the private-directory check;
correcting fixture permissions to 0700 exposed the intended shutdown failure.
The test waits for actual `100 Continue` on an authenticated mutation before
supplying one body byte, holds another partial-header socket, sends SIGTERM and
retains both sockets until process exit and socket cleanup. The existing
three-second process deadline is unchanged.

Cause: the signal handler sent a 30-second graceful-stop request while the
owner supervisor sent a one-second request. The pinned server implementation
consumes only the first stop command before draining connections, so the longer
request could win and prevent the intended supervisor deadline taking effect.
The signal handler now requests runtime shutdown only; the owner supervisor
alone stops client listeners. No membership authority, wire format or command
replay semantics changed. No temporary diagnostic logging was added.

After the fix, the focused regression passed, followed by three additional
passing runs (about 1.1 seconds each). The original full
`make test-formation-client-pressure` **passed**, including final shutdown while
the test retains its sockets, EOF/reset checks and cleanup. This supersedes
the prior checkpoint's shutdown gap only; it does not prove header capacity or
expiry, or actual response backpressure. The shared helper suite now passes
nine HTTP tests, including incomplete-body admission and refusal/EOF negatives;
the six evidence-helper tests also pass.

```sh
cargo test --locked --offline -p orishu-worker --test standalone shutdown_cancels_unfinished_client_requests --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
make test-formation-client-pressure
```

Both worker configurations passed **121 tests** (100 library, 15 binary,
6 integration). Scoped Clippy with warnings denied passed in both configurations;
workspace formatting, `make docs-check` (90 Markdown files) and
`git diff --check` passed. Full-workspace Rust and optional observability acceptance were
not run. The existing `proc-macro-error2` future-incompatibility build warning
remains unrelated to this fix.

### Pre-authentication connection and header limits — 2026-09-06

The production client-server constructor now applies fixed formation-PoC
limits before route authentication: 64 accepted connections per listener,
five-second HTTP/1 head deadline, 8 KiB HTTP/1 buffer and 32 headers. Handler
limits remain separate. Saturation pauses acceptance at the OS backlog; it
does not promise an HTTP response to an unaccepted connection or reserve a new
operator connection. No dependencies or configuration keys were added.

`incomplete_headers_bound_connections_and_expire` first **failed** because a
65th connection received a response while 64 sockets were held. After applying
the limits it **passed** in about 5.2 seconds. Each held socket first completes
a real summary response, proving acceptance rather than backlog occupancy.
Sixty-three then hold incomplete request heads; the remaining established
connection still serves status. The extra connection receives no response for
100 ms at capacity. Dropping one held socket admits that waiting request,
with status and reuse together bounded to one second and setup/reuse below
three seconds (before expiry). Retained headers close/reset within a shared
seven-second observation budget, then a fresh summary and shutdown succeed.
The fixture has a 15-second outer bound.

`oversized_and_excessive_headers_are_rejected_before_handlers` **passed**:
a 9,000-byte header value and 33 additional headers receive actual HTTP 431
responses within two seconds each, followed by a successful summary. The first
run failed because `read_to_end` encountered reset after early rejection;
diagnosis confirmed the HTTP 431 bytes precede the reset. The corrected reader
retains those bytes and still requires HTTP 431. Bare EOF/reset cannot pass.
No production change or debug logging was needed for this fixture correction.

The extended `make test-formation-client-pressure` **passed** the complete
handoff/churn/shutdown journey. A new phase holds sixty proven-accepted
incomplete heads at A, leaving four listener slots for public reads. A lock
through B converges to all three summaries within two seconds, with setup and
assertions below four seconds so header expiry cannot explain progress. This
complements the exact-capacity executable test; it does not claim unlimited
new-client progress at full capacity. Nine HTTP-helper and six evidence-helper
tests passed in that command.

```sh
cargo test --locked --offline -p orishu-worker --test standalone incomplete_headers_bound_connections_and_expire --quiet
cargo test --locked --offline -p orishu-worker --test standalone oversized_and_excessive_headers --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
make test-formation-client-pressure
```

Both worker suites **passed 123 tests** (100 library, 15 binary, 8 integration).
Default and fault-feature scoped Clippy with warnings denied, workspace
formatting, `make docs-check` (90 Markdown files) and `git diff --check` passed.
Full-workspace and optional observability acceptance were not run. Slow
response-reader backpressure is still required: prove a requested write
actually stalls, bounded response work/buffering, unrelated control progress
and server-side reclamation. Header expiry or a small response fitting the
socket buffer cannot substitute for that evidence.

### Stalled response writes — 2026-09-06

`stalled_summary_reader_times_out_without_blocking_control` initially **failed**:
a proven pending Unix transport write remained alive through the seven-second
reclamation cutoff. The production server now sets a five-second write-stall
timeout. The same test then **passed**, including stronger authenticated-control
and pipeline-work assertions. This is not a total transfer timeout and does
not require a progressing reader to finish within five seconds.

The test runs the production client-server constructor, summary/lock handlers,
CBOR writer and real membership owner on a real Unix socket. It submits exactly
2,048 complete summary requests and reads none of their responses. This finite
pipeline makes the ordinary bounded responses fill the actual socket buffers;
it does not invent a large status record or claim that a small buffered reply
alone establishes backpressure. A test-only wrapper forwards scalar and vectored
IO unchanged while recording `Pending`, written byte counts, `TimedOut` and
transport drop. A counting hook observes dispatch without changing responses.

The fixture requires a real pending write within two seconds. An independent
authenticated client must read status and receive an accepted lock receipt
within one second. Dispatch count and written bytes then remain unchanged for
100 ms while the slow client is still unread; fewer than 2,048 responses were
constructed and fewer than 1 MiB written. Reclamation must occur within seven
seconds, with an observed write timeout, without client reads, client close or
server shutdown causing it. Only afterward does the fixture drain bounded
HTTP 200 bytes, accepting reset after buffered responses, and shut down the
owner/server. The whole test has a 12-second deadline. Its JoinSet cancels the
server on fixture failure; no test observer or fault route enters normal builds.

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker stalled_summary_reader --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

Both worker suites **passed 124 tests** (100 library, 16 binary, 8 integration).
`make test-formation-client-pressure` **passed** after enabling the write-stall
deadline, including all fifteen HTTP/evidence helper tests and the full
three-process handoff/pressure/churn/shutdown journey. Default and fault-feature
scoped Clippy, workspace formatting, documentation (90 Markdown files) and
whitespace checks passed.
This is bounded handler/transport evidence, not a measured peak-memory result,
minimum-throughput SLO or large future-streaming-response test. Preserve the
earlier process peer-pressure scenarios and audit the complete limits matrix
before closing formation conformance. Full-workspace and optional observability
acceptance were not run.

### Mutation authority matrix and client error status — 2026-09-06

`every_mutation_rejects_wrong_credential_classes_before_state_change` uses two
ordinary worker processes. The target exposes real privileged join material;
the source receives valid, encoded version-1 lock, unlock, leave and join
requests. Each request is exercised with eight credential forms:

| Credential form | Required result for all four intents |
| --- | --- |
| Missing header | HTTP 401 |
| Wrong 64-character bearer | HTTP 401 |
| Other worker's actual operator bearer | HTTP 401 |
| Target formation's actual join bearer | HTTP 401 |
| Duplicate correct local operator headers | HTTP 401 |
| Correct operator then join bearer | HTTP 401 |
| Join bearer then correct operator | HTTP 401 |
| Correct token with Basic scheme | HTTP 401 |

All 32 exchanges require actual bounded HTTP 401 responses (not transport
failure), with no local/operator/target join credential bytes in responses.
After each, public status retains exact source formation/node identity,
standalone participation, one member and unlocked policy. Target membership
remains unchanged. An authenticated join-status lookup returns HTTP 404 with
no retained operation. Correct local authority subsequently uses the rejected
lock/unlock/leave IDs successfully; standalone leave is explicitly a no-op.
Individual raw exchanges have two-second deadlines and the whole fixture has
15 seconds, with process reaping on failure.

The matrix first passed its rejection checks, but the added no-history check
exposed a client decoder bug. Initial fixture expectations tried the empty-body
404 variant, then HTTP 404 `ApiError`; both failed because the CBOR error was
decoded as a successful raw envelope and later converted to `ApiError` with
status zero. The temporary secret-free status probe confirmed `UnknownOperation`
with status zero, not an accepted join. It has been removed. The shared response
decoder now preserves non-success HTTP status and refuses success envelopes on
non-success responses. GET/DELETE error fixtures now check 404/403 status, and
a new fixture checks a false success envelope under HTTP 503.

```sh
cargo test --locked --offline -p orishu-worker --test standalone every_mutation_rejects --quiet
cargo test --locked --offline -p orishu -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

After the fix, the client passed **95 tests** and each worker configuration
passed **125 tests** (100 library, 16 binary, 9 integration). The earlier worker
suite failures were the same no-history assertion before the client fix.
`make test-formation` also passed: ordinary binary build, nine HTTP and six
evidence-helper tests, and the complete public handoff/leave/crash/restart/
readmission journey. Scoped Clippy passed for both default and fault-feature
configurations; workspace formatting, documentation and whitespace checks
passed. Full-workspace Rust and optional-observability acceptance were not run.
No wire schema changed; Rust raw-caller behavior now returns non-success CBOR
envelopes as errors with real status rather than `Ok(ApiResponse::Error)`.

**Remaining audit:** actual supported transports include TLS/TCP and HTTP/2,
not only Unix HTTP/1. Existing HTTPS inspection tests do not prove the complete
mutation credential matrix or HTTP/2 stream/header/flow-control limits. Map and
test those requirements next. Monitoring-only credentials must still be tested
when P-OBSERVABILITY implements them. This checkpoint does not close that gate
or the other recovery/baseline/stale-completion rows.

### TLS mutation credential matrix — 2026-09-06

`tls_mutations_reject_wrong_credential_classes_before_state_change` **passed**
in about 1.2 seconds. It shares the preceding eight-credential/four-intent
matrix with the Unix regression. Two ordinary worker processes provide actual
target join material and distinct operator credentials. The source exposes
both its Unix inspection socket and an explicitly configured TLS/TCP listener;
all 32 negative mutation exchanges use the TLS listener.

The raw client trusts only the generated server certificate, validates the IP
server name, offers only `http/1.1` and requires that ALPN selection. Every
exchange still requires actual HTTP 401 bytes and bounded responses without
credential disclosure. EOF without TLS close-notify or reset is tolerated only
after those rejection assertions can be satisfied, never as authorization
evidence by itself. Socket connect/read/write waits are one second, each
exchange has the existing two-second outer bound and the whole fixture retains
its 15-second deadline. Blocking TLS IO runs off the async runtime thread.

After each rejection the source's public Unix summary retains exact identity,
standalone participation, one member and unlocked policy. The authenticated
production client uses TLS for readiness, the absent join-operation lookup
(404), lock, unlock and the no-op standalone leave receipt. Target membership
remains unchanged. The raw negatives prove HTTP/1.1; the normal client's
negotiation is not claimed as explicit HTTP/2 evidence. No production code,
wire shape, dependencies or deadlines changed in this increment.

```sh
cargo test --locked --offline -p orishu-worker --test standalone tls_mutations_reject --quiet
```

Default and `formation-fault-test` all-target worker suites both **passed 126
tests** (100 library, 16 binary, 10 integration). Scoped Clippy with warnings
denied passed in both configurations; workspace formatting, documentation
(90 Markdown files) and whitespace checks passed. Full-workspace Rust,
the separate CLI churn harness and observability acceptance were not rerun
for this test/documentation-only increment.

HTTP/2 credential handling and explicit stream/header/flow-control bounds are
the next transport audit. The earlier TLS inspection tests and these TLS
HTTP/1.1 mutations do not close that work. Monitoring authority remains gated
on its actual implementation.

### Explicit HTTP/2 bounds and stream/header enforcement — 2026-09-06

The production server now explicitly selects 16 concurrent streams per HTTP/2
connection, 8 KiB decoded header lists, 4 KiB HPACK tables, 16 KiB frames,
65,535-byte initial stream/connection receive windows with adaptive growth
disabled, and a 16 KiB per-stream send-buffer setting. These are additional to
the existing connection and handler caps, not replacement global capacities.

`http2_stream_capacity_is_advertised_enforced_and_reclaimed` starts an ordinary
worker process and sends the real HTTP/2 preface, SETTINGS and bounded HPACK
header blocks over its Unix client listener. It first **failed** because the
server advertised the library default of 200 streams. With explicit settings,
the test verifies advertised stream/header limits and frame/stream-window
values, then holds sixteen authenticated POST streams without body completion.
The seventeenth must receive `RST_STREAM(REFUSED_STREAM)`. Independent HTTP/1
status remains available. Cancelling stream 1, then receiving a PING ACK, allows
a replacement stream whose completed malformed body must reach the production
decoder and return CBOR `InvalidRequest`, not overload or authorization failure.

A further validly encoded HEADERS frame carries a 9,000-byte value: it fits the
frame limit but exceeds the decoded header-list limit. The test requires a
terminal HTTP 431 HEADERS frame, checking its small HPACK golden encoding.
A subsequent valid summary stream on that same connection returns the real
one-member, unlocked state. These assertions finish within three seconds,
before body expiry can explain capacity reuse. Shutdown retains fifteen
unfinished streams until the process exits under the existing three-second
deadline; the complete fixture has a ten-second bound.

Two fixture assumptions were corrected through diagnosis: this HTTP/2 path
does not emit the HTTP/1-style `100 Continue` signal, so waiting for it instead
observed the five-second `OutcomeUnknown` response; and oversized headers can
end with HTTP 431 rather than a reset. Early focused runs and both suites failed
on those assumptions. The corrected test requires actual protocol responses,
not sleeps or transport closes as evidence. Temporary bounded, secret-free
frame diagnostics were removed. No dependency or wire schema was added.

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_stream_capacity --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

Both final worker suites **passed 127 tests** (100 library, 16 binary,
11 integration). `make test-formation` passed the full public journey plus
nine HTTP and six evidence-helper tests. Both scoped Clippy configurations,
workspace formatting and documentation checks passed. Whitespace checking
passed for this increment's files; repository-wide `git diff --check` reported
unrelated trailing whitespace in concurrently changed `TODO.md` lines 228 and
239, which were left untouched. The existing `proc-macro-error2`
future-incompatibility warning remains.

This is Unix HTTP/2 stream/header evidence, not TLS ALPN or
the complete credential matrix. Receive-window and send-buffer values are
configured; this test does not establish their full adversarial enforcement.
Still test stream/connection flow-control exhaustion, withheld response window
credit, incomplete HEADERS/CONTINUATION progress and HTTP/2 credential classes.
In particular, withholding window credit may not cause a pending socket write,
so existing transport write-stall evidence cannot close stream send liveness.
Full-workspace Rust and observability acceptance remain unverified.

### HTTP/2 credential matrices and positive controls — 2026-09-06

`http2_mutations_reject_wrong_credential_classes_before_state_change` and
`tls_http2_mutations_reject_wrong_credential_classes_before_state_change`
**passed**, first with the shared negative matrix, then with added positive
controls (about 2.9 seconds for both tests together). Each starts two ordinary
worker processes and repeats the prior eight credential forms against all
four valid mutation intents. The TLS variant validates the generated server
certificate and IP identity and requires negotiated `h2`; the Unix variant
uses the actual HTTP/2 preface on the Unix listener.

Every negative exchange uses a fresh HPACK context, literal non-indexed
credential fields, real HEADERS/DATA frames and a finite reader. It requires
the HTTP 401 status header's golden HPACK encoding plus a decoded CBOR
`Unauthorized` envelope, bounded to 8 KiB, with no credential bytes in the body.
The reader accepts at most 32 frames, bounds each to 16 KiB and acknowledges
server SETTINGS. Socket read/write waits are one second and each negative
exchange has a two-second outer deadline. Existing identity/state/history and
correct-authority checks remain shared with the HTTP/1.1 matrices.

Positive raw HTTP/2 requests with fresh operation IDs then lock and unlock the
source, verified through public state, and obtain the no-op standalone leave
receipt. Finally the formerly rejected join request receives HTTP 200/202 and
its matching identified connecting/admitting/catching-up/joined outcome.
This proves authorized submission, **not** completed catch-up. The fixture
shuts down both workers under its existing bounded cleanup; successful join
completion remains covered by the separate formation journey. The complete
matrix fixture retains its 15-second deadline. No production code, dependencies
or protocol schema changed.

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_mutations_reject --quiet
```

Default and `formation-fault-test` all-target worker suites both **passed 129
tests** (100 library, 16 binary, 13 integration). Scoped Clippy with warnings
denied passed in both configurations. Workspace formatting, documentation
(90 Markdown files) and whitespace checks for this increment's files passed.
Full-workspace Rust, the separate CLI churn harness and optional observability
acceptance were not rerun for this test/documentation-only increment.

Across all four named fixtures, there are 128 negative intent/credential
exchanges, with each TLS fixture requiring its intended ALPN protocol. This
closes the scoped Unix/TLS, HTTP/1.1/HTTP/2 mutation credential mapping. It does
not substitute for actual monitoring-only credentials, flow-control exhaustion,
response-window-credit stalls, incomplete HEADERS/CONTINUATION handling or
the remaining recovery and lifecycle audits.

### HTTP/2 response credit exhaustion and recovery — 2026-09-06

The existing working-tree tests were inspected and rerun through real worker
processes and the production Unix HTTP/2 listener:

- `http2_zero_response_window_isolated_and_recovers` sets initial response
  stream credit to zero, retains sixteen responses and requires refusal of a
  seventeenth. Independent authenticated summary/lock requests complete within
  one second. Cancelling one stream admits a replacement; returning credit to
  one old stream and the replacement releases only their DATA. Decoded summaries
  retain the exact formation/source identity and show the lock state captured
  before and after the mutation. Shutdown finishes with fourteen responses
  still window-blocked, before the test closes its connection.
- `http2_connection_response_credit_is_enforced_and_recovers` issues at most
  512 bounded summary requests without connection window updates. It counts
  actual DATA bytes until the full 65,535-byte connection credit is exhausted;
  each stream retains its separate initial credit. A subsequent response head
  and PING acknowledgement progress without DATA bypassing connection credit.
  Independent inspection remains available. A connection window update resumes
  the pending bodies, which decode to the original formation/source identity.

Both fixtures have ten-second whole-journey deadlines and bounded response
buffers; worker shutdown has its existing three-second deadline. These are
regression bounds, not production SLOs or measured memory/throughput claims.
No production behavior, dependencies or wire schema changed in this follow-up.

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_ --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets --features formation-fault-test -- -D warnings
```

All commands **passed**: five focused HTTP/2 tests; 131 tests in each worker
configuration (100 library, 16 binary, 15 integration); both scoped Clippy runs.
These results supersede the previous worker count, not its scoped limitations.
The separate CLI churn/fault journeys, full-workspace Rust tests and optional
observability acceptance were not rerun.

This closes the response-credit exhaustion and explicit recovery cases only.
It does not establish timeout-driven reclamation when credit is never returned,
inbound flow-control violation handling, incomplete HEADERS/CONTINUATION bounds,
TLS-specific pressure behavior, or a combined peer-flood guarantee. Preserve
these distinctions in the remaining ingress audit; the prior socket-write
timeout test does not prove a response-window timeout.

### HTTP/2 continuation count and binding — 2026-09-06

`http2_continuation_budget_and_stream_binding_are_enforced` adds a real-worker
Unix HTTP/2 regression against the locked `h2` 0.4.19 decoder and current worker
settings. Three fresh connections exercise:

- A HEADERS block without END_HEADERS, five empty non-final CONTINUATION
  frames, then a final sixth continuation: the real summary response decodes
  with the original formation/source identity.
- The same sequence with a non-final sixth continuation: actual GOAWAY with
  `ENHANCE_YOUR_CALM` (11), with no handler response accepted by the test.
- A continuation naming a different stream: actual GOAWAY with
  `PROTOCOL_ERROR` (1), with no handler response accepted by the test.

Each outcome has a one-second deadline and sixteen-frame read budget. Fresh
public inspection after each case confirms the worker remains usable with the
same formation. The entire fixture, including cleanup, has a ten-second bound.
The focused command **passed**:

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_continuation --quiet
```

This is decoder-work/binding evidence, not a header assembly timeout,
partial-frame timeout, proof of process-wide saturation or TLS pressure test.
No production behavior or dependency changed. The remaining ingress audit
must still establish the missing temporal bounds independently.

Default and `formation-fault-test` worker all-target suites both **passed 132
tests** (100 library, 16 binary, 16 integration), using the same commands as
the response-credit follow-up above. Both scoped Clippy configurations passed
with warnings denied. Workspace formatting, `make docs-check` (90 Markdown
files) and scoped whitespace checks passed. Full-workspace Rust, separate CLI
process journeys and optional observability acceptance were not rerun.

### Silent client connection expiry — 2026-09-06

The shared production client server now configures a ten-second transport
inactivity deadline for Unix and TLS HTTP/1.1/HTTP/2, in addition to the
existing five-second HTTP/1 header and pending-write deadlines. A successful
transport read or write resets inactivity. Previously the idle fuse was unset;
neither HTTP/1 header timing nor a pending-write timer covered a silent HTTP/2
header block or response waiting for window credit.

`http2_silent_partial_headers_frames_and_responses_expire` retains three real
Unix HTTP/2 connections: HEADERS without END_HEADERS, a frame with one of its
two declared payload bytes missing, and a completed GET whose response HEADERS
arrive with zero DATA credit. Each stays open through an initial 100 ms
observation, excluding immediate malformed-input rejection as a passing result.
Without sending missing bytes, credit or cancellation, the fixture requires
EOF/reset for all three under one shared twelve-second deadline. Reads and
terminal bytes are bounded. The worker remains running, with public formation
inspection before and after expiry; shutdown happens only afterward. The
whole fixture has an eighteen-second deadline.

The initial focused run passed; the strengthened early-open regression also
**passed**, as did all-target worker suites in default and fault-feature builds:
**133 tests each** (100 library, 16 binary, 17 integration).

```sh
cargo test --locked --offline -p orishu-worker --test standalone http2_silent --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features formation-fault-test --quiet
```

Compatibility: idle client connections now close; callers must reconnect and
use operation/status replay when a mutation receipt is uncertain. The deadline
does not change accepted domain outcomes or peer QUIC sessions. No dependency
or wire schema changed. The HTTP/2 expiry fixture uses Unix, not TLS, and does
not prove full connection-capacity reclamation under saturation.

This closes silent-connection timing only. PINGs, other streams or trickled
bytes can maintain connection activity. Absolute header assembly/partial-frame
deadlines and per-stream withheld-credit timing still require their own
contract and evidence; do not relabel this inactivity timer as those bounds.

`make test-formation` **passed** its full ordinary CLI handoff, leave,
crash/restart and readmission journey, plus six evidence-helper and nine
HTTP-helper tests. Both scoped worker Clippy configurations passed with
warnings denied; workspace formatting, documentation (90 Markdown files) and
scoped whitespace checks passed. The build still reports the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning. Separate fault
journeys, full-workspace Rust and optional observability acceptance were not
rerun in this follow-up.

### Stale-IO completion inventory — 2026-09-06

Inspection of `driver::Control`, `PeerDelivery`, the owner loop and effect
interpreter distinguishes actual asynchronous completions from inline work.
This inventory narrows the remaining stale-IO audit; source inspection alone
does not upgrade the entire failure-matrix row to passed.

| Completion class | Production fence and evidence | Remaining scope |
| --- | --- | --- |
| Decoded peer packet / authenticated handshake | `Owner::peer` and the session registry recheck generation and binding; `real_handshake_is_decided_by_owner_and_closed_when_formation_changes`, plus registry binding/ejection tests | Preserve actual wire coverage; not a claim about every fault combination |
| Initial join preparation | `Control::JoinPrepared` checks generation, source formation and standalone participation; new `stale_join_preparation_cancellation_preserves_new_operation` tests the real reserved cancellation after replacement | Still stage a successful old handshake completion across lifecycle replacement; cancellation alone does not prove connection disposal |
| Pending-join reconnect | `JoinReconnectFinished` checks generation, joining state, previous session, reconnect ID and active attempt; runtime cancellation and `shutdown_fences_successful_join_reconnect_handshake` tests | Map non-shutdown late-success cases separately before closing this class |
| Admission baseline/credential transfer | `CatchupFinished` checks generation, attempt ID, participation, total deadline and current source session before adoption; named leave/ejection/shutdown/session-loss/deadline regressions in the catch-up record | Existing successful-result barriers cover these lifecycle cases; retain their real wire/runtime boundary limitations |
| Reliable exchange reply / failure | Owner send tasks capture generation/session; successful replies pass through `Owner::peer`, stale errors cannot update the new generation's failure counter; lifecycle change aborts outstanding sends | Need an explicit held-success/held-error completion mapping, not just source inspection or missing-route tests |
| Timer expiry | Timer map belongs to the owner and is cleared on identity change/ejection; expired tokens are applied synchronously, with stale-token core regressions in `swim.rs` and `join.rs` | Preserve token tests and map pending-timer lifecycle coverage at the owner boundary |
| Credential verification, ID allocation and peer selection | Production effect interpreter computes these inline and queues typed outcomes in the same bounded owner turn; identity replacement clears its pending inline queue | No asynchronous production job exists to complete after leave. Queued-policy and local-capacity wire tests exercise actual owner ordering; do not invent a verifier task to test a nonexistent race |

The optional `reserve_verification` API has reservation/cancellation tests but
is not called by production credential verification. It cannot stand in for
the actual inline verification path. Shutdown owns and drops the serialized
owner and its work; admission/catch-up completion tests must still distinguish
late delivery from orderly cancellation.

The new three-second owner regression retains an initial `JoinPreparation`,
changes lifecycle through the real owner command path, starts a preparation
with a new operation ID and formation, then drops the old preparation. An
ordered status request proves the old reserved completion was handled. The
new operation remains `Connecting`, the old operation stays
`FailedBeforeAdmission`, and replacement formation/node/generation are
unchanged. Dropping the new preparation then produces its own expected failure,
and owner shutdown completes normally. The focused command **passed**:

```sh
cargo test --locked --offline -p orishu-worker --lib stale_join_preparation --quiet
```

This is production owner/completion-path evidence inside one test process,
not a real network handshake or a public operator leave-during-join guarantee.
The test deliberately stages the lifecycle transition below the public API's
busy-operation guard. No production semantics or dependency changed.

Default and `formation-fault-test` all-target worker suites both **passed 134
tests** (101 library, 16 binary, 17 integration). Scoped Clippy passed with
warnings denied in both configurations; workspace formatting and scoped
whitespace checks passed. Full-workspace Rust, separate process journeys and
optional observability were not rerun for this owner-test/documentation change.
The documentation check still fails on missing links in concurrently authored
Kagami tasks; those unrelated files were not modified by this increment.

### Late successful initial handshake and shutdown disposal — 2026-09-06

`late_successful_initial_handshake_is_closed_after_leave_or_shutdown` stages
real pinned QUIC/TLS handshake IO against a live worker dispatcher, then holds
the resulting `PendingHandshake` before its original reserved completion.
It exercises source lifecycle replacement and completed source shutdown
separately. After late delivery, the connection must report `LocallyClosed`
within one second and its endpoint must drain within three seconds, without
the test closing that endpoint. The target remains a cluster of one. In the
leave case the source retains its replacement formation/node, standalone
participation and `FailedBeforeAdmission` operation. The whole two-case
fixture has a fifteen-second bound and joins both runtime shutdown tasks.

The initial focused run and both worker suites **failed** this new test.
Tagged connection observation isolated the failure to shutdown: leave closed
promptly, but the post-shutdown connection remained open. It was not merely
QUIC drain latency. Tokio's owned mailbox permit can publish after receiver
destruction; the queued resource-bearing value then survives while another
sender keeps the channel allocation alive. Successful transport IO had no
remaining owner to dispose of its connection.

`send_reserved_control` now retains a shared disposal handle through reserved
publication. If the returned sender is closed, it takes and drops the value;
otherwise the owner consumes it or receiver destruction drops it. The slot is
never released to race a fresh admission, and only the small completion value
is shared—not membership authority. Join preparation, pending-join reconnect
and catch-up use this path. The bounded allocation occurs per shell completion,
not per packet, timer or simulation step. No network protocol or dependency
changed. The connection observer exists only under `cfg(test)`.

The corrected real-handshake test **passed**. A second regression,
`reserved_control_disposes_payload_on_either_side_of_receiver_drop`, tests
both publication orderings while deliberately retaining a sender. Existing
owner cancellation/replay tests remain the live-delivery controls. Temporary
`DEBUG-initial-close` instrumentation was removed.

```sh
cargo test --locked --offline -p orishu-worker --lib late_successful_initial_handshake --quiet
```

This extends the initial-preparation inventory row with successful leave and
shutdown disposal evidence. It uses real wire IO and runtime owners inside
one test process; lifecycle replacement is staged below the public API's busy
guard. It does not prove public leave-during-join support or close the separate
reliable-send and timer completion audit gaps.

Both corrected worker all-target suites **passed 136 tests** (103 library,
16 binary, 17 integration), in default and `formation-fault-test`
configurations. Scoped Clippy passed with warnings denied in both builds.
`make docs-check` now passes 100 Markdown files after the concurrent Kagami
task files arrived. The subsequent workspace formatting invocation could not
load a newly added, unrelated `crates/kagami-document` manifest with no target
yet; direct Rustfmt checks for the three changed worker source files and scoped
whitespace checks passed. No files in that concurrent crate were modified by
this increment. Full-workspace Rust and optional observability acceptance were
not rerun after this fix.

`make test-formation` also **passed** the full ordinary CLI handoff,
leave/crash/restart/readmission journey and its six evidence-helper plus nine
HTTP-helper tests. The build reports the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning. Separate development fault-process journeys
were not rerun here; a passing fault-feature unit suite is not their substitute.

### Process regression reruns after completion disposal — 2026-09-06

All three commands **passed** against rebuilt workers after the reserved
completion fix. Each ran its full ordinary handoff/leave/crash/restart/
readmission journey, not an admission-only diagnostic:

| Command | Verified fault/pressure assertion |
| --- | --- |
| `make test-formation-lost-ack` | Development-only post-insertion ACK loss recovers the original assigned ID; the fault marker, source operation/recovery reference and issuer inspection agree, without duplicate live membership |
| `make test-formation-client-pressure` | Slow authenticated bodies saturate mutation capacity; reads/peer policy progress, released capacity restores control, unfinished headers retain peer progress, and all three processes shut down within the shared three-second budget while incomplete client sockets remain held |
| `make test-formation-policy-partition` | Opaque peer isolation preserves independent operator access; conflicting policies converge after heal to the exact winning version with unchanged live identities, followed by full churn |

The pressure target passed nine HTTP-helper and six evidence-helper tests.
The partition target additionally passed three UDP-relay tests. The lost-ACK
target rebuilt its explicit `formation-fault-test` binaries in
`target/formation-faults`; the other two targets used ordinary binaries.
Builds report the existing `proc-macro-error2 v2.0.1` future-incompatibility
warning. Workspace formatting and documentation checks (101 Markdown files)
also passed after the concurrent metadata/documentation gaps were resolved.

These are scoped reruns, not closure of every interrupted-admission outcome,
long/asymmetric partition, TLS/HTTP2 pressure combination, release-feature
exclusion or optional observability gate. Preserve the fault durations and
pre-expiry assertions in the harness; successful retries do not erase failures
from earlier checkpoints.

Post-fix workspace validation also **passed**:

```sh
cargo test --locked --workspace --all-targets --quiet
cargo test --locked --workspace --doc --quiet
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Worker targets passed 136 tests. Workspace doc tests passed three examples and
retained the one explicitly ignored `query_filter!` example. Benchmark smoke
targets used the plotters fallback; this is not a performance measurement.
The newly completed concurrent Kagami document crate was included by workspace
discovery, but no Kagami implementation was changed or reviewed in this audit.
Scoped whitespace checks for the edited task documentation passed. This
supersedes the earlier post-fix metadata/documentation blockers, not the open
formation acceptance rows or optional telemetry requirements.

### Reliable-send result consumption across generations — 2026-09-06

The owner loop's existing successful/error result handling is now factored
into `Owner::send_finished`, used directly by the production send-task join
branch. No result semantics, scheduling, transport or wire contract changed.
A `cfg(test)` owner command invokes that same method and returns its actual
view after processing, avoiding a stale published snapshot as the assertion.

`old_send_results_cannot_change_replacement_formation_or_counters` drives a
real serialized owner through lifecycle replacement, then presents an old
generation's successful byte result and error result. Replacement generation,
formation/node, standalone participation, completed-exchange count, failed-send
count and diagnostics remain unchanged. The successful payload is deliberately
malformed: generation rejection must precede decoding. A current-generation
error increments the failure count as a positive control. The owner shuts down
cleanly under the fixture's three-second total deadline.

```sh
cargo test --locked --offline -p orishu-worker --lib old_send_results --quiet
```

The focused regression **passed**. Its first two invocation attempts could not
load a concurrently created `kagami-session` manifest without a target; those
were workspace-resolution failures, not failing assertions. No concurrent
Kagami files were modified. Resolution subsequently recovered.

This supplies owner-consumer generation-fencing evidence for both completed
result classes. It does not execute a real exchange, create a valid same-session
wire reply, or prove cancellation of an in-flight send future. Keep those
transport/binding and cancellation guarantees separate in the stale-IO audit;
the whole matrix row remains open pending its remaining mappings.

Default and `formation-fault-test` worker all-target suites **passed 137 tests
each** (104 library, 16 binary, 17 integration). Both scoped Clippy runs passed
with warnings denied. Direct Rustfmt for `driver.rs`, documentation checks
(101 Markdown files) and scoped whitespace checks passed. Workspace formatting
reported differences in concurrently edited `kagami-session` files; those
files were not reformatted by this increment. Separate process journeys,
full-workspace tests and optional telemetry were not rerun after this refactor.

### Owner timer cleanup on leave — 2026-09-06

`leave_clears_real_owner_deadlines_and_fences_late_timer_input` starts the real
serialized owner with known peers and requests a probe round. A read-only
`cfg(test)` snapshot observes the actual timer map and captures a token whose
deadline is still pending. The test does not insert a synthetic timer into
that map. A subsequent leave and same-control-lane snapshot prove that the
replacement lifecycle has no retained deadlines.

Delivering the captured token with its original generation then increments
the stale-input counter exactly once. Replacement formation/node identity,
standalone one-member state, diagnostics and failed-send count remain
unchanged; the timer map remains empty and owner shutdown succeeds. The entire
fixture has a three-second deadline. The focused regression **passed**:

```sh
cargo test --locked --offline -p orishu-worker --lib leave_clears_real_owner --quiet
```

This supplies the pending-timer owner/leave mapping in the stale-IO inventory.
Core tests separately cover obsolete suspicion and join tokens. This fixture
does not run a network exchange or prove every ejection/adoption/shutdown
interleaving. It observes existing production cleanup; no production behavior,
protocol, persisted format or dependency changed.

Default and `formation-fault-test` worker all-target suites **passed 138 tests
each** (105 library, 16 binary, 17 integration). Scoped Clippy passed with
warnings denied in both configurations. Workspace formatting, documentation
checks (102 Markdown files) and scoped whitespace checks passed. Full-workspace
tests, separate process journeys and optional telemetry were not rerun for
this test-only increment.

### Owner-supervised operational health foundation — 2026-09-06

`driver::View` now carries an optional monotonic owner-progress timestamp.
The existing one-second owner probe tick updates it even when participation
disallows new peer work. This heartbeat is produced by the serialized owner,
not the reader/exporter. `Handle::health` reads its bounded projection without
sending a core command. `health::OwnerHealth` uses a five-second deadline and
finite reasons, keeping responsiveness separate from formation readiness.

Two pure matrix tests cover every participation state, initial absence, exact
expiry boundary, regressed supplied time and closed owner. The real-owner
`health_reads_cannot_mask_stalled_owner_shutdown` regression observes first
progress, proves a locked non-introducing standalone owner remains healthy,
then holds the existing test shutdown barrier. Continuous reads cannot alter
the last timestamp: the state changes from responsive/non-ready `Stopping`
to `Stalled` after the budget. Releasing the owner produces `Closed`. The
fixture has an eight-second whole-journey bound and does not fabricate time
or a healthy exporter heartbeat. Both focused commands **passed**:

```sh
cargo test --locked --offline -p orishu-worker --lib health:: --quiet
cargo test --locked --offline -p orishu-worker --lib health_reads --quiet
```

This implements the N-FORMATION supervision hook consumed by P-OBSERVABILITY.
It adds no exporter dependency to the core, no public endpoint or wire schema,
and no workload authority. Process initialization/required-role checks and
startup latching remain necessary before the optional adapter can serve probes.
Do not equate `formation_ready()` with full process `/readyz` or cluster
convergence. Metrics, security/feature configuration, traces and recipes remain
undelivered. The five-second budget is not a measured production SLO.

Default and `formation-fault-test` worker all-target suites **passed 141 tests
each** (108 library, 16 binary, 17 integration). Scoped Clippy passed with
warnings denied in both builds. Workspace formatting, documentation checks
(102 Markdown files) and scoped whitespace checks passed. Full-workspace Rust,
separate process journeys and optional-exporter acceptance were not rerun in
this foundation increment.

`cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
also **passed all three dependency-purity tests** after the health hook landed.

### Process startup and required-role health composition — 2026-09-06

`RunningWorker::health` now combines the existing owner supervision input with
a process-lifetime initialization latch, sticky required-role failure and
shutdown intent. The executable marks initialization only after all configured
client listeners bind. Every client-server task owns a `RequiredRole` lifetime
guard, so normal exit, panic or cancellation withholds readiness. This does not
reset startup success or assert the owner is dead. Shutdown withholds readiness
before waiting for its owner acknowledgement. Current static configuration has
no role-restart operation, so a role failure remains latched for the process.

The pure state regression checks startup before/after initialization, owner
transition gating, role failure, repeated initialization, shutdown precedence
and retained startup after owner closure. The runtime regression
`process_health_tracks_initialization_required_task_loss_and_shutdown` uses a
real owner and an actual cancelled task holding the same guard as the listener
tasks. It verifies healthy ready state, responsive-but-unready role failure,
and closed/stopping state with startup still latched under three seconds.
This is task-lifetime integration evidence, not an HTTP probe test.

Both first worker-suite runs failed setup with `UnsafePath` before reaching
health assertions: the new fixture reused a non-private temporary directory
as its credential directory. Diagnosis followed the existing ownership/mode
guards; changing only the fixture to a loader-created private child made the
focused regression pass. No security check was relaxed and no temporary
debug instrumentation was added.

```sh
cargo test --locked --offline -p orishu-worker --lib health:: --quiet
cargo test --locked --offline -p orishu-worker --lib process_health_tracks --quiet
```

Both focused commands **passed**. No optional features, diagnostic listener,
HTTP health routes, metrics, traces or operator deployment recipes ship in this
increment. The projection is local and unversioned internal state, not a new
wire resource. Future required solver/storage roles must extend the process
initialization/failure gates; current health must not claim those services.

Corrected default and `formation-fault-test` all-target worker suites **passed
143 tests each** (110 library, 16 binary, 17 integration). `make test-formation`
passed the full ordinary CLI handoff/leave/crash/restart/readmission journey,
with six evidence-helper and nine HTTP-helper tests. Workspace formatting,
documentation (102 Markdown files) and scoped whitespace checks passed. Builds
retain the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.
Full-workspace Rust, separate fault-process and optional-exporter acceptance
were not rerun after this process-health change.

Both scoped Clippy configurations passed again after the fixture correction.

### Optional diagnostics HTTP lifecycle — 2026-09-06

The initial `observability` listener has executable loopback evidence for
healthy `/metrics`, `/livez`, `/readyz` and `/startupz`, runtime-disabled startup
while the configured port is occupied, and startup rejection of unsupported
exposure before credential creation. Its three label-free health gauges are
not the full formation metric catalogue or Prometheus parser acceptance.

`diagnostics::tests::http_probes_track_initialization_role_failure_and_closed_owner`
adds real HTTP requests through the production router and bounded server,
backed by an actual membership owner. It checks initialization refusal,
successful startup, safe membership lock/unlock, cancelled required-role
readiness failure, repeated initialization not clearing that failure, and
closed-owner liveness/readiness failure while startup stays latched. Every
phase compares probe statuses with the exported health gauges; responses
remain bounded, plain text and non-cacheable. The entire fixture has a
five-second deadline and individual requests a one-second deadline.

The lifecycle fixture uses a Unix socket and deliberately keeps diagnostics
reachable after owner shutdown. It establishes HTTP projection of real owner
and role state, not independent-process shutdown ordering, join/recovery or
stalled-owner HTTP behavior. The executable loopback fixture separately covers
TCP startup and healthy routes. No new public fault control or production
health transition was introduced by this test increment.

The focused lifecycle test **passed**. The observability-enabled worker suite
**passed 147 tests** (110 library, 18 binary, 19 integration), and scoped Clippy
passed with warnings denied. These runs preceded the additional lock/unlock
assertions. The default-feature suite **passed 145 tests** (110 library,
17 binary, 18 integration); it does not compile the optional lifecycle test.
Exact reproduction commands:

```sh
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_probes_track --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
```

Feature-changing test runs use `target/debug` sequentially to avoid replacing
the executable underneath another suite. Membership dependency-purity tests
passed all three checks; formatting, documentation validation (102 Markdown
files) and scoped whitespace checks passed. Full-workspace Rust, separate
fault-process journeys, Prometheus/OTLP integration and remaining optional
feature/security matrices were not run for this increment.

After adding the lock/unlock assertions, the combined optional/fault-feature
suite **passed all 147 tests**, and its scoped Clippy check also passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test -- -D warnings
```

This is feature-build regression evidence, not a rerun of the separate
fault-process journeys or the not-yet-implemented `otlp-tracing` matrix.

### Diagnostics enablement precedence through the executable — 2026-09-06

`diagnostics_enablement_precedence_and_invalid_config_are_checked_at_startup`
starts the actual worker with YAML, child-process environment overrides and
CLI options. Five cases cover file enablement, environment enable/disable
overrides and CLI enable/disable overrides of the environment. Child-local
environment settings avoid mutating the concurrent test process environment.

Disabled cases retain an occupied diagnostics port while inspecting a real
one-member summary and terminating gracefully. Enabled cases request a
non-loopback bind and require exit code 2 before the state directory or
credentials exist: feature-disabled builds report the unavailable capability;
feature-enabled builds report unsupported remote exposure. Every case has a
five-second whole-journey deadline, with subprocess cleanup on failure.

The default-build focused test **passed**:

```sh
cargo test --locked --offline -p orishu-worker --test standalone diagnostics_enablement --quiet
```

This closes the executable enablement-precedence evidence gap, not the whole
configuration matrix. Bind-address precedence, malformed file/env/CLI values,
enabled occupied-port handling, route-specific settings and remote TLS/auth
still require their own evidence or implementation. Existing healthy TCP
probe tests, not these deliberate rejection cases, establish enabled serving.

The observability-enabled all-target suite **passed 148 tests** (110 library,
18 binary, 20 integration). Scoped Clippy passed with warnings denied in both
default and observability builds. Formatting, scoped whitespace and
documentation validation (102 Markdown files) passed. Feature-changing suites
ran sequentially against the shared `target/debug` executable path.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

No production configuration behavior changed in this increment. Full-workspace
Rust, separate process-fault journeys, the fault-feature suite and telemetry
backend integration were not rerun; their acceptance remains separate.

The subsequent default-feature all-target suite also **passed 146 tests**
(110 library, 17 binary, 19 integration):

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
```

### Configurable diagnostic route groups — 2026-09-06

The worker now accepts typed `observability.metrics` and `observability.probes`
startup settings with file < environment < CLI precedence. Metrics selects
`/metrics`; probes selects `/livez`, `/readyz` and `/startupz` together. Both
default on inside an explicitly enabled listener. Selecting a group does not
enable diagnostics, an enabled listener with neither selected is rejected,
and disabled routes are not mounted. Probe-only mode still requires loopback;
this is not remote-exemption or monitoring-credential implementation.

`config::tests::diagnostics_route_groups_are_explicit_and_cannot_enable_an_empty_listener`
checks defaults and all four group combinations with the listener disabled and
enabled, including omitted build capability. The executable regression
`diagnostics_route_groups_follow_file_environment_and_cli` exercises metrics-only,
probes-only and both, using each of the three configuration sources. Lower
priority sources deliberately disagree so the nine-process matrix proves the
effective route selection, not just argument parsing. Actual TCP requests
require 200 for healthy selected routes and 404 for absent routes; secret
client routes remain absent and normal formation identity stays unchanged.

The fixture bounds the complete matrix to twenty seconds, every diagnostics
request to one second and graceful process termination to three seconds. The
shared request helper tolerates connection refusal while the listener binds,
without extending its total request deadline. Default disabled behavior and
enablement precedence retain their existing executable regressions.

Focused configuration tests **passed two tests**, and focused executable
diagnostics tests **passed four tests**. The observability-enabled all-target
suite **passed 150 tests** (110 library, 19 binary, 21 integration); scoped
Clippy passed with warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker diagnostics_ --quiet
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

Configuration, protocol, worker manual, observability guide and worker-operator
story now describe this partial delivery. No wire profile, persisted formation
format, domain authority or dependency changed. Existing settings retain their
behavior; the new settings are opt-in route restrictions. Full remote security,
probe lifecycle/pressure coverage, Prometheus catalogue/backend integration,
tracing and deployment recipes remain open. This does not close M4.

The sequential default-feature suite **passed 147 tests** (110 library,
18 binary, 19 integration), and default scoped Clippy passed with warnings
denied. Membership dependency-purity passed all three tests; workspace
formatting, documentation validation (102 Markdown files) and scoped
whitespace checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
cargo fmt --all -- --check
make docs-check
```

Full-workspace Rust and separate process-fault journeys were not rerun for
this local diagnostics routing change.

The subsequent combined observability/fault-feature suite **passed all 150
tests** (110 library, 19 binary, 21 integration):

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test --quiet
```

This verifies that build combination, not the separate process-fault journeys
or the future OTLP feature matrix.

### Owner counters in Prometheus exposition — 2026-09-06

`Handle::counters` and `RunningWorker::owner_counters` copy six integers from
the existing published owner view without cloning membership/identity data,
enumerating peers or submitting work. They retain the last published values
after owner closure. These shell-owned counters already saturate, accumulate
across formation generations and start from zero in a fresh process; no core
exporter or new dependency was introduced.

The optional `/metrics` catalogue now adds `membership_transitions_total`,
`stale_inputs_total`, `core_diagnostics_total`, `foreign_gossip_total`,
`reliable_replies_total` and `send_failures_total`, each prefixed
`orishu_worker_`. They are unlabelled counters with explicit event semantics,
not accepted-command or peer-death counts. The three health gauges remain.
Health and counters are separately sampled local projections; they do not
promise one atomic scientific or cluster snapshot. The catalogue and manuals
document process restart and closed-owner interpretation.

The HTTP lifecycle regression checks each counter type/value representation,
bounded response size, transition growth after actual lock/unlock commands,
and exact agreement with retained owner counters after closure. Its first
extended run **failed** because the fixture expected standalone leave to
create a new identity. The owner explicitly returns `changed=false` for this
case; checking that receipt confirmed a test-assumption error. The corrected
test preserves that no-op and **passed**, without changing leave semantics.
The existing `leave_clears_real_owner_deadlines_and_fences_late_timer_input`
regression separately checks counter continuity across a real owner generation
change and a counted late timer, including retained counts after closure.

The focused HTTP command passed after correction; its earlier failure remains
recorded above:

```sh
cargo test --locked --offline -p orishu-worker --features observability --bin orishu-worker http_probes_track --quiet
```

This is initial formation instrumentation, not the full metric/latency/queue
catalogue, a Prometheus-compatible parser check, an external scrape, an
overhead measurement or trace delivery. Those acceptance requirements remain
open together with the rest of M4.

After correction, the observability all-target suite **passed 150 tests**
(110 library, 19 binary, 21 integration), and the sequential default suite
**passed 147 tests** (110 library, 18 binary, 19 integration). Scoped Clippy
passed with warnings denied in both configurations. All three membership
dependency-purity tests, workspace formatting, documentation validation (102
Markdown files) and scoped whitespace checks passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
cargo fmt --all -- --check
make docs-check
```

Full-workspace Rust, fault-feature and separate process-fault journeys were
not rerun for this counter-projection increment. No external monitoring
backend was used.

### Real Prometheus parser and source-build scrape — 2026-09-06

`make test-worker-prometheus` builds the `observability` worker and runs
`scripts/check-worker-prometheus.py` against pinned official Prometheus 3.5.0
tools. The download's Linux amd64 SHA-256 matched the official release checksum
before extraction; version assertions also run in the harness. Downloads are
not part of the Make target and add no worker or membership dependency.

The first sandboxed invocation **failed before spawning workers** because
socket creation was prohibited. The approved loopback-capable invocation
**passed**. After adding the malformed-input negative control and Make target,
the complete target **passed again**, without retrying a failed application
scenario. The tool rejects malformed exposition, accepts a bounded real
worker `/metrics` response and validates `etc/prometheus-local.yml` with only
the ephemeral target address substituted. A separate Prometheus process then
ingests and returns exactly the nine expected series with only metric-name,
job and instance labels. Healthy gauges are one, samples are finite and
non-negative, the actual operator token is absent, and the worker remains
ready. Both processes terminate gracefully within their cleanup deadlines.

Reproduction with verified local copies of the pinned tools:

```sh
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

See [the test guide](../testing-worker-prometheus.md) for the exact checksum,
version, bounds, isolation, example and limitations. The source-build local
parser/scraper requirement now has real external-tool evidence. Packaged
release artifacts, remote TLS/auth, full formation metrics, overhead, long-run
cardinality, exporter failure, traces and dashboard/alert recipes remain
unverified or unimplemented; this does not close the companion tasks or M4.
No Rust behavior changed in this harness/configuration/documentation increment;
full Rust suites and separate formation-fault journeys were not rerun.

The script's CLI/syntax check, scoped whitespace checks and repository
documentation validation **passed** (103 Markdown files). The local test
guide is indexed and the companion task/roadmap status records partial
operator-documentation delivery rather than treating the full handoff as done.

### Local operator alert rules and recovery fixtures — 2026-09-06

`etc/prometheus-worker-alerts.yml` defines the versioned
`orishu-worker-local-v1` group with three warning examples: scrape unavailable,
reachable-but-sustained-unready, and successful scrape lacking readiness data.
The readiness rules match by scrape job/instance and require successful `up`;
missing series are never converted to measured zero. The one/two-minute pending
delays and one-minute evaluation cadence are example budgets, not measured SLOs
or automatic recovery authority.

`prometheus-worker-alerts.test.yml` exercises three synthetic rule-engine
scenarios: healthy/transient/down/unready/missing separation, recovery of all
three alerts, and stale readiness versus disappearance of a discovery target.
The latter explicitly records the inventory limitation rather than pretending
that a vanished `up` series proves a healthy or dead process.

The pinned Prometheus 3.5.0 rule test **passed**, and the updated complete
worker/Prometheus target **passed**. It validates the rule syntax and fixtures,
copies the checked-in rules alongside the generated scrape configuration,
ingests nine real worker series, and verifies that the separate server loaded
the exact three alert rules with no alerts on the short healthy run:

```sh
/path/to/promtool test rules etc/prometheus-worker-alerts.test.yml
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

The [operator test guide](../testing-worker-prometheus.md#local-alert-examples-and-operator-response)
documents threshold rationale, idle and missing-data semantics, discovery
limitations and safe inspection steps. Neither an alert nor a scrape authorizes
restart, leave, new admission or exclusion removal. Rule state transitions have
synthetic engine evidence; this is not a real-worker failure journey held for
the full pending delays or Alertmanager notification-delivery evidence.
Dashboards and the remaining instrument-specific alerts/runbooks remain open.

Documentation validation **passed** (103 Markdown files) and scoped whitespace
checks passed. No Rust behavior changed; full Rust or formation-fault suites
were not rerun for this rule/example/harness change. Concurrent release and CI
work in the worktree was not modified by this increment.

### Scraper outage, operator control and fresh re-ingestion — 2026-09-06

The real Prometheus harness now builds and invokes `orishuctl` as well as the
observability-enabled worker. After the initial nine-series scrape and alert
loading check, it stops and reaps the actual Prometheus process. While that
process is absent, two authenticated CLI operations lock and unlock the worker
using its formation precondition and explicit operation IDs. Receipts and
subsequent public status agree, formation identity is unchanged, `/readyz`
stays successful, and the real owner transition counter advances.

The fixture restarts Prometheus against its existing temporary TSDB and
requires ingestion of at least the counter value reached while the scraper
was absent. Since that value is strictly greater than the value read after
the original scraper exited, old persisted samples cannot satisfy recovery.
The worker remains the same process throughout. Polling and subprocess cleanup
retain explicit deadlines; credentials are supplied to the CLI by file path,
not printed or placed in process arguments.

The full target **passed**:

```sh
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

The run used the previously checksum-verified official Prometheus 3.5.0 tools.
The build emitted the existing `proc-macro-error2 v2.0.1` future-incompatibility
warning; it was not a build or test failure. No Rust behavior changed, and full
Rust suites or separate formation-fault journeys were not rerun. This is
standalone scraper-outage/control evidence, not a multi-worker admission/SWIM
journey, scrape-saturation test, diagnostics-task failure or OTLP queue/collector
outage test. Those remaining companion requirements are not waived.

The script's CLI/syntax check, scoped whitespace checks and documentation
validation **passed** (103 Markdown files). The operator guide now documents
the actual outage/recovery assertions and their single-worker scope.

### Malformed diagnostics startup settings — 2026-09-06

`malformed_diagnostics_configuration_exits_before_credentials` exercises the
actual worker executable with invalid `enabled`, `bind`, `metrics` and `probes`
values from YAML, environment and CLI, plus an unknown nested YAML key. Each
of the thirteen cases has a three-second deadline and requires exit code 2,
an actionable parser error, no credential/state directory and no client socket.
Child-local environment overrides are isolated from concurrent test processes.
The default disabled listener does not make malformed typed settings valid.

The focused default-feature regression **passed**:

```sh
cargo test --locked --offline -p orishu-worker --test standalone malformed_diagnostics --quiet
```

This covers those malformed-input paths, not every configuration combination.
Bind-address precedence and enabled occupied-port handling remain separate
acceptance work. The existing route-selection unit matrix covers an enabled
empty listener; that is not yet independent-process rejection evidence.
No production configuration or security behavior changed in this increment.

The same focused test **passed with observability enabled**, covering all
thirteen cases again. Observability-enabled scoped Clippy with warnings denied,
workspace formatting, scoped whitespace and documentation validation (103
Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone malformed_diagnostics --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, fault-feature/process journeys and the external
Prometheus harness were not rerun for this focused test-only increment.

### Fail-fast diagnostics listener binding — 2026-09-06

The executable now uses fallible diagnostics binding after configuration/fault
validation but before credential creation, membership-owner startup or client
listener binding. It retains the resulting acceptor until serving starts,
rather than probing availability and racing a second bind. Disabled diagnostics
still bind nothing; a TCP connection during initialization is not a successful
health response. No listener/security policy or wire protocol changed.

The new executable regression holds a real loopback port throughout startup.
Its initial run **failed as expected**, observing the old `AddrInUse` panic
and exit code 101. After the change it **passed**: exit code 2, a clear
`cannot bind diagnostics listener` error, no panic, no state/credential
directory or client socket, and the fixture-owned conflicting listener remains
untouched. Each run has a three-second whole-process deadline:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone occupied_diagnostics --quiet
```

This establishes occupied diagnostics-port failure and the new startup order,
not failure atomicity for every other worker initialization path. The worker
manual and configuration guide document the behavior and safe operator action.

After the fix, the observability-enabled all-target suite **passed 152 tests**
(110 library, 19 binary, 23 integration), and the sequential default suite
**passed 148 tests** (110 library, 18 binary, 20 integration). Scoped Clippy
passed with warnings denied in both builds. Workspace formatting, scoped
whitespace and documentation validation (103 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full-workspace Rust and fault-feature/process journeys were not rerun for this
startup-order change; these results do not establish final M4 acceptance.

The separate `make test-worker-prometheus` target also **passed** with the
verified Prometheus 3.5.0 tools after the change: actual parser acceptance,
nine-series ingestion, tested alert loading, authenticated control during
scraper outage and fresh re-ingestion all remain working. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

### Diagnostics bind-address precedence through serving — 2026-09-06

`diagnostics_bind_precedence_uses_only_the_selected_address` starts real workers
for file, environment and CLI address selection. Three distinct loopback
candidate sockets are reserved; only the expected winning address is released.
The other candidates remain occupied throughout startup and HTTP assertions.
This proves actual bind selection, not just parsing: `/readyz` and `/metrics`
must be served on the winning address, the normal operator summary must retain
the same formation identity, and graceful termination must succeed. The whole
three-case journey has a twelve-second deadline; child environment settings
are isolated and all fixture-owned sockets remain untouched.

The focused regression **passed**, and observability-enabled scoped Clippy
passed with warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_bind_precedence --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

Together with the separate malformed-setting, enablement, route-selection and
occupied-port tests, this closes the documented bind-precedence evidence gap.
It does not establish secured remote binds, all platform/address families,
release feature matrices or every contradictory-setting process path. No
production behavior changed; full Rust suites, separate formation faults and
the Prometheus backend harness were not rerun for this test-only increment.

The broader executable diagnostics group **passed all seven tests**:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
```

Workspace formatting, scoped whitespace and documentation validation also
passed (103 Markdown files).

### Bounded owner-lane pressure metrics — 2026-09-06

`Handle::pressure` and `RunningWorker::owner_pressure` expose fixed-size local
readings for peer, control, completion and shutdown lanes. Each reading copies
the configured channel capacity and used slots; reserved permits count as
occupied before sending a message. No message collection is cloned, no
capacity is consumed by inspection, and no core/network/exporter dependency
was added. Different lanes are sampled independently, not as an atomic
scheduling or cluster-capacity snapshot.

The optional Prometheus catalogue adds `orishu_worker_{lane}_slots_in_use` and
`orishu_worker_{lane}_slots_capacity` for exactly those four lane names, eight
unlabelled gauges. Capacities are 64, 16, 64 and 1. The catalogue now contains
seventeen series and remains below the documented 4 KiB exposition bound.
These readings are slot occupancy, not running tasks or queue length excluding
reservations; liveness remains independently supervised.

The existing peer saturation regression now checks full peer occupancy and
untouched control/completion/shutdown capacity. The reserved-verification
regression checks that a full set of outstanding permits consumes all completion
slots even before messages are sent. HTTP lifecycle assertions verify gauge
types, configured capacities and occupancy bounds. The external Prometheus
harness now requires all seventeen names and validates capacity/occupancy
values after actual ingestion.

The observability-enabled all-target suite **passed 153 tests** (110 library,
19 binary, 24 integration), and scoped Clippy passed with warnings denied:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

This is additive local channel instrumentation, not full request/transport
metrics, scrape-overload evidence, trace export or a measured overhead budget.
The metric catalogue, worker manual, protocol maturity and operator story
describe the exact semantics; the remaining companion gates stay open.

The subsequent default suite **passed 148 tests** (110 library, 18 binary,
20 integration). All three membership dependency-purity checks passed, as did
workspace formatting, scoped whitespace and documentation validation (103
Markdown files). The real `make test-worker-prometheus` target **passed** with
the verified 3.5.0 tools: seventeen series parsed/ingested, fixed slot capacities
and bounded integral occupancy, three tested alerts loaded, operator control
during scraper outage and fresh ingestion after recovery. The existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

```sh
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-membership --test dependencies --quiet
make test-worker-prometheus PROMTOOL=/path/to/promtool PROMETHEUS=/path/to/prometheus
```

Full-workspace Rust and separate fault-feature/process journeys were not rerun
for this additive metric projection. These results do not close M4.

### Diagnostics HTTP/1.1 header limits — 2026-09-06

`diagnostics_reject_oversized_headers_and_preserve_normal_access` uses an actual
observability-enabled executable and TCP diagnostics listener. It sends 32
headers (accepted), 33 and 42 headers (431), and a header value larger than the
8192-byte request-head budget (431). Rejections contain no metric exposition;
response reads are bounded to 8192 bytes and two seconds. After every case,
ordinary metric/liveness requests still succeed and the real Unix operator
summary retains the original formation identity. The whole fixture, including
graceful process cleanup, has an eight-second deadline.

The initial oversized-input regression passed before adding the exact-count
boundary checks. This is real diagnostics HTTP/1.1 admission/recovery evidence,
not concurrent scraper saturation, stalled response writes, HTTP/2 ingress,
peer/control fairness under load, or remote-auth acceptance. No production
limits changed; the test exercises the existing shared server configuration.

With the exact-count assertions, the executable diagnostics group **passed all
eight tests**, and observability-enabled scoped Clippy passed with warnings
denied. Workspace formatting, scoped whitespace and documentation validation
(103 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, separate formation-fault journeys and external
Prometheus acceptance were not rerun for this test-only increment.

### Shutdown with unfinished diagnostics requests — 2026-09-06

`unfinished_diagnostics_requests_do_not_prevent_process_shutdown` starts the
actual worker with its optional TCP diagnostics listener and normal Unix client
surface. Eight persistent diagnostics connections each receive a complete
successful startup probe before sending an unfinished metrics request head.
This confirms server acceptance, not merely sockets waiting in a listen
backlog. The connections remain open through SIGTERM and process exit; test
cleanup does not manufacture successful server cancellation.

While those clients remain held, liveness and the normal operator summary
remain available with unchanged formation identity. Graceful termination must
exit successfully within three seconds and remove the worker's own Unix socket.
Each held diagnostics connection must then observe EOF or connection reset.
The full fixture has an eight-second deadline and bounded response reads.

The focused executable test **passed**:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone unfinished_diagnostics --quiet
```

Only half of the sixteen diagnostics connections are held, deliberately leaving
capacity for probes. This is unfinished-IO shutdown isolation, not exact-limit
saturation, overload fairness, slow-response backpressure, stalled-owner health
or HTTP/2 conformance. No production shutdown behavior changed.

The combined observability/fault-feature all-target suite **passed 155 tests**
(110 library, 19 binary, 26 integration), and scoped Clippy passed with warnings
denied in that configuration. Workspace formatting, scoped whitespace and
documentation validation (103 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability,formation-fault-test -- -D warnings
cargo fmt --all -- --check
make docs-check
```

This verifies the combined feature build, not the separate fault-process
journeys. Full-workspace Rust, default-worker suites and external Prometheus
checks were not rerun for this test-only increment.

### Unfinished diagnostics expiry and operator progress — 2026-09-06

The accepted-connection fixture now has an additional live-process expiry
path. Eight persistent connections first receive successful startup probes,
then hold unfinished metrics request heads. Authenticated lock/unlock calls
through the normal Unix client must complete within one second each while
those clients remain held; their receipts reflect the requested policy. This
adds actual owner-command progress to the earlier read-only checks.

Without shutdown or closing the test clients first, all held connections must
reach EOF/reset within one shared seven-second observation deadline (a bounded
408 response before EOF is permitted). This exercises the configured
five-second HTTP/1.1 head timeout with test margin. The same worker must then
serve normal liveness/metrics and the unchanged formation identity before
graceful termination. The expiry journey has a twelve-second total deadline;
the original shutdown path retains its eight-second total and three-second
process-termination bounds, with a shared one-second post-exit close check.

Both focused lifecycle tests **passed** in approximately five seconds after
the authenticated control assertions were added:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone unfinished_diagnostics --quiet
```

This proves deadline-driven reclamation and local control with eight held
diagnostics connections, not exact sixteen-connection saturation, active
trickle attacks, HTTP/2, peer-flood fairness or slow response backpressure.
No production timeout, authorization or shutdown behavior changed.

The wider executable diagnostics group **passed all ten tests**. Scoped
observability-enabled Clippy passed with warnings denied; workspace formatting,
scoped whitespace and documentation validation passed (104 Markdown files in
the final check after concurrent documentation additions):

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, fault-feature/process journeys and external
Prometheus acceptance were not rerun for this test-only increment.

### Full diagnostics connection budget and recovery — 2026-09-06

`diagnostics_full_connection_budget_preserves_operator_control_and_recovers`
extends the accepted-connection fixture to all sixteen diagnostics slots. Each
connection receives a successful startup probe before sending an unfinished
request head. Setup must finish within two seconds, before the five-second
head deadline can reclaim the earliest connection. A seventeenth connection
must close/refuse or remain waiting for the bounded 500 ms check; any response
byte fails the assertion. This does not count kernel backlog entries as served
HTTP requests or promise one particular refusal status.

Authenticated lock/unlock still executes through the independent Unix client
surface within one second each, and the complete control check must finish
within four seconds of pressure setup, before header expiry. All sixteen held
clients subsequently observe server-side closure under the shared expiry
budget. Normal liveness and metrics requests then succeed on the same worker
with unchanged formation identity, followed by graceful shutdown. No test
client is closed early to free any of the sixteen accepted slots.

The focused real-process regression **passed** in approximately five seconds:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_full_connection --quiet
```

This closes the scoped HTTP/1.1 diagnostics connection-budget/control/recovery
case, not arbitrary floods, active trickle traffic, TLS/HTTP2 behavior, peer
fairness, or slow-response buffering. Probes share the diagnostics connection
budget: saturation may make a probe unreachable without changing owner health.
Operator tools must not infer owner death from that transport failure alone.
No production connection limit, scheduling or membership behavior changed.

The wider executable diagnostics group **passed all eleven tests**. Scoped
observability-enabled Clippy, workspace formatting, scoped whitespace and
documentation validation (104 Markdown files) passed:

```sh
cargo test --locked --offline -p orishu-worker --features observability --test standalone diagnostics_ --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Full worker/workspace suites, separate fault-feature/process journeys and the
external Prometheus harness were not rerun for this test-only increment.

## Stalled-shutdown owner through HTTP — 2026-09-06

`diagnostics::tests::http_reads_cannot_hide_stalled_owner_shutdown` **passed**:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_reads_cannot_hide --quiet
```

The fixture uses a real membership owner and the production diagnostics
router/server over Unix. Healthy initialized probes first succeed. The existing
owner shutdown-hold fixture, now also available through the development-only
fault feature's Rust API, acknowledges shutdown while preventing further owner
progress. Repeated successful metric reads and failed readiness reads cannot
refresh supervision. Within the configured five-second deadline plus a
one-second observation allowance, the still-open owner becomes stalled:
`/livez` and `/readyz` return 503, `/startupz` and `/metrics` return 200, and
metric health values agree. Releasing the hold permits normal owner closure,
with the same failed-liveness and latched-startup HTTP results.

The whole fixture has a ten-second deadline and each HTTP request a one-second
deadline. There is no new CLI or remotely callable fault control; the existing
release-build prohibition covers the fault feature. This is in-process real
owner/HTTP evidence for a stalled shutdown, not independent-process running
owner stalls, transport saturation or executable listener shutdown ordering.
Other probe states and the full companion gates remain open.

Validation against this dirty-worktree checkpoint **passed**: combined-feature
worker all-target tests (110 library, 20 binary, 28 integration; 158 total),
default worker all-target tests (110 library, 18 binary, 20 integration; 148
total), and scoped Clippy with warnings denied for both configurations:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

Feature-changing test builds ran sequentially against `target/debug`.
Formatting, scoped whitespace checks and documentation validation (104 Markdown
files) passed. Full-workspace tests, independent formation fault journeys,
release-build exclusion and the external Prometheus harness were not rerun;
existing evidence for those boundaries is not upgraded by this increment.

## Running-owner stall and HTTP recovery — 2026-09-06

`diagnostics::tests::http_probes_detect_running_owner_stall_and_recovery`
**passed** in the focused combined-feature binary test:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --bin orishu-worker http_probes_detect_running --quiet
```

A development-only Rust control pauses the real owner at its IO loop, without
pausing the async executor, changing membership or fabricating a progress
timestamp. The fixture waits for acknowledgment of that pause. No CLI/network
route exposes it; the existing fault-feature release-build prohibition applies.
Dropping or sending the release channel resumes the owner.

The shared real Unix HTTP fixture now exercises both running and shutdown
stalls. The running case starts fully initialized and healthy, then continuously
reads metrics, liveness and readiness while owner transitions remain frozen.
Within the five-second supervision deadline plus one second of observation
allowance, the still-open owner becomes stalled. HTTP then reports 503 for
liveness/readiness and 200 for startup/metrics, with matching health gauges.
The transition counter has not advanced. After release, the actual owner tick
restores health within two seconds; transitions resume without changing
formation ID, node ID or participation. Normal shutdown and closed-owner
probe assertions finish the ten-second-bounded fixture.

This establishes running-owner stall/recovery through real owner and HTTP
boundaries in one process. It is not an OS-wide freeze, independent-process
fault journey, admission recovery or evidence for every probe/feature/security
combination. The previous shutdown fixture remains a separate regression.

Validation **passed** against this dirty-worktree checkpoint: combined-feature
worker all-target tests (110 library, 21 binary, 28 integration; 159 total),
default worker all-target tests (110 library, 18 binary, 20 integration; 148
total), and scoped Clippy with warnings denied for each configuration:

```sh
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

The initial formatting check identified one assertion requiring wrapping;
after correction, formatting and documentation checks (104 Markdown files)
passed. Test builds with different feature sets ran sequentially using
`target/debug`. Full-workspace tests, independent process-fault journeys,
release exclusion builds and the external Prometheus harness were not rerun.
No production protocol, dependency or health deadline changed.

## Admission-baseline condition audit — 2026-09-06

The baseline row has the following concrete evidence mapping. These tests
exercise different authorities: immutable encoding/retention, authenticated
transfer, atomic domain merge, receiving-owner readiness and public handoff.
None alone proves all five boundaries.

| Required condition | Named evidence and asserted boundary |
| --- | --- |
| Explicit empty baseline | `empty_baseline_is_explicit_and_mutation_cannot_change_frozen_pages`: one policy-only page, missing pages cannot finish, atomic empty installation preserves the model. `explicit_introducer_runtime_admits_but_adoption_withholds_readiness`: real empty-source catch-up is not introduction-ready until completion. |
| Complete ordered pagination | `pages_require_complete_ordered_identity_bound_content`: two pages/41 records, skipped/duplicate/missing pages rejected. `completion_requires_every_page_and_current_identity_authority`: confirmation requires both issued pages and matching root; a later certificate restriction revokes access. |
| Source changes during transfer | `source_changes_between_pages_preserve_frozen_transfer_and_refresh_next_baseline`: real mTLS QUIC, source policy/blocklist changes after page 0, original transfer remains coherent, fresh transfer has a newer snapshot/different root and updated records. |
| Immutable retry and finite retention | `retained_retry_is_immutable_and_expiry_does_not_reuse_snapshot_identity`: exact retry preserves bytes/descriptor despite source changes; expiry refuses continuation and a new begin gets a new ID. `live_leases_are_not_evicted_and_continuations_are_requester_and_generation_bound`: four leases, overload without eviction, requester/generation guards. |
| Truncated, cross-snapshot, expired or corrupt transfer | The four `peer::catchup::client::tests` fault cases each fail over real QUIC without credential confirmation, then complete a fresh two-page transfer using the same connection and one-slot exchange pool. |
| Newer receiver state and atomic rejection | `newer_local_restrictions_survive_stale_and_empty_source_views`, `late_invalid_record_rolls_back_policy_blocklist_gossip_and_removal`, `conflict_wrong_identity_duplicates_and_union_capacity_are_atomic`: pure production merge preserves newer restrictions and rolls back invalid/conflicting/over-capacity installation. |
| Incomplete/cancelled transfer and exhaustion | `explicit_introducer_runtime_admits_but_adoption_withholds_readiness` holds/cancels a prepared job, observes retained failure and no token, then completes automatic retry. `exhausted_catchup_attempts_remain_non_introducing` refuses a fourth attempt; `adoption_deadline_refuses_successful_catchup_completion` refuses late verified success at the owner deadline. |
| Stale completion and self-removal | Named runtime leave/ejection/shutdown/session-loss regressions preserve generation/session fences. `self_removal_is_explicit_and_never_readiness` and `ejection_fences_prepared_catchup_and_its_late_cancellation` pair pure self-removal with real-owner suppression of token/readiness restoration. |
| Public introducer handoff | `check_public_adoption` polls B's `joined` operation and introduction readiness, retrieves formation-bound material through B and admits C through it. The process run below passed these assertions before a later readmission convergence failure; it did not pass the whole journey. |

The new changing-source fixture runs both transfers within ten seconds using
the real source retention/resource handler and receiver. Source changes use
ordinary domain commands at the fixture boundary; this is not a multi-process
policy partition or a claim of globally latest policy. The four existing
receiver faults remain separate named regressions in the shared fixture.

Verification passed: 12 worker catch-up tests, all 149 default worker tests
(111 library, 18 binary, 20 integration), five pure baseline tests, scoped
worker Clippy with warnings denied, formatting and docs checks (104 files):

```sh
cargo test --locked --offline -p orishu-worker --lib peer::catchup --quiet
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-membership baseline::tests --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

`make test-formation` **failed** at “readmission retains dead history and three
live identities,” after the earlier handoff, voluntary leave and receiving
worker readmission assertions. Its six evidence-helper and nine HTTP-helper
tests passed. Private failure evidence is retained at
`/tmp/orishu-formation-failure-7t0gehiq`. The final recorded summaries show A
with three records/two live identities while B and C have four records/three
live identities. All processes were alive before harness cleanup. This does
not identify the cause or justify increasing the unchanged convergence budget.
The failure prevents a green final formation acceptance claim; preserve it
alongside later reproductions or resolutions.

A second run of `python3 scripts/check-formation-cli.py` against the unchanged
`target/debug` executables and unchanged deadlines **passed** the complete
handoff/leave/crash/restart/readmission journey. No production fix was applied.
The first failure remains unexplained. The initial diagnostic report lacks
the peer-session and rejected-input details needed to distinguish delayed
reconciliation, stale-session recovery or rejected membership state; further
reproduction must collect evidence at those boundaries rather than infer a
cause from aggregate counts. Optional-feature suites and release builds were
not rerun in this increment. The build retains the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning.

## Readmission reproducer and diagnostic reruns — 2026-09-06

Following the diagnose workflow, the unchanged full journey was run four times
with temporary secret-free worker debug instrumentation: two serial runs and
two concurrently executing, isolated runs. All four **passed**. The debug
probes reported registry connection counts, owner/exchange/rejection counters
and finite rejection categories; they changed no core transition, timeout or
test assertion. No failed instrumented run was captured, so these results do
not discriminate delayed reconciliation, stale-session recovery and rejected
membership state. The original failure remains open.

All `[DEBUG-readmission]` output and its registry inspection helper were
removed, then ordinary worker/CLI binaries were rebuilt. The new
`--readmission-only` harness mode calls the existing public journey, preserving
handoff, policy, leave/replay and readmission assertions and their deadlines.
It stops with normal bounded cleanup immediately after the former failing
checkpoint, before killing/restarting B. It also skips the separate standalone
CLI checks. Other scenario flags are rejected before process startup, and its
success message explicitly disclaims full conformance. No worker hook or
production API is introduced.

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --readmission-only
```

Two isolated executions of this diagnostic command **passed** against the
rebuilt, uninstrumented `target/debug` binaries; they overlapped in time.
Those runs establish that the new mode reaches and checks its intended
production boundary, not that the intermittent failure has been corrected.

Two routing/conflict regressions exercise the real harness entry point with
process creation substituted only for those argument tests. They are not
formation evidence. The complete HTTP/helper suite **passed 11 tests**, and
the failure-evidence suite **passed six tests**. Temporary instrumentation was
removed before formatting validation; documentation checks passed (104 files).
No Rust runtime fix was made, and this increment does not close the original
failure or the full N-FORMATION/M4 gates.

Final `cargo fmt --all -- --check`, `make docs-check` and scoped whitespace
checks passed. Rust unit/workspace suites, optional telemetry and fault-feature
builds were not rerun; production Rust source is unchanged after removing the
temporary debug probes. The build still reports the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning.

## Bounded post-failure readmission evidence — 2026-09-06

The original failure report retained aggregate counts, but raw identity
redaction prevented reliable correlation of individual member records. The
public harness now captures one `ls` response from each of the three workers
only after the readmission-convergence assertion fails. Before redaction, it
projects records onto four fixed harness roles: original introducer, second
introducer, departed identity and readmitted identity. Each role reports record
count, finite liveness states and a certificate-binding match boolean; the
projection also counts unexpected records. No identity strings, names,
endpoints, payload text or credentials enter this projection.

The capture refuses more than 64 KiB before JSON parsing or more than sixteen
decoded records. Every subprocess retains the existing three-second bound;
three reads permit three additional three-second subprocess waits plus bounded
local capture work before normal cleanup. Missing identity, failed command, timeout and invalid data do not
become equivalent: unavailable observations are explicit. These sequential
reads are later than the failed assertion and are not an atomic cluster view.
The assertion is never retried, its deadline is unchanged, and later convergence
cannot change the original failed result.

Three new regression tests exercise the actual projection/capture helper:
role correlation without exported identities, absent record versus failed or
timed-out read (one attempt per worker), and malformed/oversized input. The
full socket/harness suite **passed 14 tests** and the existing failure-evidence
suite **passed six tests**:

```sh
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
make docs-check
```

This is harness evidence hardening, not a membership fix or new diagnostic API.
Five consecutive `python3 scripts/check-formation-cli.py --readmission-only`
executions **passed**, stopping the loop on any failure. They used unchanged
ordinary `target/debug` binaries and the existing convergence deadline. No
failure activated the new capture during these process runs; its error paths
are established by the focused helper tests, not claimed as reproduced process
fault evidence. Final documentation validation (104 files) and scoped whitespace
checks passed. No worker/core source, wire contract or timeout changed; Rust
suites, feature/release matrices and the full crash/restart journey were not
rerun in this increment.

The original intermittent readmission-convergence failure remains open until
its cause is established and corrected or its acceptance contract is explicitly
reconciled with evidence. Passing repetitions alone cannot close it.

## Client-service metrics and real Prometheus ingestion — 2026-09-07

The optional worker IO shell now aggregates six client-service instruments:
in-flight, completed, HTTP 4xx, HTTP 5xx and cancelled handler counts, plus
cumulative completed-handler duration in seconds. Counters saturate; duration
uses a microsecond accumulator. No request bytes, route/path, identity or error
text is retained. Independent atomic reads are not a transactional snapshot.
The middleware is attached only when both the diagnostics listener and metrics
route are runtime-enabled. Diagnostics have a separate service and cannot
recursively count scrapes. No dependency or membership-core boundary changed.

Accounting covers service-handler execution, including authentication and
unknown route/method responses, not HTTP parsing failures before service entry,
socket response delivery or domain command acceptance. A dropped future releases
its in-flight count and increments cancellation rather than completion/duration.
Latency percentiles, per-route histograms, transport accounting and the remaining
formation/trace instruments are still required separately.

`loopback_diagnostics_expose_real_health_and_runtime_disable_binds_nothing`
now verifies an actual worker's initial CLI/client summary is counted; repeated
scrapes leave that count unchanged; real Unix client 401/404/405 responses add
exactly three completions/rejections; and no supplied hostile path appears in
metrics. The cancellation test drives the actual middleware/flow-control future
into a waiting handler, aborts it, and checks released occupancy and distinct
cancellation. This latter test is a middleware boundary, not evidence that
every client disconnect cancels an admitted operation. A unit test also covers
5xx classification, saturation and duration exposition. An initial compile
check exposed overly narrow module re-export visibility, corrected before the
passing tests below.

Worker all-target suites **passed** sequentially against this dirty-worktree
checkpoint and shared `target/debug` paths: observability 160 tests (111 library,
21 binary, 28 integration); observability plus fault fixtures 162 (111/23/28);
default 149 (111/18/20). Scoped Clippy with warnings denied passed for all three
configurations. Commands:

```sh
cargo test --locked --offline -p orishu-worker --features observability --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --features observability,formation-fault-test --all-targets -- -D warnings
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
```

The pinned Prometheus 3.5.0 harness **passed** with the expanded twenty-three
series and unchanged three alert rules. It parses the actual worker response
under the new 6 KiB bound, verifies the positive client-completion counter from
real operator requests, performs a real scrape, stops the scraper while
authenticated lock/unlock continues, and requires new ingestion after restart:

```sh
make test-worker-prometheus PROMTOOL=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/promtool PROMETHEUS=/tmp/orishu-prometheus-check.r4ZZNx/prometheus-3.5.0.linux-amd64/prometheus
```

The catalogue, parser harness, protocol guide, operator manual/story, task and
roadmap now agree on this partial surface. Broader M4 observability gates and
the intermittent readmission-convergence failure remain open. Full-workspace
tests, independent-process formation journeys, packaged release checks and
enabled/disabled overhead measurements were not rerun; the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

The three membership dependency-purity tests also **passed** through
`cargo test --locked --offline -p orishu-membership --test dependencies --quiet`.
Final formatting and scoped whitespace checks passed. The request exposition
regression renders maximum integer values to enforce its fixed output
contribution. An earlier documentation check passed 104 Markdown files, but the
final repository-wide recheck **failed** after concurrent Kagami ADR additions:
ADR 0020 links to missing `tasks/kagami/define-composed-object-execution.md`,
ADR 0021 to missing `tasks/kagami/implement-particle-emitters.md`, and ADR 0022
to missing `tasks/kagami/implement-kagami-viewport-workflows.md`. Those unrelated
files were not changed by this increment. The latest docs result is failed,
not superseded by the earlier green result.

## Readmission failure reproduced with role evidence — 2026-09-07

The diagnose reproduction phase ran a finite batch of at most ten unchanged
`--readmission-only` journeys, stopping at the first failure. Trials 1–8
**passed**; trial 9 **failed** at the original “readmission retains dead history
and three live identities” assertion. Trial 10 was **not run**. The shell loop
ended through `break`, so its zero exit status is not a passing test result.
No runtime fix, diagnostic logging, harness assertion or deadline change was
made during this batch.

The worker and CLI were rebuilt with ordinary default features:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --readmission-only
```

The repeated command used `target/debug` throughout; no competing feature build
replaced either executable. The dirty-worktree base was
`4d6cfba4021e7feeb988c525c7f6fac03150a2ff`, not a clean revision. Executable
SHA-256 values identify the actual build:

| Executable | SHA-256 |
| --- | --- |
| `target/debug/orishu-worker` | `18800b179888fd1310bf010407a6b6128fef8b03c32c4cc2bbe1272a3e2225c3` |
| `target/debug/orishuctl` | `7d6f9cf179f3cce89570ef62b1456dca63fc58e399d6d4b23a8959d6d5f6d478` |

Private failure artifacts are retained at
`/tmp/orishu-formation-failure-nyk0z35g`. Unlike the original failure, this run
activated the bounded public-list role projection. Its post-assertion reads
reported:

| View | Original / second introducer | Departed identity | Readmitted identity | Unexpected records |
| --- | --- | --- | --- | --- |
| A | Both Alive, expected certificate bindings | Dead, expected binding | Absent | 0 |
| B | Both Alive, expected certificate bindings | Dead, expected binding | Alive, expected binding | 0 |
| C | Both Alive, expected certificate bindings | Dead, expected binding | Alive, expected binding | 0 |

All three processes were alive before failure cleanup. These sequential reads
are not an atomic snapshot, but distinguish a missing record at A from a
wrong-liveness or wrong-certificate explanation of its summary. They do not
show whether A received and rejected the record or never received it. Ordinary
logs contain listener startup only. The original ten-second assertion remains
a failed acceptance check, not a demonstrated production convergence SLO.

The next diagnostic experiment should distinguish these ranked predictions:

1. Delayed reconciliation: a bounded observation after the failed assertion
   eventually sees the exact new identity at A, without any operator mutation.
2. Stale-session delivery failure: bounded session/reconnect evidence shows
   repeated rejection or absence of a usable route after C changes identity.
3. Membership rejection: the missing record reaches A, but a finite merge
   diagnostic identifies the rejected identity/certificate/version condition.

Collect only the evidence needed to discriminate those cases; preserve the
original failed result even if later reads converge. A post-failure observation
window must be explicitly bounded and cannot become a longer pass deadline.
No hypothesis is established yet, and the full process journey and M4 remain
open. The discovered count-only readmission success check also needs exact
identity/fingerprint/liveness assertions across all three views; that separate
validation improvement must not be presented as a fix for this failure.

The build succeeded with the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning. Rust suites, optional-feature matrices, release
checks and the full crash/restart journey were not rerun in this investigation.

## Bounded late observation and exact readmission views — 2026-09-07

The next diagnose experiment adds read-only evidence after a failed assertion,
not a longer pass deadline. `capture_readmission_failure` preserves the existing
three-worker role projection. Only `--readmission-only` then waits twenty
seconds and captures one more public `ls` response per worker. The two sets
are labelled `readmission-diagnostic` and `readmission-late-diagnostic` and
remain in the sixteen-entry evidence tail even when the CLI records its own
six responses. The original assertion is re-raised regardless of later state.

Each read retains the existing three-second subprocess timeout and bounded
projection. Thus the diagnostic mode permits six such waits plus twenty
seconds (38 seconds plus bounded local work) before failure cleanup. Normal
and fault process journeys still take only the initial three reads. Failed,
malformed and unavailable reads never become evidence of absence or recovery.
No runtime logging, new worker endpoint, mutation or recovery attempt is used.

Three helper tests were written before this capture wrapper existed and failed
with the expected missing-function errors; they pass with its implementation.
They verify both snapshots surviving the real bounded evidence container,
later appearance without losing initial absence, no repeat/wait in normal
mode, and a fixed six reads even when every lookup fails. Their wait is mocked;
they do not establish real twenty-second process observation evidence.

A finite batch of ten `python3 scripts/check-formation-cli.py --readmission-only`
runs **passed** against the unchanged default binaries identified in the
[preceding reproduction record](#readmission-failure-reproduced-with-role-evidence--2026-09-07).
No failing run activated the late capture. This does not establish or refute
delayed reconciliation, stale-session delivery failure or record rejection.
The original and reproduced failures remain unresolved; further diagnosis
must obtain evidence at those boundaries rather than count these passes as a
runtime correction.

Separately, the public journey's readmission success condition was strengthened.
The original four-record/three-live summary poll is unchanged. A second public
list poll uses only the original polling budget's remainder and requires all
three workers to hold the exact expected identity/certificate/liveness map.
The old identity must remain Dead, the readmitted identity must be Alive with
its retained certificate, and neither duplicate labels nor aggregate counts
can hide duplicate/unknown IDs or wrong bindings. As elsewhere in this harness,
the polling cutoff is checked between bounded CLI reads, not an assertion of
hard real-time completion within ten seconds.

Three additional predicate tests were written before the predicate existed,
failed for that missing function, then passed. They cover reordered valid
records, count-preserving wrong/duplicate IDs, wrong certificates, swapped
liveness, missing/extra records and malformed input. These are harness
regressions, not a membership-core bug reproduction.

After the exact-view change, the full ordinary public journey **passed**:

```sh
python3 scripts/test_formation_http.py
python3 scripts/test_formation_evidence.py
python3 scripts/check-formation-cli.py
make docs-check
```

The helper suites passed 20 and six tests respectively. The real process run
includes standalone CLI authorization checks, A-to-B-to-C handoff, policy,
leave/receipt replay, exact readmission views, crash/restart/readmission and
bounded cleanup; it is not the diagnostic subset. Documentation validation
passed 111 Markdown files and scoped whitespace checks passed. The worker
manual describes both the stronger assertion and diagnostic-only wait.
No Rust source, executable, wire profile or worker deadline changed. Rust
suites, optional telemetry, release matrices and separate fault-process
journeys were not rerun. These results close the count-only assertion gap,
not the intermittent regression or full N-FORMATION/M4 acceptance.

## Late reconnect replacement evidence and diagnostic cleanup — 2026-09-07

The readmission investigation first ran twenty `--readmission-only` journeys
against a default-feature worker with temporary `[DEBUG-readmission-round]`
probes. All twenty **passed**. The probes recorded owner counts, open/retained
connection counts, outstanding reconciliation round number/exchange count,
target liveness and remaining timer duration, plus finite registry errors and
core diagnostic discriminants. No identity, credential or payload was logged.
Logs used the existing bounded harness failure tail; no failed process run
captured them or activated the later snapshot. Instrumentation can perturb
scheduling, and these passing repetitions neither explain nor resolve the
original failure.

All temporary probes and the registry connection-count helper were removed,
their absence checked, and ordinary worker/CLI binaries rebuilt. No production
behavior or timeout change remains from the experiment. Rather than another
unchanged repetition batch, the next diagnostic fixture should control the
reconciliation/leave timing at an existing runtime/wire seam and observe the
outstanding round's disposal and subsequent membership delivery. It must
reproduce the missing-record behavior before being treated as an explanation
of the public-process failure; a plausible timing mechanism is not a cause.

Separately, the existing but previously unmapped runtime test
`replacement_fences_successful_join_reconnect_and_preserves_new_operation`
was inspected and **passed** in a named focused run and the default worker
suite. It holds an actual successful pinned TLS/application handshake for a
pending-join reconnect, changes the source lifecycle through the real owner,
and prepares a new operation before delivering the old result. The held
connection is still open before delivery and must become `LocallyClosed`
within one second afterward, without the fixture closing it. The new operation
remains Connecting; the old operation's status, replacement generation and
summary are unchanged; the target still has only its own member. Dropping the
new preparation then yields its own FailedBeforeAdmission cancellation result.
The whole fixture is bounded to ten seconds and joins normal shutdown tasks.

This fills the inventory's non-shutdown late-success pending-reconnect case.
Lifecycle replacement is deliberately staged below the public API's busy
guard; it does not authorize public leave during unresolved admission. It is
real wire/runtime evidence inside one process, not a readmission-regression
fix or proof of every stale-IO combination. The overall stale-IO row remains
partial until its remaining completion-class mapping is reconciled.

After diagnostic cleanup, verification **passed**:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --lib replacement_fences_successful_join_reconnect -- --nocapture
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo fmt --all -- --check
make docs-check
```

The default worker suite passed 150 tests (112 library, 18 binary, 20
integration); the focused run passed one named test. Documentation validation
passed 111 Markdown files and scoped whitespace checks passed. The build
retains the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.
The complete ordinary crash/restart journey, fault-process scenarios,
optional telemetry and workspace-wide Rust suites were not rerun after this
cleanup. N-FORMATION and combined M4 acceptance remain incomplete.

## Absolute HTTP/2 assembly watchdog — 2026-09-07

The active partial-frame counterexample was rerun on the dirty worktree and
survived for twelve seconds. Renaming it to
`http2_active_partial_frame_expires_despite_progress` and requiring server
expiry within five seconds plus two seconds of scheduling margin failed at
seven seconds before the fix. It passed at approximately five seconds after
the fix. The test never closes the attacker socket to manufacture expiry.

The shared client-server acceptor now observes plaintext HTTP input outside
both Unix and TLS streams. A fixed-size observer tracks the preface, frame
header/payload completion and a whole HEADERS/CONTINUATION block. Frame
progress cannot renew the original five-second budget. A separate, bounded
watcher aborts the offending connection even when the decoder is not polling;
stream drop cancels the watcher. Payloads are neither decoded nor buffered
by this observer. Hyper retains HTTP/HPACK validation and stream semantics.
No dependency, peer protocol, core authority or runtime exposure default
changed. Exact timing and connection-wide expiry semantics are documented in
the [client ingress contract](../protocol-client.md#formation-client-ingress-limits)
and worker manual. This is not a response-credit deadline or connection-age cap.

Evidence added at the production boundaries:

- `client_assembly::tests`: six tests cover fragmented/coalesced reads,
  payload progress, a continued block's retained deadline, new frame budgets,
  HTTP/1 bypass, refusal of already-expired input, independent supervision
  and watcher cancellation on drop.
- `http2_active_partial_frame_expires_despite_progress` and
  `tls_http2_active_partial_frame_expires_despite_progress`: actual executable
  Unix and certificate-verified TLS listeners expire trickled partial frames.
- `http2_continued_header_block_has_one_absolute_budget` and its `tls_`
  counterpart: three completed non-final CONTINUATION frames remain below
  the decoder's count limit but cannot extend logical assembly beyond five
  seconds. Fixtures require at least four seconds before disconnect, so
  immediate protocol rejection cannot pass as timeout evidence.
- `http2_completed_assembly_preserves_long_lived_multiplexing`: a fragmented
  frame and continued header block finish within budget, two streams return
  HTTP 200, and that same Unix connection answers PINGs for another six seconds.
  This is the real-connection control against a blanket age limit.
- `client_pressure_tests::assembly_expiry_reclaims_the_only_accepted_connection_slot`:
  the production server is configured with one connection slot. An incomplete
  HTTP/2 frame occupies it; a queued HTTP/1 request cannot dispatch until
  server-driven assembly expiry drops the first transport. The test observes
  that drop and successful dispatch while retaining the stalled client.

Verified commands on the dirty tree (base
`9d71b752902a3aaeca376968d8f03424a2361b20`, default `target` directory):

```sh
cargo test --locked --offline -p orishu-worker --bin orishu-worker client_assembly -- --nocapture
cargo test --locked --offline -p orishu-worker --test standalone http2_ -- --nocapture
cargo test --locked --offline -p orishu-worker --test standalone http2_completed_assembly -- --nocapture
cargo test --locked --offline -p orishu-worker --bin orishu-worker assembly_expiry_reclaims -- --nocapture
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo test --locked --offline -p orishu-worker --features observability --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
cargo clippy --locked --offline -p orishu-worker --all-targets --features observability -- -D warnings
```

Final reruns after the long-lived and one-slot controls passed: **167 default
tests** (116 library, 25 binary, 26 executable integration) and **179
observability tests** (116 library, 29 binary, 34 executable integration).
Clippy passed for both feature sets. One attempted final default run failed
52 socket-creating tests with sandbox `Operation not permitted` (64 passed);
the same sequential validation batch was rerun with local socket permission
and passed. Separate feature builds were run sequentially, not while another
process test used their binary. Worker formatting, scoped whitespace checks
and `make docs-check` passed (115 Markdown files). No temporary debug logging
was added. The cause was transport inactivity protection being used without an
independent pre-dispatch assembly budget; the regression now asserts the
absolute bound rather than equating incoming traffic with safe progress.

The remaining ingress audit includes inbound flow-control violations and
withheld-response-credit deadline semantics. These assembly tests do not close
those requirements, all lifecycle/recovery rows, the full fault-process reruns,
or M4's remote monitoring, trace export and operator handoff. Workspace-wide
Rust checks and the fault-feature matrix were not rerun by this increment.

## Controlled departure round and readmission budget — 2026-09-07

`peer::readmission_timing_tests::departed_reconciliation_round_can_delay_learning_a_readmitted_identity`
provides a controlled counterexample to the old ten-second readmission
expectation. It uses a real production membership owner, QUIC/mTLS sessions,
application handshake validation, serialized messages and the normal
five-second owner reconciliation cadence and ten-second core round timeout.
No worker fault flag, runtime scheduling override or new dependency is added.

The fixture starts with already-admitted test models, completes a real pull
to clear startup routing uncertainty, then starts another ordinary owner
reconciliation command one second after that periodic round. The chosen peer
receives the actual request and withholds its reply. Authenticated gossip and
a correlated Ping/Ack establish all three original IDs as Alive before that
peer sends its authenticated self-departure. The owner closes its connection
and marks the old identity Dead, but retains the outstanding core round.

The surviving test peer connects through the same production handshake and
answers probes without piggyback gossip, deliberately requiring anti-entropy
repair. It holds a new record using the departed peer's certificate and a
distinct ID. At ten seconds the owner still has only the three original
records and no new identity. After timeout and the next cadence, a new real
PullReq receives the missing record through a validated PullReply. The owner
then has four records, three live identities, the expected certificate binding
and the old Dead record. The completed focused run measured about 13.985 seconds
from surviving-peer connection to repair, within the seventeen-second
observation budget. The whole fixture, including normal shutdown, is bounded
to forty seconds.

This is an authenticated wire/owner timing fixture, not three worker processes
or a new admission procedure: models and the readmitted record are arranged
as test-peer source state; the actual CLI assignment/recovery path remains
covered by the process harness. It proves a possible missing-record timing
window under the existing contracts. It does not establish which exact
internal events occurred in the two historical process failures, whose
internal timing was not captured.

An initial partial-view version passed at about 13.990 seconds. Strengthening
the setup to require three live original records exposed a fixture failure
before departure: its test peer answered Pings but did not refute startup
suspicion. The adapter now applies the actual membership core's refutation
transition and emits its Alive/ACK with the resulting incarnation. The
strengthened focused test and default worker suite passed afterward. The
initial setup failure is not attributed to the worker runtime.

The harness now uses `READMISSION_CONVERGENCE_SECONDS = 10 + 5 + 2`: one
outstanding round timeout, one owner cadence and scheduling margin. Summary
counts and exact ID/certificate/liveness checks share that polling budget;
the second check gets no fresh window. Other scenario deadlines are unchanged,
and post-failure capture still cannot turn a failed assertion into success.
The owning peer document, worker manual, task and roadmap distinguish this
evidence-based assertion correction from a runtime fix or production SLO.
The former ten-second failures remain recorded rather than overwritten by
passing retries. Failures under the corrected budget require investigation.

Verification passed:

```sh
cargo test --locked --offline -p orishu-worker --lib departed_reconciliation_round -- --nocapture
cargo test --locked --offline -p orishu-worker --all-targets --quiet
cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings
make test-formation
cargo fmt --all -- --check
make docs-check
```

The default worker suite passed 151 tests (113 library, 18 binary, 20
integration). The full Make journey passed its six evidence-helper and twenty
HTTP/helper tests, standalone authorization checks, public handoff, policy,
leave/receipt replay, readmission, crash/restart/readmission and cleanup. This
was the complete journey, not the diagnostic subset. Documentation checks
passed 113 Markdown files after concurrent unrelated documentation additions;
scoped whitespace checks passed. The existing `proc-macro-error2 v2.0.1`
future-incompatibility warning remains. Optional telemetry, fault-feature
process journeys, release exclusion and full-workspace Rust validation were
not rerun; final N-FORMATION and M4 remain open.

A second `python3 scripts/check-formation-cli.py` execution also **passed** the
complete ordinary journey with the revised budget. It used the same rebuilt
default binaries, not a feature or fault build. Final documentation and scoped
whitespace checks passed after the task/roadmap reconciliation.

## Still required beyond this matrix

N-FORMATION's trust/bootstrap, unsupported-command honesty, configuration,
identity/credential persistence, secret handling, compatibility, platform and
workspace validation gates are accepted at the
[final checkpoint](#final-formation-acceptance-disposition--2026-09-07).
Retain their stated limits and regressions; the historical audit briefs above
do not reopen them. The remaining full-goal work is the M4 companion handoff.

P-OBSERVABILITY slices 1–3 and formation P-OBS-DOCS remain incomplete. The
local feature-gated listener, health gauges and owner/process supervision are
partial delivery; full feature/security/probe matrices, Prometheus integration,
formation instrumentation, cross-peer traces and tested operator recipes are
mandatory for combined M4. This audit does not defer or substitute for them.
