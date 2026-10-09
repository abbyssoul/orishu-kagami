# 0011 — Classify network flows and baseline observation deltas

Status: **accepted**  
Date: **2026-09-04**

## Context

Orishu carries traffic with very different correctness requirements. Client
commands propose authoritative transitions; live observations project committed
simulation state; peers exchange membership probes, halos, step decisions,
workload artifacts, and checkpoints. Treating all of these as one generic event
stream would either add unnecessary latency to presentation or permit loss to
corrupt execution.

Real-time game protocols provide a useful distinction. A server can send a
client a full snapshot and then deltas from a baseline known by both sides. A
newer presentation snapshot supersedes an older one, while commands and other
correctness-bearing messages require reliable decisions. Orishu can reuse that
semantic split, but not the assumption that every state update is disposable:
simulation coordination and durable artifacts determine scientific results.

The client protocol already sketches an initial snapshot followed by deltas,
but a delta has no explicit baseline or observation identity. It therefore
cannot be validated independently, resumed safely, or recovered when the
producer has evicted the state from which it was encoded.

## Decision

Every network flow is classified by its domain semantics before selecting a
transport or recovery policy:

- **Commands and decisions** are reliably delivered, bounded, authenticated,
  and idempotent where retry is permitted. Successful transport never implies
  that an authority accepted a command.
- **Correctness-bearing simulation data**—including halo data, step votes and
  commits, workload closure artifacts, checkpoints, and authoritative command
  decisions—uses reliable transfer with explicit identity, integrity, and
  protocol-level validation. A newer message does not silently repair an
  omitted prerequisite.
- **Ephemeral coordination hints**, such as SWIM probes, may use unreliable
  datagrams only when loss has an explicit higher-level consequence such as
  retry or suspicion.
- **Observation projections** may use latest-state-wins behaviour. A producer
  may coalesce or skip intermediate live observations under backpressure when
  the subscription permits it, because doing so never changes simulation
  execution. It must recover with a new complete snapshot rather than apply a
  delta to an unknown state.

An observation stream begins with a complete snapshot or resumes from an
explicitly named observation baseline. Every snapshot and delta carries enough
identity to prevent cross-run composition: workload identity, workload epoch,
subscription/schema revision, observation identity, committed simulation
boundary, and validity/completeness metadata. A delta additionally names its
base observation and target observation.

The consumer acknowledges or presents a resume cursor for the newest complete
observation it has successfully adopted. The producer keeps bounded reusable
baseline history. If the requested baseline is unknown, expired, incompatible,
or incomplete, the producer sends a fresh full snapshot. Acknowledgement here
describes application state, not transport packet receipt, and remains useful
over reliable QUIC or HTTP streams.

Baseline storage is shared where possible. Orishu does not copy a fixed ring of
complete universe states for every observer; it may retain shared keyframes,
content-addressed partition chunks, and encoded deltas, then project them
through each subscription.

Kagami may interpolate or extrapolate observations for smooth presentation,
but marks the result as presentation-only. Predicted state cannot become a
checkpoint, halo value, result artifact, or authored experiment state. Adoption
into an experiment remains an explicit authoring command under ADR 0004.

Transport mechanisms do not define these semantics. QUIC streams supply
reliable bytes and flow control, while datagrams permit intentional loss; the
application still owns authority, identity, idempotency, acknowledgement,
backpressure, and recovery.

## Consequences

- The client API can expose the same observation meaning over SSE, WebSocket,
  local IPC, and future transports.
- Slow observers do not stall or influence the simulation. They may miss
  presentation frames and recover from a current snapshot.
- A delta is never applied merely because it arrived in order; its baseline,
  workload, epoch, schema, subscription, and boundary must match local state.
- Reconnection and replay have an explicit recovery path rather than relying
  on hidden connection state.
- Live observation streams and immutable historical results may share schemas
  and codecs, but historical retrieval does not inherit live-stream loss or
  coalescing semantics.
- Concurrent observers own independent subscriptions, baselines, playback
  cursors, and presentation state. Sharing a run reference does not make one
  observer's timeline authoritative for another.
