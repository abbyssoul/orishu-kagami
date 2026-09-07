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

The implemented local diagnostics slice uses these typed settings, with normal
file < environment < CLI precedence:

| YAML setting | Environment | CLI | Default |
| --- | --- | --- | --- |
| `spec.observability.enabled` | `ORISHU_OBSERVABILITY_ENABLED` | `--observability.enabled true/false` | false |
| `spec.observability.bind` | `ORISHU_OBSERVABILITY_BIND` | `--observability.bind IP:PORT` | `127.0.0.1:9168` when enabled |
| `spec.observability.metrics` | `ORISHU_OBSERVABILITY_METRICS` | `--observability.metrics true/false` | true; selects `/metrics` only |
| `spec.observability.probes` | `ORISHU_OBSERVABILITY_PROBES` | `--observability.probes true/false` | true; selects `/livez`, `/readyz`, `/startupz` together |

Enablement requires the `observability` build feature. Missing capability and
non-loopback exposure fail validation before credentials/listeners start.
An enabled diagnostics address is bound fallibly before credential creation,
membership-owner startup or client binding. Address-in-use and other bind
errors report `cannot bind diagnostics listener` with exit code 2, not a panic.
The bound acceptor is retained until serving starts; there is no probe/rebind
race. TCP connection establishment during initialization is not probe success.
A disabled listener binds nothing, even if a bind setting is retained. Unknown
keys within `spec.observability` are rejected. Route selection does not enable
the listener. When enabled, at least one of metrics or probes must be selected;
an empty listener is a startup configuration error before credentials/listeners
start. Disabled routes are absent (`404`), not successful placeholder responses.
Metrics-only and probe-only configurations are still loopback-only. Secured
remote binds and trace-export configuration remain planned; no join/operator token
should be supplied to a scraper for this unauthenticated loopback surface.

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

### Implemented tracing budget configuration; exporter unavailable

The worker accepts and validates these staged settings through file < environment
< CLI precedence. `tracing.enabled=true` currently exits with a capability error
before state/listener creation in every build. The `otlp-tracing` Cargo feature
currently compiles bounded sampling/queue primitives and the protobuf codec,
not an active exporter;
there is no collector activity yet. Disabled tracing is the default; omitted
values use the defaults below when validating relationships.

| YAML field under `spec.tracing` | Environment | CLI | Default and allowed range |
| --- | --- | --- | --- |
| `enabled` | `ORISHU_TRACING_ENABLED` | `--tracing.enabled` | false; true is currently unavailable |
| `endpoint` | `ORISHU_TRACING_ENDPOINT` | `--tracing.endpoint` | unset; explicit OTLP/HTTP trace URL including its path |
| `caFile` | `ORISHU_TRACING_CA_FILE` | `--tracing.ca-file` | unset; explicit PEM collector trust-root file |
| `clientCertFile` | `ORISHU_TRACING_CLIENT_CERT_FILE` | `--tracing.client-cert-file` | unset; PEM collector-client chain, paired with client key |
| `clientKeyFile` | `ORISHU_TRACING_CLIENT_KEY_FILE` | `--tracing.client-key-file` | unset; private PEM collector-client key, paired with chain |
| `bearerTokenFile` | `ORISHU_TRACING_BEARER_TOKEN_FILE` | `--tracing.bearer-token-file` | unset; private file containing a collector-only token |
| `samplePpm` | `ORISHU_TRACING_SAMPLE_PPM` | `--tracing.sample-ppm` | 1000; 0–1,000,000 |
| `queueCapacity` | `ORISHU_TRACING_QUEUE_CAPACITY` | `--tracing.queue-capacity` | 1024 spans; 1–4096 |
| `batchSize` | `ORISHU_TRACING_BATCH_SIZE` | `--tracing.batch-size` | 128 spans; 1–256 and no larger than queue capacity |
| `activeSpanCapacity` | `ORISHU_TRACING_ACTIVE_SPAN_CAPACITY` | `--tracing.active-span-capacity` | 128 sampled spans; 1–4096 |
| `exportMaxBytes` | `ORISHU_TRACING_EXPORT_MAX_BYTES` | `--tracing.export-max-bytes` | 1,048,576 bytes; 1024–4,194,304 |
| `responseMaxBytes` | `ORISHU_TRACING_RESPONSE_MAX_BYTES` | `--tracing.response-max-bytes` | 16,384 bytes; 1024–65,536 |
| `flushIntervalMs` | `ORISHU_TRACING_FLUSH_INTERVAL_MS` | `--tracing.flush-interval-ms` | 1000 ms; 100–10,000 |
| `exportTimeoutMs` | `ORISHU_TRACING_EXPORT_TIMEOUT_MS` | `--tracing.export-timeout-ms` | 2000 ms; 100–10,000 |
| `shutdownTimeoutMs` | `ORISHU_TRACING_SHUTDOWN_TIMEOUT_MS` | `--tracing.shutdown-timeout-ms` | 3000 ms; 100–10,000 |

