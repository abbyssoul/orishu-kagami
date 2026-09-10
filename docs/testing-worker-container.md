# Source-built worker container and exec probes

Scope: **tested rootless Linux Podman evaluation recipe; no published image**.
The [evaluation Containerfile](../etc/containers/Containerfile.worker-poc)
packages two already-built Linux binaries plus a conventional curl probe
client. It is separate from the root Dockerfile's unqualified release scaffold;
it does not establish package publication, Kubernetes support, remote management
exposure, or cross-peer/log correlation.

## Build an evaluation image

Build native worker and CLI binaries with both optional capabilities:

```sh
cargo build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/worker-service-enabled
container_context=$(mktemp -d)
cp target/worker-service-enabled/debug/orishu-worker "$container_context/orishu-worker"
cp target/worker-service-enabled/debug/orishuctl "$container_context/orishuctl"
```

The context must contain only these binaries, never the workspace, credentials
or worker state. The image does not compile them or infer their features; record
the source checkpoint, exact build options and binary hashes. The tested native
x86-64 binary requires glibc 2.38; the runtime must match its architecture and
support its ABI. Copying a host binary is not cross-compilation evidence.

The recorded official Debian trixie-slim base is digest-pinned:

```sh
container_base=docker.io/library/debian@sha256:abc9cb88a5587630d7f915f47b23b0668fe250fbfc6457aa4d52b534c1bbf73f
podman pull "$container_base"
podman build --pull=never --file etc/containers/Containerfile.worker-poc \
  --build-arg "BASE_IMAGE=$container_base" \
  --build-arg WORKER_FEATURES=observability,otlp-tracing \
  --tag localhost/orishu-worker-poc:review "$container_context"
```

Use a new local tag if `review` already identifies an image you need to retain.
No image is pushed. Building installs `ca-certificates`, `curl` and `libgcc-s1`
inside the image from Debian repositories, not on the host. Package resolution
is not frozen by the base digest: record the resulting image ID and package
versions, and revalidate updated images. The ledger records the tested versions;
they are reproduction evidence, not a security-support recommendation. This
evaluation image deliberately includes CLI/curl tooling; it is not a qualified
minimal production image or the final distribution contract.

## Run with explicit private storage

Use new, owner-only directories for this worker. They contain persistent
certificate/operator credentials and its local Unix socket. Do not mount a
home directory, workspace or another worker's state into the container.

```sh
container_state=/absolute/private/poc/state
container_runtime=/absolute/private/poc/run
install -d -m 0700 "$container_state" "$container_runtime"
container_uid=$(id -u)
container_gid=$(id -g)
podman run --detach --name orishu-poc-worker --pull=never \
  --userns=keep-id --user "$container_uid:$container_gid" \
  --read-only --cap-drop=ALL --security-opt=no-new-privileges \
  --network=none --restart=no --pids-limit=128 \
  --log-driver=k8s-file --log-opt=max-size=64k \
  --volume "$container_state:/state:rw" \
  --volume "$container_runtime:/run/orishu:rw" \
  --health-cmd='curl --fail --silent --max-time 1 http://127.0.0.1:9168/livez' \
  --health-interval=disable --health-on-failure=none --health-timeout=2s \
  --health-max-log-count=3 --health-max-log-size=1024 \
  localhost/orishu-worker-poc:review \
  --state-dir=/state --listen.clients=/run/orishu/worker.sock \
  --accepts.peers=false --observability.enabled=true \
  --observability.bind=127.0.0.1:9168 --observability.metrics=true \
  --tracing.enabled=false
```

