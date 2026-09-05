# Orishu configuration model

Orishu binaries must start usefully without mandatory configuration files.
Operators may configure workers through files, environment variables, and
command-line arguments, with later/more specific sources taking precedence:

```text
configuration file < environment variable < command-line argument
```

This is an operator model, not permission for different sources to use
different schemas. Each option has one typed meaning and validation path.

Configuration and data locations follow XDG conventions where supported.
System packages may use platform locations such as `/etc/orishu` and
`/run/orishu`; user-scoped execution must not require writable system paths.
Credentials and private keys never belong in workload manifests, run
references, logs, or artifact identity.

## Worker configuration

Worker startup configuration includes listener addresses, TLS identity,
resource limits, peer/client/work admission flags, storage backend and
replication settings, and optional explicit introducer/join information.

Workers may intentionally expose only the client or peer listener appropriate
to their topology. A worker not accepting inbound peers may still join and
perform work; a worker not accepting clients remains observable through
explicitly designed relay/cached cluster views rather than by pretending it is
directly reachable.

In the MVP, `accepts.peers`, `accepts.clients`, `accepts.work`, and
`storage.replicas` are startup settings. Live changes are deferred in
`orishu-runtime-future-work.md`.

Environment-variable and CLI spelling must be documented beside the owning
binary and tested against the same typed configuration loader. Unknown,
malformed, contradictory, unsafe, or inaccessible configuration fails clearly;
there is no silent fallback that changes security or durability.

## Client configuration

Operator clients primarily configure endpoints, authentication, output, and
local persistence. Kagami additionally owns presentation and authoring
preferences. Endpoint hints never establish formation/run identity, and client
credentials never become shareable run-reference data.

Binary-specific supported options belong in each application's README. This
document defines the cross-application rules, including precedence and trust.

## Planned worker observability configuration

[ADR 0017](adr/0017-worker-operational-observability.md) adds optional build
features `observability` (Prometheus and probe HTTP listener) and `otlp-tracing`
(trace exporter). These features and settings are planned, not current options.
Official operator builds will include both; Cargo default/minimal builds omit
them. All runtime exposure/export is disabled by default.

The [implementation task](tasks/implement-worker-observability.md) must document
and test one typed configuration path for:

- listener enablement, bind address/port, route switches, TLS, monitoring-only
  credentials and explicit remote probe-only access;
- request/scrape bounds and control-loop supervision thresholds; and
- trace enablement, OTLP destination/transport/trust/credentials, sampling,
  bounded queue/batch sizes, export deadlines and shutdown flush deadline.

Use file < environment < CLI precedence and startup-only changes. A requested
feature missing from the build, unsafe exposure, invalid TLS or a failed bind
is a clear startup error. A valid collector becoming unavailable drops bounded
telemetry without making the worker unready. Enabling diagnostics defaults to
loopback; it never implicitly opens a wildcard interface. Client/peer/work
admission flags do not silently enable or disable diagnostics.

Exact flags, environment variables, port and numeric limits land with the
task and the [worker manual](../apps/orishu-worker/README.md); examples must not
claim those switches already exist.
