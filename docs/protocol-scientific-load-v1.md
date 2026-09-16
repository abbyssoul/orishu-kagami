# Experimental scientific-load HTTP v1

Status: **implemented opt-in whole-upload admission, receipt lookup and current-run descriptor; no public run stepping/control or observations yet**.
Decision: [ADR 0031](adr/0031-expose-bounded-identified-scientific-load-http.md).

## Enablement and authority

`orishu-worker --scientific.enabled true` enables these routes on its existing
client listeners. Equivalent environment: `ORISHU_SCIENTIFIC_ENABLED=true`;
configuration: `spec.scientific.enabled: true`. Explicit CLI overrides environment,
which overrides file settings, including explicit false. Default is disabled.
An unknown scientific configuration key or non-boolean value fails startup;
non-Unix enablement is unsupported by the current private journal backend.

Startup opens the owned/private receipt journal and shared sandbox on a blocking
IO lane, then installs the daemon coordinator before exposing client listeners.
Corrupt/incompatible receipt history prevents enabled startup, never resets it.
All routes below require exactly one `Authorization: Bearer <operator-token>`
header, including Unix-socket reads. TCP still requires explicitly configured TLS;
plaintext TCP and other credential classes are not substitutes. Authentication
precedes body parsing. The protocol adds no remote plugin installer or URL fetcher.

The runtime independently verifies the complete workload and permits execution
only in the already locked standalone single-member profile. Operators must lock
explicitly; submitting a workload does not change membership policy. Scientific
serving is not a claim of distributed worker capability or production hardening.

## Routes and response meanings

| Method/path | Body | Returned data |
| --- | --- | --- |
| `POST /api/v1/run-loads` | Versioned complete upload below | `RunLoadReceipt` |
| `POST /api/v1/run-loads/lookup` | Exact CBOR `LoadRequest`, at most 4096 bytes | Historical `RunLoadReceipt`, or 404 |
| `GET /api/v1/run` | None | `RetainedRun`: usable immutable descriptor or null |

Success envelopes are the existing CBOR `ApiResponse::Ok` with the named
`ResponseData` variant. Responses use `Cache-Control: no-store`. HTTP 202 means
the whole body reached EOF and the durable operation is still Pending; it does
**not** mean closure validation, compilation or publication succeeded. Once final,
a returned receipt uses HTTP 200. Its [typed state](run-load-receipts-v1.md) is the
authority: only Finished/Accepted identifies known initial publication. Refused
and Indeterminate are not accepted runs even though receipt retrieval succeeded.
An exact retry can return a historical final outcome or Pending without reading
its replacement artifact body. Prefer the small lookup route to retransmission.

Lookup requires the complete original request (formation, operation ID and root),
not just an ambiguous operation string. Historical lookup precedes current
formation eligibility and survives process restart. Rebinding its root yields
409/`OperationConflict`. `RetainedRun` is independent of receipt storage health:
it can discover a live execution after receipt IO failure, without inventing a
historical acceptance. Restart restores history, not scientific state: an old
Accepted receipt can coexist with a null current-run descriptor.

## Complete upload framing

Content type must be exactly `application/vnd.orishu.run-load.v1`. The shared
`orishu::model::run_load` exports this value and the metadata cap. Body bytes are:

```text
u32 big-endian request_length
request_length bytes: CBOR LoadRequest
remaining bytes: complete portable workload bundle
HTTP body EOF
```

`request_length` must be positive and at most 4096. The request uses the exact
strict versioned [load-request schema](run-load-receipts-v1.md); the worker's
bounded CBOR preflight rejects duplicate fields and oversized structure before
owned decoding. At least one bundle byte must follow. This framing does not
change the [portable bundle](workload-bundle-v1.md), root digest or kernel ABI.

POST requests require one positive, canonical decimal `Content-Length` describing
the entire HTTP body. Prefix and request bytes are subtracted to obtain the exact
bundle length delivered to the coordinator. Upload framing allows no more than
128 MiB + 4096 + 4 bytes; the existing receiver independently caps the bundle at
128 MiB (and the archive policy may impose tighter limits). No compression header,
Transfer-Encoding/chunked body, trailers, query parameters or If-Match precondition
is supported. Duplicate relevant headers are refused. The small lookup request
also requires Content-Length and exact `application/cbor` with no parameters.
GET permits no nonzero Content-Length and no framed/compressed body; it verifies
actual body EOF within the metadata deadline, including HTTP/2 without a length.

Metadata has an absolute five-second budget, independent of progress. The bundle
receiver has its existing absolute 30-second delivery policy and 60-second total
admission control; these are wall-time safety budgets, not simulation time. Reads
retain one HTTP data frame at a time and the receiver's one bounded upload buffer.
Empty frames yield after sixteen polls; trailers/IO errors cannot masquerade as
successful body completion. Kernel compilation starts only after complete closure
verification and expected-root checking. No disk artifact cache is populated.

After real body EOF, the handler may return Pending immediately while daemon
admission continues. Dropping the request/response after submission cannot cancel
that job or unload a published run. Client disconnection during the body can still
cause bounded delivery refusal. A response wait has a 65-second limit; expiry is
504/`OutcomeUnknown`, never rollback. Native JIT/filesystem operations retain their
resources until actual completion, even when an operation deadline is exceeded.

## Errors, pressure and retry

