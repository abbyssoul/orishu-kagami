# Receive worker traces with a local OpenTelemetry Collector

Status: **verified source-built Linux walkthrough; local client-service spans only**

This uses the official **otelcol 0.160.0** binary and the checked-in
[collector configuration](../etc/otelcol-worker-local.yml). Unlike the bounded
HTTP responder in the Prometheus test, this collector decodes OTLP and writes
inspectable span records. It is an optional operator tool, not part of worker
startup, cluster authority, or a bundled telemetry service.
For a certificate-authenticated receiver, see the separately tested
[Collector mTLS recipe](testing-worker-otelcol-mtls.md).

## Pinned prerequisites

Build from this checkout with the selected Rust toolchain. Install Python 3
and obtain the official Linux amd64 collector archive from the
[0.160.0 release](https://github.com/open-telemetry/opentelemetry-collector-releases/releases/tag/v0.160.0).
Verify the archive against its published checksum before extracting/running it:

```text
otelcol_0.160.0_linux_amd64.tar.gz
SHA-256: 5415b8daf782f17cc463c3e46816abf181a68a04b7bcf98c273c3c204096c743
```

The tested extracted `otelcol` executable SHA-256 is
`abe338fa33865e54412566db5cea4adc82374594b65b49c1099048cdf8cf2451`.
The harness requires its exact version string; another distribution or version
needs separate validation. No harness or Make target downloads tools, installs
a service, or publishes data to an external backend.

## Automated operator journey

From the repository root:

```sh
make test-worker-otelcol OTELCOL=/absolute/path/to/otelcol
```

The target builds the worker with both `observability` and `otlp-tracing` into
`target/worker-otelcol`, runs the receipt validator's negative controls and
exercises the real worker, CLI and collector. It does not replace binaries in
`target/debug`. Override `WORKER_OTELCOL_TARGET_DIR` to reuse a known idle build
directory, or use the equivalent isolated commands:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir target/formation-flow-observability
python3 scripts/test_worker_otelcol.py
python3 scripts/check-worker-otelcol.py --otelcol /absolute/path/to/otelcol --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

Do not rebuild these executable paths while a journey uses them. The check uses
private temporary state, separate loopback ports and a real Unix client socket;
an occupied port is a failure, not permission to stop another process. It:

1. Requires the pinned collector to reject an invalid configuration and accept
   the example. Only its listener port and private output path vary per run.
   A malformed protobuf request must receive HTTP 400, not enter the trace file.
2. Starts separate runtime-disabled, zero-sampled and fully sampled workers.
   After clean worker/collector shutdown, disabled and zero-sampled modes have
   no received spans. Disabled tracing omits trace metrics; zero sampling
   reports two sampled-out client requests and no queued records.
3. Receives three enabled-mode local spans: initial summary, rejected
   unauthenticated lock, and a summary proving the lock did not change.
   Exactly one span has the `rejected` adapter outcome. Resource/scope/name,
   nonzero trace/span IDs, root-parent semantics and timestamps are checked.
   Seeded operator credentials, worker labels, operation names and private-key
   markers must not occur in the file. Arbitrary span attributes are rejected.
4. Stops and reaps the collector. Authenticated lock and summary still succeed
   with unchanged formation identity and successful readiness; worker counters
   report two failed exports and retain the earlier three accepted spans.
5. Restarts only the collector, on the same endpoint but with a **new output
   file**. Unlock and summary deliver two fresh spans with new trace IDs. Old
   receipts cannot satisfy recovery. Earlier failed spans are not replayed.
6. Shuts down the worker before its receiver and verifies clean exits and
   client-socket cleanup. Diagnostic scrapes/probes did not create extra spans.

The whole script has a 120-second budget; observation polling checks a
ten-second deadline between attempts, commands have ten seconds, and HTTP/TLS
sockets have a one-second timeout. The socket timeout is not a whole-exchange
deadline for trickled input; the outer script alarm remains the final bound.
Each graceful process exit has three seconds before forced kill/reap and
failure; cleanup can extend beyond the script alarm. Receipt reads are capped at
256 KiB and 32 spans; duplicate fields/IDs, partial final records and oversized
files fail validation. Live reads ignore an unfinished last line until complete.
These are bounded smoke-test settings, not production throughput or shutdown
SLOs. Process output is suppressed; fixed phase messages identify failures
without dumping credentials or raw traces. Temporary fixture state and receipt
files are removed on exit; use the manual workflow below to retain local data.

## Inspect one local request manually

Run commands from the repository root, in separate terminals. Use a new private
directory for each session: trace files are disposable diagnostics and the
rotating exporter may replace/prune them. Never point it at an audit archive.

In the collector terminal:

```sh
umask 077
export ORISHU_OTEL_TRACE_FILE="$(mktemp -d)/traces.jsonl"
export GOMEMLIMIT=100MiB
echo "$ORISHU_OTEL_TRACE_FILE"
/absolute/path/to/otelcol validate --config etc/otelcol-worker-local.yml
/absolute/path/to/otelcol --config etc/otelcol-worker-local.yml
```

The explicit environment value is the local output path, not a credential.
The example has no default output file: validation fails if it is missing.
It binds only `127.0.0.1:4318`, exposes OTLP/HTTP, and enables only the traces
pipeline; no gRPC, debug/health extension, remote exporter or collector-metrics
listener is configured. Failure to bind must be resolved by selecting an
explicit free endpoint in a private copy and matching the worker endpoint.

In the worker terminal, using the isolated build above:

```sh
umask 077
worker_trace_dir=$(mktemp -d)
echo "$worker_trace_dir"
target/formation-flow-observability/debug/orishu-worker --state-dir "$worker_trace_dir/state" --listen.clients "$worker_trace_dir/api.sock" --tracing.enabled true --tracing.endpoint http://127.0.0.1:4318/v1/traces --tracing.sample-ppm 1000000 --tracing.batch-size 1
```

In another terminal, replace the socket path with the printed private path:

```sh
target/formation-flow-observability/debug/orishuctl --host /absolute/private/path/api.sock --output json --timeout 2s cluster info
```

Stop the worker with Ctrl-C and allow its normal drain to finish, then stop the
collector. Validate the printed trace-file path without starting any process:

```sh
python3 scripts/check-worker-otelcol.py --inspect-traces /absolute/private/path/traces.jsonl
```

One request should yield one `orishu.client.request` span in this isolated
100%-sampling walkthrough. Inspect the private JSONL file locally for its
`traceId`, `spanId` and `orishu.outcome`. The IDs identify telemetry, not an
operation receipt. There is currently no propagated parent, command ID or
trace-correlated worker log; timestamps/counts are not causal proof for several
concurrent operations. Normal sampling defaults to 1000 ppm; the 100% override
is for this short test, not a fleet recommendation.

## Limits and safe first checks

The [pinned HTTP configuration](https://github.com/open-telemetry/opentelemetry-collector/blob/v0.160.0/config/confighttp/server.go)
caps request bodies at 1 MiB, disables compressed input and bounds header/body
reads and response writes. The example does not establish hostile-client
connection-capacity or collector overload acceptance. Loopback is not identity
authentication: other local processes can submit fabricated telemetry. Do not
change it to a wildcard/remote listener without a tested TLS/authorization and
deployment policy. The [mTLS overlay](testing-worker-otelcol-mtls.md) now has
loopback receiver-security evidence, not cross-host qualification. Worker operator, monitoring and peer
credentials are never given to this collector.

The first pipeline processor applies a 128 MiB heap target with a 32 MiB spike
allowance; `GOMEMLIMIT=100MiB` complements it. This is not a hard RSS ceiling or
an OS resource limit; the
[memory limiter](https://github.com/open-telemetry/opentelemetry-collector/blob/v0.160.0/processor/memorylimiterprocessor/README.md)
can refuse data after allocation. The example adds no batch processor or retry
queue. Orishu deliberately sheds failed exports instead of retrying domain
operations or blocking owner progress.

The [file exporter](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/v0.160.0/exporter/fileexporter/README.md)
is alpha and its JSON schema is not a stable storage contract. Rotation is set
to 1 MiB, two backups and one day; it bounds diagnostic retention, not durable
delivery. Its rotation behavior, disk-full handling and memory-pressure
behavior are configured but not exercised by this short walkthrough.

If traces are missing, verify the actual build features, runtime enablement,
sampling and endpoint first. Check collector configuration/startup and private
file permissions, then the worker's
[live delivery/loss counters](orishu-observability.md#live-trace-delivery-and-loss-counters)
when metrics are enabled. A successful export response is not a durable file
receipt, and a failed export can have an uncertain receiver outcome. Preserve
that distinction; do not restart/rejoin a worker or replay a mutation merely
to regenerate telemetry. The automated outage test uses real collector
shutdown; it does not prove queue saturation or a stalled filesystem.

Remote/TLS collector deployment, cross-peer propagation, log correlation,
dashboards, service/container/release qualification and representative overhead
remain open under [P-OBS-DOCS](tasks/document-worker-observability.md) and
[P-OBSERVABILITY](tasks/implement-worker-observability.md). This local receipt
recipe is not combined M4 acceptance.
