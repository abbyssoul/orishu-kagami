# Experimental field-observation HTTP v1

Status: **opt-in worker routes, shared Rust client, Kagami headless commands and
one-shot numeric window reads implemented**. MCP/instrument consumers, subscriptions and historical
retrieval remain open.
This is a read-only transport adapter over the accepted batched sampling
contract, not a new field representation or permission to edit a run.

## Exact descriptor, then query

`POST /api/v1/run/field` accepts `application/cbor` containing:

```json
{
  "apiVersion": "orishu.field-observation-request/v1",
  "run": {
    "formationId": "formation-a",
    "workloadId": "sha256:0101010101010101010101010101010101010101010101010101010101010101",
    "workloadEpoch": 1
  },
  "boundary": 4,
  "field": "newtonian"
}
```

The field is an exact configured kernel instance, not a plugin, family or model
name to resolve. The worker acquires an immutable lease at that exact committed
boundary inside the retained executor. No current-run discovery, initialization,
provider resolution, stepping or implicit boundary refresh occurs.

Success is HTTP 200 with `application/vnd.orishu.field-observation.v1` and a
canonical shared `FieldObservation` descriptor:

- `apiVersion`: `orishu.simulation.field-observation/v1`.
- `snapshot`: committed source (workload, canonical run-descriptor digest, epoch,
  boundary and finite nonnegative SI time) plus state schema/count/length/digest.
- `context`: exact versioned `InstanceContext`, including field instance, kernel,
  contribution/scientific identities, configuration/domain identities, state
  format, precision, typed observable channels and per-instance bounds.

This transfers no field-private bytes. The shared pure codec caps the complete
descriptor at 128 KiB, independently retains the 64-KiB context ceiling, preflights
nesting/counts/text before typed allocation, validates the Field contract and
nonzero committed epoch, and requires canonical encoding. A descriptor is neither
a lease token nor proof of scientific admission. The client correlates its source
and field with the exact request, including formation via run-descriptor digest.

The descriptor does **not** pin the boundary across requests. If the run advances
before sampling acquires its lease, the sample request fails with StaleBoundary.
This is intentional for the initial manual-step workflow. Future continuous
subscriptions need explicit pinned/baseline lifetimes rather than retrying with
different state behind the user's back.

## Batched sampling

The observer builds the existing OSQ1 `SampleRequest` with descriptor-derived
`SampleMetadata`, an observer-scoped request ID, exact requested channel schemas
and observer-generated point IDs/positions. Geometry generation remains outside
the worker/kernel contract. The metadata binds exact snapshot and context digest;
the OSQ1 bytes additionally bind channel order, point order, IDs and positions.

`POST /api/v1/run/samples` requires
`application/vnd.orishu.field-sample-request.v1`. Body layout:

1. Four-byte **big-endian** CBOR metadata length.
2. Exactly that many bytes of the same `FieldObservationRequest` above.
3. One complete OSQ1 packet, with its existing scientific byte order unchanged.

The typed header is at most 4096 bytes; the packet is at most 8 MiB. The whole body
must match its declared length and EOF. Counts, unique point IDs, finite positions,
channel schemas, query/context/snapshot agreement and output layout are checked
before guest invocation. Default caps are 4096 points, 16 channels and 1,048,576
numeric values, tightened by the selected context's point/channel bounds. There
is no implicit channel substitution: exact unsupported channels retain explicit
`ChannelUnavailable` cells in the existing ABI.

The worker invokes sampling in a disposable isolated guest against the retained
immutable state. Neither guest failure nor mutation can affect scientific state.
It never runs sampling inside the scientific commit executor.

Success is HTTP 200 with `application/vnd.orishu.field-sample-response.v1` and the
existing OSP1 bytes, at most 8 MiB. Clients retain the exact OSQ1 request and context
to interpret them. OSP1 binds the complete request digest, compute precision and
complete point/channel layout; shared validation checks every cell's finite SI
values, invalidity and supported quality flags. No per-cell transport objects or
independent interpretation of plugin-private storage is introduced.

## Authority, framing and budgets

