# Worker monitoring incident runbook

Scope: **source-built Linux formation PoC; read-only incident triage**.
This covers peer loss, local unreadiness and missing operational telemetry.
It does not diagnose scientific steps, results or storage, and it does not
complete M4's cross-peer/log correlation or deployment acceptance.

Use this alongside the [metric catalogue](orishu-observability.md),
[scrape and alert examples](testing-worker-prometheus.md),
[secured monitoring proxy](testing-worker-monitoring-proxy.md) and
[local Collector](testing-worker-otelcol.md) recipes. Metrics are per process;
the authenticated worker interface supplies membership and operation evidence.
A monitoring credential grants no authority to mutate a formation.

## 1. Establish the target and collect bounded reads

Record the incident time, expected worker endpoint, process/build/feature set,
configured routes and whether the process restarted. Use deployment inventory,
not a reusable worker label, to select the endpoint. Do not print credential
contents or copy private state directories into incident reports.

For the local source-build example, substitute the actual private socket path.
The diagnostics default applies only when its listener is explicitly enabled:

```sh
worker_socket=/absolute/private/worker.sock
diagnostics_url=http://127.0.0.1:9168
orishuctl --host "$worker_socket" --timeout 2s --output json cluster info
orishuctl --host "$worker_socket" --timeout 2s --output json ls
for probe in livez readyz startupz; do
  curl --silent --show-error --connect-timeout 1 --max-time 2 \
    --max-filesize 1024 --write-out '\nHTTP %{http_code}\n' \
    "$diagnostics_url/$probe"
done
```

