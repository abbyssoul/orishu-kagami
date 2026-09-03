# orishu-worker
`orishu-worker` is the core runtime daemon for the [orishu](../README.md) project.

It runs the distributed simulation itself: each worker node participates in cluster membership,
state synchronization, workload execution, and convergence toward one coherent simulation state.
This is the part of the system that scales horizontally and is actively managed by operators.

For architecture and protocol details, see [design document](../docs/design.md).

## What It Does
`orishu-worker` is responsible for:

- Joining and maintaining decentralized cluster membership (no central control plane).
- Loading and running simulation workloads.
- Exchanging peer and simulation state updates across the cluster.
- Serving client-facing APIs for operators and tools (for example `orishu-ctl`) if configured to.
- Enforcing runtime limits and admission behavior for peers and clients.

In short: clients control and observe the system, while workers do the simulation work.

## Runtime Model
Every cluster node runs the same worker daemon with potentially different configuration. The configuration informs the role of the node in a cluster, not the workload it performs.
Nodes can be configured as:

- compute-focused nodes (peer interface enabled, client interface optional)
- introducer/seed-capable nodes (accepting join requests)
- mixed-role nodes (compute + client-facing APIs)

Workers are the scaling unit. Increasing simulation capacity is primarily done by adding
more `orishu-worker` nodes with suitable hardware/resources.

## Install / Build
Use project-level instructions in [README.md](../README.md).

For Debian-based systems, release builds also ship an `orishu-worker_<version>_<arch>.deb`
artifact. The package installs the worker binary, `/etc/orishu/orishu-worker.conf`, the systemd
unit, and the TLS credential drop-in example.

After build/install, verify the binary is available:

```sh
orishu-worker --help
```

If your local binary name differs in your build workflow, use the equivalent executable from
`cargo run` / `cargo install` output.

## Container Image
The repository ships a production-oriented `Dockerfile` for `orishu-worker`.
It follows the current Rust container baseline:

- multi-stage release build
- dependency-layer caching via [`cargo-chef`](https://crates.io/crates/cargo-chef)
- minimal distroless runtime image
- non-root execution by default
- runtime TLS/config supplied via flags and mounted files, not baked into the image

Build locally:

```sh
docker build --target orishu-worker -t orishu-worker .
```

Run the current worker implementation over TCP:

```sh
docker run --rm -p 6680:6680 orishu-worker
```

Enable TLS by mounting certificate material into `/etc/orishu/tls`:

```sh
docker run --rm -p 6680:6680 \
  -v "$(pwd)/certs:/etc/orishu/tls:ro" \
  orishu-worker \
  --listen.clients=0.0.0.0:6680 \
  --tls-cert=/etc/orishu/tls/worker.crt \
  --tls-key=/etc/orishu/tls/worker.key
```

## Systemd Service
The repository also ships a systemd unit for host-based worker deployments:

```sh
sudo install -D -m 0644 etc/orishu-worker.conf /etc/orishu/orishu-worker.conf
sudo install -D -m 0644 etc/systemd/orishu-worker.service /etc/systemd/system/orishu-worker.service
sudo systemctl daemon-reload
sudo systemctl enable --now orishu-worker
```

The service is designed for a system-wide deployment and listens on `/run/orishu/worker.sock`
by default via the checked-in config file. That socket is intentionally protected by the service's
Unix file permissions, so this root-managed deployment does **not** provide the same-user local
socket semantics that apply when `orishu-worker` is started manually or under a user session.

For the shipped unit, local socket administration is an admin action:

```sh
sudo orishuctl --host /run/orishu/worker.sock ls
```

For non-root administration of a system service, configure a TCP `listen.clients` address and use
the regular auth / TLS path instead of the local socket. Administrators can loosen socket
permissions with a local override if they intentionally want sudo-less management.

To enable TLS in the service, install a drop-in derived from
`etc/systemd/orishu-worker.service.d/tls-credentials.conf.example`. That pattern uses
systemd credentials so the service does not need direct read access to the private key file.

The Debian package already installs and enables this unit; it does not start the service
automatically. Review `/etc/orishu/orishu-worker.conf`, then start the worker explicitly with
`sudo systemctl start orishu-worker`.


## Quickstart
The fastest local path is to start a worker, then connect with operator tooling.

```sh
# 1. Start a local worker node
orishu-worker

# 2. In another terminal, inspect and control via CLI
orishuctl cluster info
orishuctl status
```

For multi-node experiments, run multiple workers with different identities/listen addresses and
point them at introducer seed addresses.

## Deployment Patterns
### 1. Minimal local test (single machine)
Run one `orishu-worker` and use `orishuctl` on the same machine to administer it. This is the
fastest way to validate a workload flow end-to-end in local development.

```sh
# terminal 1
orishu-worker

# terminal 2
orishuctl cluster info
orishuctl run <workload-resource>
orishuctl start
```

### 2. Single-machine experiment (multiple workers)
You can run multiple `orishu-worker` instances on the same machine to simulate a small cluster.

With default configuration, instances can coexist without manual TCP port allocation because by default worker uses Unix sockets, and each worker process _can_ create a unique socket path. 
For explicit network testing, switch to TCP listen addresses and assign distinct ports per instance.

### 3. Distributed cluster deployment
For a true distributed setup, deploy many worker nodes and split roles by network exposure:

- client-facing nodes that accept operator/client connections (effectively ingress/front-door nodes)
- compute-focused nodes that prioritize peer synchronization and simulation execution
- mixed-role nodes when topology simplicity is preferred

This allows admins to control network layout, blast radius, and capacity scaling. The same patterns
can be extended into orchestrated environments (for example Kubernetes) for larger, enterprise-grade
deployments.

## Configuration
`orishu-worker` follows the shared configuration model in [docs/config.md](../docs/config.md):

- Supported sources: config file, environment variables, command-line flags
- Precedence: `Config file < Environment variable < Command-line argument`
- XDG directories:
  - `$XDG_CONFIG_HOME` (default `~/.config`)
  - `$XDG_DATA_HOME` (default `~/.local/share`)
  - `$XDG_CACHE_HOME` (default `~/.cache`)

For worker-specific options and examples, see [docs/config.md](../docs/config.md) section
"orishu node config".

Current implementation note: the binary in this repository currently exposes a smaller runtime
surface than the full design docs describe. The implemented container/runtime flags today are:

- `--config`
- `--listen.clients`
- `--tls-cert`
- `--tls-key`

The currently implemented config-file fields are:

- `spec.listen.clients`
- `spec.tls.cert`
- `spec.tls.key`

The container and systemd documentation above reflect that actual shipped interface.

## Key Worker Configuration Areas
Commonly managed settings include:

- Node identity (`name`) and initial cluster identity (`cluster.name`)
- Peers/clients acceptance (`accepts.peers`, `accepts.clients`)
- Can node take on computational work (`accepts.work`) or just serve as a client-facing gateway and/or data replication target?
- Listen endpoints (`listen.peers`, `listen.clients`)
- Capacity limits on the number of connections TO the node (`limits.peers`, `limits.clients`)
- Limit number of seeds this node connects to (`limits.seeds`) to get updates from.
- Certificate/TLS settings for secure peer and client communication
- Storage backend and replication settings for checkpoints and results (`storage.backend`, `storage.replicas`, `storage.local.path`)

These settings determine how a worker joins, scales, and serves a cluster under real load.

## Security Notes
From the current design guidance:

- Peer-to-peer communication may require mTLS (or equivalent platform-provided encryption).
- Client-to-cluster communication is expected to use authenticated and TLS-protected channels.
- Worker admission and limits should be configured to reduce unauthorized access and overload risk.

See [docs/design.md](../docs/design.md) and [docs/config.md](../docs/config.md) for details.

## Troubleshooting
- Check available runtime flags:
  ```sh
  orishu-worker --help
  ```
- Worker does not join cluster:
  - verify seed/introducer addresses and `accepts.peers` on introducer nodes
  - check peer limits (`limits.peers`, `limits.seeds`)
  - verify certificate/trust configuration if TLS is enabled
- Client tools cannot connect:
  - verify `accepts.clients` and `listen.clients`
  - confirm network reachability/firewall rules
  - re-run `orishuctl auth` and verify endpoint selection

## Relationship to Other Components
- [`orishu-ctl`](../orishu-ctl/README.md): operator CLI for management and control.
- `orishu-worker`: executes and synchronizes the simulation.
- Additional client applications may consume worker client APIs for monitoring or visualization.
