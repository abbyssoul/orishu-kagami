# orishu-monitor

`orishu-monitor` is an interactive terminal UI (TUI) for administering an [orishu](../README.md) cluster. It provides the same administrative capabilities as [`orishu-ctl`](../orishu-ctl/README.md) but in a live, keyboard-driven interface — analogous to how [k9s](https://k9scli.io/) relates to `kubectl` for Kubernetes.

Both `orishu-monitor` and `orishu-ctl` are full-featured administrative clients. The choice between them is a matter of workflow: `orishu-ctl` is better suited for scripting, automation, and one-off commands; `orishu-monitor` is better suited for interactive cluster observation and management sessions where continuous visibility matters.

For architecture, data model, and API details, see [docs/design.md](../docs/design.md).

## Scope

`orishu-monitor` is focused on **cluster administration** with live visibility. It is the tool for:

- Watching node health, membership state, and advertised participation flags update in real time
- Inspecting individual nodes without leaving the terminal
- Observing a running simulation as it progresses
- Managing the blocklist and cluster membership lock interactively
- Loading and controlling simulation workloads
- Tailing cluster logs and events in a scrollable, filterable view
- Reviewing the audit trail of administrative actions

Like `orishu-ctl`, it is **not** a simulation authoring or result analysis tool. `orishu-monitor` can save a stored result artifact to a local file as an operational convenience, but crafting workload definitions and visualizing or post-processing results are tasks for separate client applications (such as `orishu-studio`).

## Install / Build

Use the project-level build instructions in [README.md](../README.md).

For Debian-based systems, release builds also ship an experimental
`orishu-monitor_<version>_<arch>.deb` artifact containing the monitor binary and package
documentation.

After building, verify the binary is available:

```sh
orishu-monitor --version
```

## Container Image
The repository `Dockerfile` can build a dedicated `orishu-monitor` image:

```sh
docker build --target orishu-monitor -t orishu-monitor .
docker run --rm -it orishu-monitor
```

Current implementation note: this image is experimental because the `orishu-monitor` binary in
this repository is still a placeholder and does not yet implement the TUI described below.

When the TUI is implemented, container users should prefer an explicit TCP endpoint for cluster
access:

```sh
docker run --rm -it \
  -e ORISHU_HOST=host.docker.internal:6680 \
  orishu-monitor
```

## Quickstart

```sh
# Connect to a local node (no auth required for read-only local access)
orishu-monitor

# Connect to a specific remote cluster
orishu-monitor --host cluster.example.com

# Connect and authenticate immediately
orishu-monitor --host cluster.example.com --auth
```

Once running, `orishu-monitor` opens to the cluster overview screen. Use keyboard shortcuts to navigate between views and act on cluster resources.

## Authentication

Read-only operations against a local node running under the same OS user require no credentials. Write operations and all remote access require Tier 2 (privileged) credentials.

When unauthenticated, write actions prompt for credentials inline — you do not need to restart the tool. Credentials are held in memory for the session. See [docs/design.md — Authentication and authorization](../docs/design.md) for the full access tier model.

## Navigation

`orishu-monitor` is organized into views, switchable by keyboard shortcut:

| Key | View |
|---|---|
| `1` | Cluster overview |
| `2` | Node list |
| `3` | Workload status |
| `4` | Live simulation stream |
| `5` | Results |
| `6` | Events |
| `7` | Logs |
| `8` | Audit log |
| `9` | Blocklist |
| `?` | Help / key reference |
| `q` | Quit |

Within each view, use arrow keys or `j`/`k` to move between items, `Enter` to select/expand, and view-specific action keys described below.

## Views and Capabilities

### Cluster Overview

A summary dashboard showing: node count, membership lock state, current workload phase, and overall cluster health. Updates continuously.

### Node List

A live table of all cluster members. Columns include node ID, version, host, advertised participation flags (`accepts.peers`, `accepts.work`, `accepts.clients`), connected peers, connected clients, resource summary, and health status. Offline or unhealthy nodes are visually distinguished.

**Actions on a selected node:**

| Key | Action |
|---|---|
| `Enter` | Open detailed node info panel |
| `d` | Run direct connectivity and health diagnostic |
| `D` | Run indirect diagnostic (prompts for a source node) |
| `r` | Remove node from cluster (requires auth; prompts for confirmation) |
| `R` | Re-admit a previously removed node (requires auth) |

In the MVP, `orishu-monitor` shows these participation flags as observed cluster state, but changing them still requires updating worker configuration and restarting the worker.

### Membership Lock

Accessible from the cluster overview or via a global shortcut.

| Key | Action |
|---|---|
| `L` | Lock cluster membership (prevents new nodes from joining) |
| `U` | Unlock cluster membership |

Both operations are idempotent. Locking does not affect nodes already in the cluster or disrupt ongoing simulations; it only blocks new admission. Crashes and voluntary disconnections can still remove nodes from the cluster regardless of lock state.

### Blocklist

A live view of all blocklist entries (node IDs and CIDR ranges).

| Key | Action |
|---|---|
| `a` | Add a node ID or CIDR range to the blocklist |
| `Delete` | Remove the selected blocklist entry |

Adding a blocklist entry for a currently-connected node immediately disconnects that node if membership is not locked. If membership is locked, the node is prevented from re-joining after it next disconnects. Removing an entry does not itself re-admit the node; it must go through the normal admission flow. All blocklist modifications require authentication and produce audit events.

### Workload Status

Shows the current simulation phase (`NotLoaded`, `Loading`, `Ready`, `Running`, `Stopped`, `Error`), simulation time, convergence metrics, and the partition-to-node assignment map.

| Key | Action |
|---|---|
| `l` | Load a workload definition (enter URL or local path) |
| `s` | Start the loaded simulation |
| `S` | Stop the simulation (prompts: graceful or immediate) |

Graceful stop saves a consistent state snapshot before halting. If the simulation is already running, `start` is a no-op. If no simulation is loaded, `start` shows an error.

### Live Simulation Stream

Connects to a running simulation and displays a continuously updating view of its current state. The initial snapshot is shown immediately so you can join mid-run without missing prior state. Disconnecting from the view does not affect the simulation or other observers. If the simulation stops while the view is open, an end-of-stream indicator is shown.

If no simulation is currently in `Running` state, the view indicates the current workload phase instead.

### Results

A reverse-chronological list of stored result artifacts. Each entry shows result ID, workload definition ID, start and end timestamps, simulation time range, artifact size, storage backend, and whether the result was captured gracefully or interrupted.

| Key | Action |
|---|---|
| `f` | Filter by workload ID or date range |
| `g` | Save selected result artifact to a local file |
| `Delete` | Delete selected result (requires auth; prompts for confirmation) |
| `P` | Bulk purge: delete all results matching a workload ID or older than a date (requires auth) |

Deletion is permanent and produces an audit event per deleted artifact.

### Events

A scrollable, filterable list of cluster events: node joins and leaves, workload lifecycle transitions, and administrative actions. Each entry includes timestamp, event type, affected resource, and relevant details.

### Logs

A live tail of cluster log lines aggregated from connected nodes. Supports filtering by log level and component. Read-only; may require auth for remote access depending on cluster configuration.

### Audit Log

A list of administrative audit events: who did what, when, to which resource, and whether it succeeded. Covers privileged operations such as membership lock/unlock, node removal, blocklist edits, workload commands, token rotation, and result purges.

### Token Rotation

Accessible via a global shortcut (`T` from any view). Creates a new join token, invalidating the current one. Workers that have already joined are unaffected; workers mid-join with the old token will be rejected and must retry. The new token value is shown once in a dialog. Requires authentication.

## Configuration

`orishu-monitor` uses the project-wide configuration model documented in [docs/config.md](../docs/config.md):

- Supported sources: config file, environment variables, command-line flags.
- Precedence: `Config file < Environment variable < Command-line argument`.
- XDG conventions:
  - Config: `$XDG_CONFIG_HOME` (default `~/.config`)
  - Data: `$XDG_DATA_HOME` (default `~/.local/share`)
  - Cache: `$XDG_CACHE_HOME` (default `~/.cache`)

## Troubleshooting

- **Key reference:** Press `?` at any time.
- **Connection or auth issues:**
  - Default target is localhost over a Unix domain socket.
  - Confirm the target node is running and configured with `accepts.clients=true`.
  - Use `--host` to specify a remote endpoint; you will be prompted for credentials.
- **Version mismatch:** Check the cluster overview for node versions vs. the local client version shown in the status bar.

## Further Reading

- [docs/user-stories](../docs/user-stories) — user stories capturing operator interactions with cluster administration, workload management, and monitoring.
- [docs/design.md](../docs/design.md) — the WHAT: high-level design of the orishu distributed system.
- [docs/architecture.md](../docs/architecture.md) —  the HOW: architecture, data model, APIs, of the worker node.
- [docs/config.md](../docs/config.md) — full configuration reference
