# Implement time-addressable run playback

Status: **ready**  
Decisions: [ADR 0011](../adr/0011-classify-network-flows-and-baseline-observation-deltas.md),
[ADR 0012](../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md)  
Stories: [Replay persisted observations](../user-stories/orishu/workload.md#replay-persisted-observations-from-a-simulation-time),
[Observe a shared run](../user-stories/kagami/authoring.md#observe-a-run-shared-by-another-user)

## Outcome

Given a stable run reference and observation access, each Kagami client can
seek to persisted simulation time, retrieve an exact ordered observation range,
play it locally at any rate, and explicitly switch to following live head when
the same workload epoch remains active. Several clients can do this concurrently
without sharing a playback cursor or requiring a Kagami relay.

## Current gap

Orishu exposes a resumable live workload stream and immutable result-artifact
metadata/downloads. It does not yet expose a finite, time-addressable
observation view across those artifacts. Kagami therefore cannot efficiently
open a run reference at a requested simulation boundary without downloading
and privately interpreting the whole artifact set.

## Implementation slices

### 1. Define run identity and references

- Define a transport-neutral `RunIdentity` from immutable cluster formation,
  workload identity, and workload epoch. Do not identify a run by display name,
  current endpoint, or whichever workload happens to be loaded.
- Define a shareable `RunReference` that combines that identity with optional
  non-authoritative connection hints. Credentials are never embedded; changing
  a host or route does not change run identity.
- Make opening a reference authenticate normally and verify the reached
  cluster/run identity before returning observations.

### 2. Index exact persisted coverage

- Build a read-only index from workload/epoch and simulation boundaries to
  complete immutable result artifacts and partition chunks.
- Represent covered ranges and gaps explicitly. Overlap, missing chunks,
  corrupt content, schema mismatch, and conflicting observations are errors,
  not opportunities for last-writer-wins selection.
- Keep the index derived and rebuildable. Do not introduce a mutable
  result-sequence object alongside the immutable artifacts.

### 3. Implement the historical observation reader

- Seek to a requested simulation time and materialize a complete observation
  at the nearest available committed boundary at or after it.
- Return subsequent exact observations as compatible deltas or snapshots in
  increasing boundary order, with the identities and validity metadata from
  ADR 0011.
- Apply projection by channels, spatial region, and level of detail without
  changing stored scientific values or simulation execution.
- Use bounded opaque continuation cursors tied to run identity, canonical
  subscription, and last complete returned boundary.

### 4. Expose the client protocol

- Implement `GET /cluster/observations/history` as a bounded finite CBOR
  sequence with the query, cursor, error, and completeness semantics in
  `protocol-client.md`.
- Stream incrementally with bounded memory and verified chunk reads. Never emit
  a partially assembled observation as complete.
- Authorize the endpoint as observation access, separately from submission,
  run control, and artifact deletion.

### 5. Build Kagami playback state

- Keep camera, selection, subscription, timeline cursor, playback rate,
  direction, pause, and follow-head mode in each Kagami client's presentation
  state.
- Buffer a bounded window for smooth forward and reverse navigation; refetch by
  simulation time when a requested boundary is outside it.
- Make transitions between historical reading and live following explicit and
  verify identical workload/epoch identity. Never attach automatically to a
  replacement workload.
- Label interpolated display frames as presentation-only and retain the exact
  source boundaries used to render them.

### 6. Verify independent clients and failure recovery

- Test two or more clients reading different ranges, projections, and rates
  from the same run without shared mutable cursor state.
- Test exact seek boundaries, pagination, gaps, incomplete/corrupt chunks,
  schema mismatch, stale cursors, moved endpoints, unauthorized access, and
  workload-epoch replacement.
- Test explicit persisted-to-live handoff both when identities match and when a
  new epoch must be rejected.
- Prove that historical reads, seeking, and playback speed never affect live
  simulation progress, stored artifacts, or another observer.

## Acceptance criteria

- A shareable run reference cannot accidentally resolve to another cluster,
  workload, or epoch, and contains no credential.
- Seeking returns the exact nearest available committed boundary and reports
  unavailable coverage without fabricating continuity.
- Historical frames carry the same observation identity, provenance, schema,
  dimension/unit, completeness, and validity guarantees as live frames.
- Multiple Kagami clients independently seek, pause, replay, and follow head
  without a Kagami relay or server-owned playback clock.
- Result artifacts remain immutable; the time-addressable view is derived and
  rebuildable.
- All buffers, query ranges, frame counts, cursor sizes, and decoded payloads
  are bounded and tested.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Live multi-writer experiment authoring or a headless document server.
- A mutable server-side playlist, playback session, or result-sequence
  resource.
- Synchronizing cameras or playback positions between Kagami clients.
- Inventing missing scientific frames through interpolation or prediction.
- Routing observation data through the submitting Kagami instance.
