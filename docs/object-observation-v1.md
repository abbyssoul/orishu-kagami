# Committed object observation — version 1

Status: **shared codec, retained-runtime producer and bounded HTTP read delivery
implemented; Kagami headless inspection implemented, window adoption pending**.
This is a full numeric-object projection, not a full
experiment/field snapshot, resumable stream, checkpoint or editable document.
It refines [ADR 0011](adr/0011-classify-network-flows-and-baseline-observation-deltas.md)
and uses the existing [scientific bulk packets](scientific-bulk-io.md).

## Authority and meaning

`orishu_plugin::execution::{ObjectObservation, encode_object_observation}` is the
transport-independent pure contract, beside the existing sampling and numeric
schemas. It adds no IO, runtime or client dependency. `ObjectSnapshot::encode` in
the shared runtime produces it from a retained committed lease. Encoding must
run off the scientific executor and retain the lease; observer byte/count and
delivery budgets remain the adapter's responsibility.

The projection is complete for **all numeric objects** in one fixed-profile run:
run-local ID, intrinsic position/velocity and optional inertial mass. Static and
uncoupled objects are included. Other authored components remain in the immutable
workload. Fields, integrator history, emitter membership transitions, trails and
presentation predictions are not part of this format. A consumer must never
interpret omitted field state as empty, zero, or a complete universe snapshot.

At boundary zero, forces are absent: uncomputed, not zero. At boundary N > 0,
the force packet contains exactly one reduced force for every dynamic object and
none for static objects. These forces were evaluated at N-1 kinematics and used
to produce N; they are **not** newly evaluated forces at the displayed N positions.
An empty dynamic set after boundary zero still has an explicit empty force packet.
This coverage rule relies on the current fixed-membership execution profile;
birth/death integration must explicitly revisit/version it, not silently reuse
the format with different membership semantics.

Numeric values use the bulk schemas' finite IEEE-754 binary64 interchange in SI:
position metres, velocity metres/second, inertial mass kilograms and force newtons,
in the admitted domain's Cartesian frame. Binary64 interchange does not imply
binary64 kernel arithmetic. Exact computational precision, selected model/kernel
and execution provenance are resolved through the identified immutable workload
and run descriptor, not guessed from values or plugin names. All records are
structurally valid; this profile has no partial/invalid-cell substitute. Decoding
does not prove scientific accuracy or authenticate commitment.

## Exact bytes

The 16-byte header is followed by canonical metadata, an object packet and an
optional force packet, with no padding, compression or trailing bytes. Header
integers are unsigned little-endian u32, never native memory layout.

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `OOF1` |
| 4 | 4 | Metadata byte length |
| 8 | 4 | Complete `orishu.simulation.objects/v1` packet byte length |
| 12 | 4 | Complete `orishu.forces/v1` packet byte length; zero only at boundary zero |

Metadata is the existing shared deterministic-CBOR profile, explicitly projected:

```text
{
  apiVersion: "orishu.simulation.object-observation/v1",
  source: {
    kind: "committed",
    workload: <canonical workload digest>,
    run: <digest of immutable run descriptor>,
    epoch: <nonzero u64>,
    boundary: <u64>,
    timeSeconds: <finite nonnegative binary64 SI time>
  }
}
```

`source` reuses `SnapshotSource` from sampling. Authored sources are not accepted
for this committed-run format. The descriptor digest binds the formation and
admission provenance; it is not a caller-generated alternate run ID. The format
does not independently verify a descriptor or infer simulation time from boundary.
Consumers correlate **every source field** against the authenticated request/run
context before adopting or composing values; `check_source` provides exact equality.

The observation ID is SHA-256 of the **entire frame**. The encoder returns it;
the reader requires an expected digest and verifies it before metadata decoding
or exposing records. Future transports carry that ID with their identified
envelope. A hash detects byte mismatch, not a malicious producer who can rewrite
both bytes and digest. This projection ID is not a subscription revision, stream
sequence, acknowledgement, resume cursor or permission to apply a delta.

## Bounds and allocation

`ObjectObservationLimits` defaults to 128 MiB **aggregate frame bytes** and one
million objects. This is separate from the retained lease budget; framing may
exceed a delivery limit even when raw leased packets fit. Both packet counts are
bounded before numeric scans. Length additions and u32 encoding are checked;
input must match the announced aggregate length exactly.

Metadata is at most 2048 bytes, four nesting levels, 256 canonical values,
128-byte text and eight fields per map. It must match its exact canonical
projection: duplicate/unknown keys, unsupported versions and noncanonical CBOR
are refused. The shared canonical reader also compares text lengths to its
remaining structural budget, so that budget accommodates full digest spellings.

Readers hash and validate all data before exposing a borrowed projection.
Numeric packets are not copied or expanded into per-object allocations. Force
coverage is a linear merge of sorted identities with constant auxiliary space.
Encoding validates first, then reserves reusable caller output once; rejection
leaves existing output bytes unchanged. Cold metadata uses bounded allocations.
Both paths are O(frame bytes + objects), not allocation-free or zero-copy wire IO.

## Delivery remains separate

The [bounded object-read HTTP adapter](protocol-object-observation-v1.md) now
delivers this codec with authenticated exact intent and separate observer budgets.
No stream frame, baseline store or GUI adoption is supplied by that read path.
Neither observer pressure nor encoding/serialization may enter the
scientific command slot or delay commit. The retained worker already has a
separate bounded acquisition ingress and exact-boundary checks.

The [streaming task](tasks/implement-resumable-observation-streaming.md) still
owns subscription identity, compatible full/delta envelopes, independent queues,
acknowledgement/resume, fallback and transport adapters. Field sampling continues
to use its typed channel/query contract over separate immutable field leases.