Both routes require scientific serving opt-in and the existing worker-local
operator credential, including Unix sockets; TCP uses the existing TLS rules.
Authentication precedes body reads. Exact Content-Type, one canonical decimal
Content-Length, complete EOF and bounded metadata decoding are mandatory.
Unknown versions/fields, duplicate keys, query strings, If-Match, compression,
Transfer-Encoding and trailers are refused. Descriptor requests cap at 4096 bytes.
Request-body reading has an absolute five-second deadline.

Success includes Content-Length, Cache-Control: no-store and the canonical
`Orishu-Observation-Digest` SHA-256 of the complete body. There is no ApiResponse
wrapper on success. Errors use the existing bounded CBOR error envelope:

| HTTP/code | Meaning |
| --- | --- |
| 401 Unauthorized | No valid operator credential; body untouched |
| 400 / 411 / 413 / 415 / 408 | Invalid request/framing/length/size/media or body deadline |
| 400 InvalidSample / SampleLimit | Invalid OSQ1 or response layout exceeds policy |
| 404 RunUnavailable | Exact live retained run unavailable |
| 409 StaleBoundary | Executor no longer owns the requested boundary |
| 409 SampleContextMismatch / SampleSnapshotMismatch | Query does not name the leased context/state |
| 503 ObserverBusy | Shared observer serving pool exhausted |
| 503 FieldUnavailable | Field instance or runtime lease unavailable; not proof of absence |
| 503 SamplingUnavailable | Guest sampling failed, was refused or exceeded execution budget |
| 503 ObservationLimit / ObservationUnavailable / ScientificUnavailable | Descriptor/worker cannot provide the response |
| 504 ObservationDeadline | Acquisition or blocking response wait exceeded its deadline |

Object reads, field descriptors and sampling share **two observer permits across
all listeners**, independent of command admission. Acquisition has a five-second
wait. The off-executor blocking task retains its permit/lease even if the request
disappears. Sampling uses a four-second OperationControl budget; the blocking
response wait is five seconds. Native compilation/execution limitations still
apply: a response timeout is not a claim that all native work immediately stopped.

Response bytes retain the lease/permit through the same owned zero-copy transport
chunks as [object delivery](protocol-object-observation-v1.md). Sampling shares the
runtime output buffer without another full output copy. Metadata and the initial
client upload still allocate bounded cold storage. Transport validation is linear
in request/response bytes and sample cells, plus O(points log points) scratch work
for point-ID uniqueness; descriptor work scales with bounded metadata. Actual
sampling cost belongs to the selected kernel (the Newtonian reference scales with
sample points times sources), not to this transport profile. The ten-second channel send
deadline and connection-level fuses apply. These isolate observer queues/capacity
from commits, not physical CPU or memory-bandwidth contention.

## Shared client and remaining work

`ScientificClient::field` validates the descriptor; `samples` preflights local
intent/query, submits it once and validates the complete response. Both return
None **only** for 404/RunUnavailable. Stale/busy/missing API and sampling failures
remain errors. Ten-second request and five-second response-body deadlines,
strict framing/digest checks and no redirects/retries mirror object reads.

Completed runs remain readable while retained. Restart does not restore fields
from historical receipts. Already acquired immutable leases can complete after
advancement/disposal, but every reply still names its original boundary. There
is no implicit checkpoint, editing, adoption, subscription, recording or field
brush operation. Kagami's [headless field/sample commands](../apps/kagami/README.md#headless-field-inspection-and-point-sampling)
now report exact descriptors and typed point readings, without entering window
observation mode. The attached window now inspects an explicitly named field
instance, selects declared channels and queries semicolon-separated point triples
in metres. It keeps exact packet/context provenance and bounded numeric display,
without initialization, plugin installation or implicit boundary refresh.
The window can also project an explicitly selected vector-3 channel as normalized
direction glyphs from these retained packets, without any new wire operation.
Flow lines, MCP consumers, local/proxy parity, instrument geometry,
partition routing and historical/pinned queries remain in the
[observation task](tasks/kagami/compile-and-query-observation-instruments.md)
and [streaming task](tasks/implement-resumable-observation-streaming.md).