The image defaults to UID 65532; this rootless recipe explicitly maps the
invoking non-root UID/GID into the container with
[`keep-id`](https://docs.podman.io/en/v5.4.2/markdown/podman-run.1.html#userns-mode).
Its two bind mounts stay writable by that account, without recursive chown,
privileged mode or broad permission changes. SELinux relabeling and other
platform-specific mount policies are not covered by this Ubuntu test.

The image's default command disables diagnostics and tracing, configures only
the private Unix client and opens no peer/client TCP port. The explicit command
above enables loopback diagnostics. There are no published ports. Read-only
rootfs does not mean the mounted state is immutable; the worker must create its
private files there. The PID/log limits are evaluation guardrails, not formation
capacity or telemetry-overhead acceptance budgets.

## Probe in the worker network namespace

```sh
podman exec orishu-poc-worker curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/startupz
podman exec orishu-poc-worker curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/livez
podman exec orishu-poc-worker curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/readyz
podman exec orishu-poc-worker curl --fail --silent --show-error --max-time 1 http://127.0.0.1:9168/metrics
podman healthcheck run orishu-poc-worker
podman exec orishu-poc-worker /usr/local/bin/orishuctl --host /run/orishu/worker.sock --timeout 2s --output json cluster info
```

`podman run` success proves process creation, not completed initialization. A
probe can initially refuse connection or report 503; use a finite observation
budget before consulting the [incident runbook](worker-monitoring-runbook.md).
The harness allows fifteen seconds, with one-second curl requests. Startup
success latches; liveness reflects local supervision; readiness is safe local
service, not peer convergence or workload admission.

The health command is an **exec probe** of the existing local HTTP contract.
Its timer is disabled for explicit testing, and
[health failure takes no action](https://docs.podman.io/en/v5.4.2/markdown/podman-run.1.html#health-on-failure-action).
No readiness/peer-count/collector signal restarts the worker. Do not enable this
health command when diagnostics are intentionally disabled: an unavailable
capability is not a failed process. Setting `--observability.metrics=false`
keeps probes available but makes metrics return 404; a minimal build rejects
`--observability.enabled=true` before creating credentials or its client socket.

Each separate container has its own loopback. The harness verifies that a
scraper with `--network=none` cannot reach this worker, while a scraper explicitly
using `--network=container:orishu-poc-worker` can parse its metrics. The latter
gets **network access only**, no worker-state bind mounts or operator token.
This is a trusted local namespace boundary, not remote authorization. Host
scrapers likewise cannot reach the worker by using their own `127.0.0.1`.

For remote monitoring, apply the [mTLS proxy contract](testing-worker-monitoring-proxy.md)
with an explicitly shared worker/proxy network namespace and a reviewed
management-network route. Publishing port 9168 or changing the worker bind to
a wildcard is not a substitute. This isolated recipe has no such route and
does not claim proxy-container co-deployment or Kubernetes validation. A future
Kubernetes recipe must preserve the startup/liveness/readiness distinctions,
credential separation and graceful termination budget; it cannot infer those
from an image build or add an unauthenticated remote metrics exception.

## Stop and restart deliberately

```sh
podman stop --time=10 orishu-poc-worker
podman inspect orishu-poc-worker --format '{{.State.ExitCode}}'
```

The image specifies SIGTERM. The test requires exit zero and removal of the
owned socket within a twelve-second observation budget around Podman's
ten-second stop allowance; a forced kill is not graceful acceptance. State
files remain. After inspection, `podman start orishu-poc-worker` explicitly
starts a fresh standalone formation/node identity while retaining credentials.
It never automatically joins or restores an old assignment. Follow the
[recovery stop conditions](cluster-admission-recovery.md) before admitting any
formerly joined worker again.

Remove only the stopped container with `podman rm orishu-poc-worker` when it is
no longer needed. That does not delete its host bind-mounted state. Do not use
volume deletion, `prune`, or credential removal as a recovery procedure.
Container log capture remains ordinary stdout/stderr. The
[bounded stdout adapter](../apps/orishu-worker/README.md#structured-stdout-logs)
is implemented; this base recipe leaves logging/tracing disabled and checks only
credential exclusion in captured output. The
[enabled collection check](#verify-enabled-container-and-trace-collection)
below verifies actual runtime/Collector receipts separately; neither output
capture nor a matching trace ID promises durable audit.

## Verify enabled container and trace collection

The [current-build verification](tasks/cluster-formation-m4-checklist.md#current-build-operator-recipe-verification--2026-09-09)
reruns both this collection extension and the five-mode base recipe after the
formation/shutdown fixes. It does not grant overhead or release acceptance.

The selected M4 extension runs a combined-feature worker and official Collector
0.160.0 in rootless Podman, using the same evaluation Containerfile. It requires
the pinned base already pulled and an idle worker/CLI build, plus the
[verified Collector binary](testing-worker-otelcol.md#pinned-prerequisites):

```sh
python3 scripts/test_worker_deployment_receipts.py
python3 scripts/check-worker-deployment-logs.py --deployment container --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl --otelcol /absolute/path/to/otelcol --podman podman
```

`make test-worker-deployment-logs OTELCOL=/absolute/path/to/otelcol
WORKER_SERVICE_TARGET_DIR=target/formation-flow-observability` builds the worker
and runs both this and the [systemd check](testing-worker-user-service.md#verify-enabled-journal-and-trace-collection).
The harness snapshots executables, builds a uniquely tagged image from those
worker/CLI copies and prints hashes and runtime-package versions. Base pulling
is disabled; the existing Containerfile's apt step can access Debian repositories
inside the build container. This is not an offline-build guarantee. It installs
no host packages and uses no workspace/credential build context.

The Collector starts with `--network=none`; the worker explicitly shares only
that namespace using `--network=container:<collector-name>`. Neither publishes
a port or has external runtime networking. OTLP uses `127.0.0.1:4318`; worker
probes/metrics use `127.0.0.1:9168` and are read with curl inside the worker.
The Collector mounts only its configuration/receipt directory and its read-only
executable. Worker state, operator credentials and the client socket are not
mounted into it. Host-side CLI requests use the worker's private bind-mounted
Unix socket, not a new network administration listener.

Both containers retain keep-id non-root execution, a read-only root filesystem,
dropped capabilities, no-new-privileges, no automatic restart and finite log/PID
limits. The worker explicitly enables stdout logging and full trace sampling
with the same small queue/batch/shutdown profile as the systemd extension.
This deliberate namespace-sharing topology is local loopback trust, not mTLS
identity isolation or a recommendation to expose plaintext OTLP remotely.

Across two explicit worker starts, eight CLI requests per start exercise exact
identity/membership reads, unauthorized-lock refusal and authenticated
lock/unlock. Probes and live trace/log counters remain healthy. Each stop must
exit zero and remove the socket within twelve seconds around Podman's ten-second
stop allowance; forced kill does not pass. Restart retains private credentials
but creates fresh formation/node identities.

The verifier reads actual `podman logs --tail=513` output after each stop, with
a 128-KiB cap. The second read must retain the complete first prefix and supply
new records; rotation/truncation cannot silently satisfy receipt checks. Each
start contributes eight operation records, three lifecycle records and twelve
final accounting records. All sixteen operation records must match distinct
received Collector spans by IDs, event, outcome and timestamp, without seeded
credentials/names. No missing export is hidden by matching only a subset.

For an operator-managed instance with logging enabled, `podman logs
orishu-poc-worker` exposes its structured records. Preserve invocation boundaries
using your runtime's collection metadata; stdout records do not carry formation
identity. Compare their `trace_id`/`span_id` with received spans, not timestamps
alone. The fixture's prefix check is a bounded two-start test, not a general
rotation or durable-retention algorithm.

The script has a 360-second alarm, 180-second image-build limit, fifteen-second
ordinary tool calls and ten-second observation budgets. Cleanup verifies UUID
labels before stopping/removing only fixture containers and their image; private
files are removed, while the pinned base and reusable build cache remain.
See the [recorded evidence](tasks/cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09).
This verifies runtime collection and local-root receipt, not three-worker
formation inside containers, new Prometheus ingestion, cross-host security,
published images, Kubernetes/cloud qualification, durable logs or overhead.

## Reproduce the acceptance

With the base already pulled, rootless Podman available, and promtool 3.5.0:

```sh
make test-worker-container PROMTOOL=/absolute/path/to/promtool
```

`PODMAN`, `WORKER_CONTAINER_BASE`, `WORKER_SERVICE_TARGET_DIR` and
`WORKER_SERVICE_MINIMAL_TARGET_DIR` can select explicit tools, a reviewed Debian
digest and isolated build directories. The [harness](../scripts/check-worker-container.py)
checks five modes across combined and minimal builds, actual client
authorization/lock/unlock, probes, Prometheus parsing, separate/shared network
namespaces, private file ownership, clean stop and explicit fresh-identity restart.
It builds only from copied binaries in a private context and binds no host ports.

Each build has a 180-second timeout; ordinary tool calls and observations have
fifteen-second budgets. Only UUID-named, matching-label fixture containers and
images are stopped/removed. Temporary test state is removed afterward. Downloaded
base images and reusable build cache remain; no global prune or host-package
change occurs. There is no published image, Docker/ARM/SELinux/Kubernetes
acceptance, full formation rerun or overhead claim. The base five-mode test does
not establish enabled trace/log collection; that has the separate extension above.
See the [base evidence](tasks/cluster-formation-conformance.md#source-built-rootless-container-monitoring--2026-09-08).
