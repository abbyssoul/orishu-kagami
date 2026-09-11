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

## Implemented structured stdout logging

Source-built Unix workers support optional version-1 JSON-line stdout logging,
independently of both telemetry Cargo features. It is disabled by default.
Non-Unix builds reject explicit enablement rather than choosing a blocking
fallback. All supplied budgets are validated even while disabled.

| Field under `spec.logging` | Environment | CLI | Default and bounds |
| --- | --- | --- | --- |
| `enabled` | `ORISHU_LOGGING_ENABLED` | `--logging.enabled` | false; explicit true/false |
| `queueRecords` | `ORISHU_LOGGING_QUEUE_RECORDS` | `--logging.queue-records` | 256; 1–4096 records |
| `shutdownMs` | `ORISHU_LOGGING_SHUTDOWN_MS` | `--logging.shutdown-ms` | 250; 0–2000 ms |

```yaml
spec:
  logging:
    enabled: true
    queueRecords: 256
    shutdownMs: 250
```

File < environment < CLI precedence includes explicit false and zero. Unknown
logging fields, unsupported destinations and out-of-range budgets fail startup.
There is no vendor sink, level selector, arbitrary dependency-log capture or
implicit exporter activation. Lifecycle/failure events remain available with
tracing disabled or zero-sampled; operation records require actual locally
sampled spans and carry their exact trace/span IDs. Records contain no worker
names, paths, credentials or arbitrary error text. Use the collector's
process/target metadata to distinguish worker stdout streams.

