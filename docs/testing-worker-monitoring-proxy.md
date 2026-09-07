# Secure worker monitoring through an mTLS proxy

Status: **source-build Linux recipe tested; not a supported release package**

This implements ADR 0017's secured-proxy option. The worker still binds
diagnostics to loopback only. A separate Nginx process terminates TLS and
requires a certificate from a dedicated monitoring CA for every diagnostics
request, including probes. There is no unauthenticated remote probe exemption
in this recipe and no worker-native remote listener change.

## Deployment

1. Build `orishu-worker` with `--features observability`. Enable
   `--observability.enabled true --observability.bind 127.0.0.1:9168`.
   Keep its client API and peer listeners independently configured.
2. Provision a server certificate with the management DNS name in its SAN and
   a monitoring-client certificate with client-auth usage. Use a dedicated
   monitoring CA, not the peer admission or operator credential authority.
   Every unrevoked certificate trusted by this proxy CA has diagnostics access;
   CA issuance is therefore an authorization decision. Keep CA signing keys
   offline, never on the proxy or scraper. Protect leaf private keys with
   owner-only permissions and provision expiry/revocation procedures through
   your PKI. This recipe does not implement certificate rotation or CRL delivery.
3. Copy [the standalone Nginx configuration](../etc/nginx-worker-monitoring.conf.example)
   to a dedicated service prefix as `nginx.conf`. Replace the documentation-only
   management IP `192.0.2.10` and `worker-mgmt.example` with your actual address
   and DNS name. Place the server chain/key and monitoring CA under `certs/`
   as named in the configuration. Choose a private writable runtime prefix for
   the PID and temporary paths, readable only by the dedicated proxy account.
   Do not run this example inside an unrelated public web-server configuration.
4. Validate and run it as that unprivileged account (port 9443 needs no root):

   ```sh
   nginx -e stderr -p /path/to/private/monitoring/ -c nginx.conf -t
   nginx -e stderr -p /path/to/private/monitoring/ -c nginx.conf
   ```

5. Adapt [the Prometheus mTLS example](../etc/prometheus-worker-mtls.yml): set
   the target and matching TLS server name, the server CA, and the scraper's
   monitoring certificate/key paths. Validate with `promtool check config`.
   Keep hostname verification enabled. The scrape uses HTTPS with redirects
   disabled. Probe clients need the same monitoring trust/credentials, for example:

   ```sh
   curl --fail --cacert certs/server-ca.crt --cert certs/monitor.crt \
     --key certs/monitor.key https://worker-mgmt.example:9443/readyz
   ```

The proxy and worker must share a trusted host/network namespace for the
loopback hop. In separate containers, `127.0.0.1` means each container itself;
use an explicitly shared network namespace or a separately reviewed secured
transport, not a wildcard worker bind. Restrict the management port with host
and network policy. Loopback diagnostics remain accessible to local processes;
mTLS on the proxy does not remove that accepted local trust boundary.

Only exact, query-free `/metrics`, `/livez`, `/readyz` and `/startupz` GETs are
forwarded. Other paths return 404 and other methods 405. TLS client verification
protects all forwarded routes. Client request headers and bodies are not
forwarded, so bearer tokens and claimed forwarded identity cannot authorize
the loopback hop. No cluster/operator path is configured on this proxy.
Responses carry `Cache-Control: no-store`; access logging is off to avoid
building an accidental identity/history store.

The example permits 64 Nginx connections (upstream sockets count too) and 16
concurrent requests per server, with 429 on request-limit exhaustion. It sets
header/body/write/keepalive and upstream timeouts and bounded buffers, with no
proxy temporary response files or retries. These proxy timeouts are not a
measured end-to-end latency SLO or proof of absolute slow-client expiry. The
worker retains its independently tested diagnostics limits. The active-request
limit and downstream writes now have pressure/recovery evidence below. Full
connection saturation and all timeout permutations are not established.

## Executable acceptance

Tested tools: Ubuntu Nginx package `1.28.0-6ubuntu1.8` (reports `nginx/1.28.0`,
compiled with `--with-debug` for the write-pressure observer),
Prometheus/promtool `3.5.0`, Python 3.13 and OpenSSL-generated temporary test PKI.
Tool versions identify reproduction evidence, not a promise that these versions
remain suitable for deployment indefinitely; use maintained security updates.
The harness downloads and installs nothing and binds only local test sockets.