Eight handlers are shared across all scientific routes/listeners, independently
of membership mutations. Existing connection, HTTP head/stream/frame/window and
write-stall limits still apply. The coordinator has one non-queuing admission/run
slot. Common bounded error envelopes include:

| Status/code | Meaning |
| --- | --- |
| 401 / `Unauthorized` | No accepted worker operator credential; body not parsed |
| 400 / `InvalidHeader`, `DuplicateHeader`, `UnsupportedFraming`, `InvalidLength`, `InvalidRequestLength`, `InvalidBody`, `InvalidRequest`, `UnexpectedBody` | Invalid framing/request, not accepted scientific intent |
| 408 / `MetadataDeadline` | The small frame/request did not complete within budget |
| 411 / `LengthRequired` | No exact length supplied |
| 413 / `BodyLimit` | Announced whole upload exceeds preflight cap |
| 415 / `UnsupportedMediaType` | Missing/incorrect exact media type |
| 404 / `OperationNotFound` | No known receipt for this request |
| 409 / `OperationConflict`, `WorkloadBusy` | Identity rebinding or occupied admission/run slot |
| 503 / `Overloaded`, `OperationHistoryFull`, `ScientificUnavailable`, `OutcomeUnknown` | Capacity/unavailability/durability problem; no invented success |
| 504 / `OutcomeUnknown` | Response wait expired; daemon work may continue |

A busy request with no recorded receipt may retry **the same** logical ID. Once
an ID has a final refused/indeterminate receipt, it cannot be changed or replayed
as fresh execution. A lost response must be reconciled by lookup. Never generate
a new ID automatically as a transport retry. For unavailable/poisoned history,
surface uncertainty and consult the live descriptor/admin recovery path.

## Formation summary compatibility

The existing `/api/v1/cluster` resource returns `schemaVersion: 2` and
`workload: "scientific"` when its single owner view contains published scientific
state. It does not continue to claim `"none"`. No additional summary fields are
introduced; the exact descriptor is read through `/api/v1/run`. Occupancy does
not prove the executor is currently running or usable. Empty formation-only
responses remain version 1/None; the shared client accepts both resource versions,
but rejects v1/Scientific and versions beyond two. Lock/join/leave schemas and peer
protocol versions stay unchanged. V1-only external summary clients need updating
before inspecting an occupied scientific worker.

## Shared Rust client (implemented)

`HttpClusterClient::scientific()` returns a borrowed `ScientificClient` using
the configured worker address/TLS and operator credential. It is distinct from
the imported authored-workload CRUD interface:

```rust,ignore
let receipt = client.scientific().submit(&request, portable_bundle_bytes).await?;
let historical = client.scientific().lookup(&request).await?;
let current = client.scientific().current().await?;
```

Submission consumes a nonempty bundle of at most 128 MiB. It streams a small
prefix and the owned bundle without concatenating another archive-sized copy.
The caller supplies the exact request/root/operation identity; the worker still
independently verifies it. No local plugin inventory, initialization, hidden lock,
automatic polling, redirect or retry participates. A transport/deadline/protocol
error after submission is not rollback; lookup the **same** request at the **same**
worker. Pending/Refused/Indeterminate are preserved as typed facts, not converted
to acceptance or a new attempt. Only 404/`OperationNotFound` maps to absent history;
404 for an unavailable API and other HTTP errors remain errors.

The client requires exact CBOR response media without content encoding. Bodies
are capped at 16 KiB while streaming, including without Content-Length; declared
oversize fails before body consumption. Response-body time is at most five seconds,
with 70-second total submission and ten-second total read budgets (a configured
transport timeout may be shorter). Before serde, the narrow fact-response scanner
bounds depth (12), values (256), map fields (16) and UTF-8 strings (1024 bytes),
rejects duplicates/trailing bytes, and admits only maps/text/unsigned integers/null.
This is response validation, not a new canonical workload codec. The typed envelope
then permits only receipts/descriptors and rejects unknown fields/variants.

Every receipt must match the complete requested identity, and accepted descriptors
must match that embedded request with a nonzero epoch. HTTP 202 is valid only for
Pending submission; lookup/current require 200. Discovery describes the presently
usable run, not necessarily a historical request; compare its immutable identity
explicitly before later controls. Historical source-node identity is not compared
to a restarted worker's new node identity.

Shared transport builders disable automatic retries, redirects and automatic
decompression even if another dependency enables optional reqwest decompressors.
The CBOR middleware now supplies a default media type without overwriting an
explicit versioned upload type. Existing operator CBOR calls retain their format.
These APIs do not yet add Kagami commands/window submission or public run controls.

## Evidence and remaining work

Actual executable tests cover auth-before-body, malformed/oversized framing,
explicit enablement/precedence, no hidden membership lock, HTTP/2 lookup and
TLS HTTP/1+HTTP/2 authentication/lookup. A real portable Newtonian/Euler upload
reaches accepted initial publication, remains retained after the upload response
is discarded, replays without replacement body validation, and reports occupied
summary v2. The complete-upload/retrieval/restart journey now uses the shared Rust
client as well as independent raw-wire checks. Process restart preserves acceptance but not a live run; interrupted
uploads recover Indeterminate. Unit tests exercise frame reuse, EOF signalling,
trailer/error rejection and bounded empty-frame work.

Kagami submission commands/window integration, public step/stop/unload, observations,
thin/resumable transfer, cache administration, distributed/reset allocation and
durable scientific storage remain open. Do not infer those from these routes.
