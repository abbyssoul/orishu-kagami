# Connect Kagami authoring, submission and run observation

Status: **in progress**. Headless export/load/control/read adapters and initial
window attachment/numeric reads/manual controls are implemented. Unix window
captured-revision preparation, new-file export and identified upload/reconciliation
are implemented, together with committed-object 3D position markers and one-shot
numeric field descriptor/channel/point queries with normalized direction glyphs. Durable
client recovery of window submissions and run commands is implemented (slice 3).
Capability negotiation, field/instrument rendering and complete local/proxy parity
remain open. Work package: **K-RUN**.

## Outcome and authorities

A researcher submits one explicitly captured experiment revision and observes its
immutable run through Kagami. Orishu owns scientific advancement. The document
authority alone changes editable intent; observations never enter undo or dirty
the document. UI and later MCP run adapters consume the same shared scientific
client and run projection semantics, not a second solver or transport-specific
document model. Follow ADRs [0004](../adr/0004-separate-authoring-commands-from-run-observations.md),
[0011](../adr/0011-classify-network-flows-and-baseline-observation-deltas.md) and
[0022](../adr/0022-persist-default-view-outside-experiment-intent.md).

## Current evidence and limits

`apps/kagami/src/run.rs` owns one bounded off-window request and a separate retained
projection. The toolbar's Remote run panel can inspect the configured worker,
explicitly attach to the inspected identity/boundary, refresh, step once, terminally
finish and reconcile/resubmit the identical original command. The shared client
checks framing, identities and schemas; the window additionally requires status
and observation SI time to agree. Late cancelled reads, or attachment reads after
editing/replacing the draft or changing modes, cannot attach. Detaching
invalidates pending read adoption without releasing its slot before actual exit.
Possible command submissions keep original intent even after detach.

The path can observe **externally submitted** workloads or the exact run identified
by its own accepted load receipt. Local preparation captures the document incarnation
and revision; accepted-load lineage is displayed only for that exact descriptor.
The current draft may have changed since submission and is never implicitly treated
as current run state. Observation hides the authoring
tree/inspector and offers 3D position markers or a numeric table (first 100 objects),
with exact frame provenance and explicit display-count limits. Markers are fixed
screen-size glyphs, not physical radii or meshes: gold for Dynamics, blue otherwise.
At most 65,536 positions are extracted; capacity omissions and coordinates outside
the SceneScale render range are reported separately, never silently clamped.
Normal camera clipping still applies. Complete validated frame bytes remain
retained under the wire ceiling. Table/geometry preparation and changed-scale
reprojection run off-window; camera motion only changes matrices. Immutable marker
batches reuse GPU instance storage without per-frame full uploads or frame decoding.
This is a manually refreshed snapshot, not a subscription or historical playback.
Return restores the untouched open draft
and sends no worker command. Remote finish is separate and visibly terminal.

Credentials/TLS inputs are process-local launch options. Nothing connects on
startup without a user run action. Submission and command intent is recorded in
the durable client journal before it is sent; see the slice 3 checkpoint below.
No automatic command retries, formation locks, run replacement or provider
substitution are permitted.

## Window field-query checkpoint

The attached window's field inspector requests an exact configured field instance
at its displayed run/boundary, checks status/descriptor time agreement, and offers
only that descriptor's channels. Sampling uses explicit metre-space points and
the shared OSQ1/OSP1 validators. It retains exact query/response packets and caches
flat numeric display values with separate quality/invalidity; invalid samples
never become zeros. The UI explicitly distinguishes complete request coverage
from its 256-cell/16-values-per-cell display limits. Point input is capped at
64 KiB and 4096 points, with at most 16 selected channels and tighter kernel bounds.
Input changes invalidate pending field replies; refresh, commands and detach clear
field query state. There is no silent provider/channel or boundary substitution.
These controls share the existing one-request lane and remain client-local, not
authored instruments or recording. Field-instance enumeration for external runs,
geometry generators, flow-line rendering and scientific MCP routing remain separate
follow-ups.