- A future collaborative authoring service announces the accepted run
  reference. It does not normally relay observation bytes or synchronize
  playback; each Kagami client subscribes directly to Orishu unless a separate
  gateway is required.
- The observation model and client protocol require implementation work for
  identities, resume cursors, acknowledgement, bounded baseline retention,
  backpressure, and snapshot fallback.

Implementation is tracked in
[Implement resumable observation streaming](../tasks/implement-resumable-observation-streaming.md).

## Full-object projection refinement — 2026-09-17

The first shared scientific observation payload is a bounded, content-identified
[complete numeric-object projection](../object-observation-v1.md), produced from
an immutable runtime lease. It reuses shared canonical source metadata and existing
portable object/force packets. It is not the stream envelope or a whole-world
snapshot: fields are absent, forces explicitly belong to the preceding evaluation
boundary, and subscription/baseline/resume identity remains the streaming task.

Raw numeric packets alone were rejected because they cannot bind themselves to a
run/boundary. Per-object JSON was rejected here because it expands the hot numeric
payload and duplicates the existing portable scientific schema. Embedding opaque
whole-run checkpoints was rejected because observation neither requires private
integrator/field state nor grants restart/adoption authority. The selected frame
keeps those distinctions while supporting the same validation for local and
remote producers. Its format does not replace or relax the stream requirements
above. Dynamic membership must revisit the fixed-membership force-coverage rule.

## Initial delivery refinement — 2026-09-17

An [exact-boundary authenticated POST read](../protocol-object-observation-v1.md)
now delivers the complete-object payload. It is deliberately not an alias for
the prototype SimulationFrame or a claim that subscriptions/resume are implemented.
The bounded typed request identifies formation/root/epoch and boundary; the client
correlates these against the payload and canonical descriptor digest. No automatic
current-run selection or refresh is permitted. Existing command receipt authority
and wire formats remain unchanged.

Observer serving uses its own two-permit lane across listeners. Reusing the
scientific handler pool was rejected because stalled observers could consume
command admission. Releasing permits on handler return was rejected because
queued transport chunks could retain large allocations after admission capacity
had returned. Each response allocation instead owns its snapshot lease and permit;
all zero-copy chunks retain them until disposal. Encoding runs off-executor.

The 16-MiB complete-response ceiling is an initial policy, not permission for partial
object coverage. Larger runs need future chunk/stream work. Field queries, client
adoption, baseline recovery and local/proxy parity remain explicit follow-ups.

## Initial field query refinement — 2026-09-17

[Field observation HTTP v1](../protocol-field-observation-v1.md) delivers a bounded
exact-source descriptor followed by the existing OSQ1/OSP1 sampling contract.
Opaque state remains with the runtime; the observer receives context, typed
channel vocabulary and snapshot identity, then supplies the point geometry.
Sampling executes in a disposable guest off the scientific executor and shares
the independent observer lane and allocation-owned delivery budgets.

Requiring clients to already know the private state's digest was rejected: an
exported initial workload cannot identify later computed state. Sending private
field buffers for Kagami to interpret was rejected because the selected kernel
owns that interpretation. A single semantic query with server-invented query
metadata could avoid the descriptor/query race, but would introduce a second
sampling envelope and change who captures exact query intent. The initial profile
instead reuses the accepted scientific packets and explicitly refuses stale
boundaries. A descriptor is not a persistent server lease. Continuous playback
and subscriptions must later add explicit retention/baseline semantics; they
must not silently refresh the user's query to another state.

No workload, guest ABI, command journal or scientific pipeline version changes.
The descriptor and read-intent schemas are additive and independently versioned.
This implements one-shot remote sampling, not instrument geometry, historical
queries, distributed partition routing or Kagami UI/MCP adoption.

## References

- [Fabien Sanglard's Quake III networking review](https://fabiensanglard.net/quake3/network.php)
- [id Software's Quake III NetChannel implementation](https://github.com/id-Software/Quake-III-Arena/blob/master/code/qcommon/net_chan.c)
