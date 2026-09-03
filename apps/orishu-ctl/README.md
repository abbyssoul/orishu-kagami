# orishu-ctl

`orishu-ctl` is the operator CLI for administering an [orishu](../README.md) cluster. It is one component of the broader orishu system — a self-organizing, decentralized simulation cluster described in detail in [docs/design.md](../docs/design.md).

## Scope

`orishu-ctl` is focused on **cluster administration**. It is the tool for:

- Inspecting cluster membership, node health, and connectivity
- Locking and unlocking cluster membership
- Managing the blocklist
- Loading and controlling simulation workloads
- Monitoring cluster logs, events, and the audit trail
- Rotating join tokens

It is **not** a simulation authoring or result analysis tool. Crafting workload definitions and visualizing or post-processing simulation output are tasks for separate client applications (such as `orishu-studio`). `orishu-ctl` can retrieve a stored result artifact and save it to a local file for offline use, but this is an operational convenience, not its primary purpose.

## Install / Build

Use the project-level build instructions in [README.md](../README.md).

For Debian-based systems, release builds also ship an `orishuctl_<version>_<arch>.deb` artifact
containing the standalone CLI binary and package documentation.

After building, verify the binary is installed and available:

```sh
orishuctl version
```

## Container Image
The repository `Dockerfile` can also build a dedicated `orishuctl` container image:

```sh
docker build --target orishu-ctl -t orishuctl .
docker run --rm orishuctl --help
```

When running the CLI in a container, prefer an explicit TCP endpoint because the default local
Unix socket path is usually not shared with the host:

```sh
docker run --rm \
  -e ORISHU_HOST=host.docker.internal:6680 \
  orishuctl cluster info
```

For remote or authenticated flows, pass the same flags or environment variables you would use
locally:

```sh
docker run --rm \
  -e ORISHU_HOST=cluster.example.com:6680 \
  -e ORISHU_TOKEN=secret-token \
  orishuctl ls
```

## Quickstart

```sh
# Authenticate against a local or remote node
orishuctl auth

# Inspect the cluster
orishuctl cluster info
orishuctl ls

# Load and run a workload
orishuctl workload load ./examples/workload.yaml
orishuctl workload start
orishuctl workload status

# Stop (graceful by default)
orishuctl workload stop
```

## Authentication

Many operations require Tier 2 (privileged) credentials. Read-only operations against a local node running under the same OS user are permitted without credentials.

```sh
orishuctl auth
```

If no host is provided, `auth` targets a local node by default.

```text
Username[$USERNAME]:
Password: ****
```

Remote access always requires authentication, regardless of operation type. See [docs/design.md — Authentication and authorization](../docs/design.md) for the full access tier model.

## Target Cluster Selection

Commands operate against the active target cluster context. Use flags or configuration to select endpoint and credentials for each environment (local dev, staging, production).

```sh
orishuctl --help
orishuctl cluster --help
```

## Command Reference

`orishu-ctl` follows a `git`/`kubectl`/`docker`-style subcommand model.

### Cluster

| Command | Purpose |
|---|---|
| `ls` | List all nodes with identity, version, advertised participation flags, resource summary, and health. |
| `inspect <node-id>` | Detailed report for a specific node: host, role, resources, connectivity. |
| `diagnose <node-id>` | Direct connectivity and health check from the local client to the target node. |
| `diagnose <node-id> --from <source-node-id>` | Indirect check: asks `source-node-id` to verify connectivity to `node-id`. Useful for 
diagnosing network partitions or asymmetric routing. |
| `rm <node-id> [--force]` | Remove a node from the cluster. Default is graceful: the node finishes in-flight work and transfers results before exiting. `--force` drops the node immediately and instructs peers to reject its pending data. Requires authentication. |
| `cluster info` | Cluster-level summary: node count, membership lock state, workload phase. |
| `cluster lock` | Lock cluster membership: prevents new nodes from joining. Idempotent. |
| `cluster unlock` | Unlock cluster membership: re-enables new node admission. Idempotent. |

### Node Admission

| Command | Purpose |
|---|---|
| `readmit <node-id>` | Explicitly re-admit a previously removed (tombstoned) node. The node must not be on the blocklist and membership must not be locked. |

A removed or blocklisted node cannot auto-rejoin the cluster. Re-admission requires:
1. Removing the node from the blocklist (if listed).
2. Issuing an explicit re-admit command to clear the tombstone.
3. Ensuring membership is unlocked.
4. The node then follows the normal admission flow.

### Blocklist

| Command | Purpose |
|---|---|
| `blocklist ls` | List all current blocklist entries (node IDs and CIDR ranges). |
| `blocklist add <target>` | Add a node ID or CIDR network range to the blocklist. If the target matches a connected node and membership is not locked, the node is immediately disconnected. |
| `blocklist rm <target>` | Remove a blocklist entry. Does not itself re-admit the node; it must go through the normal admission flow. |

All blocklist operations are idempotent and require authentication.

### Workload