The numeric report now offers explicit world-X/Y/Z mapping of one exact vector-3
channel to normalized direction glyphs. The user must check declared frame/axes/
conventions before selecting it; there is no automatic frame inference or transform.
A finite positive presentation length in metres controls arrows, not scientific
magnitudes. Invalid and zero vectors are separately counted/omitted; render-range
and precision collapse omissions are explicit. All queried points (up to 4096)
participate, not only the truncated table. Projection uses the same one-job lane
and retained validated packets, without network or guest execution. Form/hide/query
changes invalidate pending geometry. SceneScale changes reproject off-window, once
per requested scale (no retry loop on failure), while the viewport hides old-scale
batches. GPU buffers are reused and arrows share object-marker depth. Near/far
plane crossings and subpixel/view-axis arrows are deliberately omitted visually.
This is one-shot direction visualization, not magnitude mapping, flow lines,
persistent instruments, subscription or completion of the rendering slices below.

## Window compilation/submission checkpoint

`workload_preparation` reuses headless export's exact `compile_snapshot` path.
Prepare reads the current captured experiment (saving first is unnecessary),
freezes only selected installed code, compiles/packs off-window and adopts the
candidate only while document incarnation/revision/mode and inventory revision
still match. No initialize or guest call occurs. A short final inventory guard
covers adoption, not compilation, file export or network delivery. Changing the
draft/name invalidates readiness; view changes do not. Cancellation keeps its slot
until actual completion.

Adoption is the freeze point: the shown root/bytes/revision/inventory become an
immutable portable workload. Explicit Export writes a new file only. Explicit
Submit requires the expected formation ID and worker credentials, retains an
operation ID and frozen bytes, and never locks the formation or substitutes
providers. Subsequent authoring or inventory changes cannot rewrite an upload or
its retry. Pending, absent, Accepted, Refused and Indeterminate are distinct.
Uncertain loads cannot be cleared or replaced; explicit reconciliation or identical
resubmission keeps original bytes and identity. A terminal receipt may be cleared
locally without affecting the worker. Accepted history still requires exact-run
inspection and an explicit Observe action; no implicit attachment or new run.

The preparation lane admits up to 128 MiB captured scientific retention, 64 MiB
selected artifacts (8 MiB selection metadata), 128 MiB aggregate declared closure
and 128 MiB final archive, plus bounded document/projection/codec intermediates.
The one upload lane retains that archive and makes at most one additional 128 MiB
cold IO copy for the shared ownership-taking client. These are explicit local
budgets, not an aggregate RSS guarantee. A submitted bundle blocks new preparation
until its final history is explicitly cleared. Client intent, bytes and source
incarnation are client journal state, never credentials or new workload identity
fields.

## Durable client intent checkpoint (slice 3)

[ADR 0033](../adr/0033-persist-kagami-client-run-intent.md) and the
[run-intent format](../kagami-run-intents-v1.md) record the decision and format.
`run::intents` is the pure ledger and codec; `run::journal` is the Unix storage;
`run::Recovery` connects the window controller to it.

- The controller records a submission (bytes first, then ledger) or a run command
  on its job thread before it sends anything. It records the validated reply
  afterwards. The window mirrors the durable ledger. A reply that cannot be
  recorded keeps the operation unresolved and says so.
- After a restart, the panel restores recorded operations and sends nothing.
  Reconcile and resubmit keep the original identity. A restored upload sends the
  stored bytes only after full closure/root verification.
- An operation recorded for another worker address is shown but refused, with
  the `--host` value that reconciles it.
- One process holds the journal. A second instance, full or read-only storage,
  a damaged or newer record, and a write that did not complete disable submit
  and run commands with a specific reason. Observation stays available.
- `Controller::check` and `check_submit` give one reason per unavailable action.
  The panel shows it in a tooltip, and `act` refuses with the same text.
