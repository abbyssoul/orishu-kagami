# User stories: cluster administration

These stories are written from the administrator persona: the person responsible for running an `orishu` cluster, keeping it safe, understandable, and available for cluster users running scientific workloads.

This document focuses on _day-2_ cluster operations after workers exist: inspecting membership, controlling admission, diagnosing health, and making topology changes safely. Installation and per-worker setup are covered in [Orishu configuration](../../orishu-configuration.md) and [worker-admin.md](./worker-admin.md).

These stories define what administrators should be able to accomplish and what guarantees the system should provide. They are intentionally implementation-agnostic. Concrete UX details such as CLI command names, API shapes, and UI flows are examples unless and until they are specified elsewhere in corresponding documents.

The product exposes these administrative outcomes through both the scriptable
`orishuctl` CLI and the interactive `orishu-monitor` TUI. The completed TUI is
expected to provide story-level read and mutation parity with the CLI while
preserving the same authorization, validation, retry, audit, and outcome
semantics. Raw formation-admission secrets are not rendered in the TUI:
retrieval/export uses the CLI's private-file workflow, though a TUI flow may
consume already prepared material without revealing it.

Administrative mutations are authorized by access tier, not by authorship of prior mutations. In the MVP, any authenticated Tier 2 administrator may inspect, reverse, or supersede a prior administrative change made by another administrator. Cluster state such as membership lock, node membership, and blocklist entries is cluster-scoped, not owned by the administrator who created or last changed it. Audit records preserve who performed each action.

## Shared assumptions

### Current formation-PoC coverage