| Command | Purpose |
|---|---|
| `run <url\|path>` | Distribute a workload definition to all nodes and start when ready. |
| `workload load <url\|path\|- >` | Distribute a workload definition to all nodes. |
| `workload unload [--force]` | Remove the loaded workload from all nodes. By default the unload is coordinated with the cluster state; `--force` unloads immediately without waiting. |
| `workload check <url\|path\|- >` | Evaluates the referenced workload manifest against every node in the cluster and reports which nodes can satisfy the workload's requirements. |
| `workload start [--force]` | Command the cluster to begin or resume simulation. Idempotent if already running. `--force` starts immediately without waiting for all nodes to be ready; late-joining nodes synchronize from a catch-up snapshot. |
| `workload stop [--force]` | Stop the simulation. Default is graceful: workers complete their current time step and save a consistent state snapshot. `--force` halts immediately without waiting for a snapshot. |
| `workload status` | Show current simulation phase, simulation time, convergence metrics, and partition distribution. |
| `workload stream` | Connect to a loaded simulation and receive a live stream of state updates while it is `Ready`, `Running`, `Stopped`, or `Error`. Connecting does not affect the simulation or other observers. |

### Results

`orishu-ctl` provides limited access to stored simulation result artifacts for operational purposes. For analysis and visualization, use a dedicated client.

| Command | Purpose |
|---|---|
| `results ls [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>]` | List stored result artifacts in reverse-chronological order, optionally filtered by workload ID, exact workload name, or recording time. |
| `results get <result-id>` | Retrieve metadata for a stored result artifact, including content size. |
| `results get <result-id> --download -O <path>` | Download a result artifact to a local file. |
| `results rm <result-id>` | Permanently delete a result artifact. Produces an audit event. Requires authentication. |
| `results purge [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>]` | Bulk delete results matching workload ID, exact workload name, and/or time-based filters. Requires authentication. |

`--workload-name` matches exactly against the workload manifest name captured in stored result metadata. Matching is case-sensitive.

### Checkpoints

`orishu-ctl` also exposes stored checkpoints for operational inspection, download, and cleanup.

| Command | Purpose |
|---|---|
| `checkpoints ls [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>] [--resumable <bool>]` | List stored checkpoints, optionally filtered by workload ID, exact workload name, recording time, or resumability. |
| `checkpoints get <checkpoint-id>` | Retrieve metadata for a stored checkpoint. |
| `checkpoints get <checkpoint-id> --download -O <path>` | Download checkpoint data to a local file. |
| `checkpoints rm <checkpoint-id>` | Permanently delete a checkpoint. Produces an audit event. Requires authentication. |
| `checkpoints purge [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>] [--resumable <bool>]` | Bulk delete checkpoints matching workload ID, exact workload name, time-based, and/or resumability filters. Requires authentication. |

`--workload-name` matches exactly against the workload manifest name captured in stored checkpoint metadata. Matching is case-sensitive.

### Security and Audit

| Command | Purpose |
|---|---|
| `token` | Print the current join token value. This does not rotate the token or change cluster state. |
| `token --rotate` | Create a new join token, invalidating the current one. Already-joined nodes are not affected. The new token value is shown once. |
| `logs [--level <level>] [--after <timestamp>] [--before <timestamp>]` | Fetch recent log lines from the cluster. |
| `events [--type <type>] [--after <timestamp>] [--before <timestamp>]` | List recent cluster events: node joins/leaves, workload lifecycle, and administrative actions. |
| `audit` | List recent administrative audit events with actor, target, and outcome. |

### Utility

| Command | Purpose |
|---|---|
| `version` | Show CLI version and cluster version when connected. |
| `completion <shell>` | Generate shell completion script (e.g. `zsh`, `bash`). |
| `--help` / `<subcommand> --help` | Global or command-specific help. |

## Configuration

`orishu-ctl` uses the project-wide configuration model documented in [docs/config.md](../docs/config.md):

- Supported sources: config file, environment variables, command-line flags.
- Precedence: `Config file < Environment variable < Command-line argument`.
- XDG conventions:
  - Config: `$XDG_CONFIG_HOME` (default `~/.config`)
  - Data: `$XDG_DATA_HOME` (default `~/.local/share`)
  - Cache: `$XDG_CACHE_HOME` (default `~/.cache`)

## Troubleshooting

- **Command shape and flags:** `orishuctl --help` / `orishuctl <subcommand> --help`
- **Connection or auth issues:**
  - Verify target endpoint and credentials. Default is localhost over a Unix domain socket.
  - In a container, set `ORISHU_HOST` to a reachable TCP endpoint unless you intentionally mounted a shared Unix socket.
  - Re-run `orishuctl auth`.
  - Confirm the target node is running and configured with `accepts.clients=true`.
- **Version mismatch:** Run `orishuctl version` and compare with node versions from `orishuctl cluster info`.

## Further Reading

- [docs/user-stories](../docs/user-stories) — user stories capturing operator interactions with cluster administration, workload management, and monitoring.
- [docs/design.md](../docs/design.md) — the WHAT: high-level design of the orishu distributed system.
- [docs/architecture.md](../docs/architecture.md) —  the HOW: architecture, data model, APIs, of the worker node.
- [docs/config.md](../docs/config.md) — full configuration reference