- The accepted submission keeps its source incarnation/revision across restarts
  until it is cleared, so exact-run lineage is durable for that period.

Evidence: ledger codec/transition tests; journal tests for reopen, a second
instance, damaged/newer/oversized records, refused transitions, cleanup, an
injected failure and a real SIGKILL at each of 15 write barriers, and storage
error classification including a real permission-denied directory; controller
tests for reasons and restoration bound to its worker; the window lost-reply
flow across a restart (no credential or credential path in the record, a second
instance refused, resubmission of stored bytes); and the real-worker window
journey with three restart cases (restored lineage, a command applied before
its reply was recorded, and a command recorded but not sent).

Limits: the journal holds one submission and one command. It is Unix-only.
Credential rotation needs no journal change, because the journal holds no
credential and each request reads the token file; there is no dedicated
rotation test. Window close needs no special treatment, because intent is
already durable. Kagami without writable storage cannot submit or control runs;
a volatile mode for read-only media is a possible future change at
`Journal::open`.

## Remaining bounded slices

1. **Compilation/submission hardening.** The initial exact-revision path above is
   implemented. Extend aggregate effect/IO accounting across scientific preparation,
   document retention, compiled candidates and upload; avoid repeated full upload
   copies with a reviewed retained-buffer transport interface. Improve dependency
   choice UI and unsupported-emitter diagnostics without implicit migration.
2. **Submission lineage and connection capability.** Retain the association between
   submitted document revision and accepted run without making it editable run
   state. The client journal now keeps the source incarnation/revision and exact
   accepted-descriptor association across restarts until the user clears the
   submission. Keep lineage after clearing only through a reviewed history
   design. Check supported client/scientific profiles explicitly; report a
   formation-only worker without pretending connectivity implies execution.
3. **Durable client intent recovery.** Implemented; see the checkpoint above.
4. **Render projection and instruments.** Initial bounded position markers and
   SceneScale omission reporting are implemented. Extend presentation records with
   stable picking/follow identities and physical shapes when schemas supply them.
   Add field queries/vector
   glyphs/flow lines, attachment/camera follow and bounded trajectory handling.
   Request geometry belongs to the observer, and invalid cells never become zeroes.
   Coordinate with [K-OBSERVATION](kagami/compile-and-query-observation-instruments.md)
   and [S-OBSERVE](implement-resumable-observation-streaming.md); do not name the
   current one-shot object projection a stream or a complete universe.
5. **Local/proxy and adapter parity.** K-PREVIEW uses the same sandbox lifecycle and
   validated observation semantics, never native special-case physics. Leaving it
   stops it; leaving remote observation only detaches. Share authority with MCP
   after coordination with its active implementation. UI/MCP reads and controls
   must expose the same numerical meaning, refusal and mode gate.

## Acceptance evidence

- Real captured-document → selected portable closure → accepted worker → computed
  window observation, with no inventory on the worker and no implicit reinitialize.
- UI update-path tests for exact run/epoch/boundary, pending/lost/duplicate command
  replies, explicit original-intent resubmission, stale/foreign/time-inconsistent
  observations, refused mode changes and late reads after detach/replacement.
- Authored revision/history/view remain unchanged during observations; authoring
  commands are unavailable through the shared mode gate. Return restores the draft.
- One pending job, bounded input/output/cache and independent observer capacity.
  No guest/network waits, large hash/parse work or per-sample allocations per frame.
- Real renderer smoke plus manual window verification of identity/mode, terminal
  finish warning, command uncertainty, invalid/missing data and source lineage.
- Update roadmap, user-facing docs and the [X-PLUGIN ledger](x-plugin-delivery-ledger.md)
  from code/tests; do not close this package for the initial attachment slice.

## Non-goals

Worker plugin installation, automatic cluster provisioning/locking, a general
scheduler, multi-writer collaboration, native visual plugins, registry discovery,
implicit computed-state adoption or undo during Observation mode.
