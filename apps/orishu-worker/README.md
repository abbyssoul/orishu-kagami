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

## Planned monitoring interface

Prometheus metrics (`/metrics`), startup/liveness/readiness probes and sampled
OTLP trace export are planned in [ADR 0017](../../docs/adr/0017-worker-operational-observability.md).
They are not implemented options yet. The design uses optional build features
and explicit runtime configuration, with no monitoring port or exporter enabled
by default. See the [observability guide](../../docs/orishu-observability.md)
and [operator manual task](../../docs/tasks/document-worker-observability.md)
for the planned configuration, deployment examples and troubleshooting work.

See the [project architecture](../../docs/architecture.md), [security
policy](../../SECURITY.md), and root [README](../../README.md).
