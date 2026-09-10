# User stories: worker administration

These stories are written from the administrator persona: the person who installs, starts, configures, and maintains individual `orishu-worker` processes so cluster users can run scientific workloads reliably.

This document focuses on a single worker instance, especially startup and configuration. Cluster-wide operations such as inspecting membership, removing nodes, or managing admission are covered in [cluster-admin.md](./cluster-admin.md).

Although `orishu-worker` is designed to run as a service, it can also be launched manually. With proper configuration, multiple instances can coexist on the same machine. These instances can be part of different clusters or the same one.

This document mentions command-line options that can be passed to the executable. These options generally override the default configuration and are used for testing and development purposes. In production, these options are typically set via configuration files or environment variables. For more details on configuration options, refer to the [configuration documentation](../../orishu-configuration.md).

## Shared assumptions

### Current formation-PoC coverage

The source-built Linux worker has evidence for config-free local startup,
private per-instance identity/credentials, explicit peer listeners and role
configuration, standalone identity, ordinary restart and safe socket reuse.
See the [worker manual](../../../apps/orishu-worker/README.md) and
[formation evidence](../../tasks/cluster-formation-conformance.md) for actual
configuration and limits. These results do not certify service packages,
containers or other operating systems, or implement the workload/resource
capabilities described by the broader stories. Loopback metrics and process
probes are available through optional enablement; secure remote monitoring,
trace export and deployment recipes remain tracked in
[P-OBS-DOCS](../../tasks/document-worker-observability.md).

Unless a story says otherwise, all stories in this document assume the following:

- `orishu-worker` is installed on the host and can be invoked by the
  administrator; installation options are covered in
  [Install orishu-worker](#install-orishu-worker).
- The administrator can run `orishu-worker` with the desired configuration source, CLI arguments, environment variables, and file paths.
- The requested configuration is syntactically valid, and any referenced files, directories, sockets, interfaces, or addresses exist or can be created as needed.
- The administrator's account has the operating system permissions required by the requested configuration, such as creating sockets, opening configured ports, reading config files, and writing runtime data.
- Host-level prerequisites outside the worker itself are already satisfied unless a story says otherwise. Examples include available ports, reachable interfaces, and any firewall or service-manager setup required by the chosen configuration.
- Package installation, system provisioning, and failure modes caused purely by missing host permissions or missing OS capabilities are out of scope here unless a story calls them out explicitly.

## Local startup and identity

### Prepare a dedicated experiment worker

As an administrator, I want a repeatable preparation check for each physical
experiment machine so that I can discover missing software, wrong binaries,
network configuration and power/thermal risks before running a benchmark.

**Given** dedicated machines and a normal key-authenticated SSH account
**When** I prepare a private lab directory and collect readiness reports
**Then** each machine reports its setup and unresolved prerequisites without
starting a worker, modifying host policy or exporting credentials.

**Acceptance criteria:**

- Preparation never overwrites an existing worker or experiment directory.
- Reports identify the actual architecture, OS, network interface/address and
  staged artifacts; missing tools and unavailable thermal/time evidence remain
  explicit, not a performance pass.
- Readiness can be collected over verified SSH without granting passwordless
  root access or uploading a private SSH key.
- Setup, ARM64 execution, cross-host formation and measured performance retain
  separate verification status.

The [Pi setup/readiness guide](../../testing-worker-pi-cluster.md) implements
the scoped preparation workflow. Actual hardware and the complete experiment
driver remain with the [physical-host task](../../tasks/implement-physical-formation-experiment.md).

### Install orishu-worker

As an administrator, I want to install orishu-worker through my platform's
package manager, as a standalone binary, or as a container image so that I can
deploy it on my nodes with minimal setup.

**Given** a host without orishu-worker installed
**When** I install it through a supported method
**Then** I get one self-sufficient binary that runs without any initial
configuration.

**Acceptance criteria:**
- Installation is available through platform-native package managers (for
  example `apt install orishu-worker` or `brew install orishuctl`), as a
  standalone binary via `cargo install orishu-worker`, and as a container
  image (for example `docker run orishu-worker`).
- Every installation method delivers the same single self-sufficient binary:
  no companion config files, scripts, or pre-created directories are required.
- The worker runs with default settings without any initial configuration,
  including on a system where it has no write access beyond its own runtime
  data; it behaves as a true portable executable.
- Any persistent local state the worker needs is created and managed by the
  worker itself under a standard OS config/cache directory; it never depends
  on files installed alongside it.
- If the worker cannot create the state it needs, it fails with a clear error
  rather than partially starting.
- Installation and first run require no cluster or network setup: the worker
  starts as a standalone node.

### Run a local instance

As an administrator, I want to be able to run an interactive instance of `orishu-worker` to test my local client against and observe its performance.

**Given** the shared assumptions above
**When** I start `orishu-worker` locally
**Then** a local worker process starts and accepts local client requests.

**Acceptance criteria:**
- The worker can be started without any configuration, e.g. `orishu-worker`, and should use default settings.
- No special permissions should be required to run `orishu-worker` locally other than the normal operating system permission to start a process. The process should run with the permissions of the user who launched it.
- No special init or configuration files should be required to start a local instance of `orishu-worker`. The process should use default settings suitable for local testing.
- The local instance of `orishu-worker` should be able to accept and process requests from local clients, allowing for testing and development purposes.
- Unless explicitly configured (via config files, environment variables or cli options), the local instance of `orishu-worker` should accept local connection via Unix socket `$XDG_RUNTIME_DIR/orishu/worker.sock` (typically `/run/user/<uid>/orishu/worker.sock`). When installed as a system-wide service, the socket path may be configured to `/run/orishu/worker.sock` via a config file or environment variable.
- Unless explicitly configured (via config files, environment variables, or CLI options), the local instance of `orishu-worker` should not attempt to connect to any external services or resources by itself, ensuring that it operates in isolation for testing purposes.
- The default configuration is designed to be safe to start.
- The default configuration listens only for local connections on the same machine and does not open network ports.

### Assign a node name

As an administrator, I want to be able to assign a human-readable name to a worker process that I manually start, so that I can easily identify it in cluster listings, logs, and diagnostics — similar to how Docker containers can be given a name at launch.

**Given** the shared assumptions above
**When** I start `orishu-worker` with a name
**Then** the worker process starts and is identified by the given name in cluster membership and administrator tool output; operational-log collection identifies its process/target separately.

**Acceptance criteria:**
- The worker can be started with a human-readable name, e.g. `orishu-worker --name <name>` (or equivalent configuration), and should use that name as its identifier.
- The name is distinct from the node ID. The node ID is an identifier assigned by the cluster to its members and plays a part in partition ownership. The name is a human-friendly label for display purposes. See [Node identity](../../orishu-runtime-design.md#node-identity).
- If no `--name` is provided, the worker should generate a random name automatically, ensuring that every instance has a recognizable label even without explicit configuration.
- The name should appear in cluster node listings (e.g. `orishuctl ls`) and detailed node information. The bounded operational-log contract excludes raw names from records: use external process/target collection metadata to distinguish stdout streams, not a new free-text log field or metric label.
- Multiple instances can share the same name. The name is not required to be unique within the cluster and can be interpreted as representing a family of similarly configured workers.


### Assign a cluster name

As an administrator, I want to be able to assign a human-readable name to the cluster that a worker process belongs to, so that I can distinguish between multiple clusters running on the same network.

**Given** the shared assumptions above
**When** I start `orishu-worker` with a cluster name
**Then** the worker process starts and its cluster is identified by the given name.

A freshly started worker process that has not yet joined any other peers behaves as a standalone node, i.e. a cluster of one. The cluster name is set at startup as a human-readable label for that cluster formation. When other nodes join this node, they adopt the cluster name of the cluster they are joining. This means the cluster name is determined by the initial seed node(s) and propagated to all members during the join process, but it is not itself a durable proof of cluster continuity across later re-formations.

When that first worker is intentionally bootstrapping a new cluster rather than merely starting another standalone node, administrators may also provide cluster-formation inputs that shape later admission behavior from the beginning, such as an initial blocklist. See [Bootstrap a cluster with a pre-populated blocklist](./cluster-admin.md#bootstrap-a-cluster-with-a-pre-populated-blocklist).

In environments where multiple independent clusters coexist on the same network, the cluster name allows administrators to verify — after a join command completes — that a worker joined the intended cluster and not a different one sharing the same network.

**Acceptance criteria:**
- The worker can be started with a cluster name, e.g. `orishu-worker --cluster.name <name>` (or equivalent configuration), and the resulting cluster should be identified by that name.
- If no `--cluster.name` is provided, a random cluster name is generated automatically.
- When a node joins an existing cluster, it adopts the cluster name of that cluster, regardless of what cluster name it was started with. The join target's cluster name takes precedence.
- The cluster name should be visible in administrator tool output (e.g. `orishuctl ls`, `orishuctl inspect <node-id>`) so that administrators can confirm which cluster a node belongs to.
- The cluster name is a human-friendly label for the current cluster formation, not a durable unique cluster identity. When not configured, a random cluster name is generated for display and operational convenience.


## Client access

### Run a local and remotely accessible instance

As an administrator, I want to be able to run an instance of `orishu-worker` such that I can administer it locally via `orishuctl` and other administrators can access it remotely.

**Given** the shared assumptions above
**When** I start `orishu-worker` with local and remote client listeners
**Then** the worker process starts and accepts both local and remote client requests.

**Acceptance criteria:**
- The worker can be started with local and remote client listeners, e.g. `orishu-worker --listen.clients <unix://path> --listen.clients <IP-address>` (or equivalent configuration).
- The instance should be able to accept and process requests from both local and remote clients, allowing for administration and monitoring from different locations.
- Unless explicitly configured (via config files, environment variables or cli options), the local instance of `orishu-worker` should not attempt to connect to any external services or resources, ensuring that it operates in isolation for testing purposes.


### Configure the remote client limit

As an administrator, I want to be able to specify a limit on the number of remote clients a single worker will accept.

**Given** the shared assumptions above
**When** I start `orishu-worker` with a client connection limit
**Then** the worker process starts and accepts clients on the configured address up to the configured limit.

**Acceptance criteria:**
- The worker can be started with a remote client limit, e.g. `orishu-worker --limits.clients <number>` (or equivalent configuration), and should accept no more than the specified number of remote client connections.
- When the configured remote client limit is reached, new remote client connections are rejected cleanly with an appropriate error or connection refusal, while already established client connections continue unaffected.


### Start with client listeners configured but admission disabled

As an administrator, I want to be able to specify an address for clients to connect to while keeping the worker process from accepting new client connections until I explicitly change configuration and restart the worker.

**Given** the shared assumptions above
**When** I start `orishu-worker` with client listeners configured but client admission disabled
**Then** the worker process starts and does not accept clients on the configured address.

**Acceptance criteria:**
- The worker can be started with client listeners configured but client admission disabled, e.g. `orishu-worker --accepts.clients false --listen.clients <IP-address>` (or equivalent configuration).
- The process listens on the specified address and is capable of accepting clients, but does not accept any new client connections until the administrator updates the worker configuration and restarts the process with client admission enabled.
- Any attempt to connect to the worker instance as a client should be rejected until the worker is restarted with client admission enabled.
- Once restarted with client admission enabled, the instance should accept client connections on the specified address(es) up to the configured limit.


## Peer access and work admission

### Configure peer listen addresses

As an administrator, I want to be able to specify a set of addresses that `orishu-worker` should listen on for accepting peer connections.

**Given** the shared assumptions above
**When** I start `orishu-worker` with peer listeners configured
**Then** the worker process starts and accepts peers on the configured address.

**Acceptance criteria:**
- The worker can be started with peer listeners configured, e.g. `orishu-worker --listen.peers <IP-address>` (or equivalent configuration), and should accept peer connections on the specified address(es).
- The instance should be able to accept and process peer join requests on the specified address(es).

### Configure the peer limit

As an administrator, I want to be able to specify a limit on the number of peers a single worker will accept.

**Given** the shared assumptions above
**When** I start `orishu-worker` with a peer connection limit
**Then** the worker process starts and accepts peers on the configured address up to the configured limit.

**Acceptance criteria:**
- The worker can be started with a peer connection limit, e.g. `orishu-worker --limits.peers <number>` (or equivalent configuration), and should accept no more than the specified number of peer connections on the specified address(es).
- When the configured peer limit is reached, new peer connection attempts are rejected cleanly, while existing peer connections remain established and continue operating normally.


### Start with peer listeners configured but admission disabled

As an administrator, I want to be able to specify an address for peers to connect to while keeping the worker process from accepting new peer connections until I explicitly change configuration and restart the worker.

**Given** the shared assumptions above
**When** I start `orishu-worker` with peer listeners configured but peer admission disabled
**Then** the worker process starts and does not accept peers on the configured address.

**Acceptance criteria:**
- The worker can be started with peer listeners configured but peer admission disabled, e.g. `orishu-worker --accepts.peers false --listen.peers <IP-address>` (or equivalent configuration).
- The process listens on the specified address and is capable of accepting peers, but does not accept any new peer connections until the administrator updates the worker configuration and restarts the process with peer admission enabled.
- Any attempt to connect to the worker instance as a peer should be rejected until the worker is restarted with peer admission enabled.
- Once restarted with peer admission enabled, the instance should accept peer connections on the specified address(es) up to the configured limit.


### Start in a cordoned state

As an administrator, I want to be able to start a worker process that does not accept workload assignments, so that I can use it as a relay or introducer-only node, or prepare it for maintenance before allowing it to take on work.

**Given** the shared assumptions above
**When** I start `orishu-worker` with work admission disabled
**Then** the worker process starts, participates in the cluster, and does not accept workload partition assignments.

**Acceptance criteria:**
- The worker can be started in a cordoned state, e.g. `orishu-worker --accepts.work false` (or equivalent configuration).
- The process participates in the cluster (gossip, peer connectivity, client connections if configured) but is not scheduled for any workload partitions.
- The instance remains started with work admission disabled until an administrator updates the worker configuration and restarts it with `accepts.work=true`.
- Once restarted with work acceptance enabled, the instance should be eligible for workload partition assignments according to normal scheduling rules.


## Operational observability

### Monitor worker metrics and process health

The [rootless container recipe](../../testing-worker-container.md) adds scoped
source-built evidence for private mounted state, optional metrics, exec probes
and explicit stop/restart. Probes execute inside the worker's network namespace;
disabled diagnostics are unavailable by configuration, not proof of a failed
worker. This does not complete the published-container installation story.

The [source-built user-service recipe](../../testing-worker-user-service.md)
now supplies scoped systemd evidence for explicit startup/stop, disabled and
probes-only modes, private socket/credential access and fresh-identity restart.
An active unit is not a ready worker. No failed probe or process exit triggers
automatic restart/readmission; this does not qualify the packaged service or
container installation story.

The optional [snapshot dashboard](../../testing-worker-dashboard.md) now
presents the implemented per-worker instruments, withholding values for failed
or ambiguous targets and distinguishing missing series, idle estimates and
unavailable capacity from measured zero. Its HTML/query and local browser
checks do not qualify a remote monitoring deployment.

The [optional formation alerts](../../testing-worker-prometheus.md#optional-formation-warnings)
provide tested warning examples for local owner unresponsiveness and named
capacity budgets alongside admission/catch-up and transport symptoms. Their
finite labels and example delays support inspection, not automatic restarts;
missing metrics remain distinct from zero values.

The [monitoring incident runbook](../../worker-monitoring-runbook.md) supplies
the formation-stage read-only triage workflow: separate access failure from
an HTTP health result, inspect the directly targeted worker, and escalate with
bounded secret-free evidence. The linked process/HTTP checks establish scoped
behavior, not automatic remediation or complete deployment acceptance.

Status: **partial — local probes/health gauges and route selection implemented; full monitoring acceptance pending**

The optional loopback listener currently provides three health gauges, thirteen
process-lifetime owner counters, ten inbound peer handshake outcome/capacity
counters, eight bounded-lane occupancy/capacity gauges,
seven aggregate client-service count/duration instruments and
startup/liveness/readiness routes. Metrics and probes are separately selectable
through file/environment/CLI settings; disabled groups return `404`. See the
[worker manual](../../../apps/orishu-worker/README.md#optional-local-diagnostics).
Owner counters include admission insertion, refusal and assignment replay.
The [inbound peer catalogue](../../orishu-observability.md#inbound-peer-handshake-counters)
separates transport failures, timeouts and capacity refusal from core admission
outcomes; aggregate errors alone never justify restarting or readmitting a worker.
The [inbound capacity gauges](../../orishu-observability.md#inbound-connection-capacity-gauges)
show current TLS and connection-task pressure separately from cumulative
refusals. Zero capacity with metrics enabled means no peer adapter is running;
task occupancy is not established-session or formation membership count.
The [registry gauges](../../orishu-observability.md#registered-session-capacity-gauges)
separately show retained total/provisional sessions, including outgoing
introducer bindings. An active empty registry retains capacity even without a
peer listener; owner termination withdraws that projection. Closed entries
await normal pruning. Inspect operation status and related refusal/pressure
signals before acting; occupancy alone never justifies restart or readmission.
The [reliable-exchange catalogue](../../orishu-observability.md#reliable-peer-exchange-metrics)
adds request/serve outcomes, elapsed durations, partial stream bytes and pool
pressure. Operators can distinguish pending slot use from terminal byte counts;
neither locally accepted writes nor transport success proves admission.
The [datagram/pre-pool counters](../../orishu-observability.md#datagram-and-pre-pool-traffic-counters)
separate local submission/refusal/failure, payload reception before decoding
and per-connection stream-task refusal. They do not establish all datagram loss
or identify which peer failed; monitoring remains a prompt for inspection, not
automatic recovery authority.
The [outbound dial catalogue](../../orishu-observability.md#outbound-dial-and-tls-metrics)
distinguishes attempt exhaustion, candidate TLS failure/timeout, cancellation
and four-slot capacity refusal. A failed candidate may precede successful
fallback; a completed attempt still needs owner validation, not automatic
readmission based on a transport signal.
The [membership deadline counters](../../orishu-observability.md#membership-deadline-and-abandonment-counters)
separate consumed probe/reconciliation/retry timers from stale tokens and
abandoned attempts/rounds. Cancelled timers are not failures; exhausted join
retries can retain uncertain admission and require the recovery runbook's
inspection/stop procedure, not a fresh join inferred from a counter.
The [catch-up outcomes](../../orishu-observability.md#admission-state-catch-up-outcomes)
distinguish receiver validation, source refusal, invalid data, transport failure,
whole-transfer timeout and cancellation from the owner's later adoption or
fencing decision. Pages are not attempts, and validation is not introduction
readiness. Use retained join-operation status and the recovery runbook; these
counters never authorize restart, readmission or clearing an exclusion.
The [mTLS proxy recipe](../../testing-worker-monitoring-proxy.md) now provides
tested source-build secure scraping/probes with monitoring-only certificates.
Its [stalled-reader checks](../../testing-worker-monitoring-proxy.md#downstream-response-backpressure-and-expiry)
show bounded response generation and expiry/recovery while operator control
continues; they do not qualify arbitrary monitoring floods or deployments.
Native remote diagnostics, the full probe/security matrix, remaining formation
metrics, cross-peer traces and release/service/container deployment remain work, so this
story is not complete.

Local sampled export now has live delivery/loss counters when metrics and
tracing are both enabled. The [catalogue](../../orishu-observability.md#live-trace-delivery-and-loss-counters)
explains sampling, queue pressure, collector failure and absent instruments;
these signals do not establish operation acceptance or justify worker restart.
Optional [structured stdout logging](../../../apps/orishu-worker/README.md#structured-stdout-logs)
now has independent file/environment/CLI settings, sampled trace/span correlation
and nine logging-loss counters when metrics are enabled. Zero-sampled or
disabled tracing does not suppress enabled lifecycle records. Normal disabled
logging is quiet; a slow/closed stdout reader must not make a worker unready.
The [logging-counter backend recipe](../../testing-worker-prometheus.md#ingest-logging-counters-through-prometheus)
verifies independent enablement, real broken-pipe counters, unchanged control
and probes, and fresh ingested values after scraper recovery.
The [local executable evidence](../../tasks/cluster-formation-conformance.md#runtime-logging-and-local-span-receipt--2026-09-09)
does not complete the combined service/container handoff or durable log retention.
The selected [systemd](../../testing-worker-user-service.md#verify-enabled-journal-and-trace-collection)
and [rootless-container](../../testing-worker-container.md#verify-enabled-container-and-trace-collection)
extensions now verify collected records against received spans with clean stops
and explicit fresh-identity restarts. They do not promise durable retention,
three-worker formation under a supervisor or accepted overhead budgets.
The [pinned local Collector walkthrough](../../testing-worker-otelcol.md) adds
real span receipt and inspection, disabled/zero-sampling checks and collector
shutdown/recovery while authenticated control remains usable. The additional
[three-worker walkthrough](../../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
matches received admission chains to the respective stdout streams and checks
direct metrics/probes and cross-worker policy visibility. These local HTTP
recipes do not establish remote collector security or durable backend operation.
The [mTLS receiver extension](../../testing-worker-otelcol-mtls.md) now adds
dedicated collector trust/client credentials and real startup/certificate/name
refusal evidence. Cross-host deployment and credential lifecycle qualification
remain separate from its tested loopback security contract.

As an administrator, I want to enable a Prometheus scrape endpoint and
startup/liveness/readiness probes so that I can monitor a worker and configure
service/container health checks without granting monitoring tools cluster
administration privileges.

**Given** a worker built with the optional observability capability
**When** I explicitly configure its diagnostics listener
**Then** I can scrape that worker's metrics and distinguish initialization,
process responsiveness and readiness for its configured roles.

**Acceptance criteria:**

- Build capabilities and file/env/CLI options are documented. An unsupported
  feature request fails clearly; default startup opens no monitoring port.
- Metrics-only and probe-only selection is explicit and does not implicitly
  enable a listener or grant remote access. Disabled routes expose no metrics
  or successful probe response.
- Monitoring works independently of client/peer/work admission flags and uses
  the least-privilege access policy from
  [ADR 0017](../../adr/0017-worker-operational-observability.md).
- Idle, stopped and intentionally non-compute workers are not falsely reported
  dead. Peer loss or collector failure alone does not trigger a restart signal.
  Startup, joining/recovery, draining and a stalled control loop follow the
  published probe matrix.
- Slow scrapes and unavailable collectors cannot stop cluster work. Metrics
  have documented units, bounded labels and no credentials or authored data.
- I can distinguish intentionally unsampled operations, locally shed spans
  and collector-reported outcomes while the worker is running. Missing
  instruments are not presented as measured zero or proof of domain failure.
- The manual provides tested Prometheus, service/container probe and optional
  OTLP collector examples, explains remote access, and distinguishes readiness
  from workload admission and scientific validity.

Delivery: [implementation](../../tasks/implement-worker-observability.md) and
[operator documentation](../../tasks/document-worker-observability.md).

## Storage

### Configure the storage backend

As an administrator, I want to be able to specify a storage backend for `orishu-worker`.

**Given** the shared assumptions above
**When** I start `orishu-worker` with a storage backend configuration
**Then** the worker process starts using the specified storage backend.

**Acceptance criteria:**
- The worker can be started with a storage backend configuration, e.g. `orishu-worker --storage.backend <backend>` (or equivalent configuration), and should use the specified storage backend.
- The instance should be able to utilize the specified storage backend for storing and retrieving data.
- The storage backend should be properly configured and accessible from the worker process.
- If the configured storage backend is unavailable, inaccessible, or invalid at startup, the worker fails startup with a clear error rather than partially starting with degraded or implicit fallback behavior.


### Configure the storage replication factor

As an administrator, I want to be able to configure how many copies of result artifacts and checkpoints are stored across the cluster, so that I can balance data durability against storage and memory costs.

**Given** the shared assumptions above
**When** I start `orishu-worker` with a storage replication factor
**Then** the worker process starts and replicates each data partition to the specified number of nodes.

The replication factor is a cluster-level concern: all nodes in the cluster should be configured with the same value to ensure consistent durability guarantees. A higher replication factor increases resilience to node failures — if one node holding a copy is lost, replicas on other nodes ensure data remains available. The tradeoff is increased storage and memory consumption, as each additional copy requires space on another node.

For the MVP, this is startup configuration rather than a live cluster-admin control. Changing the value later requires updating worker configuration and restarting the worker. Existing result artifacts and checkpoints are not retroactively re-replicated or pruned solely because the configured value changed.

This setting applies to the `local` and `memory` storage backends. When using the `external` backend, durability is delegated to the external storage service and the replication factor is not used.

**Acceptance criteria:**
- The worker can be started with a storage replication factor, e.g. `orishu-worker --storage.replicas <number>` (or equivalent configuration), and should replicate each data partition to the specified number of nodes. A value of `1` (the default) means no extra copies — only the primary node stores the data. A value of `2` means one additional copy, and so on.
- All nodes in a cluster should use the same replication factor. Mismatched values across nodes may lead to inconsistent durability guarantees.
- The replication factor applies to both simulation result artifacts and checkpoint data.
- When a node holding a replica becomes permanently unavailable, the cluster should initiate re-replication to restore the configured number of copies.
