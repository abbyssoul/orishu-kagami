# Implement Kagami viewport workflows

Status: **slices 1–2 implemented**, except that nothing yet *enters*
Observation/replay through the UI — submitting or opening a run is K-RUN's and
K-PREVIEW's. Slices 3–6 remain gated: follow and fields on run observations,
live trails on V-LIVE, exact trails on K-OBSERVATION/V-REPLAY  
Work package: **K-VIEW** ([roadmap](../../roadmap/README.md))  
Decision: [ADR 0022](../../adr/0022-persist-default-view-outside-experiment-intent.md)

## Outcome

Kagami clearly separates Authoring from Observation/replay and supplies the
Field CAD viewport capabilities that make scientific state legible:
perspective/orthographic projection, object follow, field vectors and flow
lines, plus bounded best-effort live trails and exact retained trajectories.

## Slices

1. **Implemented** (except the entry point). Add an explicit workspace-mode
   state machine. Starting/opening a run enters
   Observation/replay; Edit initial conditions returns to Authoring, stopping a
   local preview or detaching from (not stopping) a remote run. Gate UI and MCP
   authoring adapters on that explicit transition without coupling run state
   into the document authority.
2. **Implemented.** Add orthographic projection to `kagami-renderer`, expose
   both projections in
   the view control, and persist projection in K4's separately versioned default
   view together with camera pose/focus. Authoring-view changes advance a view
   revision and dirty the file; Observation/replay view changes are ephemeral.
3. Follow a stable object identity using authored pose in Authoring and
   authoritative observed pose in Observation/replay; clear invalid targets.
4. Render bounded vector-glyph and flow-line layers from valid field
   observations, with explicit stale/undefined behavior and presentation-only
   interpolation.
5. Add a bounded best-effort live trail from received V-LIVE object-position
   observations, visibly breaking across dropped coverage. After V-REPLAY and
   recorded trajectory observations are available, add an exact replay trail;
   invalidate both caches on seek, run, epoch or schema change.
6. Implement Use as initial conditions as an explicit adoption workflow: retain
   the selected observation, validate the document command first, and switch to
   Authoring only on acceptance. Failure stays in Observation/replay; success
   stops a local preview or detaches from a remote run.

## Implementation record for slices 1–2

`kagami_session::workspace` owns the mode machine and the authoring gate;
`kagami_session::default_view` owns the projection, the bounded camera pose,
the separate `ViewRevision` and its dirty/save bookkeeping. The
`kagami.experiment` format advanced to **version 2** to carry ADR 0022's
`defaultView` section. `apps/kagami` holds the mode on its `Document` seam and
routes camera changes by mode. 21 tests in `crates/kagami-session/src/{workspace,
default_view}.rs`, 27 in `crates/kagami-session/tests/document.rs`, 17 in
`tests/store.rs`, 5 renderer camera tests, 2 viewport-boundary tests, and 22
headless app tests in `apps/kagami/tests/authoring.rs`.

Four of those tests exist because review found the corresponding defect; each
is named in the decision it belongs to below.

Seven decisions are worth carrying forward:

- **The gate is on the seam, not in `update`.** ADR 0022 says the mode
  restriction is the application/session workflow's, and K6 says every *rule*
  lives in `kagami-session`. Both hold: `Workspace` is a separate type beside
  `DocumentAuthority`, and `apps/kagami`'s `Document::submit` consults it
  before minting a command identity. Putting it on the one route to the
  authority makes it unbypassable by a call site added later, and the authority
  itself stays innocent of client mode — which ADR 0022 lists as a non-goal.
- **Leaving returns a consequence rather than performing one.**
  `Workspace::edit_initial_conditions` yields `StopPreview` or `Detach`. There
  is no variant that stops a cluster run, so "stopping the cluster run is
  never implied" is enforced by the type rather than by a comment. The shell
  logs it today; K-RUN and K-PREVIEW will execute it.
- **The envelope version moved; the view section's did not have to.** Adding
  `defaultView` under version 1 would have made an older build report a
  perfectly good file as `malformed_document`, which is the exact confusion the
  version field exists to prevent. Version 1 now loads through its own DTO and
  converts *up*, so a re-save writes version 2. The old fixtures were kept as
  `experiment_v1.json` and `experiment_empty_v1.json` — the conversion has real
  bytes to convert, not synthesised ones.