These are reads, not recovery actions. Local same-user reads need no operator
token. For certificate-verified remote operator reads, use the supported
[CLI authentication and trust configuration](../apps/orishu-ctl/README.md#operator-authentication)
and that worker's private `--operator-token-file`; do not use join material or
monitoring credentials. For remote diagnostics, follow the secured-proxy recipe
with its separate monitoring credentials. Never disable certificate checks or
open a wildcard worker listener to investigate a scrape failure.

Use exact GET paths, not HEAD, query strings or redirects. The commands retain
HTTP 503 bodies instead of hiding them behind `curl --fail`. A connection error
or curl's `HTTP 000` is no HTTP health response. Every probe has a two-second
total budget and a 1 KiB body cap. CLI `--timeout` is a request timeout; paged
`ls` also has the client's separate 30-second listing budget. A failed or
incomplete listing is not an empty formation.

Check `formationId`, `sourceNodeId`, `participation` and `view` in `cluster info`.
`view=localAtRequest` describes the queried worker's local view, not globally
current state. `nodes` counts retained records, not just reachable members;
`alive` and `introducerReady` are not process health. Keep historical operation
status separate from current participation.

If metrics are enabled, take one bounded snapshot:

```sh
curl --silent --show-error --connect-timeout 1 --max-time 2 \
  --max-filesize 32768 --write-out '\nHTTP %{http_code}\n' \
  "$diagnostics_url/metrics"
```

The appended HTTP line is for human triage; do not feed that combined output
to a Prometheus parser. Probes and metrics are separate requests and can cross
a state transition; disagreement is not automatically a worker defect. Preserve
timestamps and perform a bounded follow-up if needed. Missing series are not
zero measurements. Metrics retain process-lifetime counts across leave and
reset on restart, so compare rates/deltas only with that lifecycle in mind.

## 2. Separate monitoring failure from worker health

| Observation | Meaning and next check |
| --- | --- |
| Prometheus `up=0` / `OrishuWorkerScrapeUnavailable` | A configured scrape failed. Check target, listener/route enablement, proxy, trust, credential validity and network reachability; check worker process/supervisor state separately. It is not proof of peer death. |
| Target and its `up` series are absent | Compare expected inventory with active discovery. Per-target alerts cannot identify a target removed from discovery. A stopped Prometheus cannot evaluate its own alerts. |
| HTTP 400/404/405, or proxy authentication refusal | Check exact path/method, selected routes and access policy. Disabled routes return 404. These are not readiness responses. Probes-only configuration deliberately omits `/metrics`. |
| `/livez=200`, `/readyz=503`, `/startupz=200` | Initialization completed and the owner is responsive, but safe local service is withheld. Inspect current participation and the original join operation as below; also check shutdown intent and required-role failure. |
| `/livez=503`, with metrics still readable | Local owner supervision is not responsive. Inspect process/control-loop evidence; a responsive HTTP exporter cannot establish a healthy owner. |
| `/startupz=503` | Required local initialization has not completed. Check startup configuration, listener conflicts, permissions and bounded startup diagnostics; no cluster quorum or workload is required to become ready. |
| All probes return 200 but peer membership differs | Healthy local service does not prove membership convergence. Follow the peer-loss checks below. |

The health gauges `orishu_worker_owner_responsive`, `orishu_worker_ready` and
`orishu_worker_startup_complete` project the corresponding local inputs. Probe
bodies are deliberately generic (`owner not responsive`, `local service not
ready`, `initializing`); they do not identify a particular failed role or join
phase. The five-second owner supervision threshold is not a peer-loss timeout
or a production SLO. Startup success latches; later unreadiness does not reset it.

Probes share diagnostics connection capacity. A timeout during a scrape flood
can occur while membership and authenticated operator reads still progress.
Check access-path pressure and the [bounded diagnostics contract](protocol-client.md#worker-operational-diagnostics),
not only the process gauge last retained in Prometheus. A required-role failure
is latched in this PoC; an independently authorized restart needs formation and
admission review first, because restart does not restore old membership.

## 3. Peer loss or disagreeing membership views

Select the **assigned node ID in the expected formation**, not a display label
or the restarted process's new standalone ID. Query each reachable surviving
worker directly; use its own endpoint and credential where required:

```sh
suspect_node=FORMATION_ASSIGNED_NODE_ID
orishuctl --host "$worker_socket" --timeout 2s --output json inspect "$suspect_node"
```

Check `schemaVersion`, `formationId`, `sourceNodeId`, `nodeId`,
`certFingerprint`, `view` and `liveness`. The default indirect inspection reads
that source's membership record; it does not contact the suspected peer. Check
each result's source against the worker queried and preserve certificate
binding across `alive` → `suspected` → `dead`. If the source or formation changed,
stop treating the records as one continuous lifecycle.

SWIM decides liveness. Aggregate probe-deadline, send-failure and transport
counters can identify where to investigate, but cannot name the failed peer or
prove convergence. A Dead record is not an operator-removal tombstone. Peer
loss alone does not make survivors unready, and a locked formation can remain
healthy while correctly refusing admissions.

For an observation-only check, set a finite window before polling and wait at
least one second between polls. The three-worker crash fixture uses 60 seconds
to observe suspicion/death; this is a test budget, not a universal detection
promise. If a deployment's configured bounds require another window, record it
up front. Expiry or persistent disagreement means preserve evidence and escalate,
not restart peers, remove exclusions or submit fresh joins. Investigate the
configured peer routes, packet loss, TLS/session binding and capacity using the
owning transport diagnostics without weakening authentication.

## 4. Joining, catch-up failure or ejection

Read the original operation from the same source process with its operator
credential; use the ID retained at submission, not an inferred ID:

```sh
operator_file=/absolute/private/operator.token
orishuctl --host "$worker_socket" --operator-token-file "$operator_file" \
  --timeout 2s --output json join-status ORIGINAL_OPERATION_ID
```

Compare historical source/target and assigned identities with current
`cluster info`. A historical `joined` record does not override current ejected
or changed participation. `catchingUp`/`catchUpFailed` retain the adopted target
identity; a successful receiver transfer is not owner adoption. Catch-up
counters distinguish receiver rejection/timeout/cancellation from owner
adoption/fencing/abandonment but cannot identify which operation completed.

Follow the [admission recovery runbook](cluster-admission-recovery.md) for its
original-issuer checks, 300-second observation policy and terminal stop rules.
Do not restart its observation budget from this runbook. Missing operation
history, an unresolved outcome or inability to query the original issuer is
not proof of non-admission. Stop operator recovery actions when instructed;
that does not mean kill the worker or cancel its bounded background attempt.

## 5. Missing traces, exporter loss or queue pressure

First verify the intended build and runtime configuration. Metrics/probes and
`otlp-tracing` are independent capabilities, both runtime-disabled by default.
Disabled tracing omits trace counters; zero sampling or an idle worker can
legitimately produce no spans. Do not replay a mutation to generate a trace.

Use the [trace-loss warning table](testing-worker-prometheus.md#optional-trace-loss-warnings)
to distinguish local shedding, delivery failure with uncertain receipt and
collector-reported rejection. Inspect the collector's own availability, policy,
capacity and certificate/trust configuration using the
[Collector mTLS recipe](testing-worker-otelcol-mtls.md). Do not print private
keys, bearer values or environment dumps. Local queue loss is best-effort
diagnostic loss, not a lost command or permission to extend domain retries.

For capacity, use the catalogue's separate owner-lane, inbound-task,
registered-session, dial and reliable-exchange occupancy/capacity instruments.
The [optional formation warnings](testing-worker-prometheus.md#optional-formation-warnings)
connect those named budgets, owner supervision, admission/catch-up and transport
outcomes to this procedure. They include sensitive example thresholds, not
automatic remediation or proof of peer death.
Reserved permits count as occupied; retained registry entries are not live
sockets. A scrape below capacity does not disprove an earlier saturation event.
Check refusal counters and collector/transport progress before proposing changed
sampling or limits. Changing settings is a separately reviewed startup/deployment
action, not part of this read-only procedure. Current local traces do not yet
provide cross-peer or trace/log correlation.

## 6. Handoff and evidence limits

Preserve observation timestamps, exact targeted worker/build/feature scope,
secret-free summaries and operation references, probe HTTP/transport outcomes,
relevant counter snapshots and scrape/collector error categories. Keep endpoint
and public identity details in an access-controlled incident record; they are
not credentials, but need not be published. Never attach join material, private
credential files or raw environment dumps.

Name the unresolved assertion and the responsible worker, formation or
monitoring operator. No signal here authorizes leave, restart, fresh admission,
certificate rotation or exclusion removal. A new action needs the appropriate
authority and current formation guards.

The executable evidence is deliberately split by boundary:

| Workflow assertion | Reproduction and limit |
| --- | --- |
| Locked formation and surviving workers at suspected/dead peer checkpoints retain healthy probes/health gauges; inspection retains exact source/formation/target/certificate | `make test-formation-observability` runs the public three-worker journey with these assertions. Its process kill/restart and lock/unlock are test controls, not runbook recovery steps. |
| Collector failure and scraper shutdown do not require changing worker identity; read-only `cluster info`, `ls`, `inspect` and all probes remain usable | `make test-worker-trace-prometheus` with pinned tool paths runs the real worker/CLI/Prometheus recipe. Its collector is a bounded HTTP fixture, not a deployed telemetry backend. |
| Live-but-unready joining/catch-up/ejection, owner stalls and required-role failures are distinct | The [HTTP phase/owner evidence](orishu-observability.md#implemented-owner-supervision-input) and [formation ledger](tasks/cluster-formation-conformance.md) identify scoped runtime and process tests; not every failure is independently injected into an executable worker. |
| Missing/stale series, pending warnings, recovery and idle/reset exclusions | The [Prometheus rule fixtures](testing-worker-prometheus.md#local-alert-examples-and-operator-response) exercise the real rule engine's synthetic clock; they do not prove notification delivery. |

Run socket/process checks with local socket permission. Source-build commands,
feature selection, tool paths and cleanup budgets are specified in the linked
recipes. Fault injection belongs only in isolated test workers; never run the
formation harness against an operating cluster. This runbook does not qualify
service/container deployment, dashboards, notification routing, cross-host
monitoring security or cross-peer/log correlation. Those remain with
[P-OBS-DOCS](tasks/document-worker-observability.md).
