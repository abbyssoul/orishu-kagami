# Source-built worker monitoring under a user service

Scope: **tested Linux user-scoped systemd recipe; not a supported release package**.
This optional recipe runs a source-built standalone worker under an existing
user service manager. It does not install packages, enable a boot service,
configure login lingering, join a formation or modify the packaged system unit.
It supplies the user-service portion of P-OBS-DOCS slice 3; container/system-wide
deployment and the remaining M4 correlation handoff are separate work.

## Prepare and start explicitly

Build the combined-capability worker and CLI from the intended checkout:

```sh
cargo build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/worker-service-enabled
```

Keep these executable paths stable while the service is running. A later build
can replace an executable; record the checkpoint, features and binary hashes
before using it as acceptance evidence. The executable remains independently
usable without systemd or any monitoring backend.

Use an account with a running user manager and a private, owner-only working
directory. The parent of the Unix socket must be owned by that account and not
group/world writable. Keep state persistent across explicit stops; it contains
`identity.json` with private key material and the private `operator.token`.
Never point a test service at another worker's state or socket.

Copy the [unit example](../etc/systemd/orishu-worker-poc.service.example) to a
new file named `orishu-worker-poc.service` in that private directory. Edit every
placeholder before linking it:

| Placeholder | Value for this recipe |
| --- | --- |
| `WORKER` | Absolute path to the source-built worker executable |
| `STATE_DIR` | Absolute private state directory owned by this worker alone |
| `CLIENT_SOCKET` | Absolute socket path in the private working directory |
| `WORKING_DIRECTORY` | Existing absolute private working directory |
| `OBSERVABILITY` | `true` to enable diagnostics; `false` opens no diagnostics listener |
| `METRICS_BIND` | `127.0.0.1:9168`, or an unused loopback port for this instance |
| `METRICS` | `true` for metrics and probes; `false` for probes only |

The example explicitly disables peer admission and tracing. No peer or TCP
client listener is configured. Use paths without systemd specifiers, shell
expansions or quoting metacharacters for this PoC. The unit is not a shell
script. User services inherit their manager's environment, not the invoking
shell's exports; this recipe assumes the manager has no `ORISHU_*` settings.
The test checks that precondition without printing the environment or changing
it. Do not clear another service's environment as part of this recipe.

Validate, link for this manager lifetime, and start only the named unit:

```sh
systemd-analyze --user verify /absolute/private/poc/orishu-worker-poc.service
systemctl --user link --runtime /absolute/private/poc/orishu-worker-poc.service
systemctl --user start orishu-worker-poc.service
systemctl --user show orishu-worker-poc.service --property=ActiveState,MainPID,Result
```