Invalid limits and unknown nested keys are rejected even when tracing is
disabled. Zero sampling is valid but does not implicitly enable or disable
exporter startup. These are resource-policy bounds, not measured overhead or
delivery guarantees. Queue capacity will bound waiting completed spans; an
in-flight batch and active sampled spans have independent budgets. These
additional values are validated configuration, not implemented exporter limits:
activation remains unavailable. The exporter must reserve an active slot before
retaining sampled-span state, bound encoding before growing an export buffer,
and stop consuming collector response bytes at the configured limit. Batch
count and request-byte limits apply together; reaching either flushes a bounded
batch, and a single span that cannot fit must be shed without blocking domain
work. Per-span attribute/link/event size/count limits are still required before
the exporter is activated; a count cap alone does not bound retained memory.
Response limits must cover consumed/decompressed bytes, not merely a peer's
claimed `Content-Length`. Shutdown's
whole flush deadline must cap export attempts even when shorter than an
individual export timeout. Future full-queue and outage handling must shed
telemetry without blocking domain work or extending shutdown.

The staged destination contract targets OTLP/HTTP binary Protobuf, using an
explicit complete trace URL (for example `https://collector.example/v1/traces`).
There is no implicit collector address, path suffix, DNS lookup or outbound
connection during validation. HTTPS is required except for HTTP to a literal
loopback IPv4/IPv6 address; `http://localhost/...` is refused rather than relying
on DNS to establish loopback. Explicit ports must be 1–65535. Input is capped
at 2048 ASCII bytes and must include a non-root absolute path. User information,
query strings, fragments, whitespace/control bytes, backslashes, percent escapes
and dot path segments are refused. This deliberately narrow profile permits
collector-specific unescaped paths without accepting URL-carried secrets.

Endpoint input has redacted debug output. Validation follows precedence and
returns a fixed diagnostic without echoing the rejected URL, including when
disabled; overriding an invalid file value with a valid environment/CLI value
is allowed. This does not provision TLS trust or credentials. The exporter must
require a destination when activated, verify server identity, reject redirects
and isolate ambient SDK/proxy configuration. OTLP/HTTP's trace message and path
conventions are defined by the [OTLP specification](https://opentelemetry.io/docs/specs/otlp/);
these application settings are not a claim of implemented OTLP delivery.

Collector file settings are also staged, not loaded credentials. Every supplied
path must be nonempty and at most 4096 encoded bytes. Client certificate and
key must be configured together. Any trust/credential file requires an explicit
HTTPS endpoint, even while disabled; loopback HTTP is credential-free. The
settings are independent, so a collector may require both mTLS and bearer
authorization. No peer identity, join token, operator token or monitoring-proxy
credential is selected implicitly. Provision dedicated collector material.
Relative paths refer to the worker's working directory; changing directories
must not silently select a different secret in a deployment recipe.

Disabled tracing does not open, create or validate contents of these files.
Before exporter activation, implement bounded safe file loading, private-key
and token permission checks, PEM/chain/key consistency and header-safe token
validation. An explicit CA bundle must define the exporter's trust rather than
silently augmenting ambient SDK settings; absence of a bundle requires an
explicitly implemented and tested platform-root policy. No insecure TLS or
server-name bypass option is introduced. File-content, trust and private-key
errors must fail startup without exposing contents; collector network outage
after valid setup must remain non-fatal to domain work.

Actual credential loading/TLS enforcement, span/attribute limits,
dependency selection, SDK environment isolation and real delivery tests are
still required before enabling the exporter. They are not implied by accepting
these settings. Peer propagation additionally awaits
[ADR 0025](adr/0025-version-peer-trace-context-propagation.md) review; local
exporter work does not require changing the current peer profile.

### Remaining observability configuration

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
