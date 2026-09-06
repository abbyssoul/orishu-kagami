# orishu Client Protocol

This document defines the protocol for communication between the clients (such as CLI and results viewers) and the `orishu-worker` daemons. The protocol is designed to be simple, minimal, and secure, because it is likely to be carried over the networks outside the cluster.

For the cluster-internal peer-to-peer protocol, see [protocol-p2p.md](./protocol-p2p.md). This protocol is the client-server half of `orishu`'s two-level architecture: the cluster behind it self-organizes as a peer-to-peer system, but this protocol presents it to a client as a single service producing a stream of simulated states — see [architecture.md](./architecture.md#client-server-from-outside-peer-to-peer-inside).


## Transport

### Implemented formation subset

The worker currently serves `GET /api/v1/cluster` over HTTP/1.1 or HTTP/2 on
a Unix socket, and HTTP/1.1 or HTTP/2 over explicitly configured TLS/TCP.
Plaintext TCP configuration is rejected. Client HTTP/3 remains planned, not
an implemented fallback. Local reads require no bearer token; remote reads
require the worker-local operator bearer credential. Join and monitoring
credentials do not authorize this route. The CLI can explicitly load the
worker-local operator credential through `--operator-token-file` or the path
in `ORISHU_OPERATOR_TOKEN_FILE`; see the
[operator authentication instructions](../apps/orishu-ctl/README.md#operator-authentication).
`POST /api/v1/cluster/lock` additionally implements identified lock/unlock
intents with the worker-local operator credential, including on Unix sockets.
Identified join submission/status and voluntary leave are also implemented
through the membership resources described below. Responses are CBOR with
`Cache-Control: no-store`; successful summaries identify the source worker
in `X-Node-Id`.

The current response is `ApiResponse::Ok` containing
`ResponseData::ClusterSummary`, not the imported authored-manifest-shaped
`ClusterManifest`. The shared client and `orishuctl cluster info` consume the
same typed resource. Its version-1 fields are:

| Field | Meaning |
| --- | --- |
| `schemaVersion` | `1`; other versions are rejected by the client decoder |
| `formationId`, `clusterName` | Immutable formation identity and reusable label, respectively |
| `sourceNodeId` | Assigned identity of the worker whose model was read |
| `memberCount`, `aliveCount` | Retained member records and locally observed live members |
| `membershipLocked` | Locally held replicated lock, not a global fence |
| `participation` | `standalone`, `joining`, `joinUnresolved`, `catchingUp`, `joined`, `ejected`, or `stopping`; never inferred from member count |
| `introducerReady` | Whether the local worker can enforce every admission gate |
| `workload` | `"none"` in this no-compute PoC |
| `view` | `"localAtRequest"`; a current local read, not proof of global convergence |

Startup currently yields `standalone`, one alive member and
`introducerReady: false`: the standalone owner runs, but peer session/effect
integration is still pending. Reads use its published local projection; a
stopped owner yields `503`/`OwnerUnavailable`, never a cached success described
as fresh. Listing lifecycle variants does not claim those transitions
are implemented. Other membership mutations
remain in the formation task. Unmatched routes do not
report successful placeholder operations.

`joinUnresolved` means an emitted admission attempt exhausted its core retries
without validated local adoption. Remote insertion may already have happened;
the remaining source formation does not prove otherwise. The owner refuses a
new begin-join, drops transient secret/send state and does not invent rollback
or success. If no JoinReq was ever submitted to transport, exhaustion can
return to `standalone`. This conservative distinction precedes the still-pending
identified operation-status/recovery API; operators must not infer that an
unresolved target membership entry can safely be erased or that restarting is
outcome recovery. Clients must recognize the new lifecycle enum value.

### Formation client ingress limits

The formation PoC's HTTP/1 listener contract is 64 accepted connections per
configured listener, including connections with no authenticated request yet.
At capacity, acceptance pauses at the bounded OS listen backlog; excess clients
are not promised an HTTP overload response before admission. Existing accepted
connections and the separate peer listener can continue to make progress.
Closing or expiring an accepted connection releases capacity. This is not a
reserved remote administrator lane or a guarantee against sustained saturation.

HTTP/1 request heads have a five-second read deadline, an 8 KiB buffer limit and
at most 32 headers. These checks precede route authentication. The existing
16-slot mutation and 16-slot inspection budgets and five-second handler
deadlines remain separate: handler capacity alone cannot bound partial heads.
These fixed PoC bounds do not add runtime configuration or enable remote client
modes beyond the documented Unix and TLS/TCP subset. A transport write that
remains pending for five seconds closes its
connection. This is a write-stall deadline, not a total response deadline or
minimum-throughput guarantee for a reader that continues making progress.
The shared Unix/TLS client listener also closes a connection after ten seconds
without a successful transport read or write, for both HTTP/1.1 and HTTP/2.
This reclaims silent incomplete frames/header blocks and silent window-blocked
responses even when no socket write is pending. Idle keep-alive clients must
reconnect. This is connection inactivity, not a per-stream or absolute header
deadline: traffic on another stream, PINGs or trickled bytes can keep the
connection active. Such traffic does not reset the separate five-second
HTTP/1 header or handler deadlines. Future long-lived delivery must review
this limit explicitly rather than silently disabling it for formation routes.
The existing response codec caps each serialized response at 1 MiB; the HTTP/1
buffer is separately capped at 8 KiB. Pagination and handler budgets bound
response construction; pipelining must not queue the entire response batch
behind a blocked writer. Validation is tracked in the formation conformance
ledger. Future streaming/observation routes must define their own delivery
contract before relying on, relaxing or replacing these formation-PoC limits.

HTTP/2 additionally has an explicit 16-concurrent-stream limit per connection,
8 KiB decoded header-list limit, 4 KiB HPACK table, 16 KiB maximum frame size,
65,535-byte initial stream and connection receive windows with adaptive window
growth disabled, and a 16 KiB per-stream send-buffer setting. These do not
replace the process-wide handler budgets or the per-listener connection cap.
A seventeenth open stream is refused; cancelling one permits replacement.
Oversized decoded request headers receive a terminal 431 response, and valid
subsequent streams can still use the connection.

With these settings, the pinned decoder permits at most five non-final
CONTINUATION frames per header block; a sixth non-final frame closes the
connection with `ENHANCE_YOUR_CALM`. A final sixth continuation is permitted.
Continuations must retain the initial HEADERS stream ID; a mismatch closes the
connection with `PROTOCOL_ERROR`. A real-worker test verifies both refusals and
the valid final-sixth-frame control. This bounds continuation frame work, not
elapsed time for an unfinished header block or a partially received frame.

Real-process Unix HTTP/2 tests also verify response DATA obeys zero per-stream
credit and exhausted connection credit, explicit window updates resume the
selected responses, cancellation returns stream capacity, and independent
control and shutdown progress with window-blocked responses. This is response
flow-control/recovery evidence, not inbound flow-control violation coverage or
incomplete HEADERS/CONTINUATION deadline evidence. A stream waiting for
HTTP/2 window credit need not have a pending socket write, so the five-second
transport write-stall timeout is not evidence of a per-stream send deadline.
HTTP/1 header timing and slow-reader tests do not establish those HTTP/2 cases.

Response-write failure cannot roll back an accepted mutation. A client missing
a complete validated receipt must use the supported operation/status/replay
path rather than infer that the command was rejected.

The Rust HTTP client preserves the received HTTP status in `ClientError::ApiError`
for non-success responses carrying a CBOR error envelope. It does not treat a
success envelope carried by a non-success HTTP status as an accepted outcome.
This corrects the earlier loss of error status to zero without changing wire
schemas. Raw HTTP-caller consumers must handle these responses as errors rather
than successful returns containing an error envelope.

### Client listener shutdown

The worker's owner supervisor is the single authority that stops client
listeners after membership-owner termination. Signal handling requests runtime
shutdown; it does not race a second listener-drain timeout against the
supervisor. Client connections receive a one-second graceful drain budget,
then remaining connections are cancelled. This budget begins after owner
termination, not at signal receipt; the process regression separately requires
exit within three seconds of SIGTERM while incomplete client requests remain
open. These are local lifecycle bounds, not cluster convergence deadlines.

A disconnected client must still treat an emitted mutation without a validated
receipt as potentially accepted and use the documented operation/replay path.
Listener cancellation does not undo an accepted membership transition.

### Implemented join-material retrieval

`GET /api/v1/cluster/token` requires exactly one worker operator bearer
credential on every transport, including the same-user Unix socket. Neither
join nor monitoring credentials authorize retrieval. It returns version-1
`JoinMaterial`, replacing the imported bare `JoinToken` retrieval response:
`schemaVersion`, `formationId`, `introducerNodeId`, `introducerFingerprint`,
`peerEndpoints`, `introducerReady`, and secret `token`. The token is exactly
64 lowercase hexadecimal characters and redacted from Rust debug formatting;
serialization deliberately includes it only on this privileged export path.
The owner reads identity, endpoints and token in one serialized turn. It never
places secret material in the membership core, gossip or ordinary projections.

Responses use CBOR and no-store. The route shares the 16-request membership-read
budget and five-second owner deadline; no token or peer endpoint, overload,
owner loss or timeout returns `503`/`JoinMaterialUnavailable`. Missing/wrong or
duplicate credentials return `401` without secret output. Config-free workers
without an explicit peer endpoint cannot provide usable join material.

Executable material reports `introducerReady: true` only for an explicitly
enabled, initialized standalone or fully caught-up joined introducer with a
peer endpoint and installed formation token. Incomplete adoption/catch-up still
withholds readiness and target token export;
retrieving a token does not itself enable admission. No expiration
timestamp is fabricated: this PoC token belongs to the current formation and
is discarded on adoption/leave/restart. Rotation remains unsupported. Operators
must obtain material over the protected client path and transfer/store it
privately. The join adapter must verify the exact certificate pin and
formation before sending the secret to any endpoint candidate; a redirect
cannot relax that requirement. Retrieval alone is not evidence of that adapter.

The prepared version-1 `JoinRequest` input has `schemaVersion`, `operationId`,
`formationId` (the expected current source worker formation), and `material`
(the complete target `JoinMaterial`). It does not imply automatic leave.
The CLI now validates private JSON material, preserving the pin instead of
lowering it to the imported token-only `JoinIntent`. The current literal-address
profile accepts one to eight endpoints of at most 128 bytes each, with unicast
IPs and nonzero ports; DNS and redirects are not enabled by this input path.
Readiness is advisory and admission must still re-check gates. The identified
request executor, operation-status/retry contract and HTTP handler are not yet
connected; CLI execution explicitly rejects before transmission. The imported
`JoinIntent`/`JoinRequestAccepted` client methods are not evidence of support.

The bounded local operation tracker now specifies `JoinOperation` version 2:
`operationId`, historical `sourceFormationId`/`sourceNodeId`, `targetFormationId`
and tagged `state`, plus required `recoveryReference` (object or explicit null).
The reference contains `attemptId`, `applicantFingerprint`, `introducerNodeId`
and `introducerFingerprint`. The owner binds it after pinned handshake during
the serialized turn that starts admission, before another status request can
observe that turn. The applicant fingerprint comes from the worker's local
certificate, not caller-supplied identity. The reference remains immutable
across retry, adoption and retained historical outcomes. It contains no token;
the attempt nonce is correlation, not an assigned member ID or authorization.
Null means no reference was bound and does not independently prove remote
non-admission. Missing fields and older operation versions are rejected.
JoinRequest and JoinMaterial remain version 1, and this resource change does
not change peer ALPN; worker and operator clients must be rebuilt together.
The reference alone is not a safe-retry procedure. Process restart still loses
operation history.

#### Issuer admission inspection

`POST /api/v1/membership/admission-inspections` is an authenticated, read-only
query of the directly addressed issuer, not an admission command or persisted
resource. Its version-1 CBOR request contains `schemaVersion`, `formationId`
and the exact `reference` from version-2 join status. It requires the operator
credential even on Unix; join and monitoring credentials do not authorize it.
No query parameters, content encoding or `If-Match` are accepted. The request
is bounded to 4 KiB, shares the 16-request inspection budget, and has a
five-second handler deadline. Response uses the normal CBOR envelope/no-store
policy, echoes the request, and names the actual `sourceFormationId` and
`sourceNodeId`; the report's own `schemaVersion` is 1.

| Tagged `outcome.kind` | Meaning and safe interpretation |
| --- | --- |
| `wrongIssuer` | Formation, node or certificate pin differs from the reference. Stop; this process cannot establish that attempt's outcome. |
| `recordUnavailable` | No retained accepted record for the fingerprint/attempt pair. This is unknown, not proof of refusal, non-insertion or absent exclusion. |
| `currentMember` with `nodeId` | The retained assignment passes current local membership identity/liveness/exclusion checks. This does not prove current token, source-network authorization, catch-up readiness or successful replay. |
| `retiredOrRestricted` with `nodeId` | The retained assignment no longer passes those checks. Do not resurrect it or infer permission for a new attempt. |

The serialized owner performs the lookup against its current formation-lifetime
ledger and membership. It does not allocate an identity, renew a retry budget,
disclose a digest/token, clear restrictions or modify membership. Reports are
local-at-request facts, not cluster-wide fences; a stale report never authorizes
admission. Ledger capacity does not prevent lookup of a retained record.
HTTP authentication/decoding/overload/timeout failures produce no negative
admission evidence. The [operator recovery procedure](cluster-admission-recovery.md)
uses bounded polling and mandatory stop conditions; its complete deliberate
process-fault acceptance remains outstanding. Clients reject non-`wrongIssuer`
reports whose source formation/node contradict the referenced issuer, as well
as any report whose echoed request differs from the query.

#### Join operation phases and retention

JoinOperation phases are `connecting`, `admitting`, `catchingUp`,
`joined`, `failedBeforeAdmission`, `unresolved`, and `catchUpFailed`; adopted
phases carry `nodeId`. Processing acceptance is not admission. Catch-up cannot
change the assigned identity, and possible admission cannot transition to a
clean pre-admission failure. Unresolved/adopted-but-failed work continues to
exclude another join pending explicit recovery.

The selected PoC history bound is 64 accepted join requests per worker process,
retained across formation changes with no eviction or persistence across restart.
Full history rejects new IDs but still serves exact retries. Matching uses a
domain-separated SHA-256 digest of canonical typed request serialization;
records retain no admission token. Same ID/different request conflicts; exact
replay is checked before current-source eligibility so adoption cannot hide its
accepted outcome. New work requires the expected source formation, standalone
participation and no active join. This tracker and DTO have unit evidence;
owner execution, cancellation supervision, authenticated status routes and
client polling are not yet connected. No public `202` route is claimed yet.

Owner integration now reserves a completion slot on the bounded control lane
before accepting preparation. New work returns a single-use shell job; exact
replay returns only status. Dropping that job, including a dropped preparation
reply, queues a guaranteed pre-admission failure rather than orphaning the
record. A completed pinned handshake returns through that reserved slot; the
owner rechecks source generation/formation and validates the ACK before starting
the core attempt. Emission advances the record to `admitting`, and validated
adoption records `catchingUp` with the assigned target node ID. Records survive
adoption and remain replayable. Explicit leave preserves an honest failure or
uncertain/adopted history while releasing old lifecycle exclusion; it is not
remote outcome recovery. The real owner/dial test and cancellation tests cover
this integration. Runtime job deadlines, HTTP routes and client polling still
remain required before public operation acceptance is enabled.

Runtime execution now owns prepared jobs independently of the requesting client
future. It retains at most four job tasks, reaps completed tasks before adding
work, uses the shared dialer, and enforces a 16-second outer job deadline around
the dialer's 15-second budget. Existing configured peer endpoints are reused;
without one, an explicit join creates and retains a client-only UDP endpoint
using the first candidate's address family. Candidate addresses must be usable
with that bound endpoint. Config-free startup still opens no peer port, and
this outgoing endpoint is not advertised as a new peer listener. Failed or
cancelled jobs drop their preparation and record pre-admission failure.
Shutdown excludes new jobs, aborts/drains owned tasks within a one-second bound,
then stops the owner and closes the retained endpoint. A runtime test checks
bad-pin failure, exact replay, unchanged membership and shutdown cleanup.
Authenticated public submission/status routes and client polling remain unwired.

### Implemented identified join routes

`POST /api/v1/membership/joins` now consumes `JoinRequest`; `GET
/api/v1/membership/joins/{operationId}` reads its retained `JoinOperation`.
Both directly target the worker and require exactly one operator bearer
credential, including on Unix sockets. Requests never relay to another worker.
POST requires CBOR with no content encoding, bounds the body at 16 KiB, and
uses the shared 16-mutation request budget. GET uses the 16-read budget.
Query parameters and `If-Match` are rejected; source preconditions belong in
the request. Each handler has a five-second deadline through owner response.

POST returns `202` for pending phases or `200` for a replayed terminal record;
GET returns `200` for a found record. Both use CBOR/no-store, current serving
`X-Node-Id`, and a `Location` naming the operation resource. Unknown status IDs
return `404`; invalid input `400`/`415`; stale source `412`; ID conflict or
ineligible participation `409`; overload/history exhaustion/unavailable outcome
`503`; handler timeout `504`/`OutcomeUnknown`. Timeout or client disconnect
does not revoke work already transferred to runtime supervision. Retry the
same identified body or query status rather than inventing a new operation.

The shared client's join method now accepts the identified DTO and checks
returned operation/source/target identities. Status reads check the operation
ID. `orishuctl join` returns the processing record, and `join-status` enables
explicit polling; neither command substitutes HTTP success for `state.phase`.
The real CLI journey covers missing credentials, bad-pin failure, polling,
unknown IDs, stale source, changed-request conflict and exact replay. The
executable now permits explicit standalone introduction and automatically
schedules bounded catch-up after adoption. `scripts/check-formation-cli.py`
exercises A admitting B, B reaching `joined` and introducing C, exact replay,
three-worker live identity convergence and lock/unlock through different entry
nodes. The runtime test separately verifies withheld readiness/target token
export while catch-up is incomplete, cancellation and automatic retry.
Interrupted-admission recovery and full lifecycle/fault conformance remain
outstanding; a successful happy path does not close formation acceptance.

### Implemented node inspection

`GET /api/v1/cluster/nodes/{nodeId}` returns `NodeInspection`, not the imported
`NodeManifest`. This changes the in-progress client DTO/API contract: clients
expecting manifests must update. The optional query is exactly
`source=indirect`; omission has the same meaning. Direct/best-effort and other
query forms return `400`/`UnsupportedSource`, not a silent source substitution.

The version-1 resource contains `schemaVersion`, `formationId`, `sourceNodeId`,
`view: localAtRequest`, `nodeId`, `workerName`, `certFingerprint`, `liveness`,
`incarnation`, record `version`, advertised `accepts` flags, `peerEndpoints`
and `clientEndpoints`. Unknown schema versions are rejected; the client checks
the returned target ID against the request. Absent connection/storage statistics
are omitted. Advertised acceptance flags do not prove introducer readiness.

Inspection performs one indexed lookup at a serialized owner boundary with no
peer IO. Remote facts may be stale; this does not prove target reachability or
global convergence. Responses use CBOR/no-store, source `X-Node-Id`, and the
same local-read/remote-operator authentication policy as summary reads. Unknown
targets return `404`/`UnknownNode`, owner unavailability/overload returns `503`,
and the five-second deadline returns `504`. At most 16 HTTP inspections await
the owner across client listeners, using its bounded 16-entry control lane.
Record fields retain membership validation bounds. Removed records absent
from membership are not fabricated from tombstones.

### Implemented membership listing

`GET /api/v1/cluster/nodes` returns version-1 `MembershipPage` containing
`formationId`, `sourceNodeId`, at most four `members` (the inspection resource
above), and optional `nextAfter`. Records are ordered by assigned node ID.
Continue with URL-encoded `formationId` and `after=nextAfter`; `after` is an
exclusive key, not an offset. A continuation requires the formation ID and a
changed formation returns `412`/`StaleFormation`. Unknown/duplicate parameters,
invalid identities and queries over 1,024 bytes return `400`. There is no
server-side cursor storage or expiry history. A missing/deleted cursor key
still starts at the next larger ID; exhaustion returns an empty terminal page.

Each page is a separate current local read, not a retained snapshot. Concurrent
changes may omit newly inserted earlier IDs or combine facts from different
read boundaries. Restart listing when a newer view is needed; never use this
operator listing as admission-state catch-up or proof of global convergence.
Paging performs O(log members + 4 records) owner work. Listing shares inspection
authentication, concurrency and deadline bounds, CBOR/no-store, and source
headers. Tombstones are not synthesized as members.

The shared client's `list` now returns inspection resources rather than imported
manifests. It collects at most 4,096 records and 16 MiB of endpoint text under a
30-second overall deadline, additionally respecting per-request timeouts.
It rejects oversized pages, non-advancing IDs/cursors and changed formation or
source identities rather than returning a partial success. Name (exact), state
(case-insensitive) and role filters apply locally after bounded collection.
`introducer`/`worker` select advertised peer/work acceptance flags, not verified
readiness; unknown state/role filters fail explicitly. `ls` emits these same
resources in JSON/YAML. These limits are the current PoC profile, not a fleet
scalability claim.

### Target transport

The client protocol is **HTTP/3** ([RFC 9114](https://www.rfc-editor.org/rfc/rfc9114)) over QUIC ([RFC 9000](https://www.rfc-editor.org/rfc/rfc9000)).

HTTP/3 was chosen because:
- It is a well-understood, widely supported application protocol — any language with an HTTP client can talk to the cluster.
- QUIC provides built-in TLS 1.3 encryption, multiplexed streams without head-of-line blocking, and fast connection establishment (0-RTT in many cases).
- The binary framing of HTTP/3 pairs naturally with a binary payload format.

For local access (same-host, same-user), the worker also listens on a Unix domain socket. Local clients connect over this socket using HTTP/2  or HTTP/1.1 as a fallback (QUIC is not applicable to UDS). Local connections are implicitly Tier 1 (see [Access tiers](#access-tiers)) and do not require credentials.

Fallback: if a client cannot negotiate HTTP/3 (e.g. UDP blocked by a firewall), the worker accepts HTTP/2 over TLS as a fallback on the same port via ALPN negotiation. The payload format and API semantics are identical regardless of HTTP version.


## Payload format

The following CBOR rules apply to the client API. The separate planned
[worker diagnostics listener](#worker-operational-diagnostics) uses the
explicit exposition and probe formats described below.

All request and response bodies use **CBOR** ([RFC 8949](https://www.rfc-editor.org/rfc/rfc8949)) as the serialization format.

- Content type: `application/cbor`
- CBOR is a binary, self-describing format derived from JSON's data model. It is compact on the wire, fast to encode/decode, and supports binary byte strings natively — useful for content hashes, certificate fingerprints, and result data.
- Clients must set `Content-Type: application/cbor` on requests with a body and should set `Accept: application/cbor`.
- If a request arrives without an `Accept` header or with `Accept: */*`, the worker responds with CBOR.

Map keys in CBOR payloads are text strings matching the field names defined in this document and in the [runtime data model](./orishu-data-model.md). Fields with `null` or default values may be omitted from the encoded map to reduce payload size.

### Diagnostic and debugging aid

For development and debugging convenience, workers may optionally support `application/json` as an alternative payload format, negotiated via the `Accept` header. When JSON is requested, the worker transcodes the CBOR response to JSON. Binary fields (hashes, fingerprints) are encoded as base64url strings in the JSON representation. This is a convenience feature; CBOR is the canonical format and all clients should support it.


## Worker operational diagnostics

Status: **loopback routes implemented behind `observability`; remote security and full conformance pending**

[ADR 0017](adr/0017-worker-operational-observability.md) specifies a dedicated,
feature-gated HTTP/1.1-compatible listener, configured independently of client
and peer admission. It serves this process, never a relayed cluster view:

| Method/path | Success | Not healthy/ready | Format |
| --- | --- | --- | --- |
| `GET /metrics` | `200` | Not a health check | Prometheus text exposition |
| `GET /livez` | `200` | `503` | Bounded plain-text status/reason |
| `GET /readyz` | `200` | `503` | Bounded plain-text status/reason |
| `GET /startupz` | `200` | `503` | Bounded plain-text status/reason |

These responses do not use CBOR, the client response envelope, or require an
assigned `X-Node-Id` during startup. Metrics use
`text/plain; version=0.0.4; charset=utf-8`; probes use plain text. Responses
disable caching. Query parameters return 400; unknown routes return 404, and
no client mutation/token routes are mounted. The current catalogue is three
health gauges, six process-lifetime owner counters, eight bounded-lane slot
gauges and six client-service instruments, all unlabelled, with
constant-size snapshots and less than 6 KiB of text. Client service accounting
is enabled only with runtime metrics; it excludes diagnostics, pre-service
transport rejection and response delivery. Handler success is not command
acceptance. See the
[metric catalogue](orishu-observability.md#implemented-local-surface) for event
semantics, restart behavior and retained values after owner closure.
The `observability.metrics` startup setting selects `/metrics` independently
of `observability.probes`, which selects all three health routes together.
Both groups default on when the listener is enabled; disabled routes return
404. Route selection grants no client/peer admission authority and does not
permit remote exposure. An enabled listener with both groups disabled is
rejected at configuration validation.
The dedicated listener allows sixteen connections and inherits the formation
HTTP header/stream/idle/write bounds above; it does not share mutation permits.
Non-loopback binds currently fail startup, including wildcard addresses.
Probe meaning is defined in the [observability guide](orishu-observability.md).

Loopback diagnostic reads may be unauthenticated. Remote metrics require the
ADR's TLS and monitoring-only access policy; these credentials cannot authorize
client API mutations. An explicit restricted-network remote probe exemption
applies only to the three minimal health routes, never `/metrics` or the
client API. Existing client access tiers remain unchanged. Trace export is
outbound OTLP, not another client-control endpoint; propagated trace context
must be separately specified and bounded before client/peer wiring lands.

## Access tiers

The system defines two access tiers, as described in the [runtime design](./orishu-runtime-design.md#access-tiers):

**Tier 1 — Local unprivileged access:**
- Client connects over a Unix domain socket or loopback interface to a node running under the same OS user.
- No credentials required.
- Permits read-only operations only.

**Tier 2 — Privileged access (local or remote):**
- Required for all state-modifying operations and for any remote access.
- Authenticated via one of:
  - **mTLS** — client presents a certificate signed by the cluster's operator trust root. The certificate subject or extension carries the operator identity.
  - **Operator token** — a signed token presented in the `Authorization` header: `Authorization: Bearer <token>`.

A worker rejects a request with `401 Unauthorized` if credentials are missing when required, or `403 Forbidden` if credentials are valid but insufficient for the requested operation.


## Response headers

Every response from a worker includes the following header:

- **`X-Node-Id: <string>`** — the node ID of the worker that processed the request.

Because clients may be load-balanced across different nodes in the cluster (via DNS round-robin, a reverse proxy, or multi-address resolution), the responding node is not always predictable. This header lets the client identify which node actually handled any given request without requiring node identity to be repeated inside every response body.

Clients that need to correlate a response with a specific node (e.g. for diagnostics, retry targeting, or auditing) should read this header rather than relying on the address they connected to.

Additionally, responses for immutable resources (checkpoints and results) include:

- **`ETag: "<opaque>"`** — a strong validator derived from the resource's content hash. Since checkpoints and results are immutable once recorded, the ETag never changes for a given resource ID. Clients may use `If-None-Match` on subsequent requests to receive `304 Not Modified` when the cached copy is still valid. This works naturally with caching proxies between the client and the cluster.


## Common response structure

Most non-streaming API responses that carry a CBOR or JSON body use a uniform envelope:

```
Success (2xx):
{
  "data": <resource or array of resources>
}

Error (4xx, 5xx):
{
  "error": {
    "code": <string>,       -- machine-readable error code
    "message": <string>,    -- human-readable description
    "details": <map>        -- optional, error-specific context
  }
}
```

This envelope does not apply to responses without a body (`204 No Content`, `304 Not Modified`), streaming responses such as `GET /cluster/workload/stream`, or binary download responses such as `GET /cluster/results/:id?download=true`.

### Pagination

List endpoints that may return large result sets support cursor-based pagination:

- Request query parameter: `?cursor=<opaque>&limit=<uint>`
- Response includes a `"nextCursor"` field in the top-level object when more results are available:
  ```
  {
    "data": [...],
    "nextCursor": "<opaque>"
  }
  ```
- `limit` where not explicitly stated, default is 50, maximum 200.

### Optimistic concurrency

The planned optimistic-concurrency mechanism for versioned workload resources
uses an opaque `ETag` and `If-Match`. The formation PoC lock contract below
instead requires an explicit formation precondition and operation identity;
it does not yet implement policy compare-and-swap or ETags and rejects
`If-Match` rather than silently ignoring it. A local policy version is not a
global convergence fence.

**Validator discovery is explicit, never hidden client state.** The validator is always obtained from a prior read (or a write response that returns one); the client carries it back deliberately. There is no implicit session memory of versions.

**Stale write:** if the resource has advanced since the client last read it, the `If-Match` precondition fails and the worker responds with `412 Precondition Failed`. The client should re-read, obtain the new `ETag`, and retry only if the change still applies.

**Missing validator:** if a write omits `If-Match`, the worker does **not** enforce a concurrency check — the write proceeds under last-writer-wins for that resource's normal versioning. Optimistic concurrency is therefore strictly opt-in per request; a client that wants the guard must send the validator. This mechanism protects against stale concurrent writes; it is not an ownership check and does not require the same operator to perform both reads and writes.

Immutable resources (checkpoints, results) also expose an `ETag` (a content-hash validator for caching/`If-None-Match`), but they are never written, so `If-Match` does not apply to them. Operator CLI usage of this opt-in is described in [cluster-admin user stories](./user-stories/orishu/cluster-admin.md#optimistic-concurrency-opt-in).


### Error responses

An error response is a normal, valid server response — not an exceptional transport failure. Any endpoint may return an error envelope at any point during a client session. Clients must treat the error structure defined above as a first-class response type and handle it on every request, not only on requests that "should" fail.

Errors are only returned after the transport-level security handshake has completed. For remote (Tier 2) connections this means mTLS verification must have succeeded before the worker will process and respond to any request, including responding with an error. A client that fails the TLS handshake never receives an application-level error envelope — it receives a TLS alert or connection refusal at the transport layer instead. For local (Tier 1) connections over a Unix domain socket, no TLS handshake occurs, so error responses may be returned immediately.

Concretely, this means:

- After a successful mTLS handshake, a `401 Unauthorized` or `403 Forbidden` response is an application-level decision by the worker (e.g. a valid certificate that lacks the required operator identity), not a transport failure.
- Cluster state can change between any two requests on the same connection. A request that succeeded moments ago may return `409 Conflict`, `404 Not Found`, or `503 Service Unavailable` on the next attempt.
- Streaming endpoints (`GET /cluster/workload/stream`) may emit non-terminal `state` dataframes reporting workload phase transitions while keeping the connection open. They terminate with an `end` frame only when the workload is unloaded or the transport itself is interrupted.

Clients should not assume that a successfully established connection guarantees success of subsequent requests. For enveloped CBOR/JSON responses, clients must inspect the top-level object for `"error"` before accessing `"data"`.

Status-only responses such as `204 No Content` and `304 Not Modified` carry no response body, so they never include a `data` or `error` envelope.


## API endpoints

All paths are relative to the worker's client-facing address. The HTTP _method_ conveys the action per resource-oriented design (see [runtime design](./orishu-runtime-design.md#resource-oriented-api-design)).

The APIs are versioned and typically exposed on the `/api` path. Although, this can be re-configured using network controllers. 

Typical cluster setup, with no network controller, client calls REST API as:
```
curl -X GET 'https://cluster.orishu.local/api/v1/cluster'
```

Further in the document the API root `/api/v1/` is omitted for brevity.

### Join API

This is a special endpoint used to command a node to join or leave a cluster. As such, it is recommended **not** to load-balance this endpoint, as it affects the state of the recipient specifically, not the whole load-balanced group.
These endpoints manage the local worker's membership status and are not routed on the cluster level.

| Method | Path | Tier | Description |
|---|---|---|---|
| `POST` | `/membership/joins` | 2, including local | Identified join submission; processing is not completed admission. |
| `GET` | `/membership/joins/:operationId` | 2, including local | Retained join-operation status. |
| `POST` | `/membership/leaves` | 2, including local | Identified local departure with retained historical receipt. |

The imported `POST`/`DELETE /membership` shapes below are not implemented by
the formation PoC. In particular, there is no unauthenticated local leave exception.

### Admin API

Administrative resources are cluster-scoped, not operator-owned. In the MVP, any authenticated Tier 2 administrator may inspect, reverse, or supersede an earlier administrative change made by another administrator. Actor identity is preserved in audit records, but prior authorship does not grant exclusive modification rights.

#### Cluster resource

| Method | Path | Tier | Description |
|---|---|---|---|
| `GET` | `/cluster` | 1 (local) / 2 (remote) | Cluster summary. |
| `POST` | `/cluster/lock` | 2 | Identified lock/unlock intent; see the formation contract below. Read current lock through `GET /cluster`. |
| `GET` | `/cluster/events` | 1 (local) / 2 (remote) | List recent cluster events. |
| `GET` | `/cluster/logs` | 1 (local) / 2 (remote) | Fetch recent log lines. |
| `GET` | `/cluster/audit-log` | 2 | List administrative audit events. |
| `GET` | `/cluster/token` | 2 | Get current admission-token value and metadata. |
| `POST` | `/cluster/token` | 2 | Rotate join token. Returns new token once. |

#### Node resource

| Method | Path | Tier | Description |
|---|---|---|---|
| `GET` | `/cluster/nodes` | 1 (local) / 2 (remote) | List all nodes. |
| `GET` | `/cluster/nodes/:id` | 1 (local) / 2 (remote) | Detailed info for one node. |
| `DELETE` | `/cluster/nodes/:id` | 2 | Remove a node. `?force=true` for immediate drop. |
| `GET` | `/cluster/nodes/:id/diagnostics` | 1 (local) / 2 (remote) | Direct health check to target node. |
| `GET` | `/cluster/nodes/:id/diagnostics?from=:sourceId` | 2 | Indirect check: ask `sourceId` to probe `:id`. |

#### Membership tombstone resource

| Method | Path | Tier | Description |
|---|---|---|---|
| `GET` | `/cluster/tombstones` | 2 | List membership tombstones for removed-node records and their metadata. |
| `DELETE` | `/cluster/tombstones/:id` | 2 | Clear the membership tombstone for removed node `:id`. Idempotent. Does not admit the node automatically. |

#### Blocklist resource

| Method | Path | Tier | Description |
|---|---|---|---|
| `GET` | `/cluster/blocklist` | 1 (local) / 2 (remote) | List all blocklist entries. |
| `POST` | `/cluster/blocklist` | 2 | Add entry (identity matcher, network matcher, or both). |
| `DELETE` | `/cluster/blocklist/:entryId` | 2 | Remove an entry by ID. Idempotent. |

### Workload API

The workload is the immutable root manifest and its complete digest-addressed
artifact closure; API payload framing and artifact transfer are distribution
formats, not workload identity. The URI/`image` fields in the legacy examples
below describe the current prototype implementation and are not the target
schema accepted by ADR 0010. They will be replaced by identity-only artifact
descriptors and a separate missing-blob/source mechanism in the
[shared workload-format task](./tasks/define-and-adopt-shared-workload-format.md).
Runtime status returned by `GET` is a projection alongside the immutable
manifest and never participates in its canonical digest.

| Method | Path | Tier | Description |
|---|---|---|---|
| `GET` | `/cluster/workload` | 1 (local) / 2 (remote) | Current workload manifest and runtime status. |
| `PUT` | `/cluster/workload` | 2 | Load or replace workload. |
| `PATCH` | `/cluster/workload` | 2 | Update desired run state, reset to checkpoint. |
| `DELETE` | `/cluster/workload` | 2 | Unload workload. `?force=true` requests immediate unload. |
| `GET` | `/cluster/workload/stream` | 1 (local) / 2 (remote) | Resumable live simulation observation stream. |
| `GET` | `/cluster/observations/history` | 1 (local) / 2 (remote) | Finite time-addressed view over persisted observations. |
| `POST` | `/cluster/workload/check` | 1 (local) / 2 (remote) | Dry-run compatibility check against current cluster. |

#### Live simulation streaming

`GET /cluster/workload/stream` implements an observer pattern for a loaded simulation. The stream is available whenever a workload is loaded, regardless of its current phase (`Ready`, `Running`, `Stopped`, or `Error`). A new client receives a complete snapshot. A reconnecting client may present the resume cursor of the newest complete observation it has validated and adopted; the server sends compatible deltas when it retains that baseline or a fresh snapshot otherwise. It then remains subscribed for subsequent `state`, `snapshot`, and `delta` dataframes when the simulation transitions again. The response is a server-sent event stream (or equivalent local/WebSocket transport) structured as:

1. **Snapshot frame** — a full current observation, including the workload phase and an opaque resume cursor. It establishes a baseline on initial connection and whenever a prior baseline is unavailable or incompatible.
2. **Delta dataframes** — incremental state updates that explicitly name their base and target observations. The target supplies the next resume cursor.
3. **State frame** — a non-terminal phase transition notification emitted whenever the workload enters `Ready`, `Running`, `Stopped`, or `Error`. This lets connected observers stay attached across stops, failures, and later restarts.
4. **End-of-stream frame** — sent only when the workload is unloaded, carrying the reason `Unloaded`.

Multiple concurrent observers are supported. Observers are read-only: connecting to the stream does not affect the simulation or the state seen by peers or other observers. Observer queues are bounded, and simulation stepping never waits for a client. When a slow observer exceeds its permitted backlog, the producer may coalesce supersedable presentation updates or establish a new snapshot baseline; it never drops correctness-bearing cluster traffic because of this endpoint.

If no workload is loaded, the endpoint returns `404 Not Found` indicating that no workload is present.


#### Stepped execution

`PATCH /cluster/workload` with `{"runState": "Running", "stepLimit": N}` enables operator-controlled advancement of the simulation by a fixed number of steps. The `stepLimit` acts as a steps budget (or lease): the simulation consumes steps from this budget as they are committed, and automatically stops when the budget reaches zero. This is designed for interactive debugging, validation, and incremental inspection of simulation state. See the [user story](user-stories/orishu/workload.md#step-a-simulation-forward) for the CLI perspective.

When `stepLimit` is present:

- The simulation enters `Running` state and proceeds through normal step coordination (halo exchange, `StepVote` / `StepCommit` barriers, speculative execution — all standard mechanics apply).
- Each committed step decrements the remaining budget by one. After all N steps have been consumed (budget reaches zero), the runtime automatically transitions the simulation to `Stopped` state, as if the operator had issued a graceful stop.
- The `checkpoint` field controls checkpoint frequency during the stepped run. When set to an integer, a checkpoint is written every N steps. When `null` or omitted, no checkpoints are written during the run. A result artifact is not written on stepped execution — the intent is lightweight iteration, not result capture.
- The `stepLimit` field is only accepted when the simulation is in `Ready` or `Stopped` state. If the simulation is already `Running` (started via an open-ended run), the request is rejected — the operator must stop first, then step.
- While the stepped run is in progress, `GET /cluster/workload` reflects the remaining step budget and the original limit. Observers connected via `/cluster/workload/stream` see normal delta dataframes and then a non-terminal `state` dataframes when the budget is exhausted and the simulation returns to `Stopped`.
- If the simulation encounters an error during any step, it transitions to `Error` state immediately. The remaining budget is not consumed.
- Sequential stepped runs accumulate: stepping 3 then stepping 2 produces the same simulation state as stepping 5 from the same starting point, assuming the same cluster topology and deterministic execution.

#### Reset to checkpoint

`PATCH /cluster/workload` with `{"runState": "Stopped", "resetTo": "<checkpoint-id>"}` rewinds the simulation state to a previously recorded checkpoint without starting execution. This is the "rewind" primitive — the simulation remains in `Stopped` state and the operator can inspect, step, or start it afterward. See the [user story](user-stories/orishu/workload.md#reset-simulation-state-to-a-checkpoint) for the CLI perspective.

When `resetTo` is present:

- The simulation must be in `Ready` or `Stopped` state. If the simulation is `Running`, the request is rejected with `409 Conflict` — the operator must stop first.
- `runState` must be `"Stopped"`. Combining `resetTo` with `runState: "Running"` is not permitted in a single request — to reset and then start, issue two sequential PATCH requests. This keeps the reset operation explicit and observable.
- The checkpoint artifact must belong to the same workload manifest (or a declared compatible successor) and must be complete (no missing chunks). If the checkpoint artifact is incomplete, incompatible, or does not exist, the request is rejected with `422 Unprocessable Entity`.
- On success, the simulation's state is replaced with the checkpoint artifact's state. A new workload epoch is created, recording which checkpoint artifact it originated from (visible in `GET /cluster/workload` as `status.checkpointId`).
- The CLI convenience command `orishuctl workload start --resume <checkpoint-id>` is implemented as two sequential API calls: a reset PATCH followed by a start PATCH.

### Checkpoints API

| Method | Path | Tier | Description |
| `GET` | `/cluster/workload/checkpoints` | 1 (local) / 2 (remote) | List checkpoints. Supports filtering by `workloadId`, `workloadName`, time range, resumability, and pagination. |
| `GET` | `/cluster/workload/checkpoints/:id` | 1 (local) / 2 (remote) | Retrieve checkpoint artifact record or payload. |
| `DELETE` | `/cluster/workload/checkpoints/:id` | 2 | Delete a checkpoint. Idempotent. |
| `DELETE` | `/cluster/workload/checkpoints` | 2 | Bulk delete checkpoints. Accepts `?workloadId=`, `?workloadName=`, time filters, and `?resumable=`. |

### Results API

| Method | Path | Tier | Description |
|---|---|---|---|
| `GET` | `/cluster/results` | 1 (local) / 2 (remote) | List result artifacts. Supports filtering by `workloadId`, `workloadName`, time range, and pagination. |
| `GET` | `/cluster/results/:id` | 1 (local) / 2 (remote) | Retrieve result artifact record or payload. |
| `DELETE` | `/cluster/results/:id` | 2 | Delete a result artifact. Idempotent. |
| `DELETE` | `/cluster/results` | 2 | Bulk delete results. Accepts `?workloadId=`, `?workloadName=`, and/or time filters. |


## Message definitions

This section defines the CBOR structure of request and response bodies for each endpoint. Field names match the [runtime data model](./orishu-data-model.md).

### Common types

#### VersionTuple
```
{
  "epoch":   <uint>,
  "counter": <uint>,
  "actorId": <string>
}
```

#### RunIdentity and RunReference

`RunIdentity` identifies one execution independently of the node or route used
to reach it:

```
{
  "formationId":  <string>,
  "workloadId":   <string>,
  "workloadEpoch": <uint64>
}
```

A shareable `RunReference` carries that identity plus optional discovery hints:

```
{
  "apiVersion":      "orishu.run-ref/v1",
  "run":             <RunIdentity>,
  "clusterName":     <string | null>,
  "connectionHints": [<string>, ...]
}
```

`clusterName` is descriptive and `connectionHints` are untrusted, mutable
routes. Neither contributes to run identity. A reference contains no bearer
token, certificate, local path, presentation state, subscription, or playback
cursor. The client authenticates normally, verifies the reached cluster's
`formationId`, and then verifies workload/epoch identity before adopting any
observation.

#### NodeCapabilities
```json-schema
{
  "cpuCores":       <uint>,
  "memoryBytes":    <uint>,
  "architecture":   <string>,
  "storage":        <StorageType>,
  "accelerators":   [<string>, ...],
  "engines":        [<EngineCapability>, ...]   -- runtime engines this node can execute
}
```

`engines` advertises the workload graph profiles, runtime engines and component
lifecycles the node can actually enforce, so the cluster can check compatibility
before a run. The admitted design uses graph profile
`orishu.workload-graph/v1`, engine `wasm-component`, and component lifecycle
`orishu.component/v1`. Another engine requires a new architectural decision.
See [Runtime engine](./protocol-workload.md#runtime-engine).

#### EngineCapability
```json-schema
{
  "engine":                 <string>,
  "componentLifecycles":    [<string>],
  "workloadGraphProfiles":  [<string>]
}
```

#### StorageType
```json-schema
{
  "replicas":   <uint32>,
  "backend":    <string>,   -- "local" | "memory" | "external"
}
```

#### ObjectMeta
Descriptive metadata for identifying and organizing resources. Based on [Kubernetes ObjectMeta](https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/object-meta/).
```
{
  "name":      <string>,          -- object name; uniqueness depends on the resource kind.
  "namespace": <string | null>,   -- object namespace. For cluster resources, the human-readable cluster name, not formation identity
  "uid":       <string | null>,   -- system assigned resource identifier.
  "labels":    <map | null>       -- optional labels - key-value pairs.
}
```

#### NodeManifest
Standard representation used for node resources.

For node resources, `metadata.name` is a human-readable label and is not required to be unique within a cluster. `metadata.uid` carries the cluster-assigned node ID and is the stable unique identifier.

```
{
  "metadata": <ObjectMeta>,
  "spec": {
    "version":           <string>,
    "host":              <string>,
    "clientHost":        <string>,
    "certFingerprint":   <bytes>,   -- SHA-256, raw bytes in CBOR, base64url in JSON
    "accepts": {
      "clients":  <bool>,
      "peers":    <bool>,
      "work":     <bool>
    },
    "limits": {
      "peers":     <uint | null>,
      "clients":   <uint | null>
    },
    "storage": {
      "replicas":  <uint32>
    },
    "capabilities":     <NodeCapabilities>,
    "memberState":      <string>,    -- "Alive" | "Suspected" | "Dead" | "Removed"
    "incarnation":      <uint>,
    "stateVersion":     <VersionTuple>,
    "connectedPeers":   <uint>,      -- runtime-only
    "connectedClients": <uint>,     -- runtime-only
    "workload": {
      "name":  <string>,
      "state": <string>,
      "time":  <uint>
    }
  }
}
```

#### BlocklistIdentityMatcher
```
{
  "type":  <string>,   -- "id" | "name" | "certFingerprint"
  "value": <string>    -- UUID, name string, or hex fingerprint
}
```

#### BlocklistEntry
Standard representation used for blocklist resources.

Omitted matcher dimensions are returned as `null`; at least one of `identity` or `network` is always non-null.

```
{
  "entryId":  <string>,
  "identity": <BlocklistIdentityMatcher | null>,
  "network":  <string | null>,   -- IP or CIDR range
  "addedAt":  <string>,          -- RFC 3339
  "addedBy":  <string>           -- operator identity
}
```

#### TombstoneRecord
Operator-visible representation of a removed node retained in the cluster's membership CRDT. This is a membership tombstone, keyed by the removed node's former cluster-assigned ID.

```
{
  "nodeId":          <string>,          -- tombstoned node ID
  "name":            <string | null>,   -- last known node name
  "certFingerprint": <bytes | null>,    -- last known worker certificate fingerprint
  "removedAt":       <string>,          -- RFC 3339
  "removedBy":       <string>,          -- operator identity or system component
  "removalMode":     <string>,          -- "graceful" | "force" | "dead" | "blocklist"
  "reason":          <string | null>    -- free-form explanation if recorded
}
```


### `POST /membership`

Command the local worker to join an existing cluster. This is the MVP admission path: an administrator supplies explicit introducer address(es), the worker contacts them, performs an mTLS handshake, and presents the mandatory join token in a `JoinReq` via the peer protocol. Discovery-led auto-join is deferred to future proposal work. This endpoint blocks until the join attempt completes or fails.

This endpoint is only available on a standalone worker (one that has not yet joined any cluster). If the worker is already a member of a cluster, the request is rejected with `409 Conflict`. To move a node to a different cluster, first call `DELETE /membership` to leave the current cluster, then `POST /membership` to join the new one.

**Request body:**
```
{
  "addresses": [<string>, ...],   -- one or more introducer addresses (host:port)
  "token":     <string>           -- join token obtained from the target cluster
}
```

`addresses` must contain at least one entry. When multiple addresses are provided, the worker attempts them in parallel (or with short stagger) and uses the first successful connection, consistent with [multi-address resolution](#multi-address-resolution) semantics.

**Response `200 OK`:**
```
{
  "data": {
    "nodeId":         <string>,           -- the node ID assigned by the cluster to a newly joined node.
    "admittedBy":     <string>,           -- the node ID of the introducer that admitted this node to the cluster.
    "cluster":        <ClusterManifest>   -- the cluster manifest.
  }
}
```

**Response `409 Conflict`:** the worker is already a member of a cluster.

**Response `422 Unprocessable Entity`:** the join attempt failed. The response body indicates the reason:
```
{
  "error": {
    "code":    <string>,           -- "token_invalid" | "membership_locked" | "blocklisted" | "tombstoned" | "no_capacity" | "cert_mismatch" | "unreachable"
    "message": <string>,
    "details": {
      "attempted":     [<string>, ...], -- addresses that were attempted
      "matchedNodeId": <string | null>  -- for "tombstoned": the retained node ID whose membership tombstone blocked admission
    }
  }
}
```

- `token_invalid` — the join token was rejected by the introducer.
- `membership_locked` — the target cluster's membership is currently locked.
- `blocklisted` — this worker's identity or network address is on the cluster's blocklist.
- `tombstoned` — this worker matches a retained removed-node membership tombstone in the target cluster. `details.matchedNodeId` identifies the tombstoned node ID to clear via `DELETE /cluster/tombstones/:id` before a later join attempt can succeed.
- `no_capacity` — the introducer has no capacity to accept new peers.
- `cert_mismatch` — mTLS handshake failed due to certificate verification failure.
- `unreachable` — none of the provided addresses could be reached.

---

### Implemented `POST /membership/leaves`

The formation PoC uses this identified resource instead of the imported empty
`DELETE /membership` command below. All requests require the addressed worker's
operator credential, including Unix-socket requests. Join and monitoring
credentials do not authorize departure. No compute drain or artifact transfer
is implemented by this membership-only operation.

The CBOR request has exactly `schemaVersion: 1`, `operationId` (the same bounded
ID type as joins/locks), and `formationId` (the source worker's current formation).
The owner rechecks the formation and queued generation before acceptance. It
refuses `joining`, `joinUnresolved` and shutdown states with `409`; an ambiguous
admission must be resolved rather than hidden by another identity change.
Joined, catching-up and ejected workers can leave. A standalone formation of
one is a recorded no-op; a standalone introducer with other member records
leaves normally. The membership lock does not forbid voluntary departure.

`200` returns a `LeaveReceipt` with `schemaVersion`, `operationId`,
`previousFormationId`, `previousNodeId`, `changed` and `current` (the synthetic
summary at local acceptance). Changed departures create fresh random formation/
node IDs and a fresh join token, retain the certificate and local operator
credential, and reuse the display cluster label. No tombstone is written.
Old work is generation-fenced; a best-effort old-identity departure announcement
does not prove remote receipt. Survivors converge through announcement or SWIM.

The worker retains at most 64 successful leave receipts, including no-ops, for
its process lifetime, across adoption/leave/ejection. It never evicts them into
reexecution. Exact typed request replay returns the historical receipt before
checking current formation; changed input under a retained ID returns `409`.
New requests with stale formation return `412`; a full history returns `503`.
Rejected requests consume no receipt slot. Restart loses this history and
starts a fresh formation, so old source preconditions cannot apply there.
The `current` field in a replay is historical, not a fresh status read.

This route shares the 16-request mutation budget with joins/locks, checks
authorization before reading a body, accepts at most 4 KiB bounded CBOR and
uses a five-second whole-handler deadline. Duplicate credentials return `401`;
invalid bodies/schema return `400`; unsupported encoding/media returns `415`.
`If-Match` is unsupported (`400`). Owner loss/overload returns `503`; timeout
returns `504 OutcomeUnknown`. A client disconnect or timeout does not revoke
an accepted operation: retry the identical body to recover it. Responses use
CBOR, no-store and a current serving-node header where the owner is available.

`orishuctl leave --formation-id SOURCE --operation-id ID` targets the worker
selected by `--host`, requires its operator-token file, and prints the receipt.
Only local departure is acknowledged; poll surviving workers separately.

### Imported `DELETE /membership` (not implemented by the formation PoC)

Command the local worker to gracefully leave its current cluster and return to standalone. The worker drains in-flight computation, transfers result data to replicas, announces `Leave` to peers via the peer protocol, drops its cluster-assigned ID (see [Node identity](orishu-runtime-design.md#node-identity)), and becomes a standalone cluster of one.

This is the inverse of `POST /membership`. No membership tombstone is created in the cluster's membership CRDT — the node can rejoin the same cluster via a normal `POST /membership` without any prior membership-tombstone-clearing step. This distinguishes voluntary leave from operator-initiated removal via `DELETE /cluster/nodes/:id`, which creates a membership tombstone for the node.

If the worker is not currently a member of any cluster (already standalone), the request succeeds as a no-op — consistent with other idempotent `DELETE` endpoints.

**Request body:** empty (no parameters required).

**Response `200 OK`:**
```
{
  "data": {
    "previousCluster":  <string>,         -- human-readable name of the cluster the node just left
    "previousNodeId":   <string>          -- the cluster-assigned ID that was dropped
  }
}
```

**Response `200 OK` (no-op):** worker was already standalone.
```
{
  "data": {
    "previousCluster":  null,
    "previousNodeId":   null
  }
}
```

---

### `GET /cluster`

**Response `200 OK`:** Cluster Manifest
```
{
  "data": {
    "apiVersion": "orishu.dev/v1",
    "kind":       "Cluster",
    "metadata":   <ObjectMeta>,
    "spec": {
      "membershipLocked": <bool>,
      "workload":         <WorkloadManifest | null>
    },
    "status": {                              -- optional
      "version": {
        "epoch":   <uint64>,
        "counter": <uint64>
      },
      "nodeCount":        <uint32>,
      "simulationStatus": <SimulationState | null>
    }
  }
}
```

The response follows the uniform `Manifest<Spec, Status>` structure used across all resources. `spec` describes the desired/configured state; `status` describes the observed runtime state. `status` may be absent if the node has not yet computed cluster-level status.

---

### `GET /cluster/lock`

Not exposed by the formation PoC. Read `membershipLocked` in the current
`GET /cluster` summary; the shared client's `is_lock()` uses that projection.

### `POST /cluster/lock`

Set the desired lock state with a required version-1 `LockRequest` CBOR body:

```json
{"schemaVersion":1,"operationId":"lock-001","formationId":"formation-a","locked":true}
```

Use `locked: false` to unlock. `operationId` is 1–64 ASCII letters, digits,
underscores or hyphens. All fields are required; unknown/duplicate fields and
unsupported versions are rejected. The exact endpoint is the addressed worker:
the shared client does not follow HTTP redirects. Every request authenticates
with that worker's operator credential before its body is read. The body limit
is 4,096 bytes, with the bounded CBOR scanner's structural limits; compression
is not accepted. At most 16 mutation handlers across all client listeners hold
permits, without a permit wait queue. Body reading and owner response share a
five-second deadline; the owner also uses its bounded 16-entry control lane.

The owner checks formation and lifecycle generation when consuming the command.
It accepts policy changes only while standalone or joined. A `200` response
contains `ResponseData::LockReceipt`: `schemaVersion`, `operationId`,
`formationId`, `sourceNodeId`, `locked`, and `policyVersion` (null for a no-op
against the initial unlocked policy). This is historical **local acceptance**,
not current state, a lock owned by this administrator, or global convergence.
Read a fresh summary to inspect current state. Any authenticated operator may
submit a newer intent; the replicated policy's ordering determines convergence.

Retry the exact same request against the same worker. It returns the original
outcome without reapplying the intent, even after a later unlock. Reusing an ID
for different contents returns `409 OperationConflict`. Results, including
domain rejections, are retained for this worker's current formation lifetime,
up to 1,024 distinct operation IDs. When full, new IDs return
`503 OperationHistoryFull`; existing retries still work. There is no time-based
eviction that could turn a delayed retry into a new policy change. History
expires on formation change or restart, when the old formation precondition
also becomes invalid. This is an explicit PoC throughput limit, not a durable
audit log or an unbounded production operation service.

`412 StaleFormation` means no mutation of the replacement formation.
`409 ParticipationUnavailable` or `PolicyRejected` reports a local refusal.
`401 Unauthorized`, `400 InvalidRequest`/`InvalidBody`, `415 UnsupportedMediaType`
and `503 Overloaded`/`OwnerUnavailable` cover admission failures.
`If-Match` is rejected as `400 UnsupportedPrecondition` until policy
compare-and-swap is specified. A timeout or owner failure after submission
returns `504` or `503 OutcomeUnknown`: the command may have committed. A lost
connection does not cancel it. Recover by replaying the same ID and body,
never by assuming failure or automatically inventing another ID. No separate
operation-polling endpoint is required for these synchronous local transitions;
asynchronous join/catch-up uses the identified join resources described above.

### `DELETE /cluster/lock`

Not exposed by the formation PoC. Use the identified POST with `locked: false`;
the imported unconditioned DELETE contract must not bypass retry identity or
the formation precondition.

---

### `GET /cluster/events`

**Query parameters:**
- `after`, `before` — RFC 3339 filtering time-range.
- `type` — comma-separated event types to filter (e.g. `NodeJoined,WorkloadStarted`).
- `cursor`, `limit` — pagination.

**Response `200 OK`:**
```
{
  "data": [
    {
      "timestamp": <string>,     -- RFC 3339
      "eventType": <string>,
      "actor":     <string>,
      "targetId":  <string>,
      "outcome":   <string>,     -- "Success" | "Failure"
      "details":   <map>
    },
    ...
  ],
  "nextCursor": <string | null>
}
```

---

### `GET /cluster/logs`

**Query parameters:**
- `after`, `before` — RFC 3339 filtering time-range.
- `level` — minimum log level: `debug`, `info`, `warn`, `error`.
- `component` — filter by component name.
- `cursor`, `limit` — pagination.

**Response `200 OK`:**
```
{
  "data": [
    {
      "timestamp": <string>,
      "level":     <string>,
      "component": <string>,
      "nodeId":    <string>,
      "message":   <string>
    },
    ...
  ],
  "nextCursor": <string | null>
}
```

---

### `GET /cluster/audit-log`

**Query parameters:**
- `after`, `before` — RFC 3339 filtering time-range.
- `type` — comma-separated audit event types (see below).
- `cursor`, `limit` — pagination.

**Response `200 OK`:**
```
{
  "data": [
    {
      "timestamp": <string>,     -- RFC 3339
      "eventType": <string>,     -- see AuditEventType below
      "actor":     <string>,     -- operator or system component that triggered the event
      "targetId":  <string>,     -- node ID, blocklist entry, or workload ID affected
      "outcome":   <string>,     -- "Success" | "Failure"
      "details":   <map>         -- event-type-specific metadata (string keys and values)
    },
    ...
  ],
  "nextCursor": <string | null>
}
```

The `actor` field is provided for traceability only. It does not imply ownership of the affected cluster resource or exclusive rights to revert a prior action.

**AuditEventType values:** `NodeJoined`, `NodeRemoved`, `NodeDead`, `NodeLeft`, `NodeTombstoneCleared`, `MembershipLocked`, `MembershipUnlocked`, `BlocklistEntryAdded`, `BlocklistEntryRemoved`, `WorkloadLoaded`, `WorkloadStarted`, `WorkloadStopped`, `TokenRotated`, `ResultPurged`, `CheckpointPurged`.

---

### `GET /cluster/token`

Retrieve the current admission token and its metadata. This endpoint is available to Tier 2 administrators even when cluster membership is locked. Reading the token does not bypass the membership lock: new join attempts still fail until the cluster is unlocked. This allows administrators to recover a misplaced token without forcing an unnecessary rotation.

**Response `200 OK`:**
```
{
  "data": {
    "token":     <string>,       -- the current join token value
    "expiresAt": <string>        -- RFC 3339 expiry timestamp
  }
}
```

### `POST /cluster/token`

Rotate the join token. This rotation primitive immediately invalidates the current token and generates a new one. Existing cluster members are unaffected, but any new nodes attempting to join must use the newly generated token.

**Request body:** empty or `{}`.

**Response `201 Created`:**
```
{
  "data": {
    "token":     <string>,       -- the new join token value, shown once
    "expiresAt": <string>,       -- RFC 3339 expiry timestamp
    "rotatedAt": <string>        -- RFC 3339 timestamp of rotation
  }
}
```

---

### `GET /cluster/nodes`

**Query parameters:**
- `memberState` — filter by state: `Alive`, `Suspected`, `Dead`, `Removed`.
- `name` - filter nodes by name.
- `role` — filter by capability role: `introducer` (maps to accepts.peers: true) or `worker` (maps to accepts.work: true).
- `cursor`, `limit` — pagination.

**Response `200 OK`:**
```
{
  "data": [<NodeManifest>, ...],
  "nextCursor": <string | null>
}
```

`memberState=Removed` is a convenience filter that exposes entries with membership tombstones through the node listing view. The canonical operator-facing membership tombstone view is `GET /cluster/tombstones`, which includes removal metadata such as `removedAt`, `removedBy`, `removalMode`, and `reason`.

---

### `GET /cluster/nodes/:id`

**Query parameters:**
- `source` — optional data origin preference: `direct` (query the node directly), `indirect` (query via peers), or `best-effort` (default, use gossip data if direct fails).

**Response `200 OK`:**
```
{
  "data": {
    "manifest": <NodeManifest>,
    "origin":   <string>,          -- "direct" | "gossip"
    "lastSeen": <string | null>    -- RFC 3339 timestamp (only for gossip origin)
  }
}
```

(See [NodeManifest](#nodemanifest) in Common types for full field definitions)

**Response `404 Not Found`:** node ID does not exist in the membership.

---

### `DELETE /cluster/nodes/:id`

Remove a node from the cluster.

In the MVP, any authenticated Tier 2 administrator may do so regardless of which administrator originally admitted or previously managed the node.

**Query parameters:**
- `force` — `true` for immediate drop without drain. Default `false` (graceful).

Successful removal creates or preserves a membership tombstone for the removed node in the cluster membership state. That membership tombstone prevents later re-join until it is explicitly cleared via `DELETE /cluster/tombstones/:id`.

**Response `204 No Content`:** if a node has been removed.
**Response `404 Not Found`:** node ID does not exist.
**Response `409 Conflict`:** membership is locked and removal is not permitted.

---

### `GET /cluster/nodes/:id/diagnostics`

**Query parameters:**
- `from` — optional source node ID for indirect check.

**Response `200 OK`:**
```
{
  "data": {
    "targetId":       <string>,
    "sourceId":       <string | null>,    -- null for direct check
    "reachable":      <bool>,
    "latencyMs":      <float>,
    "memberState":    <string>,
    "peerConnections": <uint>,
    "clientConnections": <uint>,
    "checks": [
      {
        "name":    <string>,              -- e.g. "ping", "gossip_sync", "tls_handshake"
        "passed":  <bool>,
        "detail":  <string>
      },
      ...
    ],
    "checkedAt":      <string>            -- RFC 3339
  }
}
```

---

### `GET /cluster/tombstones`

**Query parameters:**
- `after`, `before` — RFC 3339 filtering time-range on `removedAt`.
- `name` — exact-match filter on last known node name.
- `removedBy` — comma-separated list of operator identities or system actors of interest.
- `removalMode` — comma-separated list of `graceful`, `force`, `dead`, `blocklist`.
- `cursor`, `limit` — pagination.

**Response `200 OK`:**
```
{
  "data": [
    <TombstoneRecord>,
    ...
  ],
  "nextCursor": <string | null>
}
```

This is the canonical operator-facing membership tombstone listing. `GET /cluster/nodes?memberState=Removed` remains available as a convenience shortcut, but it does not replace this resource.

---

### `DELETE /cluster/tombstones/:id`

Clear the membership tombstone for removed node `:id`.

This operation removes the retained removal barrier from cluster membership state, making the node eligible for a later normal `POST /membership` attempt. It does **not** automatically admit the node, does **not** clear matching blocklist entries, and does **not** bypass the cluster membership lock for future join attempts.

Membership tombstone deletion is independent of both blocklist state and the current membership-lock state. If the node is still blocklisted, or the cluster is still locked, this request still succeeds; the later join attempt is what remains blocked.

**Response `204 No Content`:** membership tombstone removed successfully.

**Response `204 No Content`:** membership tombstone did not exist (idempotent, same shape).

---

### `GET /cluster/blocklist`

**Query parameters:**
- `after`, `before` — RFC 3339 filtering time-range.
- `identity.id`, `identity.name`, `identity.certFingerprint` — exact-match identity filters.
- `network.host`, `network.cidr` — exact-match network filters.
- `addedBy` — comma-separated list of operator identity of interest.
- `cursor`, `limit` — pagination.

**Response `200 OK`:**
```
{
  "data": [
    <BlocklistEntry>,
    ...
  ]
}
```

### `POST /cluster/blocklist`

**Request body:**
```
{
  "identity": <BlocklistIdentityMatcher | null>,  -- optional; omit for "any identity"
  "network":  <string | null>,                    -- optional; IP or CIDR range; omit for "any network"
}
```

At least one of `identity` or `network` must be provided. If both are provided, the entry matches only when a joining node matches on both dimensions simultaneously.

**Response body:**
```
{
  "data":  {
    "entry":  <BlocklistEntry>,
    "effect": <string>   -- "blocked" | "blocked_and_removed" | "blocked_deferred"
  }
}
```

`effect` reports the admission-control effect observed while handling this request: whether the ensured-present rule only affects future joins, immediately caused a matching active node to be removed, or matched active nodes that could not be evicted because membership updates are locked.
- `blocked_and_removed` — the entry matched an active node and membership was not locked, so the node was disconnected.
- `blocked_deferred` — the entry matched an active node but membership is locked; the node remains until it leaves for another reason.
- `blocked` — no active node matched; the entry prevents future joins.

`201 Created` means a new entry was created. 
`200 OK` means an entry with identical matchers already existed (idempotent).
 In both cases, `data.entry` follows the shared `BlocklistEntry` shape above, so omitted matchers are returned as `null` here the same way they are in `GET /cluster/blocklist`.


### `DELETE /cluster/blocklist/:entryId`

**Response `204 No Content`:** entry removed successfully.

**Response `204 No Content`:** entry did not exist (idempotent, same shape).

---

### `POST /cluster/workload/check`

Evaluate a workload manifest against the current cluster without loading, distributing, or executing it. This is a read-only, side-effect-free operation. The client supplies the complete workload manifest in the request body. The contacted node validates that manifest and attempts to fetch or inspect any external resources it references, but neither the manifest nor those fetched resources are persisted or propagated to other nodes.

**Request body:** a complete workload manifest object (`<Workload>`).
```
{
  "apiVersion": "orishu.dev/v1",
  "kind":       "Workload",
  "metadata":   <ObjectMeta>,
  "spec":       { ... }
}
```

**Response `200 OK`:**
```
{
  "data": {
    "compatible":    <bool>,             -- true if enough eligible nodes exist to run the workload
    "totalNodes":    <uint>,             -- total nodes in the cluster
    "eligibleNodes": <uint>,             -- nodes that satisfy all requirements
    "manifest": {
      "name":        <string>,
      "contentHash": <string>
    },
    "nodes": [
      {
        "nodeId":   <string>,
        "eligible": <bool>,
        "reasons":  [<string>, ...]      -- empty if eligible; otherwise specific failure reasons
      },
      ...
    ]
  }
}
```

Each entry in `nodes` reports whether that node can run the workload. For ineligible nodes, `reasons` contains one or more human-readable explanations, for example:
- `"missing accelerator: gpu.nvidia — node has none"`
- `"unsupported workload graph profile: requires orishu.workload-graph/v1"`
- `"unsupported component lifecycle: requires orishu.component/v1"`
- `"unsupported integration scheme: package does not expose velocity-verlet"`
- `"unsigned artifact rejected by cluster trust policy"`
- `"insufficient memory: requires 16 GiB, node has 8 GiB"`

**Response `422 Unprocessable Entity`:** the manifest is invalid, or one of its referenced resources could not be fetched or inspected.
```
{
  "error": {
    "code":    <string>,                 -- "referenced_resource_fetch_failed" | "manifest_invalid"
    "message": <string>,
    "details": {
      "resource": <string | null>        -- manifest field or URI/image reference that failed
    }
  }
}
```

- `referenced_resource_fetch_failed` — the manifest parsed successfully, but a referenced artifact, image, or linked resource could not be reached, fetched, or inspected.
- `manifest_invalid` — the manifest body could not be parsed or fails schema validation.

---

### `GET /cluster/workload`

**Response headers:**
- `ETag: "<opaque>"` — current validator for the workload resource.

**Response `200 OK`:**
```
{
  "data": {
    "apiVersion": "orishu.dev/v1",
    "kind":       "Workload",
    "metadata":   <ObjectMeta>,
    "spec": {
      "compute": {
        "workloadGraphProfile": "orishu.workload-graph/v1",
        "components": [{
          "instanceId": <string>,
          "artifact": <ArtifactDescriptor>,
          "pluginId": <string>,
          "modelId": <string>,
          "schemaId": <string>,
          "engine": "wasm-component",
          "lifecycle": "orishu.component/v1",
          "roles": [<string>, ...],
          "stateOwnership": [<string>, ...],
          "limits": <map>
        }, ...],
        "channels": [<StateChannelDescriptor>, ...],
        "stepPlan": <StepPlan>,
        "placementConstraints": [<map>, ...]
      },
      "domain": {
        "dimensions": <uint8>,
        "bounds":     <string> | [<string>, ...],
        "discretization": {
          "space": <string> | [<map>, ...],
          "time": {
            "step": <string>,
            "integration": {
              "scheme":     <string>,
              "parameters": <map>
            } | null
          }
        }
      },
      "inputs": {
        "geometry": <ArtifactDescriptor>,
        "initialConditions": [<ArtifactDescriptor>, ...]
      },
      "requirements": {
        "hardware":          <map>,
        "workloadGraphProfile": <string>,
        "executionProfile":  <map>
      }
    },
    "status": {
      "phase":              <string>,
      "simulationTime":     <float>,
      "convergenceMetrics": <map>,
      "epoch":              <uint64>,
      "checkpointId":       <string | null>,
      "startMode":          <string>,
      "partitionMap":       <map>,
      "stateVersion":       <VersionTuple>
    }
  }
}
```

`spec.domain.discretization.time.integration.scheme` is a workload-defined
identifier selected from the closed set the admitted component graph and step
plan support. `parameters` carries any scheme-specific stepping configuration
the participating components expose. If the graph supports only one fixed integration scheme, the
`integration` field may be omitted or `null`; if the package exposes multiple
schemes, the selected one must be declared explicitly.

#### Expression-bearing workload intent

The workload specification may retain named variables and expression source
under a declared expression-language version. Only numeric fields explicitly
marked expression-capable by the workload schema participate; status,
observations, artifacts, and operational cluster resources are not expression
graphs. Literal quantities such as `1 m` remain the simplest expressions.

Expressions may reference named variables and other expression-capable fields
through canonical schema paths, permitting relationships such as a bound width
defined as half its height. The concrete CBOR/JSON representation of variable
definitions, expression-valued fields, and canonical field paths must be added
to this versioned schema before the feature is implemented; it must not be
inferred from arbitrary strings or object traversal.

`POST /cluster/workload/check` and `PUT /cluster/workload` both use the shared
Orishu Kagami variables subsystem to enforce the same language version,
resource bounds, name and field-path resolution, cycle rules, dimensions,
finite-value rules, and canonicalization. A client-provided preview resolution
is not authoritative. Failure produces `422 Unprocessable Entity` with
structured expression diagnostics and does not load or replace a workload.

On acceptance, Orishu retains the source-bearing manifest and freezes a
canonical resolved-parameter set and fingerprint for the workload epoch. All
participating workers must support the declared language version and agree on
that fingerprint before the workload reaches `Ready`. Workload packages consume
only resolved values; expressions are never reevaluated during stepping.
Changing a variable or expression requires submission of a new workload
resource through the normal replacement flow.

**Response `200 OK` with `phase: "NotLoaded"`:** returned when no workload is loaded. `manifest` is null, `status.phase` is `"NotLoaded"`.

---

### `PUT /cluster/workload`

Load or replace the workload.

**Request body:** a complete workload manifest object (`<Workload>`).
```
{
  "apiVersion": "orishu.dev/v1",
  "kind":       "Workload",
  "metadata":   <ObjectMeta>,
  "spec":       { ... }
}
```

The manifest's temporal-stepping contract is part of what is loaded. In
particular, any selected integration scheme under
`spec.domain.discretization.time.integration` is authoritative workload intent,
not a worker-local preference that may be substituted at load time.

**Response `202 Accepted`:**
```
{
  "data": {
    "workloadId": <string>,
    "epoch":      <uint64>,
    "phase":      "Loading"
  }
}
```

**Response `409 Conflict`:** a workload is already loaded; unload or stop it first.

---

### `PATCH /cluster/workload`

**Request headers:**
- `If-Match: "<opaque>"` — optional optimistic concurrency precondition.

Update the desired run state.

**Request body:**
```
{
  "runState":        <string>,            -- "Running" | "Stopped"
  "transitionMode":  <string | null>,     -- How to apply new state: "Coordinated" (default) | "Immediate"
  "stepLimit":       <uint | null>,       -- steps budget: simulation consumes steps until budget reaches zero, then auto-stops. null = run to completion
  "checkpoint":      <uint | null>,       -- for stepped execution: checkpoint every N steps. null = no checkpoints
  "resetTo":         <string | null>,     -- checkpoint ID to reset simulation state to
}
```

`stepLimit` and `checkpoint` are only valid when `runState` is `"Running"`. The request is rejected with `409 Conflict` if the simulation is already in `Running` state and `stepLimit` is present — stepped execution is only valid from `Ready` or `Stopped`. See [Stepped execution](#stepped-execution) for full semantics.

`resetTo` is only valid when `runState` is `"Stopped"` and the simulation is in `Ready` or `Stopped` state. When provided, the simulation's state is replaced with the specified checkpoint artifact's state and a new workload epoch is created recording which checkpoint artifact it originated from (visible in `GET /cluster/workload` as `status.checkpointId`). The simulation remains in `Stopped` state — to start execution from the reset point, issue a subsequent PATCH with `runState: "Running"`. The checkpoint artifact must belong to the same workload manifest (or a declared compatible successor) and must be complete (no missing chunks). If the checkpoint artifact is incomplete, incompatible, or does not exist, the request is rejected with `422 Unprocessable Entity`. `resetTo` combined with `runState: "Running"` is rejected with `409 Conflict` — reset and start must be separate requests. See the [user story](user-stories/orishu/workload.md#reset-simulation-state-to-a-checkpoint) and [Reset to checkpoint](#reset-to-checkpoint) for full semantics.

**Response headers:**
- `ETag: "<opaque>"` — validator for the updated workload resource.

**Response `200 OK`:**
```
{
  "data": {
    "runState":     <string>,
    "resetTo":      <string | null>,        -- echoed back if resetting to a checkpoint
    "stepLimit":    <uint | null>,          -- echoed back: the initial steps budget
    "checkpoint":   <uint | null>,          -- echoed back: checkpoint frequency in steps
    "stateVersion": <VersionTuple>
  }
}
```

**Response `409 Conflict`:** already in requested run state, `stepLimit` requested while already `Running`, or `resetTo` combined with `runState: "Running"`.
**Response `412 Precondition Failed`:** returned if `If-Match` was provided and the current validator does not match.
**Response `422 Unprocessable Entity`:** `resetTo` checkpoint artifact is missing, incomplete, or incompatible with the active workload.

---

### `DELETE /cluster/workload`

Unload the current workload. By default the request follows the coordinated cluster transition path. `?force=true` requests immediate unload without waiting for coordinated shutdown. No checkpoint is produced by this endpoint.

**Response `204 No Content`:** Workload unload request accepted.
**Response `404 Not Found`:** no workload is loaded.

---

### `GET /cluster/workload/stream`

Live simulation observation via **Server-Sent Events** (SSE). The stream is available whenever a workload is loaded, regardless of its current phase (`Ready`, `Running`, `Stopped`, or `Error`). A client remains subscribed until the workload is unloaded or it disconnects.

**Resume:** the client may supply `?resume=<opaque-cursor>` or an SSE
`Last-Event-ID` header containing the resume cursor of the newest observation it
has completely validated and adopted. The query parameter takes precedence
when both are present. A compatible cursor may resume with deltas; an absent,
expired, unknown, or incompatible cursor causes a full `snapshot`, not an
error. Cursors are opaque, bounded, non-authoritative capabilities to request a
baseline: the server validates their workload, epoch, subscription, schema, and
observation identity and does not trust client-supplied decoded fields.

**Identity guard:** a client opening a shared run reference may also supply
`workloadId` and `workloadEpoch` query parameters. When present, both are
required and the server returns `409 Conflict` unless they identify the
currently loaded run. This prevents a reconnect or persisted-to-live handoff
from silently following a replacement workload.

**Response `200 OK`:** `Content-Type: text/event-stream`

Each SSE event carries a CBOR-encoded payload in the `data` field (base64-encoded when carried over SSE text framing).

Each observation-bearing SSE event also has an `id` equal to its opaque
`resumeCursor`. Transport receipt alone does not mean a client has adopted the
observation: a client persists or presents that cursor only after atomically
validating and applying the complete frame. If it cannot apply a delta, it
discards the partial target and reconnects from its last adopted cursor (or
without a cursor to force a snapshot).

**Event types:**

1. `snapshot` — a full current observation, sent on first connection and
   whenever the requested/current baseline cannot be used:
   ```
   event: snapshot
   id: <opaque-resume-cursor>
   data: <base64(cbor({
     "workloadId":     <string>,
     "workloadEpoch":  <uint64>,
     "schemaVersion":  <string>,
     "subscriptionRevision": <string>,
     "observationId":  <string>,
     "simulationBoundary": <uint64>,
     "phase":          <string>,     -- "Ready" | "Running" | "Stopped" | "Error"
     "simulationTime": <float>,
     "partitionMap":   <map>,
     "state":          <bytes>,      -- serialized simulation state snapshot
     "complete":       true,
     "validity":       <map>,        -- channel/model numerical validity metadata
     "resumeCursor":   <string>,
     "detail":         <string | null>
   }))>
   ```

2. `delta` — incremental update:
   ```
   event: delta
   id: <opaque-resume-cursor>
   data: <base64(cbor({
     "workloadId":     <string>,
     "workloadEpoch":  <uint64>,
     "schemaVersion":  <string>,
     "subscriptionRevision": <string>,
     "baseObservationId": <string>,
     "observationId":  <string>,     -- target observation
     "simulationBoundary": <uint64>, -- target committed boundary
     "simulationTime": <float>,
     "partitions":     <map>,       -- partition ID -> delta payload
     "complete":       true,
     "validity":       <map>,
     "resumeCursor":   <string>
    }))>
   ```

   A client applies the frame only when `baseObservationId` is its current
   complete observation and every workload, epoch, schema, and subscription
   field is compatible. Application is atomic across the subscribed payload;
   failure leaves the previous observation current.

3. `state` — non-terminal workload phase transition:
   ```
   event: state
   data: <base64(cbor({
     "workloadId":     <string>,
     "workloadEpoch":  <uint64>,
     "phase":          <string>,     -- "Ready" | "Running" | "Stopped" | "Error"
     "simulationBoundary": <uint64>,
     "simulationTime": <float>,
     "detail":         <string | null>
   }))>
   ```

4. `end` — workload unloaded:
   ```
   event: end
   data: <base64(cbor({
     "workloadId":    <string>,
     "workloadEpoch": <uint64>,
     "reason": "Unloaded",
     "detail": <string | null>
   }))>
   ```

**Alternative transport:** When the client negotiates a WebSocket upgrade (`Upgrade: websocket`), the stream is delivered as binary WebSocket dataframes containing raw CBOR messages (no base64 wrapping). Each frame is a CBOR map with a `"type"` field (`"snapshot"`, `"delta"`, `"state"`, `"end"`) and the corresponding payload fields. A WebSocket client may additionally acknowledge its newest completely adopted `resumeCursor`; this application acknowledgement permits earlier baseline eviction but does not change simulation execution. Local IPC exposes the same cursor and frame semantics.

The server retains bounded, preferably shared baseline material rather than a
complete state copy per observer. An implementation may use keyframes,
partition chunks, and encoded deltas internally. Its retention and coalescing
policy is invisible on the wire because every unusable baseline recovers with
a complete snapshot. These loss-tolerant rules apply only to observation
projections; command decisions, peer coordination, checkpoints, and artifact
transfers retain their own reliable protocols. See
[ADR 0011](./adr/0011-classify-network-flows-and-baseline-observation-deltas.md).
Implementation of the model and adapters is tracked in
[Implement resumable observation streaming](./tasks/implement-resumable-observation-streaming.md).

**Response `404 Not Found`:** no workload is loaded.

**Response `409 Conflict`:** `workloadId`/`workloadEpoch` expectations do not
identify the currently loaded run.

---

### `GET /cluster/observations/history`

Returns a finite, time-addressed observation stream reconstructed from
immutable result artifacts. It is a derived read view, not a mutable run or
result-sequence resource. Playback clocks and cursors remain client-local.

**Query parameters:**

- `workloadId` — required immutable workload identity.
- `workloadEpoch` — required epoch; prevents attaching to a replacement run.
- `at` — simulation time at which to seek, required unless `cursor` is present.
  The first frame identifies the nearest available committed boundary at or
  after this value.
- `until` — optional inclusive upper simulation-time bound.
- `cursor` — opaque continuation from a prior response. When present it
  replaces `at`; changing workload, epoch, or subscription parameters makes it
  incompatible.
- `limit` — maximum number of complete observation frames, subject to the
  server's enforced bound.
- `channels`, `region`, `levelOfDetail` — optional observation projection. Their
  canonical forms and limits are defined by the workload's observation schema.

**Response `200 OK`:** `Content-Type: application/cbor-seq`

The first item is a complete `snapshot` using the identity, boundary,
completeness, validity, and subscription fields from the live observation
protocol. Following items are exact compatible `delta` or `snapshot` frames in
increasing simulation-boundary order. Historical responses are not coalesced
according to network pressure and never contain presentation-predicted values.

The response carries `X-Orishu-Next-Cursor` when more matching persisted
observations are available. The cursor is opaque and identifies the workload,
epoch, canonical subscription, and last complete returned boundary. It records
a retrieval position, not playback speed or a mutable server session.

Multiple clients may read the same persisted run with unrelated parameters and
cursors. A client implements pause, rate, reverse playback, and seeking locally;
reverse navigation uses cached observations or another time-addressed request,
not reverse-applied scientific deltas.

If stored coverage ends while the same workload epoch is still live, the
response ends normally and identifies the last returned boundary. The client
may then explicitly connect to `/cluster/workload/stream` with matching
workload/epoch expectations. The server never silently crosses into live mode
or a different epoch.

**Failure responses:**

- `400 Bad Request` — missing/invalid bounds or malformed projection.
- `404 Not Found` — no persisted observation exists for the workload and epoch
  at or after the requested time.
- `409 Conflict` — the cursor belongs to another workload, epoch, or canonical
  subscription.
- `422 Unprocessable Entity` — the requested coverage exists but is incomplete,
  corrupt, or currently unavailable. The response identifies the unavailable
  simulation-time range and missing artifact/chunk identities without returning
  a plausible partial observation.

The implementation work is tracked by
[Implement time-addressable run playback](./tasks/implement-time-addressable-run-playback.md).

---

### `GET /cluster/workload/checkpoints`

**Query parameters:**
- `after`, `before` — RFC 3339 filtering time-range.
- `workloadId` — filter by workload ID.
- `workloadName` — filter by workload name.
- `resumable` - filter by resumability. "false" to return only incomplete checkpoints.
- `cursor`, `limit` — pagination.

`workloadName` matches exactly against the stored workload manifest `metadata.name` snapshot captured in the `CheckpointRecord`. Matching is case-sensitive.

**Response `200 OK`:**
```
{
  "data": [
    {
      "id":             <string>,
      "workloadId":     <string>,
      "workloadName":   <string>,
      "workloadEpoch":  <uint64>,
      "originFormationId": <string>,
      "originClusterName": <string>,
      "recordedAt":     <string>,
      "simulationTime": <float>,
      "graceful":       <bool>,
      "resumable":      <bool>        -- true if missingChunks is empty
    },
    ...
  ],
  "nextCursor": <string | null>
}
```

### `GET /cluster/workload/checkpoints/:id`

Retrieves the checkpoint artifact record by default, or downloads the full checkpoint artifact when `?download=true` is present. The downloaded artifact uses the same format as initial conditions, so a downloaded checkpoint artifact can be directly referenced as `spec.inputs.initialConditions` in a new workload manifest.

**Request headers:**
- `If-None-Match: "<opaque>"` — optional; if the ETag matches, the server returns `304 Not Modified` with no body.

**Response headers:**
- `ETag: "<opaque>"` — strong validator derived from the checkpoint's content hash.

**Response `200 OK`:**

For metadata-only requests (`Accept: application/cbor` without `?download=true`):
```
{
  "data": {
    "id":              <string>,
    "workloadId":      <string>,
    "workloadName":    <string>,
    "workloadEpoch":   <uint64>,
    "originFormationId": <string>,
    "originClusterName": <string>,
    "recordedAt":      <string>,
    "simulationTime":  <float>,
    "partitionMap":    <map>,
    "storageBackend":  <string>,
    "chunks":          <map>,
    "missingChunks":   [<string>, ...],
    "graceful":        <bool>,
    "resumable":       <bool>      -- true if missingChunks is empty
  }
}
```

For data download (`?download=true`):
- `Content-Type: application/octet-stream`
- Body: the assembled checkpoint artifact streamed from partition owners.
- Header `X-Missing-Chunks` lists any partition IDs that could not be retrieved, if the checkpoint artifact is incomplete.

### `DELETE /cluster/workload/checkpoints/:id`

Deleting a stored checkpoint creates or preserves the purge tombstone defined
by [ADR 0014](./adr/0014-prevent-purged-artifact-resurrection.md) before stale
inventory can make the artifact visible again. A successful response does not
claim that every offline physical copy has already been erased.

**Response `204 No Content`:** entry removed successfully.

**Response `204 No Content`:** entry did not exist (idempotent, same shape).


### `DELETE /cluster/workload/checkpoints`

Bulk delete. Each matched artifact receives its own idempotent purge intent and
audit event; partial processing must be reported rather than represented as one
atomic cluster-wide erasure.

**Query parameters:**
- `workloadId` — scope to a specific workload.
- `workloadName` — filter by workload name.
- `after`, `before` — RFC 3339 filtering time-range.
- `resumable` — scope to resumable (`true`) or non-resumable (`false`) checkpoints.

`workloadName` matches exactly against the stored workload manifest `metadata.name` snapshot captured in the `CheckpointRecord`. Matching is case-sensitive.

At least one parameter is required.

**Response `200 OK`:**
```
{
  "data": {
    "count": <uint>
  }
}
```

---

> **Stored artifacts, not sequences.** The `/cluster/results` endpoints expose **immutable stored result artifacts** (each a `ResultRecord` with a stable `id` and a content-hash-derived `ETag`). A **result sequence** — the operator-facing "results of this simulation so far" — is *derived/operator-facing terminology*, not a protocol resource in the MVP: it is reconstructed client-side by grouping these immutable artifacts (e.g. by `workloadId` / `workloadEpoch`). There is no mutable result object and no `/cluster/result-sequences` resource; continuing a run adds new immutable artifacts rather than changing existing ones. See [Result sequences versus stored result artifacts](./orishu-runtime-design.md#result-sequences-versus-stored-result-artifacts).

### `GET /cluster/results`

**Query parameters:**
- `workloadId` — filter by workload.
- `workloadName` — filter by workload name.
- `after`, `before` — RFC 3339 filtering time-range (on `recordedAt`).
- `cursor`, `limit` — pagination.

`workloadName` matches exactly against the stored workload manifest `metadata.name` snapshot captured in the `ResultRecord`. Matching is case-sensitive.

**Response `200 OK`:**
```
{
  "data": [
    {
      "id":                  <string>,
      "workloadId":          <string>,
      "workloadName":        <string>,
      "workloadEpoch":       <uint64>,
      "startedAt":           <string>,
      "recordedAt":          <string>,
      "simulationTimeRange": [<float>, <float>],
      "totalSizeBytes":      <uint64>,
      "graceful":            <bool>,
      "complete":            <bool>      -- true if missingChunks is empty
    },
    ...
  ],
  "nextCursor": <string | null>
}
```

### `GET /cluster/results/:id`

Retrieves the result artifact record by default, or downloads the full result artifact when `?download=true` is present. For large result artifacts, the download response is streamed.

**Request headers:**
- `If-None-Match: "<opaque>"` — optional; if the ETag matches, the server returns `304 Not Modified` with no body.

**Response headers:**
- `ETag: "<opaque>"` — strong validator derived from the result's content hash.

**Response `200 OK`:**

For metadata-only requests (`Accept: application/cbor` without `?download=true`):
```
{
  "data": <full ResultRecord as defined in design doc, including persisted workloadName and workloadEpoch provenance fields>
}
```

For data download (`?download=true`):
- `Content-Type: application/octet-stream`
- Body: the assembled result artifact streamed from partition owners.
- Header `X-Missing-Chunks` lists any partition IDs that could not be retrieved, if the result is incomplete.

### `DELETE /cluster/results/:id`

Deleting a stored result creates or preserves the purge tombstone defined by
[ADR 0014](./adr/0014-prevent-purged-artifact-resurrection.md) before stale
inventory can make the artifact visible again. A successful response does not
claim that every offline physical copy has already been erased.

**Response `204 No Content`:** entry removed successfully.

**Response `204 No Content`:** entry did not exist (idempotent, same shape).


### `DELETE /cluster/results`

Bulk delete. Each matched artifact receives its own idempotent purge intent and
audit event; partial processing must be reported rather than represented as one
atomic cluster-wide erasure.

**Query parameters:**
- `workloadId` — scope to a specific workload.
- `workloadName` — filter by workload name.
- `after`, `before` — RFC 3339 filtering time-range.

`workloadName` matches exactly against the stored workload manifest `metadata.name` snapshot captured in the `ResultRecord`. Matching is case-sensitive.

At least one parameter is required.

**Response `200 OK`:**
```
{
  "data": {
    "count": <uint>
  }
}
```


## Error codes

Machine-readable error codes returned in the `error.code` field for responses that carry an error envelope. Status-only responses such as `304 Not Modified` do not use this table because they return no body.

| Code | HTTP Status | Meaning |
|---|---|---|
| `not_found` | 404 | The referenced resource does not exist. |
| `bad_request` | 400 | Malformed request body or invalid parameters. |
| `unauthorized` | 401 | Missing or invalid credentials. |
| `forbidden` | 403 | Valid credentials but insufficient privileges. |
| `conflict` | 409 | Conflicting resource state (e.g. workload already loaded, membership locked). |
| `precondition_failed` | 412 | A precondition for the operation is not met (e.g. `If-Match` validator mismatch). |
| `unprocessable_entity` | 422 | Request is well-formed but semantically invalid (e.g. join token rejected, manifest invalid, checkpoint incompatible). Endpoint-specific `error.code` values provide detail — see `POST /membership`, `POST /cluster/workload/check`, and `PATCH /cluster/workload`. |
| `node_not_accepting_clients` | 421 | The contacted node does not accept client connections. The response may include `"alternatives"` — a list of node addresses that do accept clients. |
| `service_unavailable` | 503 | Node is not ready to serve requests (e.g. still joining the cluster). |


## Connection lifecycle

1. **Discovery:** The client resolves the target — either a direct IP/hostname or a service name that resolves to multiple addresses.
2. **Connect:** The client attempts QUIC connection establishment to one or more resolved addresses. On success, the HTTP/3 session is established with TLS 1.3.
3. **Authentication:** For Tier 2 operations, the client presents credentials (mTLS certificate or `Authorization` header) on each request. For Tier 1 local access over UDS, no credentials are needed.
4. **Requests:** The client sends HTTP requests. Multiple requests may be multiplexed over the same QUIC connection.
5. **Streaming:** For `GET /cluster/workload/stream`, the connection remains open for the duration of the observation. The client may close the stream at any time without affecting the simulation.
6. **Disconnection:** The client closes the QUIC connection when done. No server-side state is tied to a client connection — reconnecting produces a fresh view of the current cluster state.

### Multi-address resolution

When a hostname resolves to multiple addresses (assumed to be part of the same cluster), the client should:
1. Attempt connection to each address in parallel (or with short stagger).
2. Use the first successful connection.
3. If all connections fail, report an error listing the addresses attempted.

This matches the behavior described in the user stories for `orishuctl` commands that accept a hostname parameter.
