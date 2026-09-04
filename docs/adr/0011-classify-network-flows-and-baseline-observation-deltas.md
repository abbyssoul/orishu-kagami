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

## References

- [Fabien Sanglard's Quake III networking review](https://fabiensanglard.net/quake3/network.php)
- [id Software's Quake III NetChannel implementation](https://github.com/id-Software/Quake-III-Arena/blob/master/code/qcommon/net_chan.c)
