# orishu-worker

`orishu-worker` is the Orishu runtime daemon. A running instance is a node; a
cluster is a set of nodes cooperating to execute one workload and manage its
artifacts.

From the repository root:

```sh
make build
make run-worker
```

The current implementation exposes configuration, client-listener, and TLS
options. Run `orishu-worker --help` for the implemented interface. Deployment
examples live in `etc/`; the root `Dockerfile` produces a non-root worker image.

See the [project architecture](../../docs/architecture.md), [security
policy](../../SECURITY.md), and root [README](../../README.md).