- **The two sections have opposite version policies, deliberately.** The
  envelope refuses a newer version outright. A `defaultView` this build cannot
  read is reported and dropped while the experiment opens normally, because a
  camera must never be why a colleague cannot open an experiment. That is what
  "separately versioned" buys, and it is why the *whole* section — its version
  header included — is held as uninterpreted JSON until `decode` has had its
  say. The first attempt typed the `version` field, which meant a section
  *missing* one failed whole-document deserialization and made the experiment
  unopenable over a presentational field nobody reads; review caught it.
  `no_saved_view_whatsoever_can_stop_an_experiment_from_opening` now covers a
  missing version, a non-numeric one, a newer one, a wrong body, an undeclared
  field, and a section that is not even an object.
- **The version is now checked before the content.** The format's own header
  said so, but `serde_json::from_slice` parsed the whole document first and
  `into_experiment` looked at the version afterwards. `decode_document` reads a
  permissive `{format, formatVersion}` header, checks the identifier and then
  the version, and only then picks a DTO. `store::load` also reports *why* a
  primary was refused (`LoadError::Refused`) instead of falling through to
  "no readable document", which for a newer file would have sent its holder
  looking for corruption that was not there.
- **Recovery is for damage, and only for damage.** `DocumentError::is_damage`
  separates truncated or garbled bytes from a document that is intact but
  declined — a newer format version, or another format entirely. `load` falls
  back to the backup only for damage, and `save` moves anything that is *not*
  damage aside as the backup before replacing it. Conflating the two was a
  data-loss path review found: an unsupported-version primary with a readable
  older backup opened the backup, reported a successful recovery, and the next
  save then wrote over the intact newer document — with the old backup-promotion
  rule declining to keep a copy of it, because it did not decode.
  `a_newer_primary_is_never_recovered_around` and
  `saving_over_a_newer_document_preserves_it_as_the_backup` cover both halves,
  and `damaged_bytes_are_still_dropped_rather_than_preserved` keeps the original
  behaviour for real damage.
- **The camera had to leave the widget to be saveable.** It lived in iced's
  shader `Program::State`, which the app cannot read at save time.
  `SceneProgram` now carries a `Camera` the model supplies each frame and
  *publishes* a `CameraMotion` per gesture; only pointer bookkeeping stays in
  the widget. That also retired the fragility `apps/kagami/src/view/mod.rs`
  documented, where the camera reset whenever the widget's position in the tree
  changed — and it is the drag source K6's slice 4 was waiting for.
- **Bounds and motion belong to the same owner.** The plan put the orbit/pan/
  dolly arithmetic in `kagami-renderer` and the bounds in `kagami-session`,
  which would have been two clamps that could disagree. Both are in
  `CameraPose` instead: `moved` cannot produce a pose the file would then
  clamp. Pan needed no linear-algebra dependency after all — for an orbit
  camera the view basis is a closed form in yaw and pitch, which is also exact
  at the pitch limit where the previous normalised cross product degenerated.
- **A pose has to be *renderable*, not merely finite.** `MAX_CAMERA_TARGET`
  bounds the orbit target because `f32` has no value for 1e40: a perfectly
  finite `f64` target narrowed to infinity, and every matrix derived from it
  was then non-finite. Two separate fixes, because there were two separate
  faults. The bound keeps the eye finite, and `viewport::narrow` makes the
  narrowing itself total, since `Camera` is public and its other callers are
  not bound by the file's rules. Then `Camera::view_matrix` stopped using
  `look_at`: recovering the view direction by subtracting eye from target
  cancels completely in `f32` once the target dwarfs the orbit distance, and
  normalising the resulting zero vector filled the matrix with NaN. The orbit
  camera knows its direction analytically, so `look_to` uses that and the
  orientation stays exact at any coordinate — large coordinates now cost
  translation precision instead of correctness.

- **One modified indication means one question.** The dirty title is the OR of
  the two halves, and the unsaved-changes prompt has to ask about the same
  thing the user is looking at. It originally asked only about the experiment,
  on the reasoning that a camera is not work — which silently discarded a view
  someone had framed and not saved. The authority cannot answer for the view
  (ADR 0022 makes it client-owned), so `Document::may_discard_view` asks the
  shell's half with the same rule the authority uses for its own: nobody's
  unsaved work goes without an explicit decision (ADR 0006). Checked before the
  authority is consulted, so a refusal costs nothing.
  `Document::experiment_is_dirty` remains, but only to make the *separation* a
  checkable property rather than a claim.

### One defect found on the way