The [formation task](../../tasks/implement-cluster-formation-poc.md) and
[evidence ledger](../../tasks/cluster-formation-conformance.md) establish the
membership-only portions: real node views, pinned explicit join and catch-up,
replicated lock/unlock, identified leave/replay, and interrupted-admission
recovery or bounded stop. The [CLI manual](../../../apps/orishu-ctl/README.md)
defines the supported commands and transports. These results do not complete
the broader stories below: host/resource telemetry, administrative removal,
durable audit, compute drain, artifact transfer and workload operations remain
outside that PoC. Sample commands and desired fields must not be interpreted
as already-supported behavior. Operational metrics/probes are partially
implemented; secure deployment, traces and remaining monitoring workflows
stay with [P-OBS-DOCS](../../tasks/document-worker-observability.md). Its
[mTLS proxy recipe](../../testing-worker-monitoring-proxy.md) now tests
monitoring-only access, a real source-build Prometheus scrape and
[stalled-reader expiry/recovery](../../testing-worker-monitoring-proxy.md#downstream-response-backpressure-and-expiry)
with continued operator control; it does not
complete fleet, container/service or release deployment acceptance.

Unless a story says otherwise, all stories in this document assume the following:

- The target node or cluster is already running. Initial worker installation and first-process setup are covered in [worker-admin.md](./worker-admin.md).
- The administrator can reach the relevant local or remote client endpoint for the target node or cluster.
- The administrator has whatever authentication tier is required for the specific operation. Stories call out cases where local read-only access may be allowed without stronger credentials.
- Referenced node IDs, cluster names, addresses, or other operator inputs are either already known or discoverable through the documented read-only commands.
- Failures caused purely by missing local tooling, missing credentials on disk, or unrelated host provisioning issues are out of scope unless a story calls them out explicitly.


## Node visibility

### List cluster nodes
As an administrator, I want to see a list of all nodes in the cluster so that I can understand the current cluster composition, capabilities, health, and status.

**Given** a running cluster
**When** I list cluster nodes
**Then** I see a summary of all nodes including their identities, versions, roles, resource availability, and health status.

This is a read-only operation that provides visibility into the cluster state without modifying it. No running workloads or simulations are affected by this operation. Nodes that are offline or unhealthy may be indicated in the output, but they will still appear in the list for visibility.

**Acceptance criteria:**
- A command is available to list nodes, e.g. `orishuctl ls`.
- Output includes node ID, version, host information on which the worker process is running, role (worker/introducer), number of connected peers (if applicable) and clients (if applicable), resources/capabilities summary, and health status.
- Unhealthy or offline nodes are clearly indicated in the output.
- The command is read-only and does not modify cluster state or affect running workloads. Running it multiple times in quick succession does not change output or cluster state unless the underlying cluster state has changed (e.g. a node has gone offline or come online).
- The command can be run at any time without disrupting ongoing simulations or workloads.
- Running the command without prior authentication does not fail, because a locally running instance is targeted by default. If no local instance is available, an appropriate error message is shown. Running without authentication is acceptable for local access, provided the same user is running the worker process and the two can communicate. Other non-local workers may connect to the local worker and be shown in the list, but the command does not require authentication to show the local node and its connected peers.
- The command can also be run with explicit host or endpoint parameters to target a specific cluster or node, and appropriate error messages are shown if connection or authentication fails. A user can specify an _IP address_ or _hostname_ of the node to connect to. If an IP address is provided, the command attempts to connect to that single node directly, assuming the target node accepts client connections. If a _hostname_ is provided, it is resolved to multiple IP addresses. It is assumed that all addresses are part of the same cluster. If connection to _any_ of the resolved addresses succeeds, the command gets a list of nodes in the cluster from that node. Only if connection to _all_ of the resolved addresses fails does the command return an error indicating that connection or authentication to the specified cluster failed.


### Inspect a node
As an administrator, I want to get detailed information about a specific node in the cluster so that I can understand its capabilities, health status, and role in the cluster.

**Given** a running cluster and a target node
**When** I inspect the node
**Then** I see a comprehensive report on the node's identity, version, host information, role, resource availability, health status, and connectivity with peers and clients.

**Acceptance criteria:**
- A command is available to inspect a node in detail, e.g. `orishuctl inspect <node-id>`.
- The output includes detailed information about the specified node, such as its unique identifier, the software version it is running, host information (e.g. IP address, hostname), role in the cluster (worker, introducer, etc.), a summary of available resources (e.g. CPU, memory), health status (e.g. healthy, unhealthy, under load), and connectivity status with respect to its peers and clients (e.g. number of connected peers, number of connected clients, any recent connectivity issues).
- The command can be run at any time without affecting running workloads, as it is a diagnostic operation that does not modify cluster state. However, if the target node is currently participating in workloads, the information provided may indicate that the node is under load or has reduced responsiveness, which can be useful information for troubleshooting.
- Because node manifests are gossiped across the cluster, the same node's manifest can be obtained from different sources with different freshness trade-offs. The command supports a `--source` flag (or similar) to control where the manifest data comes from. Three modes are available:
  - `--source direct`: The manifest is fetched directly from the target node. This returns the most up-to-date information but requires the target node to be reachable and accepting client connections. If the target node is unreachable or does not accept client connections, the command fails with a clear error message indicating the reason.
  - `--source indirect`: The manifest is returned from another node's gossip-based view of the target, without contacting the target node at all. The first client-accepting node that has gossip state for the target responds with its cached version. This is useful for diagnostics when the target node is suspected to be unreachable or overloaded, or when the administrator wants to see how the rest of the cluster perceives the node. The output clearly indicates that the manifest was obtained from gossip and includes a staleness indicator (e.g. last-seen timestamp or gossip round) so the administrator can judge the freshness of the data.
  - `--source best-effort` (default when `--source` is omitted): The system first attempts to fetch the manifest directly from the target node. If the target is unreachable or does not respond within a reasonable timeout, the system falls back to returning a cached gossip-based view from another node, if one is available. If neither a direct nor cached manifest can be obtained, the command fails with an error. The output always indicates which source was actually used (direct or gossip-cached) so the administrator knows the freshness of the returned data.
- Regardless of the source mode used, the response always indicates the manifest origin (direct from the target node vs. gossip-cached from a peer) so the administrator can assess the reliability and freshness of the information.


### Correlate cluster metrics and traces

The [rootless container recipe](../../testing-worker-container.md) verifies
that ordinary containers have separate loopbacks, and only an explicitly
network-sharing scraper can reach a worker's local metrics. Sharing networking
does not require mounting worker credentials. This local trust boundary does
not replace remote mTLS authorization or prove formation convergence.

The [user-service monitoring recipe](../../testing-worker-user-service.md)
distinguishes service-manager state, process health and formation identity.
Its explicit restart creates a fresh standalone formation with retained
credentials, not restored membership. Per-worker service checks do not prove
three-worker convergence or cross-peer trace/log correlation.

The [formation dashboard example](../../testing-worker-dashboard.md) supplies
per-target metric and trace-delivery views with explicit source selection and
graph links. It does not itself provide cross-peer/log correlation,
infer peer identity from a scrape label or prove convergence from availability.

The [official Collector formation walkthrough](../../testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
separately proves both admission trace chains against per-worker stdout records,
exact membership identities and cross-worker lock/unlock visibility. Healthy
direct scrapes and probes accompany the journey. A separate
[Prometheus logging-counter check](../../testing-worker-prometheus.md#ingest-logging-counters-through-prometheus)
now establishes backend ingestion and fresh write/failure samples after scraper
outage. Separate [systemd and rootless-container collection evidence](../../tasks/cluster-formation-conformance.md#enabled-systemd-and-rootless-container-log-collection--2026-09-09)
now verifies enabled stdout records through those runtimes. Supervisor-specific
three-worker formation and reviewed overhead are not implied by combining these
scoped results.

The [formation warning examples](../../testing-worker-prometheus.md#optional-formation-warnings)
distinguish recent admission refusals, receiver failures, exhausted budgets
and transport timeouts. They do not identify a failed peer or prove command
outcomes; an informational refusal can be correct lock enforcement. Query
the original operation and the runbook's exact worker/formation identities.

The [monitoring incident runbook](../../worker-monitoring-runbook.md) now links
peer-loss signals to exact formation/source/node/certificate inspection and
the existing admission-recovery procedure. The three-worker companion checks
healthy survivor probes at suspected/dead peer checkpoints; readiness and
aggregate counters still do not establish convergence or permit readmission.

**Current evidence:** the [local Collector walkthrough](../../testing-worker-otelcol.md)
receives worker client-service spans and checks authenticated control through
collector shutdown/recovery. It does not establish this story's cross-peer
correlation or fleet dashboard outcome; those remain gated on the documented
propagation and operator-handoff work.
The [Collector mTLS checks](../../testing-worker-otelcol-mtls.md) also verify
receiver access and worker server-identity checks using collector-only keys;
they do not bind submitted telemetry to authoritative membership identities.
Inbound peer metrics additionally distinguish TLS/application handshake
outcomes and local capacity refusals. These are per-process signals, not
per-peer attribution, membership convergence or admission acceptance; consult
the [catalogue](../../orishu-observability.md#inbound-peer-handshake-counters)
and public operation status before taking recovery action.
The [inbound capacity gauges](../../orishu-observability.md#inbound-connection-capacity-gauges)
distinguish current adapter pressure from historical refusal counts. Connection
tasks include unfinished handshakes and uncollected completions; do not use
their occupancy as a membership or cluster-size measurement.
The [registry gauges](../../orishu-observability.md#registered-session-capacity-gauges)
separately count retained sessions and their provisional subset, not admitted
members or live sockets. Outgoing introducer bindings share the provisional
budget. Compare these with authenticated operation status and transport
pressure; neither high occupancy nor a withdrawn owner projection authorizes
automatic restart, readmission or membership changes.
The [catch-up counters](../../orishu-observability.md#admission-state-catch-up-outcomes)
separate a joiner's validated transfer from safe owner adoption. Late results
can be fenced or abandoned; a source refusal is not a new admission refusal.
Compare authenticated join-operation status before acting, not page counts,
aggregate readiness or a successful transport response. Counters retain totals
across formation changes but are not durable admission history.
An observed `catchUpFailed` can be followed by the worker's existing bounded
automatic retry. Continue inspecting the same operation and assigned identity
within the chosen observation deadline; do not report successful readiness,
extend the deadline on each phase change, or issue a new admission ID.
Reliable-exchange metrics additionally expose aggregate request/serve outcomes,
duration, partial stream bytes and pool pressure. Use their
[accounting boundaries](../../orishu-observability.md#reliable-peer-exchange-metrics)
to distinguish local transport work from accepted admissions; they supply no
per-peer attribution or end-to-end convergence timing.
The [traffic catalogue](../../orishu-observability.md#datagram-and-pre-pool-traffic-counters)
also exposes local datagram submission outcomes/payload bytes and pre-pool
refusal. Independent send/receive totals are not a network-loss measurement;
decoded SWIM activity and public membership/operation status remain distinct.
The [outbound dial catalogue](../../orishu-observability.md#outbound-dial-and-tls-metrics)
adds nested attempt/TLS outcomes and duration with dial-slot pressure. It helps
distinguish candidate fallback from whole-attempt exhaustion; neither a
completed TLS handshake nor a returned reply establishes admission or recovery.
The [membership deadline catalogue](../../orishu-observability.md#membership-deadline-and-abandonment-counters)
adds local consumed timers and join/reconciliation abandonment. A learned
liveness change need not consume a local suspicion timer, and a round can
exhaust its continuation budget without a timeout. Use actual membership and
operation status for recovery decisions; these counts are not cluster authority.

Status: **planned — ADR 0017**

As an administrator, I want per-worker dashboards and sampled traces across
client and peer operations so that I can locate admission, connectivity,
coordination or storage problems without treating one entry node's view as
cluster truth.

**Given** explicitly enabled monitoring and trace export on participating workers
**When** I investigate a slow or failed operation
**Then** I can correlate its bounded diagnostic spans and aggregate metrics,
identify missing telemetry, and consult authoritative runtime/artifact state.

**Acceptance criteria:**

- Formation monitoring ships with the three-worker PoC; runtime, transfer,
  storage and observation instruments ship with their owning services.
- Dashboards/alerts distinguish unavailable scrapes from zero errors, local
  process readiness from cluster progress, and retrieval from durability.
- Sampled traces correlate authenticated client/peer operations without
  exposing credentials or requiring workload/peer IDs as metric labels.
- Monitoring credentials grant no membership/workload mutation authority;
  probes never replace SWIM, and missing traces are not proof an action failed.
- Tested runbooks explain peer loss, queue pressure, slow steps and unavailable
  artifacts; recovery decisions consult the relevant authority rather than
  automatically removing or resetting nodes based on one diagnostic signal.

Delivery: [observability guide](../../orishu-observability.md) and
[operator documentation task](../../tasks/document-worker-observability.md).

### Diagnose node connectivity and health
As an administrator, I want to diagnose the connectivity and health of nodes in the cluster so that I can identify and troubleshoot issues with cluster members.

**Given** a running cluster and a target node
**When** I diagnose the node
**Then** I see a report on the node's connectivity status, its health, and any issues affecting its participation in the cluster.

**Acceptance criteria:**
- A command is available to perform a direct connectivity and health check from the administrator's tool to the target node, e.g. `orishuctl diagnose <node-id>`.
- A command is available to perform an **indirect** connectivity check via another node, e.g. `orishuctl diagnose <node-id> --from <source-node-id>`. In this mode, the administrator asks one node (`source-node-id`) to verify its connectivity to another node (`node-id`), which is critical for identifying network partitions or asymmetrical routing issues.
- The output includes information on whether the node is healthy, responsive to clients, and able to communicate with its peers.
- This is a diagnostic operation that does not modify cluster state or disrupt ongoing simulations. If the target node is under heavy load, the report reflects this state to assist in troubleshooting.
- The result is consistent regardless of whether the check was direct or indirect; if the target is unreachable, it is clearly marked.


Local [structured stdout logs](../../../apps/orishu-worker/README.md#structured-stdout-logs)
can now be matched to actual sampled OTLP trace/span IDs. Logging and tracing
are independently enabled; absence of a log is not proof that an operation did
not occur. Operators use authoritative status/receipts for decisions and inspect
bounded logging-loss counters to distinguish output pressure from cluster faults.
This does not add an `orishuctl logs` query, retained history, audit authority or
automatic worker restart. The selected three-worker/deployment monitoring
walkthrough and reviewed overhead remain part of the open M4 handoff.

## Membership and topology

### Inspect an uncertain admission outcome

As an administrator, I want to correlate an unresolved join with its original
introducer's retained evidence so that I do not accidentally duplicate an
admission or bypass a restriction while troubleshooting.

Acceptance criteria:

- Source operation status identifies the original target and secret-free
  attempt/issuer reference independently of reusable worker labels.
- An authenticated read on the original issuer distinguishes a retained current
  assignment, a retired/restricted assignment, missing history and wrong issuer.
- The output identifies its source and remains a local-at-request report, not
  a cluster-wide fence or permission to admit a worker.
- Missing history, restart, unreachable issuer and failed authentication never
  become evidence that insertion did not happen. The operator has a documented
  stop condition and does not automatically remove restrictions or switch issuers.
- Inspection never changes membership, retries, credentials or ledger retention.

PoC status: source correlation and issuer inspection are implemented, and the
[bounded recovery procedure](../../cluster-admission-recovery.md) is documented.
Independent-process tests cover recovered ACK loss and mandatory stop after
issuer loss with real retry exhaustion, followed by source crash/restart with
retained credentials but missing operation history and fresh identities.
The separate source-loss journey verifies the same missing-history stop
condition while the original issuer survives and retains the accepted ID.
No fresh join is submitted as recovery. A separate Dead-assignment journey
preserves the original source and verifies bounded exhaustion without revival
or duplicate membership after SWIM retirement. The removed-assignment journey
also verifies bounded stop after a post-insertion force removal, without
readmitting the old identity or allocating a replacement. A certificate-block
journey keeps the restricted member record and verifies the same bounded stop
without replacement identity. A negative restart test verifies that retaining
the excluded certificate still blocks a fresh attempt despite fresh process
identities, while another certificate can join. It does not authorize restart
or new admission as recovery. A post-adoption wire-ejection journey verifies
retained diagnostic identities, stopped participation, historical join status
and explicit leave to standalone; it is not a public removal API or proof of
cluster-wide removal convergence. The remaining formation fault acceptance
stay in progress. See the
[CLI manual](../../../apps/orishu-ctl/README.md).

The lost-departure process journey also suppresses both voluntary leave
announcements, verifies survivor SWIM detection and readmission with the same
certificate/fresh assigned ID, and retains dead history. A separate public
leave-response-loss journey discards the accepted response through a local
proxy and verifies exact CLI retry recovers the original receipt without
another identity change, including historical replay after readmission. See the
[formation task](../../tasks/implement-cluster-formation-poc.md).

### Retrieve the current join token

As an administrator, I want to retrieve the cluster's current join token so that I can admit a new worker without unnecessarily rotating cluster admission secrets.

**Given** a running cluster
**When** I request the current join token
**Then** I receive the current token value without changing cluster admission state.

**Acceptance criteria:**
- A command is available to retrieve the current join token, e.g. `orishuctl token`.
- By default, successful command output prints the current token value only, without extra prose or generated join instructions, so it is easy to copy or pipe into other tooling.
- Retrieving the current join token is a privileged operation requiring authentication, even though it does not mutate cluster state.
- Retrieving the current join token does not rotate, invalidate, or otherwise change the token.
- Retrieving the current join token does not disrupt running workloads, does not change cluster membership, and does not affect already joined nodes.
- The current join token can still be retrieved while cluster membership is locked. This does not bypass the membership lock: new join attempts continue to fail until the cluster is unlocked.
- Retrieving the current join token does not itself create an administrative audit event.


### Bootstrap a cluster with a pre-populated blocklist

As an administrator, I want to form a new cluster with a pre-populated blocklist from a saved or operator-authored source so that known-bad nodes and network origins are excluded before any later join attempts are evaluated.

**Given** a standalone worker that is about to bootstrap a new cluster and a blocklist source prepared by the administrator
**When** I start cluster formation and provide that blocklist source
**Then** the new cluster is created with those blocklist entries already active, and subsequent join attempts are evaluated against them from the first admission decision onward.

**Acceptance criteria:**
- The administrator can provide a blocklist source when bootstrapping a new cluster, for example by referencing a saved file or other explicit operator-managed input.
- The supplied source may contain entries the administrator previously saved from prior operational use and entries the administrator authored manually. After successful bootstrap, both are treated as normal cluster blocklist entries.
- If the provided blocklist source cannot be read, parsed, or validated, cluster formation fails with a clear error rather than silently starting with an empty blocklist.
- A cluster formed with a supplied blocklist applies those entries before admitting any later joining workers. A worker that matches one of those entries is rejected through the normal blocked-admission path.
- Bootstrap-loaded entries are visible through the normal blocklist inspection workflow after cluster formation and can later be added to or removed through the normal blocklist management workflow.
- If the supplied blocklist would match the founding worker's own identity or source network, cluster formation is rejected with a clear explanation rather than creating a cluster whose initial member contradicts its own admission policy.
- Supplying an initial blocklist does not weaken or bypass other admission controls. Join tokens, membership lock, tombstones, and normal authentication rules still apply.
- If no blocklist source is supplied, cluster formation behaves as normal and starts with the default empty cluster blocklist.

### Lock cluster membership
As an administrator, I want to lock the entire cluster membership so that I can prevent any changes to the cluster composition during critical operations or maintenance windows.

**Given** a running cluster
**When** I lock cluster membership
**Then** the cluster membership is locked and no new nodes can join, while existing nodes can still disconnect if worker processes stop, terminate, or crash.

**Acceptance criteria:**
- A command is available to lock cluster membership, e.g. `orishuctl cluster lock`. The command prevents any changes to the cluster composition, meaning that new nodes cannot join the cluster and existing nodes cannot be removed through administrator commands. However, if a node's worker process is stopped, terminated, or crashes, it may disconnect from the cluster.
- The membership lock is cluster-scoped administrative state, not a lock owned by the administrator who created it.
- The output confirms that the cluster membership is now locked and indicates that no changes to the cluster composition are allowed until it is unlocked.
- The command can be run at any time, and it does not disrupt ongoing simulations.
- While the cluster membership is locked, any attempts to add or remove nodes through administrator commands should fail with an appropriate error message indicating that the cluster membership is currently locked and changes are not allowed.
- Membership lock applies to administrator-initiated membership edits. It does not prevent nodes from disappearing due to crash, shutdown, or network partition.
- If an administrator adds a blocklist entry (whether by identity, network, or both) while membership is locked, an already-connected matching node is not forcibly removed solely because of the new blocklist entry. Instead, the blocklist prevents future admission if that node later disconnects or is removed for another reason.
- The cluster should continue to operate and converge as long as there are remaining healthy nodes, even if the membership is locked. The lock only prevents changes to the cluster composition, but does not affect the ongoing operations of the cluster.
- Locking an already locked cluster is a no-op and does not cause errors.


### Unlock cluster membership
As an administrator, I want to unlock the entire cluster membership so that I can allow changes to the cluster composition after critical operations or maintenance windows.

**Given** a locked cluster
**When** I unlock cluster membership
**Then** the cluster membership is unlocked and new nodes can join, and existing nodes can be removed through administrator commands.

**Acceptance criteria:**
- A command is available to unlock cluster membership, e.g. `orishuctl cluster unlock`. The command allows changes to the cluster composition, meaning that new nodes can join the cluster and existing nodes can be removed through administrator commands.
- In the MVP, any authenticated Tier 2 administrator may do so, even if a different administrator originally locked it. This is a safety feature to prevent administrative lock-out.
- The output confirms that the cluster membership is now unlocked and indicates that changes to the cluster composition are allowed.
- The command can be run at any time, and it does not disrupt ongoing simulations.
- While the cluster membership is unlocked, any attempts to add or remove nodes through administrator commands should succeed as normal.
- The cluster should continue to operate and converge as long as there are remaining healthy nodes, even if the membership is unlocked. The unlock only allows changes to the cluster composition, but does not affect the ongoing operations of the cluster.
- Unlocking an unlocked cluster is a no-op and does not cause errors.


## Cluster visibility

### Inspect cluster status
As an administrator, I want to see a cluster-level summary so that I can quickly verify cluster health, confirm whether a node is part of a cluster, and understand the current state of any loaded workload.

**Given** a running cluster or a standalone node
**When** I inspect cluster status
**Then** I see a summary of the cluster including its name, node count, membership lock state, and workload status.

This is a quick "health check" operation that answers the most common administrator questions at a glance: how many nodes are in the cluster, whether membership is locked, and whether a simulation is loaded or running. For detailed per-node information, see `orishuctl ls` and `orishuctl inspect <node-id>`.

**Acceptance criteria:**
- A command is available to inspect cluster status, e.g. `orishuctl cluster info`.
- Output includes the cluster name, total node count, membership lock state (locked or unlocked), and current workload phase (not loaded, loading, ready, running, stopped, error).
- If a workload is loaded, the output includes the workload name and metadata.
- If no workload is loaded, the output makes that explicit and omits workload-specific fields or marks them as unset rather than implying stale workload state.
- If the target node is not part of any cluster, the output reflects that it is a standalone node, i.e. a cluster of one.
- The command is read-only and does not modify cluster state or affect running workloads.
- Running without authentication is permitted for local access (Tier 1). Remote access follows standard authentication requirements.


### Remove a node from the cluster

As an administrator, I want to remove a node from the cluster so that I can re-adjust topology, perform maintenance, or remove unhealthy nodes.

**Given** a running cluster and a target node
**When** I remove the node from the cluster
**Then** the specified node is gracefully removed from the cluster membership and no longer participates in simulations or workloads.

**Acceptance criteria:**
- A command is available to remove a node, e.g. `orishuctl rm <node-id> [--force]`.
- In the MVP, any authenticated Tier 2 administrator may remove a node regardless of which administrator originally admitted or previously managed that node.
- **Default (graceful):** The command initiates a graceful removal process where the target node is notified and given an opportunity to finish in-flight computation, transfer result data to replicas for durability, and then exit cleanly. If the node is unresponsive within a timeout, it is marked as removed. This is the safe default that ensures no data is lost and the simulation can continue without disruption.
- **With `--force`:** The node is immediately dropped from the cluster membership. Other nodes are informed not to accept any pending results or state transfers from this node, preventing potentially corrupted or incomplete data from poisoning the simulation. Use this when a node is suspected of producing incorrect results, is stuck, or must be removed urgently.
- The removed node no longer appears in active cluster membership lists and is not scheduled for new workloads. It may still be visible in tombstone-specific views and via explicit removed-state filters for diagnostics and auditability. In the graceful case, existing workload partitions on the removed node are transferred or rescheduled before the node exits. In the force case, the cluster treats the node's partitions as lost and reassigns them to remaining nodes.
- The command provides feedback on the removal process, including success confirmation or error messages if the specified node ID does not exist or if the removal process encounters issues.
- The command can be run at any time, and will not affect running workloads if the removed node was actively participating in them. Although it can impact the overall speed of the computation if the removed node was contributing significant resources, it will not cause failures or disruptions to ongoing simulations. The cluster should continue to operate and converge as long as there are remaining healthy nodes.
- Running the command without prior authentication should fail with an appropriate error message indicating that authentication is required to perform node removal, as this is a privileged operation that modifies cluster state. The command should not allow unauthenticated users to remove nodes from the cluster, as this could lead to unauthorized disruptions.
- After running the command, the same worker cannot automatically rejoin the cluster unless an administrator explicitly clears its tombstone and the worker later goes through the normal registration process again. This ensures that removed nodes do not accidentally rejoin the cluster without administrator intent.
- If the removed node was an introducer, the cluster should continue to operate and converge as long as there are remaining healthy nodes. The removal of an introducer may impact the ability of new nodes to join the cluster until a new introducer is added, but it should not cause failures or disruptions to ongoing simulations. The cluster should be able to continue operating with the remaining nodes, and a new introducer can be designated if needed to allow new nodes to join in the future.


### List tombstoned nodes

As an administrator, I want to list retained tombstones for removed nodes so that I can understand which workers remain explicitly excluded, why they were removed, and whether any of those tombstones should be cleared.

**Given** a running cluster
**When** I list cluster tombstones
**Then** I see the current set of removed-node tombstones along with enough metadata to understand who created them and why they still matter.

**Acceptance criteria:**
- A command is available to list tombstones, e.g. `orishuctl tombstones ls`.
- The output includes the tombstoned node ID, last known node name, certificate fingerprint, tombstone creation time, actor that created it, removal mode (`graceful`, `force`, `dead`, or `blocklist`), and any recorded reason.
- The command is read-only and does not change cluster state or affect running workloads.
- Listing tombstones requires authentication, because the output exposes privileged cluster-removal history and durable node identity material.
- If no tombstones exist, the command returns an empty result with a clear indication that the cluster currently has no retained tombstones.
- This tombstone listing is the canonical administrator view of retained removals. Existing node-listing commands may still expose removed nodes through explicit removed-state filters as a convenience shortcut, but tombstone-specific metadata is available through the tombstone listing.


### Command a node to leave its cluster

As an administrator, I want to command a worker node to voluntarily leave its current cluster and return to a standalone state, so that I can detach it without restarting the process — for example, to idle it before reassignment, to prepare for maintenance, or to undo an incorrect join.

**Given** a node that currently belongs to a cluster
**When** I instruct the node to leave
**Then** the node gracefully leaves its cluster (draining in-flight work, transferring data to replicas, announcing `Leave` to peers), drops its cluster-assigned ID, and becomes a standalone node, i.e. a cluster of one.

This is the mirror of the `join` command: `join` transitions a standalone node into a cluster member, while `leave` transitions it back. The command is sent directly to the node (e.g. `orishuctl leave` targeting the local or remote worker), and it is the node itself that initiates the departure — not the cluster evicting it.

This story is distinct from related operations:
- **Remove** (`orishuctl rm <node-id>`) is a cluster-side eviction: any administrator issues it against the cluster, the node is tombstoned, and it cannot rejoin until an administrator explicitly clears that tombstone. Leave is node-initiated, cooperative, and does not create a tombstone.
- **Move** (`orishuctl join <new-cluster>` on a node already in a cluster) combines leave and join into a single operation. Leave is the standalone-node primitive — detach with no destination.
- **Start with work admission disabled** is a worker-startup choice covered in [worker-admin.md](./worker-admin.md). Leave removes the node from the cluster entirely.

**Acceptance criteria:**
- A command is available to command a node to leave its cluster, e.g. `orishuctl leave`, targeting a specific worker node (local by default, or remote via `--host`).
- The node gracefully leaves its cluster: it finishes in-flight computation, transfers result data to replicas for durability, announces `Leave` to peers, and drops its cluster-assigned ID (see [Node identity](../../orishu-runtime-design.md#node-identity)).
- After leaving, the node becomes a standalone node, i.e. a cluster of one. It retains its name and certificate identity but has no cluster-assigned ID.
- The node disappears from `orishuctl ls` on the cluster it left. The cluster reassigns the node's workload partitions to remaining members, as with any graceful departure.
- No tombstone is created for the node in the cluster's membership CRDT. The node can rejoin the same cluster via a normal `join` command without first clearing a tombstone. This distinguishes voluntary leave from administrative removal.
- If the node is not currently a member of any cluster and is already a standalone node, the command is a no-op and does not cause errors.
- The command can be run at any time. If the node is actively running a workload, it drains work before departing — existing workloads on the cluster are not disrupted.
- The command does not require cluster-level Tier 2 authentication (unlike `rm`), because it is the node itself choosing to depart. However, it requires that the caller has access to the target node's client API (local Tier 1 access or remote Tier 2 access to that specific node).


### Add a node to a cluster explicitly

As an administrator, I want to explicitly command a worker node to join an existing cluster using a join token, so that I have full control over which nodes join which cluster.

**Given** a running cluster and a worker that is still a standalone node
**When** I issue a join command with a valid introducer address and join token
**Then** the worker presents the token and its mTLS certificate to the cluster, is admitted, adopts the cluster's name, and becomes available for workloads.

By default, a freshly started `orishu-worker` does not attempt to join any cluster automatically. It starts as a standalone node, i.e. a cluster of one, and waits. To add it to an existing cluster, the administrator must:

1. Retrieve the current join token from the target cluster (e.g. via `orishuctl token`).
2. If the previously shared token is suspected to be exposed or should otherwise be replaced, rotate it first (e.g. `orishuctl token --rotate`) and use the newly returned value for future joins.
3. Command the worker to join, providing the token and the address of one or more introducer nodes (e.g. via `orishuctl join <address> --token <token>` targeting the worker, or by starting the worker with `--join <address> --token <token>`).

The worker then contacts the specified address, performs mTLS handshake, and presents the join token in its `JoinReq`. If the introducer accepts the request, the worker joins the cluster and adopts its cluster name. If the request is rejected (invalid token, tombstoned, blocklisted, membership locked, no capacity), the worker reports the failure to the administrator.

This is the recommended flow for small clusters, deliberate topology experiments, and environments where administrators want explicit control over cluster composition.

Automatic discovery, auto-join, and mTLS-only admission are intentionally out of MVP scope. A worker that can see introducers on the network still remains a standalone node until an administrator explicitly provides introducer address information and a join token. Post-MVP mDNS-based auto-join is preserved in [Orishu runtime deferred design work](../../orishu-runtime-future-work.md#discovery-assisted-admission).

**Acceptance criteria:**
- The administrator can obtain a join token from the cluster and use it to command a worker to join at a specific introducer address.
- Obtaining the current join token does not itself admit any worker or otherwise change cluster membership.
- Joining a node to a cluster explicitly is a privileged operation requiring authentication.
- The worker performs mTLS verification and presents the join token during the join handshake.
- A freshly started worker does not discover or join a cluster on its own in the MVP. Reachable introducers, mDNS visibility, or existing worker identity material alone are not sufficient to admit it.
- On success, the worker joins the cluster and adopts the cluster name. Running `orishuctl inspect <node-id>` on the newly joined node shows the target cluster's human-readable cluster name and membership details, confirming it joined the intended cluster.
- On failure (invalid token, tombstoned, certificate mismatch, membership locked, blocklisted, no capacity), the system provides a clear error message explaining why the join was rejected.
- If the token is rotated after an administrator retrieved it but before the worker finishes joining, the join using the older token is rejected with a clear error and the worker remains standalone.
- If the command fails before admission succeeds, the worker remains a standalone node, i.e. a cluster of one.
- The newly joined node appears in `orishuctl ls` output on any node in the cluster.
- Existing simulations can be scheduled on the new node if a workload is currently running, and the new node can take on workloads unless it was started with work admission disabled.
- Node health is monitored alongside other cluster members.
- If cluster membership is locked, the join is rejected until membership is unlocked.


### Move a node between clusters

As an administrator, I want to command a worker node that is already a member of one cluster to leave it and join a different cluster, so that I can rebalance compute resources across clusters or correct an incorrect cluster assignment without restarting the worker process.

**Given** a node in cluster A and a reachable cluster B
**When** I issue a join command for cluster B
**Then** the node gracefully leaves cluster A (draining in-flight work, transferring data), drops its cluster A identity, and joins cluster B through the standard admission flow, receiving a new cluster-assigned ID from cluster B.

In larger deployments, administrators routinely rebalance compute capacity across clusters — for example, moving idle nodes from a completed simulation's cluster to one that is under-provisioned, or correcting a node that was accidentally joined to the wrong cluster. This operation reuses the same join mechanism described in "Add a node to a cluster explicitly", with the additional step that the node must first leave its current cluster. A node can be a member of at most one cluster at a time (see [Node identity](../../orishu-runtime-design.md#node-identity)).

**Acceptance criteria:**
- The administrator can command a node that is already in a cluster to join a different cluster using `orishuctl join <address> --token <token>` (or similar) targeting the node. The command is the same as for a standalone node; the system detects that the node is already in a cluster and initiates a leave-then-join sequence.
- Before joining the new cluster, the node gracefully leaves its current cluster: it finishes in-flight computation, transfers result data to replicas for durability, announces `Leave` to peers, and drops its current cluster-assigned ID.
- After leaving, the node follows the standard admission flow for the target cluster — mTLS handshake, join token presentation, and admission gate evaluation. On success, the node receives a new cluster-assigned ID from the target cluster and adopts its cluster name.
- If the join to the new cluster fails (invalid token, tombstoned, membership locked, blocklisted, no capacity), the node reports the failure to the administrator. The node is no longer a member of the old cluster at this point — it becomes a standalone node, i.e. a cluster of one, until the administrator resolves the issue or commands it to join again.
- The node disappears from `orishuctl ls` on the old cluster and appears in `orishuctl ls` on the new cluster.
- Running workloads on the old cluster are not disrupted — the node's partitions are reassigned to remaining nodes, as with any graceful removal.
- The node can take on workloads in the new cluster immediately unless it was started with work admission disabled, and its health is monitored alongside other members.
- The command requires authentication, as it modifies cluster state on both the source and target clusters.
- Re-running the command when the node is already a member of the target cluster is a no-op and does not cause errors.


### Re-admit a removed or blocklisted node

As an administrator, I want to explicitly re-admit a node that was previously removed or blocklisted so that I can return a trusted worker to service without weakening the intent of removal or blocklisting.

**Given** a node identity that was previously removed from the cluster or placed on the blocklist
**When** I re-admit the node through the normal admission flow
**Then** the node can participate again only after the prior exclusion is reversed and admission succeeds.

**Acceptance criteria:**
- A previously removed or blocklisted node is not automatically re-admitted merely because it becomes reachable again on the network.
- If the node is tombstoned because it was previously removed, the administrator must explicitly clear that tombstone (e.g. `orishuctl tombstones rm <node-id>`) before the worker becomes eligible for a later normal join.
- Clearing a tombstone is a privileged cluster-scoped mutation requiring administrative authentication.
- If the node identity is present on the cluster blocklist, the administrator must remove all matching blocklist entries before a later join can succeed.
- Clearing tombstones and removing blocklist entries are independent preparatory actions and may be performed in either order. Neither operation causes the node to join automatically.
- Clearing a tombstone does not require cluster membership to be unlocked. However, the later join attempt is still blocked while cluster membership remains locked.
- After the relevant prior exclusions are reversed, the node follows the same explicit token-based admission flow as any other joining worker.
- The system provides clear feedback explaining why admission is denied at each stage (e.g. still tombstoned, still blocklisted, membership currently locked, authentication failure, certificate mismatch).


### Manage the cluster blocklist
As an administrator, I want to manage a cluster-wide blocklist so that I can prevent specific node identities, network origins, or combinations of both from joining the cluster.

**Given** a running cluster
**When** I view, add, or remove blocklist entries
**Then** the blocklist is updated across the cluster, and blocked nodes are prevented from joining or are disconnected if already present.

Blocklist entries operate on two independent dimensions: **identity** and **network**. An entry may specify one or both. When both are specified, a join request must match on both dimensions for the entry to apply. This gives administrators fine-grained control — for example, blocking a specific node spec name only from a particular subnet, or blocking all traffic from a CIDR range regardless of identity.

**Identity dimension** (at most one per entry):
- **Node ID** (`--id <uuid>`): blocks a specific cluster-assigned instance. Transient — only effective while the node holds that ID.
- **Node name** (`--name <name>`): blocks all current and future instances with this name. Since names are shared across instances from the same manifest, this blocks a class of workers.
- **Certificate fingerprint** (`--cert <fingerprint>`): blocks a specific physical node by its mTLS certificate. Durable — persists across cluster leave/rejoin and name changes. This is the strongest form of identity exclusion.

**Network dimension** (at most one per entry):
- **Host IP** (`--host <ip>`): blocks a specific source IP address.
- **CIDR range** (`--cidr <range>`): blocks a network segment.

At least one dimension must be specified per entry (double-wildcard entries are rejected).

**CLI examples:**

```
# List all blocklist entries
orishuctl blocklist ls

# Block a single node by its cluster-assigned ID (transient)
orishuctl blocklist add --id 550e8400-e29b-41d4-a716-446655440000

# Block all instances named "worker-gpu" from any network
orishuctl blocklist add --name worker-gpu

# Block a specific physical node by certificate fingerprint (durable)
orishuctl blocklist add --cert ab12cd34ef56...

# Block an entire subnet
orishuctl blocklist add --cidr 10.0.5.0/24

# Block a specific host IP
orishuctl blocklist add --host 192.168.1.50

# Compound entry: block "worker-gpu" instances only from a specific subnet
orishuctl blocklist add --name worker-gpu --cidr 10.0.5.0/24

# Remove an entry by its entry ID (shown in blocklist ls output)
orishuctl blocklist rm <entry-id>
```

**Acceptance criteria:**
- A command is available to list blocklist entries, e.g. `orishuctl blocklist ls`. The output shows each entry's identity matcher (if any), network matcher (if any), creation timestamp, and the administrator who added it. Each entry has a unique entry ID for use with the `rm` command.
- A command is available to add a blocklist entry, e.g. `orishuctl blocklist add <flags>`. Exactly one identity flag (`--id`, `--name`, or `--cert`) and/or exactly one network flag (`--host` or `--cidr`) must be provided. Identity flags are mutually exclusive with each other; network flags are mutually exclusive with each other. At least one flag overall is required.
- If the new entry matches an active node and cluster membership is not locked, that node is immediately disconnected and removed from the cluster membership.
- If the new entry matches an active node and cluster membership is locked, the node remains in the cluster until it leaves for some other reason, but it must not be allowed to re-join while the blocklist entry remains in place.
- A command is available to remove a blocklist entry, e.g. `orishuctl blocklist rm <entry-id>`, allowing previously blocked targets to attempt to join the cluster again.
- The blocklist is treated as a persistent, cluster-wide resource that is always present, even if empty.
- Operations are idempotent: adding an entry with identical matchers to an existing entry, or removing a non-existent entry ID, is handled gracefully without errors.
- An entry that was previously removed may later be added again; blocklist membership and blocklist history are treated as separate concerns.
- Removing an entry from the blocklist does not itself clear any tombstone or re-admit any node. Nodes must still satisfy all other admission gates and go through the normal admission flow to join the cluster again.
- Name-based blocking affects all instances sharing that name, including instances that join after the entry is created. This is intentional — it blocks a class of workers, not a specific instance.
- Certificate-fingerprint blocking is the recommended approach for permanently excluding a specific physical node, because it survives cluster leave/rejoin and is immune to name changes.
- Modifying the blocklist is a privileged operation requiring administrative authentication.


## Operations and governance

### Monitor cluster events and logs

As an administrator, I want to monitor cluster events and logs so that I can stay informed about the cluster's operation, identify issues, and understand the history of actions taken.

**Given** a running cluster
**When** I view recent events and logs
**Then** I see a chronological list of significant cluster events, including node joins and leaves, workload starts and stops, and administrative actions taken by administrators.


**Acceptance criteria:**
- A command is available to fetch and display recent logs, e.g. `orishuctl logs`. Logs include system messages, errors, warnings, and informational messages from the cluster components. They are displayed in chronological order and may be filtered by time range, log level, or component.
- A command is available to display recent cluster events, e.g. `orishuctl events`. The output includes node join/leave events, workload start/stop events, and administrative actions taken by administrators (e.g. locking membership, removing nodes, clearing tombstones, editing the blocklist). Each event entry includes a timestamp, event type, and relevant details (e.g. node ID for join/leave events, workload ID for start/stop events, administrator username for administrative actions).
- If no logs or events match the requested filters or time window, the command returns an empty result with a clear indication that nothing matched, rather than treating it as an error.
- The commands can be run at any time without disrupting ongoing simulations, as they are read-only operations that provide visibility into the cluster's operation and history. However, if the cluster is currently under heavy load or experiencing issues, the logs and events may indicate warnings or errors that can help the administrator identify and troubleshoot problems.
- Running the commands without prior authentication may be permitted for local access, but may require authentication when accessing logs or events from remote nodes or when the cluster is configured to restrict access to this information. This allows administrators to monitor the cluster while maintaining security controls as needed.


### Operate safely with multiple administrators
As an administrator, I want administrative actions to remain recoverable and understandable even when multiple administrators manage the same cluster, so that the cluster does not become stuck or unsafe when one administrator is unavailable or working from stale information.

**Given** a cluster that may be administered by multiple authenticated administrators
**When** different administrators issue administrative commands against the same cluster resources over time
**Then** the system authorizes actions by access tier rather than by authorship, preserves an audit trail of who changed what, and detects stale write attempts when the command depends on outdated cluster state.

**Acceptance criteria:**
- A membership lock created by one authenticated administrator can be removed by another authenticated Tier 2 administrator.
- A node admitted or previously managed by one authenticated administrator can be removed, have its tombstone cleared, or otherwise prepared for later re-join by another authenticated Tier 2 administrator.
- A join token previously retrieved or rotated by one authenticated administrator can be rotated by another authenticated Tier 2 administrator, and only the current token remains valid for future joins.
- Administrative resources are cluster-scoped, not owned by the administrator who last changed them.
- If an administrator issues a state-changing command based on stale cluster state and the command includes a concurrency precondition or resource version, the system rejects the request with a clear precondition/conflict error instead of silently applying an unintended overwrite.
- If an administrator issues a state-changing command without such a precondition, the system may still apply the most recent authorized mutation according to normal command ordering rules, but the audit trail must show who performed the resulting change.
- Audit and event output make it possible to reconstruct which administrator performed each action and in what approximate order, even though authorship does not grant exclusive authority over later changes.


### Optimistic concurrency (opt-in)
As an administrator, I want to opt in to a concurrency check on a write so that my command is rejected rather than silently overwriting a change another administrator made since I last read the resource.

**Given** a versioned, operator-controlled resource (membership lock, workload spec, or workload run-state)
**When** I issue a write with an explicit concurrency precondition obtained from a prior read
**Then** the command succeeds only if the resource has not advanced since I read it, and is rejected with a clear conflict error otherwise.

**Acceptance criteria:**
- A read command surfaces the resource's current validator (the protocol `ETag`), e.g. via `orishuctl ... --show-version` (or an equivalent field in `-o json` output), so the administrator can discover it explicitly. The validator is never hidden client state.
- A write command accepts an opt-in flag, e.g. `orishuctl ... --if-match <validator>`, which is sent as the protocol `If-Match` precondition. This is the same opaque validator obtained from the read.
- If the resource advanced since the read, the command fails with a clear conflict message (mapped from `412 Precondition Failed`) telling the administrator to re-read and retry; it does not silently overwrite.
- If the administrator omits the flag, no concurrency check is applied and the write follows normal last-writer-wins ordering — the flag is strictly opt-in per command, consistent with [protocol-client.md](../../protocol-client.md#optimistic-concurrency).
- Making optimistic concurrency a CLI default (rather than opt-in) is deferred post-MVP; see [deferred runtime design](../../orishu-runtime-future-work.md).



### Maintain cluster security and auditability
As an administrator, I want to manage access and review changes to the cluster to ensure stability and security.

**Given** a running cluster
**When** I perform sensitive operations or security maintenance
**Then** the cluster remains secure and a record of the change is preserved.

**Acceptance criteria:**
- A command is available to retrieve the current join token, e.g. `orishuctl token`. On success, it prints the current token value without rotating it.
- A command is available to rotate join tokens, e.g. `orishuctl token --rotate`. It invalidates the current join token and generates a new one to prevent unauthorized nodes from joining after a maintenance window.
- Rotating the join token does not evict or otherwise disrupt workers that already joined successfully, but future join attempts must use the newly generated token.
- A command is available to review recent administrative actions, e.g. `orishuctl audit`. Audit output preserves actor identity and timing for traceability, but the actor who performed an earlier action does not gain exclusive authority to reverse or modify it later.
- Retrieving the current join token is read-only and does not create an audit event. Rotating the join token is a privileged state-changing action and does create an audit event.
- Privileged commands (like `rm`, `blocklist add`, or `cluster lock`) require authentication, while read-only diagnostics (`ls`, `inspect`) may be permitted from local or authorized clients.
- Workload submission validates content hashes and, when enabled by cluster policy, verifies workload signatures before any node begins execution.
- Administrator credentials, worker join tokens, and worker identity material are distinct and must not be interchangeable.
