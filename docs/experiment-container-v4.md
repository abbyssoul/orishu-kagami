# Scientific experiment container v4

Status: implemented pure codec and durable save/open path under
[X-PLUGIN](tasks/define-and-implement-plugin-contract.md). Explicit Unix inspector
field reinitialization now uses guarded background adoption; scientific creation/
configuration and MCP parity, other effects and IO-buffer reservations remain work.
This is not completion of the whole plugin feature.

## Version and ownership

Legacy experiment JSON v1–v3 remains readable, with legacy setup re-saved as v3.
Exact component pins still require v3 or later. An explicit scientific setup uses
a **stored ZIP** whose root `document.json` declares `format: kagami.experiment`
and `formatVersion: 4`. Merely opening an old file never changes its domain,
discretization or boundary interpretation or selects a provider.

`container::{encode, decode}` owns v4. `decode_document` dispatches by framing;
the durable store chooses the writer by the explicit setup variant. Both readers
normalize to the existing in-memory `ExperimentDocument` DTO/version 3, not a
promise that scientific state fits in JSON v3. Bare serde serialization of
scientific setup still refuses. The on-disk version is always checked before its
typed body; no raw JSON v4 body is accepted outside its container.

## Contents

The root preserves authored objects, exact component pins, variables and expression
source, counters, metadata and the separately versioned default view. Scientific
setup contains the shared geometric domain, timestep, exact selected contribution
graph and configured kernel contexts. Each capture records authored configuration,
resolved configuration identity, portable state identity and (for the integrator)
the exact initial Dynamics projection used to initialize history.

`blobs/sha256/<lowercase-hex>` entries carry exact canonical release evidence,
selected declaration payloads, resolved configuration, opaque field/history bytes
and history-source packets. State identities include format/schema, count, byte
length and SHA-256. Repeated blob identities share one entry and one restored
allocation. No paths to sidecars, installed inventory, catalogs or runtime memory
enter this closure. Executables, unrelated contribution payloads and plugin assets
are not included. Release evidence may describe unselected artifacts; those
descriptors are membership evidence, not dependencies to load.

The separate `VerifiedDeclarations` witness verifies membership, contracts,
dependency bindings and context semantics without requiring code availability.
It cannot satisfy APIs requiring a full `VerifiedSelection`. Export and runtime
admission still independently verify all selected executable bytes. File opening
does not install, enable, initialize, validate numerically or execute a kernel.

## Validation and recovery

The archive shares the strict plugin stored-ZIP framing implementation: regular
files only, exact root/blob names, no extraction, compression, ZIP64, encryption,
extra fields or descriptors. Local and central headers must agree, with no
overlap, hidden data, duplicate entries or CRC mismatch. Container verification
then checks exact referenced closure and SHA-256; CRC is not identity.

Default physical container ceiling is 512 MiB; one blob is at most 128 MiB and
root JSON at most 16 MiB, with at most 8192 physical entries. Streaming metadata
preflight bounds one million values/keys, depth 32, text 4096 bytes, object fields
64, collection items 65536 and tighter known scientific collections. An excess
collection entry is refused before it is deserialized. Caller-owned policies can
be tightened. Scientific reference totals are charged before copying opaque bytes;
the authority additionally admits unique buffers plus canonical metadata weight
across current state, undo/redo and command replay (default 512 MiB). Old replay
receipts may be evicted, but undo history is not discarded to make a capture fit.
This does **not** claim a unified process-heap budget including pending effects,
external snapshot holders and save buffers. The cold transaction copy also has
bounded metadata overhead; it does not copy opaque state.

Reconstruction revalidates state-format/count/context metadata and final document
hydration rechecks configuration expressions and initial-object/history coherence.
Captures are restored, never regenerated. Unsupported schemas or missing providers
must not silently change physics. The installed capability projection remains
separate from retained evidence.

Saving uses the existing temporary/verification/flush/backup/rename workflow. The
real reader limits the opened handle even if a file grows after its metadata check;
legacy JSON retains its 64 MiB ceiling. Newer document versions, archive policy
refusals and exhausted limits do not trigger automatic recovery around the primary.
Confirmed archive CRC corruption can use a verified backup; structurally unrecognized
ZIP is conservatively declined rather than presumed damaged. An unsupported save
candidate fails before filesystem mutation.

Tests use real Newtonian/Euler captures, deterministic round trips, offline
hydration, exact code exclusion, missing/extra blobs, forged descriptors, caller
limits, durable save/reopen, scientific backup recovery and newer-primary refusal.
The original plugin framing/interoperability corpus and legacy document fixtures
remain unchanged.