`serde_json`'s default float parser is fast rather than bit-exact, so a saved
`f64` could come back one bit away from what was written. The camera round-trip
test caught it; the same defect applied to every authored **position, velocity
and radius** in the experiment section, and nothing had noticed because every
fixture used exactly-representable values like `1.0` and `2.0`. The workspace
now enables serde_json's `float_roundtrip` feature, and
`a_coordinate_survives_the_file_bit_for_bit` is the regression test. This makes
K4's "save then open reproduces an identical experiment" true for arbitrary
coordinates rather than only for round numbers.

### The camera bounds this task shipped were provisional, and
[K14](choose-scene-scale.md) has retired them

`MIN_CAMERA_DISTANCE` (1 m), `MAX_CAMERA_DISTANCE` (2 km) and
`MAX_CAMERA_TARGET` (1e30 m) were absolute constants. The first two were
inherited prototype numbers and the third was chosen only to keep an `f32`
narrowing finite; none was a scientific judgement. Together they made this a
viewer of room-sized scenes: a 2 nm molecule could not be approached and a
planetary orbit could not be framed.

Widening them would only have moved which scale was broken. K14 replaced all
three with `MIN_CAMERA_DISTANCE_UNITS`, `MAX_CAMERA_DISTANCE_UNITS` and
`MAX_CAMERA_TARGET_UNITS` in *render units*, deriving the metre-space limit
from a `SceneScale`. Two consequences reach back into this task's code: the
validated unit is now `AuthoringView` rather than `CameraPose`, since a pose is
only out of range relative to a scale; and `MAX_CAMERA_TARGET_UNITS` is a
precision bound rather than a magnitude one, because what breaks first is
`f32`'s ability to *resolve* the orbit distance against the target, not its
ability to hold the number.

### What is honestly not done

No UI path enters Observation/replay. Submitting a run is K-RUN's and
previewing one is K-PREVIEW's, and neither exists, so nothing is fabricated to
stand in for one — the toolbar keeps saying `Run: unavailable`. The
consequence is that the mode machine, the authoring gate, the mode indicator
and the absence of authoring controls are verified headlessly rather than by
eye. `RunLabel` is a bounded opaque stand-in for S-OBSERVE's run identity.

Slice 3's follow target is not implemented, and the `defaultView` section
carries no field for one. Adding it advances `DEFAULT_VIEW_VERSION` rather than
the envelope version, which is the point of versioning the section separately.

## Acceptance criteria

- Mode, run identity and simulation time are unmistakable; authoring/undo
  controls are absent in Observation/replay and no mode transition adopts run
  state implicitly.
- Perspective/orthographic selection round-trips through the file but never
  changes experiment revision, undo history or workload identity.
- Authoring camera/projection changes dirty the file and save against an exact
  view revision; playback camera changes dirty nothing.
- Camera follow and visualization choices remain per-window and outside MCP
  parity.
- Vector/flow layers never render undefined as zero or feed interpolated values
  back into physics.
- Best-effort live and exact recorded trails are distinguishable; both enforce
  object/sample/time/memory bounds and expose gaps rather than inventing motion.
- Renderer tests plus `make smoke-kagami` are run where graphics hardware is
  available; any manual verification is recorded.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

### Verification recorded for slices 1–2

- `make fmt-check`, `make lint`, `make test`, `make docs` and `make docs-check`
  pass.
- `make smoke-kagami` renders 120 offscreen frames, half under each
  projection, on `Intel(R) Iris(R) Xe Graphics (RPL-P)` / Mesa 25.2.8 /
  Vulkan.
- `cargo run -p kagami -- --exit-after 4` launches and self-exits cleanly,
  which exercises the rebuilt shader-program path.
- **Not yet done by hand:** an interactive pass confirming that orbiting marks
  the title modified with no new undo entry, that switching projection does not
  change the apparent scale, and that a saved view returns on reopen. Each of
  those is asserted headlessly
  (`a_camera_change_dirties_the_file_without_touching_the_experiment`,
  `the_two_projections_frame_the_target_plane_alike`,
  `the_projection_and_camera_survive_save_and_reopen`), but nobody has watched
  it happen in a window.

## Non-goals

- Shared cameras, presenter-follow or synchronized playback.
- Selection/gizmo completion or arbitrary visualization plugin code.
- Scene scale, and therefore atomic- or astronomical-scale viewing:
  [K14](choose-scene-scale.md) owns the metres-per-unit factor and the
  scale-relative camera bounds that have since replaced this task's fixed
  ones.
