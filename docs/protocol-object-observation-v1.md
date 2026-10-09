# Experimental object-observation HTTP v1

Status: **opt-in worker route, shared Rust client and Kagami headless object reads
implemented**. Window adoption and resumable streaming remain work. Separate
[field descriptor/query routes](protocol-field-observation-v1.md) now share the
same independent observer serving pool.
This delivers the [shared complete-object payload](object-observation-v1.md),
not an editable experiment, checkpoint or whole-field snapshot.

## Request and authority

`POST /api/v1/run/objects` requires the existing worker-local operator bearer
credential, including on Unix sockets. The route exists only with explicitly
enabled scientific serving. TCP retains the existing TLS requirement. It uses
the already admitted workload's retained executor; there is no worker plugin
inventory, initialization, provider choice or implicit simulation step.

The CBOR body is `ObjectObservationRequest`:

```json
{
  "apiVersion": "orishu.object-observation-request/v1",
  "run": {
    "formationId": "formation-a",
    "workloadId": "sha256:0101010101010101010101010101010101010101010101010101010101010101",
    "workloadEpoch": 1
  },
  "boundary": 4
}
```

The exact run must be retained and usable; epoch zero is unavailable, not an
allocation request. The boundary is checked **inside** the executor against
committed state. A stale request is refused; neither server nor client silently
refreshes it. Finish retains observations; unload/replacement/restart can make
the requested run unavailable. An already acquired immutable projection can
finish delivery after later advancement/disposal without changing its identity.

Authenticate before reading the body. Require exactly `application/cbor`, one
canonical decimal Content-Length at most 4096 bytes, complete EOF and the shared
bounded metadata decoder. Metadata has a five-second absolute deadline. Unknown
versions/fields, duplicate keys, trailing/truncated/oversized data, query strings,
If-Match, Content-Encoding, Transfer-Encoding and trailers are refused. This is
a read-only query, not an identified command requiring a durable receipt.

## Reply

HTTP 200 has exactly these scientific framing headers:

- `Content-Type: application/vnd.orishu.object-observation.v1`
- `Content-Length: <complete OOF1 frame bytes>`
- `Orishu-Observation-Digest: sha256:<64 lowercase hex digits>`
- `Cache-Control: no-store`

The body is the entire OOF1 frame, **not** a CBOR ApiResponse wrapper. Its source
names workload root, canonical immutable run-descriptor digest, epoch, boundary
and finite nonnegative SI time. The client checks descriptor digest against the
requested formation/root/epoch, as well as root, epoch and boundary directly.
Time is a reported committed value, not inferred from boundary or wall-clock time.
The authenticated producer remains responsible for actual commitment; a digest
alone cannot prove authority or scientific accuracy.

The response limit is 16 MiB, including framing and metadata. Large projections
are refused, never silently truncated or marked complete after filtering objects.
The complete-object schema's count, force-phase, SI and numeric checks also apply.
Initial force absence is uncomputed; later forces are those evaluated at N-1 and
used for boundary N. Field state and integrator history are not included.

Errors use the existing bounded `application/cbor` ApiResponse error envelope:

| HTTP/code | Meaning |
| --- | --- |
| 401 Unauthorized | Credential missing/invalid; no body was decoded |
| 400 / 411 / 413 / 415 / 408 | Invalid framing/length/size/media or metadata deadline |
| 404 RunUnavailable | The exact live retained run is unavailable |
| 409 StaleBoundary | Requested boundary is not the executor's committed boundary |
| 503 ObserverBusy | Independent observer admission/lease capacity unavailable |
| 503 ObservationLimit | Complete projection could not fit/encode under delivery policy |
| 503 ObservationUnavailable / ScientificUnavailable | Encoding task or scientific owner unavailable |
| 504 ObservationDeadline | Acquisition or encoding response wait expired |

An unknown/missing route is not equivalent to RunUnavailable. A lost observation
reply has no command outcome to reconcile and never authorizes stepping. There is
no implicit retry or current-run discovery.

## Isolation and lifetime bounds

There are **two observer permits shared across all scientific listeners**,
separate from the eight load/control handlers and the executor's scientific
command slot. A request holds one through metadata, acquisition, encoding and
delivery. Acquisition uses the existing separate eight-request executor ingress,
with command priority and shared bounded immutable leases. Acquisition and
encoding each have five-second response waits; encoding runs off-executor in a
blocking task and retains its permit if its awaiting request disappears.

Each encoded allocation owns both its retained scientific lease and observer
permit. Zero-copy response chunks share that owner. Even the last chunk buffered
by HTTP keeps both budgets until it is dropped; finishing the handler/producer
cannot release capacity while its large allocation remains retained. Encoding
copies the numeric projection once; chunking then shares bytes in at most 64-KiB
pieces. A bounded channel supplies backpressure only to the observation producer,
with a ten-second send deadline. Existing connection/head/HTTP2/write-stall limits
remain in force. Failure during delivery can truncate the response, which clients
must reject; it never affects a simulation commit.

These bounds do not reserve independent CPUs, network connections or memory
controllers for observers. They prevent observation buffers/queues from owning
scientific command capacity, not all resource contention on a shared machine.

## Shared client

`ScientificClient::objects(&ObjectObservationRequest)` returns
`Option<ObservedObjects>`. Only exact 404/RunUnavailable yields None. Success
requires exact media/length/digest headers without compression/chunked framing,
complete EOF within the announced bounded length, a matching complete-frame
digest, valid shared numeric payload and exact requested source. The client has
a ten-second request deadline and five-second body deadline, allocates at most
the announced capped bytes, and never redirects or retries.

`ObservedObjects` keeps private immutable validated bytes; `view()` currently
revalidates to return borrowed scientific packets. Retain that view while reading
multiple objects rather than rehashing per object. This initial adapter does not
claim allocation-free delivery or cache self-referential views. No executable
runtime dependency is added to the shared client: it consumes `orishu-plugin`'s
pure schema codec.

Kagami's [headless object command](../apps/kagami/README.md#headless-committed-object-observations)
now reports the validated projection as versioned JSON without command execution
or document mutation. It does not imply window observation mode or streaming.

## Remaining work

This read path is a first delivery adapter, not a subscription, baseline/delta
stream, acknowledgement/resume protocol, historical query or recording service.
Those remain in the [streaming task](tasks/implement-resumable-observation-streaming.md).
Kagami run projection/window adoption and local/proxy parity must consume the same
validated scientific meaning. No GUI integration is implied by this endpoint.