```sh
make test-worker-monitoring-proxy \
  NGINX=/path/to/nginx \
  PROMTOOL=/path/to/promtool \
  PROMETHEUS=/path/to/prometheus
```

The [harness](../scripts/check-worker-monitoring-proxy.py) starts a real worker,
the adapted checked-in proxy and a real Prometheus server. It verifies trusted
mTLS scrapes/probes, absent/wrong-CA client refusal, route/method isolation,
operator-token refusal at the proxy, monitoring-fingerprint refusal for CLI
mutation, successful authorized lock/unlock, actual Prometheus receipt and
worker readiness/identity after proxy shutdown. It parses real exposition with
promtool and checks that the operator secret is absent. Temporary keys and
processes are cleaned up on success or failure with bounded shutdown. Worker
and CLI executables are copied into the private fixture directory before
startup, so concurrent rebuilds cannot swap the tested command implementation.

Negative TLS tests also require client-side refusal of an untrusted server CA
and mismatched server hostname. After the worker exits normally, the harness
starts the same proxy without its upstream: all four authenticated diagnostics
requests return `502` with no metric samples and `Cache-Control: no-store`.
Unauthenticated requests remain refused and operator routes remain absent.
This checks connection-refused upstream behavior, not a stalled or malicious
upstream or credential revocation/rotation.

The pressure case interposes a bounded TCP relay on the loopback upstream hop.
Sixteen authenticated `/metrics` requests reach real relay sockets and remain
unanswered. A seventeenth receives non-cacheable `429` without metric samples,
while direct worker readiness and authenticated operator lock/unlock succeed.
The fixture releases the requests before the configured read deadline, forwards
them to the actual worker and relays its unchanged response bytes. All sixteen
complete with metrics, then a fresh probe reaches the relay and succeeds,
proving capacity reuse. This is a controlled upstream hold, not a synthetic
health response or a test of a slow worker's domain decisions. It does not
establish downstream socket-backpressure deadlines or full TLS admission limits.

The expiry variant never releases the sixteen requests. The proxy's configured
read timeout produces non-cacheable `504` responses without samples, and Nginx
closes the upstream sockets. The fixture checks this within one seven-second
observation budget, with a four-second lower bound to exclude immediate setup
refusal. The original fixture-side sockets remain open while a fresh probe
reaches the relay and succeeds, so cleanup does not manufacture capacity reuse.
Authorized lock/unlock is checked during the hold with distinct operation IDs.
This covers an upstream sending no response bytes; it is not an absolute
deadline against a trickling upstream or a slow downstream reader.

### Pre-HTTP pressure and expiry

The same harness now holds sixteen TLS handshakes after receiving actual server
handshake bytes, without sending the remaining client flight, and sixteen
completed TLS connections with valid monitoring certificates but incomplete
HTTP headers. Server bytes/full handshakes establish accepted connections, not
merely TCP backlog. Eight incomplete heads stay silent; eight receive additional
header bytes at two, two-and-a-half and three seconds. Header progress must not
extend the configured whole-header timeout.

All thirty-two connections must close from the server side within one
seven-second observation budget, with at least four seconds from each
connection's creation. EOF observers run concurrently before expiry so an early
refusal cannot be hidden behind another connection's timeout. Observations are
byte-bounded; timeout is a failed assertion, not proof of closure. Before expiry,
real proxy scrapes/probes, direct worker readiness and authenticated CLI
lock/unlock succeed. Fresh proxy requests succeed afterward while all original
client-side handles remain open; neither cleanup nor restarting Nginx produces
the result. Exact worker state/identity checks remain in the full journey.