Choose a different unit name if that name is already used. Runtime
[linking](https://github.com/systemd/systemd/blob/v257/man/systemctl.xml)
makes the file available to the manager; it is not boot enablement. Keep the
file accessible until the unit is stopped and unlinked. This user-manager
recipe is not a promise of unattended operation after logout or reboot.

The unit uses `Type=exec`, not `notify`: successful service start means the
executable was started, not that local initialization or formation catch-up
completed. Consult the probes and directly targeted client separately.

## Scrape and probe from the correct boundary

```sh
curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/startupz
curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/livez
curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/readyz
curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/metrics
orishuctl --host /absolute/private/poc/worker.sock --timeout 2s --output json cluster info
```

Initialization can briefly yield `503` or a refused connection. Use a finite
observation budget (ten seconds in the harness), then inspect the unit and
worker instead of retrying indefinitely. `startupz` latches initialization;
`livez` measures supervised local progress; `readyz` reports safe local service
of configured roles. Neither systemd's `active` state nor a probe proves peer
membership, introduction readiness, command acceptance or workload capacity.

Follow the [local Prometheus recipe](testing-worker-prometheus.md) using this
instance's loopback target. With `METRICS=false`, probes still work but metrics
returns 404. With diagnostics disabled, all HTTP probes are unavailable by
configuration; this is not a dead worker. Requesting diagnostics in a build
without `observability` fails startup explicitly before credentials or sockets
are created. Tracing remains independent and disabled in this example.

Loopback diagnostics trust local processes. For a remote scraper, use the
[separately tested mTLS proxy](testing-worker-monitoring-proxy.md), a dedicated
monitoring CA/credential and restricted management-network access. Do not
change this worker bind to a wildcard. The proxy and worker must share the
trusted host/network namespace for their loopback hop; separate containers do
not share `127.0.0.1` by default. A local exec probe may invoke the bounded curl
commands inside that namespace when an HTTP probe cannot supply remote mTLS
credentials. This is not a container/Kubernetes deployment test or a new
unauthenticated remote probe exemption.

Keep operator and monitoring credentials separate. Local client reads need no
token; mutations still require the worker's operator token file. Never make it
readable by a remote scraper, embed it in a unit or copy it into metrics labels.

## Stop, investigate and restart deliberately

```sh
systemctl --user stop orishu-worker-poc.service
systemctl --user show orishu-worker-poc.service --property=ActiveState,Result,ExecMainStatus,MainPID
```

The recipe sends SIGTERM to the service control group and allows ten seconds
before systemd may force termination. The harness requires clean exit status
zero, successful unit result and owned socket removal within that budget, even
with an unfinished diagnostics request. Forced termination is not graceful
shutdown evidence. Larger future exporter-flush budgets require reviewing this
service timeout together with the worker's whole-shutdown contract.

`Restart=no` is intentional: a failed probe, peer loss, collector outage or
process failure does not automatically create a replacement formation. There
is no probe-triggered timer, watchdog or restart policy in this recipe. This
does not change the existing packaged unit's policy; its publication lifecycle
remains owned by [P-INSTALL](tasks/publish-installable-artifacts.md).

After inspecting the [incident runbook](worker-monitoring-runbook.md), an
operator may explicitly start the same unit again. Restart retains its private
certificate/operator credential but creates fresh standalone formation and node
identities. It never restores an old assignment or automatically rejoins.
For a formerly joined worker, follow the
[admission-recovery stop conditions](cluster-admission-recovery.md) before any
new join. Retained credentials cannot bypass exclusion.

The base recipe captures worker stdout/stderr but leaves structured logging and
tracing disabled. The
[bounded stdout adapter](../apps/orishu-worker/README.md#structured-stdout-logs)
is implemented; the [enabled collection check](#verify-enabled-journal-and-trace-collection)
below verifies actual journal/Collector receipts separately from the base
recipe's credential-exclusion assertion. Journal storage is not durable
audit/provenance. Do not enable debug payload logging to make this recipe work.

To remove only this optional runtime link after stopping the service:

```sh
systemctl --user disable --runtime orishu-worker-poc.service
```

This leaves the source unit file and private state intact. Do not remove the
state directory to clear a failed unit or force readmission. Use only the exact
unit name; broad `disable`, `reset-failed` or cleanup commands can affect other
user services.

## Verify enabled journal and trace collection

The [current-build verification](tasks/cluster-formation-m4-checklist.md#current-build-operator-recipe-verification--2026-09-09)
reruns both this collection extension and the five-mode base recipe after the
formation/shutdown fixes. It does not grant overhead or release acceptance.

The selected M4 extension uses the same unit template, with tracing/logging
explicitly enabled in a private runtime-linked copy. It changes no permanent
unit, boot/login policy or shared journal configuration. Obtain the pinned
[official Collector 0.160.0](testing-worker-otelcol.md#pinned-prerequisites), then
run against an idle combined-feature worker and CLI:

```sh
python3 scripts/test_worker_deployment_receipts.py
python3 scripts/check-worker-deployment-logs.py --deployment systemd --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --otelcol /absolute/path/to/otelcol
```

To build and verify **both** selected deployment examples, use
`make test-worker-deployment-logs OTELCOL=/absolute/path/to/otelcol
WORKER_SERVICE_TARGET_DIR=target/formation-flow-observability`. Podman and the
cached pinned Debian base are additionally required for that combined target;
see the [container extension](testing-worker-container.md#verify-enabled-container-and-trace-collection).
The harness snapshots all executable bytes before starting; later builds cannot
replace a binary used by its explicit restart. Do not race the initial snapshot
with a build. The snapshot hashes identify each result.

The extension enables a 256-record stdout queue, 250 ms logging shutdown,
100% trace sampling, one-span export batches and 1000 ms export/shutdown
deadlines. Its Collector is a separate loopback process, with the checked-in
configuration and private receipt file. These are short-check settings, not
recommended production sampling or measured performance budgets. Normal user
service defaults remain unchanged.

Each of two explicit starts performs eight CLI requests, including a rejected
unauthenticated lock, authenticated lock/unlock and exact identity/membership
reads. Actual HTTP probes remain healthy; live trace/log counters confirm all
eight spans exported and all nine pre-shutdown records written without loss.
The worker must stop cleanly within the existing ten-second service budget and
remove its socket. Restart retains credentials but creates fresh identities;
it is deliberate test control, never a telemetry repair action.

Journal reads select the exact unit, `InvocationID`, worker PID and stdout
transport, not a recent timestamp or the whole user journal. Each start must
yield eight operation records, three lifecycle records and twelve final trace
accounting records. Every operation matches an actual Collector receipt by
trace/span IDs, event, outcome and completion time. Old-invocation records,
missing receipts, duplicates, partial/oversized JSON and credential/name markers
fail. Both starts together must match sixteen distinct received spans.

Journald may attach `_CMDLINE` and other process metadata containing configured
public names. The verifier forbids credentials in every journal field and
forbids authored-name/operation markers in the worker's `MESSAGE`; it does not
misrepresent systemd metadata as part of Orishu's fixed JSON schema. Do not put
secrets on process command lines or assume Orishu controls journal metadata.

For an operator-managed instance with these settings, obtain its invocation
identity **before stopping** and inspect only that invocation:

```sh
service_invocation=$(systemctl --user show orishu-worker-poc.service --property=InvocationID --value)
journalctl --user --unit orishu-worker-poc.service --no-pager --output=cat "_SYSTEMD_INVOCATION_ID=$service_invocation" _TRANSPORT=stdout
```

Match `trace_id`/`span_id` to the Collector receipt, not to a formation identity
or operation receipt. The automated verifier also checks the exact worker PID,
schema and byte/count limits. Lifecycle and final accounting records intentionally
have no trace IDs. Journal availability/retention is external to Orishu; missing
logs do not authorize retrying a mutation or restarting a healthy worker.

The complete script has a 360-second alarm (including the optional container
build); observations have ten seconds, ordinary commands fifteen seconds, HTTP
reads one second and Collector process exit three seconds. Journal queries
request at most 513 entries and reject more than 512 or 512 KiB of raw output;
extracted records retain the 320-byte/128-KiB worker limits. Fixture units and
private files are removed, but journal entries are retained. See the
[recorded evidence](tasks/cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09).
This verifies local-root trace/log collection through systemd, not three-worker
formation under systemd, remote/mTLS co-deployment, audit durability or overhead.
The ordinary-process admission-chain proof remains separate.

## Executable evidence

Verified on systemd 257 (Ubuntu `257.9-0ubuntu2.5`) with Prometheus promtool 3.5.0
and the source-built Linux worker/CLI checkpoint in the
[conformance ledger](tasks/cluster-formation-conformance.md#source-built-user-service-monitoring--2026-09-08).
These versions identify evidence, not a production-version recommendation.

```sh
make test-worker-user-service PROMTOOL=/absolute/path/to/promtool
```

The [harness](../scripts/check-worker-user-service.py) requires a running user
manager. It copies explicitly supplied binaries into a private temporary
directory, substitutes the checked-in unit's paths/settings, verifies that unit,
and starts uniquely named runtime-linked services. Five modes cover enabled,
runtime-disabled, probes-only, feature-omitted and omitted-but-requested startup.
Checks exercise real client authorization and lock/unlock, private file/socket
permissions, real probe responses and parsed metrics, graceful stop with pending
input, explicit restart and one deliberate fixture-worker SIGKILL followed by
operator-controlled startup. No production worker is targeted.

The test does not require a globally healthy user manager and does not repair
unrelated failed units. Every tool invocation is bounded at fifteen seconds;
worker/HTTP/Prometheus checks have their own one/two/ten-second limits. Cleanup
stops, resets failed status and unlinks only the UUID-named fixture units before
removing temporary credentials. Ordinary unit journal records remain; the
harness checks its bounded journal tail for the operator token but does not
delete shared journals. No package, permanent unit, login policy or boot target
is changed. These base tests do not establish remote-proxy co-deployment,
three-worker formation under systemd or release acceptance; enabled service-level
trace/log correlation has the separate check above.