See the [bounded record and loss contract](orishu-observability.md#bounded-operational-log-adapter)
and [worker manual](../apps/orishu-worker/README.md#structured-stdout-logs).
Standard startup errors remain separate from runtime output. The logger's
bounded drain occurs after existing owner/server/exporter shutdown; its timeout
is not a replacement for those independent deadlines or a durable-flush promise.

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

Explicit per-role network placement uses the same precedence:

| YAML setting | Environment | CLI | Default |
| --- | --- | --- | --- |
| `spec.interface.peers` | `ORISHU_INTERFACE_PEERS` | `--interface.peers NAME` | unset; peer sockets follow host routing |
| `spec.interface.clients` | `ORISHU_INTERFACE_CLIENTS` | `--interface.clients NAME` | unset; client replies follow host routing |

A placed role binds every socket it creates to that device, covering listening,
the outbound initial join and client replies. Placing a role requires a wildcard
bind for it, because an address belonging to a different device binds
successfully and then matches no ingress. An unknown interface, a concrete bind,
a client interface with only Unix listeners, or a non-Linux platform fails
startup before any listener exists; there is no fallback to an unconstrained
socket. Unknown keys within `spec.interface` are rejected, though a misspelled
outer key is still discarded like any other unknown `spec` key, so operators
confirm the startup `placement` report. Startup-only; no hot reload. See
[ADR 0026](adr/0026-worker-network-interface-placement.md).

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

<a id="implemented-tracing-budget-configuration-exporter-unavailable"></a>

### Implemented local tracing configuration

The worker accepts these settings through file < environment < CLI precedence.
With `otlp-tracing`, `tracing.enabled=true` enables sampled local client-service
spans at the explicit endpoint. Without that feature, enabling export fails
startup. Disabled tracing is the default and performs no credential-file or
collector IO. Invalid credentials fail before state/listener creation. HTTPS
uses the explicit CA file when configured, otherwise the supported system
bundle described below. Other trust-store layouts need an explicit CA file.

| YAML field under `spec.tracing` | Environment | CLI | Default and allowed range |
| --- | --- | --- | --- |
| `enabled` | `ORISHU_TRACING_ENABLED` | `--tracing.enabled` | false; true requires the `otlp-tracing` build capability and explicit endpoint |
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

Invalid limits and unknown nested keys are rejected even when disabled.
Zero sampling is valid and produces no spans. Active spans, queued records,
the reusable batch buffer and encoded request have separate bounds. Records
retain fixed-size identities and finite operation/outcome vocabulary, with
no arbitrary attributes, events, links, headers or payloads. The codec checks
size before output allocation. Collector bodies are bounded while streaming;
decompression is disabled rather than trusting compressed Content-Length.
Shutdown's single drain deadline caps attempts even when shorter than an
individual export timeout. Full queues and exporter failures shed telemetry.
These are resource limits, not measured performance or delivery guarantees.
Graceful shutdown reports fixed aggregate delivery and queue sampling/drop
counts. Queue acceptance is not collector delivery; interrupted exports may
already have reached the collector. With both capabilities and runtime metrics
and tracing enabled, twelve live trace counters extend `/metrics` to 163 series;
allow a 32 KiB scrape response budget. Tracing disabled or omitted leaves the
base 151-series catalogue, including catch-up, membership deadlines and peer IO metrics, within
the same 32 KiB bound. Peer counter collection follows metrics enablement,
not tracing or probe enablement. See the
[counter semantics and troubleshooting](orishu-observability.md#live-trace-delivery-and-loss-counters).

The destination contract targets OTLP/HTTP binary Protobuf, using an
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
the local delivery evidence does not establish cross-peer trace propagation.

Collector files are loaded only when tracing is enabled. Every supplied
path must be nonempty and at most 4096 encoded bytes. Client certificate and
key must be configured together. Any trust/credential file requires an explicit
HTTPS endpoint, even while disabled; loopback HTTP is credential-free. The
settings are independent, so a collector may require both mTLS and bearer
authorization. No peer identity, join token, operator token or monitoring-proxy
credential is selected implicitly. Provision dedicated collector material.
Relative paths refer to the worker's working directory; changing directories
must not silently select a different secret in a deployment recipe.

Disabled tracing does not open, create or validate contents of these files.
The Unix `CollectorFiles` adapter implements bounded file loading,
private-key/token permission checks, PEM decoding, certificate/key matching
and header-safe token validation during enabled worker startup.
Its current file contract is:

- CA and client-chain PEM files: at most 65,536 bytes, containing at most 64
  roots or eight chain certificates respectively. The chain is leaf-first;
  ordinary TLS verification at the collector still decides client trust.
- Key PEM: at most 16,384 bytes and exactly one supported private key.
  Token: at most 4096 header-safe characters, optionally followed by one LF or
  CRLF (4098 file bytes maximum); arbitrary surrounding whitespace is refused.
- Files are single-link regular files owned by the worker user or root. Keys
  and tokens have no group/other permissions; public CA/chain files may be
  readable but cannot be group/other writable.
- Paths are walked using directory descriptors without following symlinks;
  parent traversal and more than 128 components are refused. Traversed
  directories are root/worker-owned and not group/other writable, except
  root/worker-owned sticky directories such as `/tmp`. Relative paths start
  at the worker's working directory. Symlink-based mounted secret recipes
  are not supported by this loader; do not assume a Kubernetes recipe works.
- Files are read without creating or modifying them. Non-regular files,
  including FIFOs, are refused without waiting for a writer. Growth after
  metadata inspection is checked with one extra byte in a fixed-size buffer.

An explicit CA bundle replaces default trust; invalid explicit input never
falls back to another source. On Linux, an omitted CA file loads only
`/etc/ssl/certs/ca-certificates.crt`, the OS-managed bundle used by the
[Debian ca-certificates tooling](https://manpages.debian.org/bookworm/ca-certificates/update-ca-certificates.8.en.html).
This supported Debian/Ubuntu layout has a 1 MiB file limit and 1024-certificate
limit, with root-owned directories/file and the same non-symlink, regular-file,
single-link and write-permission checks. Empty, malformed, missing, unsafe or
oversized bundles fail startup; partial trust is not silently accepted.
`SSL_CERT_FILE`, `SSL_CERT_DIR`, SDK settings and directory enumeration cannot
select or augment this source. System trust is loaded once at startup; restart
after an OS trust update. Other Linux layouts and non-Linux platforms require
an explicit CA file supported by the credential loader. This does not provide
Windows/macOS native trust integration. The feature-enabled Linux default-store
integration test requires an installed, valid `ca-certificates` bundle; it does
not modify host trust to install fixture certificates. No insecure TLS or
server-name bypass option is introduced. File-content, trust and private-key
errors must fail startup without exposing contents; collector network outage
after valid setup must remain non-fatal to domain work.

The delivery primitive has real HTTP/HTTPS tests with explicitly supplied TLS
configuration and synthetic span records, including a successful mTLS exchange
using loaded credential files. Startup credential loading and trust
selection, local client-service instrumentation, configured budgets and bounded
exporter shutdown are connected, with real-worker loopback receipt evidence.
Live drop counters have scoped scrape/parser evidence. Remaining
process/deployment coverage and log correlation still need acceptance evidence;
other native trust-store layouts are outside the tested Linux support. Peer propagation awaits
[ADR 0025](adr/0025-version-peer-trace-context-propagation.md) review; local
exporter work does not require changing the current peer profile.

### Remaining observability configuration

[ADR 0017](adr/0017-worker-operational-observability.md) adds optional build
features `observability` (Prometheus and probe HTTP listener) and `otlp-tracing`
(trace exporter). Both capabilities now exist; the items below retain the full
configuration contract, not a claim that every deployment setting is available.
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

Use the implemented settings above and the
[worker manual](../apps/orishu-worker/README.md) for available flags, environment
variables, ports and limits. Future deployment/security settings must remain
explicitly planned until their owning implementation and tests land.