These cases use the existing `client_header_timeout 5s` without increasing it.
The pinned [Nginx HTTP connection/handshake source](https://github.com/nginx/nginx/blob/release-1.28.0/src/http/ngx_http_request.c)
uses this budget before and during an incomplete TLS handshake. Nginx's
[request connection limit](https://nginx.org/en/docs/http/ngx_http_limit_conn_module.html)
starts counting after a complete request header, so `limit_conn monitoring 16`
does **not** limit pre-HTTP clients to sixteen. The process-wide
[worker connection budget](https://nginx.org/en/docs/ngx_core_module.html#worker_connections)
also includes upstream connections. The recipe sets it to 64; this fixture
exercises thirty-two pending clients with spare capacity, not exhaustion of
that complete pool, denial-of-service resistance or a fleet-scale SLO.

`make test-worker-monitoring-proxy` also runs
`python3 scripts/test_worker_monitoring_proxy.py`: real socket-pair controls
check byte limits, EOF, expired deadlines and refusal to interpret silence as
successful closure. The [conformance record](tasks/cluster-formation-conformance.md#monitoring-proxy-pre-http-pressure-and-expiry--2026-09-07)
records exact commands and tool/build scope. These incoming-handshake/header
cases do not substitute for the separate downstream evidence below.

## Downstream response backpressure and expiry

The harness uses actual worker `/metrics` responses over certificate-verified
TLS. Its client requests a 1 KiB receive buffer and 512-byte TCP maximum segment
size before connecting, then sends a finite pipeline of 100 GETs without
reading responses. These are Linux client-side pressure controls, not worker
or proxy deployment settings. A separate connection with the same settings
successfully reads three real metrics responses first.

The proxy runs the checked-in configuration with debug logging enabled only in
this fixture. A continuously drained pipe retains numeric per-connection
evidence, not raw logs, request fields or credentials. The observer bounds each
line to 8 KiB, total input to 8 MiB and its inventory to 64 connections; exceeding
a bound fails the test. Debug output must not be enabled in the deployment
recipe to obtain normal monitoring.

Evidence correlates the client's source port to the proxy connection. A failed
`SSL_write` immediately followed on that connection by `SSL_ERROR_WANT_WRITE`
after a metrics request establishes actual response-write pressure. Handshake
or read-side retry events do not qualify. This interpretation follows the
[pinned Nginx TLS write adapter](https://github.com/nginx/nginx/blob/release-1.28.0/src/event/ngx_event_openssl.c).
The fixture requires positive progress, fewer than all 100 requests processed,
less than 1 MiB accepted by TLS writes, and unchanged request/byte totals during
a 100 ms observation and through expiry. These byte totals exclude TLS record
overhead and are not a whole-process memory measurement.

With the normal sixteen-request capacity, fresh authenticated metrics/probes
and direct worker readiness succeed while blocked, alongside real operator
lock/unlock within four seconds. A second fixture changes only the active
request capacity to one: fresh proxy requests receive 429 while the stalled
request holds that slot; direct worker probes and operator control still work.
Both require a response-send timeout and proxy connection-close event within
seven seconds from sending the pipeline, no earlier than four seconds after
observed backpressure. Fresh authenticated metrics/probes then succeed with
the original client still open and unread. In the one-slot variant, that proves
reuse of the occupied request slot without cleanup or a proxy restart.

The five-second [Nginx send timeout](https://nginx.org/en/docs/http/ngx_http_core_module.html#send_timeout)
bounds inactivity between writes, not total response age under a continuously
progressing reader. The result covers a stalled reader, not arbitrary trickle
patterns, full connection-pool exhaustion, whole-process memory limits or an
end-to-end production SLO. No server timeout, socket-buffer setting, metric
catalogue, upstream response or TLS policy is changed to create backpressure.
See the [conformance record](tasks/cluster-formation-conformance.md#monitoring-proxy-downstream-backpressure-and-expiry--2026-09-07)
for actual commands, measurements and limitations.

This is a same-host source-build test of the remote security boundary, not a
cross-host firewall, service-manager, container, package or fleet certification.
It does not close tracing, enabled/disabled overhead, multi-worker outage
equivalence or the full P-OBSERVABILITY/P-OBS-DOCS acceptance matrix.

If scrapes fail, separate server trust/name errors, rejected client credentials,
proxy reachability and worker readiness. Inspect the worker through its real
operator interface; proxy failure does not imply peer death or authorize a
restart, new join or membership removal. A proxy-generated `502` means no
usable upstream response; it is not the worker's `/readyz` `503` domain health
response. A proxy-generated `504` is an upstream timeout, likewise not a
worker health decision. Do not convert these into cached `200` responses or route requests
to the worker's administrative API as a fallback.

References: Nginx [TLS client verification](https://nginx.org/en/docs/http/ngx_http_ssl_module.html#ssl_verify_client),
[proxy request handling](https://nginx.org/en/docs/http/ngx_http_proxy_module.html),
and Prometheus [TLS configuration](https://prometheus.io/docs/prometheus/latest/configuration/configuration/#tls_config).
