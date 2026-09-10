# Authenticate worker trace export to the Collector with mTLS

Status: **verified source-built Linux receiver-security recipe on loopback**

This extends the [pinned local Collector walkthrough](testing-worker-otelcol.md)
with the [mTLS overlay](../etc/otelcol-worker-mtls.yml). It uses official
`otelcol 0.160.0`, real worker/CLI processes and dedicated collector credentials.
It establishes receiver TLS/client-certificate enforcement and worker server
verification, not cross-host deployment qualification or cross-peer tracing.

## Run the security journey

Use the collector archive/executable checksums from the local guide. The added
fixture needs OpenSSL; the recorded Linux run used OpenSSL 3.5.3 with Python 3.13.
All generated keys and certificates are private, one-day test material in a
temporary directory and are removed when the check exits. No CA or credential
is installed into the system, worker peer trust or operator configuration.

```sh
make test-worker-otelcol-mtls OTELCOL=/absolute/path/to/otelcol
```

Like the HTTP target, it defaults to `target/worker-otelcol` and builds both
optional capabilities. To use an already built, idle isolated directory:

```sh
python3 scripts/test_worker_otelcol.py
python3 scripts/check-worker-otelcol.py --mtls --otelcol /absolute/path/to/otelcol --worker target/formation-flow-observability/debug/orishu-worker --ctl target/formation-flow-observability/debug/orishuctl
```

The full Make target can use `WORKER_OTELCOL_TARGET_DIR=target/formation-flow-observability`
instead. Run target builds sequentially; do not replace live fixture binaries.
The existing 120-second whole-journey budget includes certificate generation.
Per-command, HTTP, observation, receipt-size and process-cleanup limits remain
those in the local guide. Tests do not request a wildcard listener or contact
an external collector.

## What is tested

| Case | Required evidence |
| --- | --- |
| Missing or malformed collector client-CA file | Collector startup fails with the applicable file/CA-loader diagnostic; no surviving receiver listener or span receipt |
| Collector server key does not match its certificate | Startup fails for the key mismatch, not an unrelated configuration error |
| Plain HTTP sent to the TLS endpoint | HTTP 400; plaintext cannot enter the trace pipeline |
| TLS client supplies no certificate | Explicit TLS certificate-required alert; a refused TCP connection is not sufficient evidence |
| Client offers only TLS 1.2 | Explicit protocol-version alert; the recipe requires TLS 1.3 |
| Valid mTLS, runtime-disabled or zero-sampled worker | No received spans after clean shutdown; feature/counter absence versus measured zero stays distinct |
| Valid mTLS, enabled worker | Real file receipts with the expected local span identities/outcomes; collector stop/restart preserves authenticated lock/unlock and readiness and yields fresh receipts |
| Worker supplies no client certificate | Worker starts and remains ready; three client operations produce failed exports and no accepted/received spans |
| Worker supplies a certificate from an unrelated client CA | Same bounded export failure and domain-control continuity; no fallback to operator or peer credentials |
| Worker trusts an unrelated server CA | No received worker spans; normal authenticated control remains usable |
| Trusted server certificate has the wrong SAN for the worker URL | Worker refuses delivery without bypassing endpoint identity verification |

Every worker-failure case keeps an independently certificate-verified receiver
control working. In the name-mismatch case that control verifies the test
server's actual `other.invalid` identity while still connecting to loopback;
the worker uses its unchanged `https://127.0.0.1:port/v1/traces` URL. Neither
client disables certificate/hostname verification. Thus a stopped collector
cannot masquerade as successful certificate-refusal evidence.

Received files exclude seeded operator tokens, labels, operation names, PEM
certificates/private keys and their continuous base64 payloads. The secure
happy path retains the HTTP walkthrough's three initial spans, two failed
exports during actual collector shutdown, and two new spans after recovery.
Worker shutdown precedes receiver shutdown and the client socket is cleaned up.
The receipt validator and TLS-alert/credential-selection negative controls are
part of the target; a successful unauthenticated HTTP result must fail the
access-policy assertion.

## Provision dedicated material

For a real installation, use your collector PKI rather than the test helper's
throwaway CA generator. Keep the collector's server trust and exporter-client
trust separate from each other and from Orishu peer, operator and monitoring
credentials. These are the two verification directions:

