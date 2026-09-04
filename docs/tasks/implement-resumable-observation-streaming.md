# Implement resumable observation streaming

Status: **ready**  
Decisions: [ADR 0004](../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0011](../adr/0011-classify-network-flows-and-baseline-observation-deltas.md),
[ADR 0012](../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md)

## Outcome

Kagami and other Orishu clients consume one transport-independent observation
stream whose snapshots and deltas have explicit identities and baselines. A
client can reconnect or resume from its newest completely applied observation;
if that baseline is no longer usable, the cluster supplies a new full snapshot.
A slow observer may skip display updates without affecting simulation progress
or scientific results. Concurrent observers own unrelated subscriptions,
baseline cursors, queues, and presentation state.

## Current gap

`SimulationFrame::Snapshot` currently wraps `WorkloadStatus`, while `Delta`
contains only simulation time and partition updates. Neither identifies the
workload epoch, subscription/schema revision, observation, committed boundary,
or delta baseline. The client protocol now specifies the required resume and
fallback semantics, but the shared model, baseline store, and transport adapters
do not implement them.

## Implementation slices

### 1. Define observation identity and compatibility

- Add stable types for observation ID, committed boundary, subscription
  revision, schema version, completeness, and validity.
- Define the exact equality/compatibility checks required before a client can
  apply a delta. Never combine frames from different workload identities,
  epochs, subscription revisions, or schemas.
- Keep simulation time distinct from wall-clock production time and ordering
  sequence numbers.

### 2. Define snapshot, delta, and recovery frames

- Replace the prototype `SimulationFrame` payloads with complete snapshot,
  baseline-naming delta, workload-state, reset/snapshot-required, and terminal
  representations.
- Make every delta name its base and target observation and committed
  simulation boundary.
- Version the payload schema using stable typed fields rather than Rust or C
  memory layout. Preserve dimensions, units, provenance, and validity required
  by each subscribed channel.

### 3. Add application acknowledgement and resume

- Define one transport-independent resume cursor for the newest observation a
  consumer has fully validated and adopted.
- Map that cursor explicitly onto SSE reconnection/local IPC and WebSocket
  operation. Do not equate socket delivery or SSE parsing with application
  adoption.
- Specify idempotent reconnect behaviour and the response when a cursor is
  unknown, expired, belongs to another workload epoch, or names an incompatible
  subscription.

### 4. Retain bounded shared baselines

- Retain a bounded history of shared keyframes, partition chunks, and/or
  encoded deltas; do not store a complete universe copy per observer.
- Define eviction by bytes and age, pinning only what active encoding requires.
- Fall back atomically to a current full snapshot when a baseline cannot be
  served. Never synthesize a delta from partially available state.

### 5. Implement backpressure and presentation policy

- Bound every observer queue by bytes and frames.
- Coalesce or drop only observation projections explicitly marked
  supersedable. Never place halo exchange, step decisions, artifacts, command
  decisions, or checkpoints on this lossy path.
- Keep simulation advancement independent of observer speed and expose dropped
  or reset frame diagnostics.
- Mark client interpolation/extrapolation as presentation-only and prevent it
  from crossing result, checkpoint, simulation-input, or document-authority
  boundaries.

### 6. Verify all adapters

- Exercise the same domain frames over the local, SSE, and WebSocket adapters.
- Exercise two or more simultaneous clients with different subscriptions and
  baselines; reconnecting or falling behind in one must not reset another.
- Test normal delta application, duplicate delivery, reconnect/resume, evicted
  baseline fallback, workload replacement, epoch change, subscription change,
  schema mismatch, out-of-order frames, corrupt chunks, and slow consumers.
- Prove that losing/coalescing presentation observations cannot change committed
  simulation state and that correctness-bearing peer messages remain on their
  reliable paths.

## Acceptance criteria

- A snapshot or delta is uniquely attributable to a workload, epoch,
  subscription/schema revision, observation, and committed boundary.
- A delta that does not name the client's current complete baseline is rejected
  without partially updating its run projection.
- Reconnecting with a retained compatible cursor resumes with valid deltas;
  reconnecting with an unusable cursor returns a full snapshot.
- Observer queues and baseline storage have enforced byte/count bounds, and a
  slow observer cannot backpressure simulation stepping.
- Local preview and remote execution expose identical observation semantics.
- Predicted/interpolated presentation state cannot be serialized as an
  authoritative result, checkpoint, halo, or document update.
- Protocol fixtures and integration tests cover SSE or local streaming plus a
  bidirectional WebSocket path.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Making live observation delivery part of the simulation commit protocol.
- Retaining every live presentation frame indefinitely.
- Replacing immutable result and checkpoint artifact retrieval with an
  ephemeral observation stream.
- Time-addressable historical playback; that is tracked separately in
  [Implement time-addressable run playback](./implement-time-addressable-run-playback.md).
- Implementing client-side scientific prediction or allowing presentation
  interpolation to influence execution.
