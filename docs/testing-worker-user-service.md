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

The journal captures existing worker stdout/stderr. It is neither a structured
trace-correlated logging implementation nor durable audit/provenance. The
pending [logging-output choice](tasks/implement-worker-observability.md#accepted-logging-output-decision)
and sink-pressure/shutdown acceptance remain unchanged. Do not enable debug
payload logging to make this recipe work.

To remove only this optional runtime link after stopping the service:

```sh
systemctl --user disable --runtime orishu-worker-poc.service
```

This leaves the source unit file and private state intact. Do not remove the
state directory to clear a failed unit or force readmission. Use only the exact
unit name; broad `disable`, `reset-failed` or cleanup commands can affect other
user services.

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
is changed. Tests do not establish remote-proxy co-deployment, service-level
trace correlation, three-worker formation under systemd or release acceptance.