| Reader | File | Meaning |
| --- | --- | --- |
| Collector | `collector-tls/server.crt` | Server leaf-first chain with server-auth purpose and a SAN matching the worker's configured collector host/IP |
| Collector | `collector-tls/server.key` | Matching private server key, readable only by its service identity/root |
| Collector | `collector-tls/client-ca.crt` | Dedicated CA roots allowed to issue trace-export client certificates |
| Worker | Explicit `--tracing.ca-file` | Collector server trust roots; not the client-issuing CA or peer trust |
| Worker | Explicit `--tracing.client-cert-file` and `--tracing.client-key-file` | Dedicated client-auth leaf-first chain and matching private exporter key |

The collector policy accepts valid client-auth certificates chaining to its
configured client CA; it is not a per-worker SAN allowlist. Do not put a broad
corporate/peer CA in that file and assume only Orishu exporters gain access.
A collector client can submit telemetry, not mutate Orishu. Its certificate
does not turn submitted `service.name`, trace IDs or attributes into trusted
formation identity or scientific provenance.

This recipe uses mTLS, not a bearer-auth extension. Do not give it a join,
operator or monitoring token, and do not assume an `Authorization` header
adds another enforced policy. A bearer-authorized receiver needs its own
configuration and tests.

The overlay's credential paths are **relative to the collector process's
working directory**, not the YAML file. Use a new private directory containing
`collector-tls/`, provision the files there, and start from that directory.
Keep the directory `0700` and keys `0600`; do not copy a CA signing key there.
For worker files, follow the stricter
[bounded credential-file contract](orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable),
including ownership, single-link files and no symlink traversal. Mounted-secret
or rotation procedures are not established by this recipe.

The [pinned Collector TLS implementation](https://github.com/open-telemetry/opentelemetry-collector/blob/v0.160.0/config/configtls/configtls.go)
enables required client authentication when `client_ca_file` is nonempty.
The checked-in overlay deliberately uses a fixed nonempty filename; missing or
invalid contents fail startup. Do not replace it with an optional environment
variable that can expand to an empty value. `validate` checks configuration;
real startup and the no-certificate rejection check are still required to
establish the effective access policy.

## Start the receiver and worker

After provisioning the files, in the collector's private working directory:

```sh
umask 077
export ORISHU_OTEL_TRACE_FILE="$PWD/traces.jsonl"
export GOMEMLIMIT=100MiB
/absolute/path/to/otelcol validate --config /absolute/repo/etc/otelcol-worker-local.yml --config /absolute/repo/etc/otelcol-worker-mtls.yml
/absolute/path/to/otelcol --config /absolute/repo/etc/otelcol-worker-local.yml --config /absolute/repo/etc/otelcol-worker-mtls.yml
```

The second file overlays TLS onto the same `otlp` HTTP receiver. It does not
add an unsecured fallback listener. The bind stays `127.0.0.1:4318`; ensure the
server certificate has that IP SAN for this local workflow. The file exporter
retains the base recipe's disposable, rotating output and its limitations.

In a separate worker terminal, using private worker state and provisioned
absolute credential paths:

```sh
target/formation-flow-observability/debug/orishu-worker --state-dir /absolute/private/worker-state --listen.clients /absolute/private/worker.sock --tracing.enabled true --tracing.endpoint https://127.0.0.1:4318/v1/traces --tracing.ca-file /absolute/private/server-ca.crt --tracing.client-cert-file /absolute/private/exporter.crt --tracing.client-key-file /absolute/private/exporter.key --tracing.sample-ppm 1000000 --tracing.batch-size 1
```

Use the ordinary CLI summary request and post-shutdown `--inspect-traces`
command in the local guide. The 100% sample rate is a short walkthrough
override, not a fleet recommendation. Never disable trust/name verification to
make a failed export succeed; inspect endpoint/SAN, issuer, expiry, purposes,
chain order and file permissions while keeping credentials private.

## Remaining deployment gates

Before using a different host, explicitly configure its management bind,
endpoint/SAN, firewall/access policy and service isolation, then validate that
deployment. This loopback mTLS evidence does not qualify cross-host networking,
certificate rotation/revocation, bearer authorization, hostile-handshake
saturation, collector disk/memory pressure or package/container releases.
There is no certificate reload or worker restart promise here. Collector
failure remains a telemetry failure, not a reason to restart/rejoin a worker.

The separate [local formation walkthrough](testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
now verifies profile-5 admission/log correlation using HTTP. It does not make
this root-span mTLS recipe a cross-peer receipt test. Final selected-deployment
reconciliation and representative overhead remain tracked by
[P-OBSERVABILITY](tasks/implement-worker-observability.md) and
[P-OBS-DOCS](tasks/document-worker-observability.md); neither recipe closes M4.
